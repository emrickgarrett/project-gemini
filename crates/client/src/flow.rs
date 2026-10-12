//! Mouse, keyboard, and gamepad menus. Screens are painted with the crate-reveal
//! pixel font. Buttons call the same start and pause paths the match already uses.

use bevy::asset::RenderAssetUsages;
use bevy::camera::ClearColorConfig;
use bevy::image::Image;
use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::picking::hover::Hovered;
use bevy::ui_widgets::{Activate, Button};
use bevy::window::{CursorGrabMode, CursorOptions};

use gemini_meshes::{paint_menu, Hit, MenuShot, PaintPrefs};
use gemini_sim::content::{currency_reward, Content};
use gemini_sim::reveal::RevealRun;
use gemini_sim::sim::Phase;

use crate::audio;
use crate::reveal_ui::{self, RevealPlayback};
use crate::session::{Game, PauseMenu, PendingDifficulty};
use crate::settings::{self, Loaded, Prefs};
use crate::AppState;

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Page {
    #[default]
    Title,
    Heroes,
    Difficulty,
    Settings,
    Crates,
    Results,
}

#[derive(Resource, Debug)]
pub struct DebugHotkeys(pub bool);

impl Default for DebugHotkeys {
    fn default() -> Self {
        Self(std::env::var("GEMINI_DEBUG_HOTKEYS").ok().as_deref() == Some("1"))
    }
}

#[derive(Resource, Clone, Debug)]
pub struct MatchSummary {
    pub victory: bool,
    pub waves: u32,
    pub kills: u32,
    pub seconds: f32,
    pub currency: f32,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ServerCrate {
    pub rarity: Option<String>,
    pub fakeout: Option<String>,
    pub failed: bool,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct PointerReady(pub bool);

#[derive(Resource, Default)]
pub struct Soon(pub bool);

#[derive(Resource, Default)]
pub struct ResetNotice(pub bool);

#[derive(Resource, Default)]
pub(crate) struct Dirty(bool);

#[derive(Resource, Default)]
pub(crate) struct Focus(usize);

#[derive(Resource, Default)]
pub(crate) struct Hits(Vec<Hit>);

#[derive(Resource, Default)]
pub struct PendingAction(pub Option<String>);

#[derive(Component)]
pub(crate) struct MenuRoot;

/// UI camera for the menu. The match cameras are play pieces and despawn with
/// the match, so without this the menu has nothing to clear or draw.
#[derive(Component)]
pub(crate) struct MenuCamera;

#[derive(Component)]
pub(crate) struct HitButton(String);

#[derive(Resource)]
pub struct PendingHero(pub String);

impl Default for PendingHero {
    fn default() -> Self {
        Self(String::new())
    }
}

pub fn boot_resources(content: &Content) -> (Prefs, bool) {
    match settings::load_saved(content) {
        Loaded::Fresh(prefs) | Loaded::Restored(prefs) => (prefs, false),
        Loaded::Rejected(prefs) => (prefs, true),
    }
}

pub fn register(app: &mut App) {
    let content = Content::embedded();
    let (prefs, notice) = boot_resources(&content);
    app.insert_resource(prefs)
        .insert_resource(ResetNotice(notice))
        .init_resource::<Page>()
        .init_resource::<DebugHotkeys>()
        .init_resource::<ServerCrate>()
        .init_resource::<PointerReady>()
        .init_resource::<Soon>()
        .init_resource::<Dirty>()
        .init_resource::<Focus>()
        .init_resource::<Hits>()
        .init_resource::<PendingAction>()
        .init_resource::<PendingHero>()
        .init_resource::<PauseMenu>()
        .add_systems(OnEnter(AppState::Menu), enter_menu)
        .add_systems(OnExit(AppState::Menu), despawn_menu)
        .add_systems(OnEnter(AppState::Playing), (enter_match, drop_summary))
        .add_systems(Update, (navigate, hover, apply, rebuild, debug_hotkeys, crate::audio::flush_ui_cues).chain().run_if(in_state(AppState::Menu)));
}

fn enter_menu(
    mut commands: Commands,
    existing: Query<Entity, With<MenuCamera>>,
    mut dirty: ResMut<Dirty>,
    summary: Option<Res<MatchSummary>>,
    mut page: ResMut<Page>,
    mut focus: ResMut<Focus>,
) {
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    // Clears the framebuffer. A camera with no clear leaves the last match frame frozen.
    commands.spawn((
        MenuCamera,
        Camera2d,
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Default,
            ..default()
        },
        IsDefaultUiCamera,
    ));
    if summary.is_some() {
        *page = Page::Results;
        focus.0 = 0;
    }
    dirty.0 = true;
}

fn enter_match(mut ready: ResMut<PointerReady>, mut pause: ResMut<PauseMenu>, mut dirty: ResMut<Dirty>, mut cursor: Query<&mut CursorOptions>) {
    pause.open = false;
    pause.settings = false;
    dirty.0 = true;
    if cfg!(target_arch = "wasm32") {
        ready.0 = false;
    } else {
        ready.0 = true;
        lock_cursor(&mut cursor, true);
    }
}

fn drop_summary(mut commands: Commands) {
    commands.remove_resource::<MatchSummary>();
}

fn despawn_menu(mut commands: Commands, roots: Query<Entity, With<MenuRoot>>, cameras: Query<Entity, With<MenuCamera>>) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    for entity in &cameras {
        commands.entity(entity).despawn();
    }
}

