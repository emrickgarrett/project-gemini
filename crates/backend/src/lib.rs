//! Crate pulls, pity, and the audit log.
//! The native host calls [`Account::pull`]. Wasm does not link this crate.
//! Callers play back `Pull::rarity` and `Pull::fakeout`. They do not roll them.

#[cfg(target_arch = "wasm32")]
compile_error!("gemini-backend does not build for wasm32");

use std::collections::BTreeMap;

use gemini_sim::content::Content;
use rand_chacha::ChaCha20Rng;
use rand_core::{Rng, SeedableRng};

const TIERS: [&str; 4] = ["common", "rare", "epic", "legendary"];

#[derive(Clone, Debug, Default, PartialEq)]
struct Pity {
    since_legendary: u32,
    since_epic: u32,
}

/// Why a pull did not run. The client prints this from content copy. It does not invent a tier.
#[derive(Clone, Debug, PartialEq)]
pub enum PullError {
    Short { cost: f32, have: f32 },
    Banner,
    /// The rolled tier has no shard row. The wallet is unchanged.
    Shards,
    /// A pod inside a ten-pull failed. The whole ten-pull is unchanged.
    Pod { pod: u32 },
    /// The client sent a pull with no idempotency key.
    Key,
    /// This key was saved for a different banner or a different pull kind.
    Mismatch,
    /// The rolled rarity has no item in the catalog. The wallet is unchanged.
    Item,
}

impl std::fmt::Display for PullError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Short { cost, have } => write!(f, "need {cost} credits to pull, have {have}"),
            Self::Banner => write!(f, "pull needs a banner"),
            Self::Shards => write!(f, "rolled rarity has no shard cost"),
            Self::Pod { pod } => write!(f, "pod {pod} failed"),
            Self::Key => write!(f, "pull needs an idempotency key"),
            Self::Mismatch => write!(f, "pull key was saved for a different banner"),
            Self::Item => write!(f, "rolled rarity has no item"),
        }
    }
}

/// One server pull. `fakeout` is the teased tier, exactly one step above `rarity`, or none.
#[derive(Clone, Debug, PartialEq)]
pub struct Pull {
    pub banner: String,
    pub rarity: String,
    /// Catalog id chosen by this roll. Callers draw that row. They do not pick another.
    pub item: String,
    pub fakeout: Option<String>,
    pub rank: u32,
    pub shards: u32,
    /// True when this pull raised duplicate rank. Rank is the only persistent power.
    pub powered: bool,
}

/// One row in the pull audit log.
#[derive(Clone, Debug)]
pub struct AuditEntry {
    pub banner: String,
    pub rarity: String,
    pub fakeout: Option<String>,
    pub rank: u32,
    pub shards: u32,
    pub powered: bool,
    pub credits_spent: f32,
    pub since_legendary: u32,
    pub since_epic: u32,
    pub pity_legendary_before: u32,
    pub pity_epic_before: u32,
}

/// One deposit. The key is the match id and the player id.
#[derive(Clone, Copy, Debug)]
struct Banked {
    earned: f32,
    bonus: f32,
}

/// Account wallet, per-banner pity, and the pull log. RNG state stays here.
#[derive(Clone, Debug)]
pub struct Account {
    pub rank: u32,
    pub shards: u32,
    credits: f32,
    pity: BTreeMap<String, Pity>,
    active_banner: Option<String>,
    rng: ChaCha20Rng,
    pub audit: Vec<AuditEntry>,
    /// Deposits already written. A repeat key does not pay again.
    banks: BTreeMap<(u64, u64), Banked>,
    /// Pull results already committed. A repeat key does not charge again.
    pulls_by_key: BTreeMap<String, StoredPull>,
    /// Next receipt order. Older receipts are the ones a full save drops.
    next_pull_seq: u64,
}

/// One committed pull request. A fault stores no pods and leaves the wallet untouched.
#[derive(Clone, Debug)]
struct StoredPull {
    banner: String,
    ten: bool,
    seq: u64,
    pulls: Vec<Pull>,
    fault: Option<PullError>,
}

/// The pods the client renders. `fresh` is false when this key was already stored.
#[derive(Clone, Debug)]
pub struct PullBatch {
    pub pulls: Vec<Pull>,
    pub fresh: bool,
}

impl StoredPull {
    fn replay(&self) -> Result<PullBatch, PullError> {
        match &self.fault {
            Some(err) => Err(err.clone()),
            None => Ok(PullBatch { pulls: self.pulls.clone(), fresh: false }),
        }
    }
}

/// Why a deposit did not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BankError {
    Player,
    Amount,
}

/// What the server stored. `fresh` is false when this match and player were already banked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BankReceipt {
    pub earned: f32,
    pub bonus: f32,
    pub balance: f32,
    pub fresh: bool,
}

/// Currency copied off a sim the server is running. Callers do not pass a total.
#[derive(Clone, Debug)]
pub struct PlayerGain {
    pub earned: f32,
    pub bonus: f32,
}

#[derive(Clone, Debug)]
pub struct MatchRecord {
    pub id: u64,
    gains: BTreeMap<u64, PlayerGain>,
}

/// Match ids the server minted, and the gain it copied off each sim.
#[derive(Clone, Debug)]
pub struct MatchBook {
    next_id: u64,
    matches: BTreeMap<u64, MatchRecord>,
}

impl MatchBook {
    pub fn new() -> Self {
        Self { next_id: 1, matches: BTreeMap::new() }
    }

    pub fn open(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).expect("match ids exhausted");
        self.matches.insert(id, MatchRecord { id, gains: BTreeMap::new() });
        id
    }

    pub fn note(&mut self, id: u64, sim: &gemini_sim::Sim) -> Result<(), String> {
        let record = self.matches.get_mut(&id).ok_or_else(|| format!("no match record {id}"))?;
        record.note(sim)
    }

    pub fn record(&self, id: u64) -> Option<&MatchRecord> {
        self.matches.get(&id)
    }
}

impl MatchRecord {
    /// Copy `currency_gain` off the server's sim. A missing bonus row is an error.
    pub fn note(&mut self, sim: &gemini_sim::Sim) -> Result<(), String> {
        let mut gains = BTreeMap::new();
        for (token, earned) in &sim.currency_gain {
            if !earned.is_finite() {
                return Err(format!("player {token} currency_gain is not finite"));
            }
            let bonus = sim
                .currency_bonus
                .get(token)
                .copied()
                .ok_or_else(|| format!("player {token} has currency_gain and no currency_bonus"))?;
            if !bonus.is_finite() {
                return Err(format!("player {token} currency_bonus is not finite"));
            }
            gains.insert(*token, PlayerGain { earned: *earned, bonus });
        }
        self.gains = gains;
        Ok(())
    }

    pub fn gain(&self, player_id: u64) -> Option<&PlayerGain> {
        self.gains.get(&player_id)
    }
}

impl Account {
    /// A live account. Dev builds receive `banners.dev.starting_credits`. Release builds do not.
    /// The grant is this crate's `debug_assertions` flag. No cargo feature can turn it on.
    pub fn open(content: &Content) -> Self {
        Self::open_with_dev(content, dev_currency_enabled())
    }

