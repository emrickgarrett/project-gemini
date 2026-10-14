//! Stems share a start instant. Bar lines come from each file's decoded length.
//! Sfx playback volume is the `volume` column. Charge stops drop that owner's voice
//! in the same frame the sim emits the stop.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::audio::{
    AddAudioSource, AudioSinkPlayback, AudioSource, Decodable, GlobalVolume, Sample, SeekError,
    Source, Volume,
};
use bevy::prelude::*;

use gemini_sim::audio_mix::{select_voices, LiveVoice, SoundEvent, Voice};
use gemini_sim::content::Content;
use gemini_sim::math::Vec3 as SimVec;
use gemini_sim::music::{
    away_mix, bpm_sanity, combat_intensity, plan_switch, switch_clock, FileClock, Lowpass,
    MusicDirectorCfg, PlannedSwitch, StemMixer, SwitchClock, TrackDef, TransitionKind,
};
use gemini_sim::ogg::vorbis_seconds;
use gemini_sim::sim::MusicMoment;

use crate::session::{Away, FrameFx, Game, PauseMenu};
use crate::settings::Prefs;

#[derive(Component)]
pub(crate) struct MusicStem {
    index: usize,
}

/// Arena bed. Not a MusicStem, so a track change does not despawn it.
#[derive(Component)]
pub(crate) struct BedStem;

/// A prop loop that stays up until the voice list drops it.
#[derive(Component)]
pub(crate) struct PropLoop {
    id: String,
    pos: SimVec,
}

#[derive(Resource)]
pub(crate) struct ArenaAudio {
    last_time: f32,
    travel: BTreeMap<u32, f32>,
    variant: BTreeMap<u32, u32>,
    beacon_cycle: i64,
    /// Warning newcomers waiting for a fade to free their row slot.
    waiting: Vec<Voice>,
    /// Stealable loops taken off the cap. A free voice resumes the nearest one.
    stolen: Vec<StolenLoop>,
}

