//! Match-end count-up cues. The sim does not play them and does not touch a wallet.

/// Why the match-end screen is up. The chime plays for an end or a win only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankShow {
    End,
    Win,
    Quit,
    Disconnect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankCue {
    Tick,
    Chime,
}

/// Cues whose time crossed `(prev, now]`. Ticks stay on `tick_secs` and stop at `count_secs`.
/// An earned total of 0 plays nothing, including the chime.
pub fn cues_between(show: BankShow, prev: f32, now: f32, tick_secs: f32, count_secs: f32, earned: f32) -> Result<Vec<BankCue>, String> {
    if !(tick_secs > 0.0) {
        return Err("count tick interval must be > 0".into());
    }
    if !(count_secs > 0.0) {
        return Err("count duration must be > 0".into());
    }
    if !(earned > 0.0) {
        return Ok(Vec::new());
    }
    let mut cues = Vec::new();
    let mut tick_at = tick_secs;
    while tick_at <= count_secs && tick_at <= now {
        if tick_at > prev {
            cues.push(BankCue::Tick);
        }
        tick_at += tick_secs;
    }
    let chime = match show {
        BankShow::End | BankShow::Win => true,
        BankShow::Quit | BankShow::Disconnect => false,
    };
    if chime && prev < count_secs && now >= count_secs {
        cues.push(BankCue::Chime);
    }
    Ok(cues)
}

