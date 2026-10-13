//! Playback of a crate result the backend already rolled.
//! The client supplies `rarity` and an optional `fakeout`. This module does not roll either.

use crate::content::Content;

#[derive(Clone, Debug)]
pub struct RevealCue {
    pub at: f32,
    pub id: String,
}

#[derive(Clone, Debug)]
pub struct RevealPlan {
    pub rarity: String,
    pub fakeout: Option<String>,
    pub cues: Vec<RevealCue>,
    pub card_at: f32,
    drop: [f32; 2],
    crack: [f32; 2],
    burst: f32,
    rise: [f32; 2],
    freeze: Option<[f32; 2]>,
    fakeout_window: [f32; 2],
}

#[derive(Clone, Debug)]
pub struct RevealLook {
    /// Tier name on screen. The fakeout name only while its window is open.
    pub shown: String,
    pub rgb: [f32; 4],
    pub card: bool,
    /// 0 at the top of the drop, 1 when the crate has landed.
    pub drop: f32,
    /// 0 closed, 1 split open.
    pub crack: f32,
    /// 1 on the burst frame, fading across `burst_fade`.
    pub burst: f32,
    /// 0 before the rise window, 1 at its end.
    pub rise: f32,
    /// `freeze_dim` while a legendary freeze is holding, otherwise 0.
    pub dim: f32,
}

/// Clock for one playback. `advance` emits cues as elapsed time crosses them.
#[derive(Clone, Debug)]
pub struct RevealRun {
    pub plan: RevealPlan,
    pub time: f32,
    played: Vec<bool>,
}

pub fn plan(content: &Content, rarity: &str, fakeout: Option<&str>) -> Result<RevealPlan, String> {
    let tier = content
        .crate_reveal
        .tier(rarity)
        .ok_or_else(|| format!("unknown rarity {rarity}"))?;
    if let Some(tease) = fakeout {
        if content.crate_reveal.tier(tease).is_none() {
            return Err(format!("unknown fakeout {tease}"));
        }
        let from = tier_index(rarity).ok_or_else(|| format!("unknown rarity {rarity}"))?;
        let to = tier_index(tease).ok_or_else(|| format!("unknown fakeout {tease}"))?;
        if to != from + 1 {
            return Err(format!("fakeout {tease} is not one tier above {rarity}"));
        }
    }
    let sting = format!("crate_{rarity}");
    let sting_from_table = file_stem(&tier.sting);
    if sting != sting_from_table {
        return Err(format!("{rarity} sting {sting_from_table} does not match {sting}"));
    }
    let mut cues = vec![RevealCue {
        at: tier.drop[0],
        id: file_stem(&tier.tease),
    }];
    if fakeout.is_some() {
        let id = file_stem(&content.crate_reveal.fakeout_sfx);
        if !id.is_empty() {
            cues.push(RevealCue {
                at: tier.fakeout_window[0],
                id,
            });
        }
    }
    cues.push(RevealCue {
        at: tier.burst,
        id: sting,
    });
    Ok(RevealPlan {
        rarity: rarity.to_string(),
        fakeout: fakeout.map(|id| id.to_string()),
        cues,
        card_at: tier.card,
        drop: tier.drop,
        crack: tier.crack,
        burst: tier.burst,
        rise: tier.rise,
        freeze: tier.freeze,
        fakeout_window: tier.fakeout_window,
    })
}

impl RevealRun {
    pub fn start(content: &Content, rarity: &str, fakeout: Option<&str>) -> Result<Self, String> {
        let plan = plan(content, rarity, fakeout)?;
        let played = vec![false; plan.cues.len()];
        Ok(Self { plan, time: 0.0, played })
    }

    /// Move the clock forward by real elapsed seconds and return the cues that fire.
    pub fn advance(&mut self, dt: f32) -> Vec<String> {
        self.time += dt.max(0.0);
        self.take_due(false)
    }

    /// Jump to the card. The sting still fires. A skipped fakeout does not.
    pub fn skip(&mut self) -> Vec<String> {
        self.time = self.plan.card_at;
        self.take_due(true)
    }

