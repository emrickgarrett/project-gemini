//! Per-track music clocks. There is no shared tempo in this module:
//! every bar, loop, and fade is computed from the track you pass in.

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct MusicLayer {
    pub id: String,
    pub path: String,
    pub min_intensity: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TrackDef {
    pub bpm: f32,
    pub beats_per_bar: u32,
    pub loop_bars: u32,
    pub fade_bars: f32,
    pub role: String,
    #[serde(default)]
    pub boss_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub layer: Vec<MusicLayer>,
}

impl TrackDef {
    /// Authored loop length. Compared with the decoded file; never used as the clock.
    pub fn bpm_loop_seconds(&self) -> f32 {
        if self.bpm <= f32::EPSILON {
            return 0.0;
        }
        self.loop_bars as f32 * self.beats_per_bar as f32 * 60.0 / self.bpm
    }

    pub fn shares_tempo_with(&self, other: &TrackDef) -> bool {
        self.bpm == other.bpm && self.beats_per_bar == other.beats_per_bar
    }
}

/// Bar grid taken from a decoded stem (or single-file loop).
/// `bar = loop_seconds / loop_bars`. Bpm is not an input.
#[derive(Clone, Copy, Debug)]
pub struct FileClock {
    pub loop_bars: u32,
    pub loop_seconds: f32,
    pub fade_bars: f32,
}

impl FileClock {
    pub fn measure(track: &TrackDef, loop_seconds: f32) -> Self {
        Self {
            loop_bars: track.loop_bars,
            loop_seconds,
            fade_bars: track.fade_bars,
        }
    }

    pub fn bar_seconds(&self) -> f32 {
        if self.loop_bars == 0 {
            0.0
        } else {
            self.loop_seconds / self.loop_bars as f32
        }
    }

    pub fn fade_seconds(&self) -> f32 {
        self.bar_seconds() * self.fade_bars
    }

    pub fn bar_index(&self, elapsed: f32) -> u32 {
        let bar = self.bar_seconds();
        if bar <= f32::EPSILON || elapsed <= 0.0 {
            return 0;
        }
        (elapsed / bar).floor() as u32
    }

    pub fn crossed_bar(&self, prev_elapsed: f32, elapsed: f32) -> bool {
        self.bar_index(elapsed) != self.bar_index(prev_elapsed)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BpmMismatch {
    pub stem_id: String,
    pub file_loop_seconds: f32,
    pub bpm_loop_seconds: f32,
    pub error_seconds: f32,
}

/// `warn_seconds` is the director's `bpm_warn_ms` converted by the caller.
/// A mismatch does not change the clock; the file length still wins.
pub fn bpm_sanity(
    stem_id: &str,
    track: &TrackDef,
    file_loop_seconds: f32,
    warn_seconds: f32,
) -> Option<BpmMismatch> {
    let bpm_loop = track.bpm_loop_seconds();
    let error = (bpm_loop - file_loop_seconds).abs();
    if error > warn_seconds {
        Some(BpmMismatch {
            stem_id: stem_id.to_string(),
            file_loop_seconds,
            bpm_loop_seconds: bpm_loop,
            error_seconds: error,
        })
    } else {
        None
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct DifficultyBias {
    pub normal: f32,
    pub hard: f32,
    pub heroic: f32,
    pub legendary: f32,
}

impl DifficultyBias {
    pub fn for_id(&self, id: &str) -> f32 {
        match id {
            "normal" => self.normal,
            "hard" => self.hard,
            "heroic" => self.heroic,
            "legendary" => self.legendary,
            _ => 0.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct MusicDirectorCfg {
    pub intensity_per_wave: f32,
    pub boss_intensity_bonus: f32,
    pub intensity_cap: f32,
    /// File-vs-bpm warning window, in milliseconds per loop.
    pub bpm_warn_ms: f32,
    pub difficulty_bias: DifficultyBias,
}

impl MusicDirectorCfg {
    pub fn bpm_warn_seconds(&self) -> f32 {
        self.bpm_warn_ms / 1000.0
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Transitions {
    pub combat_to_boss: String,
    pub combat_to_boss_cue: String,
    pub boss_to_combat: String,
    pub boss_to_combat_cue: String,
    pub menu_to_combat: String,
    pub menu_to_combat_cue: String,
    pub combat_to_menu: String,
    pub combat_to_menu_cue: String,
    pub same_tempo: String,
    pub different_tempo: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKind {
    MenuToCombat,
    CombatToMenu,
    CombatToBoss,
    BossToCombat,
    /// Any other pair. Mode comes from same_tempo / different_tempo.
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlannedSwitch {
    /// Immediate. Boss edges fire on their cue and are never delayed to a bar.
    HardCut { cue: String },
    /// Only produced when both tracks share bpm and beats_per_bar.
    /// `fade_bars` is the outgoing track's fade, in that track's bars.
    CrossfadeOnBar { fade_bars: f32 },
}

/// Decide how to leave `from` for `to`.
///
/// Combat↔boss follows `[transitions]` and is a hard cut on the cue written
/// there. A crossfade is returned only when the requested mode is a bar-line
/// crossfade AND the two tracks share a tempo. Different tempos are never
/// beat-matched; they hard-cut instead.
pub fn plan_switch(
    from: &TrackDef,
    to: &TrackDef,
    from_loop_seconds: f32,
    to_loop_seconds: f32,
    kind: TransitionKind,
    transitions: &Transitions,
    warn_seconds: f32,
) -> PlannedSwitch {
    let (mode, cue) = match kind {
        TransitionKind::CombatToBoss => (
            transitions.combat_to_boss.as_str(),
            transitions.combat_to_boss_cue.clone(),
        ),
        TransitionKind::BossToCombat => (
            transitions.boss_to_combat.as_str(),
            transitions.boss_to_combat_cue.clone(),
        ),
        TransitionKind::MenuToCombat => (
            transitions.menu_to_combat.as_str(),
            transitions.menu_to_combat_cue.clone(),
        ),
        TransitionKind::CombatToMenu => (
            transitions.combat_to_menu.as_str(),
            transitions.combat_to_menu_cue.clone(),
        ),
        TransitionKind::Other => {
            if same_bar_grid(from, to, from_loop_seconds, to_loop_seconds, warn_seconds) {
                (transitions.same_tempo.as_str(), String::new())
            } else {
                (transitions.different_tempo.as_str(), String::new())
            }
        }
    };

    let wants_crossfade = mode == "crossfade_on_bar";
    if wants_crossfade && same_bar_grid(from, to, from_loop_seconds, to_loop_seconds, warn_seconds)
    {
        PlannedSwitch::CrossfadeOnBar {
            fade_bars: from.fade_bars,
        }
    } else {
        PlannedSwitch::HardCut { cue }
    }
}

/// Same authored tempo and file loops that agree within the warn window.
/// Bar lines themselves still come from each file, not from bpm.
fn same_bar_grid(
    from: &TrackDef,
    to: &TrackDef,
    from_loop_seconds: f32,
    to_loop_seconds: f32,
    warn_seconds: f32,
) -> bool {
    from.shares_tempo_with(to) && (from_loop_seconds - to_loop_seconds).abs() <= warn_seconds
}

pub fn combat_intensity(
    director: &MusicDirectorCfg,
    difficulty: &str,
    wave: u32,
    boss_alive: bool,
) -> f32 {
    let mut v = director.difficulty_bias.for_id(difficulty)
        + director.intensity_per_wave * wave.saturating_sub(1) as f32;
    if boss_alive {
        v += director.boss_intensity_bonus;
    }
    v.clamp(0.0, director.intensity_cap)
}

pub fn layer_targets(track: &TrackDef, intensity: f32) -> Vec<f32> {
    track
        .layer
        .iter()
        .map(|layer| {
            if intensity + f32::EPSILON >= layer.min_intensity {
                1.0
            } else {
                0.0
            }
        })
        .collect()
}

/// One playing stem. Its bar lines are `loop_seconds / loop_bars` of *this* file.
#[derive(Clone, Debug)]
pub struct StemVoice {
    pub elapsed: f32,
    pub loop_seconds: f32,
    pub current: f32,
    pub target: f32,
}

/// Stem gains for one track. Each voice keeps the decoded length of its own file.
#[derive(Clone, Debug)]
pub struct StemMixer {
    pub voices: Vec<StemVoice>,
}

impl StemMixer {
    pub fn start(track: &TrackDef, loop_seconds: &[f32], intensity: f32) -> Self {
        let target = layer_targets(track, intensity);
        let voices = target
            .iter()
            .enumerate()
            .map(|(i, gain)| StemVoice {
                elapsed: 0.0,
                loop_seconds: loop_seconds.get(i).copied().unwrap_or(0.0),
                current: *gain,
                target: *gain,
            })
            .collect();
        Self { voices }
    }

    /// Intensity changes latch on each stem's own file-derived bar line,
    /// then slew across `fade_bars` of that same bar.
    pub fn advance(&mut self, track: &TrackDef, dt: f32, intensity: f32) {
        let desired = layer_targets(track, intensity);
        for (voice, want) in self.voices.iter_mut().zip(desired) {
            let clock = FileClock::measure(track, voice.loop_seconds);
            let prev = voice.elapsed;
            voice.elapsed += dt;
            if clock.crossed_bar(prev, voice.elapsed) {
                voice.target = want;
            }
            let fade = clock.fade_seconds().max(1e-4);
            let step = dt / fade;
            let delta = voice.target - voice.current;
            if delta.abs() <= step {
                voice.current = voice.target;
            } else {
                voice.current += delta.signum() * step;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchClock {
    /// New track starts at elapsed 0. Used for hard cuts (boss roar / death sting).
    Reset,
    /// Wait until `from` crosses a bar, then start the destination.
    WaitForBar,
}

pub fn switch_clock(plan: &PlannedSwitch) -> SwitchClock {
    match plan {
        PlannedSwitch::HardCut { .. } => SwitchClock::Reset,
        PlannedSwitch::CrossfadeOnBar { .. } => SwitchClock::WaitForBar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Content;

    fn track(bpm: f32, bars: u32) -> TrackDef {
        TrackDef {
            bpm,
            beats_per_bar: 4,
            loop_bars: bars,
            fade_bars: 1.0,
            role: "combat".into(),
            boss_id: None,
            path: None,
            layer: vec![
                MusicLayer {
                    id: "drums".into(),
                    path: "a".into(),
                    min_intensity: 0.0,
                },
                MusicLayer {
                    id: "lead".into(),
                    path: "b".into(),
                    min_intensity: 0.5,
                },
            ],
        }
    }

    #[test]
    fn bars_follow_the_file_not_bpm() {
        let track = track(141.58, 16);
        let file_loop = 27.0;
        let clock = FileClock::measure(&track, file_loop);
        let from_file = file_loop / 16.0;
        let from_bpm = track.bpm_loop_seconds() / 16.0;
        assert!((clock.bar_seconds() - from_file).abs() < 1e-5);
        assert!((from_file - from_bpm).abs() > 0.001);
        let warn = Content::embedded().music.director.bpm_warn_seconds();
        assert!(bpm_sanity("drums", &track, file_loop, warn).is_some());
        let close = track.bpm_loop_seconds() + 0.001;
        assert!(bpm_sanity("drums", &track, close, warn).is_none());
        assert!((FileClock::measure(&track, close).bar_seconds() - close / 16.0).abs() < 1e-5);
    }

    #[test]
    fn stems_use_their_own_file_lengths() {
        let mut track = track(141.58, 4);
        // Both layers start silent so a bar latch is visible. A min of 0 would
        // already be on at intensity 0 (the combat drums layer).
        track.layer[0].min_intensity = 0.2;
        track.layer[1].min_intensity = 0.2;
        let mut mixer = StemMixer::start(&track, &[8.0, 4.0], 0.0);
        mixer.advance(&track, 1.9, 1.0);
        // 8s file / 4 bars = 2.0s; 1.9s has not crossed a bar.
        assert!((mixer.voices[0].target - 0.0).abs() < 1e-4);
        // 4s file / 4 bars = 1.0s; that stem latched on its own grid.
        assert!((mixer.voices[1].target - 1.0).abs() < 1e-4);
        mixer.advance(&track, 0.2, 1.0);
        assert!((mixer.voices[0].target - 1.0).abs() < 1e-4);
    }

    #[test]
    fn boss_edges_hard_cut_even_if_tempos_match() {
        let content = Content::embedded();
        let combat = content.combat_track().clone();
        let mut boss = content.boss_track("brood_mother").unwrap().clone();
        boss.bpm = combat.bpm;
        boss.beats_per_bar = combat.beats_per_bar;
        let warn = content.music.director.bpm_warn_seconds();
        let plan = plan_switch(
            &combat,
            &boss,
            10.0,
            10.0,
            TransitionKind::CombatToBoss,
            &content.music.transitions,
            warn,
        );
        assert!(matches!(plan, PlannedSwitch::HardCut { .. }));
        assert_eq!(switch_clock(&plan), SwitchClock::Reset);
    }

    #[test]
    fn crossfade_only_when_tempo_and_files_agree() {
        let content = Content::embedded();
        let a = track(120.0, 8);
        let b = track(120.0, 8);
        let warn = content.music.director.bpm_warn_seconds();
        let same = plan_switch(
            &a,
            &b,
            20.0,
            20.0,
            TransitionKind::Other,
            &content.music.transitions,
            warn,
        );
        assert!(matches!(same, PlannedSwitch::CrossfadeOnBar { .. }));
        let drift = plan_switch(
            &a,
            &b,
            20.0,
            20.05,
            TransitionKind::Other,
            &content.music.transitions,
            warn,
        );
        assert!(matches!(drift, PlannedSwitch::HardCut { .. }));
        let other_bpm = track(137.67, 8);
        let cut = plan_switch(
            &a,
            &other_bpm,
            20.0,
            20.0,
            TransitionKind::Other,
            &content.music.transitions,
            warn,
        );
        assert!(matches!(cut, PlannedSwitch::HardCut { .. }));
    }

    #[test]
    fn embedded_tracks_have_distinct_clocks() {
        let music = &Content::embedded().music;
        let combat = music.track.get("combat_01").unwrap();
        let brood = music.track.get("boss_brood").unwrap();
        let theme = music.track.get("theme_main").unwrap();
        assert!((combat.bpm - 141.58).abs() < 1e-3);
        assert!((brood.bpm - 137.67).abs() < 1e-3);
        assert_eq!(combat.loop_bars, 16);
        assert_eq!(brood.loop_bars, 16);
        assert!(theme.bpm != combat.bpm);
        assert_ne!(combat.bpm, brood.bpm);
        assert_eq!(music.transitions.combat_to_boss, "hard_cut");
        assert_eq!(music.transitions.combat_to_boss_cue, "boss_roar");
        assert_eq!(music.transitions.boss_to_combat_cue, "death_sting");
    }
}
