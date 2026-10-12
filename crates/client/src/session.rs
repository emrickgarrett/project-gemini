//! Fixed-timestep match. Look and presentation live here; the sim owns the rules.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashSet;

use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
#[cfg(not(target_arch = "wasm32"))]
use bevy::window::WindowOccluded;

#[cfg(not(target_arch = "wasm32"))]
use gemini_meshes::raster_bug;
use gemini_sim::audio_mix::SoundEvent;
use gemini_sim::content::Content;
use gemini_sim::host_away::{on_host_visibility_changed, AwayPhase, HostAwayClock};
use gemini_sim::math::Vec3 as SimVec3;
use gemini_sim::sim::{countdown_aim, HitPing, MusicMoment, Phase, PlayerInput, Sim, SoundStop, SIM_DT};

use crate::act_now::{self, ActNow};
use crate::settings::Prefs;
use crate::AppState;

#[derive(Resource)]
pub struct Game {
    pub content: Content,
    pub sim: Sim,
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
    pub bob: bool,
    pub dip: f32,
    pub difficulty: String,
    pub token: u64,
    pub seat: usize,
}

#[derive(Resource, Clone)]
pub enum Launch {
    Solo,
    #[cfg(not(target_arch = "wasm32"))]
    Host(String),
    #[cfg(not(target_arch = "wasm32"))]
    Join { addr: String, token: Option<u64> },
}

#[derive(Resource)]
pub enum CoopLink {
    Solo,
    #[cfg(not(target_arch = "wasm32"))]
    Host(crate::net::Host),
    #[cfg(not(target_arch = "wasm32"))]
    Client(crate::net::Client),
}

impl Game {
    pub fn local(&self) -> Option<&gemini_sim::sim::Player> {
        self.sim
            .players
            .iter()
            .find(|player| player.token == self.token)
            .or_else(|| self.sim.players.first())
    }
}

pub fn launch_from_env() -> Launch {
    #[cfg(target_arch = "wasm32")]
    {
        Launch::Solo
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::net::launch_from_env()
    }
}

pub fn autoplay(mut commands: Commands, launch: Res<Launch>, mut next: ResMut<NextState<AppState>>, mut opened: Local<bool>) {
    if !*opened {
        if let Ok(spec) = std::env::var("GEMINI_REVEAL") {
            if let Some(run) = crate::reveal_ui::from_env(&spec) {
                *opened = true;
                commands.insert_resource(crate::reveal_ui::RevealPlayback(run));
                next.set(AppState::Reveal);
                return;
            }
        }
    }
    if !matches!(*launch, Launch::Solo) {
        next.set(AppState::Playing);
    }
}

#[derive(Resource, Default)]
pub struct FrameFx {
    pub sounds: Vec<SoundEvent>,
    pub stops: Vec<SoundStop>,
    pub pings: Vec<HitPing>,
    pub moments: Vec<MusicMoment>,
    pub tick: u64,
}

#[derive(Resource)]
pub struct PendingDifficulty(pub String);

/// Pause menu on this client. Solo matches stop stepping while it is open.
/// Co-op keeps the shared sim running and does not send a pause.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct PauseMenu {
    pub open: bool,
    pub settings: bool,
}

/// Leftover time since the last sim step. A frame faster than `SIM_DT` must keep
/// this, or a 60 Hz display never reaches a tick and the match looks frozen.
#[derive(Resource, Default)]
pub struct SimClock {
    acc: f32,
}

impl SimClock {
    pub fn reset(&mut self) {
        self.acc = 0.0;
    }

    pub fn accumulated(&self) -> f32 {
        self.acc
    }

    pub(crate) fn set_accumulated(&mut self, acc: f32) {
        self.acc = acc;
    }
}

/// Duplicate rank supplied to the match. The client does not roll crates.
#[derive(Resource)]
pub struct Account {
    pub rank: u32,
}

