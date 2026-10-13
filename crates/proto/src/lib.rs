//! Versioned envelopes for the listen server.
//! Each tick's "delta" is a full sim snapshot until interest management exists.
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use gemini_sim::PlayerInput;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME: usize = 8 * 1024 * 1024;
/// Welcome seat when the joiner is queued for the next breather.
pub const PENDING_SEAT: u32 = u32::MAX;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Hello {
    pub version: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InputPacket {
    pub version: u32,
    pub tick: u64,
    pub input: PlayerInput,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SnapshotPacket {
    pub version: u32,
    pub tick: u64,
    /// `bincode` bytes of a `gemini_sim::Sim` snapshot. The whole sim, not an interest delta.
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct JoinRequest {
    pub version: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReconnectRequest {
    pub version: u32,
    pub token: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Welcome {
    pub version: u32,
    pub token: u64,
    pub seat: u32,
    pub snapshot: Vec<u8>,
}

/// A crate pull. The host rolls. The client sends this and waits for [`CratePullReply`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CratePullRequest {
    pub version: u32,
    pub banner: String,
}

/// Why the host did not return a tier. The client prints copy from content. It does not roll.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum CratePullFault {
    Short { cost: f32, have: f32 },
    Banner,
    /// The peer speaks this protocol and has no wallet.
    Unavailable,
    /// The rolled rarity has no shard cost in content.
    Shards,
}

/// The host's answer to [`CratePullRequest`]. A later server sends the same bytes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CratePullReply {
    pub version: u32,
    pub rarity: Option<String>,
    pub fakeout: Option<String>,
    pub rank: u32,
    pub fault: Option<CratePullFault>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Packet {
    Hello(Hello),
    Input(InputPacket),
    Snapshot(SnapshotPacket),
    Join(JoinRequest),
    Reconnect(ReconnectRequest),
    Welcome(Welcome),
    /// Host entered away after the detect wait. Clients show the banner on their next tick.
    HostAway { version: u32 },
    CratePull(CratePullRequest),
    CratePullReply(CratePullReply),
}

impl Packet {
    pub fn encode(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

pub fn write_packet(stream: &mut impl Write, packet: &Packet) -> std::io::Result<()> {
    let body = packet
        .encode()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))?;
    if body.len() > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "packet exceeds frame cap",
        ));
    }
    stream.write_all(&(body.len() as u32).to_le_bytes())?;
    stream.write_all(&body)?;
    stream.flush()?;
    Ok(())
}

pub fn read_packet(stream: &mut impl Read) -> std::io::Result<Packet> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "packet exceeds frame cap",
        ));
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body)?;
    Packet::decode(&body).map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))
}

/// Bytes already pulled off a nonblocking socket.
#[derive(Default)]
pub struct FrameBuf {
    data: Vec<u8>,
}

impl FrameBuf {
    pub fn extend(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    pub fn next(&mut self) -> std::io::Result<Option<Packet>> {
        if self.data.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_le_bytes(self.data[..4].try_into().unwrap()) as usize;
        if len > MAX_FRAME {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "packet exceeds frame cap",
            ));
        }
        if self.data.len() < 4 + len {
            return Ok(None);
        }
        let body = self.data[4..4 + len].to_vec();
        self.data.drain(..4 + len);
        let packet = Packet::decode(&body)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))?;
        Ok(Some(packet))
    }
}
