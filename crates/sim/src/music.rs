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
    /// File-vs-bpm warning window, in milliseconds per loop.
    pub bpm_warn_ms: f32,
    pub pressure_full_at: f32,
    pub progress_floor_cap: f32,
    pub progress_wave: f32,
    pub downed_bump: f32,
    pub breather_db: f32,
    pub hysteresis_bars: u32,
    pub layer_fade_bars: f32,
    pub difficulty_floor: DifficultyBias,
    pub breather_stems: Vec<String>,
}

impl MusicDirectorCfg {
    pub fn bpm_warn_seconds(&self) -> f32 {
        self.bpm_warn_ms / 1000.0
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Transitions {
    pub combat_to_boss: String,
    pub boss_to_combat: String,
    pub boss_loop_to_enrage: String,
    pub boss_loop_to_enrage_beats: f32,
    pub tyrant_phase_change: String,
    pub tyrant_phase_change_beats: f32,
    pub combat_loop_swap: String,
    pub menu_to_combat: String,
    pub menu_to_combat_cue: String,
    pub combat_to_menu: String,
    pub combat_to_menu_cue: String,
    pub same_tempo: String,
    pub different_tempo: String,
}

impl Transitions {
    /// Cue named by `hard_cut_on_<cue>`. Empty when the mode is not a hard cut.
    pub fn cue_of(&self, mode: &str) -> String {
        mode.strip_prefix("hard_cut_on_")
            .unwrap_or("")
            .to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKind {
    MenuToCombat,
    CombatToMenu,
    CombatToBoss,
    BossToCombat,
    BossEnrage,
    BossPhase,
    LoopSwap,
    /// Any other pair. Mode comes from same_tempo / different_tempo.
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlannedSwitch {
    /// Immediate. Boss edges fire on their cue and are never delayed to a bar.
    HardCut { cue: String },
    /// Only produced when both tracks share bpm and beats_per_bar.
    /// `fade_bars` is the outgoing track's fade, in that track's bars.
    CrossfadeOnBar { fade_bars: f32, wait_for_bar: bool },
}

/// Decide how to leave `from` for `to`.
///
/// Combat↔boss follows `[transitions]` and is a hard cut on the cue written
/// there. `crossfade_on_bar` waits for a bar, except boss enrage, which fires
/// on the roar. `crossfade_on_roar` starts the fade immediately. A crossfade
/// is returned only when the two tracks share a tempo. Different tempos hard-cut.
/// `hard_cut`, `hard_cut_on_*`, and `breather_only` are hard cuts. Any other
/// mode is an error.
pub fn known_transition(mode: &str) -> bool {
    mode == "crossfade_on_bar"
        || mode == "crossfade_on_roar"
        || mode == "hard_cut"
        || mode == "breather_only"
        || mode.starts_with("hard_cut_on_")
}

pub fn plan_switch(
    from: &TrackDef,
    to: &TrackDef,
    from_loop_seconds: f32,
    to_loop_seconds: f32,
    kind: TransitionKind,
    transitions: &Transitions,
    warn_seconds: f32,
) -> Result<PlannedSwitch, String> {
    let (mode, cue, fade_bars) = match kind {
        TransitionKind::CombatToBoss => (
            transitions.combat_to_boss.as_str(),
            transitions.cue_of(&transitions.combat_to_boss),
            from.fade_bars,
        ),
        TransitionKind::BossToCombat => (
            transitions.boss_to_combat.as_str(),
            transitions.cue_of(&transitions.boss_to_combat),
            from.fade_bars,
        ),
        TransitionKind::MenuToCombat => (
            transitions.menu_to_combat.as_str(),
            transitions.menu_to_combat_cue.clone(),
            from.fade_bars,
        ),
        TransitionKind::CombatToMenu => (
            transitions.combat_to_menu.as_str(),
            transitions.combat_to_menu_cue.clone(),
            from.fade_bars,
        ),
        TransitionKind::BossEnrage => (
            transitions.boss_loop_to_enrage.as_str(),
            String::new(),
            transitions.boss_loop_to_enrage_beats / from.beats_per_bar.max(1) as f32,
        ),
        TransitionKind::BossPhase => (
            transitions.tyrant_phase_change.as_str(),
            String::new(),
            transitions.tyrant_phase_change_beats / from.beats_per_bar.max(1) as f32,
        ),
        TransitionKind::LoopSwap => (transitions.combat_loop_swap.as_str(), String::new(), from.fade_bars),
        TransitionKind::Other => {
            if same_bar_grid(from, to, from_loop_seconds, to_loop_seconds, warn_seconds) {
                (transitions.same_tempo.as_str(), String::new(), from.fade_bars)
            } else {
                (transitions.different_tempo.as_str(), String::new(), from.fade_bars)
            }
        }
    };

    if !known_transition(mode) {
        return Err(format!("unknown transition mode \"{mode}\""));
    }
    let tempos_ok = same_bar_grid(from, to, from_loop_seconds, to_loop_seconds, warn_seconds);
    // Enrage is authored as crossfade_on_bar but fires on the roar, so it does not wait.
    let wait_for_bar = mode == "crossfade_on_bar" && !matches!(kind, TransitionKind::BossEnrage);
    let crossfade = mode == "crossfade_on_bar" || mode == "crossfade_on_roar";
    if crossfade && tempos_ok {
        Ok(PlannedSwitch::CrossfadeOnBar { fade_bars, wait_for_bar })
    } else {
        Ok(PlannedSwitch::HardCut { cue })
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

/// `pressure = alive_cost / (wave_budget * pressure_full_at)`, then
/// `max(pressure, progress floor)` plus the downed bump, clamped to the difficulty floor.
pub fn combat_intensity(
    director: &MusicDirectorCfg,
    difficulty: &str,
    wave: u32,
    endless: bool,
    alive_cost: f32,
    wave_budget: f32,
    player_downed: bool,
) -> f32 {
    let full = director.pressure_full_at.max(f32::EPSILON);
    let pressure = if wave_budget <= f32::EPSILON {
        0.0
    } else {
        alive_cost / (wave_budget * full)
    };
    let wave_span = director.progress_wave.max(1.0);
    let progress = if endless {
        director.progress_floor_cap
    } else {
        director.progress_floor_cap * (wave as f32 / wave_span).min(1.0)
    };
    let mut value = pressure.max(progress);
    if player_downed {
        value += director.downed_bump;
    }
    value.clamp(director.difficulty_floor.for_id(difficulty), 1.0)
}

pub fn layer_targets(track: &TrackDef, intensity: f32, breather: Option<(&[String], f32)>) -> Vec<f32> {
    if let Some((stems, gain)) = breather {
        return track
            .layer
            .iter()
            .map(|layer| if stems.iter().any(|id| id == &layer.id) { gain } else { 0.0 })
            .collect();
    }
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
    /// Bars the desired gain has held. A layer changes only after hysteresis.
    pub held_bars: u32,
    pub pending: f32,
}

/// Stem gains for one track. Each voice keeps the decoded length of its own file.
#[derive(Clone, Debug)]
pub struct StemMixer {
    pub voices: Vec<StemVoice>,
}

impl StemMixer {
    pub fn start(track: &TrackDef, loop_seconds: &[f32], intensity: f32) -> Self {
        let target = layer_targets(track, intensity, None);
        let voices = target
            .iter()
            .enumerate()
            .map(|(i, gain)| StemVoice {
                elapsed: 0.0,
                loop_seconds: loop_seconds.get(i).copied().unwrap_or(0.0),
                current: *gain,
                target: *gain,
                held_bars: 0,
                pending: *gain,
            })
            .collect();
        Self { voices }
    }

    /// Intensity changes latch on each stem's own file-derived bar line,
    /// after `hysteresis_bars` of the same request, then slew across `fade_bars`.
    pub fn advance(
        &mut self,
        track: &TrackDef,
        dt: f32,
        intensity: f32,
        hysteresis_bars: u32,
        breather: Option<(&[String], f32)>,
    ) {
        let desired = layer_targets(track, intensity, breather);
        let need = hysteresis_bars.max(1);
        for (voice, want) in self.voices.iter_mut().zip(desired) {
            let clock = FileClock::measure(track, voice.loop_seconds);
            let prev = voice.elapsed;
            voice.elapsed += dt;
            if (want - voice.pending).abs() > 1e-4 {
                voice.pending = want;
                voice.held_bars = 0;
            }
            if clock.crossed_bar(prev, voice.elapsed) {
                voice.held_bars = voice.held_bars.saturating_add(1);
                if voice.held_bars >= need {
                    voice.target = voice.pending;
                }
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
    /// New track starts at elapsed 0. Used for hard cuts (boss roar / death sting / breather swap).
    Reset,
    /// Wait until `from` crosses a bar, then start the destination.
    WaitForBar,
    /// Start the destination now and slew across `fade_bars` (enrage roar, tyrant phase).
    FadeNow,
}

pub fn switch_clock(plan: &PlannedSwitch) -> SwitchClock {
    match plan {
        PlannedSwitch::HardCut { .. } => SwitchClock::Reset,
        PlannedSwitch::CrossfadeOnBar { wait_for_bar, .. } => {
            if *wait_for_bar {
                SwitchClock::WaitForBar
            } else {
                SwitchClock::FadeNow
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct MusicRotation {
    pub loops: Vec<String>,
    pub slice_early: String,
    pub slice_late: String,
    pub endless_every: u32,
}

#[derive(Clone, Debug)]
pub struct LoadedMusic {
    pub director: MusicDirectorCfg,
    pub track: std::collections::BTreeMap<String, TrackDef>,
    pub transitions: Transitions,
    pub rotation: MusicRotation,
}

pub fn load_music(text: &str) -> Result<LoadedMusic, String> {
    #[derive(Deserialize)]
    struct MusicSrc {
        pressure_full_at: f32,
        progress_floor_cap: f32,
        progress_wave: f32,
        #[serde(default)]
        layer: Vec<LayerSrc>,
        overrides: OverrideSrc,
        rotation: RotationSrc,
        director: DirectorSrc,
        track: std::collections::BTreeMap<String, TrackSrc>,
        transitions: Transitions,
    }
    #[derive(Deserialize)]
    struct LayerSrc {
        stem: String,
        on_at: f32,
    }
    #[derive(Deserialize)]
    struct OverrideSrc {
        breather: Vec<String>,
        breather_db: f32,
        player_downed_any: f32,
        hysteresis_bars: u32,
        difficulty_floor: DifficultyBias,
    }
    #[derive(Deserialize)]
    struct RotationSrc {
        loops: Vec<String>,
        slice_waves_1_5: String,
        slice_waves_6_10: String,
        endless_rotate_every_waves: u32,
    }
    #[derive(Deserialize)]
    struct DirectorSrc {
        bpm_warn_ms: f32,
        layer_fade_bars: f32,
    }
    #[derive(Deserialize)]
    struct TrackSrc {
        path: String,
        bpm: f32,
        beats_per_bar: u32,
        loop_bars: u32,
        #[serde(default)]
        role: String,
        #[serde(default)]
        boss_id: Option<String>,
        #[serde(default)]
        phases: Vec<String>,
        #[serde(default)]
        fade_bars: f32,
    }

    let src: MusicSrc = toml::from_str(text).map_err(|err| err.to_string())?;
    let mut track = std::collections::BTreeMap::new();
    for (id, row) in src.track {
        let fade = if row.fade_bars > 0.0 {
            row.fade_bars
        } else {
            src.director.layer_fade_bars
        };
        if !row.phases.is_empty() {
            let boss_id = row
                .boss_id
                .clone()
                .filter(|name| !name.is_empty())
                .ok_or_else(|| format!("{id} is missing boss_id"))?;
            let dir = audio_path(&row.path);
            for (i, phase) in row.phases.iter().enumerate() {
                let key = format!("{id}_phase{}", i + 1);
                let role = if i == 0 { "boss" } else { "phase" };
                track.insert(
                    key,
                    TrackDef {
                        bpm: row.bpm,
                        beats_per_bar: row.beats_per_bar,
                        loop_bars: row.loop_bars,
                        fade_bars: fade,
                        role: role.into(),
                        boss_id: Some(boss_id.clone()),
                        path: Some(format!("{dir}{phase}")),
                        layer: Vec::new(),
                    },
                );
            }
            continue;
        }
        if row.role.is_empty() {
            return Err(format!("{id} is missing role"));
        }
        let role = row.role.clone();
        let boss_id = if matches!(role.as_str(), "boss" | "enrage" | "phase") {
            Some(
                row.boss_id
                    .clone()
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| format!("{id} is missing boss_id"))?,
            )
        } else {
            row.boss_id.clone().filter(|name| !name.is_empty())
        };
        let layered = row.path.ends_with('/');
        let layer = if layered {
            src.layer
                .iter()
                .map(|stem| MusicLayer {
                    id: stem.stem.clone(),
                    path: format!("{}{}.ogg", audio_path(&row.path), stem.stem),
                    min_intensity: stem.on_at,
                })
                .collect()
        } else {
            Vec::new()
        };
        let path = if layered { None } else { Some(audio_path(&row.path)) };
        track.insert(
            id,
            TrackDef {
                bpm: row.bpm,
                beats_per_bar: row.beats_per_bar,
                loop_bars: row.loop_bars,
                fade_bars: fade,
                role,
                boss_id,
                path,
                layer,
            },
        );
    }
    if src.rotation.loops.is_empty() {
        return Err("rotation loops is empty".into());
    }
    if src.rotation.endless_rotate_every_waves == 0 {
        return Err("endless_rotate_every_waves is 0".into());
    }
    for id in [&src.rotation.slice_waves_1_5, &src.rotation.slice_waves_6_10] {
        if id.is_empty() || !track.contains_key(id.as_str()) {
            return Err(format!("rotation names unknown track \"{id}\""));
        }
    }
    for id in &src.rotation.loops {
        if !track.contains_key(id) {
            return Err(format!("rotation loop \"{id}\" is not a track"));
        }
    }
    for mode in [
        src.transitions.combat_to_boss.as_str(),
        src.transitions.boss_to_combat.as_str(),
        src.transitions.boss_loop_to_enrage.as_str(),
        src.transitions.tyrant_phase_change.as_str(),
        src.transitions.combat_loop_swap.as_str(),
        src.transitions.menu_to_combat.as_str(),
        src.transitions.combat_to_menu.as_str(),
        src.transitions.same_tempo.as_str(),
        src.transitions.different_tempo.as_str(),
    ] {
        if !known_transition(mode) {
            return Err(format!("unknown transition mode \"{mode}\""));
        }
    }
    Ok(LoadedMusic {
        director: MusicDirectorCfg {
            bpm_warn_ms: src.director.bpm_warn_ms,
            pressure_full_at: src.pressure_full_at,
            progress_floor_cap: src.progress_floor_cap,
            progress_wave: src.progress_wave,
            downed_bump: src.overrides.player_downed_any,
            breather_db: src.overrides.breather_db,
            hysteresis_bars: src.overrides.hysteresis_bars,
            layer_fade_bars: src.director.layer_fade_bars,
            difficulty_floor: src.overrides.difficulty_floor,
            breather_stems: src.overrides.breather,
        },
        track,
        transitions: src.transitions,
        rotation: MusicRotation {
            loops: src.rotation.loops,
            slice_early: src.rotation.slice_waves_1_5,
            slice_late: src.rotation.slice_waves_6_10,
            endless_every: src.rotation.endless_rotate_every_waves,
        },
    })
}

fn audio_path(path: &str) -> String {
    if path.starts_with("audio/") {
        path.to_string()
    } else {
        format!("audio/{path}")
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
        mixer.advance(&track, 1.9, 1.0, 1, None);
        // 8s file / 4 bars = 2.0s; 1.9s has not crossed a bar.
        assert!((mixer.voices[0].target - 0.0).abs() < 1e-4);
        // 4s file / 4 bars = 1.0s; that stem latched on its own grid.
        assert!((mixer.voices[1].target - 1.0).abs() < 1e-4);
        mixer.advance(&track, 0.2, 1.0, 1, None);
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
        let plan = plan.expect("mode");
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
        assert!(matches!(same.expect("mode"), PlannedSwitch::CrossfadeOnBar { .. }));
        let drift = plan_switch(
            &a,
            &b,
            20.0,
            20.05,
            TransitionKind::Other,
            &content.music.transitions,
            warn,
        );
        assert!(matches!(drift.expect("mode"), PlannedSwitch::HardCut { .. }));
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
        assert!(matches!(cut.expect("mode"), PlannedSwitch::HardCut { .. }));
    }

    #[test]
    fn embedded_tracks_have_distinct_clocks() {
        let music = &Content::embedded().music;
        let combat = music.track.get("combat_01").unwrap();
        let brood = music.track.get("boss_brood_loop").unwrap();
        let theme = music.track.get("theme_main").unwrap();
        assert!((combat.bpm - 141.58).abs() < 1e-3);
        assert!((brood.bpm - 137.67).abs() < 1e-3);
        assert_eq!(combat.loop_bars, 16);
        assert_eq!(brood.loop_bars, 16);
        assert!(theme.bpm != combat.bpm);
        assert_ne!(combat.bpm, brood.bpm);
        assert_eq!(music.transitions.combat_to_boss, "hard_cut_on_boss_roar");
        assert_eq!(music.transitions.cue_of(&music.transitions.combat_to_boss), "boss_roar");
        assert_eq!(music.transitions.cue_of(&music.transitions.boss_to_combat), "death_sting");
        assert!(music.track.contains_key("combat_02"));
        assert!(music.track.contains_key("boss_tyrant_phase1"));
    }
}
