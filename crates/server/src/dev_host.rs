//! Local stand-in host for crate pulls.
//! Delete this file and the `dev-host` feature to drop it.
//! The client only exchanges [`gemini_proto::Packet`] bytes. A real server can
//! read the same `CratePull` and write the same `CratePullReply`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use std::sync::{Mutex, MutexGuard, OnceLock};

use gemini_backend::{Account, BankError, MatchBook, MatchRecord, PullError};
use gemini_proto::{
    BankFault, BankReply, BankRequest, CratePullFault, CratePullReply, Packet, PROTOCOL_VERSION,
};
use gemini_sim::Content;

const SAVE_NAME: &str = "gemini-dev-account.bin";

/// A failed stand-in exchange. The client matches the variant. It does not search the text.
#[derive(Debug)]
pub enum ExchangeError {
    Corrupt(String),
    Unreadable(String),
    Failed(String),
}

impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Corrupt(message) | Self::Unreadable(message) | Self::Failed(message) => f.write_str(message),
        }
    }
}

/// Save written in the process working directory. `cargo run` keeps the
/// directory you launched from, so a run from the repo root writes
/// `gemini-dev-account.bin` next to `Cargo.toml`.
pub fn save_path() -> PathBuf {
    PathBuf::from(SAVE_NAME)
}

/// Delete the save. A missing file is already reset. This does not create a new account.
pub fn reset() -> Result<PathBuf, ExchangeError> {
    reset_at(&save_path())
}

pub fn reset_at(path: &Path) -> Result<PathBuf, ExchangeError> {
    if path.exists() {
        fs::remove_file(path).map_err(|err| ExchangeError::Failed(format!("could not wipe {}: {err}", path.display())))?;
    }
    let partial = partial_path(path);
    if partial.exists() {
        fs::remove_file(&partial).map_err(|err| ExchangeError::Failed(format!("could not wipe {}: {err}", partial.display())))?;
    }
    Ok(path.to_path_buf())
}

/// Missing file is fine. A present file that cannot be read is an error.
pub fn require_readable() -> Result<(), ExchangeError> {
    require_readable_at(&save_path())
}

pub fn require_readable_at(path: &Path) -> Result<(), ExchangeError> {
    if !path.exists() {
        return Ok(());
    }
    let bytes = fs::read(path).map_err(|err| unreadable(path, err))?;
    Account::from_bytes(&bytes).map(|_| ()).map_err(|err| corrupt(path, err))
}

/// One proto round trip against the on-disk account.
pub fn exchange(request: &[u8]) -> Result<Vec<u8>, ExchangeError> {
    exchange_at(&save_path(), request)
}

pub fn exchange_at(path: &Path, request: &[u8]) -> Result<Vec<u8>, ExchangeError> {
    exchange_with(path, &Content::embedded(), request)
}

fn exchange_with(path: &Path, content: &Content, request: &[u8]) -> Result<Vec<u8>, ExchangeError> {
    let packet = Packet::decode(request).map_err(|err| ExchangeError::Failed(format!("host request is not a proto packet: {err}")))?;
    let mut account = load_or_create(path, content)?;
    let reply = answer(&mut account, content, &packet)?;
    if !matches!(&reply, Packet::CratePullReply(body) if matches!(body.fault, Some(CratePullFault::Shards))) {
        write_account(path, &account)?;
    }
    reply.encode().map_err(|err| ExchangeError::Failed(format!("could not encode the host reply: {err}")))
}

fn books() -> Result<MutexGuard<'static, MatchBook>, ExchangeError> {
    static BOOKS: OnceLock<Mutex<MatchBook>> = OnceLock::new();
    BOOKS
        .get_or_init(|| Mutex::new(MatchBook::new()))
        .lock()
        .map_err(|_| ExchangeError::Failed("match book lock is poisoned".into()))
}

/// Mint a match id. The server copies gain off its own sim later.
pub fn open_match() -> Result<u64, ExchangeError> {
    Ok(books()?.open())
}

/// Copy `currency_gain` off the sim the server is running. The client does not send a total.
pub fn note_match(id: u64, sim: &gemini_sim::Sim) -> Result<(), ExchangeError> {
    books()?.note(id, sim).map_err(ExchangeError::Failed)
}

