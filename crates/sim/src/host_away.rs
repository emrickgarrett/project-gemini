//! Real-time clock for a hidden host tab. The sim only stores `host_away` so a
//! frozen step returns before enemies, the wave timer, bleedout, or cooldowns move.

use std::collections::BTreeMap;

use crate::sim::{MusicMoment, Phase, Sim};

/// Seconds from `coop.toml` `[host_away]`.
#[derive(Clone, Debug)]
pub struct HostAwayRules {
    pub detect_after_seconds: f32,
    pub end_after_seconds: f32,
    pub resume_countdown_seconds: f32,
    pub freeze: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AwayPhase {
    Live,
    Away,
    Countdown,
    Ended,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AwayFrame {
    pub phase: AwayPhase,
    pub banner: bool,
    pub solo: bool,
    pub timer_fill: f32,
    pub digit: Option<u8>,
    pub digit_u: f32,
    pub cues: Vec<String>,
    pub frozen: bool,
    pub ended: bool,
}

#[derive(Clone, Debug)]
pub struct HostAwayClock {
    solo: bool,
    /// Page visibility. A hidden page does not start the resume countdown.
    page_hidden: bool,
    /// Host tab hidden, or this peer has not heard a live snapshot.
    silent: bool,
    silence: f32,
    countdown: f32,
    countdown_full: f32,
    phase: AwayPhase,
    reported: BTreeMap<u64, Vec<u32>>,
    pending: Vec<String>,
    /// HostAway was noticed. Silence stays at least `detect` until the next tick.
    packet_latched: bool,
    wants_announce: bool,
    finished: bool,
}

impl HostAwayClock {
    pub fn new(solo: bool) -> Self {
        Self {
            solo,
            page_hidden: false,
            silent: false,
            silence: 0.0,
            countdown: 0.0,
            countdown_full: 0.0,
            phase: AwayPhase::Live,
            reported: BTreeMap::new(),
            pending: Vec::new(),
            packet_latched: false,
            wants_announce: false,
            finished: false,
        }
    }

    pub fn phase(&self) -> AwayPhase {
        self.phase
    }

    pub fn frozen(&self) -> bool {
        matches!(self.phase, AwayPhase::Away | AwayPhase::Countdown)
    }

    /// True only during the away banner. Countdown snapshots say the host is back.
    pub fn replicate_away(&self) -> bool {
        self.phase == AwayPhase::Away
    }

    /// Solo is chosen while the match is live. Away and the countdown keep that choice.
    pub fn set_solo(&mut self, solo: bool) {
        if self.phase == AwayPhase::Live {
            self.solo = solo;
        }
    }

    pub fn set_page_hidden(&mut self, hidden: bool) {
        if hidden && self.phase == AwayPhase::Countdown {
            self.phase = AwayPhase::Away;
            self.countdown = 0.0;
            self.pending.push("host_away".to_string());
        }
        self.page_hidden = hidden;
    }

    pub fn set_silent(&mut self, silent: bool) {
        self.silent = silent;
    }

    /// The host already waited out the detect window. The banner waits for `tick`.
    pub fn note_away_packet(&mut self, detect: f32) {
        if matches!(self.phase, AwayPhase::Ended | AwayPhase::Countdown) {
            return;
        }
        if self.silence < detect {
            self.silence = detect;
        }
        self.packet_latched = true;
    }

    /// A frozen snapshot does not move the end timer or start the countdown.
    pub fn note_snapshot(&mut self, host_away: bool, countdown_secs: f32) {
        if matches!(self.phase, AwayPhase::Ended | AwayPhase::Countdown) {
            return;
        }
        if host_away {
            return;
        }
        if self.phase == AwayPhase::Away && !self.page_hidden {
            self.begin_countdown(countdown_secs);
            return;
        }
        if self.phase == AwayPhase::Live && !self.silent {
            self.silence = 0.0;
            self.packet_latched = false;
        }
    }

    /// The host tab is visible again. Call this after `tick` so a hitch is silence, not countdown.
    pub fn host_shown(&mut self, countdown_secs: f32) {
        self.page_hidden = false;
        self.silent = false;
        if self.phase == AwayPhase::Away {
            self.begin_countdown(countdown_secs);
        }
    }

    pub fn take_announce(&mut self) -> bool {
        if self.wants_announce && self.phase == AwayPhase::Away {
            self.wants_announce = false;
            true
        } else {
            false
        }
    }

    pub fn tick(&mut self, dt: f32, rules: &HostAwayRules, sim: &Sim) -> AwayFrame {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let mut cues = std::mem::take(&mut self.pending);
        if self.phase == AwayPhase::Ended {
            return self.frame(rules, cues);
        }
        if self.phase == AwayPhase::Countdown {
            if self.silent || self.page_hidden {
                self.phase = AwayPhase::Away;
                self.countdown = 0.0;
                cues.push("host_away".to_string());
            } else {
                self.countdown = (self.countdown - dt).max(0.0);
                if self.countdown <= 0.0 {
                    self.phase = AwayPhase::Live;
                    self.silence = 0.0;
                    self.packet_latched = false;
                    self.wants_announce = false;
                }
                return self.frame(rules, cues);
            }
        }
        if self.phase == AwayPhase::Live && !self.silent && !self.packet_latched {
            self.silence = 0.0;
        }
        if self.silent {
            self.silence += dt;
        }
        if self.phase == AwayPhase::Live && self.silence >= rules.detect_after_seconds {
            self.reported = sim.credits.clone();
            self.packet_latched = false;
            if !self.solo && self.silence >= rules.end_after_seconds {
                self.phase = AwayPhase::Ended;
            } else {
                self.phase = AwayPhase::Away;
                self.wants_announce = true;
                cues.push("host_away".to_string());
            }
        } else if self.phase == AwayPhase::Away && !self.solo && self.silence >= rules.end_after_seconds {
            self.phase = AwayPhase::Ended;
        }
        self.frame(rules, cues)
    }

    pub fn sample(&self, rules: &HostAwayRules) -> AwayFrame {
        self.frame(rules, Vec::new())
    }

    /// Puts the pre-freeze credit map back, marks defeat, and leaves the host connected.
    pub fn finish(&mut self, sim: &mut Sim) {
        if self.finished || self.phase != AwayPhase::Ended {
            return;
        }
        self.finished = true;
        sim.credits = self.reported.clone();
        sim.host_away = false;
        if !matches!(sim.phase, Phase::Victory | Phase::Defeat) {
            sim.phase = Phase::Defeat;
            sim.music.push(MusicMoment::MatchEnd);
        }
    }

    fn begin_countdown(&mut self, secs: f32) {
        if self.phase == AwayPhase::Countdown {
            return;
        }
        self.phase = AwayPhase::Countdown;
        self.countdown_full = secs;
        self.countdown = secs;
        self.pending.push("host_back".to_string());
    }

    fn frame(&self, rules: &HostAwayRules, cues: Vec<String>) -> AwayFrame {
        let end = rules.end_after_seconds;
        let timer_fill = if end > 0.0 {
            (1.0 - self.silence / end).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (digit, digit_u) = self.digit();
        AwayFrame {
            phase: self.phase,
            banner: self.phase == AwayPhase::Away,
            solo: self.solo,
            timer_fill,
            digit,
            digit_u,
            cues,
            frozen: self.frozen(),
            ended: self.phase == AwayPhase::Ended,
        }
    }

    fn digit(&self) -> (Option<u8>, f32) {
        if self.phase != AwayPhase::Countdown || !(self.countdown_full > 0.0) {
            return (None, 0.0);
        }
        let counts = self.countdown_full.round().max(1.0);
        let consumed = (self.countdown_full - self.countdown).clamp(0.0, self.countdown_full);
        let index = consumed.floor();
        let digit = (counts as i32 - index as i32).max(1) as u8;
        (Some(digit), consumed.fract())
    }
}