/// Earned digits at `elapsed`. The count finishes at `count_secs`.
pub fn shown_earned(earned: f32, elapsed: f32, count_secs: f32) -> f32 {
    if elapsed >= count_secs {
        earned
    } else if elapsed <= 0.0 {
        0.0
    } else {
        earned * (elapsed / count_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{milestone_payout, win_payout, Content};
    use crate::sim::{Phase, PlayerInput, Sim};

    #[test]
    fn ticks_stay_on_the_content_interval_and_quit_has_no_chime() {
        let content = Content::embedded();
        let tick = content.ui.bank.count_tick_secs;
        let count = content.ui.bank.count_secs;
        assert!(tick > 0.0 && count >= tick);
        let earned = 1.0;
        let early = cues_between(BankShow::Win, 0.0, tick * 0.5, tick, count, earned).unwrap();
        assert!(early.is_empty(), "half an interval does not tick");
        let once = cues_between(BankShow::Win, 0.0, tick, tick, count, earned).unwrap();
        assert_eq!(once, vec![BankCue::Tick]);
        let next = cues_between(BankShow::End, tick, tick * 2.0, tick, count, earned).unwrap();
        assert_eq!(next, vec![BankCue::Tick]);
        let quit = cues_between(BankShow::Quit, count - tick, count, tick, count, earned).unwrap();
        assert!(quit.contains(&BankCue::Tick));
        assert!(!quit.contains(&BankCue::Chime));
        let dropped = cues_between(BankShow::Disconnect, count - tick, count, tick, count, earned).unwrap();
        assert!(!dropped.contains(&BankCue::Chime));
        let won = cues_between(BankShow::Win, count - tick, count, tick, count, earned).unwrap();
        assert!(won.contains(&BankCue::Tick));
        assert!(won.contains(&BankCue::Chime));
        let done = cues_between(BankShow::End, count, count + tick, tick, count, earned).unwrap();
        assert!(done.is_empty(), "the count does not tick after it finishes");
    }

    fn clear_wave(sim: &mut Sim, content: &Content) {
        sim.enemies.clear();
        sim.pending_vents.clear();
        sim.director.pool = 0.0;
        sim.director.queue = 0.0;
        sim.phase = Phase::Combat;
        sim.step(content, PlayerInput::default());
    }

    fn open_next_wave(sim: &mut Sim, content: &Content) {
        let choice = PlayerInput { upgrade_choice: Some(0), ..PlayerInput::default() };
        sim.step(content, choice);
        sim.phase = Phase::Breather { left: 0.01 };
        sim.step(content, PlayerInput::default());
    }

    #[test]
    fn a_quit_before_wave_1_clears_banks_zero_and_stays_silent() {
        let content = Content::embedded();
        let sim = Sim::new(&content, "normal", 1);
        let token = sim.players[0].token;
        let earned = sim.currency_gain.get(&token).copied().unwrap_or(f32::NAN);
        assert_eq!(earned, 0.0, "opening wave 1 does not pay");
        assert_eq!(sim.currency_bonus.get(&token).copied(), Some(0.0));
        let tick = content.ui.bank.count_tick_secs;
        let count = content.ui.bank.count_secs;
        let quit = cues_between(BankShow::Quit, 0.0, count, tick, count, earned).unwrap();
        assert!(quit.is_empty(), "an earned 0 quit plays no ticks and no chime");
        let won = cues_between(BankShow::Win, 0.0, count, tick, count, earned).unwrap();
        assert!(won.is_empty(), "an earned 0 screen plays no ticks and no chime");
    }

    #[test]
    fn a_quit_after_cleared_waves_banks_that_total_without_a_bonus() {
        let content = Content::embedded();
        let mut sim = Sim::new(&content, "normal", 3);
        let token = sim.players[0].token;
        let tier = content.tier("normal").currency;
        let first_boss = content.waves.wave.iter().find(|wave| wave.boss.is_some()).map(|wave| wave.n).expect("a boss wave");
        let cleared = first_boss.checked_sub(1).expect("a wave before the first boss");
        let mut n = 0u32;
        while n < cleared {
            clear_wave(&mut sim, &content);
            assert!(sim.sounds.iter().any(|sound| sound.id == "currency_gain"), "the gain cue plays on the clear");
            assert!(sim.sounds.iter().all(|sound| sound.id != "wave"), "the gain cue does not share the frame with wave_start");
            n += 1;
            if n < cleared {
                open_next_wave(&mut sim, &content);
                assert!(sim.sounds.iter().any(|sound| sound.id == "wave"), "the next wave opens with wave_start");
                assert!(sim.sounds.iter().all(|sound| sound.id != "currency_gain"), "opening a wave does not replay the gain cue");
            }
        }
        let expected = crate::content::currency_reward(&content.currency, cleared, tier, false, 0);
        assert_eq!(sim.currency_gain[&token], expected);
        assert_eq!(sim.currency_bonus[&token], 0.0);
        let mile = milestone_payout(&content.currency, content.currency.flat_after_wave + content.currency.endless_milestone_every, tier);
        assert_eq!(mile, content.currency.endless_milestone_bonus * tier);
        assert_eq!(milestone_payout(&content.currency, content.currency.flat_after_wave, tier), 0.0);
    }

    #[test]
    fn a_quit_past_a_wave_20_milestone_keeps_the_milestone_pay() {
        let content = Content::embedded();
        let mut sim = Sim::new(&content, "normal", 1);
        sim.endless_mode = true;
        let token = sim.players[0].token;
        let tier = content.tier("normal").currency;
        let flat = content.currency.flat_after_wave;
        let milestone_wave = flat + content.currency.endless_milestone_every;
        assert_eq!(milestone_payout(&content.currency, flat, tier), 0.0);
        let mile = milestone_payout(&content.currency, milestone_wave, tier);
        assert_eq!(mile, content.currency.endless_milestone_bonus * tier);
        assert!(mile > 0.0);
        let mut n = 0u32;
        while n < milestone_wave {
            clear_wave(&mut sim, &content);
            n += 1;
            if n < milestone_wave {
                open_next_wave(&mut sim, &content);
            }
        }
        assert_eq!(sim.director.wave, milestone_wave);
        assert!(!matches!(sim.phase, Phase::Victory), "a quit is not a win");
        let expected = crate::content::currency_reward(&content.currency, milestone_wave, tier, false, 0);
        assert_eq!(sim.currency_gain[&token], expected);
        assert_eq!(sim.currency_bonus[&token], mile, "the milestone stays and the win bonus does not");
        assert!(expected > mile);
    }

    #[test]
    fn clearing_the_last_wave_banks_the_win_bonus_once() {
        let content = Content::embedded();
        let mut sim = Sim::new(&content, "normal", 2);
        let token = sim.players[0].token;
        let before = sim.currency_gain[&token];
        sim.enemies.clear();
        sim.pending_vents.clear();
        sim.director.pool = 0.0;
        sim.director.queue = 0.0;
        sim.phase = Phase::Combat;
        let last = content.waves.wave.iter().map(|wave| wave.n).max().expect("waves");
        sim.director.wave = last;
        sim.step(&content, PlayerInput::default());
        assert!(matches!(sim.phase, Phase::Victory));
        let bonus = win_payout(&content.currency, content.tier("normal").currency);
        assert_eq!(sim.currency_bonus[&token], bonus);
        assert_eq!(sim.currency_gain[&token], before + bonus);
        sim.step(&content, PlayerInput::default());
        assert_eq!(sim.currency_bonus[&token], bonus, "a finished match does not grant the win again");
    }
}