/// Apply one already-decoded packet. Callers on a socket use this after `Packet::decode`.
pub fn answer(account: &mut Account, content: &Content, packet: &Packet) -> Result<Packet, ExchangeError> {
    match packet {
        Packet::CratePull(request) => answer_pull(account, content, request),
        Packet::Bank(request) => {
            let record = books()?.record(request.match_id).cloned();
            answer_bank(account, record.as_ref(), request)
        }
        Packet::Hello(_)
        | Packet::Input(_)
        | Packet::Snapshot(_)
        | Packet::Join(_)
        | Packet::Reconnect(_)
        | Packet::Welcome(_)
        | Packet::HostAway { .. }
        | Packet::CratePullReply(_)
        | Packet::BankReply(_) => Err(ExchangeError::Failed("the stand-in only answers a crate pull or a bank packet".into())),
    }
}

fn answer_pull(account: &mut Account, content: &Content, request: &gemini_proto::CratePullRequest) -> Result<Packet, ExchangeError> {
    if request.version != PROTOCOL_VERSION {
        return Err(ExchangeError::Failed(format!(
            "crate pull version {} does not match {PROTOCOL_VERSION}",
            request.version
        )));
    }
    let reply = match account.pull(content, &request.banner) {
        Ok(pull) => CratePullReply {
            version: PROTOCOL_VERSION,
            rarity: Some(pull.rarity),
            fakeout: pull.fakeout,
            rank: pull.rank,
            fault: None,
        },
        Err(PullError::Short { cost, have }) => CratePullReply {
            version: PROTOCOL_VERSION,
            rarity: None,
            fakeout: None,
            rank: account_rank(account),
            fault: Some(CratePullFault::Short { cost, have }),
        },
        Err(PullError::Banner) => CratePullReply {
            version: PROTOCOL_VERSION,
            rarity: None,
            fakeout: None,
            rank: account_rank(account),
            fault: Some(CratePullFault::Banner),
        },
        Err(PullError::Shards) => CratePullReply {
            version: PROTOCOL_VERSION,
            rarity: None,
            fakeout: None,
            rank: account_rank(account),
            fault: Some(CratePullFault::Shards),
        },
    };
    Ok(Packet::CratePullReply(reply))
}

fn answer_bank(account: &mut Account, record: Option<&MatchRecord>, request: &BankRequest) -> Result<Packet, ExchangeError> {
    if request.version != PROTOCOL_VERSION {
        return Err(ExchangeError::Failed(format!(
            "bank version {} does not match {PROTOCOL_VERSION}",
            request.version
        )));
    }
    let Some(record) = record else {
        return Ok(Packet::BankReply(bank_fault(BankFault::Unknown)));
    };
    if record.id != request.match_id {
        return Ok(Packet::BankReply(bank_fault(BankFault::Unknown)));
    }
    let reply = match account.bank(record, request.player_id) {
        Ok(receipt) => BankReply {
            version: PROTOCOL_VERSION,
            earned: receipt.earned,
            bonus: receipt.bonus,
            balance: receipt.balance,
            fresh: receipt.fresh,
            fault: None,
        },
        Err(BankError::Player) => bank_fault(BankFault::Player),
        Err(BankError::Amount) => bank_fault(BankFault::Amount),
    };
    Ok(Packet::BankReply(reply))
}

fn bank_fault(fault: BankFault) -> BankReply {
    BankReply {
        version: PROTOCOL_VERSION,
        earned: 0.0,
        bonus: 0.0,
        balance: 0.0,
        fresh: false,
        fault: Some(fault),
    }
}

fn account_rank(account: &Account) -> u32 {
    account.rank
}

fn load_or_create(path: &Path, content: &Content) -> Result<Account, ExchangeError> {
    if path.exists() {
        let bytes = fs::read(path).map_err(|err| unreadable(path, err))?;
        return Account::from_bytes(&bytes).map_err(|err| corrupt(path, err));
    }
    let partial = partial_path(path);
    if partial.exists() {
        let bytes = fs::read(&partial).map_err(|err| unreadable(&partial, err))?;
        let account = Account::from_bytes(&bytes).map_err(|err| corrupt(&partial, err))?;
        write_account(path, &account)?;
        return Ok(account);
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| ExchangeError::Failed(format!("could not create {}: {err}", parent.display())))?;
        }
    }
    let account = Account::open(content);
    write_account(path, &account)?;
    Ok(account)
}

fn corrupt(path: &Path, err: impl std::fmt::Display) -> ExchangeError {
    ExchangeError::Corrupt(format!("dev account save {} is corrupt: {err}", path.display()))
}

