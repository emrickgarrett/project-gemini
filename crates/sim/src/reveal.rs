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
        self.take_due()
    }

    /// Jump to the card. Only the rolled tier's hit plays. The full sting does not.
    pub fn skip(&mut self) -> Vec<String> {
        self.time = self.plan.card_at;
        for played in &mut self.played {
            *played = true;
        }
        vec![format!("crate_{}_hit", self.plan.rarity)]
    }

    fn take_due(&mut self) -> Vec<String> {
        let mut fired = Vec::new();
        for (i, cue) in self.plan.cues.iter().enumerate() {
            if self.played[i] || cue.at > self.time {
                continue;
            }
            self.played[i] = true;
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

/// One pod in roll order. The client copies this off the server reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridCard {
    pub rarity: String,
    pub fakeout: Option<String>,
}

/// Highest rarity in roll order. A tie keeps the earlier pod.
pub fn best_index(content: &Content, cards: &[GridCard]) -> Result<usize, String> {
    if cards.is_empty() {
        return Err("ten pull has no pods".into());
    }
    let mut best_at = 0usize;
    let mut best_rank = rank_of(content, &cards[0].rarity)?;
    for (index, card) in cards.iter().enumerate().skip(1) {
        let rank = rank_of(content, &card.rarity)?;
        if rank > best_rank {
            best_rank = rank;
            best_at = index;
        }
    }
    Ok(best_at)
}

fn rank_of(content: &Content, rarity: &str) -> Result<u32, String> {
    content
        .crate_reveal
        .tier(rarity)
        .map(|tier| tier.rank)
        .ok_or_else(|| format!("unknown rarity {rarity}"))
}

#[derive(Clone, Debug)]
pub struct GridPlan {
    pub cards: Vec<GridCard>,
    pub best: usize,
    pub fast: Vec<usize>,
    pub fast_flip: f32,
    pub flip_cue: String,
    pub best_plan: RevealPlan,
}

/// Fast-flips every card except the best, then plays that card's full tier.
#[derive(Clone, Debug)]
pub struct GridRun {
    pub plan: GridPlan,
    pub time: f32,
    fast_fired: Vec<bool>,
    best_fired: Vec<bool>,
}

impl GridRun {
    pub fn start(content: &Content, cards: &[GridCard]) -> Result<Self, String> {
        let pods = content.banners.cost.pods as usize;
        if cards.len() != pods {
            return Err(format!("ten pull has {} pods, content says {pods}", cards.len()));
        }
        let best = best_index(content, cards)?;
        let fast: Vec<usize> = (0..cards.len()).filter(|index| *index != best).collect();
        let card = &cards[best];
        let best_plan = plan(content, &card.rarity, card.fakeout.as_deref())?;
        let fast_flip = content.crate_reveal.grid.fast_flip;
        if !(fast_flip > 0.0) {
            return Err("crate_reveal.toml grid.fast_flip must be > 0".into());
        }
        Ok(Self {
            fast_fired: vec![false; fast.len()],
            best_fired: vec![false; best_plan.cues.len()],
            plan: GridPlan {
                cards: cards.to_vec(),
                best,
                fast,
                fast_flip,
                flip_cue: content.crate_reveal.grid.flip_cue.clone(),
                best_plan,
            },
            time: 0.0,
        })
    }

    pub fn advance(&mut self, dt: f32) -> Vec<String> {
        self.time += dt.max(0.0);
        self.take_due()
    }

    /// Drop every fast flip that has not played. The best card's timeline still runs.
    pub fn skip(&mut self) -> Vec<String> {
        for fired in &mut self.fast_fired {
            *fired = true;
        }
        let start = self.best_start();
        if self.time < start {
            self.time = start;
        }
        Vec::new()
    }

    pub fn finished(&self) -> bool {
        self.shown_face(self.plan.best).is_some()
    }

    /// Face rarity once that card is up. `None` is the shared back.
    pub fn shown_face(&self, index: usize) -> Option<&str> {
        if index == self.plan.best {
            let start = self.best_start();
            if self.time < start {
                return None;
            }
            if self.time - start >= self.plan.best_plan.card_at {
                return Some(self.plan.cards[index].rarity.as_str());
            }
            return None;
        }
        let position = self.plan.fast.iter().position(|slot| *slot == index)?;
        let at = (position as f32 + 1.0) * self.plan.fast_flip;
        if self.fast_fired[position] || self.time >= at {
            return Some(self.plan.cards[index].rarity.as_str());
        }
        None
    }

    fn best_start(&self) -> f32 {
        self.plan.fast.len() as f32 * self.plan.fast_flip
    }

    fn take_due(&mut self) -> Vec<String> {
        let mut fired = Vec::new();
        let flip = self.plan.fast_flip;
        let cue = self.plan.flip_cue.clone();
        for (position, done) in self.fast_fired.iter_mut().enumerate() {
            if *done {
                continue;
            }
            let at = (position as f32 + 1.0) * flip;
            if self.time < at {
                continue;
            }
            *done = true;
            fired.push(cue.clone());
        }
        let start = self.plan.fast.len() as f32 * flip;
        if self.time < start {
            return fired;
        }
        let local = self.time - start;
        for (index, cue) in self.plan.best_plan.cues.iter().enumerate() {
            if self.best_fired[index] || cue.at > local {
                continue;
            }
            self.best_fired[index] = true;
            fired.push(cue.id.clone());
        }
        fired
    }
}

/// Refund screen. At most one neutral UI tick. No sting and no bank chime.
#[derive(Clone, Debug)]
pub struct RefundRun {
    pub message: String,
    tick: String,
    pub time: f32,
    played: bool,
}

impl RefundRun {
    pub fn start(content: &Content) -> Self {
        Self {
            message: content.crate_reveal.grid.refund_message.clone(),
            tick: content.crate_reveal.grid.refund_tick.clone(),
            time: 0.0,
            played: false,
        }
    }

    pub fn advance(&mut self, dt: f32) -> Vec<String> {
        self.time += dt.max(0.0);
        if self.played {
            return Vec::new();
        }
        self.played = true;
        vec![self.tick.clone()]
    }

    pub fn skip(&mut self) -> Vec<String> {
        self.played = true;
        Vec::new()
    }
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
        let cards = [("common", [3.6, 4.2], 4.2), ("rare", [3.6, 5.0], 5.0), ("epic", [3.6, 5.3], 5.3)];
        for (rarity, rise, card) in cards {
            let tier = content.crate_reveal.tier(rarity).unwrap();
            assert_eq!(tier.drop, [0.0, 2.0], "{rarity}");
            assert_eq!(tier.crack, [2.0, 3.5], "{rarity}");
            assert_eq!(tier.burst, 3.5, "{rarity}");
            assert_eq!(tier.rise, rise, "{rarity}");
            assert_eq!(tier.card, card, "{rarity}");
            assert!(tier.freeze.is_none(), "{rarity}");
        }
        let legendary = &content.crate_reveal.legendary;
        let freeze = legendary.freeze.expect("legendary freeze");
        assert!((freeze[1] - freeze[0] - 1.0).abs() < 1e-4, "legendary freeze is one second");
        assert_eq!(legendary.drop, [0.0, 2.0]);
        assert_eq!(legendary.crack, [2.0, 3.5]);
        assert_eq!(freeze, [3.5, 4.5]);
        assert_eq!(legendary.burst, 4.5);
        assert_eq!(legendary.rise, [4.6, 6.5]);
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
    fn skip_plays_exactly_one_hit_for_the_rolled_tier() {
        let content = Content::embedded();
        let cases = [
            ("common", None),
            ("rare", Some("epic")),
            ("epic", Some("legendary")),
            ("legendary", None),
        ];
        for (rarity, fakeout) in cases {
            let mut run = RevealRun::start(&content, rarity, fakeout).unwrap();
            let _ = run.advance(0.05);
            let fired = run.skip();
            assert_eq!(fired, vec![format!("crate_{rarity}_hit")], "{rarity} skip {fired:?}");
            assert!(fired.iter().all(|id| id != &format!("crate_{rarity}")), "the full sting must not play");
            assert!((run.time - run.plan.card_at).abs() < 1e-4);
            let again = run.advance(30.0);
            assert!(again.is_empty(), "{rarity} cues replayed {again:?}");
            let hit = format!("crate_{rarity}_hit");
            let row = content.sfx(&hit).unwrap_or_else(|| panic!("{hit} mix row"));
            assert!(row.category_ui, "{hit} is a UI sound");
            let sting_vol = content.cue_volume(&format!("crate_{rarity}")).unwrap();
            assert!((content.cue_volume(&hit).unwrap() - sting_vol).abs() < 1e-4);
            let bytes = std::fs::read(
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets").join(content.cue_path(&hit).unwrap()),
            )
            .unwrap();
            let seconds = crate::ogg::vorbis_seconds(&bytes).unwrap_or_else(|| panic!("{hit} duration"));
            let expect = match rarity {
                "common" => 0.31,
                "rare" => 0.51,
                "epic" => 0.71,
                "legendary" => 0.96,
                _ => unreachable!(),
            };
            assert!((seconds - expect).abs() < 0.03, "{hit} is {seconds}s, expected about {expect}");
        }
    }

    fn grid_cards(content: &Content, legendary_at: Option<usize>) -> Vec<GridCard> {
        let pods = content.banners.cost.pods as usize;
        let mut cards = vec![GridCard { rarity: "common".into(), fakeout: None }; pods];
        if let Some(index) = legendary_at {
            cards[index].rarity = "legendary".into();
        }
        cards
    }

    #[test]
    fn ten_pull_fast_flips_spare_the_best_card() {
        let content = Content::embedded();
        let pods = content.banners.cost.pods as usize;
        let legendary_at = pods / 2;
        let cards = grid_cards(&content, Some(legendary_at));
        assert_eq!(best_index(&content, &cards).unwrap(), legendary_at);
        let mut tied = grid_cards(&content, None);
        tied[0].rarity = "epic".into();
        tied[legendary_at].rarity = "epic".into();
        assert_eq!(best_index(&content, &tied).unwrap(), 0, "a tie stays with the first pod");
        let mut run = GridRun::start(&content, &cards).unwrap();
        assert_eq!(run.plan.best, legendary_at);
        let flip = content.crate_reveal.grid.fast_flip;
        let cue = content.crate_reveal.grid.flip_cue.clone();
        let mut ticks = 0usize;
        for _ in 0..pods - 1 {
            let fired = run.advance(flip);
            ticks += fired.iter().filter(|id| *id == &cue).count();
        }
        assert_eq!(ticks, pods - 1, "one tick per fast flip");
        assert!(run.shown_face(legendary_at).is_none(), "the best card stays face down");
        let sting = format!("crate_{}", cards[legendary_at].rarity);
        let tail = run.advance(run.plan.best_plan.card_at + flip);
        assert_eq!(tail.iter().filter(|id| *id == &cue).count(), 0);
        assert_eq!(tail.iter().filter(|id| *id == &sting).count(), 1);
        assert_eq!(tail.iter().filter(|id| id.as_str() == "crate_common" || id.as_str() == "crate_rare" || id.as_str() == "crate_epic").count(), 0);
        assert!(run.shown_face(legendary_at).is_some());
    }

    #[test]
    fn skip_stops_flip_ticks_and_still_plays_the_best_sting() {
        let content = Content::embedded();
        let pods = content.banners.cost.pods as usize;
        let cards = grid_cards(&content, Some(pods / 2));
        let mut run = GridRun::start(&content, &cards).unwrap();
        let flip = content.crate_reveal.grid.fast_flip;
        let cue = content.crate_reveal.grid.flip_cue.clone();
        let first = run.advance(flip);
        assert_eq!(first.iter().filter(|id| *id == &cue).count(), 1);
        let skipped = run.skip();
        assert!(skipped.iter().all(|id| id != &cue), "skip emits no queued flip ticks: {skipped:?}");
        let sting = format!("crate_{}", cards[run.plan.best].rarity);
        let rest = run.advance(run.plan.best_plan.card_at + flip);
        assert_eq!(rest.iter().filter(|id| *id == &cue).count(), 0, "no flip ticks after the skip");
        assert_eq!(rest.iter().filter(|id| *id == &sting).count(), 1, "the best sting still plays");
        let tease = rest.iter().any(|id| id.contains("tease"));
        assert!(tease, "tease plays only with the best card: {rest:?}");
        assert_eq!(rest.iter().filter(|id| id.as_str() == "crate_fakeout").count(), 0, "legendary has no fakeout");

        let mut cold = GridRun::start(&content, &cards).unwrap();
        assert!(cold.skip().is_empty());
        let played = cold.advance(cold.plan.best_plan.card_at + flip);
        assert_eq!(played.iter().filter(|id| *id == &cue).count(), 0);
        assert_eq!(played.iter().filter(|id| *id == &sting).count(), 1);
    }

    #[test]
    fn display_order_does_not_change_the_best_pod() {
        let content = Content::embedded();
        let pods = content.banners.cost.pods as usize;
        let cards = grid_cards(&content, Some(pods / 2));
        let best = best_index(&content, &cards).unwrap();
        let mut reversed = cards.clone();
        reversed.reverse();
        let reversed_best = best_index(&content, &reversed).unwrap();
        assert_eq!(cards[best].rarity, reversed[reversed_best].rarity);
        assert_ne!(best, reversed_best);
        let run = GridRun::start(&content, &cards).unwrap();
        assert_eq!(run.plan.best, best);
        assert_eq!(run.plan.fast.len(), pods - 1);
        assert!(!run.plan.fast.contains(&best));
    }

    #[test]
    fn refund_plays_one_neutral_tick_and_no_sting() {
        let content = Content::embedded();
        let mut refund = RefundRun::start(&content);
        let tick = content.crate_reveal.grid.refund_tick.clone();
        assert_ne!(tick, content.ui.sounds.bank_chime);
        let cues = refund.advance(content.crate_reveal.grid.fast_flip);
        assert_eq!(cues, vec![tick]);
        assert!(refund.advance(content.crate_reveal.grid.fast_flip).is_empty());
        let mut skipped = RefundRun::start(&content);
        assert!(skipped.skip().is_empty());
        assert!(skipped.advance(content.crate_reveal.grid.fast_flip).is_empty());
    }
}