    /// A live account with an empty wallet. Tests and [`Account::open`] use this.
    pub fn new() -> Self {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).expect("os randomness for a live account");
        Self::from_chacha(ChaCha20Rng::from_seed(seed))
    }

    pub fn credits(&self) -> f32 {
        self.credits
    }

    fn open_with_dev(content: &Content, dev: bool) -> Self {
        let mut account = Self::new();
        if dev {
            account.grant_dev_credits(content);
        }
        account
    }

    /// Credits only. Pity counters, the banner, and the generator stay as they are.
    fn grant_dev_credits(&mut self, content: &Content) {
        self.credits = content.banners.dev.starting_credits;
    }

    /// Fixed seed for tests. Live accounts use [`Account::new`].
    #[cfg(test)]
    fn from_seed(seed: u64) -> Self {
        Self::from_chacha(ChaCha20Rng::seed_from_u64(seed))
    }

    fn from_chacha(rng: ChaCha20Rng) -> Self {
        Self {
            rank: 1,
            shards: 0,
            credits: 0.0,
            pity: BTreeMap::new(),
            active_banner: None,
            rng,
            audit: Vec::new(),
            banks: BTreeMap::new(),
            pulls_by_key: BTreeMap::new(),
            next_pull_seq: 1,
        }
    }

    /// Deposit this match's recorded gain. The key and the wallet move together.
    /// A second call for the same match and player returns the stored amounts.
    pub fn bank(&mut self, record: &MatchRecord, player_id: u64) -> Result<BankReceipt, BankError> {
        let key = (record.id, player_id);
        if let Some(prev) = self.banks.get(&key) {
            return Ok(BankReceipt {
                earned: prev.earned,
                bonus: prev.bonus,
                balance: self.credits,
                fresh: false,
            });
        }
        let Some(gain) = record.gain(player_id) else {
            return Err(BankError::Player);
        };
        if !gain.earned.is_finite() || !gain.bonus.is_finite() || gain.earned < 0.0 || gain.bonus < 0.0 {
            return Err(BankError::Amount);
        }
        self.credits += gain.earned;
        self.banks.insert(key, Banked { earned: gain.earned, bonus: gain.bonus });
        Ok(BankReceipt {
            earned: gain.earned,
            bonus: gain.bonus,
            balance: self.credits,
            fresh: true,
        })
    }

    pub fn since_legendary(&self, banner: &str) -> u32 {
        self.pity.get(banner).map(|p| p.since_legendary).unwrap_or(0)
    }

    /// Bytes for the stand-in save. A later host can use the same account fields.
    pub fn to_bytes(&self, content: &Content) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        out.extend(SAVE_V5);
        write_u32(&mut out, self.rank);
        write_u32(&mut out, self.shards);
        write_f32(&mut out, self.credits)?;
        write_opt_str(&mut out, self.active_banner.as_deref());
        write_u32(&mut out, self.pity.len() as u32);
        for (banner, pity) in &self.pity {
            write_str(&mut out, banner);
            write_u32(&mut out, pity.since_legendary);
            write_u32(&mut out, pity.since_epic);
        }
        write_u32(&mut out, self.audit.len() as u32);
        for row in &self.audit {
            write_str(&mut out, &row.banner);
            write_str(&mut out, &row.rarity);
            write_opt_str(&mut out, row.fakeout.as_deref());
            write_u32(&mut out, row.rank);
            write_u32(&mut out, row.shards);
            out.push(u8::from(row.powered));
            write_f32(&mut out, row.credits_spent)?;
            write_u32(&mut out, row.since_legendary);
            write_u32(&mut out, row.since_epic);
            write_u32(&mut out, row.pity_legendary_before);
            write_u32(&mut out, row.pity_epic_before);
        }
        out.extend(self.rng.get_seed());
        let pos = self.rng.get_word_pos();
        out.extend((pos as u64).to_le_bytes());
        out.extend(((pos >> 64) as u64).to_le_bytes());
        write_u32(&mut out, self.banks.len() as u32);
        for ((match_id, player_id), row) in &self.banks {
            out.extend(match_id.to_le_bytes());
            out.extend(player_id.to_le_bytes());
            write_f32(&mut out, row.earned)?;
            write_f32(&mut out, row.bonus)?;
        }
        let max_keys = content.banners.saved.max_keys as usize;
        if self.pulls_by_key.len() > max_keys {
            return Err("dev account save has too many pull keys".into());
        }
        write_u32(&mut out, self.pulls_by_key.len() as u32);
        for (key, stored) in &self.pulls_by_key {
            write_str(&mut out, key);
            write_str(&mut out, &stored.banner);
            out.push(if stored.ten { 2 } else { 1 });
            write_u64(&mut out, stored.seq);
            write_u32(&mut out, stored.pulls.len() as u32);
            for pull in &stored.pulls {
                write_str(&mut out, &pull.banner);
                write_str(&mut out, &pull.rarity);
                write_opt_str(&mut out, pull.fakeout.as_deref());
                write_u32(&mut out, pull.rank);
                write_u32(&mut out, pull.shards);
                out.push(u8::from(pull.powered));
                write_str(&mut out, &pull.item);
            }
            write_fault(&mut out, &stored.fault)?;
        }
        Ok(out)
    }

    /// Restore a save. Truncation, a bad header, and trailing bytes are errors.
    pub fn from_bytes(bytes: &[u8], content: &Content) -> Result<Self, String> {
        if bytes.len() < SAVE_V1.len() {
            return Err("dev account save header is wrong".into());
        }
        let magic = &bytes[..SAVE_V1.len()];
        let version = match magic {
            SAVE_V1 => 1u32,
            SAVE_V2 => 2u32,
            SAVE_V3 => 3u32,
            SAVE_V4 => 4u32,
            SAVE_V5 => 5u32,
            _ => return Err("dev account save header is wrong".into()),
        };
        let mut at = SAVE_V1.len();
        let rank = read_u32(bytes, &mut at)?;
        let shards = read_u32(bytes, &mut at)?;
        let credits = read_f32(bytes, &mut at)?;
        let active_banner = read_opt_str(bytes, &mut at)?;
        let pity_len = read_u32(bytes, &mut at)? as usize;
        if pity_len > 1024 {
            return Err("dev account save has too many pity rows".into());
        }
        let mut pity = BTreeMap::new();
        for _ in 0..pity_len {
            let banner = read_str(bytes, &mut at)?;
            let since_legendary = read_u32(bytes, &mut at)?;
            let since_epic = read_u32(bytes, &mut at)?;
            pity.insert(banner, Pity { since_legendary, since_epic });
        }
        let audit_len = read_u32(bytes, &mut at)? as usize;
        if audit_len > 1_000_000 {
            return Err("dev account save has too many audit rows".into());
        }
        let mut audit = Vec::with_capacity(audit_len);
        for _ in 0..audit_len {
            let banner = read_str(bytes, &mut at)?;
            let rarity = read_str(bytes, &mut at)?;
            let fakeout = read_opt_str(bytes, &mut at)?;
            let row_rank = read_u32(bytes, &mut at)?;
            let row_shards = read_u32(bytes, &mut at)?;
            let powered = read_u8(bytes, &mut at)?;
            if powered > 1 {
                return Err("dev account save has a bad audit flag".into());
            }
            let credits_spent = read_f32(bytes, &mut at)?;
            audit.push(AuditEntry {
                banner,
                rarity,
                fakeout,
                rank: row_rank,
                shards: row_shards,
                powered: powered == 1,
                credits_spent,
                since_legendary: read_u32(bytes, &mut at)?,
                since_epic: read_u32(bytes, &mut at)?,
                pity_legendary_before: read_u32(bytes, &mut at)?,
                pity_epic_before: read_u32(bytes, &mut at)?,
            });
        }
        let seed_bytes = read_exact(bytes, &mut at, 32)?;
        let mut seed = [0u8; 32];
        seed.copy_from_slice(seed_bytes);
        let pos_lo = read_u64(bytes, &mut at)?;
        let pos_hi = read_u64(bytes, &mut at)?;
        let mut banks = BTreeMap::new();
        if version >= 2 {
            let bank_len = read_u32(bytes, &mut at)? as usize;
            if bank_len > 1024 {
                return Err("dev account save has too many bank rows".into());
            }
            for _ in 0..bank_len {
                let match_id = read_u64(bytes, &mut at)?;
                let player_id = read_u64(bytes, &mut at)?;
                let earned = read_f32(bytes, &mut at)?;
                let bonus = read_f32(bytes, &mut at)?;
                if !earned.is_finite() || !bonus.is_finite() || earned < 0.0 || bonus < 0.0 {
                    return Err("dev account save has a bad bank amount".into());
                }
                if banks.insert((match_id, player_id), Banked { earned, bonus }).is_some() {
                    return Err("dev account save repeats a bank key".into());
                }
            }
        }
        let mut pulls_by_key = BTreeMap::new();
        if version >= 3 {
            let pull_keys = read_u32(bytes, &mut at)? as usize;
            if pull_keys > 1_000_000 {
                return Err("dev account save has too many pull keys".into());
            }
            for index in 0..pull_keys {
                let key = read_str(bytes, &mut at)?;
                let (banner, ten, seq) = if version >= 4 {
                    let banner = read_str(bytes, &mut at)?;
                    if banner.is_empty() {
                        return Err("dev account save has a pull key with no banner".into());
                    }
                    let kind = read_u8(bytes, &mut at)?;
                    let ten = match kind {
                        1 => false,
                        2 => true,
                        other => return Err(format!("dev account save has a bad pull kind {other}")),
                    };
                    let seq = read_u64(bytes, &mut at)?;
                    (banner, ten, seq)
                } else {
                    (String::new(), false, index as u64)
                };
                let pull_len = read_u32(bytes, &mut at)? as usize;
                if pull_len > 1024 {
                    return Err("dev account save has too many pods".into());
                }
                let mut pulls = Vec::with_capacity(pull_len);
                for _ in 0..pull_len {
                    let banner = read_str(bytes, &mut at)?;
                    let rarity = read_str(bytes, &mut at)?;
                    let fakeout = read_opt_str(bytes, &mut at)?;
                    let rank = read_u32(bytes, &mut at)?;
                    let shards = read_u32(bytes, &mut at)?;
                    let powered = read_u8(bytes, &mut at)?;
                    if powered > 1 {
                        return Err("dev account save has a bad pull flag".into());
                    }
                    if version < 5 {
                        continue;
                    }
                    let item = read_str(bytes, &mut at)?;
                    if item.is_empty() {
                        return Err("dev account save has a pull with no item".into());
                    }
                    pulls.push(Pull {
                        banner,
                        rarity,
                        item,
                        fakeout,
                        rank,
                        shards,
                        powered: powered == 1,
                    });
                }
                let fault = read_fault(bytes, &mut at)?;
                if version < 5 {
                    continue;
                }
                if pulls_by_key
                    .insert(key, StoredPull { banner, ten, seq, pulls, fault })
                    .is_some()
                {
                    return Err("dev account save repeats a pull key".into());
                }
            }
        }
        if at != bytes.len() {
            return Err("dev account save has trailing bytes".into());
        }
        let mut rng = ChaCha20Rng::from_seed(seed);
        rng.set_word_pos((u128::from(pos_hi) << 64) | u128::from(pos_lo));
        let next_pull_seq = pulls_by_key.values().map(|row| row.seq).max().unwrap_or(0).saturating_add(1);
        let mut account = Self {
            rank,
            shards,
            credits,
            pity,
            active_banner,
            rng,
            audit,
            banks,
            pulls_by_key,
            next_pull_seq,
        };
        account.prune(content);
        Ok(account)
    }

    pub fn pull(&mut self, content: &Content, banner: &str) -> Result<Pull, PullError> {
        if banner.is_empty() {
            return Err(PullError::Banner);
        }
        let cost = content.banners.cost.single as f32;
        self.charge(cost)?;
        match self.roll(content, banner, cost) {
            Ok(pull) => Ok(pull),
            Err(err) => {
                self.credits += cost;
                Err(err)
            }
        }
    }

    /// One server pull under an idempotency key. A replay returns the stored pods.
    pub fn pull_with_key(&mut self, content: &Content, banner: &str, key: &str) -> Result<PullBatch, PullError> {
        self.commit(content, banner, key, false, None)
    }

    /// Ten pods for the resolved ten-pull price. Pity advances once per pod.
    /// The charge, pity, and inventory land in one step, or none of them do.
    pub fn pull_ten(&mut self, content: &Content, banner: &str, key: &str) -> Result<PullBatch, PullError> {
        self.commit(content, banner, key, true, None)
    }

    /// Fail the numbered pod (1-based) after the earlier pods have rolled.
    /// The caller sees the account exactly as it was before the request.
    #[cfg(test)]
    fn pull_ten_failing_pod(&mut self, content: &Content, banner: &str, key: &str, pod: u32) -> Result<PullBatch, PullError> {
        self.commit(content, banner, key, true, Some(pod))
    }

    fn commit(
        &mut self,
        content: &Content,
        banner: &str,
        key: &str,
        ten: bool,
        fail_at: Option<u32>,
    ) -> Result<PullBatch, PullError> {
        if key.is_empty() {
            return Err(PullError::Key);
        }
        if banner.is_empty() {
            return Err(PullError::Banner);
        }
        if let Some(stored) = self.pulls_by_key.get(key) {
            if stored.banner != banner || stored.ten != ten {
                return Err(PullError::Mismatch);
            }
            return stored.replay();
        }
        let snap = self.snap();
        let result = if ten { self.run_ten(content, banner, fail_at) } else { self.pull(content, banner).map(|pull| vec![pull]) };
        match result {
            Ok(pulls) => {
                self.remember(content, key, banner, ten, pulls.clone(), None);
                Ok(PullBatch { pulls, fresh: true })
            }
            Err(err @ PullError::Short { .. }) => {
                self.restore(snap);
                Err(err)
            }
            Err(err) => {
                self.restore(snap);
                self.remember(content, key, banner, ten, Vec::new(), Some(err.clone()));
                Err(err)
            }
        }
    }

    fn remember(&mut self, content: &Content, key: &str, banner: &str, ten: bool, pulls: Vec<Pull>, fault: Option<PullError>) {
        let seq = self.next_pull_seq;
        self.next_pull_seq = self.next_pull_seq.saturating_add(1);
        self.pulls_by_key.insert(
            key.to_string(),
            StoredPull { banner: banner.to_string(), ten, seq, pulls, fault },
        );
        self.prune(content);
    }

    fn prune(&mut self, content: &Content) {
        let max = content.banners.saved.max_keys as usize;
        while self.pulls_by_key.len() > max {
            let oldest = self
                .pulls_by_key
                .iter()
                .min_by(|left, right| left.1.seq.cmp(&right.1.seq).then_with(|| left.0.cmp(right.0)))
                .map(|(key, _)| key.clone());
            let Some(key) = oldest else { break };
            self.pulls_by_key.remove(&key);
        }
    }

    fn run_ten(&mut self, content: &Content, banner: &str, fail_at: Option<u32>) -> Result<Vec<Pull>, PullError> {
        let pods = content.banners.cost.pods;
        let cost = content.banners.cost.ten_pull_cost as f32;
        self.charge(cost)?;
        let mut pulls = Vec::with_capacity(pods as usize);
        for index in 0..pods {
            let pod = index + 1;
            if fail_at == Some(pod) {
                return Err(PullError::Pod { pod });
            }
            let spent = if index == 0 { cost } else { 0.0 };
            pulls.push(self.roll(content, banner, spent)?);
        }
        Ok(pulls)
    }

    fn snap(&self) -> AccountSnap {
        AccountSnap {
            rank: self.rank,
            shards: self.shards,
            credits: self.credits,
            pity: self.pity.clone(),
            active_banner: self.active_banner.clone(),
            seed: self.rng.get_seed(),
            word_pos: self.rng.get_word_pos(),
            audit: self.audit.clone(),
            banks: self.banks.clone(),
        }
    }

    fn restore(&mut self, snap: AccountSnap) {
        self.rank = snap.rank;
        self.shards = snap.shards;
        self.credits = snap.credits;
        self.pity = snap.pity;
        self.active_banner = snap.active_banner;
        let mut rng = ChaCha20Rng::from_seed(snap.seed);
        rng.set_word_pos(snap.word_pos);
        self.rng = rng;
        self.audit = snap.audit;
        self.banks = snap.banks;
    }

    fn charge(&mut self, cost: f32) -> Result<(), PullError> {
        if self.credits + 1e-3 < cost {
            return Err(PullError::Short { cost, have: self.credits });
        }
        self.credits -= cost;
        Ok(())
    }

    fn roll(&mut self, content: &Content, banner: &str, spent: f32) -> Result<Pull, PullError> {
        let max_rank = content.duplicate.max_rank;
        let powered = self.rank < max_rank;
        if !powered {
            for tier in TIERS {
                if content.cosmetics.shards_for(tier).is_err() {
                    return Err(PullError::Shards);
                }
            }
        }
        let mut rng = self.rng.clone();
        let mut pity = self.pity_for(content, banner);
        let pity_legendary_before = pity.since_legendary;
        let pity_epic_before = pity.since_epic;
        let rarity = roll_rarity(&mut pity, &mut rng, content);
        let fakeout = roll_fakeout(&mut rng, content, &rarity);
        let item = pick_item(content, &rarity, &mut rng)?;
        let shards = if powered {
            0
        } else {
            match content.cosmetics.shards_for(&rarity) {
                Ok(shards) => shards,
                Err(_) => return Err(PullError::Shards),
            }
        };
        self.rng = rng;
        self.active_banner = Some(banner.to_string());
        self.pity.insert(banner.to_string(), pity.clone());
        if powered {
            self.rank += 1;
        }
        self.shards += shards;
        let pull = Pull {
            banner: banner.to_string(),
            rarity,
            item,
            fakeout,
            rank: self.rank,
            shards,
            powered,
        };
        self.audit.push(AuditEntry {
            banner: pull.banner.clone(),
            rarity: pull.rarity.clone(),
            fakeout: pull.fakeout.clone(),
            rank: pull.rank,
            shards: pull.shards,
            powered: pull.powered,
            credits_spent: spent,
            since_legendary: pity.since_legendary,
            since_epic: pity.since_epic,
            pity_legendary_before,
            pity_epic_before,
        });
        Ok(pull)
    }

    /// Starting pity for this pull. Carried counters are copied here and written back only if the roll lands.
    fn pity_for(&self, content: &Content, banner: &str) -> Pity {
        if content.banners.pity.carries_between_banners {
            if let Some(prev) = &self.active_banner {
                if prev != banner {
                    return self.pity.get(prev).cloned().unwrap_or_default();
                }
            }
        }
        self.pity.get(banner).cloned().unwrap_or_default()
    }
}

