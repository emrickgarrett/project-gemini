//! Native listen-server link. Wasm stays solo; TCP is not available in the browser build.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use gemini_proto::{
    FrameBuf, InputPacket, JoinRequest, Packet, ReconnectRequest, SnapshotPacket, Welcome, PROTOCOL_VERSION,
};
use gemini_sim::content::Content;
use gemini_sim::sim::{PlayerInput, Sim};

use crate::session::{CoopLink, Game, Launch};

pub fn launch_from_env() -> Launch {
    let args: Vec<String> = std::env::args().collect();
    let value_after = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    if let Some(addr) = value_after("--host") {
        Launch::Host(addr)
    } else if args.iter().any(|arg| arg == "--host") {
        Launch::Host("127.0.0.1:17650".into())
    } else if let Some(addr) = value_after("--join") {
        Launch::Join {
            addr,
            token: value_after("--token").and_then(|text| text.parse().ok()),
        }
    } else if args.iter().any(|arg| arg == "--join") {
        Launch::Join {
            addr: "127.0.0.1:17650".into(),
            token: value_after("--token").and_then(|text| text.parse().ok()),
        }
    } else {
        Launch::Solo
    }
}

pub fn prepare(launch: &Launch, content: &Content, sim: Sim) -> (Sim, u64, CoopLink) {
    match launch {
        Launch::Solo => {
            let token = sim.players.first().map(|player| player.token).unwrap_or(1);
            (sim, token, CoopLink::Solo)
        }
        Launch::Host(addr) => {
            let listener = TcpListener::bind(addr).unwrap_or_else(|err| panic!("host bind {addr}: {err}"));
            listener.set_nonblocking(true).expect("nonblocking listen");
            eprintln!("gemini host listening on {addr}");
            let token = sim.players.first().map(|player| player.token).unwrap_or(1);
            (
                sim,
                token,
                CoopLink::Host(Host {
                    listener,
                    peers: Vec::new(),
                }),
            )
        }
        Launch::Join { addr, token } => {
            let mut stream = TcpStream::connect(addr).unwrap_or_else(|err| panic!("join {addr}: {err}"));
            stream.set_nodelay(true).ok();
            stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let hello = match token {
                Some(token) => Packet::Reconnect(ReconnectRequest {
                    version: PROTOCOL_VERSION,
                    token: *token,
                }),
                None => Packet::Join(JoinRequest {
                    version: PROTOCOL_VERSION,
                }),
            };
            gemini_proto::write_packet(&mut stream, &hello).expect("handshake");
            let welcome = match gemini_proto::read_packet(&mut stream).expect("welcome") {
                Packet::Welcome(welcome) => welcome,
                other => panic!("expected welcome, got {other:?}"),
            };
            let sim = Sim::restore(content, &welcome.snapshot).expect("welcome snapshot");
            stream.set_nonblocking(true).expect("nonblocking join");
            eprintln!("joined token {} seat {}", welcome.token, welcome.seat);
            (
                sim,
                welcome.token,
                CoopLink::Client(Client {
                    stream,
                    buf: FrameBuf::default(),
                    out: Vec::new(),
                }),
            )
        }
    }
}

pub struct Host {
    listener: TcpListener,
    peers: Vec<Peer>,
}

struct Peer {
    stream: TcpStream,
    buf: FrameBuf,
    out: Vec<u8>,
    token: u64,
    input: PlayerInput,
    welcomed: bool,
    dead: bool,
}

impl Host {
    pub fn tick(&mut self, game: &mut Game, input: PlayerInput) {
        self.accept(game);
        let mut inputs = vec![PlayerInput::default(); game.sim.players.len().max(1)];
        if !inputs.is_empty() {
            inputs[0] = input;
        }
        for peer in &mut self.peers {
            if peer.dead {
                continue;
            }
            let _ = peer.poll();
            if let Some(seat) = game.sim.players.iter().position(|player| player.token == peer.token) {
                if seat < inputs.len() {
                    inputs[seat] = peer.input;
                }
            }
        }
        game.sim.step_peers(&game.content, &inputs);
        let body = game.sim.to_bytes().unwrap_or_default();
        let tick = game.sim.tick;
        for peer in &mut self.peers {
            if peer.dead || !peer.welcomed {
                continue;
            }
            let packet = Packet::Snapshot(SnapshotPacket {
                version: PROTOCOL_VERSION,
                tick,
                body: body.clone(),
            });
            if let Ok(frame) = encode(&packet) {
                peer.enqueue(&frame);
            }
        }
    }

