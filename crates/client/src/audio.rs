//! Stems share a start instant. Bar lines come from each file's decoded length.
//! Sfx playback volume is the `volume` column. Charge stops drop that owner's voice
//! in the same frame the sim emits the stop.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bevy::audio::{
    AddAudioSource, AudioSinkPlayback, AudioSource, Decodable, GlobalVolume, Sample, SeekError, Source, Volume,
};
use bevy::prelude::*;

use gemini_sim::audio_mix::{select_voices, SoundEvent};
use gemini_sim::content::Content;
use gemini_sim::math::Vec3 as SimVec;
use gemini_sim::music::{
    away_mix, bpm_sanity, combat_intensity, plan_switch, switch_clock, FileClock, Lowpass, MusicDirectorCfg,
    PlannedSwitch, StemMixer, SwitchClock, TrackDef, TransitionKind,
};
use gemini_sim::ogg::vorbis_seconds;
use gemini_sim::sim::MusicMoment;

use crate::session::{Away, FrameFx, Game, PauseMenu};
use crate::settings::Prefs;

#[derive(Component)]
pub(crate) struct MusicStem {
    index: usize,
}

#[derive(Component)]
pub(crate) struct StemFile(Handle<AudioSource>);

/// Shared cutoff. The playing decoder reads it every sample, so the low-pass
/// moves without respawning the stem or seeking back to zero.
struct FilterParams {
    cutoff_bits: AtomicU32,
}

impl FilterParams {
    fn open() -> Self {
        Self {
            cutoff_bits: AtomicU32::new(f32::INFINITY.to_bits()),
        }
    }

    fn set_hz(&self, hz: f32) {
        self.cutoff_bits.store(hz.to_bits(), Ordering::Relaxed);
    }

    fn hz(&self) -> f32 {
        f32::from_bits(self.cutoff_bits.load(Ordering::Relaxed))
    }
}

#[derive(Clone)]
struct MusicFilter(Arc<FilterParams>);

impl Default for MusicFilter {
    fn default() -> Self {
        Self(Arc::new(FilterParams::open()))
    }
}

impl Default for FilterParams {
    fn default() -> Self {
        Self::open()
    }
}

#[derive(Asset, TypePath, Clone)]
pub(crate) struct DuckedMusic {
    bytes: Arc<[u8]>,
    filter: Arc<FilterParams>,
}

pub(crate) struct LiveLowpass<I> {
    inner: I,
    filter: Arc<FilterParams>,
    bands: [Lowpass; 8],
    channel: u16,
}

impl<I: Source> Iterator for LiveLowpass<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        let channels = self.inner.channels().get() as usize;
        let idx = (self.channel as usize) % self.bands.len().min(channels).max(1);
        self.channel = self.channel.wrapping_add(1) % channels as u16;
        let rate = self.inner.sample_rate().get() as f32;
        Some(self.bands[idx].process(sample, self.filter.hz(), rate))
    }
}

impl<I: Source> Source for LiveLowpass<I> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> bevy::audio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> bevy::audio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.bands = [Lowpass::new(); 8];
        self.channel = 0;
        self.inner.try_seek(pos)
    }
}

impl Decodable for DuckedMusic {
    type Decoder = LiveLowpass<<AudioSource as Decodable>::Decoder>;

    fn decoder(&self) -> Self::Decoder {
        let inner = AudioSource {
            bytes: Arc::clone(&self.bytes),
        }
        .decoder();
        LiveLowpass {
            inner,
            filter: Arc::clone(&self.filter),
            bands: [Lowpass::new(); 8],
            channel: 0,
        }
    }
}

#[derive(Component)]
pub(crate) struct OneShot {
    id: String,
    owner: u32,
    /// Seconds when this voice started. The steal tie-break uses this, not query order.
    started: f32,
    /// Monotonic stamp so two voices that share a timestamp still have an order.
    order: u32,
}

#[derive(Component)]
pub(crate) struct CrowdVoice;

#[derive(Resource)]
pub struct Music {
    pub track: String,
    pub mixer: Option<StemMixer>,
    pub loops: Vec<f32>,
    pub entities: Vec<Entity>,
    pub pending: Option<String>,
    /// One-beat overlap. Outgoing stems ramp down while the new track ramps up.
    pub fade_left: f32,
    pub fade_dur: f32,
    /// Linear gain on top of the stem mix. 1 while live. Eases back from the away duck.
    pub duck_gain: f32,
    /// Master times the music slider. Effects never touch this bus.
    pub user_gain: f32,
    filter: MusicFilter,
}