pub fn boot(
    mut commands: Commands,
    pending: Res<PendingDifficulty>,
    mut clock: ResMut<SimClock>,
    launch: Res<Launch>,
    account: Res<Account>,
    prefs: Res<Prefs>,
) {
    clock.reset();
    let content = Content::embedded();
    let mut sim = Sim::new(&content, &pending.0, 7);
    sim.apply_persistent_rank(&content, account.rank);
    let (sim, token, link) = open_link(&launch, &content, sim);
    let solo = sim.connected_count() <= 1;
    let away_view = {
        let clock = HostAwayClock::new(solo);
        clock.sample(&content.coop.host_away)
    };
    commands.insert_resource(Game {
        content,
        sim,
        yaw: 0.0,
        pitch: 0.0,
        fov: prefs.fov,
        bob: prefs.view_bob,
        dip: 0.0,
        difficulty: pending.0.clone(),
        token,
        seat: 0,
    });
    commands.insert_resource(link);
    commands.insert_resource(FrameFx::default());
    commands.insert_resource(Away {
        clock: HostAwayClock::new(solo),
        view: away_view,
    });
    commands.insert_resource(Page::default());
    commands.insert_resource(ActNow::default());
}

fn open_link(launch: &Launch, content: &Content, sim: Sim) -> (Sim, u64, CoopLink) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::net::prepare(launch, content, sim)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (launch, content);
        let token = sim.players.first().map(|player| player.token).unwrap_or(1);
        (sim, token, CoopLink::Solo)
    }
}

#[derive(Resource)]
pub struct Away {
    pub clock: HostAwayClock,
    pub view: gemini_sim::host_away::AwayFrame,
}

#[derive(Resource, Default)]
pub struct Page {
    hidden: bool,
    #[cfg(not(target_arch = "wasm32"))]
    occluded: HashSet<Entity>,
}