fn debug_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    flag: Res<DebugHotkeys>,
    mut pending: ResMut<PendingDifficulty>,
    mut hero: ResMut<PendingHero>,
    mut next: ResMut<NextState<AppState>>,
    mut prefs: ResMut<Prefs>,
) {
    if !flag.0 {
        return;
    }
    let content = Content::embedded();
    let index = if keys.just_pressed(KeyCode::Digit1) {
        Some(0)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(1)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(2)
    } else if keys.just_pressed(KeyCode::Digit4) {
        Some(3)
    } else {
        None
    };
    if let Some(index) = index {
        let Some(id) = content.difficulty.get(index).map(|tier| tier.id.clone()) else {
            return;
        };
        hero.0 = content.hero.id.clone();
        prefs.character = Some(content.hero.id.clone());
        prefs.difficulty = Some(id.to_string());
        settings::store(&content, &prefs);
        pending.0 = id.to_string();
        next.set(AppState::Playing);
    }
}

pub(crate) fn pause_keys(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut pause: ResMut<PauseMenu>,
    mut dirty: ResMut<Dirty>,
    mut focus: ResMut<Focus>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    prefs: Res<Prefs>,
    mut cursor: Query<&mut CursorOptions>,
) {
    let back = keys.just_pressed(KeyCode::Escape) || pads.iter().any(|pad| pad.just_pressed(GamepadButton::East) || pad.just_pressed(GamepadButton::Start));
    if !back {
        return;
    }
    let content = Content::embedded();
    if pause.settings {
        pause.settings = false;
        dirty.0 = true;
        audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.ui.sounds.back);
        return;
    }
    if pause.open {
        pause.open = false;
        dirty.0 = true;
        audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.ui.sounds.pause_close);
        lock_cursor(&mut cursor, true);
        return;
    }
    pause.open = true;
    pause.settings = false;
    focus.0 = 0;
    dirty.0 = true;
    audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.ui.sounds.pause_open);
    lock_cursor(&mut cursor, false);
}

pub(crate) fn click_prompt(
    mut ready: ResMut<PointerReady>,
    pause: Res<PauseMenu>,
    mut dirty: ResMut<Dirty>,
    mut cursor: Query<&mut CursorOptions>,
    mut action: ResMut<PendingAction>,
) {
    if !cfg!(target_arch = "wasm32") {
        ready.0 = true;
        return;
    }
    if pause.open {
        return;
    }
    if action.0.as_deref() == Some("click") {
        action.0 = None;
        ready.0 = true;
        lock_cursor(&mut cursor, true);
        dirty.0 = true;
    }
}

