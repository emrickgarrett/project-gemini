//! Crate pulls cross a proto boundary. This module does not roll and does not keep a wallet.
//! `dev-host` swaps in the local stand-in. A real server replaces that transport only.

use gemini_proto::{CratePullReply, CratePullRequest, Packet, PROTOCOL_VERSION};

pub(crate) fn request_bytes(banner: &str) -> Result<Vec<u8>, String> {
    Packet::CratePull(CratePullRequest {
        version: PROTOCOL_VERSION,
        banner: banner.to_string(),
    })
    .encode()
    .map_err(|err| err.to_string())
}

/// Encode a pull, hand the bytes to the host, and decode the reply packet.
pub(crate) fn round_trip(banner: &str) -> Result<CratePullReply, String> {
    let bytes = request_bytes(banner)?;
    let reply = transport(&bytes)?;
    match Packet::decode(&reply).map_err(|err| err.to_string())? {
        Packet::CratePullReply(reply) => Ok(reply),
        _ => Err("crate host returned an unexpected packet".into()),
    }
}

#[cfg(feature = "dev-host")]
fn transport(bytes: &[u8]) -> Result<Vec<u8>, String> {
    gemini_server::dev_host::exchange(bytes)
}

#[cfg(not(feature = "dev-host"))]
fn transport(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let _ = bytes;
    Err("no crate host".into())
}

pub(crate) fn boot() {
    let reset = std::env::args().any(|arg| arg == "--reset-dev-account");
    #[cfg(feature = "dev-host")]
    {
        if reset {
            match gemini_server::dev_host::reset() {
                Ok(path) => {
                    println!("wiped dev account {}", path.display());
                    std::process::exit(0);
                }
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(1);
                }
            }
        }
        if let Err(err) = gemini_server::dev_host::require_readable() {
            eprintln!("{err}");
            std::process::exit(1);
        }
        return;
    }
    #[cfg(not(feature = "dev-host"))]
    if reset {
        eprintln!("--reset-dev-account needs the dev-host feature");
        eprintln!("cargo run -p gemini-client --features dev-host -- --reset-dev-account");
        std::process::exit(1);
    }
}
