//! Persistent account power. Crate pulls are the only thing that raises it.
//! Run upgrades live on the match and are gone when the next `Sim` is built.

use crate::content::{Banners, Content};

#[derive(Clone, Debug)]
pub struct Pull {
    pub rarity: String,
    pub rank: u32,
    pub shards: u32,
    /// True when this pull raised duplicate rank. Rank is the only persistent power.
    pub powered: bool,
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub rank: u32,
    pub shards: u32,
    pub credits: f32,
    pub since_legendary: u32,
    pub since_epic: u32,
    state: u64,
}

impl Profile {
    pub fn new(seed: u64) -> Self {
        Self {
            rank: 1,
            shards: 0,
            credits: 0.0,
            since_legendary: 0,
            since_epic: 0,
            state: seed | 1,
        }
    }

    pub fn pull(&mut self, content: &Content) -> Result<Pull, String> {
        let cost = content.banners.cost.single as f32;
        if self.credits + 1e-3 < cost {
            return Err(format!("need {cost} credits to pull"));
        }
        self.credits -= cost;
        let rarity = self.roll_rarity(&content.banners);
        let powered = if self.rank < 6 {
            self.rank += 1;
            true
        } else {
            false
        };
        let shards = if powered { 0 } else { content.cosmetics.shards_for(&rarity) };
        self.shards += shards;
        Ok(Pull {
            rarity,
            rank: self.rank,
            shards,
            powered,
        })
    }

    fn roll_rarity(&mut self, banners: &Banners) -> String {
        self.since_legendary += 1;
        self.since_epic += 1;
        let pity = &banners.pity;
        let rates = &banners.rates;
        if self.since_legendary >= pity.legendary_hard {
            self.since_legendary = 0;
            self.since_epic = 0;
            return "legendary".to_string();
        }
        let mut legendary = rates.legendary;
        if self.since_legendary >= pity.legendary_soft_start {
            let steps = (self.since_legendary - pity.legendary_soft_start) as f32;
            legendary = (legendary + steps * pity.legendary_soft_step).min(1.0);
        }
        let roll = self.next_unit();
        let mut rarity = if roll < legendary {
            "legendary"
        } else if roll < legendary + rates.epic {
            "epic"
        } else if roll < legendary + rates.epic + rates.rare {
            "rare"
        } else {
            "common"
        };
        if self.since_epic >= pity.epic_or_better_guarantee && (rarity == "common" || rarity == "rare") {
            rarity = "epic";
        }
        if rarity == "legendary" {
            self.since_legendary = 0;
            self.since_epic = 0;
        } else if rarity == "epic" {
            self.since_epic = 0;
        }
        rarity.to_string()
    }

    fn next_unit(&mut self) -> f32 {
        self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
        ((self.state >> 8) & 0x00ff_ffff) as f32 / (1u32 << 24) as f32
    }
}
