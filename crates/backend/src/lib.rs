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

#[derive(Clone, Debug, Default)]
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
}

impl std::fmt::Display for PullError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Short { cost, have } => write!(f, "need {cost} credits to pull, have {have}"),
            Self::Banner => write!(f, "pull needs a banner"),
            Self::Shards => write!(f, "rolled rarity has no shard cost"),
        }
    }
}

/// One server pull. `fakeout` is the teased tier, exactly one step above `rarity`, or none.
#[derive(Clone, Debug)]
pub struct Pull {
    pub banner: String,
    pub rarity: String,
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
        }
    }

    pub fn since_legendary(&self, banner: &str) -> u32 {
        self.pity.get(banner).map(|p| p.since_legendary).unwrap_or(0)
    }

    /// Bytes for the stand-in save. A later host can use the same account fields.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        out.extend(SAVE_MAGIC);
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
        Ok(out)
    }

    /// Restore a save. Truncation, a bad header, and trailing bytes are errors.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < SAVE_MAGIC.len() || &bytes[..SAVE_MAGIC.len()] != SAVE_MAGIC {
            return Err("dev account save header is wrong".into());
        }
        let mut at = SAVE_MAGIC.len();
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
        if at != bytes.len() {
            return Err("dev account save has trailing bytes".into());
        }
        let mut rng = ChaCha20Rng::from_seed(seed);
        rng.set_word_pos((u128::from(pos_hi) << 64) | u128::from(pos_lo));
        Ok(Self {
            rank,
            shards,
            credits,
            pity,
            active_banner,
            rng,
            audit,
        })
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

    /// Ten pods for `cost.ten_pull`. Pity and the epic guarantee still apply per pod.
    pub fn pull_ten(&mut self, content: &Content, banner: &str) -> Result<Vec<Pull>, PullError> {
        if banner.is_empty() {
            return Err(PullError::Banner);
        }
        let cost = content.banners.cost.ten_pull as f32;
        self.charge(cost)?;
        let mut pulls = Vec::with_capacity(10);
        for i in 0..10 {
            let spent = if i == 0 { cost } else { 0.0 };
            match self.roll(content, banner, spent) {
                Ok(pull) => pulls.push(pull),
                Err(err) => {
                    self.credits += spent;
                    return Err(err);
                }
            }
        }
        Ok(pulls)
    }

    fn charge(&mut self, cost: f32) -> Result<(), PullError> {
        if self.credits + 1e-3 < cost {
            return Err(PullError::Short { cost, have: self.credits });
        }
        self.credits -= cost;
        Ok(())
    }

    fn roll(&mut self, content: &Content, banner: &str, spent: f32) -> Result<Pull, PullError> {
        let mut pity = self.begin_banner(content, banner);
        let pity_legendary_before = pity.since_legendary;
        let pity_epic_before = pity.since_epic;
        let rarity = roll_rarity(&mut pity, &mut self.rng, content);
        let fakeout = roll_fakeout(&mut self.rng, content, &rarity);
        let max_rank = content.duplicate.max_rank;
        let powered = self.rank < max_rank;
        let shards = if powered {
            0
        } else {
            match content.cosmetics.shards_for(&rarity) {
                Ok(shards) => shards,
                Err(_) => return Err(PullError::Shards),
            }
        };
        self.pity.insert(banner.to_string(), pity.clone());
        if powered {
            self.rank += 1;
        }
        self.shards += shards;
        let pull = Pull {
            banner: banner.to_string(),
            rarity,
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

    /// When pity carries, the banner being pulled inherits the counter from the banner just left.
    fn begin_banner(&mut self, content: &Content, banner: &str) -> Pity {
        if content.banners.pity.carries_between_banners {
            if let Some(prev) = self.active_banner.clone() {
                if prev != banner {
                    let carried = self.pity.get(&prev).cloned().unwrap_or_default();
                    self.pity.insert(banner.to_string(), carried);
                }
            }
        }
        self.active_banner = Some(banner.to_string());
        self.pity.entry(banner.to_string()).or_default().clone()
    }
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

const SAVE_MAGIC: &[u8] = b"GEMACCT1";

fn write_u32(out: &mut Vec<u8>, value: u32) {
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

        account.credits = content.banners.cost.ten_pull as f32;
        let ten = account.pull_ten(&content, "standard").unwrap();
        assert_eq!(ten.len(), 10);
        assert!(
            ten.iter().any(|pull| pull.rarity == "epic" || pull.rarity == "legendary"),
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
    fn a_save_roundtrip_keeps_pity_and_garbage_fails() {
        let content = Content::embedded();
        let mut account = Account::from_seed(4);
        account.credits = content.banners.dev.starting_credits;
        let first = account.pull(&content, "standard").unwrap();
        assert!(TIERS.contains(&first.rarity.as_str()), "{}", first.rarity);
        let bytes = account.to_bytes().unwrap();
        let mut loaded = Account::from_bytes(&bytes).unwrap();
        assert_eq!(loaded.credits(), account.credits());
        assert_eq!(loaded.since_legendary("standard"), account.since_legendary("standard"));
        assert_eq!(loaded.audit.len(), account.audit.len());
        assert_eq!(loaded.rank, account.rank);
        let again = account.pull(&content, "standard").unwrap();
        let restored = loaded.pull(&content, "standard").unwrap();
        assert_eq!(restored.rarity, again.rarity);
        assert_eq!(restored.fakeout, again.fakeout);
        assert!(Account::from_bytes(b"garbage").is_err());
        assert!(Account::from_bytes(&bytes[..bytes.len() - 1]).is_err());
        let mut extra = bytes.clone();
        extra.push(9);
        assert!(Account::from_bytes(&extra).is_err());
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
}