pub(crate) fn end_of_match(
    game: Option<Res<Game>>,
    summary: Option<Res<MatchSummary>>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    assets: Res<AssetServer>,
    prefs: Res<Prefs>,
) {
    let Some(game) = game else { return };
    if summary.is_some() {
        return;
    }
    let victory = match game.sim.phase {
        Phase::Victory => true,
        Phase::Defeat => false,
        _ => return,
    };
    let waves = if victory { game.sim.director.wave } else { game.sim.director.wave.saturating_sub(1) };
    let tier = game.content.tier(&game.difficulty);
    let currency = currency_reward(&game.content.currency, waves, tier.currency, victory, game.sim.boss_kills);
    commands.insert_resource(MatchSummary {
        victory,
        waves,
        kills: game.sim.kills,
        seconds: game.sim.time,
        currency,
    });
    let content = Content::embedded();
    let id = if victory { &content.ui.sounds.victory } else { &content.ui.sounds.defeat };
    audio::ui_cue(&mut commands, &assets, &content, &prefs, id);
    next.set(AppState::Menu);
}

pub(crate) fn navigate(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    hits: Res<Hits>,
    mut focus: ResMut<Focus>,
    mut dirty: ResMut<Dirty>,
    mut action: ResMut<PendingAction>,
    mut prefs: ResMut<Prefs>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut repeat: Local<f32>,
) {
    if hits.0.is_empty() || action.0.is_some() {
        return;
    }
    let content = Content::embedded();
    let mut dir = 0i32;
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::Tab) {
        dir = 1;
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        dir = -1;
    }
    let pad = pads.iter().next();
    if let Some(pad) = pad {
        if pad.just_pressed(GamepadButton::DPadDown) {
            dir = 1;
        }
        if pad.just_pressed(GamepadButton::DPadUp) {
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
        focus.0 = move_focus(&hits.0, focus.0, dir);
        dirty.0 = true;
        audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.ui.sounds.hover);
    }
    let raw = hits.0.get(focus.0).map(|hit| hit.id.clone()).unwrap_or_default();
    let id = setting_base(&raw);
    let mut nudge = 0i32;
    if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) {
        nudge = -1;
    } else if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD) {
        nudge = 1;
    }
    if let Some(pad) = pad {
        if pad.just_pressed(GamepadButton::DPadLeft) {
            nudge = -1;
        }
        if pad.just_pressed(GamepadButton::DPadRight) {
            nudge = 1;
        }
    }
    if nudge != 0 && (id.starts_with("slider:") || id.starts_with("toggle:")) {
        edit_pref(&mut prefs, id, nudge, &content);
        settings::store(&content, &prefs);
        dirty.0 = true;
        let cue = if id.starts_with("toggle:") { &content.ui.sounds.toggle } else { &content.ui.sounds.slider };
        audio::ui_cue(&mut commands, &assets, &content, &prefs, cue);
    }
    let activate = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) || pad.is_some_and(|pad| pad.just_pressed(GamepadButton::South));
    if activate && !id.is_empty() {
        action.0 = Some(id.to_string());
    }
}

fn edit_pref(prefs: &mut Prefs, id: &str, dir: i32, content: &Content) {
    let settings = &content.ui.settings;
    let step = dir as f32;
    match id {
        "slider:sensitivity" => prefs.sensitivity = step_to(prefs.sensitivity, step * settings.sensitivity_step, settings.sensitivity_min, settings.sensitivity_max),
        "slider:fov" => prefs.fov = step_to(prefs.fov, step * settings.fov_step, settings.fov_min, settings.fov_max),
        "slider:master" => prefs.master = step_to(prefs.master, step * settings.volume_step, settings.volume_min, settings.volume_max),
        "slider:music" => prefs.music = step_to(prefs.music, step * settings.volume_step, settings.volume_min, settings.volume_max),
        "slider:effects" => prefs.effects = step_to(prefs.effects, step * settings.volume_step, settings.volume_min, settings.volume_max),
        "toggle:bob" => prefs.view_bob = !prefs.view_bob,
        "toggle:invert" => prefs.invert_y = !prefs.invert_y,
        _ => {}
    }
}

