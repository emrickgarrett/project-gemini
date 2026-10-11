//! Stems share a start instant. Bar lines come from each file's decoded length.
//! Sfx playback volume is the `volume` column. Charge stops drop that owner's voice
//! in the same frame the sim emits the stop.

use bevy::audio::{AudioSinkPlayback, Volume};
use bevy::prelude::*;

use gemini_sim::audio_mix::{select_voices, SoundEvent};
use gemini_sim::content::Content;
use gemini_sim::math::Vec3 as SimVec;
use gemini_sim::music::{
    bpm_sanity, combat_intensity, plan_switch, switch_clock, FileClock, MusicDirectorCfg, PlannedSwitch,
    StemMixer, SwitchClock, TrackDef, TransitionKind,
};
use gemini_sim::ogg::vorbis_seconds;
use gemini_sim::sim::MusicMoment;

use crate::session::{FrameFx, Game};

#[derive(Component)]
pub(crate) struct MusicStem {
    index: usize,
}

#[derive(Component)]
pub(crate) struct OneShot {
    id: String,
    owner: u32,
}

#[derive(Component)]
pub(crate) struct CrowdVoice;

#[derive(Resource, Default)]
pub struct Music {
    pub track: String,
    pub mixer: Option<StemMixer>,
    pub loops: Vec<f32>,
    pub entities: Vec<Entity>,
    pub pending: Option<String>,
    /// One-beat overlap. Outgoing stems ramp down while the new track ramps up.
    pub fade_left: f32,
    pub fade_dur: f32,
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
    fx: Res<FrameFx>,
    mut music: ResMut<Music>,
    mut crowd: ResMut<Crowd>,
    shots: Query<(Entity, &OneShot), Without<MusicStem>>,
    time: Res<Time>,
) {
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
            crowd_vol = def.volume * fullness;
            continue;
        }
        let mut speed = 1.0;
        if let Some((min, max)) = game.content.pitch_range(&voice.id) {
            let h = (fx.tick as u32).wrapping_mul(17).wrapping_add(shot_i.wrapping_mul(13));
            let t = (h % 1000) as f32 / 1000.0;
            speed = min + (max - min) * t;
            shot_i += 1;
        }
        let atten = attenuate(&game.content, voice.pos, listener);
        let volume = def.volume * atten;
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

pub fn footstep(commands: &mut Commands, assets: &AssetServer, game: &Game, id: &str, pos: SimVec) {
    let Some(def) = game.content.sfx(id) else { return };
    let listener = listener_of(&game);
    let volume = def.volume * attenuate(&game.content, pos, listener);
    commands.spawn((
        AudioPlayer::new(assets.load::<AudioSource>(def.path.clone())),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
    ));
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
    );
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
                    AudioPlayer::new(assets.load::<AudioSource>(path)),
                    PlaybackSettings::LOOP.with_volume(Volume::Linear(0.85)),
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
                    AudioPlayer::new(assets.load::<AudioSource>(path.clone())),
                    PlaybackSettings::LOOP.with_volume(Volume::Linear(gain)),
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

pub fn sync_gains(music: Res<Music>, mut stems: Query<(&MusicStem, &mut AudioSink)>) {
    let Some(mixer) = &music.mixer else { return };
    for (stem, mut sink) in &mut stems {
        if let Some(voice) = mixer.voices.get(stem.index) {
            sink.set_volume(Volume::Linear(voice.current.clamp(0.0, 1.0)));
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
