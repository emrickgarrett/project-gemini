//! Player-hosted listen server. The sim stays socket-free; this crate owns TCP.

#[cfg(all(feature = "dev-host", not(debug_assertions)))]
compile_error!("dev-host is a debug stand-in and cannot be built in release");

#[cfg(feature = "dev-host")]
pub mod dev_host;

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use gemini_proto::{
    read_packet, write_packet, InputPacket, JoinRequest, Packet, ReconnectRequest, SnapshotPacket, Welcome,
    PROTOCOL_VERSION, PENDING_SEAT,
};
use gemini_sim::sim::{Phase, PlayerInput, Sim};
use gemini_sim::Content;

pub fn main_cli() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--listen") => {
            let addr = args.get(1).map(String::as_str).unwrap_or("127.0.0.1:17650");
            let listener = TcpListener::bind(addr).unwrap_or_else(|err| panic!("bind {addr}: {err}"));
            let bound = listener.local_addr().expect("local addr");
            println!("listening {bound}");
            let content = Content::embedded();
            let mut sim = Sim::new(&content, "normal", 11);
            let ticks = arg_after(&args, "--ticks").unwrap_or(180);
            let sim = host_exchange(listener, &mut sim, &content, ticks);
            println!(
                "gemini-server listen tick={} seats={} pending={} phase={}",
                sim.tick,
                sim.players.iter().filter(|player| player.connected).count(),
                sim.pending_joins.len(),
                phase_name(&sim.phase)
            );
        }
        Some("--join") => {
            let addr: SocketAddr = args
                .get(1)
                .map(String::as_str)
                .unwrap_or("127.0.0.1:17650")
                .parse()
                .unwrap_or_else(|err| panic!("join address: {err}"));
            let ticks = arg_after(&args, "--ticks").unwrap_or(60);
            let token = arg_after(&args, "--token").map(|value| value as u64);
            let sim = client_exchange(addr, ticks, token);
            let me = token.and_then(|token| sim.players.iter().find(|player| player.token == token));
            println!(
                "gemini-server join tick={} seats={} pos={}",
                sim.tick,
                sim.players.len(),
                me.map(|player| format!("{:.1},{:.1},{:.1}", player.pos.x, player.pos.y, player.pos.z))
                    .unwrap_or_else(|| "queued".into())
            );
        }
        _ => smoke(),
    }
}

fn arg_after(args: &[String], flag: &str) -> Option<u32> {
    args.iter().position(|arg| arg == flag).and_then(|index| args.get(index + 1)).and_then(|text| text.parse().ok())
}

fn smoke() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 11);
    let input = PlayerInput {
        move_z: 1.0,
        fire: true,
        yaw: 0.4,
        ..PlayerInput::default()
    };
    for _ in 0..180 {
        sim.step(&content, input);
    }
    println!(
        "gemini-server smoke tick={} wave={} left={} phase={}",
        sim.tick,
        sim.director.wave,
        sim.enemies_left(),
        phase_name(&sim.phase)
    );
}

fn phase_name(phase: &Phase) -> &'static str {
    match phase {
        Phase::Combat => "combat",
        Phase::Upgrade { .. } => "upgrade",
        Phase::Breather { .. } => "breather",
        Phase::Victory => "victory",
        Phase::Defeat => "defeat",
    }
}

fn prepare(stream: &TcpStream) -> io::Result<()> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    Ok(())
}

/// Accept one peer, exchange inputs, and return the host sim.
pub fn host_exchange(listener: TcpListener, sim: &mut Sim, content: &Content, ticks: u32) -> Sim {
    let (mut stream, _) = listener.accept().expect("accept");
    prepare(&stream).expect("peer socket");
    let first = read_packet(&mut stream).expect("handshake");
    let token = match first {
        Packet::Join(_) => sim.request_join(content).expect("join"),
        Packet::Reconnect(req) => {
            sim.reconnect_token(content, req.token).expect("reconnect");
            req.token
        }
        other => panic!("expected join or reconnect, got {other:?}"),
    };
    send_welcome(&mut stream, sim, token);
    for _ in 0..ticks {
        let input = match read_packet(&mut stream).expect("input") {
            Packet::Input(packet) => packet.input,
            other => panic!("expected input, got {other:?}"),
        };
        let mut inputs = vec![PlayerInput::default(); sim.players.len().max(1)];
        if let Some(seat) = sim.players.iter().position(|player| player.token == token) {
            if seat < inputs.len() {
                inputs[seat] = input;
            }
        }
        sim.step_peers(content, &inputs);
        let body = sim.to_bytes().expect("snapshot");
        write_packet(
            &mut stream,
            &Packet::Snapshot(SnapshotPacket {
                version: PROTOCOL_VERSION,
                tick: sim.tick,
                body,
            }),
        )
        .expect("snapshot");
    }
    sim.clone()
}

fn send_welcome(stream: &mut TcpStream, sim: &Sim, token: u64) {
    let seat = sim
        .players
        .iter()
        .position(|player| player.token == token)
        .map(|seat| seat as u32)
        .unwrap_or(PENDING_SEAT);
    let snapshot = sim.to_bytes().expect("welcome snapshot");
    write_packet(
        stream,
        &Packet::Welcome(Welcome {
            version: PROTOCOL_VERSION,
            token,
            seat,
            snapshot,
        }),
    )
    .expect("welcome");
}

pub fn client_exchange(addr: SocketAddr, ticks: u32, token: Option<u64>) -> Sim {
    let mut stream = TcpStream::connect(addr).unwrap_or_else(|err| panic!("connect {addr}: {err}"));
    prepare(&stream).expect("socket");
    let hello = match token {
        Some(token) => Packet::Reconnect(ReconnectRequest {
            version: PROTOCOL_VERSION,
            token,
        }),
        None => Packet::Join(JoinRequest {
            version: PROTOCOL_VERSION,
        }),
    };
    write_packet(&mut stream, &hello).expect("hello");
    let welcome = match read_packet(&mut stream).expect("welcome") {
        Packet::Welcome(welcome) => welcome,
        other => panic!("expected welcome, got {other:?}"),
    };
    let content = Content::embedded();
    let mut sim = Sim::restore(&content, &welcome.snapshot).expect("restore welcome");
    let input = PlayerInput {
        move_z: 1.0,
        ..PlayerInput::default()
    };
    for _ in 0..ticks {
        write_packet(
            &mut stream,
            &Packet::Input(InputPacket {
                version: PROTOCOL_VERSION,
                tick: sim.tick,
                input,
            }),
        )
        .expect("send input");
        let snap = match read_packet(&mut stream).expect("snapshot") {
            Packet::Snapshot(snap) => snap,
            other => panic!("expected snapshot, got {other:?}"),
        };
        sim = Sim::restore(&content, &snap.body).expect("restore snapshot");
    }
    let _ = welcome.token;
    sim
}