fn step_to(value: f32, delta: f32, min: f32, max: f32) -> f32 {
    (value + delta).clamp(min, max)
}

pub(crate) fn hover(
    buttons: Query<(&Hovered, &HitButton), Changed<Hovered>>,
    mut focus: ResMut<Focus>,
    hits: Res<Hits>,
    mut dirty: ResMut<Dirty>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    prefs: Res<Prefs>,
) {
    let content = Content::embedded();
    for (hovered, hit) in &buttons {
        if !hovered.get() {
            continue;
        }
        let Some(index) = hits.0.iter().position(|row| row.id == hit.0) else { continue };
        let index = focus_slot(&hits.0, index);
        if focus.0 == index {
            continue;
        }
        focus.0 = index;
        dirty.0 = true;
        audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.ui.sounds.hover);
    }
}

fn on_press(activate: On<Activate>, buttons: Query<&HitButton>, mut action: ResMut<PendingAction>, mut focus: ResMut<Focus>, hits: Res<Hits>, mut dirty: ResMut<Dirty>) {
    let Ok(hit) = buttons.get(activate.entity) else { return };
    if let Some(index) = hits.0.iter().position(|row| row.id == hit.0) {
        let index = focus_slot(&hits.0, index);
        if focus.0 != index {
            focus.0 = index;
            dirty.0 = true;
        }
    }
    action.0 = Some(hit.0.clone());
}