/// One catalog row of `rarity`. The index comes from this rng, after the tier roll.
fn pick_item(content: &Content, rarity: &str, rng: &mut ChaCha20Rng) -> Result<String, PullError> {
    let pool: Vec<&str> = content
        .banners
        .items
        .iter()
        .filter(|item| item.rarity == rarity)
        .map(|item| item.id.as_str())
        .collect();
    if pool.is_empty() {
        return Err(PullError::Item);
    }
    let index = ((next_f32(rng) * pool.len() as f32) as usize).min(pool.len() - 1);
    Ok(pool[index].to_string())
}

fn roll_rarity(pity: &mut Pity, rng: &mut ChaCha20Rng, content: &Content) -> String {
    let rules = &content.banners.pity;
    let rates = &content.banners.rates;
    pity.since_legendary += 1;
    pity.since_epic += 1;
    if pity.since_legendary >= rules.legendary_hard {
        pity.since_legendary = 0;
        pity.since_epic = 0;
        return "legendary".to_string();
    }
    let mut legendary = rates.legendary;
    if pity.since_legendary >= rules.legendary_soft_start {
        let steps = (pity.since_legendary - rules.legendary_soft_start) as f32;
        legendary = (legendary + steps * rules.legendary_soft_step).min(1.0);
    }
    let roll = next_f32(rng);
    let mut rarity = if roll < legendary {
        "legendary"
    } else if roll < legendary + rates.epic {
        "epic"
    } else if roll < legendary + rates.epic + rates.rare {
        "rare"
    } else {
        "common"
    };
    if pity.since_epic >= rules.epic_or_better_guarantee && (rarity == "common" || rarity == "rare") {
        rarity = "epic";
    }
    if rarity == "legendary" {
        pity.since_legendary = 0;
        pity.since_epic = 0;
    } else if rarity == "epic" {
        pity.since_epic = 0;
    }
    rarity.to_string()
}