impl Default for Music {
    fn default() -> Self {
        Self {
            track: String::new(),
            mixer: None,
            loops: Vec::new(),
            entities: Vec::new(),
            pending: None,
            fade_left: 0.0,
            fade_dur: 0.0,
            duck_gain: 1.0,
            user_gain: 1.0,
            filter: MusicFilter::default(),
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct Crowd {
    volume: f32,
    entity: Option<Entity>,
}

pub fn boot(mut commands: Commands, assets: Res<AssetServer>, game: Res<Game>) {
    let mut music = Music::default();
    // Menu already started the theme. MatchStart hard-cuts into combat.
    start_track(&mut commands, &assets, &game.content, &mut music, "theme_main");
    let moments = game.sim.music.clone();
    apply_moments(&mut commands, &assets, &game.content, &mut music, &moments, &game.sim);
    commands.insert_resource(music);
    commands.insert_resource(Crowd::default());
}

pub fn play(
    mut commands: Commands,
    assets: Res<AssetServer>,
    game: Res<Game>,
    away: Res<Away>,
    fx: Res<FrameFx>,
    mut music: ResMut<Music>,
    mut crowd: ResMut<Crowd>,
    shots: Query<(Entity, &OneShot), Without<MusicStem>>,
    time: Res<Time>,
    prefs: Res<Prefs>,
    hold: Res<PauseMenu>,
) {
    let hud = &game.content.presentation.hud.away;
    let mix = away_mix(away.view.look, hud.music_duck_db, hud.music_duck_lowpass_hz);
    let solo = game.sim.connected_count() <= 1;
    if hold.open && solo {
        // Same duck and low-pass as the host-away banner.
        music.duck_gain = db_to_gain(hud.solo_pause_duck_db);
        music.filter.0.set_hz(hud.music_duck_lowpass_hz);
    } else if hold.open {
        // This client only. Game sounds stay full; music ducks without the low-pass.
        music.duck_gain = mix.gain.min(db_to_gain(hud.coop_pause_duck_db));
        music.filter.0.set_hz(mix.lowpass_hz);
    } else {
        music.duck_gain = mix.gain;
        music.filter.0.set_hz(mix.lowpass_hz);
    }
    music.user_gain = bus_gain(&prefs, true);
    let listener = listener_of(&game);
    if !fx.moments.is_empty() {
        apply_moments(&mut commands, &assets, &game.content, &mut music, &fx.moments, &game.sim);
    }
    advance(&game, &mut music, time.delta_secs());
    if let Some(target) = music.pending.clone() {
        if bar_crossed(&music) {
            start_track(&mut commands, &assets, &game.content, &mut music, &target);
            music.pending = None;
        }
    }

    let selected = select_voices(&game.content.audio, &fx.sounds, listener);
    let mut crowd_vol = 0.0;
    let mut shot_i = 0u32;
    for voice in &selected {
        let Some(def) = game.content.sfx(&voice.id) else { continue };
        if voice.crowd {
            let fullness = if def.loudness > 0.0 { voice.gain / def.loudness } else { 1.0 };
            crowd_vol = def.volume * fullness * bus_gain(&prefs, false);
            continue;
        }
        let mut speed = 1.0;
        if let Some((min, max)) = game.content.pitch_range(&voice.id) {
            let h = (fx.tick as u32).wrapping_mul(17).wrapping_add(shot_i.wrapping_mul(13));
            let t = (h % 1000) as f32 / 1000.0;
            speed = min + (max - min) * t;
            shot_i += 1;
        }
        let atten = if def.flat { 1.0 } else { attenuate(&game.content, voice.pos, listener) };
        let volume = def.volume * atten * bus_gain(&prefs, false);
        let looping = def.looped;
        let settings = if looping {
            PlaybackSettings::LOOP
        } else {
            PlaybackSettings::DESPAWN
        }
        .with_volume(Volume::Linear(volume))
        .with_speed(speed);
        let entity = commands
            .spawn((
                AudioPlayer::new(assets.load::<AudioSource>(def.path.clone())),
                settings,
                OneShot {
                    id: voice.id.clone(),
                    owner: owner_of(&fx.sounds, &voice.id, voice.pos),
                    started: time.elapsed_secs(),
                    order: next_voice_order(),
                },
            ))
            .id();
        let owner = owner_of(&fx.sounds, &voice.id, voice.pos);
        if fx.stops.iter().any(|s| s.id == voice.id && s.owner == owner) {
            commands.entity(entity).despawn();
        }
    }
    for (entity, shot) in &shots {
        if fx.stops.iter().any(|s| s.id == shot.id && s.owner == shot.owner) {
            commands.entity(entity).despawn();
        }
    }
    crowd.volume = crowd_vol;
    if crowd.entity.is_none() {
        if let Some(def) = game.content.sfx("swarmer_crowd") {
            let entity = commands
                .spawn((
                    AudioPlayer::new(assets.load::<AudioSource>(def.path.clone())),
                    PlaybackSettings::LOOP.with_volume(Volume::Linear(crowd_vol)),
                    CrowdVoice,
                ))
                .id();
            crowd.entity = Some(entity);
        }
    }
}

pub fn sync_crowd(crowd: Res<Crowd>, mut sinks: Query<&mut AudioSink, With<CrowdVoice>>) {
    for mut sink in &mut sinks {
        sink.set_volume(Volume::Linear(crowd.volume));
    }
}

pub fn footstep(commands: &mut Commands, assets: &AssetServer, game: &Game, id: &str, pos: SimVec, prefs: &Prefs) {
    let Some(def) = game.content.sfx(id) else { return };
    let listener = listener_of(game);
    let volume = def.volume * attenuate(&game.content, pos, listener) * bus_gain(prefs, false);
    commands.spawn((
        AudioPlayer::new(assets.load::<AudioSource>(def.path.clone())),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
    ));
}

/// Master scales every bus. Music scales stems. Effects scales every other mix category.
pub fn bus_gain(prefs: &Prefs, music: bool) -> f32 {
    let bus = if music { prefs.music } else { prefs.effects };
    (prefs.master * bus).clamp(0.0, 1.0)
}

#[derive(Component)]
pub(crate) struct UiRequest(pub(crate) String);

/// Queues a UI cue. `flush_ui_cues` runs it through the shared voice picker.
pub fn ui_cue(commands: &mut Commands, _assets: &AssetServer, content: &Content, _prefs: &Prefs, id: &str) {
    if content.sfx(id).is_none() {
        return;
    }
    commands.spawn(UiRequest(id.to_string()));
}

/// Live voices occupy `max_instances` and the mix voice cap. A UI cue that needs a
/// slot stops the quietest stealable one-shot. Warnings and loops stay.
pub fn flush_ui_cues(
    mut commands: Commands,
    assets: Res<AssetServer>,
    prefs: Res<Prefs>,
    time: Res<Time>,
    requests: Query<(Entity, &UiRequest)>,
    live: Query<(Entity, &OneShot)>,
) {
    let asked: Vec<String> = requests.iter().map(|(_, request)| request.0.clone()).collect();
    for (entity, _) in &requests {
        commands.entity(entity).despawn();
    }
    if asked.is_empty() {
        return;
    }
    let content = Content::embedded();
    let live: Vec<(Entity, Heard)> = live
        .iter()
        .map(|(entity, shot)| {
            (
                entity,
                Heard {
                    id: shot.id.clone(),
                    started: shot.started,
                    order: shot.order,
                },
            )
        })
        .collect();
    let playing: Vec<Heard> = live.iter().map(|(_, heard)| heard.clone()).collect();
    let admitted = admit_ui(&content.audio, &playing, &asked, time.elapsed_secs());
    for index in admitted.stop {
        if let Some((entity, _)) = live.get(index) {
            commands.entity(*entity).despawn();
        }
    }
    for id in admitted.play {
        let Some(def) = content.sfx(&id) else { continue };
        commands.spawn((
            AudioPlayer::new(assets.load::<AudioSource>(def.path.clone())),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(def.volume * bus_gain(&prefs, false))),
            OneShot {
                id,
                owner: 0,
                started: time.elapsed_secs(),
                order: next_voice_order(),
            },
        ));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiVoices {
    pub play: Vec<String>,
    pub stop: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Heard {
    pub id: String,
    pub started: f32,
    pub order: u32,
}

struct Occupant {
    id: String,
    loudness: f32,
    stealable: bool,
    started: f32,
    order: u32,
    /// Index into the `playing` slice. `None` is a cue admitted in this call.
    playing: Option<usize>,
}

static VOICE_ORDER: AtomicU32 = AtomicU32::new(1);

fn next_voice_order() -> u32 {
    VOICE_ORDER.fetch_add(1, Ordering::Relaxed)
}

/// Admits queued cues without going past the voice cap.
///
/// When the cap is full, the cue steals the quietest stealable one-shot that is
/// already playing. Equal loudness keeps the voice with the earlier stored start
/// time as the victim. If every voice is a warning or a loop, the cue is dropped.
pub fn admit_ui(audio: &gemini_sim::content::AudioTable, playing: &[Heard], asked: &[String], now: f32) -> UiVoices {
    let cap = audio.voice_cap.max(1);
    let mut occupants: Vec<Occupant> = playing
        .iter()
        .enumerate()
        .map(|(index, heard)| {
            let def = audio.sfx.iter().find(|row| row.id == heard.id);
            Occupant {
                id: heard.id.clone(),
                loudness: def.map(|row| row.loudness).unwrap_or(f32::MAX),
                stealable: def.is_some_and(|row| row.stealable && !row.looped && !row.is_warning),
                started: heard.started,
                order: heard.order,
                playing: Some(index),
            }
        })
        .collect();
    let mut stop = Vec::new();
    let mut born = playing.len() as u32;
    for id in asked {
        let Some(def) = audio.sfx.iter().find(|row| row.id == *id) else {
            continue;
        };
        if def.max_instances > 0 {
            let live = occupants.iter().filter(|occupant| occupant.id == def.id).count() as u32;
            if live >= def.max_instances {
                continue;
            }
        }
        if occupants.len() >= cap {
            let victim = occupants
                .iter()
                .enumerate()
                .filter(|(_, occupant)| occupant.stealable)
                .min_by(|(_, a), (_, b)| {
                    a.loudness
                        .partial_cmp(&b.loudness)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.started.partial_cmp(&b.started).unwrap_or(std::cmp::Ordering::Equal))
                        .then_with(|| a.order.cmp(&b.order))
                });
            let Some((index, _)) = victim else {
                continue;
            };
            let removed = occupants.remove(index);
            if let Some(playing_index) = removed.playing {
                stop.push(playing_index);
            }
        }
        occupants.push(Occupant {
            id: def.id.clone(),
            loudness: def.loudness,
            stealable: def.stealable && !def.looped && !def.is_warning,
            started: now,
            order: 0x8000_0000 + born,
            playing: None,
        });
        born += 1;
    }
    let play = occupants.iter().filter(|occupant| occupant.playing.is_none()).map(|occupant| occupant.id.clone()).collect();
    UiVoices { play, stop }
}

pub fn menu_levels(prefs: Res<Prefs>, mut music: Option<ResMut<Music>>) {
    let Some(music) = music.as_mut() else { return };
    music.user_gain = bus_gain(&prefs, true);
    music.duck_gain = 1.0;
    music.filter.0.set_hz(f32::INFINITY);
}

/// Solo pause suspends looping game sounds. Closing the menu resumes them in place.
pub fn hold_loops(hold: Res<PauseMenu>, game: Res<Game>, mut sinks: Query<(&OneShot, &mut AudioSink)>) {
    let pause_loops = hold.open && game.sim.connected_count() <= 1;
    for (shot, sink) in &mut sinks {
        let looping = game.content.sfx(&shot.id).is_some_and(|def| def.looped);
        if !looping {
            continue;
        }
        if pause_loops {
            sink.pause();
        } else if sink.is_paused() {
            sink.play();
        }
    }
}

pub fn menu_theme(mut commands: Commands, assets: Res<AssetServer>) {
    let content = Content::embedded();
    let mut music = Music::default();
    start_track(&mut commands, &assets, &content, &mut music, "theme_main");
    commands.insert_resource(music);
}

pub fn stop_all(mut commands: Commands, music: Option<ResMut<Music>>, crowd: Option<Res<Crowd>>, shots: Query<Entity, Or<(With<OneShot>, With<CrowdVoice>, With<MusicStem>)>>) {
    for entity in &shots {
        commands.entity(entity).despawn();
    }
    if let Some(crowd) = crowd {
        if let Some(entity) = crowd.entity {
            commands.entity(entity).despawn();
        }
    }
    if music.is_some() {
        commands.remove_resource::<Music>();
    }
    commands.remove_resource::<Crowd>();
}

fn apply_moments(
    commands: &mut Commands,
    assets: &AssetServer,
    content: &Content,
    music: &mut Music,
    moments: &[MusicMoment],
    sim: &gemini_sim::sim::Sim,
) {
    for moment in moments {
        let wave = sim.director.wave.max(1);
        let combat = content.combat_loop_id(wave, sim.endless_mode).to_string();
        let (kind, target) = match moment {
            MusicMoment::MatchStart => (TransitionKind::MenuToCombat, content.combat_loop_id(1, false).to_string()),
            MusicMoment::MatchEnd => (TransitionKind::CombatToMenu, menu_id(content)),
            MusicMoment::BossIntro { boss_id } => (
                TransitionKind::CombatToBoss,
                content.boss_track_id(boss_id).unwrap_or(combat.as_str()).to_string(),
            ),
            MusicMoment::BossDown { .. } => (TransitionKind::BossToCombat, combat),
            MusicMoment::LoopSwap { id } => (TransitionKind::LoopSwap, id.clone()),
            MusicMoment::BossEnrage { boss_id } => (
                TransitionKind::BossEnrage,
                content.enrage_track_id(boss_id).unwrap_or(combat.as_str()).to_string(),
            ),
            MusicMoment::BossPhase { boss_id, phase } => (
                TransitionKind::BossPhase,
                content.phase_track_id(boss_id, *phase).unwrap_or(combat.as_str()).to_string(),
            ),
        };
        switch_to(commands, assets, content, music, kind, &target, sim);
    }
}

fn switch_to(
    commands: &mut Commands,
    assets: &AssetServer,
    content: &Content,
    music: &mut Music,
    kind: TransitionKind,
    target: &str,
    sim: &gemini_sim::sim::Sim,
) {
    let Some(from) = content.track(&music.track).cloned().or_else(|| content.track(target).cloned()) else {
        start_track(commands, assets, content, music, target);
        return;
    };
    let Some(to) = content.track(target).cloned() else { return };
    let from_loop = music.loops.first().copied().unwrap_or(0.0);
    let to_loop = decoded_loops(content, &to).into_iter().next().unwrap_or(0.0);
    let plan = plan_switch(
        &from,
        &to,
        from_loop,
        to_loop,
        kind,
        &content.music.transitions,
        content.music.director.bpm_warn_seconds(),
    )
    .expect("transition mode");
    match switch_clock(&plan) {
        SwitchClock::Reset => start_track(commands, assets, content, music, target),
        SwitchClock::WaitForBar => {
            let _ = sim;
            music.pending = Some(target.to_string());
        }
        SwitchClock::FadeNow => {
            let bars = match plan {
                PlannedSwitch::CrossfadeOnBar { fade_bars, .. } => fade_bars,
                PlannedSwitch::HardCut { .. } => 0.0,
            };
            let clock = FileClock::measure(&from, from_loop);
            let seconds = clock.bar_seconds() * bars;
            fade_now(commands, assets, content, music, target, seconds);
        }
    }
}

fn start_track(commands: &mut Commands, assets: &AssetServer, content: &Content, music: &mut Music, id: &str) {
    for entity in music.entities.drain(..) {
        commands.entity(entity).despawn();
    }
    music.pending = None;
    music.fade_left = 0.0;
    music.fade_dur = 0.0;
    let Some(track) = content.track(id) else { return };
    let loops = decoded_loops(content, track);
    let warn = content.music.director.bpm_warn_seconds();
    for (i, seconds) in loops.iter().enumerate() {
        let stem = stem_name(track, i);
        if let Some(mismatch) = bpm_sanity(&stem, track, *seconds, warn) {
            bevy::log::warn!(
                "music {stem}: file loop {:.4}s vs bpm loop {:.4}s ({} ms)",
                mismatch.file_loop_seconds,
                mismatch.bpm_loop_seconds,
                mismatch.error_seconds * 1000.0
            );
        }
    }
    let intensity = combat_intensity(&content.music.director, "normal", 1, false, 0.0, 1.0, false);
    let paths = stem_paths(track);
    if track.layer.is_empty() {
        music.mixer = None;
        music.loops = loops;
        for path in paths {
            let entity = commands
                .spawn((
                    StemFile(assets.load::<AudioSource>(path)),
                    PlaybackSettings::LOOP.with_volume(Volume::Linear(0.85 * music.duck_gain * music.user_gain)),
                    MusicStem { index: 0 },
                ))
                .id();
            music.entities.push(entity);
        }
    } else {
        let mixer = StemMixer::start(track, &loops, intensity);
        for (i, path) in paths.iter().enumerate() {
            let gain = mixer.voices.get(i).map(|v| v.current).unwrap_or(0.0);
            let entity = commands
                .spawn((
                    StemFile(assets.load::<AudioSource>(path.clone())),
                    PlaybackSettings::LOOP.with_volume(Volume::Linear(gain * music.duck_gain * music.user_gain)),
                    MusicStem { index: i },
                ))
                .id();
            music.entities.push(entity);
        }
        music.mixer = Some(mixer);
        music.loops = loops;
    }
    music.track = id.to_string();
}

fn advance(game: &Game, music: &mut Music, dt: f32) {
    let Some(track) = game.content.track(&music.track).cloned() else { return };
    if music.fade_dur > 0.0 {
        music.fade_left = (music.fade_left - dt).max(0.0);
    }
    if track.layer.is_empty() {
        return;
    }
    let downed = game.sim.players.first().map(|p| p.downed).unwrap_or(false);
    let alive_cost: f32 = game
        .sim
        .enemies
        .iter()
        .filter(|e| e.alive)
        .map(|e| game.content.enemy(e.kind).cost)
        .sum();
    let intensity = combat_intensity(
        &game.content.music.director,
        &game.difficulty,
        game.sim.director.wave,
        game.sim.endless_mode,
        alive_cost,
        game.sim.director.wave_budget,
        downed,
    );
    let breather = match game.sim.phase {
        gemini_sim::sim::Phase::Breather { .. } => Some((
            game.content.music.director.breather_stems.as_slice(),
            db_to_gain(game.content.music.director.breather_db),
        )),
        _ => None,
    };
    if let Some(mixer) = music.mixer.as_mut() {
        mixer.advance(
            &track,
            dt,
            intensity,
            game.content.music.director.hysteresis_bars,
            breather,
        );
        if music.fade_dur > 0.0 && music.fade_left > 0.0 {
            let ramp = 1.0 - music.fade_left / music.fade_dur;
            for voice in &mut mixer.voices {
                voice.current *= ramp;
            }
        }
    }
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn fade_now(
    commands: &mut Commands,
    assets: &AssetServer,
    content: &Content,
    music: &mut Music,
    target: &str,
    seconds: f32,
) {
    start_track(commands, assets, content, music, target);
    music.fade_dur = seconds.max(0.0);
    music.fade_left = music.fade_dur;
}

pub(crate) fn register(app: &mut App) -> &mut App {
    app.add_audio_source::<DuckedMusic>()
}

pub fn attach_duck(
    mut commands: Commands,
    sources: Res<Assets<AudioSource>>,
    ducked: Option<ResMut<Assets<DuckedMusic>>>,
    music: Option<Res<Music>>,
    output: Option<Res<GlobalVolume>>,
    pending: Query<(Entity, &StemFile), (With<MusicStem>, Without<AudioPlayer<DuckedMusic>>)>,
) {
    // Bevy keeps AudioOutput private. GlobalVolume is inserted with that output,
    // and a headless match does not install the audio plugin. A missing
    // music_duck_db or host_away row still fails when content loads.
    if output.is_none() {
        return;
    }
    let Some(mut ducked) = ducked else {
        panic!("audio output is present but DuckedMusic was not registered");
    };
    let Some(music) = music else {
        panic!("audio output is present but Music was not inserted");
    };
    for (entity, file) in &pending {
        let Some(source) = sources.get(&file.0) else { continue };
        let handle = ducked.add(DuckedMusic {
            bytes: Arc::clone(&source.bytes),
            filter: Arc::clone(&music.filter.0),
        });
        // The entity already exists and keeps playing. Inserting the player once
        // starts the file at its beginning. Later cutoff changes do not respawn it.
        commands.entity(entity).insert(AudioPlayer(handle));
    }
}

pub fn sync_gains(music: Option<Res<Music>>, mut stems: Query<(&MusicStem, &mut AudioSink)>) {
    let Some(music) = music else { return };
    let duck = (music.duck_gain * music.user_gain).clamp(0.0, 1.0);
    if music.mixer.is_none() {
        for (_stem, mut sink) in &mut stems {
            sink.set_volume(Volume::Linear(0.85 * duck));
        }
        return;
    }
    let mixer = music.mixer.as_ref().expect("mixer");
    for (stem, mut sink) in &mut stems {
        if let Some(voice) = mixer.voices.get(stem.index) {
            sink.set_volume(Volume::Linear((voice.current * duck).clamp(0.0, 1.0)));
        }
    }
}

fn bar_crossed(music: &Music) -> bool {
    let Some(mixer) = &music.mixer else { return true };
    // A pending crossfade fires when any stem has moved onto a new file-bar since the switch was queued.
    // The mixer already latched targets on the bar; the destination starts now.
    mixer.voices.iter().any(|voice| {
        let clock = FileClock {
            loop_bars: 1,
            loop_seconds: voice.loop_seconds,
            fade_bars: 1.0,
        };
        clock.bar_seconds() > 0.0 && voice.elapsed > clock.bar_seconds()
    }) || mixer.voices.is_empty()
}

fn decoded_loops(content: &Content, track: &TrackDef) -> Vec<f32> {
    let _ = content;
    stem_paths(track)
        .iter()
        .map(|path| ogg_seconds(path).unwrap_or(0.0))
        .collect()
}

fn stem_paths(track: &TrackDef) -> Vec<String> {
    if track.layer.is_empty() {
        track.path.clone().into_iter().collect()
    } else {
        track.layer.iter().map(|l| l.path.clone()).collect()
    }
}

fn stem_name(track: &TrackDef, index: usize) -> String {
    track
        .layer
        .get(index)
        .map(|l| l.id.clone())
        .or_else(|| track.path.clone())
        .unwrap_or_else(|| "track".into())
}

fn menu_id(content: &Content) -> String {
    content
        .music
        .track
        .iter()
        .find(|(_, t)| t.role == "menu")
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| "theme_main".into())
}

fn ogg_seconds(path: &str) -> Option<f32> {
    let bytes: &[u8] = match path {
        "audio/music/theme_main.ogg" => include_bytes!("../../../assets/audio/music/theme_main.ogg"),
        "audio/music/combat_01/drums.ogg" => include_bytes!("../../../assets/audio/music/combat_01/drums.ogg"),
        "audio/music/combat_01/bass.ogg" => include_bytes!("../../../assets/audio/music/combat_01/bass.ogg"),
        "audio/music/combat_01/synth.ogg" => include_bytes!("../../../assets/audio/music/combat_01/synth.ogg"),
        "audio/music/combat_01/lead.ogg" => include_bytes!("../../../assets/audio/music/combat_01/lead.ogg"),
        "audio/music/combat_02/drums.ogg" => include_bytes!("../../../assets/audio/music/combat_02/drums.ogg"),
        "audio/music/combat_02/bass.ogg" => include_bytes!("../../../assets/audio/music/combat_02/bass.ogg"),
        "audio/music/combat_02/synth.ogg" => include_bytes!("../../../assets/audio/music/combat_02/synth.ogg"),
        "audio/music/combat_02/lead.ogg" => include_bytes!("../../../assets/audio/music/combat_02/lead.ogg"),
        "audio/music/boss_brood/boss_loop.ogg" => include_bytes!("../../../assets/audio/music/boss_brood/boss_loop.ogg"),
        "audio/music/boss_brood/boss_enrage.ogg" => include_bytes!("../../../assets/audio/music/boss_brood/boss_enrage.ogg"),
        "audio/music/boss_tyrant/phase1.ogg" => include_bytes!("../../../assets/audio/music/boss_tyrant/phase1.ogg"),
        "audio/music/boss_tyrant/phase2.ogg" => include_bytes!("../../../assets/audio/music/boss_tyrant/phase2.ogg"),
        "audio/music/boss_tyrant/phase3.ogg" => include_bytes!("../../../assets/audio/music/boss_tyrant/phase3.ogg"),
        _ => return None,
    };
    vorbis_seconds(bytes)
}

fn listener_of(game: &Game) -> SimVec {
    game.sim
        .players
        .first()
        .map(|p| p.pos)
        .unwrap_or(SimVec::ZERO)
}

fn attenuate(content: &Content, pos: SimVec, listener: SimVec) -> f32 {
    let dx = pos.x - listener.x;
    let dz = pos.z - listener.z;
    let dist = (dx * dx + dz * dz).sqrt().max(content.audio.min_distance);
    (8.0 / (8.0 + dist)).clamp(0.15, 1.0)
}

fn owner_of(events: &[SoundEvent], id: &str, pos: SimVec) -> u32 {
    events
        .iter()
        .find(|e| e.id == id && (e.pos.x - pos.x).abs() < 0.05 && (e.pos.z - pos.z).abs() < 0.05)
        .map(|e| e.owner)
        .unwrap_or(0)
}

#[allow(dead_code)]
fn _director(content: &Content) -> &MusicDirectorCfg {
    &content.music.director
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bevy::audio::{AudioPlayer, AudioSource, GlobalVolume, Volume};
    use bevy::prelude::*;

    use super::{attach_duck, DuckedMusic, Music, MusicStem, StemFile};

    fn app_with_stem() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<AudioSource>>();
        let handle = app.world_mut().resource_mut::<Assets<AudioSource>>().add(AudioSource {
            bytes: Arc::<[u8]>::from(b"riff".as_slice()),
        });
        app.world_mut().spawn((StemFile(handle), MusicStem { index: 0 }));
        app.add_systems(Update, attach_duck);
        app
    }

    fn ducked_count(app: &mut App) -> usize {
        let mut players = app.world_mut().query::<&AudioPlayer<DuckedMusic>>();
        players.iter(app.world()).count()
    }

    #[test]
    fn attach_duck_skips_when_the_audio_output_is_absent() {
        let mut app = app_with_stem();
        app.insert_resource(Music::default());
        app.init_resource::<Assets<DuckedMusic>>();
        app.update();
        assert_eq!(ducked_count(&mut app), 0);
    }

    #[test]
    fn attach_duck_attaches_when_the_audio_output_is_present() {
        let mut app = app_with_stem();
        app.insert_resource(GlobalVolume::new(Volume::Linear(1.0)));
        app.insert_resource(Music::default());
        app.init_resource::<Assets<DuckedMusic>>();
        app.update();
        assert_eq!(ducked_count(&mut app), 1);
    }

    #[test]
    #[should_panic(expected = "DuckedMusic was not registered")]
    fn attach_duck_panics_when_output_is_up_without_the_duck_source() {
        let mut app = app_with_stem();
        app.insert_resource(GlobalVolume::new(Volume::Linear(1.0)));
        app.insert_resource(Music::default());
        app.update();
    }

    #[test]
    #[should_panic(expected = "Music was not inserted")]
    fn attach_duck_panics_when_output_is_up_without_music() {
        let mut app = app_with_stem();
        app.insert_resource(GlobalVolume::new(Volume::Linear(1.0)));
        app.init_resource::<Assets<DuckedMusic>>();
        app.update();
    }

    #[test]
    fn repeated_hovers_stay_within_max_instances() {
        use gemini_sim::content::Content;

        use super::{admit_ui, Heard};

        let content = Content::embedded();
        let hover = content.ui.sounds.hover.clone();
        let cap = content.sfx(&hover).unwrap().max_instances;
        assert_eq!(cap, 2, "ui_hover max_instances");
        let asked = vec![hover.clone(); 8];
        let admitted = admit_ui(&content.audio, &[], &asked, 0.0);
        assert_eq!(admitted.play.len() as u32, cap);
        assert!(admitted.stop.is_empty());
        let playing: Vec<Heard> = admitted
            .play
            .iter()
            .enumerate()
            .map(|(index, id)| Heard { id: id.clone(), started: index as f32, order: index as u32 })
            .collect();
        assert!(admit_ui(&content.audio, &playing, &asked, 10.0).play.is_empty(), "live hovers already fill the cap");
        let tick = content.ui.sounds.slider.clone();
        assert_eq!(content.sfx(&tick).unwrap().max_instances, 3, "ui_slider_tick max_instances");
        let ticks = vec![tick; 6];
        assert_eq!(admit_ui(&content.audio, &[], &ticks, 0.0).play.len(), 3);
    }

    #[test]
    fn hover_steals_the_quietest_oneshot_and_spares_warnings_and_loops() {
        use gemini_sim::content::Content;

        use super::{admit_ui, Heard};

        let content = Content::embedded();
        let voice_cap = content.audio.voice_cap;
        assert_eq!(voice_cap, 32);
        let hover = content.ui.sounds.hover.clone();
        let warning_ids: Vec<&str> = content.audio.sfx.iter().filter(|row| row.is_warning).map(|row| row.id.as_str()).collect();
        let loop_ids: Vec<&str> = content.audio.sfx.iter().filter(|row| row.looped).map(|row| row.id.as_str()).collect();
        assert!(!warning_ids.is_empty());
        assert!(!loop_ids.is_empty());
        let quiet = content
            .audio
            .sfx
            .iter()
            .filter(|row| row.stealable && !row.looped && !row.is_warning && row.id != hover)
            .min_by(|a, b| a.loudness.partial_cmp(&b.loudness).unwrap())
            .expect("stealable one-shot");
        let louder = content.audio.sfx.iter().find(|row| {
            row.stealable && !row.looped && !row.is_warning && row.id != hover && row.id != quiet.id && row.loudness > quiet.loudness
        });
        let cue = |id: &str, started: f32| Heard { id: id.to_string(), started, order: started as u32 };
        let mut playing = vec![cue(&quiet.id, 0.0), cue(&quiet.id, 1.0)];
        let mut protected = Vec::new();
        for id in warning_ids.iter().chain(loop_ids.iter()) {
            if playing.len() + 1 >= voice_cap {
                break;
            }
            protected.push(playing.len());
            playing.push(cue(id, 10.0 + playing.len() as f32));
        }
        assert!(protected.len() >= 2, "the full list includes a warning and a loop");
        while playing.len() < voice_cap {
            if let Some(row) = louder {
                playing.push(cue(&row.id, 20.0 + playing.len() as f32));
            } else {
                playing.push(cue(warning_ids[playing.len() % warning_ids.len()], 20.0 + playing.len() as f32));
            }
        }
        assert_eq!(playing.len(), voice_cap);
        let asked = vec![hover.clone()];
        let stolen = admit_ui(&content.audio, &playing, &asked, 100.0);
        assert_eq!(stolen.play, vec![hover.clone()]);
        assert_eq!(stolen.stop, vec![0], "the earlier start time of the quietest one-shots is the victim");
        for index in &protected {
            assert!(!stolen.stop.contains(index), "protected {}", playing[*index].id);
            let row = content.sfx(&playing[*index].id).unwrap();
            assert!(row.is_warning || row.looped);
            assert!(!row.stealable);
        }
        assert_eq!(playing.len() - stolen.stop.len() + stolen.play.len(), voice_cap);

        let mut blocked = Vec::new();
        while blocked.len() < voice_cap {
            let id = if blocked.len() % 2 == 0 {
                warning_ids[blocked.len() % warning_ids.len()]
            } else {
                loop_ids[blocked.len() % loop_ids.len()]
            };
            blocked.push(cue(id, blocked.len() as f32));
        }
        let dropped = admit_ui(&content.audio, &blocked, &asked, 100.0);
        assert!(dropped.play.is_empty(), "a full list of warnings and loops drops the cue");
        assert!(dropped.stop.is_empty());

        let mut reversed = vec![
            Heard { id: quiet.id.clone(), started: 4.0, order: 0 },
            Heard { id: quiet.id.clone(), started: 1.0, order: 9 },
        ];
        while reversed.len() < voice_cap {
            reversed.push(cue(warning_ids[reversed.len() % warning_ids.len()], 30.0));
        }
        let later = admit_ui(&content.audio, &reversed, &asked, 100.0);
        assert_eq!(later.stop, vec![1], "the stored start time beats list order");
    }
}