pub(crate) fn apply(
    mut action: ResMut<PendingAction>,
    mut page: ResMut<Page>,
    mut focus: ResMut<Focus>,
    mut dirty: ResMut<Dirty>,
    mut pause: ResMut<PauseMenu>,
    mut hero: ResMut<PendingHero>,
    mut pending: ResMut<PendingDifficulty>,
    mut prefs: ResMut<Prefs>,
    mut next: ResMut<NextState<AppState>>,
    mut soon: ResMut<Soon>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut cursor: Query<&mut CursorOptions>,
    crate_box: Option<Res<ServerCrate>>,
) {
    let Some(id) = action.0.take() else { return };
    let content = Content::embedded();
    let confirm = &content.ui.sounds.confirm;
    let back_cue = &content.ui.sounds.back;
    let error = &content.ui.sounds.error;
    match id.as_str() {
        "play" => {
            *page = Page::Heroes;
            focus.0 = content.heroes.iter().position(|hero| content.hero_selectable(&hero.id)).unwrap_or(0);
            dirty.0 = true;
            audio::ui_cue(&mut commands, &assets, &content, &prefs, confirm);
        }
        "crates" => {
            *page = Page::Crates;
            focus.0 = 0;
            soon.0 = false;
            dirty.0 = true;
            audio::ui_cue(&mut commands, &assets, &content, &prefs, confirm);
        }
        "settings" => {
            if pause.open {
                pause.settings = true;
            } else {
                *page = Page::Settings;
            }
            focus.0 = 0;
            dirty.0 = true;
            audio::ui_cue(&mut commands, &assets, &content, &prefs, confirm);
        }
        "quit" => {
            audio::ui_cue(&mut commands, &assets, &content, &prefs, back_cue);
            #[cfg(not(target_arch = "wasm32"))]
            {
                commands.write_message(AppExit::Success);
            }
        }
        "back" => {
            let leaving_results = matches!(*page, Page::Results) && !pause.settings;
            go_back(&mut page, &mut pause, &mut focus);
            if leaving_results {
                commands.remove_resource::<MatchSummary>();
            }
            dirty.0 = true;
            audio::ui_cue(&mut commands, &assets, &content, &prefs, back_cue);
        }
        "resume" => {
            pause.open = false;
            pause.settings = false;
            dirty.0 = true;
            lock_cursor(&mut cursor, true);
            audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.ui.sounds.pause_close);
        }
        "quit_to_menu" => {
            pause.open = false;
            pause.settings = false;
            commands.remove_resource::<MatchSummary>();
            *page = Page::Title;
            focus.0 = 0;
            lock_cursor(&mut cursor, false);
            next.set(AppState::Menu);
            audio::ui_cue(&mut commands, &assets, &content, &prefs, back_cue);
        }
        "host" | "join" => {
            soon.0 = true;
            dirty.0 = true;
            audio::ui_cue(&mut commands, &assets, &content, &prefs, error);
        }
        "play_again" => {
            let started = play_again(&content, &mut prefs, &mut hero, &mut pending, &mut page, &mut focus, &mut next, &mut commands, &mut dirty);
            let cue = if started { &content.ui.sounds.match_start } else { error };
            audio::ui_cue(&mut commands, &assets, &content, &prefs, cue);
        }
        "main_menu" => {
            commands.remove_resource::<MatchSummary>();
            *page = Page::Title;
            focus.0 = 0;
            lock_cursor(&mut cursor, false);
            dirty.0 = true;
            audio::ui_cue(&mut commands, &assets, &content, &prefs, back_cue);
        }
        "pull" => {
            // The client does not roll. A tier appears only when a server result is already in ServerCrate.
            commands.insert_resource(ServerCrate { rarity: None, fakeout: None, failed: true });
            dirty.0 = true;
            audio::ui_cue(&mut commands, &assets, &content, &prefs, error);
        }
        "open" => {
            if let Some(crate_box) = crate_box {
                if let Some(rarity) = crate_box.rarity.clone() {
                    if let Ok(run) = RevealRun::start(&content, &rarity, crate_box.fakeout.as_deref()) {
                        commands.insert_resource(RevealPlayback(run));
                        commands.insert_resource(reveal_ui::RevealFrom::Crates);
                        next.set(AppState::Reveal);
                        audio::ui_cue(&mut commands, &assets, &content, &prefs, confirm);
                    }
                }
            }
        }
        "click" => {}
        other if other.starts_with("hero:") => {
            let id = other.trim_start_matches("hero:");
            if !content.hero_selectable(id) {
                audio::ui_cue(&mut commands, &assets, &content, &prefs, error);
            } else {
                hero.0 = id.to_string();
                prefs.character = Some(id.to_string());
                settings::store(&content, &prefs);
                *page = Page::Difficulty;
                focus.0 = content
                    .difficulty
                    .iter()
                    .position(|tier| tier.id == content.ui.settings.default_difficulty)
                    .unwrap_or(0);
                dirty.0 = true;
                audio::ui_cue(&mut commands, &assets, &content, &prefs, confirm);
            }
        }
        other if other.starts_with("tier:") => {
            let id = other.trim_start_matches("tier:");
            if content.difficulty.iter().any(|tier| tier.id == id) && content.hero_selectable(&hero.0) {
                pending.0 = id.to_string();
                prefs.difficulty = Some(id.to_string());
                settings::store(&content, &prefs);
                commands.remove_resource::<MatchSummary>();
                audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.ui.sounds.match_start);
                next.set(AppState::Playing);
            } else {
                audio::ui_cue(&mut commands, &assets, &content, &prefs, error);
            }
        }
        other if other.starts_with("slider:") || other.starts_with("toggle:") => {
            let dir = if other.ends_with(":-") { -1 } else { 1 };
            edit_pref(&mut prefs, setting_base(other), dir, &content);
            settings::store(&content, &prefs);
            dirty.0 = true;
            let cue = if other.starts_with("toggle:") { &content.ui.sounds.toggle } else { &content.ui.sounds.slider };
            audio::ui_cue(&mut commands, &assets, &content, &prefs, cue);
        }
        _ => {}
    }
}

