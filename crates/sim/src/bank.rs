//! Match-end count-up cues. The sim does not play them and does not touch a wallet.

use crate::content::{milestone_payout, wave_payout, win_payout};

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
pub fn cues_between(show: BankShow, prev: f32, now: f32, tick_secs: f32, count_secs: f32) -> Result<Vec<BankCue>, String> {
    if !(tick_secs > 0.0) {
        return Err("count tick interval must be > 0".into());
    }
    if !(count_secs > 0.0) {
        return Err("count duration must be > 0".into());
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
    use crate::content::Content;
    use crate::sim::{Phase, PlayerInput, Sim};

    #[test]
    fn ticks_stay_on_the_content_interval_and_quit_has_no_chime() {
        let content = Content::embedded();
        let tick = content.ui.bank.count_tick_secs;
        let count = content.ui.bank.count_secs;
        assert!(tick > 0.0 && count >= tick);
        let early = cues_between(BankShow::Win, 0.0, tick * 0.5, tick, count).unwrap();
        assert!(early.is_empty(), "half an interval does not tick");
        let once = cues_between(BankShow::Win, 0.0, tick, tick, count).unwrap();
        assert_eq!(once, vec![BankCue::Tick]);
        let next = cues_between(BankShow::End, tick, tick * 2.0, tick, count).unwrap();
        assert_eq!(next, vec![BankCue::Tick]);
        let quit = cues_between(BankShow::Quit, count - tick, count, tick, count).unwrap();
        assert!(quit.contains(&BankCue::Tick));
        assert!(!quit.contains(&BankCue::Chime));
        let dropped = cues_between(BankShow::Disconnect, count - tick, count, tick, count).unwrap();
        assert!(!dropped.contains(&BankCue::Chime));
        let won = cues_between(BankShow::Win, count - tick, count, tick, count).unwrap();
        assert!(won.contains(&BankCue::Tick));
        assert!(won.contains(&BankCue::Chime));
        let done = cues_between(BankShow::End, count, count + tick, tick, count).unwrap();
        assert!(done.is_empty(), "the count does not tick after it finishes");
    }

    #[test]
    fn currency_gain_is_the_open_wave_until_a_win_adds_the_content_bonus() {
        let content = Content::embedded();
        let sim = Sim::new(&content, "normal", 1);
        let token = sim.players[0].token;
        let tier = content.tier("normal").currency;
        let wave = wave_payout(&content.currency, 1, tier);
        assert_eq!(sim.currency_gain.get(&token).copied(), Some(wave));
        assert_eq!(sim.currency_bonus.get(&token).copied(), Some(0.0));
        let full = crate::content::currency_reward(&content.currency, 10, tier, true, 0);
        assert!(wave < full, "a quit on the first wave is not the win total");

        let mut won = sim.clone();
        won.grant_victory_bonus(&content);
        let bonus = win_payout(&content.currency, tier);
        assert_eq!(won.currency_bonus.get(&token).copied(), Some(bonus));
        assert_eq!(won.currency_gain.get(&token).copied(), Some(wave + bonus));
        let mile = milestone_payout(&content.currency, content.currency.flat_after_wave + content.currency.endless_milestone_every, tier);
        assert_eq!(mile, content.currency.endless_milestone_bonus * tier);
        assert_eq!(milestone_payout(&content.currency, content.currency.flat_after_wave, tier), 0.0);
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