fn unreadable(path: &Path, err: impl std::fmt::Display) -> ExchangeError {
    ExchangeError::Unreadable(format!("dev account save {} is unreadable: {err}", path.display()))
}

fn partial_path(path: &Path) -> PathBuf {
    let mut partial = path.as_os_str().to_os_string();
    partial.push(".partial");
    PathBuf::from(partial)
}

fn write_account(path: &Path, account: &Account) -> Result<(), ExchangeError> {
    let bytes = account.to_bytes().map_err(|err| ExchangeError::Failed(format!("could not encode the dev account: {err}")))?;
    let partial = partial_path(path);
    {
        let mut file = fs::File::create(&partial).map_err(|err| ExchangeError::Failed(format!("could not write {}: {err}", partial.display())))?;
        file.write_all(&bytes).map_err(|err| ExchangeError::Failed(format!("could not write {}: {err}", partial.display())))?;
        file.sync_all().ok();
    }
    let written = fs::read(&partial).map_err(|err| unreadable(&partial, err))?;
    Account::from_bytes(&written).map_err(|err| corrupt(&partial, err))?;
    if path.exists() {
        fs::remove_file(path).map_err(|err| ExchangeError::Failed(format!("could not replace {}: {err}", path.display())))?;
    }
    fs::rename(&partial, path).map_err(|err| ExchangeError::Failed(format!("could not replace {}: {err}", path.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gemini_proto::{BankFault, BankRequest, CratePullRequest, Packet};

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("gemini-dev-host-{}-{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn request(banner: &str) -> Vec<u8> {
        Packet::CratePull(CratePullRequest {
            version: PROTOCOL_VERSION,
            banner: banner.to_string(),
        })
        .encode()
        .unwrap()
    }

    #[test]
    fn a_pull_succeeds_and_the_save_keeps_pity_and_credits() {
        let dir = scratch("pull");
        let path = dir.join("account.bin");
        let content = Content::embedded();
        let banner = content.banners.id.clone();
        let cost = content.banners.cost.single as f32;
        let bytes = request(&banner);
        let first = Packet::decode(&exchange_at(&path, &bytes).unwrap()).unwrap();
        let Packet::CratePullReply(reply) = first else {
            panic!("the stand-in must answer with the crate pull reply");
        };
        assert!(reply.fault.is_none(), "{reply:?}");
        let rarity = reply.rarity.expect("a funded pull returns a tier");
        assert!(["common", "rare", "epic", "legendary"].contains(&rarity.as_str()));
        let saved = Account::from_bytes(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.credits(), content.banners.dev.starting_credits - cost);
        assert!(saved.since_legendary(&banner) <= 1);
        assert_eq!(saved.audit.len(), 1);

        let second = Packet::decode(&exchange_at(&path, &bytes).unwrap()).unwrap();
        let Packet::CratePullReply(again) = second else { panic!("reply") };
        assert!(again.fault.is_none(), "{again:?}");
        let reloaded = Account::from_bytes(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(reloaded.credits(), content.banners.dev.starting_credits - cost * 2.0);
        assert_eq!(reloaded.audit.len(), 2);
        assert_ne!(reloaded.credits(), content.banners.dev.starting_credits, "a launch must not grant again");

        let before = fs::read(&path).unwrap();
        fs::write(&path, b"not-an-account").unwrap();
        let err = exchange_at(&path, &bytes).unwrap_err();
        assert!(matches!(err, ExchangeError::Corrupt(_)), "{err}");
        assert_eq!(fs::read(&path).unwrap(), b"not-an-account", "a bad save must stay in place");
        let _ = before;
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_save_fails_and_reset_restarts_credits_and_pity() {
        let dir = scratch("reset");
        let path = dir.join("account.bin");
        let content = Content::embedded();
        let banner = content.banners.id.as_str();
        exchange_at(&path, &request(banner)).unwrap();
        exchange_at(&path, &request(banner)).unwrap();
        let spent = Account::from_bytes(&fs::read(&path).unwrap()).unwrap();
        let cost = content.banners.cost.single as f32;
        assert_eq!(spent.credits(), content.banners.dev.starting_credits - cost * 2.0);
        assert_eq!(spent.audit.len(), 2);

        let blocked = dir.join("blocked");
        fs::create_dir(&blocked).unwrap();
        let err = require_readable_at(&blocked).unwrap_err();
        assert!(matches!(err, ExchangeError::Unreadable(_)), "{err}");
        assert!(blocked.is_dir(), "an unreadable path is not replaced with a new account");

        reset_at(&path).unwrap();
        assert!(!path.exists());
        exchange_at(&path, &request(banner)).unwrap();
        let fresh = Account::from_bytes(&fs::read(&path).unwrap()).unwrap();
        let cost = content.banners.cost.single as f32;
        assert_eq!(fresh.credits(), content.banners.dev.starting_credits - cost);
        assert_eq!(fresh.audit.len(), 1, "reset drops the old audit with the old pity");
        assert!(fresh.since_legendary(banner) <= 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_short_wallet_names_the_cost_without_a_tier() {
        let dir = scratch("short");
        let path = dir.join("account.bin");
        let content = Content::embedded();
        let banner = content.banners.id.as_str();
        let mut account = Account::open(&content);
        account.pull(&content, banner).unwrap();
        while account.pull(&content, banner).is_ok() {}
        write_account(&path, &account).unwrap();
        let have = account.credits();
        let reply = Packet::decode(&exchange_at(&path, &request(banner)).unwrap()).unwrap();
        let Packet::CratePullReply(reply) = reply else { panic!("reply") };
        assert!(matches!(reply.fault, Some(CratePullFault::Short { have: got, .. }) if (got - have).abs() < 1e-3));
        assert!(reply.rarity.is_none());
        let still = Account::from_bytes(&fs::read(&path).unwrap()).unwrap();
        assert!((still.credits() - have).abs() < 1e-3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_shards_fault_does_not_rewrite_the_save() {
        let dir = scratch("shards");
        let path = dir.join("account.bin");
        let mut content = Content::embedded();
        content.cosmetics.forget_shard_row("common").unwrap();
        let mut account = Account::open(&content);
        account.rank = content.duplicate.max_rank;
        write_account(&path, &account).unwrap();
        let before = fs::read(&path).unwrap();
        let banner = content.banners.id.clone();
        let reply = Packet::decode(&exchange_with(&path, &content, &request(&banner)).unwrap()).unwrap();
        let Packet::CratePullReply(reply) = reply else { panic!("reply") };
        assert!(matches!(reply.fault, Some(CratePullFault::Shards)), "{reply:?}");
        assert!(reply.rarity.is_none());
        assert_eq!(fs::read(&path).unwrap(), before, "a shard fault must leave the save where it was");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bank_request_has_no_amount_and_a_repeat_does_not_pay_again() {
        let dir = scratch("bank");
        let path = dir.join("account.bin");
        let content = Content::embedded();
        let sim = gemini_sim::Sim::new(&content, "normal", 1);
        let player = sim.players[0].token;
        let id = open_match().unwrap();
        note_match(id, &sim).unwrap();
        let request = BankRequest { version: PROTOCOL_VERSION, match_id: id, player_id: player };
        let bytes = Packet::Bank(request.clone()).encode().unwrap();
        let Packet::Bank(decoded) = Packet::decode(&bytes).unwrap() else { panic!("bank request") };
        assert_eq!(decoded, request);
        let first = Packet::decode(&exchange_at(&path, &bytes).unwrap()).unwrap();
        let Packet::BankReply(first) = first else { panic!("bank reply") };
        assert!(first.fault.is_none(), "{first:?}");
        assert!(first.fresh);
        assert_eq!(first.earned, 0.0, "an open wave is not a deposit");
        assert_eq!(first.earned, sim.currency_gain[&player]);
        let saved = Account::from_bytes(&fs::read(&path).unwrap()).unwrap();
        assert!((saved.credits() - first.balance).abs() < 1e-3);
        let second = Packet::decode(&exchange_at(&path, &bytes).unwrap()).unwrap();
        let Packet::BankReply(second) = second else { panic!("bank reply") };
        assert!(!second.fresh);
        assert_eq!(second.earned, first.earned);
        assert!((second.balance - first.balance).abs() < 1e-3);
        let missing = BankRequest { version: PROTOCOL_VERSION, match_id: id.wrapping_add(9), player_id: player };
        let unknown = Packet::decode(&exchange_at(&path, &Packet::Bank(missing).encode().unwrap()).unwrap()).unwrap();
        let Packet::BankReply(unknown) = unknown else { panic!("bank reply") };
        assert_eq!(unknown.fault, Some(BankFault::Unknown));
        let still = Account::from_bytes(&fs::read(&path).unwrap()).unwrap();
        assert!((still.credits() - first.balance).abs() < 1e-3);
        let _ = fs::remove_dir_all(&dir);
    }
}