    fn take_due(&mut self, skip: bool) -> Vec<String> {
        let sting = format!("crate_{}", self.plan.rarity);
        let mut fired = Vec::new();
        for (i, cue) in self.plan.cues.iter().enumerate() {
            if self.played[i] || cue.at > self.time {
                continue;
            }
            self.played[i] = true;
            if skip && cue.id != sting {
                continue;
            }
            fired.push(cue.id.clone());
        }
        fired
    }
}

pub fn look(content: &Content, plan: &RevealPlan, t: f32) -> RevealLook {
    let card = t >= plan.card_at;
    let in_fakeout = plan.fakeout.is_some()
        && t >= plan.fakeout_window[0]
        && t <= plan.fakeout_window[1]
        && !card;
    let shown = if in_fakeout {
        plan.fakeout.clone().unwrap_or_else(|| plan.rarity.clone())
    } else {
        plan.rarity.clone()
    };
    let rgb = content.palette.key(&format!("rarity_{shown}"));
    let dim = plan
        .freeze
        .filter(|window| t >= window[0] && t < window[1] && !card)
        .map(|_| content.crate_reveal.freeze_dim.clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let fade = content.crate_reveal.burst_fade;
    let burst = if fade > 0.0 && t >= plan.burst && t < plan.burst + fade && !card {
        1.0 - (t - plan.burst) / fade
    } else {
        0.0
    };
    RevealLook {
        shown,
        rgb,
        card,
        drop: span(t, plan.drop),
        crack: span(t, plan.crack),
        burst,
        rise: span(t, plan.rise),
        dim,
    }
}

fn span(t: f32, window: [f32; 2]) -> f32 {
    if window[1] <= window[0] {
        return if t >= window[1] { 1.0 } else { 0.0 };
    }
    ((t - window[0]) / (window[1] - window[0])).clamp(0.0, 1.0)
}

fn tier_index(id: &str) -> Option<usize> {
    ["common", "rare", "epic", "legendary"].iter().position(|tier| *tier == id)
}

fn file_stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.trim_end_matches(".ogg").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sting_matches_the_server_rarity() {
        let content = Content::embedded();
        for rarity in ["common", "rare", "epic", "legendary"] {
            let plan = plan(&content, rarity, None).unwrap_or_else(|err| panic!("{rarity}: {err}"));
            let sting = format!("crate_{rarity}");
            assert!(
                plan.cues.iter().any(|cue| cue.id == sting && cue.at == content.crate_reveal.tier(rarity).unwrap().burst),
                "{rarity} cues {:?}",
                plan.cues.iter().map(|cue| &cue.id).collect::<Vec<_>>()
            );
            let path = content.cue_path(&sting).unwrap_or_else(|| panic!("{sting} has no file"));
            assert!(path.ends_with(&format!("{sting}.ogg")), "{path}");
            let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
            assert!(root.join(path).is_file(), "{path}");
        }
    }

    #[test]
    fn no_fakeout_sound_without_a_fakeout() {
        let content = Content::embedded();
        for rarity in ["common", "rare", "epic", "legendary"] {
            let mut run = RevealRun::start(&content, rarity, None).unwrap();
            let fired = run.advance(30.0);
            assert!(
                fired.iter().all(|id| id != "crate_fakeout"),
                "{rarity} played {fired:?}"
            );
            assert!(fired.iter().any(|id| id == &format!("crate_{rarity}")));
        }
        let mut teased = RevealRun::start(&content, "rare", Some("epic")).unwrap();
        let fired = teased.advance(30.0);
        assert!(fired.iter().any(|id| id == "crate_fakeout"), "{fired:?}");
        assert!(fired.iter().any(|id| id == "crate_rare"), "sting stays on the server rarity");
        assert!(!fired.iter().any(|id| id == "crate_epic"), "the tease is not the sting");
    }

    #[test]
    fn tease_and_fakeout_timing_come_from_the_table() {
        let content = Content::embedded();
        let legendary = plan(&content, "legendary", None).unwrap();
        assert_eq!(legendary.cues[0].id, "crate_tease_legendary");
        assert_eq!(legendary.cues[0].at, content.crate_reveal.legendary.drop[0]);
        let rare = plan(&content, "rare", None).unwrap();
        assert_eq!(rare.cues[0].id, "crate_tease");
        let teased = plan(&content, "epic", Some("legendary")).unwrap();
        let fake = teased.cues.iter().find(|cue| cue.id == "crate_fakeout").unwrap();
        assert_eq!(fake.at, content.crate_reveal.epic.fakeout_window[0]);
        let look_mid = look(&content, &teased, 2.8);
        assert_eq!(look_mid.shown, "legendary");
        assert!(!look_mid.card);
        let look_card = look(&content, &teased, teased.card_at);
        assert_eq!(look_card.shown, "epic");
        assert!(look_card.card);
        let legendary = plan(&content, "legendary", None).unwrap();
        let frozen = look(&content, &legendary, 4.0);
        assert!((frozen.dim - content.crate_reveal.freeze_dim).abs() < 1e-4);
        let rising = look(&content, &legendary, (legendary.rise[0] + legendary.rise[1]) * 0.5);
        assert!(rising.rise > 0.4 && rising.rise < 0.6, "rise {}", rising.rise);
        let gold = content.palette.key("rarity_legendary");
        let epic = content.palette.key("rarity_epic");
        assert_eq!(look_mid.rgb, gold);
        assert_eq!(look_card.rgb, epic);
    }

    #[test]
    fn storyboard_timings_and_a_one_tier_fakeout() {
        let content = Content::embedded();
        for rarity in ["common", "rare", "epic"] {
            let tier = content.crate_reveal.tier(rarity).unwrap();
            assert_eq!(tier.drop, [0.0, 2.0], "{rarity}");
            assert_eq!(tier.crack, [2.0, 3.5], "{rarity}");
            assert_eq!(tier.burst, 3.5, "{rarity}");
            assert_eq!(tier.rise, [4.0, 5.5], "{rarity}");
            assert_eq!(tier.card, 5.5, "{rarity}");
            assert!(tier.freeze.is_none(), "{rarity}");
        }
        let legendary = &content.crate_reveal.legendary;
        let freeze = legendary.freeze.expect("legendary freeze");
        assert!((freeze[1] - freeze[0] - 1.0).abs() < 1e-4, "legendary freeze is one second");
        assert_eq!(legendary.drop, [0.0, 2.0]);
        assert_eq!(legendary.crack, [2.0, 3.5]);
        assert_eq!(freeze, [3.5, 4.5]);
        assert_eq!(legendary.burst, 4.5);
        assert_eq!(legendary.rise, [5.0, 6.5]);
        assert_eq!(legendary.card, 6.5);
        assert!(plan(&content, "common", Some("epic")).is_err(), "a fake-out cannot skip a tier");
        assert!(plan(&content, "rare", Some("epic")).is_ok());
        let silver = content.palette.hex("rarity_common").unwrap();
        let blue = content.palette.hex("rarity_rare").unwrap();
        let purple = content.palette.hex("rarity_epic").unwrap();
        let gold = content.palette.hex("rarity_legendary").unwrap();
        assert_eq!(silver, "#C0C7D0");
        assert_eq!(blue, "#3B82F6");
        assert_eq!(purple, "#A855F7");
        assert_eq!(gold, "#FACC15");
    }

    #[test]
    fn skip_plays_the_sting_and_skips_the_fakeout() {
        let content = Content::embedded();
        let mut run = RevealRun::start(&content, "rare", Some("epic")).unwrap();
        let fired = run.skip();
        assert_eq!(fired, vec!["crate_rare".to_string()]);
        assert!((run.time - run.plan.card_at).abs() < 1e-4);
        let again = run.advance(1.0);
        assert!(again.is_empty(), "cues do not replay {again:?}");
    }
}