fn play_again(
    content: &Content,
    prefs: &mut Prefs,
    hero: &mut PendingHero,
    pending: &mut PendingDifficulty,
    page: &mut Page,
    focus: &mut Focus,
    next: &mut NextState<AppState>,
    commands: &mut Commands,
    dirty: &mut Dirty,
) -> bool {
    let hero_ok = prefs.character.as_ref().is_some_and(|id| content.hero_selectable(id));
    let diff_ok = prefs.difficulty.as_ref().is_some_and(|id| content.difficulty.iter().any(|tier| &tier.id == id));
    if !hero_ok {
        prefs.character = None;
        settings::store(content, prefs);
        *page = Page::Heroes;
        focus.0 = 0;
        dirty.0 = true;
        return false;
    }
    if !diff_ok {
        prefs.difficulty = None;
        settings::store(content, prefs);
        hero.0 = content.hero.id.clone();
        *page = Page::Difficulty;
        focus.0 = 0;
        dirty.0 = true;
        return false;
    }
    hero.0 = prefs.character.clone().unwrap_or_default();
    pending.0 = prefs.difficulty.clone().unwrap_or_default();
    commands.remove_resource::<MatchSummary>();
    next.set(AppState::Playing);
    true
}

fn go_back(page: &mut Page, pause: &mut PauseMenu, focus: &mut Focus) {
    if pause.settings {
        pause.settings = false;
        focus.0 = 0;
        return;
    }
    *page = match *page {
        Page::Heroes | Page::Settings | Page::Crates | Page::Results => Page::Title,
        Page::Difficulty => Page::Heroes,
        Page::Title => Page::Title,
    };
    focus.0 = 0;
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MenuView<'w, 's> {
    page: Res<'w, Page>,
    pause: Res<'w, PauseMenu>,
    focus: Res<'w, Focus>,
    hero: Res<'w, PendingHero>,
    prefs: Res<'w, Prefs>,
    soon: Res<'w, Soon>,
    notice: Res<'w, ResetNotice>,
    summary: Option<Res<'w, MatchSummary>>,
    crate_box: Option<Res<'w, ServerCrate>>,
    ready: Res<'w, PointerReady>,
    state: Res<'w, State<AppState>>,
    windows: Query<'w, 's, &'static Window>,
    images: Option<ResMut<'w, Assets<Image>>>,
    old: Query<'w, 's, Entity, With<MenuRoot>>,
    menu_cam: Query<'w, 's, Entity, With<MenuCamera>>,
    ui_cam: Query<'w, 's, Entity, With<IsDefaultUiCamera>>,
    hits: ResMut<'w, Hits>,
}

pub(crate) fn rebuild(mut commands: Commands, mut dirty: ResMut<Dirty>, mut view: MenuView) {
    if !dirty.0 && !view.page.is_changed() && !view.pause.is_changed() && !view.notice.is_changed() {
        return;
    }
    dirty.0 = false;
    for entity in &view.old {
        commands.entity(entity).despawn();
    }
    let (width, height) = view
        .windows
        .single()
        .ok()
        .map(|window| (window.width().max(8.0) as u32, window.height().max(8.0) as u32))
        .unwrap_or((1280, 720));
    let content = Content::embedded();
    let shot = shot_for(
        view.state.get(),
        *view.page,
        *view.pause,
        view.focus.0,
        &view.hero.0,
        &view.prefs,
        view.soon.0,
        view.notice.0,
        view.summary.as_deref(),
        view.crate_box.as_deref(),
        view.ready.0,
    );
    let Some(shot) = shot else {
        view.hits.0.clear();
        return;
    };
    let (rgb, painted) = paint_menu(&content, &shot, width, height);
    view.hits.0 = painted;
    let target = view.menu_cam.iter().next().or_else(|| view.ui_cam.iter().next());
    if let Some(images) = view.images.as_mut() {
        let handle = images.add(image_from_rgb(&rgb, width, height));
        let mut spawned = commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(width as f32),
                height: Val::Px(height as f32),
                ..default()
            },
            ImageNode::new(handle),
            Visibility::Visible,
            MenuRoot,
        ));
        if let Some(cam) = target {
            spawned.insert(UiTargetCamera(cam));
        }
    }
    for hit in &view.hits.0 {
        let mut spawned = commands.spawn((
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
            Visibility::Visible,
            MenuRoot,
        ));
        if let Some(cam) = target {
            spawned.insert(UiTargetCamera(cam));
        }
        spawned.observe(on_press);
    }
}