/// Fake-out uses only `crate_reveal.toml`. Pity does not change the chance.
fn roll_fakeout(rng: &mut ChaCha20Rng, content: &Content, rarity: &str) -> Option<String> {
    let reveal = &content.crate_reveal;
    if !reveal.fakeout_enabled {
        return None;
    }
    let above = tier_above(rarity)?;
    let tier = reveal.tier(rarity)?;
    if !tier.fakeout_allowed || tier.fakeout_chance <= 0.0 {
        return None;
    }
    let window = tier.fakeout_window;
    if window[1] <= window[0] {
        return None;
    }
    if next_f32(rng) < tier.fakeout_chance {
        Some(above.to_string())
    } else {
        None
    }
}

/// [0, 1) from 24 bits of ChaCha20 output.
fn next_f32(rng: &mut ChaCha20Rng) -> f32 {
    (rng.next_u32() >> 8) as f32 / 16_777_216.0
}

/// `debug_assertions` is on for dev builds and off for release. There is no feature flag.
fn dev_currency_enabled() -> bool {
    cfg!(debug_assertions)
}

const SAVE_V1: &[u8] = b"GEMACCT1";
const SAVE_V2: &[u8] = b"GEMACCT2";
const SAVE_V3: &[u8] = b"GEMACCT3";
const SAVE_V4: &[u8] = b"GEMACCT4";
const SAVE_V5: &[u8] = b"GEMACCT5";

struct AccountSnap {
    rank: u32,
    shards: u32,
    credits: f32,
    pity: BTreeMap<String, Pity>,
    active_banner: Option<String>,
    seed: [u8; 32],
    word_pos: u128,
    audit: Vec<AuditEntry>,
    banks: BTreeMap<(u64, u64), Banked>,
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}

fn write_u64(out: &mut Vec<u8>, value: u64) {
    out.extend(value.to_le_bytes());
}

fn write_f32(out: &mut Vec<u8>, value: f32) -> Result<(), String> {
    if !value.is_finite() {
        return Err("dev account save refuses a non-finite credit value".into());
    }
    out.extend(value.to_le_bytes());
    Ok(())
}

fn write_str(out: &mut Vec<u8>, value: &str) {
    write_u32(out, value.len() as u32);
    out.extend(value.as_bytes());
}

fn write_opt_str(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(text) => {
            out.push(1);
            write_str(out, text);
        }
        None => out.push(0),
    }
}

fn read_exact<'a>(bytes: &'a [u8], at: &mut usize, len: usize) -> Result<&'a [u8], String> {
    let end = at.checked_add(len).ok_or("dev account save is truncated")?;
    if end > bytes.len() {
        return Err("dev account save is truncated".into());
    }
    let slice = &bytes[*at..end];
    *at = end;
    Ok(slice)
}

fn read_u32(bytes: &[u8], at: &mut usize) -> Result<u32, String> {
    let slice = read_exact(bytes, at, 4)?;
    Ok(u32::from_le_bytes(slice.try_into().unwrap()))
}

fn read_u64(bytes: &[u8], at: &mut usize) -> Result<u64, String> {
    let slice = read_exact(bytes, at, 8)?;
    Ok(u64::from_le_bytes(slice.try_into().unwrap()))
}

fn read_u8(bytes: &[u8], at: &mut usize) -> Result<u8, String> {
    Ok(read_exact(bytes, at, 1)?[0])
}

fn read_f32(bytes: &[u8], at: &mut usize) -> Result<f32, String> {
    let slice = read_exact(bytes, at, 4)?;
    let value = f32::from_le_bytes(slice.try_into().unwrap());
    if !value.is_finite() {
        return Err("dev account save has a non-finite credit value".into());
    }
    Ok(value)
}

fn read_str(bytes: &[u8], at: &mut usize) -> Result<String, String> {
    let len = read_u32(bytes, at)? as usize;
    if len > bytes.len() {
        return Err("dev account save is truncated".into());
    }
    let slice = read_exact(bytes, at, len)?;
    String::from_utf8(slice.to_vec()).map_err(|_| "dev account save has a non-utf8 string".into())
}

fn read_opt_str(bytes: &[u8], at: &mut usize) -> Result<Option<String>, String> {
    match read_u8(bytes, at)? {
        0 => Ok(None),
        1 => Ok(Some(read_str(bytes, at)?)),
        _ => Err("dev account save has a bad optional string".into()),
    }
}

fn write_fault(out: &mut Vec<u8>, fault: &Option<PullError>) -> Result<(), String> {
    match fault {
        None => {
            out.push(0);
            write_f32(out, 0.0)?;
            write_f32(out, 0.0)?;
            write_u32(out, 0);
        }
        Some(PullError::Short { cost, have }) => {
            out.push(1);
            write_f32(out, *cost)?;
            write_f32(out, *have)?;
            write_u32(out, 0);
        }
        Some(PullError::Banner) => {
            out.push(2);
            write_f32(out, 0.0)?;
            write_f32(out, 0.0)?;
            write_u32(out, 0);
        }
        Some(PullError::Shards) => {
            out.push(3);
            write_f32(out, 0.0)?;
            write_f32(out, 0.0)?;
            write_u32(out, 0);
        }
        Some(PullError::Pod { pod }) => {
            out.push(4);
            write_f32(out, 0.0)?;
            write_f32(out, 0.0)?;
            write_u32(out, *pod);
        }
        Some(PullError::Key) => {
            out.push(5);
            write_f32(out, 0.0)?;
            write_f32(out, 0.0)?;
            write_u32(out, 0);
        }
        Some(PullError::Mismatch) => {
            out.push(6);
            write_f32(out, 0.0)?;
            write_f32(out, 0.0)?;
            write_u32(out, 0);
        }
        Some(PullError::Item) => {
            out.push(7);
            write_f32(out, 0.0)?;
            write_f32(out, 0.0)?;
            write_u32(out, 0);
        }
    }
    Ok(())
}

fn read_fault(bytes: &[u8], at: &mut usize) -> Result<Option<PullError>, String> {
    let tag = read_u8(bytes, at)?;
    let cost = read_f32(bytes, at)?;
    let have = read_f32(bytes, at)?;
    let pod = read_u32(bytes, at)?;
    match tag {
        0 => Ok(None),
        1 => Ok(Some(PullError::Short { cost, have })),
        2 => Ok(Some(PullError::Banner)),
        3 => Ok(Some(PullError::Shards)),
        4 => Ok(Some(PullError::Pod { pod })),
        5 => Ok(Some(PullError::Key)),
        6 => Ok(Some(PullError::Mismatch)),
        7 => Ok(Some(PullError::Item)),
        other => Err(format!("dev account save has a bad pull fault {other}")),
    }
}