    fn accept(&mut self, game: &mut Game) {
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true).ok();
                    stream.set_nodelay(true).ok();
                    self.peers.push(Peer {
                        stream,
                        buf: FrameBuf::default(),
                        out: Vec::new(),
                        token: 0,
                        input: PlayerInput::default(),
                        welcomed: false,
                        dead: false,
                    });
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => {
                    eprintln!("accept failed: {err}");
                    break;
                }
            }
        }
        let mut welcomes: Vec<(usize, Welcome)> = Vec::new();
        for (index, peer) in self.peers.iter_mut().enumerate() {
            if peer.welcomed || peer.dead {
                continue;
            }
            if peer.poll().is_err() {
                peer.dead = true;
                continue;
            }
            let packet = match peer.buf.next() {
                Ok(Some(packet)) => packet,
                Ok(None) => continue,
                Err(err) => {
                    eprintln!("peer handshake: {err}");
                    peer.dead = true;
                    continue;
                }
            };
            let welcome = match packet {
                Packet::Join(_) => match game.sim.request_join(&game.content) {
                    Ok(token) => Some(token),
                    Err(err) => {
                        eprintln!("join refused: {err}");
                        peer.dead = true;
                        None
                    }
                },
                Packet::Reconnect(req) => match game.sim.reconnect_token(&game.content, req.token) {
                    Ok(_) => Some(req.token),
                    Err(err) => {
                        eprintln!("reconnect refused: {err}");
                        peer.dead = true;
                        None
                    }
                },
                _ => None,
            };
            if let Some(token) = welcome {
                peer.token = token;
                peer.welcomed = true;
                let seat = game
                    .sim
                    .players
                    .iter()
                    .position(|player| player.token == token)
                    .map(|seat| seat as u32)
                    .unwrap_or(gemini_proto::PENDING_SEAT);
                let snapshot = game.sim.to_bytes().unwrap_or_default();
                welcomes.push((
                    index,
                    Welcome {
                        version: PROTOCOL_VERSION,
                        token,
                        seat,
                        snapshot,
                    },
                ));
            }
        }
        for (index, welcome) in welcomes {
            if let Some(peer) = self.peers.get_mut(index) {
                if let Ok(frame) = encode(&Packet::Welcome(welcome)) {
                    peer.enqueue(&frame);
                }
            }
        }
    }
}

pub struct Client {
    stream: TcpStream,
    buf: FrameBuf,
    out: Vec<u8>,
}

impl Client {
    pub fn tick(&mut self, game: &mut Game, input: PlayerInput) {
        let packet = Packet::Input(InputPacket {
            version: PROTOCOL_VERSION,
            tick: game.sim.tick,
            input,
        });
        if let Ok(frame) = encode(&packet) {
            self.enqueue(&frame);
        }
        self.pull(game);
    }

    fn enqueue(&mut self, frame: &[u8]) {
        self.out.extend_from_slice(frame);
        flush_out(&mut self.stream, &mut self.out);
    }

    fn pull(&mut self, game: &mut Game) {
        let mut tmp = [0u8; 8192];
        loop {
            match self.stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => self.buf.extend(&tmp[..n]),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => {
                    eprintln!("snapshot read: {err}");
                    break;
                }
            }
        }
        while let Ok(Some(packet)) = self.buf.next() {
            if let Packet::Snapshot(snap) = packet {
                if let Ok(sim) = Sim::restore(&game.content, &snap.body) {
                    game.sim = sim;
                }
            }
        }
    }
}

impl Peer {
    fn poll(&mut self) -> io::Result<()> {
        let mut tmp = [0u8; 8192];
        loop {
            match self.stream.read(&mut tmp) {
                Ok(0) => {
                    self.dead = true;
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "peer closed"));
                }
                Ok(n) => self.buf.extend(&tmp[..n]),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }
        while let Ok(Some(packet)) = self.buf.next() {
            if let Packet::Input(packet) = packet {
                self.input = packet.input;
            }
        }
        flush_out(&mut self.stream, &mut self.out);
        Ok(())
    }

    fn enqueue(&mut self, frame: &[u8]) {
        self.out.extend_from_slice(frame);
        flush_out(&mut self.stream, &mut self.out);
    }
}

fn encode(packet: &Packet) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    gemini_proto::write_packet(&mut out, packet)?;
    Ok(out)
}

fn flush_out(stream: &mut TcpStream, out: &mut Vec<u8>) {
    while !out.is_empty() {
        match stream.write(out) {
            Ok(0) => break,
            Ok(n) => {
                out.drain(..n);
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
            Err(_) => {
                out.clear();
                break;
            }
        }
    }
}