fn setting_base(id: &str) -> &str {
    id.strip_suffix(":-").or_else(|| id.strip_suffix(":+")).unwrap_or(id)
}

fn stepper_hit(id: &str) -> bool {
    id.ends_with(":-") || id.ends_with(":+")
}

fn move_focus(hits: &[Hit], focus: usize, dir: i32) -> usize {
    let n = hits.len() as i32;
    if n == 0 {
        return 0;
    }
    let mut i = focus as i32;
    for _ in 0..n {
        i = (i + dir).rem_euclid(n);
        if !stepper_hit(&hits[i as usize].id) {
            return i as usize;
        }
    }
    focus
}

fn focus_slot(hits: &[Hit], index: usize) -> usize {
    let base = setting_base(&hits[index].id);
    if base == hits[index].id {
        return index;
    }
    hits.iter().position(|hit| hit.id == base).unwrap_or(index)
}

fn shot_for(
    state: &AppState,
    page: Page,
    pause: PauseMenu,
    focus: usize,
    hero: &str,
    prefs: &Prefs,
    soon: bool,
    notice: bool,
    summary: Option<&MatchSummary>,
    crate_box: Option<&ServerCrate>,
    ready: bool,
) -> Option<MenuShot> {
    if *state == AppState::Playing {
        if pause.settings {
            return Some(MenuShot::Settings {
                focus,
                hero_id: hero.to_string(),
                prefs: paint_prefs(prefs),
                notice,
            });
        }
        if pause.open {
            return Some(MenuShot::Pause { focus, hero_id: hero.to_string() });
        }
        if cfg!(target_arch = "wasm32") && !ready {
            return Some(MenuShot::ClickToPlay);
        }
        return None;
    }
    Some(match page {
        Page::Title => MenuShot::Title { focus, notice, native_quit: cfg!(not(target_arch = "wasm32")) },
        Page::Heroes => MenuShot::Heroes { focus },
        Page::Difficulty => MenuShot::Difficulty { focus, hero_id: hero.to_string(), notice: soon },
        Page::Settings => MenuShot::Settings { focus, hero_id: hero.to_string(), prefs: paint_prefs(prefs), notice },
        Page::Results => {
            let Some(summary) = summary.cloned() else {
                return None;
            };
            MenuShot::Results {
                focus,
                victory: summary.victory,
                waves: summary.waves,
                kills: summary.kills,
                seconds: summary.seconds,
                currency: summary.currency,
            }
        }
        Page::Crates => MenuShot::Crates {
            focus,
            rarity: crate_box.and_then(|row| row.rarity.clone()),
            failed: crate_box.is_some_and(|row| row.failed),
            can_pull: true,
        },
    })
}

fn paint_prefs(prefs: &Prefs) -> PaintPrefs {
    PaintPrefs {
        sensitivity: prefs.sensitivity,
        fov: prefs.fov,
        invert_y: prefs.invert_y,
        view_bob: prefs.view_bob,
        master: prefs.master,
        music: prefs.music,
        effects: prefs.effects,
    }
}

fn image_from_rgb(rgb: &[u8], width: u32, height: u32) -> Image {
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

fn lock_cursor(cursor: &mut Query<&mut CursorOptions>, lock: bool) {
    if let Ok(mut cursor) = cursor.single_mut() {
        if lock {
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        } else {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
    }
}

/// Solo pause freezes the sim. Co-op keeps the shared match running.
pub fn freeze_solo(open: bool, connected: usize) -> bool {
    open && connected <= 1
}

