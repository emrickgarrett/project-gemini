//! Versioned envelopes for M2. Nothing on the wire in the single-player slice.
use serde::{Deserialize, Serialize};

use gemini_sim::PlayerInput;

pub const PROTOCOL_VERSION: u32 = 1;

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
    /// `bincode` bytes of a `gemini_sim::Sim` snapshot.
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Packet {
    Hello(Hello),
    Input(InputPacket),
    Snapshot(SnapshotPacket),
}

impl Packet {
    pub fn encode(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}