fn tier_above(rarity: &str) -> Option<&'static str> {
    let idx = TIERS.iter().position(|tier| *tier == rarity)?;
    TIERS.get(idx + 1).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn funded(seed: u64, content: &Content) -> Account {
        let mut account = Account::from_seed(seed);
        account.credits = content.banners.cost.single as f32 * 20.0;
        account
    }

    #[test]
    fn pulls_raise_rank_until_max_then_pay_shards() {
        let content = Content::embedded();
        let mut account = funded(9, &content);
        account.credits = content.banners.cost.single as f32;
        let pull = account.pull(&content, "standard").expect("one pull");
        assert!(pull.powered);
        assert_eq!(pull.rank, 2);
        assert!(pull.fakeout.is_none() || tier_above(&pull.rarity) == pull.fakeout.as_deref());
        assert_eq!(account.audit.len(), 1);
        let err = account.pull(&content, "standard").expect_err("a pull needs credits");
        assert!(err.to_string().contains("credits"), "{err}");

        account.credits = content.banners.cost.ten_pull_cost as f32;
        let ten = account.pull_ten(&content, "standard", "rank-ten").unwrap();
        assert_eq!(ten.pulls.len(), content.banners.cost.pods as usize);
        assert!(
            ten.pulls.iter().any(|pull| pull.rarity == "epic" || pull.rarity == "legendary"),
            "a ten-pull includes epic or better"
        );

        account.credits = content.banners.cost.single as f32 * 30.0;
        while account.rank < content.duplicate.max_rank {
            let again = account.pull(&content, "standard").unwrap();
            assert!(again.powered);
        }
        let shards_before = account.shards;
        let rank_before = account.rank;
        let past = account.pull(&content, "standard").unwrap();
        assert!(!past.powered);
        assert_eq!(past.rank, rank_before);
        assert_eq!(account.rank, content.duplicate.max_rank);
        assert!(past.shards > 0);
        assert!(account.shards > shards_before);
        assert_eq!(past.shards, content.cosmetics.shards_for(&past.rarity).unwrap());
    }

    #[test]
    fn pity_carries_onto_the_next_banner() {
        let content = Content::embedded();
        assert!(content.banners.pity.carries_between_banners);
        let mut account = funded(3, &content);
        account.credits = content.banners.cost.single as f32 * 4.0;
        account.pity.insert(
            "standard".into(),
            Pity {
                since_legendary: content.banners.pity.legendary_hard - 1,
                since_epic: 0,
            },
        );
        account.active_banner = Some("standard".into());
        let carried = account.pull(&content, "featured").unwrap();
        assert_eq!(carried.rarity, "legendary");
        assert_eq!(carried.banner, "featured");
        assert!(carried.fakeout.is_none(), "a legendary never teases a higher tier");
    }

    #[test]
    fn seeded_million_pulls_match_published_odds() {
        let content = Content::embedded();
        let published = content.banners.published.legendary_overall_rate;
        let hard = content.banners.pity.legendary_hard;
        let n = 1_000_000u32;
        let mut account = Account::from_seed(0x5EED_F00D);
        account.credits = content.banners.cost.single as f32;
        let banners = ["standard", "featured"];
        let mut legendaries = 0u32;
        let mut streak = 0u32;
        let mut max_streak = 0u32;
        let mut epic_plus = vec![false; n as usize];
        let mut rare_low = [0u32, 0];
        let mut rare_high = [0u32, 0];
        let mut epic_low = [0u32, 0];
        let mut epic_high = [0u32, 0];
        let soft = content.banners.pity.legendary_soft_start;
        for i in 0..n {
            account.credits = content.banners.cost.single as f32;
            let banner = banners[(i % 2) as usize];
            let pull = account.pull(&content, banner).unwrap();
            let before = account.audit.last().unwrap().pity_legendary_before;
            let high = before >= soft;
            if pull.rarity == "legendary" {
                legendaries += 1;
                max_streak = max_streak.max(streak);
                streak = 0;
            } else {
                streak += 1;
                assert!(streak <= hard, "streak {streak} without a legendary");
            }
            epic_plus[i as usize] = pull.rarity == "epic" || pull.rarity == "legendary";
            let bumped = pull.fakeout.is_some();
            match pull.rarity.as_str() {
                "common" | "legendary" => assert!(!bumped, "{} fakeout {:?}", pull.rarity, pull.fakeout),
                "rare" => {
                    if high {
                        rare_high[1] += 1;
                        if bumped {
                            rare_high[0] += 1;
                        }
                    } else {
                        rare_low[1] += 1;
                        if bumped {
                            rare_low[0] += 1;
                        }
                    }
                    if let Some(tease) = &pull.fakeout {
                        assert_eq!(tease, "epic");
                    }
                }
                "epic" => {
                    if high {
                        epic_high[1] += 1;
                        if bumped {
                            epic_high[0] += 1;
                        }
                    } else {
                        epic_low[1] += 1;
                        if bumped {
                            epic_low[0] += 1;
                        }
                    }
                    if let Some(tease) = &pull.fakeout {
                        assert_eq!(tease, "legendary");
                    }
                }
                other => panic!("unknown rarity {other}"),
            }
        }
        max_streak = max_streak.max(streak);
        assert!(max_streak <= hard, "max streak {max_streak}");
        let rate = legendaries as f64 / n as f64;
        let delta = (rate - published as f64).abs();
        assert!(
            delta <= 0.001,
            "legendary rate {rate:.5} is {delta:.5} away from published {published}"
        );
        let mut window = 0u32;
        for i in 0..n as usize {
            if epic_plus[i] {
                window += 1;
            }
            if i >= 10 && epic_plus[i - 10] {
                window -= 1;
            }
            if i >= 9 {
                assert!(window > 0, "pulls {}..={i} include no epic or legendary", i - 9);
            }
        }
        let rare_chance = content.crate_reveal.tier("rare").unwrap().fakeout_chance;
        let epic_chance = content.crate_reveal.tier("epic").unwrap().fakeout_chance;
        assert_close_rate("rare low pity", rare_low, rare_chance);
        assert_close_rate("rare high pity", rare_high, rare_chance);
        assert_close_rate("epic low pity", epic_low, epic_chance);
        assert_close_rate("epic high pity", epic_high, epic_chance);
        assert!(
            (rate_of(rare_low) - rate_of(rare_high)).abs() < 0.015,
            "rare fakeout rate moved with pity"
        );
        assert!(
            (rate_of(epic_low) - rate_of(epic_high)).abs() < 0.015,
            "epic fakeout rate moved with pity"
        );

        let off = content_with_fakeouts_disabled();
        let mut quiet = Account::from_seed(0x5EED_F00D);
        for i in 0..100_000 {
            quiet.credits = off.banners.cost.single as f32;
            let pull = quiet.pull(&off, banners[(i % 2) as usize]).unwrap();
            assert!(pull.fakeout.is_none(), "fakeout while disabled: {pull:?}");
        }
    }

    fn rate_of(hits_and_n: [u32; 2]) -> f64 {
        hits_and_n[0] as f64 / hits_and_n[1] as f64
    }

    fn assert_close_rate(label: &str, hits_and_n: [u32; 2], chance: f32) {
        assert!(hits_and_n[1] > 1000, "{label} sample {}", hits_and_n[1]);
        let rate = rate_of(hits_and_n);
        assert!(
            (rate - chance as f64).abs() < 0.015,
            "{label} fakeout rate {rate:.4} vs chance {chance}"
        );
    }

    fn content_with_fakeouts_disabled() -> Content {
        let base = gemini_sim::content::embedded_sources();
        let reveal = base.crate_reveal.replacen("fakeout_enabled = true", "fakeout_enabled = false", 1);
        assert_ne!(reveal, base.crate_reveal, "fakeout_enabled was not rewritten");
        Content::parse(&gemini_sim::content::Sources {
            waves: base.waves,
            difficulty: base.difficulty,
            controller: base.controller,
            run_upgrades: base.run_upgrades,
            endless: base.endless,
            currency: base.currency,
            banners: base.banners,
            duplicate_ranks: base.duplicate_ranks,
            bosses: base.bosses,
            weapons: base.weapons,
            hero: base.hero,
            arena: base.arena,
            presentation: base.presentation,
            combat: base.combat,
            palette: base.palette,
            music: base.music,
            audio: base.audio,
            bugs: base.bugs,
            crate_reveal: &reveal,
            coop: base.coop,
            cosmetics: base.cosmetics,
            ui: base.ui,
            heroes_visual: base.heroes_visual,
            arena_visual: base.arena_visual,
        })
        .expect("content with fakeouts off")
    }

    #[test]
    fn default_accounts_draw_different_pull_sequences() {
        let content = Content::embedded();
        let mut first = Account::new();
        let mut second = Account::new();
        let mut left = Vec::new();
        let mut right = Vec::new();
        for _ in 0..64 {
            let cost = content.banners.cost.single as f32;
            first.credits = cost;
            second.credits = cost;
            let a = first.pull(&content, "standard").unwrap();
            let b = second.pull(&content, "standard").unwrap();
            left.push((a.rarity, a.fakeout));
            right.push((b.rarity, b.fakeout));
        }
        assert_ne!(left, right, "two os-seeded accounts drew the same 64 pulls");
    }

    #[test]
    fn sim_and_client_do_not_name_backend() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for rel in ["../sim/Cargo.toml", "../client/Cargo.toml"] {
            let path = root.join(rel);
            let text = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
            assert!(!text.contains("gemini-backend"), "{rel} depends on gemini-backend");
            assert!(!text.contains("crates/backend") && !text.contains("../backend"), "{rel} paths at backend");
        }
    }

    #[test]
    fn a_missing_shard_cost_does_not_move_pity_or_the_rng() {
        let mut content = Content::embedded();
        content.cosmetics.forget_shard_row("legendary").unwrap();
        let mut account = Account::from_seed(7);
        account.rank = content.duplicate.max_rank;
        account.credits = content.banners.cost.single as f32 * 3.0;
        account.pity.insert("featured".into(), Pity { since_legendary: 40, since_epic: 9 });
        account.active_banner = Some("featured".into());
        let before = account.to_bytes(&content).unwrap();
        let err = account.pull(&content, "standard").unwrap_err();
        assert!(matches!(err, PullError::Shards), "{err}");
        assert_eq!(account.to_bytes(&content).unwrap(), before, "a shard fault must not save a new pity or rng state");
        assert_eq!(account.since_legendary("featured"), 40);
        assert_eq!(account.since_legendary("standard"), 0, "pity must not carry onto the banner");
        assert!(account.audit.is_empty());
        assert_eq!(account.credits(), content.banners.cost.single as f32 * 3.0);
    }

    #[test]
    fn a_save_roundtrip_keeps_pity_and_garbage_fails() {
        let content = Content::embedded();
        let mut account = Account::from_seed(4);
        account.credits = content.banners.dev.starting_credits;
        let first = account.pull(&content, "standard").unwrap();
        assert!(TIERS.contains(&first.rarity.as_str()), "{}", first.rarity);
        let bytes = account.to_bytes(&content).unwrap();
        let mut loaded = Account::from_bytes(&bytes, &content).unwrap();
        assert_eq!(loaded.credits(), account.credits());
        assert_eq!(loaded.since_legendary("standard"), account.since_legendary("standard"));
        assert_eq!(loaded.audit.len(), account.audit.len());
        assert_eq!(loaded.rank, account.rank);
        let again = account.pull(&content, "standard").unwrap();
        let restored = loaded.pull(&content, "standard").unwrap();
        assert_eq!(restored.rarity, again.rarity);
        assert_eq!(restored.fakeout, again.fakeout);
        assert!(Account::from_bytes(b"garbage", &content).is_err());
        assert!(Account::from_bytes(&bytes[..bytes.len() - 1], &content).is_err());
        let mut extra = bytes.clone();
        extra.push(9);
        assert!(Account::from_bytes(&extra, &content).is_err());
    }

    #[test]
    fn release_accounts_start_empty_and_the_dev_grant_is_content() {
        let content = Content::embedded();
        let banner = content.banners.id.as_str();
        let empty = Account::open_with_dev(&content, false);
        assert_eq!(empty.credits(), 0.0, "a release account must not receive test currency");
        assert_eq!(empty.since_legendary(banner), 0);
        let mut broke = Account::open_with_dev(&content, false);
        let err = broke.pull(&content, banner).expect_err("an empty wallet cannot pull");
        assert!(matches!(err, PullError::Short { have, .. } if have == 0.0), "{err}");

        let mut funded = Account::open_with_dev(&content, true);
        assert_eq!(funded.credits(), content.banners.dev.starting_credits);
        assert_eq!(funded.since_legendary(banner), 0);
        let pull = funded.pull(&content, banner).expect("the dev wallet can pay");
        assert!(TIERS.contains(&pull.rarity.as_str()), "{}", pull.rarity);
        let above = tier_above(&pull.rarity);
        assert!(pull.fakeout.is_none() || pull.fakeout.as_deref() == above);
        let pity = funded.since_legendary(banner);
        funded.grant_dev_credits(&content);
        assert_eq!(funded.since_legendary(banner), pity, "the grant must not reset pity");
        assert_eq!(funded.credits(), content.banners.dev.starting_credits);

        let mut via_grant = Account::from_seed(11);
        via_grant.grant_dev_credits(&content);
        let mut via_wallet = Account::from_seed(11);
        via_wallet.credits = content.banners.dev.starting_credits;
        let granted = via_grant.pull(&content, banner).unwrap();
        let paid = via_wallet.pull(&content, banner).unwrap();
        assert_eq!(granted.rarity, paid.rarity, "the grant must not force a tier");
        assert_eq!(granted.fakeout, paid.fakeout);

        let opened = Account::open(&content);
        let expected = if dev_currency_enabled() {
            content.banners.dev.starting_credits
        } else {
            0.0
        };
        assert_eq!(opened.credits(), expected);
        assert_eq!(dev_currency_enabled(), cfg!(debug_assertions));
    }

    fn noted(sim: &gemini_sim::Sim) -> (MatchBook, u64) {
        let mut book = MatchBook::new();
        let id = book.open();
        book.note(id, sim).unwrap();
        (book, id)
    }

    #[test]
    fn a_match_is_banked_once_and_a_rejoin_does_not_pay_again() {
        let content = gemini_sim::Content::embedded();
        let mut sim = gemini_sim::Sim::new(&content, "normal", 1);
        let player = sim.players[0].token;
        let (book, id) = noted(&sim);
        let mut account = Account::from_seed(4);
        let pity_rows = account.pity.len();
        let audit_rows = account.audit.len();
        let seed = account.rng.get_seed();
        let pos = account.rng.get_word_pos();
        account.grant_dev_credits(&content);
        let start = account.credits();
        let first = account.bank(book.record(id).unwrap(), player).unwrap();
        assert!(first.fresh);
        assert_eq!(first.earned, 0.0, "an uncleared wave deposits nothing");
        assert_eq!(first.bonus, 0.0);
        assert_eq!(account.credits(), start);
        assert_eq!(account.pity.len(), pity_rows);
        assert_eq!(account.audit.len(), audit_rows);
        assert_eq!(account.rng.get_seed(), seed);
        assert_eq!(account.rng.get_word_pos(), pos);

        sim.enemies.clear();
        sim.pending_vents.clear();
        sim.director.pool = 0.0;
        sim.director.queue = 0.0;
        sim.phase = gemini_sim::Phase::Combat;
        sim.step(&content, gemini_sim::PlayerInput::default());
        let mut later = book.clone();
        later.note(id, &sim).unwrap();
        assert!(sim.currency_gain[&player] > first.earned, "a cleared wave is on the record and must not be deposited again");
        let second = account.bank(later.record(id).unwrap(), player).unwrap();
        assert!(!second.fresh);
        assert_eq!(second.earned, first.earned);
        assert_eq!(second.bonus, first.bonus);
        assert_eq!(account.credits(), start + first.earned);
        assert_eq!(account.pity.len(), pity_rows);
        assert_eq!(account.rng.get_seed(), seed);
        assert_eq!(account.rng.get_word_pos(), pos);
    }

    #[test]
    fn a_quit_on_an_open_wave_banks_zero_and_a_win_adds_the_content_bonus() {
        let content = gemini_sim::Content::embedded();
        let tier = content.tier("normal").currency;
        let sim = gemini_sim::Sim::new(&content, "normal", 1);
        let player = sim.players[0].token;
        let (book, id) = noted(&sim);
        let mut account = Account::from_seed(5);
        let partial = account.bank(book.record(id).unwrap(), player).unwrap();
        assert_eq!(partial.earned, 0.0);
        assert_eq!(partial.bonus, 0.0);
        assert_eq!(account.credits(), partial.earned);

        let mut won = sim.clone();
        won.grant_victory_bonus(&content);
        let (won_book, won_id) = noted(&won);
        let mut wallet = Account::from_seed(6);
        let receipt = wallet.bank(won_book.record(won_id).unwrap(), player).unwrap();
        let bonus = gemini_sim::content::win_payout(&content.currency, tier);
        assert_eq!(receipt.bonus, bonus);
        assert_eq!(receipt.earned, bonus);
        assert_eq!(wallet.credits(), receipt.earned);
    }

    #[test]
    fn a_bad_amount_is_rejected_and_a_v1_save_loads_with_an_empty_ledger() {
        let content = gemini_sim::Content::embedded();
        let mut sim = gemini_sim::Sim::new(&content, "normal", 1);
        let player = sim.players[0].token;
        sim.currency_gain.insert(player, -1.0);
        let (book, id) = noted(&sim);
        let mut account = Account::from_seed(7);
        account.grant_dev_credits(&content);
        let start = account.credits();
        assert_eq!(account.bank(book.record(id).unwrap(), player).unwrap_err(), BankError::Amount);
        assert_eq!(account.credits(), start, "a rejected deposit does not change the dev grant");
        assert_eq!(account.bank(book.record(id).unwrap(), player.wrapping_add(1)).unwrap_err(), BankError::Player);

        let empty = Account::from_seed(8);
        let v4 = empty.to_bytes(&content).unwrap();
        assert!(v4.starts_with(SAVE_V5));
        let mut v2 = v4[..v4.len() - 4].to_vec();
        v2[..SAVE_V2.len()].copy_from_slice(SAVE_V2);
        let loaded_v2 = Account::from_bytes(&v2, &content).unwrap();
        assert!(loaded_v2.banks.is_empty());
        assert!(loaded_v2.pulls_by_key.is_empty());
        let mut v1 = v2[..v2.len() - 4].to_vec();
        v1[..SAVE_V1.len()].copy_from_slice(SAVE_V1);
        let loaded = Account::from_bytes(&v1, &content).unwrap();
        assert!(loaded.banks.is_empty());
        assert!(loaded.pulls_by_key.is_empty());
        assert_eq!(loaded.credits(), empty.credits());
        let round = Account::from_bytes(&account.to_bytes(&content).unwrap(), &content).unwrap();
        assert_eq!(round.credits(), account.credits());
        assert!(round.banks.is_empty(), "the rejected amount was not stored");
    }

    #[test]
    fn a_saved_deposit_round_trips_and_stays_idempotent() {
        let content = gemini_sim::Content::embedded();
        let sim = gemini_sim::Sim::new(&content, "normal", 1);
        let player = sim.players[0].token;
        let (book, id) = noted(&sim);
        let mut account = Account::from_seed(9);
        let first = account.bank(book.record(id).unwrap(), player).unwrap();
        let loaded = Account::from_bytes(&account.to_bytes(&content).unwrap(), &content).unwrap();
        assert_eq!(loaded.credits(), first.balance);
        let mut again = loaded;
        let second = again.bank(book.record(id).unwrap(), player).unwrap();
        assert!(!second.fresh);
        assert_eq!(second.earned, first.earned);
        assert_eq!(again.credits(), first.balance);
    }

    #[test]
    fn the_wallet_and_the_match_key_are_saved_together() {
        let content = gemini_sim::Content::embedded();
        let mut sim = gemini_sim::Sim::new(&content, "normal", 1);
        let player = sim.players[0].token;
        sim.enemies.clear();
        sim.pending_vents.clear();
        sim.director.pool = 0.0;
        sim.director.queue = 0.0;
        sim.phase = gemini_sim::Phase::Combat;
        sim.step(&content, gemini_sim::PlayerInput::default());
        let earned = sim.currency_gain[&player];
        assert!(earned > 0.0, "a cleared wave is a real deposit");
        let (book, id) = noted(&sim);
        let mut account = Account::from_seed(12);
        let start = account.credits();
        let receipt = account.bank(book.record(id).unwrap(), player).unwrap();
        assert!(receipt.fresh);
        assert_eq!(receipt.earned, earned);
        assert_eq!(account.credits(), start + earned);
        assert_eq!(account.banks[&(id, player)].earned, earned);
        assert_eq!(account.banks[&(id, player)].bonus, receipt.bonus);
        let bytes = account.to_bytes(&content).unwrap();
        let loaded = Account::from_bytes(&bytes, &content).unwrap();
        assert_eq!(loaded.credits(), start + earned);
        assert_eq!(loaded.banks[&(id, player)].earned, earned);
        assert_eq!(loaded.banks[&(id, player)].bonus, receipt.bonus);
        let record_bytes = std::mem::size_of::<u64>() * 2 + std::mem::size_of::<f32>() * 2;
        assert!(Account::from_bytes(&bytes[..bytes.len() - record_bytes], &content).is_err(), "a cut save is not a balance without its key");
        let mut again = loaded;
        let second = again.bank(book.record(id).unwrap(), player).unwrap();
        assert!(!second.fresh);
        assert_eq!(second.earned, earned);
        assert_eq!(again.credits(), start + earned);
    }

    #[test]
    fn a_failed_pod_rolls_the_ten_pull_back() {
        let content = Content::embedded();
        let mut account = funded(4, &content);
        let banner = "standard";
        account.credits = content.banners.cost.ten_pull_cost as f32 * 2.0;
        let credits = account.credits;
        let rank = account.rank;
        let shards = account.shards;
        let pity = account.pity.clone();
        let audit = account.audit.len();
        let seed = account.rng.get_seed();
        let pos = account.rng.get_word_pos();
        let active = account.active_banner.clone();
        assert!(content.banners.cost.pods > 7, "pod 7 has to sit inside the ten-pull");
        let err = account.pull_ten_failing_pod(&content, banner, "pod-7", 7).unwrap_err();
        assert_eq!(err, PullError::Pod { pod: 7 });
        assert_eq!(account.credits, credits);
        assert_eq!(account.rank, rank);
        assert_eq!(account.shards, shards);
        assert_eq!(account.pity, pity);
        assert_eq!(account.audit.len(), audit);
        assert_eq!(account.rng.get_seed(), seed);
        assert_eq!(account.rng.get_word_pos(), pos);
        assert_eq!(account.active_banner, active);
        let again = account.pull_ten(&content, banner, "pod-7").unwrap_err();
        assert_eq!(again, PullError::Pod { pod: 7 });
        assert_eq!(account.credits, credits);
        let mut loaded = Account::from_bytes(&account.to_bytes(&content).unwrap(), &content).unwrap();
        assert_eq!(loaded.pull_ten(&content, banner, "pod-7").unwrap_err(), PullError::Pod { pod: 7 });
        assert_eq!(loaded.credits, credits);
        assert_eq!(loaded.rng.get_seed(), seed);
        assert_eq!(loaded.rng.get_word_pos(), pos);
    }

    #[test]
    fn a_replayed_ten_pull_charges_once() {
        let content = Content::embedded();
        let mut account = funded(5, &content);
        let price = content.banners.cost.ten_pull_cost as f32;
        account.credits = price * 2.0;
        let start = account.credits;
        assert_eq!(account.pull_ten(&content, "standard", "").unwrap_err(), PullError::Key);
        assert_eq!(account.credits, start);
        let first = account.pull_ten(&content, "standard", "a").unwrap();
        assert!(first.fresh);
        assert_eq!(first.pulls.len(), content.banners.cost.pods as usize);
        assert!((account.credits - (start - price)).abs() < 1e-3);
        let again = account.pull_ten(&content, "standard", "a").unwrap();
        assert!(!again.fresh);
        for (left, right) in first.pulls.iter().zip(&again.pulls) {
            assert_eq!(left.rarity, right.rarity);
            assert_eq!(left.fakeout, right.fakeout);
            assert_eq!(left.item, right.item);
        }
        assert!((account.credits - (start - price)).abs() < 1e-3);
        let second = account.pull_ten(&content, "standard", "b").unwrap();
        assert!(second.fresh);
        assert!((account.credits - (start - price * 2.0)).abs() < 1e-3);
        let mut loaded = Account::from_bytes(&account.to_bytes(&content).unwrap(), &content).unwrap();
        let replay = loaded.pull_ten(&content, "standard", "a").unwrap();
        assert!(!replay.fresh);
        assert!((loaded.credits - account.credits).abs() < 1e-3);
        assert_eq!(replay.pulls.len(), first.pulls.len());
    }

    #[test]
    fn pity_resets_mid_ten_pull_and_keeps_counting() {
        let content = Content::embedded();
        let hard = content.banners.pity.legendary_hard;
        let pods = content.banners.cost.pods;
        let mid = pods / 2;
        assert!(mid >= 2 && mid < pods, "the threshold has to land inside the ten-pull");
        let price = content.banners.cost.ten_pull_cost as f32;
        let mut account = Account::from_seed(945);
        account.credits = price;
        account.pity.insert(
            "standard".into(),
            Pity {
                since_legendary: hard - mid,
                since_epic: 0,
            },
        );
        let batch = account.pull_ten(&content, "standard", "pity").unwrap();
        let early = batch.pulls.iter().take(mid as usize - 1).any(|pull| pull.rarity == "legendary");
        assert!(!early, "seed 945 keeps hard pity inside the ten-pull");
        let rows = &account.audit;
        assert_eq!(rows.len(), pods as usize);
        let hit = &rows[mid as usize - 1];
        assert_eq!(hit.rarity, "legendary");
        assert_eq!(hit.pity_legendary_before, hard - 1);
        assert_eq!(hit.since_legendary, 0);
        let next = &rows[mid as usize];
        assert_eq!(next.pity_legendary_before, 0);
        if next.rarity == "legendary" {
            assert_eq!(next.since_legendary, 0);
        } else {
            assert_eq!(next.since_legendary, 1);
        }
        let mut counter = hard - mid;
        for row in rows {
            assert_eq!(row.pity_legendary_before, counter);
            counter += 1;
            if counter >= hard || row.rarity == "legendary" {
                assert_eq!(row.since_legendary, 0);
                counter = 0;
            } else {
                assert_eq!(row.since_legendary, counter);
            }
        }
    }

    fn pull_section(bytes: &[u8]) -> usize {
        let mut at = SAVE_V1.len();
        let _ = read_u32(bytes, &mut at).unwrap();
        let _ = read_u32(bytes, &mut at).unwrap();
        let _ = read_f32(bytes, &mut at).unwrap();
        let _ = read_opt_str(bytes, &mut at).unwrap();
        let pity_len = read_u32(bytes, &mut at).unwrap() as usize;
        for _ in 0..pity_len {
            let _ = read_str(bytes, &mut at).unwrap();
            let _ = read_u32(bytes, &mut at).unwrap();
            let _ = read_u32(bytes, &mut at).unwrap();
        }
        let audit_len = read_u32(bytes, &mut at).unwrap() as usize;
        for _ in 0..audit_len {
            let _ = read_str(bytes, &mut at).unwrap();
            let _ = read_str(bytes, &mut at).unwrap();
            let _ = read_opt_str(bytes, &mut at).unwrap();
            let _ = read_u32(bytes, &mut at).unwrap();
            let _ = read_u32(bytes, &mut at).unwrap();
            let _ = read_u8(bytes, &mut at).unwrap();
            let _ = read_f32(bytes, &mut at).unwrap();
            let _ = read_u32(bytes, &mut at).unwrap();
            let _ = read_u32(bytes, &mut at).unwrap();
            let _ = read_u32(bytes, &mut at).unwrap();
            let _ = read_u32(bytes, &mut at).unwrap();
        }
        let _ = read_exact(bytes, &mut at, 32).unwrap();
        let _ = read_u64(bytes, &mut at).unwrap();
        let _ = read_u64(bytes, &mut at).unwrap();
        let bank_len = read_u32(bytes, &mut at).unwrap() as usize;
        for _ in 0..bank_len {
            let _ = read_u64(bytes, &mut at).unwrap();
            let _ = read_u64(bytes, &mut at).unwrap();
            let _ = read_f32(bytes, &mut at).unwrap();
            let _ = read_f32(bytes, &mut at).unwrap();
        }
        at
    }

    /// Rewrite a v5 save's receipts as an older header. v3 and v4 have no item string.
    fn rewrite_pulls(bytes: &[u8], version: u32, blank_item: bool) -> Vec<u8> {
        let at = pull_section(bytes);
        let mut out = bytes[..at].to_vec();
        let header = match version {
            3 => SAVE_V3,
            4 => SAVE_V4,
            5 => SAVE_V5,
            _ => panic!("save version {version}"),
        };
        out[..header.len()].copy_from_slice(header);
        let mut cursor = at;
        let pull_keys = read_u32(bytes, &mut cursor).unwrap();
        write_u32(&mut out, pull_keys);
        for _ in 0..pull_keys {
            let key = read_str(bytes, &mut cursor).unwrap();
            write_str(&mut out, &key);
            let banner = read_str(bytes, &mut cursor).unwrap();
            let kind = read_u8(bytes, &mut cursor).unwrap();
            let seq = read_u64(bytes, &mut cursor).unwrap();
            if version >= 4 {
                write_str(&mut out, &banner);
                out.push(kind);
                write_u64(&mut out, seq);
            }
            let pull_len = read_u32(bytes, &mut cursor).unwrap();
            write_u32(&mut out, pull_len);
            for pull_index in 0..pull_len {
                let pod_banner = read_str(bytes, &mut cursor).unwrap();
                let rarity = read_str(bytes, &mut cursor).unwrap();
                let fakeout = read_opt_str(bytes, &mut cursor).unwrap();
                let rank = read_u32(bytes, &mut cursor).unwrap();
                let shards = read_u32(bytes, &mut cursor).unwrap();
                let powered = read_u8(bytes, &mut cursor).unwrap();
                let item = read_str(bytes, &mut cursor).unwrap();
                write_str(&mut out, &pod_banner);
                write_str(&mut out, &rarity);
                write_opt_str(&mut out, fakeout.as_deref());
                write_u32(&mut out, rank);
                write_u32(&mut out, shards);
                out.push(powered);
                if version >= 5 {
                    let written = if blank_item && pull_index == 0 { "" } else { item.as_str() };
                    write_str(&mut out, written);
                }
            }
            let fault = read_fault(bytes, &mut cursor).unwrap();
            write_fault(&mut out, &fault).unwrap();
        }
        assert_eq!(cursor, bytes.len());
        out
    }

    #[test]
    fn branch_receipts_without_an_item_are_dropped_and_a_blank_v5_item_fails() {
        let content = Content::embedded();
        let mut account = funded(14, &content);
        let price = content.banners.cost.ten_pull_cost as f32;
        account.credits = price * 2.0;
        account.pull_ten(&content, "standard", "branch-key").unwrap();
        let credits = account.credits();
        let bytes = account.to_bytes(&content).unwrap();
        for version in [3u32, 4] {
            let older = rewrite_pulls(&bytes, version, false);
            let loaded = Account::from_bytes(&older, &content).unwrap();
            assert!(loaded.pulls_by_key.is_empty(), "v{version} receipts are dropped");
            assert!((loaded.credits() - credits).abs() < 1e-3);
            let mut again = loaded;
            let replay = again.pull_ten(&content, "standard", "branch-key").unwrap();
            assert!(replay.fresh, "a dropped v{version} key is not a replay");
        }
        let blank = rewrite_pulls(&bytes, 5, true);
        let err = Account::from_bytes(&blank, &content).unwrap_err();
        assert!(err.contains("no item"), "{err}");
    }

    #[test]
    fn a_replay_with_the_wrong_banner_or_kind_faults() {
        let content = Content::embedded();
        let mut account = funded(11, &content);
        let first = account.pull_with_key(&content, "standard", "same").unwrap();
        let credits = account.credits();
        assert_eq!(account.pull_with_key(&content, "other", "same").unwrap_err(), PullError::Mismatch);
        assert_eq!(account.credits(), credits, "a mismatched banner does not charge");
        assert_eq!(account.pull_ten(&content, "standard", "same").unwrap_err(), PullError::Mismatch);
        assert_eq!(account.credits(), credits, "a mismatched kind does not charge");
        let again = account.pull_with_key(&content, "standard", "same").unwrap();
        assert!(!again.fresh);
        assert_eq!(again.pulls, first.pulls);
        let stored = account.pulls_by_key.get("same").expect("the key is stored");
        assert_eq!(stored.banner, "standard");
        assert!(!stored.ten);
        assert!(stored.fault.is_none());
        assert_eq!(stored.pulls, first.pulls);
        let mut loaded = Account::from_bytes(&account.to_bytes(&content).unwrap(), &content).unwrap();
        assert_eq!(loaded.pull_ten(&content, "standard", "same").unwrap_err(), PullError::Mismatch);
    }

    #[test]
    fn saved_keys_stay_bounded_and_a_short_pull_does_not_grow_them() {
        let mut content = Content::embedded();
        content.banners.saved.max_keys = 2;
        let mut account = funded(12, &content);
        for key in ["m", "a", "z", "b"] {
            account.pull_with_key(&content, "standard", key).unwrap();
        }
        assert_eq!(account.pulls_by_key.len(), 2);
        assert!(!account.pulls_by_key.contains_key("m"));
        assert!(!account.pulls_by_key.contains_key("a"));
        assert!(account.pulls_by_key.contains_key("z"));
        assert!(account.pulls_by_key.contains_key("b"));

        content.banners.saved.max_keys = 8;
        let mut roomy = funded(13, &content);
        for key in ["m", "a", "z", "b"] {
            roomy.pull_with_key(&content, "standard", key).unwrap();
        }
        assert_eq!(roomy.pulls_by_key.len(), 4);
        let bytes = roomy.to_bytes(&content).unwrap();
        content.banners.saved.max_keys = 2;
        assert!(roomy.to_bytes(&content).is_err(), "an oversized set fails at save time");
        let loaded = Account::from_bytes(&bytes, &content).unwrap();
        assert_eq!(loaded.pulls_by_key.len(), 2, "an old oversized save still loads");
        assert!(loaded.pulls_by_key.contains_key("z"));
        assert!(loaded.pulls_by_key.contains_key("b"));

        let mut broke = Account::from_seed(14);
        for n in 0..5 {
            let err = broke.pull_with_key(&content, "standard", &format!("short-{n}")).unwrap_err();
            assert!(matches!(err, PullError::Short { .. }), "{err}");
        }
        assert!(broke.pulls_by_key.is_empty(), "a short pull must not store a key");
        assert_eq!(broke.credits(), 0.0);
    }
}
