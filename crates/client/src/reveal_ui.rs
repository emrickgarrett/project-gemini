//! Full-screen playback of a server crate result. Timings, sounds, and colors come from the tables.
//! The ten-pull grid and the refund panel are the same frames `art/ten_pull` is rendered from.

use bevy::asset::RenderAssetUsages;
use bevy::audio::{AudioSink, Volume};
use bevy::camera::ClearColorConfig;
use bevy::image::Image;
use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::picking::hover::Hovered;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::GlobalZIndex;
use bevy::ui_widgets::Button;

use gemini_meshes::{paint_grid, paint_refund, GridStill, RefundStill};
use gemini_sim::content::{shortfall_line, Content};
use gemini_sim::reveal::{look, GridRun, RefundRun, RevealRun};

use crate::flow::{self, HitButton};
use crate::settings::Prefs;
use crate::AppState;

#[derive(Clone, Debug)]
pub enum RevealShow {
    Single(RevealRun),
    Grid(GridRun),
    Refund(RefundRun),
}

#[derive(Resource)]
pub struct RevealPlayback {
    pub show: RevealShow,
    /// Idempotency key this playback is showing. Empty on a refund.
    pub key: String,
}

/// Where Enter returns after the card. The crates screen stays on its route.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RevealFrom {
    #[default]
    Menu,
    Crates,
}

#[derive(Component)]
pub(crate) struct RevealRoot;

#[derive(Component)]
pub(crate) struct RevealCam;

#[derive(Component)]
pub(crate) struct RevealVoice {
    pub id: String,
    /// Pull that started this voice. A later pull fades every voice from an older one.
    pub gen: u64,
    pub gain: f32,
}

#[derive(Component)]
pub(crate) struct FadingVoice {
    left: f32,
    dur: f32,
    gain: f32,
}

#[derive(Resource, Clone, PartialEq, Eq)]
pub(crate) struct PaintKey {
    kind: u8,
    width: u32,
    height: u32,
    focus: usize,
    ready: bool,
    again: bool,
    shortfall: String,
    failure: String,
    items: Vec<String>,
}

pub fn enter(mut commands: Commands, mut focus: ResMut<flow::Focus>, mut hits: ResMut<flow::Hits>, existing: Query<Entity, With<RevealCam>>) {
    focus.0 = 0;
    hits.0.clear();
    commands.remove_resource::<PaintKey>();
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        RevealCam,
        Camera2d,
        Camera {
            order: 5,
            clear_color: ClearColorConfig::Default,
            ..default()
        },
        IsDefaultUiCamera,
    ));
}

pub fn drive(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    assets: Res<AssetServer>,
    mut playback: ResMut<RevealPlayback>,
    mut next: ResMut<NextState<AppState>>,
    mut crate_box: ResMut<flow::ServerCrate>,
    voices: Query<Entity, With<RevealVoice>>,
    roots: Query<Entity, With<RevealRoot>>,
    mut prefs: ResMut<Prefs>,
) {
    let content = Content::embedded();
    let dt = time.delta_secs();
    let bus = crate::audio::bus_gain(&prefs, false);
    let gen = crate_box.reveal_gen;
    let showing = playback.key.clone();
    let in_flight = crate_box.pull_busy;
    match &mut playback.show {
        RevealShow::Single(run) => {
            let skip = keys.just_pressed(KeyCode::Space) || mouse.just_pressed(MouseButton::Left);
            if skip {
                for entity in &voices {
                    commands.entity(entity).despawn();
                }
            }
            let cues = if skip { run.skip() } else { run.advance(dt) };
            for id in cues {
                play_cue(&mut commands, &assets, &content, &id, bus, gen);
            }
            if run.time >= run.plan.card_at {
                flow::mark_pull_seen(&mut prefs, &showing, in_flight);
                if keys.just_pressed(KeyCode::Enter) {
                    next.set(AppState::Menu);
                    return;
                }
            }
            for entity in &roots {
                commands.entity(entity).despawn();
            }
            paint_single(&mut commands, &content, run);
        }
        RevealShow::Grid(run) => {
            // Space and a click skip only while the best face is still down.
            let skip = !run.finished() && (keys.just_pressed(KeyCode::Space) || mouse.just_pressed(MouseButton::Left));
            if skip {
                for entity in &voices {
                    commands.entity(entity).despawn();
                }
            }
            let cues = if skip { run.skip() } else { run.advance(dt) };
            for id in cues {
                play_cue(&mut commands, &assets, &content, &id, bus, gen);
            }
            crate_box.buttons_ready = run.finished();
            if run.finished() {
                flow::mark_pull_seen(&mut prefs, &showing, in_flight);
            }
        }
        RevealShow::Refund(run) => {
            let cues = run.advance(dt);
            for id in cues {
                play_cue(&mut commands, &assets, &content, &id, bus, gen);
            }
            crate_box.buttons_ready = true;
        }
    }
}

pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    hits: Res<flow::Hits>,
    crate_box: Res<flow::ServerCrate>,
    mut focus: ResMut<flow::Focus>,
    mut action: ResMut<flow::PendingAction>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    prefs: Res<Prefs>,
    mut repeat: Local<f32>,
) {
    if !crate_box.buttons_ready || hits.0.is_empty() || action.0.is_some() {
        return;
    }
    let content = Content::embedded();
    let mut dir = 0i32;
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::Tab) {
        dir = 1;
    } else if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::ArrowLeft) {
        dir = -1;
    }
    let pad = pads.iter().next();
    if let Some(pad) = pad {
        if pad.just_pressed(GamepadButton::DPadDown) || pad.just_pressed(GamepadButton::DPadRight) {
            dir = 1;
        }
        if pad.just_pressed(GamepadButton::DPadUp) || pad.just_pressed(GamepadButton::DPadLeft) {
            dir = -1;
        }
        let y = pad.get(bevy::input::gamepad::GamepadAxis::LeftStickY).unwrap_or(0.0);
        *repeat -= time.delta_secs();
        if y.abs() > content.ui.layout.stick_deadzone && *repeat <= 0.0 {
            dir = if y > 0.0 { -1 } else { 1 };
            *repeat = content.ui.layout.nav_repeat_secs;
        }
    }
    if dir != 0 {
        let n = hits.0.len() as i32;
        focus.0 = (focus.0 as i32 + dir).rem_euclid(n) as usize;
        crate::audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.ui.sounds.hover);
    }
    if focus.0 >= hits.0.len() {
        focus.0 = 0;
    }
    let activate = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) || pad.is_some_and(|pad| pad.just_pressed(GamepadButton::South));
    if activate {
        action.0 = Some(hits.0[focus.0].id.clone());
    }
}

pub fn mark_fades(crate_box: Res<flow::ServerCrate>, mut commands: Commands, voices: Query<(Entity, &RevealVoice), Without<FadingVoice>>) {
    let dur = Content::embedded().audio.warning_fade;
    for (entity, voice) in &voices {
        if voice.gen == crate_box.reveal_gen {
            continue;
        }
        commands.entity(entity).insert(FadingVoice { left: dur, dur, gain: voice.gain });
    }
}

pub fn fade_voices(time: Res<Time>, mut commands: Commands, mut fading: Query<(Entity, &mut FadingVoice, Option<&mut AudioSink>)>) {
    let dt = time.delta_secs();
    for (entity, mut fade, sink) in &mut fading {
        fade.left -= dt;
        let gain = if fade.dur > 0.0 { fade.gain * (fade.left / fade.dur).clamp(0.0, 1.0) } else { 0.0 };
        if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(gain));
        }
        if fade.left <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