/// A prop loop the mixer took. `elapsed` is the loop position at the steal.
#[derive(Clone, Debug)]
struct StolenLoop {
    id: String,
    pos: SimVec,
    priority: i32,
    elapsed: f32,
    /// Mix row id is the tie-break. This stamp splits two copies of that row.
    order: u32,
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
    /// Linear volume before a co-op pause duck. Solo pause pauses the sink instead.
    gain: f32,
    pos: SimVec,
    /// Content priority this voice was played at, including a teammate step's remote priority.
    priority: i32,
    /// Seconds this voice has played. A stolen loop resumes at this loop position.
    elapsed: f32,
    /// Seconds left on a fade. Zero means this voice is not fading.
    fade_left: f32,
    fade_dur: f32,
    /// A stolen loop fades back in. A warning replacement fades out.
    fade_in: bool,
    /// Seek here once the sink exists. The loop position captured at the steal.
    resume_at: Option<f32>,
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
    let linked = arena_audio_linked();
    if linked == 0 {
        panic!("arena audio embedded no bytes");
    }
    let mut music = Music::default();
    // Menu already started the theme. MatchStart hard-cuts into combat.
    start_track(
        &mut commands,
        &assets,
        &game.content,
        &mut music,
        "theme_main",
    );
    let moments = game.sim.music.clone();
    apply_moments(
        &mut commands,
        &assets,
        &game.content,
        &mut music,
        &moments,
        &game.sim,
    );
    spawn_bed(&mut commands, &assets, &game.content, &music);
    let period = game.content.arena_scene.beacon_glow_period;
    if !(period > 0.0) {
        panic!("beacon_glow_period must be > 0");
    }
    commands.insert_resource(ArenaAudio {
        last_time: game.sim.time,
        travel: BTreeMap::new(),
        variant: BTreeMap::new(),
        beacon_cycle: (game.sim.time / period).floor() as i64,
        waiting: Vec::new(),
        stolen: Vec::new(),
    });
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
    mut shots: Query<
        (Entity, &mut OneShot, Option<&mut AudioSink>),
        (Without<MusicStem>, Without<PropLoop>),
    >,
    time: Res<Time>,
    prefs: Res<Prefs>,
    hold: Res<PauseMenu>,
    mut arena_audio: ResMut<ArenaAudio>,
    mut prop_loops: Query<(Entity, &PropLoop, &mut OneShot, Option<&mut AudioSink>)>,
) {
    let hud = &game.content.presentation.hud.away;
    let mix = away_mix(away.view.look, hud.music_duck_db, hud.music_duck_lowpass_hz);
    let solo = game.sim.connected_count() <= 1;
    let (gain, hz) = pause_music(
        hold.open,
        solo,
        mix.gain,
        mix.lowpass_hz,
        hud.solo_pause_duck_db,
        hud.music_duck_lowpass_hz,
        hud.coop_pause_duck_db,
    );
    music.duck_gain = gain;
    music.filter.0.set_hz(hz);
    music.user_gain = bus_gain(&prefs, true);
    let listener = listener_of(&game);
    if !fx.moments.is_empty() {
        apply_moments(
            &mut commands,
            &assets,
            &game.content,
            &mut music,
            &fx.moments,
            &game.sim,
        );
    }
    advance(&game, &mut music, time.delta_secs());
    if let Some(target) = music.pending.clone() {
        if bar_crossed(&music) {
            start_track(&mut commands, &assets, &game.content, &mut music, &target);
            music.pending = None;
        }
    }

    let dt = time.delta_secs();
    let mut finished = Vec::new();
    for (entity, mut shot, mut sink) in &mut shots {
        if advance_voice(&mut shot, sink.as_deref_mut(), dt) {
            remember_stolen(&game.content, &mut arena_audio.stolen, &shot);
            finished.push(entity);
        }
    }
    for (entity, _, mut shot, mut sink) in &mut prop_loops {
        if advance_voice(&mut shot, sink.as_deref_mut(), dt) {
            remember_stolen(&game.content, &mut arena_audio.stolen, &shot);
            finished.push(entity);
        }
    }
    for entity in &finished {
        commands.entity(*entity).despawn();
    }
    let mut playing: Vec<LiveVoice> = shots
        .iter()
        .filter_map(|(entity, shot, _)| {
            if finished.contains(&entity) {
                return None;
            }
            let def = game.content.sfx(&shot.id)?;
            if def.looped {
                return None;
            }
            Some(LiveVoice {
                id: shot.id.clone(),
                pos: shot.pos,
                priority: shot.priority,
                fading: shot.fade_dur > 0.0,
            })
        })
        .collect();
    for (entity, _, shot, _) in &prop_loops {
        if finished.contains(&entity) {
            continue;
        }
        playing.push(LiveVoice {
            id: shot.id.clone(),
            pos: shot.pos,
            priority: shot.priority,
            fading: shot.fade_dur > 0.0,
        });
    }
    let mut events = fx.sounds.clone();
    events.extend(arena_cues(&mut arena_audio, &game));
    let cold = cold_stealable_loops(&game, &events, &playing);
    let mix_events: Vec<SoundEvent> = events
        .iter()
        .filter(|event| {
            !cold
                .iter()
                .any(|held| held.id == event.id && held.pos == event.pos)
        })
        .cloned()
        .collect();
    let pick = select_voices(
        &game.content.audio,
        &mix_events,
        listener,
        &playing,
        &mut arena_audio.waiting,
    );
    for faded in &pick.fade {
        for (entity, mut shot, mut sink) in &mut shots {
            if finished.contains(&entity)
                || shot.fade_dur > 0.0
                || shot.id != faded.id
                || shot.pos != faded.pos
            {
                continue;
            }
            begin_warning_fade(&mut shot, game.content.audio.warning_fade);
            if let Some(sink) = sink.as_mut() {
                sink.set_volume(Volume::Linear(shot.gain));
            }
            break;
        }
        for (entity, _, mut shot, mut sink) in &mut prop_loops {
            if finished.contains(&entity)
                || shot.fade_dur > 0.0
                || shot.id != faded.id
                || shot.pos != faded.pos
            {
                continue;
            }
            begin_warning_fade(&mut shot, game.content.audio.warning_fade);
            if let Some(sink) = sink.as_mut() {
                sink.set_volume(Volume::Linear(shot.gain));
            }
            break;
        }
    }
    let selected = pick.voices;
    let mut crowd_vol = 0.0;
    let mut shot_i = 0u32;
    for voice in &selected {
        let Some(def) = game.content.sfx(&voice.id) else {
            continue;
        };
        if def.looped && def.category == "prop" {
            continue;
        }
        if voice.crowd {
            let fullness = if def.loudness > 0.0 {
                voice.gain / def.loudness
            } else {
                1.0
            };
            crowd_vol = def.volume * fullness * bus_gain(&prefs, false);
            continue;
        }
        let mut speed = 1.0;
        if let Some((min, max)) = game.content.pitch_range(&voice.id) {
            let h = (fx.tick as u32)
                .wrapping_mul(17)
                .wrapping_add(shot_i.wrapping_mul(13));
            let t = (h % 1000) as f32 / 1000.0;
            speed = min + (max - min) * t;
            shot_i += 1;
        }
        let atten = if def.flat {
            1.0
        } else {
            attenuate(&game.content, voice.pos, listener)
        };
        let full = def.volume * atten * bus_gain(&prefs, false);
        let bus = paused_bus(
            hold.open,
            solo,
            def.category_ui,
            def.is_warning,
            hud.coop_pause_duck_db,
        );
        if bus == 0.0 {
            continue;
        }
        let volume = full * bus;
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
                    owner: owner_of(&events, &voice.id, voice.pos),
                    started: time.elapsed_secs(),
                    order: next_voice_order(),
                    gain: full,
                    pos: voice.pos,
                    priority: voice.priority,
                    elapsed: 0.0,
                    fade_left: 0.0,
                    fade_dur: 0.0,
                    fade_in: false,
                    resume_at: None,
                },
            ))
            .id();
        let owner = owner_of(&events, &voice.id, voice.pos);
        if fx
            .stops
            .iter()
            .any(|s| s.id == voice.id && s.owner == owner)
        {
            commands.entity(entity).despawn();
        }
    }
    for (entity, shot, _) in &shots {
        if fx
            .stops
            .iter()
            .any(|s| s.id == shot.id && s.owner == shot.owner)
        {
            commands.entity(entity).despawn();
        }
    }
    sync_prop_loops(
        &mut commands,
        &assets,
        &game,
        &prefs,
        hold.open,
        solo,
        hud.coop_pause_duck_db,
        time.elapsed_secs(),
        listener,
        &selected,
        &mut prop_loops,
    );
    let mut playing_after = playing.clone();
    for faded in &pick.fade {
        if let Some(slot) = playing_after
            .iter_mut()
            .find(|voice| voice.id == faded.id && voice.pos == faded.pos)
        {
            slot.fading = true;
        }
    }
    for voice in &selected {
        if voice.crowd
            || playing_after
                .iter()
                .any(|live| live.id == voice.id && live.pos == voice.pos)
        {
            continue;
        }
        playing_after.push(LiveVoice {
            id: voice.id.clone(),
            pos: voice.pos,
            priority: voice.priority,
            fading: false,
        });
    }
    arena_audio.stolen.retain(|wait| {
        cold.iter()
            .any(|event| event.id == wait.id && event.pos == wait.pos)
    });
    let resume = take_stolen(&mut arena_audio.stolen, listener, pick.free);
    for row in &resume {
        spawn_prop_loop(
            &mut commands,
            &assets,
            &game,
            &prefs,
            hold.open,
            solo,
            hud.coop_pause_duck_db,
            time.elapsed_secs(),
            listener,
            &row.id,
            row.pos,
            row.priority,
            row.order,
            Some(row.elapsed),
        );
        playing_after.push(LiveVoice {
            id: row.id.clone(),
            pos: row.pos,
            priority: row.priority,
            fading: false,
        });
    }
    let left: Vec<SoundEvent> = cold
        .into_iter()
        .filter(|event| {
            !resume
                .iter()
                .any(|row| row.id == event.id && row.pos == event.pos)
                && !arena_audio
                    .stolen
                    .iter()
                    .any(|row| row.id == event.id && row.pos == event.pos)
        })
        .collect();
    let left = within_new_instance_cap(&game, &playing_after, left);
    if !left.is_empty() {
        let mut ignored = Vec::new();
        let more = select_voices(
            &game.content.audio,
            &left,
            listener,
            &playing_after,
            &mut ignored,
        );
        for voice in &more.voices {
            let Some(def) = game.content.sfx(&voice.id) else {
                continue;
            };
            if !(def.looped && def.category == "prop") {
                continue;
            }
            if playing_after
                .iter()
                .any(|live| live.id == voice.id && live.pos == voice.pos)
            {
                continue;
            }
            spawn_prop_loop(
                &mut commands,
                &assets,
                &game,
                &prefs,
                hold.open,
                solo,
                hud.coop_pause_duck_db,
                time.elapsed_secs(),
                listener,
                &voice.id,
                voice.pos,
                voice.priority,
                next_voice_order(),
                None,
            );
        }
    }
    crowd.volume = crowd_vol * paused_bus(hold.open, solo, false, false, hud.coop_pause_duck_db);
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
/// slot stops the quietest row whose mix `stealable` flag is set. A stolen loop waits.
pub fn flush_ui_cues(
    mut commands: Commands,
    assets: Res<AssetServer>,
    prefs: Res<Prefs>,
    time: Res<Time>,
    requests: Query<(Entity, &UiRequest)>,
    live: Query<(Entity, &OneShot)>,
    mut arena: Option<ResMut<ArenaAudio>>,
) {
    let asked: Vec<String> = requests
        .iter()
        .map(|(_, request)| request.0.clone())
        .collect();
    for (entity, _) in &requests {
        commands.entity(entity).despawn();
    }
    if asked.is_empty() {
        return;
    }
    let content = Content::embedded();
    for id in &asked {
        if content.sfx(id).is_none() {
            panic!("flush_ui_cues: unknown id {id}");
        }
    }
    let live: Vec<(Entity, Heard, SimVec, f32, i32)> = live
        .iter()
        .map(|(entity, shot)| {
            (
                entity,
                Heard {
                    id: shot.id.clone(),
                    started: shot.started,
                    order: shot.order,
                },
                shot.pos,
                shot.elapsed,
                shot.priority,
            )
        })
        .collect();
    let playing: Vec<Heard> = live
        .iter()
        .map(|(_, heard, _, _, _)| heard.clone())
        .collect();
    let admitted = admit_ui(&content.audio, &playing, &asked, time.elapsed_secs());
    for index in admitted.stop {
        if let Some((entity, heard, pos, elapsed, priority)) = live.get(index) {
            if let Some(arena) = arena.as_mut() {
                remember_stolen_at(
                    &content,
                    &mut arena.stolen,
                    &heard.id,
                    *pos,
                    *priority,
                    *elapsed,
                    heard.order,
                );
            }
            commands.entity(*entity).despawn();
        }
    }
    for id in admitted.play {
        let def = content
            .sfx(&id)
            .unwrap_or_else(|| panic!("flush_ui_cues: unknown id {id}"));
        commands.spawn((
            AudioPlayer::new(assets.load::<AudioSource>(def.path.clone())),
            PlaybackSettings::DESPAWN
                .with_volume(Volume::Linear(def.volume * bus_gain(&prefs, false))),
            OneShot {
                id,
                owner: 0,
                started: time.elapsed_secs(),
                order: next_voice_order(),
                gain: def.volume * bus_gain(&prefs, false),
                pos: SimVec::ZERO,
                priority: def.priority,
                elapsed: 0.0,
                fade_left: 0.0,
                fade_dur: 0.0,
                fade_in: false,
                resume_at: None,
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
    /// Outside the voice cap. It does not take a slot and it is not a steal victim.
    outside: bool,
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
/// When the cap is full, the cue steals the quietest voice whose mix row sets
/// `stealable`. That flag is not derived from `loop`. Equal loudness keeps the
/// voice with the earlier stored start time as the victim. If every voice has
/// `stealable = false`, the cue is dropped.
pub fn admit_ui(
    audio: &gemini_sim::content::AudioTable,
    playing: &[Heard],
    asked: &[String],
    now: f32,
) -> UiVoices {
    let cap = audio.voice_cap.max(1);
    let mut occupants: Vec<Occupant> = playing
        .iter()
        .enumerate()
        .map(|(index, heard)| {
            let def = audio
                .sfx
                .iter()
                .find(|row| row.id == heard.id)
                .unwrap_or_else(|| panic!("playing voice {} is not in the mix", heard.id));
            Occupant {
                id: heard.id.clone(),
                loudness: def.loudness,
                stealable: def.stealable && !def.is_warning && !def.outside_voice_cap,
                outside: def.outside_voice_cap,
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
            let live = occupants
                .iter()
                .filter(|occupant| occupant.id == def.id)
                .count() as u32;
            if live >= def.max_instances {
                continue;
            }
        }
        let inside = occupants.iter().filter(|occupant| !occupant.outside).count();
        if !def.outside_voice_cap && inside >= cap {
            let victim = occupants
                .iter()
                .enumerate()
                .filter(|(_, occupant)| occupant.stealable && !occupant.outside)
                .min_by(|(_, a), (_, b)| {
                    a.loudness
                        .partial_cmp(&b.loudness)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| {
                            a.started
                                .partial_cmp(&b.started)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
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
            stealable: def.stealable && !def.is_warning && !def.outside_voice_cap,
            outside: def.outside_voice_cap,
            started: now,
            order: 0x8000_0000 + born,
            playing: None,
        });
        born += 1;
    }
    let play = occupants
        .iter()
        .filter(|occupant| occupant.playing.is_none())
        .map(|occupant| occupant.id.clone())
        .collect();
    UiVoices { play, stop }
}

pub fn menu_levels(prefs: Res<Prefs>, mut music: Option<ResMut<Music>>) {
    let Some(music) = music.as_mut() else { return };
    music.user_gain = bus_gain(&prefs, true);
    music.duck_gain = 1.0;
    music.filter.0.set_hz(f32::INFINITY);
}

/// Solo pause stops game sounds. Co-op ducks them. UI cues and act-now warnings stay full.
pub fn hold_loops(
    hold: Res<PauseMenu>,
    game: Res<Game>,
    mut sinks: Query<(&OneShot, &mut AudioSink)>,
) {
    let solo = game.sim.connected_count() <= 1;
    let duck = game.content.presentation.hud.away.coop_pause_duck_db;
    for (shot, mut sink) in &mut sinks {
        let def = game.content.sfx(&shot.id);
        let ui = def.is_some_and(|row| row.category_ui);
        let warning = def.is_some_and(|row| row.is_warning);
        let bus = paused_bus(hold.open, solo, ui, warning, duck);
        if bus == 0.0 {
            sink.pause();
            continue;
        }
        if sink.is_paused() {
            sink.play();
        }
        let gain = if shot.fade_dur > 0.0 {
            fade_gain(shot.gain, shot.fade_left, shot.fade_dur, shot.fade_in)
        } else {
            shot.gain
        };
        sink.set_volume(Volume::Linear(gain * bus));
    }
}

pub fn menu_theme(mut commands: Commands, assets: Res<AssetServer>) {
    let content = Content::embedded();
    let mut music = Music::default();
    start_track(&mut commands, &assets, &content, &mut music, "theme_main");
    commands.insert_resource(music);
}

pub fn stop_all(mut commands: Commands, music: Option<ResMut<Music>>, crowd: Option<Res<Crowd>>, shots: Query<Entity, Or<(With<OneShot>, With<CrowdVoice>, With<MusicStem>, With<BedStem>, With<PropLoop>)>>) {
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

/// 0 stops the voice. 1 leaves it alone. Anything between is the co-op duck.
/// UI cues and warnings stay at 1 while a pause is open.
/// Solo pause ducks music by `solo_pause_duck_db` and keeps the away low-pass.
/// Co-op pause ducks by `coop_pause_duck_db` without replacing the away filter.
fn pause_music(open: bool, solo: bool, away_gain: f32, away_hz: f32, solo_duck_db: f32, solo_hz: f32, coop_duck_db: f32) -> (f32, f32) {
    if open && solo {
        (db_to_gain(solo_duck_db), solo_hz)
    } else if open {
        (away_gain.min(db_to_gain(coop_duck_db)), away_hz)
    } else {
        (away_gain, away_hz)
    }
}

fn paused_bus(open: bool, solo: bool, ui: bool, warning: bool, coop_duck_db: f32) -> f32 {
    if !open || ui || warning {
        return 1.0;
    }
    if solo {
        return 0.0;
    }
    db_to_gain(coop_duck_db)
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
    pending: Query<(Entity, &StemFile), (Or<(With<MusicStem>, With<BedStem>)>, Without<AudioPlayer<DuckedMusic>>)>,
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
    game.local()
        .map(|player| player.pos)
        .unwrap_or(SimVec::ZERO)
}

fn attenuate(content: &Content, pos: SimVec, listener: SimVec) -> f32 {
    let dx = pos.x - listener.x;
    let dz = pos.z - listener.z;
    let dist = (dx * dx + dz * dz).sqrt().max(content.audio.min_distance);
    (8.0 / (8.0 + dist)).clamp(0.15, 1.0)
}

const ARENA_OGG: &[&str] = &[
    "arena_01_bed.ogg",
    "hive_nest_idle.ogg",
    "relay_tower_hum.ogg",
    "beacon_pulse.ogg",
    "step_dirt_1.ogg",
    "step_dirt_2.ogg",
    "step_dirt_3.ogg",
    "step_dirt_4.ogg",
    "step_metal_1.ogg",
    "step_metal_2.ogg",
    "step_metal_3.ogg",
    "step_metal_4.ogg",
    "step_rock_1.ogg",
    "step_rock_2.ogg",
    "step_rock_3.ogg",
    "step_rock_4.ogg",
    "step_resin_1.ogg",
    "step_resin_2.ogg",
    "step_resin_3.ogg",
    "step_resin_4.ogg",
];

fn arena_audio_bytes(name: &str) -> Result<&'static [u8], String> {
    let bytes: &[u8] = match name {
        "arena_01_bed.ogg" => include_bytes!("../../../assets/audio/sfx/arena_01_bed.ogg"),
        "hive_nest_idle.ogg" => include_bytes!("../../../assets/audio/sfx/hive_nest_idle.ogg"),
        "relay_tower_hum.ogg" => include_bytes!("../../../assets/audio/sfx/relay_tower_hum.ogg"),
        "beacon_pulse.ogg" => include_bytes!("../../../assets/audio/sfx/beacon_pulse.ogg"),
        "step_dirt_1.ogg" => include_bytes!("../../../assets/audio/sfx/step_dirt_1.ogg"),
        "step_dirt_2.ogg" => include_bytes!("../../../assets/audio/sfx/step_dirt_2.ogg"),
        "step_dirt_3.ogg" => include_bytes!("../../../assets/audio/sfx/step_dirt_3.ogg"),
        "step_dirt_4.ogg" => include_bytes!("../../../assets/audio/sfx/step_dirt_4.ogg"),
        "step_metal_1.ogg" => include_bytes!("../../../assets/audio/sfx/step_metal_1.ogg"),
        "step_metal_2.ogg" => include_bytes!("../../../assets/audio/sfx/step_metal_2.ogg"),
        "step_metal_3.ogg" => include_bytes!("../../../assets/audio/sfx/step_metal_3.ogg"),
        "step_metal_4.ogg" => include_bytes!("../../../assets/audio/sfx/step_metal_4.ogg"),
        "step_rock_1.ogg" => include_bytes!("../../../assets/audio/sfx/step_rock_1.ogg"),
        "step_rock_2.ogg" => include_bytes!("../../../assets/audio/sfx/step_rock_2.ogg"),
        "step_rock_3.ogg" => include_bytes!("../../../assets/audio/sfx/step_rock_3.ogg"),
        "step_rock_4.ogg" => include_bytes!("../../../assets/audio/sfx/step_rock_4.ogg"),
        "step_resin_1.ogg" => include_bytes!("../../../assets/audio/sfx/step_resin_1.ogg"),
        "step_resin_2.ogg" => include_bytes!("../../../assets/audio/sfx/step_resin_2.ogg"),
        "step_resin_3.ogg" => include_bytes!("../../../assets/audio/sfx/step_resin_3.ogg"),
        "step_resin_4.ogg" => include_bytes!("../../../assets/audio/sfx/step_resin_4.ogg"),
        other => return Err(format!("arena audio \"{other}\" is not embedded")),
    };
    if bytes.is_empty() {
        return Err(format!("arena audio \"{name}\" is empty"));
    }
    Ok(bytes)
}

fn arena_audio_linked() -> usize {
    ARENA_OGG
        .iter()
        .map(|name| {
            arena_audio_bytes(name)
                .unwrap_or_else(|err| panic!("{err}"))
                .len()
        })
        .sum()
}

fn spawn_bed(commands: &mut Commands, assets: &AssetServer, content: &Content, music: &Music) {
    let def = content
        .sfx("arena_01_bed")
        .unwrap_or_else(|| panic!("missing arena_01_bed"));
    let volume = def.volume * music.duck_gain * music.user_gain;
    commands.spawn((
        StemFile(assets.load::<AudioSource>(def.path.clone())),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(volume)),
        BedStem,
    ));
}

pub fn sync_bed(
    music: Option<Res<Music>>,
    game: Option<Res<Game>>,
    mut sinks: Query<&mut AudioSink, With<BedStem>>,
) {
    let (Some(music), Some(game)) = (music, game) else {
        return;
    };
    let def = game
        .content
        .sfx("arena_01_bed")
        .unwrap_or_else(|| panic!("missing arena_01_bed"));
    let volume = def.volume * music.duck_gain * music.user_gain;
    for mut sink in &mut sinks {
        sink.set_volume(Volume::Linear(volume));
    }
}

fn begin_warning_fade(shot: &mut OneShot, dur: f32) {
    if !(dur > 0.0) {
        panic!("warning fade must be > 0");
    }
    shot.fade_in = false;
    shot.fade_dur = dur;
    shot.fade_left = dur;
}

fn fade_gain(gain: f32, left: f32, dur: f32, fade_in: bool) -> f32 {
    if !(dur > 0.0) {
        panic!("fade must be > 0");
    }
    if !(left > 0.0) {
        return if fade_in { gain } else { 0.0 };
    }
    if fade_in {
        gain * (1.0 - left / dur)
    } else {
        gain * left / dur
    }
}

fn warning_fade_gain(gain: f32, left: f32, dur: f32) -> f32 {
    fade_gain(gain, left, dur, false)
}

fn arena_cues(state: &mut ArenaAudio, game: &Game) -> Vec<SoundEvent> {
    let mut events = footstep_cues(state, game);
    events.extend(beacon_cues(state, game));
    events.extend(prop_cues(game));
    events
}

fn footstep_cues(state: &mut ArenaAudio, game: &Game) -> Vec<SoundEvent> {
    let now = game.sim.time;
    let dt = now - state.last_time;
    state.last_time = now;
    if dt == 0.0 {
        return Vec::new();
    }
    if dt < 0.0 {
        // A co-op rejoin can apply a snapshot from earlier in the match.
        // Catching up would burst steps and beacon pulses, so the clocks restart.
        state.travel.clear();
        state.variant.clear();
        state.waiting.clear();
        state.stolen.clear();
        let period = game.content.arena_scene.beacon_glow_period;
        if !(period > 0.0) {
            panic!("beacon_glow_period must be > 0");
        }
        state.beacon_cycle = (now / period).floor() as i64;
        return Vec::new();
    }
    let steps = &game.content.audio.footstep;
    if !(steps.stride > 0.0) {
        panic!("footstep stride must be > 0");
    }
    let local = game.local().map(|player| player.id);
    let mut out = Vec::new();
    for player in &game.sim.players {
        if !player.alive || !player.connected || !player.grounded {
            continue;
        }
        let speed = (player.vel.x * player.vel.x + player.vel.z * player.vel.z).sqrt();
        if speed < steps.min_speed {
            continue;
        }
        let travel = state.travel.entry(player.id).or_insert(0.0);
        *travel += speed * dt;
        if *travel < steps.stride {
            continue;
        }
        *travel -= steps.stride;
        let kind = game
            .content
            .surface_kind(&player.under_part)
            .unwrap_or_else(|err| panic!("{err}"));
        let rows = steps
            .rows
            .get(&kind)
            .unwrap_or_else(|| panic!("footstep.rows missing {kind}"));
        if rows.is_empty() {
            panic!("footstep.rows {kind} is empty");
        }
        let n = state.variant.entry(player.id).or_insert(0);
        let id = rows[*n as usize % rows.len()].clone();
        *n = n.wrapping_add(1);
        let priority = if Some(player.id) == local {
            None
        } else {
            Some(steps.remote_priority)
        };
        out.push(SoundEvent {
            id,
            pos: player.pos,
            owner: player.id,
            priority,
        });
    }
    out
}

fn beacon_cues(state: &mut ArenaAudio, game: &Game) -> Vec<SoundEvent> {
    let period = game.content.arena_scene.beacon_glow_period;
    if !(period > 0.0) {
        panic!("beacon_glow_period must be > 0");
    }
    let cycle = (game.sim.time / period).floor() as i64;
    if cycle == state.beacon_cycle {
        return Vec::new();
    }
    state.beacon_cycle = cycle;
    let points = gemini_sim::scenery::beacon_points(&game.content.arena, &game.content.arena_scene)
        .unwrap_or_else(|err| panic!("{err}"));
    points
        .into_iter()
        .map(|pos| SoundEvent {
            id: "beacon_pulse".into(),
            pos,
            owner: 0,
            priority: None,
        })
        .collect()
}

fn prop_cues(game: &Game) -> Vec<SoundEvent> {
    let mut out = Vec::new();
    let nests = gemini_sim::scenery::nest_points(&game.content.arena, &game.content.arena_scene)
        .unwrap_or_else(|err| panic!("{err}"));
    for pos in nests {
        out.push(SoundEvent {
            id: "hive_nest_idle".into(),
            pos,
            owner: 0,
            priority: None,
        });
    }
    let towers = gemini_sim::scenery::relay_points(&game.content.arena, &game.content.arena_scene)
        .unwrap_or_else(|err| panic!("{err}"));
    for pos in towers {
        out.push(SoundEvent {
            id: "relay_tower_hum".into(),
            pos,
            owner: 0,
            priority: None,
        });
    }
    out
}

fn sync_prop_loops(
    commands: &mut Commands,
    assets: &AssetServer,
    game: &Game,
    prefs: &Prefs,
    hold_open: bool,
    solo: bool,
    coop_duck_db: f32,
    time: f32,
    listener: SimVec,
    selected: &[Voice],
    existing: &mut Query<(Entity, &PropLoop, &mut OneShot, Option<&mut AudioSink>)>,
) {
    let mut wanted: Vec<Voice> = selected
        .iter()
        .filter(|voice| {
            game.content
                .sfx(&voice.id)
                .is_some_and(|def| def.looped && def.category == "prop")
        })
        .cloned()
        .collect();
    for (entity, prop, mut shot, _) in existing.iter_mut() {
        let slot = wanted
            .iter()
            .position(|voice| voice.id == prop.id && voice.pos == prop.pos);
        if let Some(index) = slot {
            let voice = &wanted[index];
            let def = game
                .content
                .sfx(&voice.id)
                .unwrap_or_else(|| panic!("missing {}", voice.id));
            let atten = if def.flat {
                1.0
            } else {
                attenuate(&game.content, voice.pos, listener)
            };
            shot.gain = def.volume * atten * bus_gain(prefs, false);
            wanted.remove(index);
        } else if !(shot.fade_dur > 0.0) {
            commands.entity(entity).despawn();
        }
    }
    for voice in wanted {
        spawn_prop_loop(
            commands,
            assets,
            game,
            prefs,
            hold_open,
            solo,
            coop_duck_db,
            time,
            listener,
            &voice.id,
            voice.pos,
            voice.priority,
            next_voice_order(),
            None,
        );
    }
}

fn spawn_prop_loop(
    commands: &mut Commands,
    assets: &AssetServer,
    game: &Game,
    prefs: &Prefs,
    hold_open: bool,
    solo: bool,
    coop_duck_db: f32,
    time: f32,
    listener: SimVec,
    id: &str,
    pos: SimVec,
    priority: i32,
    order: u32,
    resume_at: Option<f32>,
) {
    let def = game
        .content
        .sfx(id)
        .unwrap_or_else(|| panic!("missing {id}"));
    let atten = if def.flat {
        1.0
    } else {
        attenuate(&game.content, pos, listener)
    };
    let full = def.volume * atten * bus_gain(prefs, false);
    let bus = paused_bus(
        hold_open,
        solo,
        def.category_ui,
        def.is_warning,
        coop_duck_db,
    );
    let (volume, elapsed, fade_left, fade_dur, fade_in) = if let Some(at) = resume_at {
        let dur = game.content.audio.loop_resume_fade;
        if !(dur > 0.0) {
            panic!("loop resume fade must be > 0");
        }
        (0.0, at, dur, dur, true)
    } else {
        (full * bus, 0.0, 0.0, 0.0, false)
    };
    commands.spawn((
        AudioPlayer::new(assets.load::<AudioSource>(def.path.clone())),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(volume)),
        OneShot {
            id: id.to_string(),
            owner: 0,
            started: time,
            order,
            gain: full,
            pos,
            priority,
            elapsed,
            fade_left,
            fade_dur,
            fade_in,
            resume_at,
        },
        PropLoop {
            id: id.to_string(),
            pos,
        },
    ));
}

/// Tracks the sink position, seeks a resumed loop, and advances a fade.
/// Fade-out completion returns true so the caller can drop the voice.
fn advance_voice(shot: &mut OneShot, mut sink: Option<&mut AudioSink>, dt: f32) -> bool {
    if let Some(at) = shot.resume_at {
        if let Some(sink) = sink.as_deref() {
            sink.try_seek(Duration::from_secs_f32(at))
                .unwrap_or_else(|err| panic!("loop seek failed: {err:?}"));
            shot.resume_at = None;
            shot.elapsed = at;
        }
    } else if let Some(sink) = sink.as_deref() {
        shot.elapsed = sink.position().as_secs_f32();
    } else {
        shot.elapsed += dt;
    }
    if !(shot.fade_dur > 0.0) {
        return false;
    }
    shot.fade_left -= dt;
    if shot.fade_left > 0.0 {
        let volume = fade_gain(shot.gain, shot.fade_left, shot.fade_dur, shot.fade_in);
        if let Some(sink) = sink.as_mut() {
            sink.set_volume(Volume::Linear(volume));
        }
        return false;
    }
    if shot.fade_in {
        shot.fade_left = 0.0;
        shot.fade_dur = 0.0;
        shot.fade_in = false;
        if let Some(sink) = sink.as_mut() {
            sink.set_volume(Volume::Linear(shot.gain));
        }
        return false;
    }
    true
}

fn remember_stolen(content: &Content, stolen: &mut Vec<StolenLoop>, shot: &OneShot) {
    remember_stolen_at(
        content,
        stolen,
        &shot.id,
        shot.pos,
        shot.priority,
        shot.elapsed,
        shot.order,
    );
}

fn remember_stolen_at(
    content: &Content,
    stolen: &mut Vec<StolenLoop>,
    id: &str,
    pos: SimVec,
    priority: i32,
    elapsed: f32,
    order: u32,
) {
    let def = content
        .sfx(id)
        .unwrap_or_else(|| panic!("playing voice {id} is not in the mix"));
    if !(def.stealable && def.looped) {
        return;
    }
    if stolen.iter().any(|row| row.id == id && row.pos == pos) {
        return;
    }
    stolen.push(StolenLoop {
        id: id.to_string(),
        pos,
        priority,
        elapsed: loop_position(id, elapsed),
        order,
    });
}

/// Cold starts must leave room for loops already sounding. `select_voices` does not
/// count a playing loop toward `max_instances`, because those loops also arrive as events.
fn within_new_instance_cap(
    game: &Game,
    playing: &[LiveVoice],
    events: Vec<SoundEvent>,
) -> Vec<SoundEvent> {
    let mut live: BTreeMap<String, u32> = BTreeMap::new();
    for voice in playing {
        *live.entry(voice.id.clone()).or_default() += 1;
    }
    let mut spawned: BTreeMap<String, u32> = BTreeMap::new();
    events
        .into_iter()
        .filter(|event| {
            let def = game
                .content
                .sfx(&event.id)
                .unwrap_or_else(|| panic!("voice {} is not in the mix", event.id));
            let already = live.get(&event.id).copied().unwrap_or(0);
            let extra = spawned.entry(event.id.clone()).or_default();
            if def.max_instances > 0 && already + *extra >= def.max_instances {
                return false;
            }
            *extra += 1;
            true
        })
        .collect()
}

fn cold_stealable_loops(
    game: &Game,
    events: &[SoundEvent],
    playing: &[LiveVoice],
) -> Vec<SoundEvent> {
    events
        .iter()
        .filter(|event| {
            let def = game
                .content
                .sfx(&event.id)
                .unwrap_or_else(|| panic!("voice {} is not in the mix", event.id));
            def.stealable
                && def.looped
                && !playing
                    .iter()
                    .any(|voice| voice.id == event.id && voice.pos == event.pos)
        })
        .cloned()
        .collect()
}

/// Nearest stolen loop first. Equal distance keeps the lower voice id.
fn take_stolen(stolen: &mut Vec<StolenLoop>, listener: SimVec, free: usize) -> Vec<StolenLoop> {
    if free == 0 || stolen.is_empty() {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..stolen.len()).collect();
    order.sort_by(|&left, &right| {
        let a = &stolen[left];
        let b = &stolen[right];
        listener_distance(a.pos, listener)
            .total_cmp(&listener_distance(b.pos, listener))
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.order.cmp(&b.order))
            .then_with(|| a.pos.x.total_cmp(&b.pos.x))
            .then_with(|| a.pos.y.total_cmp(&b.pos.y))
            .then_with(|| a.pos.z.total_cmp(&b.pos.z))
    });
    let chosen: Vec<StolenLoop> = order
        .iter()
        .take(free)
        .map(|&index| stolen[index].clone())
        .collect();
    let mut remove: Vec<usize> = order.into_iter().take(free).collect();
    remove.sort_unstable_by(|a, b| b.cmp(a));
    for index in remove {
        stolen.remove(index);
    }
    chosen
}

fn listener_distance(pos: SimVec, listener: SimVec) -> f32 {
    let dx = pos.x - listener.x;
    let dy = pos.y - listener.y;
    let dz = pos.z - listener.z;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn loop_position(id: &str, elapsed: f32) -> f32 {
    let seconds = prop_loop_seconds(id);
    elapsed.rem_euclid(seconds)
}

fn prop_loop_seconds(id: &str) -> f32 {
    static CACHE: Mutex<BTreeMap<String, f32>> = Mutex::new(BTreeMap::new());
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|err| panic!("loop duration cache poisoned: {err}"));
    if let Some(seconds) = cache.get(id).copied() {
        return seconds;
    }
    let name = format!("{id}.ogg");
    let bytes = arena_audio_bytes(&name).unwrap_or_else(|err| panic!("{err}"));
    let seconds = vorbis_seconds(bytes).unwrap_or_else(|| panic!("{id} has no duration"));
    if !(seconds > 0.0) {
        panic!("{id} duration must be > 0");
    }
    cache.insert(id.to_string(), seconds);
    seconds
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
        let handle = app
            .world_mut()
            .resource_mut::<Assets<AudioSource>>()
            .add(AudioSource {
                bytes: Arc::<[u8]>::from(b"riff".as_slice()),
            });
        app.world_mut()
            .spawn((StemFile(handle), MusicStem { index: 0 }));
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
            .map(|(index, id)| Heard {
                id: id.clone(),
                started: index as f32,
                order: index as u32,
            })
            .collect();
        assert!(
            admit_ui(&content.audio, &playing, &asked, 10.0)
                .play
                .is_empty(),
            "live hovers already fill the cap"
        );
        let tick = content.ui.sounds.slider.clone();
        assert_eq!(
            content.sfx(&tick).unwrap().max_instances,
            3,
            "ui_slider_tick max_instances"
        );
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
        let warning_ids: Vec<&str> = content
            .audio
            .sfx
            .iter()
            .filter(|row| row.is_warning)
            .map(|row| row.id.as_str())
            .collect();
        let loop_ids: Vec<&str> = content
            .audio
            .sfx
            .iter()
            .filter(|row| row.looped)
            .map(|row| row.id.as_str())
            .collect();
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
            row.stealable
                && !row.looped
                && !row.is_warning
                && row.id != hover
                && row.id != quiet.id
                && row.loudness > quiet.loudness
        });
        let cue = |id: &str, started: f32| Heard {
            id: id.to_string(),
            started,
            order: started as u32,
        };
        let mut playing = vec![cue(&quiet.id, 0.0), cue(&quiet.id, 1.0)];
        let mut protected = Vec::new();
        for id in warning_ids.iter().chain(loop_ids.iter()) {
            if playing.len() + 1 >= voice_cap {
                break;
            }
            protected.push(playing.len());
            playing.push(cue(id, 10.0 + playing.len() as f32));
        }
        assert!(
            protected.len() >= 2,
            "the full list includes a warning and a loop"
        );
        while playing.len() < voice_cap {
            if let Some(row) = louder {
                playing.push(cue(&row.id, 20.0 + playing.len() as f32));
            } else {
                playing.push(cue(
                    warning_ids[playing.len() % warning_ids.len()],
                    20.0 + playing.len() as f32,
                ));
            }
        }
        assert_eq!(playing.len(), voice_cap);
        let asked = vec![hover.clone()];
        let stolen = admit_ui(&content.audio, &playing, &asked, 100.0);
        assert_eq!(stolen.play, vec![hover.clone()]);
        assert_eq!(
            stolen.stop,
            vec![0],
            "the earlier start time of the quietest one-shots is the victim"
        );
        for index in &protected {
            assert!(
                !stolen.stop.contains(index),
                "protected {}",
                playing[*index].id
            );
            let row = content.sfx(&playing[*index].id).unwrap();
            assert!(row.is_warning || row.looped);
            assert!(!row.stealable || row.looped);
        }
        assert_eq!(
            playing.len() - stolen.stop.len() + stolen.play.len(),
            voice_cap
        );

        let held_loops: Vec<&str> = loop_ids
            .iter()
            .copied()
            .filter(|id| !content.sfx(id).unwrap().stealable)
            .collect();
        assert!(!held_loops.is_empty(), "the bed and the crowd loop stay");
        let mut blocked = Vec::new();
        while blocked.len() < voice_cap {
            let id = if blocked.len() % 2 == 0 {
                warning_ids[blocked.len() % warning_ids.len()]
            } else {
                held_loops[blocked.len() % held_loops.len()]
            };
            blocked.push(cue(id, blocked.len() as f32));
        }
        let dropped = admit_ui(&content.audio, &blocked, &asked, 100.0);
        assert!(
            dropped.play.is_empty(),
            "a full list of warnings and non-stealable loops drops the cue"
        );
        assert!(dropped.stop.is_empty());

        let mut reversed = vec![
            Heard {
                id: quiet.id.clone(),
                started: 4.0,
                order: 0,
            },
            Heard {
                id: quiet.id.clone(),
                started: 1.0,
                order: 9,
            },
        ];
        while reversed.len() < voice_cap {
            reversed.push(cue(warning_ids[reversed.len() % warning_ids.len()], 30.0));
        }
        let later = admit_ui(&content.audio, &reversed, &asked, 100.0);
        assert_eq!(
            later.stop,
            vec![1],
            "the stored start time beats list order"
        );

        let nest = content.sfx("hive_nest_idle").unwrap();
        assert!(nest.stealable && nest.looped, "the mix row is stealable");
        let mut nest_full = vec![cue(&nest.id, 0.0)];
        while nest_full.len() < voice_cap {
            nest_full.push(cue(
                warning_ids[nest_full.len() % warning_ids.len()],
                nest_full.len() as f32,
            ));
        }
        let took = admit_ui(&content.audio, &nest_full, &asked, 100.0);
        assert_eq!(
            took.stop,
            vec![0],
            "a stealable prop loop is taken from the row flag"
        );
        assert_eq!(took.play, vec![hover.clone()]);
        let bed = content.sfx("arena_01_bed").unwrap();
        assert!(!bed.stealable && bed.looped);
        let mut bed_full = vec![cue(&bed.id, 0.0)];
        while bed_full.len() < voice_cap {
            bed_full.push(cue(
                warning_ids[bed_full.len() % warning_ids.len()],
                bed_full.len() as f32,
            ));
        }
        let spared = admit_ui(&content.audio, &bed_full, &asked, 100.0);
        assert!(spared.play.is_empty(), "the bed row is not stealable");
        assert!(spared.stop.is_empty());
    }

    #[test]
    fn count_ticks_sit_outside_the_voice_cap_and_keep_one_instance() {
        use gemini_sim::content::Content;

        use super::{admit_ui, Heard};

        let content = Content::embedded();
        let tick = content.ui.sounds.count_tick.clone();
        let chime = content.ui.sounds.bank_chime.clone();
        let row = content.sfx(&tick).expect("count tick");
        assert!(row.outside_voice_cap);
        assert_eq!(row.max_instances, 1);
        assert_eq!(row.priority, 10);
        assert!(row.category_ui);
        assert!((row.volume - gemini_sim::audio_mix::db_to_linear(-16.0)).abs() < 1e-4);
        let chime_row = content.sfx(&chime).expect("bank chime");
        assert!(chime_row.outside_voice_cap);
        assert_eq!(chime_row.max_instances, 1);
        assert_eq!(chime_row.priority, 10);
        let filler = content
            .audio
            .sfx
            .iter()
            .find(|row| row.stealable && !row.looped && !row.is_warning && !row.outside_voice_cap)
            .expect("stealable one-shot");
        let playing: Vec<Heard> = (0..content.audio.voice_cap)
            .map(|index| Heard { id: filler.id.clone(), started: index as f32, order: index as u32 })
            .collect();
        let admitted = admit_ui(&content.audio, &playing, &[tick.clone()], 0.0);
        assert_eq!(admitted.play, vec![tick.clone()]);
        assert!(admitted.stop.is_empty(), "an outside cue does not steal a voice");
        let held = Heard { id: tick.clone(), started: 0.0, order: 1 };
        let second = admit_ui(&content.audio, &[held], &[tick, chime.clone()], 1.0);
        assert_eq!(second.play, vec![chime]);
        assert!(second.stop.is_empty());
    }

    #[test]
    fn pause_stops_solo_game_sounds_and_ducks_coop() {
        let content = gemini_sim::content::Content::embedded();
        let duck = content.presentation.hud.away.coop_pause_duck_db;
        let ducked = super::paused_bus(true, false, false, false, duck);
        assert!(
            ducked > 0.0 && ducked < 1.0,
            "co-op game sounds duck, got {ducked}"
        );
        assert_eq!(super::db_to_gain(duck), ducked);
        assert_eq!(
            super::paused_bus(true, true, false, false, duck),
            0.0,
            "solo pause stops game sounds"
        );
        let solo_db = content.presentation.hud.away.solo_pause_duck_db;
        let (gain, hz) = super::pause_music(
            true,
            true,
            1.0,
            20_000.0,
            solo_db,
            content.presentation.hud.away.music_duck_lowpass_hz,
            duck,
        );
        assert!((solo_db - -12.0).abs() < 1e-3);
        assert!(
            (gain - super::db_to_gain(solo_db)).abs() < 1e-4,
            "solo pause ducks music, got {gain}"
        );
        assert!(gain > 0.0);
        assert!((hz - content.presentation.hud.away.music_duck_lowpass_hz).abs() < 1e-3);
        assert_eq!(
            super::paused_bus(true, true, true, false, duck),
            1.0,
            "ui stays full in solo"
        );
        assert_eq!(
            super::paused_bus(true, false, false, true, duck),
            1.0,
            "warnings stay full in co-op"
        );
        assert_eq!(
            super::paused_bus(true, true, false, true, duck),
            1.0,
            "warnings stay full in solo"
        );
        assert_eq!(super::paused_bus(false, true, false, false, duck), 1.0);
    }

    #[test]
    fn sim_time_going_backwards_resyncs_instead_of_panicking() {
        let content = gemini_sim::content::Content::embedded();
        let mut sim = gemini_sim::sim::Sim::new(&content, "normal", 1);
        let period = content.arena_scene.beacon_glow_period;
        let stride = content.audio.footstep.stride;
        let fov = content.hero.viewmodel_fov;
        sim.time = period * 0.5;
        let player = sim.players[0].id;
        let game = crate::session::Game {
            content,
            sim,
            yaw: 0.0,
            pitch: 0.0,
            fov,
            bob: false,
            dip: 0.0,
            difficulty: "normal".into(),
            token: 0,
            seat: 0,
            pending_pick: None,
            match_id: None,
        };
        let cycle = (game.sim.time / period).floor() as i64;
        let warn = game.content.sfx("vent_warn").expect("vent_warn");
        let mut state = super::ArenaAudio {
            last_time: game.sim.time + period,
            travel: std::collections::BTreeMap::from([(player, stride)]),
            variant: std::collections::BTreeMap::from([(player, 1)]),
            beacon_cycle: cycle + 1,
            waiting: vec![gemini_sim::audio_mix::Voice {
                id: warn.id.clone(),
                score: warn.loudness,
                critical: warn.critical,
                crowd: warn.crowd,
                gain: warn.volume,
                pos: gemini_sim::math::Vec3::ZERO,
                priority: warn.priority,
                is_warning: warn.is_warning,
            }],
            stolen: vec![super::StolenLoop {
                id: "hive_nest_idle".into(),
                pos: gemini_sim::math::Vec3::new(1.0, 0.0, 0.0),
                priority: 10,
                elapsed: 0.2,
                order: 1,
            }],
        };
        let events = super::arena_cues(&mut state, &game);
        assert_eq!(state.last_time, game.sim.time);
        assert!(state.travel.is_empty());
        assert!(state.variant.is_empty());
        assert!(state.waiting.is_empty());
        assert!(state.stolen.is_empty());
        assert_eq!(state.beacon_cycle, cycle);
        assert!(events.iter().all(|event| !event.id.starts_with("step_")));
        assert!(events.iter().all(|event| event.id != "beacon_pulse"));
        assert!(events
            .iter()
            .any(|event| event.id == "hive_nest_idle" || event.id == "relay_tower_hum"));
    }

    #[test]
    fn a_replaced_warning_uses_the_content_fade() {
        let content = gemini_sim::content::Content::embedded();
        let mut shot = super::OneShot {
            id: "vent_warn".into(),
            owner: 0,
            started: 0.0,
            order: 0,
            gain: content.audio.warning_fade,
            pos: gemini_sim::math::Vec3::ZERO,
            priority: content.sfx("vent_warn").expect("vent_warn").priority,
            elapsed: 0.0,
            fade_left: 0.0,
            fade_dur: 0.0,
            fade_in: false,
            resume_at: None,
        };
        super::begin_warning_fade(&mut shot, content.audio.warning_fade);
        assert_eq!(shot.fade_dur, content.audio.warning_fade);
        assert_eq!(shot.fade_left, content.audio.warning_fade);
        let half = shot.fade_left * 0.5;
        let gain = super::warning_fade_gain(shot.gain, half, shot.fade_dur);
        assert!((gain - shot.gain * 0.5).abs() < 1e-4);
        assert_eq!(super::warning_fade_gain(shot.gain, 0.0, shot.fade_dur), 0.0);
    }

    #[test]
    fn arena_sounds_are_embedded_without_a_disk_fallback() {
        for name in super::ARENA_OGG {
            let bytes = super::arena_audio_bytes(name).unwrap_or_else(|err| panic!("{err}"));
            assert!(bytes.len() > 32, "{name}");
        }
        let err = super::arena_audio_bytes("missing.ogg").unwrap_err();
        assert!(err.contains("not embedded"), "{err}");
    }

    use std::time::Duration;

    use bevy::asset::AssetApp;

    fn play_app(listener: gemini_sim::math::Vec3) -> App {
        let content = gemini_sim::content::Content::embedded();
        let prefs = crate::settings::Prefs::defaults(&content);
        let view = gemini_sim::host_away::HostAwayClock::new(true).sample(&content.coop.host_away);
        let mut sim = gemini_sim::sim::Sim::new(&content, "normal", 1);
        sim.players[0].pos = listener;
        sim.players[0].vel = gemini_sim::math::Vec3::ZERO;
        let period = content.arena_scene.beacon_glow_period;
        let cycle = (sim.time / period).floor() as i64;
        let last_time = sim.time;
        let fov = content.hero.viewmodel_fov;
        let game = crate::session::Game {
            content,
            sim,
            yaw: 0.0,
            pitch: 0.0,
            fov,
            bob: false,
            dip: 0.0,
            difficulty: "normal".into(),
            token: 0,
            seat: 0,
            pending_pick: None,
            match_id: None,
        };
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin {
                file_path: "../../assets".into(),
                ..default()
            },
        ));
        app.init_asset::<bevy::audio::AudioSource>();
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_secs_f32(0.05),
        ));
        app.insert_resource(game);
        app.insert_resource(crate::session::Away {
            clock: gemini_sim::host_away::HostAwayClock::new(true),
            view,
        });
        app.insert_resource(crate::session::FrameFx::default());
        app.insert_resource(super::Music::default());
        app.insert_resource(super::Crowd::default());
        app.insert_resource(prefs);
        app.insert_resource(crate::session::PauseMenu::default());
        app.insert_resource(super::ArenaAudio {
            last_time,
            travel: std::collections::BTreeMap::new(),
            variant: std::collections::BTreeMap::new(),
            beacon_cycle: cycle,
            waiting: Vec::new(),
            stolen: Vec::new(),
        });
        app.add_systems(Update, super::play);
        app
    }

    fn fill_oneshots(app: &mut App, count: usize) {
        let row = gemini_sim::content::Content::embedded()
            .sfx("vent_burst")
            .expect("vent_burst")
            .clone();
        for i in 0..count {
            app.world_mut().spawn(super::OneShot {
                id: row.id.clone(),
                owner: i as u32 + 1,
                started: 0.0,
                order: super::next_voice_order(),
                gain: row.volume,
                pos: gemini_sim::math::Vec3::new(80.0 + i as f32, 0.0, 0.0),
                priority: row.priority,
                elapsed: 0.0,
                fade_left: 0.0,
                fade_dur: 0.0,
                fade_in: false,
                resume_at: None,
            });
        }
    }

    struct PropRow {
        id: String,
        pos: gemini_sim::math::Vec3,
        elapsed: f32,
        fade_left: f32,
        fade_dur: f32,
        fade_in: bool,
        resume_at: Option<f32>,
    }

    fn prop_rows(app: &mut App) -> Vec<PropRow> {
        let mut query = app
            .world_mut()
            .query::<(&super::PropLoop, &super::OneShot)>();
        query
            .iter(app.world())
            .map(|(_, shot)| PropRow {
                id: shot.id.clone(),
                pos: shot.pos,
                elapsed: shot.elapsed,
                fade_left: shot.fade_left,
                fade_dur: shot.fade_dur,
                fade_in: shot.fade_in,
                resume_at: shot.resume_at,
            })
            .collect()
    }

    fn warning_entities(app: &mut App) -> Vec<Entity> {
        let mut query = app
            .world_mut()
            .query::<(Entity, &super::OneShot, Option<&super::PropLoop>)>();
        query
            .iter(app.world())
            .filter(|(_, shot, prop)| shot.id == "vent_warn" && prop.is_none())
            .map(|(entity, _, _)| entity)
            .collect()
    }

    fn sound_at(id: &str, pos: gemini_sim::math::Vec3) -> gemini_sim::audio_mix::SoundEvent {
        gemini_sim::audio_mix::SoundEvent {
            id: id.to_string(),
            pos,
            owner: 0,
            priority: None,
        }
    }

    #[test]
    fn a_full_mix_fades_the_nest_for_a_warning_and_resumes_its_position() {
        let content = gemini_sim::content::Content::embedded();
        assert_eq!(
            content.sfx("vent_warn").expect("vent_warn").max_instances,
            3
        );
        assert_eq!(
            content
                .sfx("perch_climb_warn")
                .expect("perch")
                .max_instances,
            4
        );
        let warning_sum: u32 = content
            .audio
            .sfx
            .iter()
            .filter(|row| row.is_warning && row.id == row.row_id)
            .map(|row| row.max_instances)
            .sum();
        assert_eq!(warning_sum, 28);
        assert_eq!(content.audio.hit_reserve, 3);
        assert!((content.audio.loop_resume_fade - 0.3).abs() < 1e-6);
        let nest = content.sfx("hive_nest_idle").expect("nest");
        assert!(nest.stealable && nest.looped);
        let towers =
            gemini_sim::scenery::relay_points(&content.arena, &content.arena_scene).expect("relay");
        let mut app = play_app(towers[0]);
        for _ in 0..4 {
            app.update();
        }
        let playing = prop_rows(&mut app);
        assert!(
            playing.iter().any(|row| row.id == "hive_nest_idle"),
            "nest is playing in the full mix"
        );
        assert!(
            playing
                .iter()
                .any(|row| row.id == "hive_nest_idle" && row.elapsed > 0.0),
            "the nest loop has advanced"
        );
        let loops = playing.len();
        assert!(loops > 0);
        let fillers = content.audio.voice_cap - 1 - loops;
        fill_oneshots(&mut app, fillers);
        app.world_mut()
            .resource_mut::<crate::session::FrameFx>()
            .sounds
            .push(sound_at(
                "vent_warn",
                gemini_sim::math::Vec3::new(0.0, 0.0, 1.0),
            ));
        app.update();
        app.world_mut()
            .resource_mut::<crate::session::FrameFx>()
            .sounds
            .clear();
        let fading: Vec<_> = prop_rows(&mut app)
            .into_iter()
            .filter(|row| row.id == "hive_nest_idle" && row.fade_dur > 0.0 && !row.fade_in)
            .collect();
        assert_eq!(fading.len(), 1, "the nest fades out");
        let faded_at = fading[0].pos;
        assert!((fading[0].fade_dur - content.audio.warning_fade).abs() < 1e-4);
        assert!((fading[0].fade_left - content.audio.warning_fade).abs() < 1e-4);
        assert!(
            warning_entities(&mut app).is_empty(),
            "the warning waits out the fade"
        );
        assert!(app
            .world()
            .resource::<super::ArenaAudio>()
            .waiting
            .iter()
            .any(|voice| voice.id == "vent_warn"));

        let mut preserved = None;
        for _ in 0..4 {
            app.update();
            let gone = prop_rows(&mut app)
                .iter()
                .all(|row| !(row.id == "hive_nest_idle" && row.pos == faded_at));
            if gone && warning_entities(&mut app).len() == 1 {
                preserved = app
                    .world()
                    .resource::<super::ArenaAudio>()
                    .stolen
                    .iter()
                    .find(|row| row.id == "hive_nest_idle" && row.pos == faded_at)
                    .map(|row| row.elapsed);
                break;
            }
        }
        let preserved = preserved.expect("warning starts after the nest fade");
        assert!(preserved > 0.0, "the loop position was kept");

        for entity in warning_entities(&mut app) {
            app.world_mut().entity_mut(entity).despawn();
        }
        app.update();
        let back: Vec<_> = prop_rows(&mut app)
            .into_iter()
            .filter(|row| row.id == "hive_nest_idle" && row.pos == faded_at)
            .collect();
        assert_eq!(back.len(), 1, "the nest returns");
        assert!(back[0].fade_in, "fade in");
        assert!((back[0].fade_dur - content.audio.loop_resume_fade).abs() < 1e-4);
        assert!((back[0].fade_left - content.audio.loop_resume_fade).abs() < 1e-4);
        assert_eq!(back[0].resume_at, Some(preserved));
        assert!((back[0].elapsed - preserved).abs() < 1e-4);
        app.update();
        let fading_in: Vec<_> = prop_rows(&mut app)
            .into_iter()
            .filter(|row| row.id == "hive_nest_idle" && row.pos == faded_at)
            .collect();
        assert_eq!(fading_in.len(), 1, "the fade-in keeps the nest");
        assert!(fading_in[0].fade_in);
        assert!(fading_in[0].fade_left < content.audio.loop_resume_fade);
        assert!(fading_in[0].fade_left > 0.0);
    }

    fn park_stolen(app: &mut App, rows: Vec<super::StolenLoop>) {
        let content = gemini_sim::content::Content::embedded();
        let fillers = content.audio.voice_cap - 1 - 1;
        fill_oneshots(app, fillers);
        let sounds: Vec<_> = rows.iter().map(|row| sound_at(&row.id, row.pos)).collect();
        app.world_mut()
            .resource_mut::<crate::session::FrameFx>()
            .sounds = sounds;
        app.world_mut().resource_mut::<super::ArenaAudio>().stolen = rows;
    }

    #[test]
    fn a_free_voice_resumes_the_nearest_stolen_loop_and_ties_by_voice_id() {
        let content = gemini_sim::content::Content::embedded();
        let nest = content.sfx("hive_nest_idle").expect("nest").clone();
        let tower = content.sfx("relay_tower_hum").expect("tower").clone();
        assert!(nest.stealable && tower.stealable);
        assert!(nest.id.as_str() < tower.id.as_str());
        let near = gemini_sim::math::Vec3::new(1.0, 0.0, 0.0);
        let far = gemini_sim::math::Vec3::new(8.0, 0.0, 0.0);
        let mut app = play_app(gemini_sim::math::Vec3::ZERO);
        park_stolen(
            &mut app,
            vec![
                super::StolenLoop {
                    id: tower.id.clone(),
                    pos: near,
                    priority: tower.priority,
                    elapsed: 0.4,
                    order: 2,
                },
                super::StolenLoop {
                    id: nest.id.clone(),
                    pos: far,
                    priority: nest.priority,
                    elapsed: 0.7,
                    order: 1,
                },
            ],
        );
        app.update();
        let resumed: Vec<_> = prop_rows(&mut app)
            .into_iter()
            .filter(|row| row.pos == near)
            .collect();
        assert_eq!(resumed.len(), 1, "the nearer loop resumes");
        assert_eq!(resumed[0].id, tower.id);
        assert!(resumed[0].fade_in);
        assert_eq!(resumed[0].resume_at, Some(0.4));
        let waiting = &app.world().resource::<super::ArenaAudio>().stolen;
        assert!(waiting
            .iter()
            .any(|row| row.id == nest.id && row.pos == far));
        assert!(waiting.iter().all(|row| row.pos != near));

        let left = gemini_sim::math::Vec3::new(-3.0, 0.0, 0.0);
        let right = gemini_sim::math::Vec3::new(3.0, 0.0, 0.0);
        let mut tied = play_app(gemini_sim::math::Vec3::ZERO);
        park_stolen(
            &mut tied,
            vec![
                super::StolenLoop {
                    id: tower.id.clone(),
                    pos: right,
                    priority: tower.priority,
                    elapsed: 0.25,
                    order: 1,
                },
                super::StolenLoop {
                    id: nest.id.clone(),
                    pos: left,
                    priority: nest.priority,
                    elapsed: 0.55,
                    order: 9,
                },
            ],
        );
        tied.update();
        let resumed: Vec<_> = prop_rows(&mut tied)
            .into_iter()
            .filter(|row| row.fade_in)
            .collect();
        assert_eq!(resumed.len(), 1, "one voice frees");
        assert_eq!(
            resumed[0].id, nest.id,
            "equal distance keeps the lower voice id"
        );
        assert_eq!(resumed[0].pos, left);
        assert_eq!(resumed[0].resume_at, Some(0.55));
        let waiting = &tied.world().resource::<super::ArenaAudio>().stolen;
        assert!(waiting
            .iter()
            .any(|row| row.id == tower.id && row.pos == right));
        assert!(waiting.iter().all(|row| row.id != nest.id));
    }

    fn bare_game() -> crate::session::Game {
        let content = gemini_sim::content::Content::embedded();
        let fov = content.hero.viewmodel_fov;
        let sim = gemini_sim::sim::Sim::new(&content, "normal", 1);
        crate::session::Game {
            content,
            sim,
            yaw: 0.0,
            pitch: 0.0,
            fov,
            bob: false,
            dip: 0.0,
            difficulty: "normal".into(),
            token: 0,
            seat: 0,
            pending_pick: None,
            match_id: None,
        }
    }

    fn unknown_loop() -> gemini_sim::audio_mix::SoundEvent {
        gemini_sim::audio_mix::SoundEvent {
            id: "missing_loop".into(),
            pos: gemini_sim::math::Vec3::ZERO,
            owner: 0,
            priority: None,
        }
    }

    #[test]
    #[should_panic(expected = "voice missing_loop is not in the mix")]
    fn a_cold_stealable_loop_panics_on_an_unknown_id() {
        let game = bare_game();
        super::cold_stealable_loops(&game, &[unknown_loop()], &[]);
    }

    #[test]
    #[should_panic(expected = "voice missing_loop is not in the mix")]
    fn a_new_instance_cap_panics_on_an_unknown_id() {
        let game = bare_game();
        super::within_new_instance_cap(&game, &[], vec![unknown_loop()]);
    }
}
