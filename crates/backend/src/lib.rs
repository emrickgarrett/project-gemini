//! Crate pulls, pity, and the audit log. The sim and the client do not depend on this crate.
//! The client plays back `Pull::rarity` and `Pull::fakeout`. It does not roll them.

#[cfg(target_arch = "wasm32")]
compile_error!("gemini-backend does not build for wasm32");

use std::collections::BTreeMap;

use gemini_sim::content::Content;
use gemini_sim::rng::Rng;

const TIERS: [&str; 4] = ["common", "rare", "epic", "legendary"];

#[derive(Clone, Debug, Default)]
struct Pity {
    since_legendary: u32,
    since_epic: u32,
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
    pub credits: f32,
    pity: BTreeMap<String, Pity>,
    active_banner: Option<String>,
    rng: Rng,
    pub audit: Vec<AuditEntry>,
}

impl Account {
    pub fn new(seed: u64) -> Self {
        Self {
            rank: 1,
            shards: 0,
            credits: 0.0,
            pity: BTreeMap::new(),
            active_banner: None,
            rng: Rng::new(seed),
            audit: Vec::new(),
        }
    }

    pub fn since_legendary(&self, banner: &str) -> u32 {
        self.pity.get(banner).map(|p| p.since_legendary).unwrap_or(0)
    }

    pub fn pull(&mut self, content: &Content, banner: &str) -> Result<Pull, String> {
        let cost = content.banners.cost.single as f32;
        self.charge(cost)?;
        self.roll(content, banner, cost)
    }

    /// Ten pods for `cost.ten_pull`. Pity and the epic guarantee still apply per pod.
    pub fn pull_ten(&mut self, content: &Content, banner: &str) -> Result<Vec<Pull>, String> {
        let cost = content.banners.cost.ten_pull as f32;
        self.charge(cost)?;
        let mut pulls = Vec::with_capacity(10);
        for i in 0..10 {
            let spent = if i == 0 { cost } else { 0.0 };
            pulls.push(self.roll(content, banner, spent)?);
        }
        Ok(pulls)
    }

    fn charge(&mut self, cost: f32) -> Result<(), String> {
        if self.credits + 1e-3 < cost {
            return Err(format!("need {cost} credits to pull"));
        }
        self.credits -= cost;
        Ok(())
    }

    fn roll(&mut self, content: &Content, banner: &str, spent: f32) -> Result<Pull, String> {
        if banner.is_empty() {
            return Err("pull needs a banner".into());
        }
        let mut pity = self.begin_banner(content, banner);
        let pity_legendary_before = pity.since_legendary;
        let pity_epic_before = pity.since_epic;
        let rarity = roll_rarity(&mut pity, &mut self.rng, content);
        let fakeout = roll_fakeout(&mut self.rng, content, &rarity);
        self.pity.insert(banner.to_string(), pity.clone());
        let max_rank = content.duplicate.max_rank;
        let powered = if self.rank < max_rank {
            self.rank += 1;
            true
        } else {
            false
        };
        let shards = if powered {
            0
        } else {
            content.cosmetics.shards_for(&rarity)?
        };
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

fn roll_rarity(pity: &mut Pity, rng: &mut Rng, content: &Content) -> String {
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
    let roll = rng.next_f32();
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
fn roll_fakeout(rng: &mut Rng, content: &Content, rarity: &str) -> Option<String> {
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
    if rng.next_f32() < tier.fakeout_chance {
        Some(above.to_string())
    } else {
        None
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
        let mut account = Account::new(seed);
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
        assert!(err.contains("credits"), "{err}");

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
        let mut account = Account::new(0x5EED_F00D);
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
        let mut quiet = Account::new(0x5EED_F00D);
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
        })
        .expect("content with fakeouts off")
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
}