pub fn paint(
    mut commands: Commands,
    playback: Option<Res<RevealPlayback>>,
    crate_box: Res<flow::ServerCrate>,
    prefs: Res<Prefs>,
    focus: Res<flow::Focus>,
    mut hits: ResMut<flow::Hits>,
    mut cached: Option<ResMut<PaintKey>>,
    windows: Query<&Window>,
    mut images: Option<ResMut<Assets<Image>>>,
    roots: Query<Entity, With<RevealRoot>>,
    cams: Query<Entity, With<RevealCam>>,
) {
    let Some(playback) = playback.as_ref() else {
        clear_roots(&mut commands, &roots);
        hits.0.clear();
        commands.remove_resource::<PaintKey>();
        return;
    };
    if matches!(playback.show, RevealShow::Single(_)) {
        return;
    }
    let content = Content::embedded();
    let (width, height) = windows
        .single()
        .ok()
        .map(|window| (window.width().max(8.0) as u32, window.height().max(8.0) as u32))
        .unwrap_or((content.ui.layout.fit_width.round() as u32, content.ui.layout.fit_height.round() as u32));
    let owed = !prefs.pull_key.is_empty() && prefs.pull_seen_keys.iter().all(|row| row != &prefs.pull_key);
    let afford = owed || ten_affordable(&content, crate_box.credits);
    let shortfall = if afford { String::new() } else { ten_shortfall(&content, crate_box.credits) };
    let again = afford && !crate_box.pull_busy;
    let failure = crate_box.reason.clone();
    let (kind, items, ready) = match &playback.show {
        RevealShow::Single(_) => return,
        RevealShow::Grid(run) => {
            let shown: Vec<String> = (0..run.plan.cards.len()).map(|index| run.shown_item(index).unwrap_or("").to_string()).collect();
            (0u8, shown, crate_box.buttons_ready)
        }
        RevealShow::Refund(_) => (1u8, Vec::new(), true),
    };
    let key = PaintKey {
        kind,
        width,
        height,
        focus: focus.0,
        ready,
        again,
        shortfall: shortfall.clone(),
        failure: failure.clone(),
        items: items.clone(),
    };
    if cached.as_deref() == Some(&key) {
        return;
    }
    clear_roots(&mut commands, &roots);
    let painted = match &playback.show {
        RevealShow::Single(_) => return,
        RevealShow::Grid(_) => {
            let faces: Vec<Option<&str>> = items.iter().map(|item| if item.is_empty() { None } else { Some(item.as_str()) }).collect();
            paint_grid(&content, &GridStill { items: &faces, ready, again, focus: focus.0, shortfall: &shortfall, failure: &failure }, width, height)
        }
        RevealShow::Refund(_) => paint_refund(&content, &RefundStill { again, focus: focus.0, shortfall: &shortfall, failure: &failure }, width, height),
    };
    let Ok((rgb, painted_hits)) = painted else {
        hits.0.clear();
        return;
    };
    hits.0 = painted_hits;
    let target = cams.iter().next().unwrap_or_else(|| {
        commands
            .spawn((
                RevealCam,
                Camera2d,
                Camera {
                    order: 5,
                    clear_color: ClearColorConfig::Default,
                    ..default()
                },
                IsDefaultUiCamera,
            ))
            .id()
    });
    if let Some(images) = images.as_mut() {
        let handle = images.add(rgb_image(&rgb, width, height));
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(width as f32),
                height: Val::Px(height as f32),
                ..default()
            },
            ImageNode::new(handle),
            GlobalZIndex(1),
            Pickable::IGNORE,
            Visibility::Visible,
            RevealRoot,
            UiTargetCamera(target),
        ));
    }
    for hit in &hits.0 {
            commands
            .spawn((
                Button,
                Hovered::default(),
                HitButton(hit.id.clone()),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(hit.x as f32),
                    top: Val::Px(hit.y as f32),
                    width: Val::Px(hit.w as f32),
                    height: Val::Px(hit.h as f32),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                GlobalZIndex(2),
                Visibility::Visible,
                RevealRoot,
                UiTargetCamera(target),
            ))
            .observe(flow::on_press);
    }
    if let Some(cached) = cached.as_mut() {
        **cached = key;
    } else {
        commands.insert_resource(key);
    }
}