pub fn gather_and_step(
    time: Res<Time>,
    mut clock: ResMut<SimClock>,
    mut game: ResMut<Game>,
    mut link: ResMut<CoopLink>,
    mut fx: ResMut<FrameFx>,
    mut away: ResMut<Away>,
    mut page: ResMut<Page>,
    mut act: ResMut<ActNow>,
    #[cfg(not(target_arch = "wasm32"))] mut occluded: MessageReader<WindowOccluded>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    gamepads: Query<&Gamepad>,
    mut cursor: Query<&mut CursorOptions>,
    hold: Res<PauseMenu>,
    prefs: Res<Prefs>,
) {
    // The pause menu owns the cursor. A click on a menu button must not grab it again.
    // Wasm waits for the click-to-play control so the browser can grant the lock.
    if !hold.open && !cfg!(target_arch = "wasm32") && mouse_buttons.just_pressed(MouseButton::Left) {
        if let Ok(mut cursor) = cursor.single_mut() {
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if keys.just_pressed(KeyCode::F10) {
        save_bug_stills();
    }

    let sens = game.content.presentation.mouse_sensitivity * prefs.sensitivity;
    let pitch_limit = game.content.presentation.pitch_limit_deg.to_radians();
    let fov_min = game.content.presentation.fov_min;
    let fov_max = game.content.presentation.fov_max;
    let look_deg = game.content.presentation.gamepad_look_deg_per_sec;
    let pad = gamepads.iter().next();
    let using_pad = pad.is_some()
        && pad
            .map(|p| {
                p.get(GamepadAxis::LeftStickX).unwrap_or(0.0).abs() > 0.2
                    || p.get(GamepadAxis::LeftStickY).unwrap_or(0.0).abs() > 0.2
                    || p.get(GamepadAxis::RightStickX).unwrap_or(0.0).abs() > 0.05
                    || p.pressed(GamepadButton::South)
                    || p.pressed(GamepadButton::RightTrigger2)
            })
            .unwrap_or(false);

    if !using_pad && !hold.open {
        let y_sign = if prefs.invert_y { -1.0 } else { 1.0 };
        game.yaw -= motion.delta.x * sens;
        game.pitch -= motion.delta.y * sens * y_sign;
        game.pitch = game.pitch.clamp(-pitch_limit, pitch_limit);
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        game.fov = (game.fov + 5.0).min(fov_max);
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        game.fov = (game.fov - 5.0).max(fov_min);
    }
    if keys.just_pressed(KeyCode::KeyV) {
        game.bob = !game.bob;
    }

    let mut move_x = axis(keys.pressed(KeyCode::KeyD), keys.pressed(KeyCode::KeyA));
    let mut move_z = axis(keys.pressed(KeyCode::KeyW), keys.pressed(KeyCode::KeyS));
    let mut look_dx = 0.0;
    let mut look_dy = 0.0;
    let mut jump = keys.just_pressed(KeyCode::Space);
    let mut sprint = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let mut crouch = keys.pressed(KeyCode::ControlLeft);
    let mut boost_held = keys.pressed(KeyCode::KeyF);
    let mut boost_pressed = keys.just_pressed(KeyCode::KeyF);
    let mut fire = mouse_buttons.pressed(MouseButton::Left);
    let mut reload = keys.just_pressed(KeyCode::KeyR);
    let mut ability = keys.just_pressed(KeyCode::KeyQ);
    let mut interact = keys.pressed(KeyCode::KeyE);
    if let Some(pad) = pad {
        move_x += pad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
        move_z += pad.get(GamepadAxis::LeftStickY).unwrap_or(0.0);
        let look = look_deg * time.delta_secs();
        look_dx += pad.get(GamepadAxis::RightStickX).unwrap_or(0.0) * look;
        look_dy += pad.get(GamepadAxis::RightStickY).unwrap_or(0.0) * look;
        jump |= pad.just_pressed(GamepadButton::South);
        sprint |= pad.pressed(GamepadButton::LeftTrigger2);
        boost_held |= pad.pressed(GamepadButton::RightTrigger);
        boost_pressed |= pad.just_pressed(GamepadButton::RightTrigger);
        fire |= pad.pressed(GamepadButton::RightTrigger2);
        reload |= pad.just_pressed(GamepadButton::West);
        ability |= pad.just_pressed(GamepadButton::North);
        interact |= pad.pressed(GamepadButton::East);
    }

    let mut choice = None;
    if matches!(game.sim.phase, Phase::Upgrade { .. }) {
        if keys.just_pressed(KeyCode::Digit1) {
            choice = Some(0);
        } else if keys.just_pressed(KeyCode::Digit2) {
            choice = Some(1);
        } else if keys.just_pressed(KeyCode::Digit3) {
            choice = Some(2);
        }
    }
    if hold.open {
        crouch = false;
        move_x = 0.0;
        move_z = 0.0;
        look_dx = 0.0;
        look_dy = 0.0;
        jump = false;
        sprint = false;
        boost_held = false;
        boost_pressed = false;
        fire = false;
        reload = false;
        ability = false;
        interact = false;
        choice = None;
    }

    let mut input = PlayerInput {
        move_x,
        move_z,
        yaw: game.yaw,
        pitch: game.pitch,
        look_dx_deg: look_dx,
        look_dy_deg: look_dy,
        jump_pressed: jump,
        sprint,
        crouch,
        boost_held,
        boost_pressed,
        fire,
        reload,
        ability,
        interact_held: interact,
        upgrade_choice: choice,
        using_gamepad: using_pad,
    };

    let rules = game.content.coop.host_away.clone();
    away.clock.set_solo(game.sim.connected_count() <= 1);
    let mut cues = Vec::new();
    let mut stops = Vec::new();
    #[cfg(not(target_arch = "wasm32"))]
    let mut announce = false;
    #[cfg(not(target_arch = "wasm32"))]
    let samples = {
        for msg in occluded.read() {
            if msg.occluded {
                page.occluded.insert(msg.window);
            } else {
                page.occluded.remove(&msg.window);
            }
        }
        page.hidden = !page.occluded.is_empty();
        vec![(page.hidden, time.elapsed_secs())]
    };
    #[cfg(target_arch = "wasm32")]
    let samples = page_visibility::samples();
    match &mut *link {
        #[cfg(not(target_arch = "wasm32"))]
        CoopLink::Client(client) => {
            let (client_cues, client_stops) = client_away(&mut game, client, &mut away, page.hidden, time.delta_secs());
            cues = client_cues;
            stops = client_stops;
        }
        _ => {
            for (hidden, now) in samples {
                page.hidden = hidden;
                let effect = on_host_visibility_changed(&mut away.clock, hidden, now, &rules, &game.sim);
                if effect.frame.ended {
                    away.clock.finish(&mut game.sim);
                }
                cues.extend(effect.frame.cues);
                stops.extend(effect.frame.stops);
                #[cfg(not(target_arch = "wasm32"))]
                {
                    announce |= effect.send_host_away;
                }
            }
        }
    }
    away.view = away.clock.sample(&rules);

    // Solo pause stops the sim and does not bank time for a catch-up. Co-op keeps stepping
    // with an empty local input and never marks the match paused.
    if hold.open && game.sim.connected_count() <= 1 {
        fx.sounds.clear();
        fx.stops.clear();
        fx.pings.clear();
        fx.moments.clear();
        return;
    }

    let dt = time.delta_secs();
    clock.acc += dt;
    let max_steps = game.content.controller.sim.max_sim_steps_per_frame;
    let mut steps = 0u32;
    fx.sounds.clear();
    fx.stops.clear();
    fx.pings.clear();
    fx.moments.clear();
    for id in cues {
        fx.sounds.push(SoundEvent {
            id,
            pos: SimVec3::ZERO,
            owner: 0,
        });
    }
    for id in stops {
        fx.stops.push(SoundStop { id, owner: 0 });
    }
    fx.moments.append(&mut game.sim.music);
    let counting = away.clock.phase() == AwayPhase::Countdown;
    game.sim.present_countdown = counting;
    if counting {
        input = countdown_aim(input);
    }
    if away.clock.frozen() {
        // The banner freezes the world. The countdown stays frozen too, and only aim moves.
        game.sim.host_away = !counting;
        if counting {
            let Game { sim, content, .. } = &mut *game;
            sim.step_peers(content, &[input]);
        }
        match &mut *link {
            CoopLink::Solo => {}
            #[cfg(not(target_arch = "wasm32"))]
            CoopLink::Host(host) => {
                let mode = if counting {
                    crate::net::HostStep::SignalBack
                } else {
                    crate::net::HostStep::Hold { announce }
                };
                host.tick(&mut game, input, mode);
            }
            #[cfg(not(target_arch = "wasm32"))]
            CoopLink::Client(client) => client.send(game.sim.tick, input),
        }
    } else {
        game.sim.host_away = false;
        game.sim.present_countdown = false;
        #[cfg(not(target_arch = "wasm32"))]
        if away.clock.phase() == AwayPhase::Live {
            if let CoopLink::Client(client) = &mut *link {
                let before = act_now::seats(&game.sim);
                let started = game.sim.time;
                client.apply(&mut game);
                let elapsed = game.sim.time - started;
                watch_act_now(&mut act, &game, interact, &before, elapsed);
            }
        }
        // Sim::new emits wave_start before the first tick. step() clears sounds on entry,
        // so keep that cue or the match-start sting never plays.
        fx.sounds.append(&mut game.sim.sounds);
        while clock.acc >= SIM_DT && steps < max_steps {
            let before = act_now::seats(&game.sim);
            let started = game.sim.time;
            let stepping = steps_sim(&link);
            drive(&mut game, &mut link, input);
            if stepping {
                let elapsed = game.sim.time - started;
                watch_act_now(&mut act, &game, interact, &before, elapsed);
            }
            fx.sounds.append(&mut game.sim.sounds);
            fx.stops.append(&mut game.sim.stops);
            fx.pings.append(&mut game.sim.pings);
            fx.moments.append(&mut game.sim.music);
            let landed = game.sim.landed_pulse;
            if landed > 0.0 {
                game.dip = 1.0;
            }
            input.jump_pressed = false;
            input.boost_pressed = false;
            input.upgrade_choice = None;
            input.look_dx_deg = 0.0;
            input.look_dy_deg = 0.0;
            clock.acc -= SIM_DT;
            steps += 1;
        }
    }
    // A hitch longer than the cap must not catch up on later frames.
    if steps == max_steps && clock.acc >= SIM_DT {
        clock.acc %= SIM_DT;
    }
    fx.tick = game.sim.tick;
    if using_pad {
        let aim = game.local().map(|player| (player.yaw, player.pitch));
        if let Some((yaw, pitch)) = aim {
            game.yaw = yaw;
            game.pitch = pitch;
        }
    }
    game.dip = (game.dip - time.delta_secs() * 6.0).max(0.0);
}

fn steps_sim(link: &CoopLink) -> bool {
    match link {
        CoopLink::Solo => true,
        #[cfg(not(target_arch = "wasm32"))]
        CoopLink::Host(_) => true,
        #[cfg(not(target_arch = "wasm32"))]
        CoopLink::Client(_) => false,
    }
}

fn watch_act_now(act: &mut ActNow, game: &Game, interact: bool, before: &[act_now::Seat], elapsed: f32) {
    let Some(ability) = game.content.ability_of(&game.content.hero.id) else {
        return;
    };
    act_now::note(
        act,
        game.content.warnings,
        ability,
        game.content.combat.revive_range,
        game.token,
        interact,
        before,
        &act_now::seats(&game.sim),
        game.sim.time,
        elapsed,
    );
}

fn axis(pos: bool, neg: bool) -> f32 {
    (pos as i32 - neg as i32) as f32
}

#[cfg(not(target_arch = "wasm32"))]
fn save_bug_stills() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let _ = std::fs::create_dir_all("art/bugs");
    for id in ["swarmer", "spitter", "brute", "brood_mother", "hive_tyrant"] {
        let rgb = raster_bug(id, 640, 480);
        let path = format!("art/bugs/{id}.png");
        let mut child = match Command::new("ffmpeg")
            .args(["-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", "640x480", "-i", "pipe:0", &path])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => {
                bevy::log::warn!("ffmpeg missing; F10 could not write {path}");
                return;
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(&rgb);
        }
        let _ = child.wait();
        bevy::log::info!("wrote {path}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn client_away(game: &mut Game, client: &mut crate::net::Client, away: &mut Away, hidden: bool, dt: f32) -> (Vec<String>, Vec<String>) {
    let rules = game.content.coop.host_away.clone();
    let heard = client.poll(&game.content);
    let host_live = heard.snapshot_away == Some(false);
    let starting = host_live && away.clock.phase() == AwayPhase::Away && !hidden;
    away.clock.set_page_hidden(hidden);
    if heard.away_packet {
        away.clock.note_away_packet(rules.detect_after_seconds);
    }
    if let Some(flag) = heard.snapshot_away {
        away.clock.note_snapshot(flag, rules.resume_countdown_seconds);
    }
    away.clock.set_silent(!host_live || hidden);
    let step_dt = if starting { 0.0 } else { dt };
    let frame = away.clock.tick(step_dt, &rules, &game.sim);
    if frame.ended {
        away.clock.finish(&mut game.sim);
    }
    (frame.cues, frame.stops)
}

#[cfg(target_arch = "wasm32")]
mod page_visibility {
    use std::cell::RefCell;
    use std::sync::Once;

    use wasm_bindgen::JsCast;

    thread_local! {
        static EVENTS: RefCell<Vec<(bool, f32)>> = const { RefCell::new(Vec::new()) };
    }

    pub fn samples() -> Vec<(bool, f32)> {
        install();
        let mut queued = EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
        if queued.is_empty() {
            queued.push((document_hidden(), now_secs()));
        }
        queued
    }

    fn install() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let Some(window) = web_sys::window() else { return };
            let Some(document) = window.document() else { return };
            // The browser callback records (hidden, now). The match reader applies each
            // sample with on_host_visibility_changed, the same function WindowOccluded uses.
            let closure = wasm_bindgen::closure::Closure::wrap(Box::new(|| {
                EVENTS.with(|events| events.borrow_mut().push((document_hidden(), now_secs())));
            }) as Box<dyn FnMut()>);
            let _ = document.add_event_listener_with_callback("visibilitychange", closure.as_ref().unchecked_ref());
            closure.forget();
        });
    }

    fn document_hidden() -> bool {
        web_sys::window().and_then(|window| window.document()).map(|document| document.hidden()).unwrap_or(false)
    }

    fn now_secs() -> f32 {
        web_sys::window()
            .and_then(|window| window.performance())
            .map(|performance| (performance.now() / 1000.0) as f32)
            .unwrap_or(0.0)
    }
}

fn drive(game: &mut Game, link: &mut CoopLink, input: PlayerInput) {
    match link {
        CoopLink::Solo => game.sim.step(&game.content, input),
        #[cfg(not(target_arch = "wasm32"))]
        CoopLink::Host(host) => host.tick(game, input, crate::net::HostStep::Simulate),
        #[cfg(not(target_arch = "wasm32"))]
        CoopLink::Client(client) => client.send(game.sim.tick, input),
    }
    if let Some(seat) = game.sim.players.iter().position(|player| player.token == game.token) {
        game.seat = seat;
    }
}

pub fn shutdown(mut commands: Commands, mut clock: ResMut<SimClock>) {
    clock.reset();
    commands.remove_resource::<Game>();
    commands.remove_resource::<FrameFx>();
    commands.remove_resource::<CoopLink>();
    commands.remove_resource::<Away>();
    commands.remove_resource::<Page>();
    commands.remove_resource::<ActNow>();
}
