//! Fixed-timestep match. Look and presentation live here; the sim owns the rules.

use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

#[cfg(not(target_arch = "wasm32"))]
use gemini_meshes::raster_bug;
use gemini_sim::audio_mix::SoundEvent;
use gemini_sim::content::Content;
use gemini_sim::sim::{HitPing, MusicMoment, Phase, PlayerInput, Sim, SoundStop, SIM_DT};

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

pub fn boot(mut commands: Commands, pending: Res<PendingDifficulty>, mut clock: ResMut<SimClock>) {
    clock.reset();
    let content = Content::embedded();
    let sim = Sim::new(&content, &pending.0, 7);
    let fov = content.controller.camera.fov_default;
    commands.insert_resource(Game {
        content,
        sim,
        yaw: 0.0,
        pitch: 0.0,
        fov,
        bob: true,
        dip: 0.0,
        difficulty: pending.0.clone(),
    });
    commands.insert_resource(FrameFx::default());
}

pub fn gather_and_step(
    time: Res<Time>,
    mut clock: ResMut<SimClock>,
    mut game: ResMut<Game>,
    mut fx: ResMut<FrameFx>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    gamepads: Query<&Gamepad>,
    mut cursor: Query<&mut CursorOptions>,
    mut next: ResMut<NextState<AppState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        if let Ok(mut cursor) = cursor.single_mut() {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
    }
    if mouse_buttons.just_pressed(MouseButton::Left) {
        if let Ok(mut cursor) = cursor.single_mut() {
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if keys.just_pressed(KeyCode::F10) {
        save_bug_stills();
    }

    let sens = game.content.presentation.mouse_sensitivity;
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

    if !using_pad {
        game.yaw -= motion.delta.x * sens;
        game.pitch -= motion.delta.y * sens;
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
    let crouch = keys.pressed(KeyCode::ControlLeft);
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
    if matches!(game.sim.phase, Phase::Victory | Phase::Defeat) && keys.just_pressed(KeyCode::Enter) {
        next.set(AppState::Menu);
        return;
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

    clock.acc += time.delta_secs();
    let max_steps = game.content.controller.sim.max_sim_steps_per_frame;
    let mut steps = 0u32;
    fx.sounds.clear();
    fx.stops.clear();
    fx.pings.clear();
    fx.moments.clear();
    // Sim::new emits wave_start before the first tick. step() clears sounds on entry,
    // so keep that cue or the match-start sting never plays.
    fx.sounds.append(&mut game.sim.sounds);
    while clock.acc >= SIM_DT && steps < max_steps {
        {
            let Game { sim, content, .. } = &mut *game;
            sim.step(content, input);
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
    // A hitch longer than the cap must not catch up on later frames.
    if steps == max_steps && clock.acc >= SIM_DT {
        clock.acc %= SIM_DT;
    }
    fx.tick = game.sim.tick;
    if using_pad {
        let aim = game.sim.players.first().map(|player| (player.yaw, player.pitch));
        if let Some((yaw, pitch)) = aim {
            game.yaw = yaw;
            game.pitch = pitch;
        }
    }
    game.dip = (game.dip - time.delta_secs() * 6.0).max(0.0);
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

pub fn shutdown(mut commands: Commands, mut clock: ResMut<SimClock>) {
    clock.reset();
    commands.remove_resource::<Game>();
    commands.remove_resource::<FrameFx>();
}