fn ten_affordable(content: &Content, credits: Option<f32>) -> bool {
    let cost = content.banners.cost.ten_pull_cost as f32;
    !credits.is_some_and(|have| have + 1e-3 < cost)
}

fn ten_shortfall(content: &Content, credits: Option<f32>) -> String {
    let cost = content.banners.cost.ten_pull_cost as f32;
    let have = credits.unwrap_or(0.0);
    shortfall_line(&content.ui.copy, content.ui.layout.currency_decimals, (cost - have).max(0.0))
}

fn clear_roots(commands: &mut Commands, roots: &Query<Entity, With<RevealRoot>>) {
    for entity in roots {
        commands.entity(entity).despawn();
    }
}

fn rgb_image(rgb: &[u8], width: u32, height: u32) -> Image {
    let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
    for px in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    Image::new(
        Extent3d { width, height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn paint_single(commands: &mut Commands, content: &Content, run: &RevealRun) {
    let view = look(content, &run.plan, run.time);
    let layout = &content.crate_reveal.layout;
    let rgb = Color::srgb(view.rgb[0], view.rgb[1], view.rgb[2]);
    let luma = view.rgb[0] * 0.3 + view.rgb[1] * 0.59 + view.rgb[2] * 0.11;
    let ink_name = if luma > layout.ink_luma { &layout.ink_dark } else { &layout.ink_light };
    let ink_rgb = content.palette.key(ink_name);
    let ink = Color::srgb(ink_rgb[0], ink_rgb[1], ink_rgb[2]);
    let paper = content.palette.key(&layout.ink_light);
    let stage = content.palette.key(&layout.stage);
    let title = view.shown.to_uppercase();
    let hint = if view.card { "ENTER" } else { "SKIP" };
    let hint_style = &content.crate_reveal.hint;
    let title_px = layout.title_px;
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(layout.row_gap),
            ..default()
        },
        BackgroundColor(Color::srgb(stage[0], stage[1], stage[2])),
        RevealRoot,
        children![
            (
                Node {
                    width: Val::Px(layout.panel_width),
                    height: Val::Px(layout.panel_height),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(content.crate_reveal.card.title_pad)),
                    ..default()
                },
                BackgroundColor(rgb),
                children![(Text::new(title), TextFont::from_font_size(title_px), TextColor(ink))],
            ),
            (
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(hint_style.margin_x),
                    bottom: Val::Px(hint_style.margin_y),
                    ..default()
                },
                children![(
                    Text::new(hint),
                    TextFont::from_font_size(hint_style.text_px),
                    TextColor(Color::srgba(paper[0], paper[1], paper[2], hint_style.opacity)),
                )],
            ),
        ],
    ));
    if view.dim > 0.0 {
        commands.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, view.dim)),
            RevealRoot,
        ));
    }
}

pub fn close(mut commands: Commands, roots: Query<Entity, Or<(With<RevealRoot>, With<RevealVoice>, With<RevealCam>)>>) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<RevealPlayback>();
    commands.remove_resource::<RevealFrom>();
    commands.remove_resource::<PaintKey>();
}

pub fn from_env(spec: &str) -> Option<RevealRun> {
    let content = Content::embedded();
    let (rarity, fakeout) = spec.split_once('+').map(|(rarity, tease)| (rarity, Some(tease))).unwrap_or((spec, None));
    RevealRun::start(&content, rarity.trim(), fakeout).ok()
}

fn play_cue(commands: &mut Commands, assets: &AssetServer, content: &Content, id: &str, bus: f32, gen: u64) {
    let Some(path) = content.cue_path(id) else { return };
    let Some(volume) = content.cue_volume(id) else { return };
    let gain = volume * bus;
    commands.spawn((
        AudioPlayer::new(assets.load::<AudioSource>(path.to_string())),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain)),
        RevealVoice { id: id.to_string(), gen, gain },
    ));
}
