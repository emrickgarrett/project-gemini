//! Match deposits cross a proto boundary. This module does not keep a wallet and does not send a total.
//! `dev-host` swaps in the local stand-in. A real server replaces that transport only.

use std::fmt;

use gemini_proto::{BankReply, BankRequest, Packet, PROTOCOL_VERSION};
use gemini_sim::Sim;

/// Why a deposit did not come back as a receipt. Flow matches this. It does not search the text.
#[derive(Debug)]
pub(crate) enum BankHostError {
    Unavailable(String),
}

impl fmt::Display for BankHostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) => f.write_str(message),
        }
    }
}

/// Mint a match id on the host. Without a host this is unavailable and writes nothing.
pub(crate) fn open() -> Result<u64, BankHostError> {
    #[cfg(feature = "dev-host")]
    {
        return gemini_server::dev_host::open_match().map_err(|err| BankHostError::Unavailable(err.to_string()));
    }
    #[cfg(not(feature = "dev-host"))]
    {
        Err(BankHostError::Unavailable("no bank host".into()))
    }
}

/// Hand the host the sim it is already running so it can copy `currency_gain`.
pub(crate) fn note(id: u64, sim: &Sim) -> Result<(), BankHostError> {
    #[cfg(feature = "dev-host")]
    {
        return gemini_server::dev_host::note_match(id, sim).map_err(|err| BankHostError::Unavailable(err.to_string()));
    }
    #[cfg(not(feature = "dev-host"))]
    {
        let _ = (id, sim);
        Err(BankHostError::Unavailable("no bank host".into()))
    }
}

pub(crate) fn request_bytes(match_id: u64, player_id: u64) -> Result<Vec<u8>, BankHostError> {
    Packet::Bank(BankRequest {
        version: PROTOCOL_VERSION,
        match_id,
        player_id,
    })
    .encode()
    .map_err(|err| BankHostError::Unavailable(err.to_string()))
}

/// Encode a deposit request. The bytes name the match and the player. They do not name an amount.
pub(crate) fn bank(match_id: u64, player_id: u64) -> Result<BankReply, BankHostError> {
    let bytes = request_bytes(match_id, player_id)?;
    let reply = transport(&bytes)?;
    match Packet::decode(&reply).map_err(|err| BankHostError::Unavailable(err.to_string()))? {
        Packet::BankReply(reply) => Ok(reply),
        Packet::Hello(_)
        | Packet::Input(_)
        | Packet::Snapshot(_)
        | Packet::Join(_)
        | Packet::Reconnect(_)
        | Packet::Welcome(_)
        | Packet::HostAway { .. }
        | Packet::CratePull(_)
        | Packet::CratePullReply(_)
        | Packet::Bank(_) => Err(BankHostError::Unavailable("bank host returned an unexpected packet".into())),
    }
}

#[cfg(feature = "dev-host")]
fn transport(bytes: &[u8]) -> Result<Vec<u8>, BankHostError> {
    gemini_server::dev_host::exchange(bytes).map_err(|err| BankHostError::Unavailable(err.to_string()))
}

#[cfg(not(feature = "dev-host"))]
fn transport(bytes: &[u8]) -> Result<Vec<u8>, BankHostError> {
    let _ = bytes;
    Err(BankHostError::Unavailable("no bank host".into()))
}
