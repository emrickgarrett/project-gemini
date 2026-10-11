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
    /// Continuous live play required before a resumed away timer returns to zero.
    pub away_timer_reset_after_play_seconds: f32,
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
    pub stops: Vec<String>,
    pub frozen: bool,
    pub ended: bool,
}

/// Result of one page-visibility sample. Native occlusion and the browser
/// `visibilitychange` handler both produce this through [`on_host_visibility_changed`].
#[derive(Clone, Debug)]
pub struct VisibilityEffect {
    pub frame: AwayFrame,
    pub send_host_away: bool,
}

#[derive(Clone, Debug)]
pub struct HostAwayClock {
    solo: bool,
    /// Page visibility. A hidden page does not start the resume countdown.
    page_hidden: bool,
    /// Host tab hidden, or this peer has not heard a live snapshot.
    silent: bool,
    silence: f32,
    /// Live, visible play since the countdown ended. Silence clears only after the reset window.
    played: f32,
    /// Absolute seconds of the previous visibility sample. `None` until the first one.
    last_now: Option<f32>,
    countdown: f32,
    countdown_full: f32,
    phase: AwayPhase,
    reported: BTreeMap<u64, Vec<u32>>,
    pending: Vec<String>,
    pending_stops: Vec<String>,
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
            played: 0.0,
            last_now: None,
            countdown: 0.0,
            countdown_full: 0.0,
            phase: AwayPhase::Live,
            reported: BTreeMap::new(),
            pending: Vec::new(),
            pending_stops: Vec::new(),
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

    pub fn silence(&self) -> f32 {
        self.silence
    }

    pub fn set_page_hidden(&mut self, hidden: bool) {
        if hidden && self.phase == AwayPhase::Countdown {
            self.abort_countdown();
        }
        if hidden {
            self.played = 0.0;
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
        let mut dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let mut cues = std::mem::take(&mut self.pending);
        let mut stops = std::mem::take(&mut self.pending_stops);
        if self.phase == AwayPhase::Ended {
            return self.frame(rules, cues, stops);
        }
        if self.phase == AwayPhase::Countdown {
            if self.silent || self.page_hidden {
                cues.push("host_away".to_string());
                stops.push("host_back".to_string());
                self.phase = AwayPhase::Away;
                self.countdown = 0.0;
                self.played = 0.0;
            } else if dt < self.countdown {
                self.countdown -= dt;
                return self.frame(rules, cues, stops);
            } else {
                dt -= self.countdown;
                self.countdown = 0.0;
                self.phase = AwayPhase::Live;
                self.packet_latched = false;
                self.wants_announce = false;
            }
        }
        let covering = self.silent || self.page_hidden;
        if covering {
            self.played = 0.0;
            self.silence += dt;
        } else if self.phase == AwayPhase::Live && self.silence > 0.0 && !self.packet_latched {
            self.played += dt;
            if self.played >= rules.away_timer_reset_after_play_seconds {
                self.silence = 0.0;
                self.played = 0.0;
            }
        }
        let latched = self.silent || self.page_hidden || self.packet_latched;
        if self.phase == AwayPhase::Live && latched && self.silence >= rules.detect_after_seconds {
            self.reported = sim.credits.clone();
            self.packet_latched = false;
            if !self.solo && self.silence >= rules.end_after_seconds {
                self.phase = AwayPhase::Ended;
            } else {
                self.phase = AwayPhase::Away;
                self.wants_announce = true;
                cues.push("host_away".to_string());
            }
        } else if self.phase == AwayPhase::Away && covering && !self.solo && self.silence >= rules.end_after_seconds {
            self.phase = AwayPhase::Ended;
        }
        self.frame(rules, cues, stops)
    }

    pub fn sample(&self, rules: &HostAwayRules) -> AwayFrame {
        self.frame(rules, Vec::new(), Vec::new())
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

    fn abort_countdown(&mut self) {
        self.phase = AwayPhase::Away;
        self.countdown = 0.0;
        self.played = 0.0;
        self.pending.push("host_away".to_string());
        self.pending_stops.push("host_back".to_string());
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

    fn frame(&self, rules: &HostAwayRules, cues: Vec<String>, stops: Vec<String>) -> AwayFrame {
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
            stops,
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

/// One sample of page visibility. `now` is absolute seconds. The interval since
/// the previous sample is applied under the previous hidden state, so a resume
/// hitch counts as silence and then a full countdown starts. A second call with
/// the same `now` does not add time.
pub fn on_host_visibility_changed(
    clock: &mut HostAwayClock,
    hidden: bool,
    now: f32,
    rules: &HostAwayRules,
    sim: &Sim,
) -> VisibilityEffect {
    let now = if now.is_finite() { now } else { clock.last_now.unwrap_or(0.0) };
    let dt = match clock.last_now {
        Some(prev) => (now - prev).max(0.0),
        None => 0.0,
    };
    clock.last_now = Some(now);

    let mut frame = clock.tick(dt, rules, sim);
    if frame.ended {
        return VisibilityEffect {
            send_host_away: clock.take_announce(),
            frame,
        };
    }
    if hidden {
        clock.set_page_hidden(true);
        clock.set_silent(true);
    } else {
        // Announce while still Away, before the countdown leaves that phase.
        let send_host_away = clock.take_announce();
        clock.host_shown(rules.resume_countdown_seconds);
        let extra = clock.tick(0.0, rules, sim);
        extend_frame(&mut frame, extra);
        return VisibilityEffect { frame, send_host_away };
    }
    let extra = clock.tick(0.0, rules, sim);
    extend_frame(&mut frame, extra);
    VisibilityEffect {
        send_host_away: clock.take_announce(),
        frame,
    }
}

fn extend_frame(frame: &mut AwayFrame, extra: AwayFrame) {
    frame.cues.extend(extra.cues);
    frame.stops.extend(extra.stops);
    frame.phase = extra.phase;
    frame.banner = extra.banner;
    frame.solo = extra.solo;
    frame.timer_fill = extra.timer_fill;
    frame.digit = extra.digit;
    frame.digit_u = extra.digit_u;
    frame.frozen = extra.frozen;
    frame.ended = extra.ended;
}
