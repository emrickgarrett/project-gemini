//! Crate pulls cross a proto boundary. This module does not roll and does not keep a wallet.
//! `dev-host` swaps in the local stand-in. A real server replaces that transport only.

use std::fmt;

use gemini_proto::{CratePullReply, CratePullRequest, Packet, PROTOCOL_VERSION};

/// Why a pull did not come back as a reply. Flow matches this. It does not search the text.
#[derive(Debug)]
pub(crate) enum PullHostError {
    /// Filled by the dev-host feature when the save cannot be parsed.
    #[allow(dead_code)]
    Corrupt(String),
    /// Filled by the dev-host feature when the save cannot be read.
    #[allow(dead_code)]
    Unreadable(String),
    Unavailable(String),
    /// The host may already have saved the pull. The same key must be sent again.
    Lost(String),
}

impl fmt::Display for PullHostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corrupt(message) | Self::Unreadable(message) | Self::Unavailable(message) | Self::Lost(message) => f.write_str(message),
        }
    }
}

pub(crate) fn request_bytes(banner: &str, key: &str, ten: bool) -> Result<Vec<u8>, PullHostError> {
    Packet::CratePull(CratePullRequest {
        version: PROTOCOL_VERSION,
        banner: banner.to_string(),
        idempotency_key: key.to_string(),
        ten,
        seen: false,
    })
    .encode()
    .map_err(|err| PullHostError::Unavailable(err.to_string()))
}

#[cfg(test)]
thread_local! {
    static NEXT_PULL: std::cell::RefCell<Vec<Result<CratePullReply, PullHostError>>> = const { std::cell::RefCell::new(Vec::new()) };
    static NEXT_RAW: std::cell::RefCell<Option<Vec<u8>>> = const { std::cell::RefCell::new(None) };
    static ON_SEND: std::cell::RefCell<Vec<Box<dyn Fn()>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// One scripted reply for the next `round_trip`. Tests use this. The host is not called.
#[cfg(test)]
pub(crate) fn script_next_pull(reply: Result<CratePullReply, PullHostError>) {
    NEXT_PULL.with(|slot| {
        slot.borrow_mut().clear();
        slot.borrow_mut().push(reply);
    });
}

/// Replies in order. One press that settles and then pulls consumes two.
#[cfg(test)]
pub(crate) fn script_pulls(replies: Vec<Result<CratePullReply, PullHostError>>) {
    NEXT_PULL.with(|slot| *slot.borrow_mut() = replies);
}

#[cfg(test)]
pub(crate) fn clear_scripted_pull() {
    NEXT_PULL.with(|slot| slot.borrow_mut().clear());
    NEXT_RAW.with(|slot| *slot.borrow_mut() = None);
    ON_SEND.with(|slot| slot.borrow_mut().clear());
}

/// Host bytes for the next `round_trip`. Decode still runs, so a bad packet is `Lost`.
#[cfg(test)]
pub(crate) fn script_raw_reply(bytes: Vec<u8>) {
    NEXT_RAW.with(|slot| *slot.borrow_mut() = Some(bytes));
}

/// Runs at the start of the next `round_trip`, before the host is asked.
#[cfg(test)]
pub(crate) fn on_next_round_trip(hook: impl Fn() + 'static) {
    ON_SEND.with(|slot| slot.borrow_mut().push(Box::new(hook)));
}

/// Encode a pull, hand the bytes to the host, and decode the reply packet.
pub(crate) fn round_trip(banner: &str, key: &str, ten: bool) -> Result<CratePullReply, PullHostError> {
    #[cfg(test)]
    if let Some(hook) = ON_SEND.with(|slot| {
        let mut hooks = slot.borrow_mut();
        if hooks.is_empty() { None } else { Some(hooks.remove(0)) }
    }) {
        hook();
    }
    #[cfg(test)]
    if let Some(scripted) = NEXT_PULL.with(|slot| {
        let mut queue = slot.borrow_mut();
        if queue.is_empty() { None } else { Some(queue.remove(0)) }
    }) {
        return scripted;
    }
    let bytes = request_bytes(banner, key, ten)?;
    #[cfg(test)]
    let reply = if let Some(raw) = NEXT_RAW.with(|slot| slot.borrow_mut().take()) {
        raw
    } else {
        transport(&bytes)?
    };
    #[cfg(not(test))]
    let reply = transport(&bytes)?;
    // A decode failure or a packet that is not a pull reply means the host may already
    // have saved the pull. Those are Lost, not a clean refusal.
    match Packet::decode(&reply).map_err(|err| PullHostError::Lost(err.to_string()))? {
        Packet::CratePullReply(reply) => Ok(reply),
        _ => Err(PullHostError::Lost("crate host returned an unexpected packet".into())),
    }
}

#[cfg(feature = "dev-host")]
fn transport(bytes: &[u8]) -> Result<Vec<u8>, PullHostError> {
    gemini_server::dev_host::exchange(bytes).map_err(|err| match err {
        gemini_server::dev_host::ExchangeError::Corrupt(message) => PullHostError::Corrupt(message),
        gemini_server::dev_host::ExchangeError::Unreadable(message) => PullHostError::Unreadable(message),
        gemini_server::dev_host::ExchangeError::Failed(message) => PullHostError::Unavailable(message),
        gemini_server::dev_host::ExchangeError::Lost(message) => PullHostError::Lost(message),
    })
}

#[cfg(not(feature = "dev-host"))]
fn transport(bytes: &[u8]) -> Result<Vec<u8>, PullHostError> {
    let _ = bytes;
    Err(PullHostError::Unavailable("no crate host".into()))
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
