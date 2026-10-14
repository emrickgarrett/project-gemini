//! Mouse, keyboard, and gamepad menus. Screens are painted with the crate-reveal
//! pixel font. Buttons call the same start and pause paths the match already uses.

use bevy::asset::RenderAssetUsages;
use bevy::camera::ClearColorConfig;
use bevy::image::Image;
use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::picking::hover::Hovered;
use bevy::picking::Pickable;
use bevy::ui::GlobalZIndex;
use bevy::ui_widgets::{Activate, Button};
use bevy::window::{CursorGrabMode, CursorOptions};

use gemini_meshes::{paint_menu_masked, Hit, MenuPaint, MenuShot, PaintPrefs};
use gemini_proto::{BankFault, CratePullFault, CratePullReply};
use gemini_sim::bank::{cues_between, shown_earned, BankCue, BankShow};
use gemini_sim::content::{format_fixed, shortfall_line, Content};
use gemini_sim::reveal::{GridCard, GridRun, RefundRun, RevealRun};
use gemini_sim::sim::Phase;

use crate::audio;
use crate::reveal_ui::{self, RevealPlayback, RevealShow};
use crate::session::{Account, Game, PauseMenu, PendingDifficulty};
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
    pub show: BankShow,
    pub waves: u32,
    pub kills: u32,
    pub seconds: f32,
    pub earned: f32,
    pub bonus: f32,
    /// Wallet after the deposit. `None` when the host did not answer.
    pub balance: Option<f32>,
}

/// Count-up clock. The tick interval lives in `ui.toml`.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct BankClock {
    elapsed: f32,
    shown: f32,
}

#[derive(Clone, Debug, Default)]
pub struct ServerPod {
    pub rarity: String,
    pub fakeout: Option<String>,
    pub item: String,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ServerCrate {
    pub rarity: Option<String>,
    pub fakeout: Option<String>,
    pub failed: bool,
    pub reason: String,
    pub pods: Vec<ServerPod>,
    pub ten: bool,
    /// False when this key was already stored. An unseen replay still plays.
    pub fresh: bool,
    /// Last balance the server reported. None until a pull or a bank answers.
    pub credits: Option<f32>,
    /// True after a pull press until the next frame, so a second press in this frame does not send.
    pub pull_busy: bool,
    /// Bumps when a new reveal replaces the one on screen. Older voices fade out.
    pub reveal_gen: u64,
    /// Keys this client actually sent. Tests read it. A busy or disabled press does not push.
    pub sent_keys: Vec<String>,
    /// True once the best card's face is up, or immediately on the refund panel.
    pub buttons_ready: bool,
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
pub(crate) struct Focus(pub(crate) usize);

#[derive(Resource, Default)]
pub(crate) struct Hits(pub(crate) Vec<Hit>);

#[derive(Resource, Default)]
pub struct PendingAction(pub Option<String>);

#[derive(Component)]
pub(crate) struct MenuRoot;

/// UI camera for the menu. The match cameras are play pieces and despawn with
/// the match, so without this the menu has nothing to clear or draw.
#[derive(Component)]
pub(crate) struct MenuCamera;

#[derive(Component)]
pub(crate) struct HitButton(pub(crate) String);

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
        .init_resource::<BankClock>()
        .add_systems(Update, (release_pull, navigate, hover, apply, tick_bank, rebuild, debug_hotkeys, crate::audio::flush_ui_cues).chain().run_if(in_state(AppState::Menu)));
}

pub(crate) fn release_pull(mut crate_box: ResMut<ServerCrate>) {
    crate_box.pull_busy = false;
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
    pause.resume_click = false;
    dirty.0 = true;
    if cfg!(target_arch = "wasm32") {
        ready.0 = false;
    } else {
        ready.0 = true;
        lock_cursor(&mut cursor, &mut pause, true);
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
        save_settings(&prefs);
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
        lock_cursor(&mut cursor, &mut pause, true);
        return;
    }
    pause.open = true;
    pause.settings = false;
    focus.0 = 0;
    dirty.0 = true;
    audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.ui.sounds.pause_open);
    lock_cursor(&mut cursor, &mut pause, false);
}

pub(crate) fn click_prompt(
    mut ready: ResMut<PointerReady>,
    mut pause: ResMut<PauseMenu>,
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
        lock_cursor(&mut cursor, &mut pause, true);
        // Gather already sampled this frame, so the flag would skip the next shot.
        pause.swallow_fire = false;
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
    mut crate_box: ResMut<ServerCrate>,
) {
    let Some(game) = game else { return };
    if summary.is_some() {
        return;
    }
    let show = match game.sim.phase {
        Phase::Victory => BankShow::Win,
        Phase::Defeat => BankShow::End,
        Phase::Combat | Phase::Upgrade { .. } | Phase::Breather { .. } => return,
    };
    let waves = match show {
        BankShow::Win => game.sim.director.wave,
        BankShow::End | BankShow::Quit | BankShow::Disconnect => game.sim.director.wave.saturating_sub(1),
    };
    present_bank(&mut commands, &game, show, waves, &assets, &prefs, &mut crate_box);
    next.set(AppState::Menu);
}

fn present_bank(commands: &mut Commands, game: &Game, show: BankShow, waves: u32, assets: &AssetServer, prefs: &Prefs, crate_box: &mut ServerCrate) {
    let (earned, bonus, balance) = settle_bank(game);
    if let Some(have) = balance {
        crate_box.credits = Some(have);
    }
    commands.insert_resource(MatchSummary {
        show,
        waves,
        kills: game.sim.kills,
        seconds: game.sim.time,
        earned,
        bonus,
        balance,
    });
    commands.insert_resource(BankClock::default());
    let content = Content::embedded();
    let sting = match show {
        BankShow::Win => Some(content.ui.sounds.victory.as_str()),
        BankShow::End => Some(content.ui.sounds.defeat.as_str()),
        BankShow::Quit | BankShow::Disconnect => None,
    };
    if let Some(id) = sting {
        audio::ui_cue(commands, assets, &content, prefs, id);
    }
}

/// Note the host's sim, then deposit by match id and player id. A missing host leaves the balance unset.
fn settle_bank(game: &Game) -> (f32, f32, Option<f32>) {
    let earned = game.sim.currency_gain.get(&game.token).copied().unwrap_or(0.0);
    let bonus = game.sim.currency_bonus.get(&game.token).copied().unwrap_or(0.0);
    let Some(id) = game.match_id else {
        return (earned, bonus, None);
    };
    if crate::bank_host::note(id, &game.sim).is_err() {
        return (earned, bonus, None);
    }
    match crate::bank_host::bank(id, game.token) {
        Ok(reply) => match reply.fault {
            None => (reply.earned, reply.bonus, Some(reply.balance)),
            Some(BankFault::Player | BankFault::Amount | BankFault::Unavailable | BankFault::Unknown) => (earned, bonus, None),
        },
        Err(_) => (earned, bonus, None),
    }
}

pub(crate) fn tick_bank(
    time: Res<Time>,
    page: Res<Page>,
    summary: Option<Res<MatchSummary>>,
    mut clock: ResMut<BankClock>,
    mut dirty: ResMut<Dirty>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    prefs: Res<Prefs>,
) {
    if !matches!(*page, Page::Results) {
        return;
    }
    let Some(summary) = summary else { return };
    let content = Content::embedded();
    let tick = content.ui.bank.count_tick_secs;
    let count = content.ui.bank.count_secs;
    let prev = clock.elapsed;
    clock.elapsed += time.delta_secs();
    let cues = match cues_between(summary.show, prev, clock.elapsed, tick, count, summary.earned) {
        Ok(cues) => cues,
        Err(err) => panic!("bank count interval: {err}"),
    };
    for cue in cues {
        let id = match cue {
            BankCue::Tick => content.ui.sounds.count_tick.as_str(),
            BankCue::Chime => content.ui.sounds.bank_chime.as_str(),
        };
        audio::ui_cue(&mut commands, &assets, &content, &prefs, id);
    }
    let shown = shown_earned(summary.earned, clock.elapsed, count);
    let places = content.ui.layout.currency_decimals;
    if format_fixed(shown, places) != format_fixed(clock.shown, places) {
        dirty.0 = true;
    }
    clock.shown = shown;
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
        save_settings(&prefs);
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

pub(crate) fn on_press(activate: On<Activate>, buttons: Query<&HitButton>, mut action: ResMut<PendingAction>, mut focus: ResMut<Focus>, hits: Res<Hits>, mut dirty: ResMut<Dirty>) {
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
    mut crate_box: ResMut<ServerCrate>,
    mut rank: ResMut<Account>,
    game: Option<Res<Game>>,
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
            if !prefs.pull_key.is_empty() && !has_seen(&prefs, &prefs.pull_key) {
                let key = prefs.pull_key.clone();
                let ten = prefs.pull_ten;
                let kept_gen = crate_box.reveal_gen;
                dirty.0 = true;
                match crate::pull_host::round_trip(&content.banners.id, &key, ten) {
                    Ok(reply) => {
                        let (mut landed, pulled_rank) = crate_from_reply(&content, &reply);
                        if let Some(pulled_rank) = pulled_rank {
                            rank.rank = pulled_rank;
                        }
                        landed.credits = Some(reply.credits);
                        let mut keys = crate_box.sent_keys.clone();
                        push_sent(&mut keys, key.clone());
                        if is_refund(&reply.fault) {
                            clear_pull_memory(&mut prefs);
                            keep_session(&mut landed, keys, false, kept_gen.saturating_add(1), true);
                            landed.reason.clear();
                            *crate_box = landed;
                            commands.insert_resource(RevealPlayback {
                                show: RevealShow::Refund(RefundRun::start(&content, ten)),
                                key: String::new(),
                            });
                            commands.insert_resource(reveal_ui::RevealFrom::Crates);
                            next.set_if_different(AppState::Reveal);
                            return;
                        }
                        if !landed.failed && !is_short(&reply.fault) {
                            match show_result(&content, &landed, &key, &mut commands, &mut next) {
                                Ok(()) => {
                                    prefs.pull_awaiting = false;
                                    save_settings(&prefs);
                                    keep_session(&mut landed, keys, false, kept_gen.saturating_add(1), false);
                                    landed.reason.clear();
                                    *crate_box = landed;
                                    return;
                                }
                                Err(message) => {
                                    eprintln!("saved pull reveal failed: {message}");
                                    keep_session(&mut landed, keys, false, kept_gen, true);
                                    landed.reason.clear();
                                    *crate_box = landed;
                                }
                            }
                        } else {
                            if landed.failed {
                                audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.crate_reveal.grid.pull_failed);
                            }
                            clear_pull_memory(&mut prefs);
                            keep_session(&mut landed, keys, false, kept_gen, true);
                            *crate_box = landed;
                        }
                    }
                    Err(err) => {
                        eprintln!("crate pull failed: {err}");
                        mark_lost(&mut prefs);
                        crate_box.failed = true;
                        crate_box.reason = transport_lost(&err, &content.ui.copy);
                        push_sent(&mut crate_box.sent_keys, key);
                        audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.crate_reveal.grid.pull_failed);
                    }
                }
                *page = Page::Crates;
                focus.0 = 0;
                soon.0 = false;
                return;
            }
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
            lock_cursor(&mut cursor, &mut pause, true);
            audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.ui.sounds.pause_close);
        }
        "quit_to_menu" => {
            pause.open = false;
            pause.settings = false;
            focus.0 = 0;
            lock_cursor(&mut cursor, &mut pause, false);
            if let Some(game) = game.as_deref() {
                let waves = game.sim.director.wave.saturating_sub(1);
                present_bank(&mut commands, game, BankShow::Quit, waves, &assets, &prefs, &mut crate_box);
                *page = Page::Results;
            } else {
                commands.remove_resource::<MatchSummary>();
                *page = Page::Title;
            }
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
            lock_cursor(&mut cursor, &mut pause, false);
            dirty.0 = true;
            audio::ui_cue(&mut commands, &assets, &content, &prefs, back_cue);
        }
        "pull" | "ten" => {
            let asked = id == "ten";
            let unseen = !prefs.pull_key.is_empty() && !has_seen(&prefs, &prefs.pull_key);
            let ten = if unseen { prefs.pull_ten } else { asked };
            let cost = if ten { content.banners.cost.ten_pull_cost } else { content.banners.cost.single } as f32;
            if crate_box.credits.is_some_and(|have| have + 1e-3 < cost) {
                dirty.0 = true;
                return;
            }
            let (key, ten) = claim_key(&mut prefs, ten);
            let credits = crate_box.credits;
            let mut kept_keys = crate_box.sent_keys.clone();
            push_sent(&mut kept_keys, key.clone());
            let kept_busy = crate_box.pull_busy;
            let kept_gen = crate_box.reveal_gen;
            let kept_ready = crate_box.buttons_ready;
            dirty.0 = true;
            match crate::pull_host::round_trip(&content.banners.id, &key, ten) {
                Ok(reply) => {
                    let (mut landed, pulled_rank) = crate_from_reply(&content, &reply);
                    if let Some(pulled_rank) = pulled_rank {
                        rank.rank = pulled_rank;
                    }
                    let failed = landed.failed;
                    let refund = is_refund(&reply.fault);
                    let short = is_short(&reply.fault);
                    landed.credits = Some(reply.credits);
                    keep_session(&mut landed, kept_keys, kept_busy, kept_gen, kept_ready);
                    if refund {
                        clear_pull_memory(&mut prefs);
                        landed.reason.clear();
                        landed.reveal_gen = kept_gen.saturating_add(1);
                        *crate_box = landed;
                        commands.insert_resource(RevealPlayback {
                            show: RevealShow::Refund(RefundRun::start(&content, ten)),
                            key: String::new(),
                        });
                        commands.insert_resource(reveal_ui::RevealFrom::Crates);
                        next.set_if_different(AppState::Reveal);
                    } else if short {
                        clear_pull_memory(&mut prefs);
                        *crate_box = landed;
                    } else if failed {
                        clear_pull_memory(&mut prefs);
                        *crate_box = landed;
                        audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.crate_reveal.grid.pull_failed);
                    } else if !reply.fresh && has_seen(&prefs, &key) {
                        settle_success(&mut prefs, &key);
                        landed.reveal_gen = kept_gen;
                        landed.reason.clear();
                        *crate_box = landed;
                    } else if unseen || !reply.fresh {
                        match show_result(&content, &landed, &key, &mut commands, &mut next) {
                            Ok(()) => {
                                prefs.pull_awaiting = false;
                                save_settings(&prefs);
                                landed.reason.clear();
                                landed.reveal_gen = kept_gen.saturating_add(1);
                                landed.buttons_ready = false;
                                *crate_box = landed;
                            }
                            Err(message) => {
                                eprintln!("ten pull reveal failed: {message}");
                                landed.reason.clear();
                                landed.reveal_gen = kept_gen;
                                *crate_box = landed;
                            }
                        }
                    } else {
                        prefs.pull_awaiting = false;
                        save_settings(&prefs);
                        *crate_box = landed;
                        audio::ui_cue(&mut commands, &assets, &content, &prefs, confirm);
                    }
                }
                Err(err) => {
                    eprintln!("crate pull failed: {err}");
                    mark_lost(&mut prefs);
                    *crate_box = ServerCrate {
                        rarity: None,
                        fakeout: None,
                        failed: true,
                        reason: transport_lost(&err, &content.ui.copy),
                        pods: Vec::new(),
                        ten: false,
                        fresh: false,
                        credits,
                        pull_busy: kept_busy,
                        reveal_gen: kept_gen,
                        sent_keys: kept_keys,
                        buttons_ready: kept_ready,
                    };
                    audio::ui_cue(&mut commands, &assets, &content, &prefs, &content.crate_reveal.grid.pull_failed);
                }
            }
        }
        "open" => {
            if !crate_box.fresh && (prefs.pull_key.is_empty() || has_seen(&prefs, &prefs.pull_key)) {
                return;
            }
            if crate_box.ten && !crate_box.failed {
                let cards: Vec<GridCard> = crate_box
                    .pods
                    .iter()
                    .map(|pod| GridCard {
                        rarity: pod.rarity.clone(),
                        fakeout: pod.fakeout.clone(),
                        item: pod.item.clone(),
                    })
                    .collect();
                match GridRun::start(&content, &cards) {
                    Ok(run) => {
                        commands.insert_resource(RevealPlayback { show: RevealShow::Grid(run), key: prefs.pull_key.clone() });
                        commands.insert_resource(reveal_ui::RevealFrom::Crates);
                        next.set_if_different(AppState::Reveal);
                        audio::ui_cue(&mut commands, &assets, &content, &prefs, confirm);
                    }
                    Err(message) => {
                        eprintln!("ten pull reveal failed: {message}");
                        dirty.0 = true;
                    }
                }
            } else if let Some(rarity) = crate_box.rarity.clone() {
                match RevealRun::start(&content, &rarity, crate_box.fakeout.as_deref()) {
                    Ok(run) => {
                        commands.insert_resource(RevealPlayback { show: RevealShow::Single(run), key: prefs.pull_key.clone() });
                        commands.insert_resource(reveal_ui::RevealFrom::Crates);
                        next.set_if_different(AppState::Reveal);
                        audio::ui_cue(&mut commands, &assets, &content, &prefs, confirm);
                    }
                    Err(message) => {
                        eprintln!("crate reveal failed: {message}");
                        dirty.0 = true;
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
                save_settings(&prefs);
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
                save_settings(&prefs);
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
            save_settings(&prefs);
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
        save_settings(prefs);
        *page = Page::Heroes;
        focus.0 = 0;
        dirty.0 = true;
        return false;
    }
    if !diff_ok {
        prefs.difficulty = None;
        save_settings(prefs);
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

/// CONTINUE and PULL AGAIN on the live grid and the refund panel.
/// A second PULL AGAIN in this frame sees `pull_busy` and does not send.
pub(crate) fn reveal_act(
    mut action: ResMut<PendingAction>,
    mut page: ResMut<Page>,
    mut next: ResMut<NextState<AppState>>,
    mut commands: Commands,
    mut crate_box: ResMut<ServerCrate>,
    mut rank: ResMut<Account>,
    mut playback: Option<ResMut<RevealPlayback>>,
    assets: Res<AssetServer>,
    mut prefs: ResMut<Prefs>,
) {
    let Some(id) = action.0.clone() else { return };
    if id != "continue" && id != "pull_again" {
        return;
    }
    action.0 = None;
    let content = Content::embedded();
    let confirm = &content.ui.sounds.confirm;
    if id == "continue" {
        if !crate_box.buttons_ready {
            return;
        }
        commands.remove_resource::<RevealPlayback>();
        commands.remove_resource::<reveal_ui::RevealFrom>();
        next.set(AppState::Menu);
        *page = Page::Crates;
        audio::ui_cue(&mut commands, &assets, &content, &prefs, confirm);
        return;
    }
    let unseen = !prefs.pull_key.is_empty() && !has_seen(&prefs, &prefs.pull_key);
    let ten = if unseen { prefs.pull_ten } else { true };
    let cost = if ten { content.banners.cost.ten_pull_cost } else { content.banners.cost.single } as f32;
    // Disabled, busy, and short presses spend no key and play no cue.
    if !crate_box.buttons_ready || crate_box.pull_busy {
        return;
    }
    if crate_box.credits.is_some_and(|have| have + 1e-3 < cost) {
        return;
    }
    crate_box.pull_busy = true;
    let (key, ten) = claim_key(&mut prefs, ten);
    push_sent(&mut crate_box.sent_keys, key.clone());
    audio::ui_cue(&mut commands, &assets, &content, &prefs, confirm);
    let kept_keys = crate_box.sent_keys.clone();
    let kept_gen = crate_box.reveal_gen;
    let kept_credits = crate_box.credits;
    let failed_cue = content.crate_reveal.grid.pull_failed.clone();
    match crate::pull_host::round_trip(&content.banners.id, &key, ten) {
        Ok(reply) => {
            let (mut landed, pulled_rank) = crate_from_reply(&content, &reply);
            let refund = is_refund(&reply.fault);
            if refund {
                if let Some(pulled_rank) = pulled_rank {
                    rank.rank = pulled_rank;
                }
                landed.credits = Some(reply.credits);
                landed.reason.clear();
                keep_session(&mut landed, kept_keys, true, kept_gen + 1, true);
                *crate_box = landed;
                if let Some(playback) = playback.as_mut() {
                    playback.show = RevealShow::Refund(RefundRun::start(&content, ten));
                    playback.key.clear();
                }
                clear_pull_memory(&mut prefs);
                return;
            }
            let seen_replay = !reply.fresh && has_seen(&prefs, &key) && !landed.failed;
            if seen_replay {
                settle_success(&mut prefs, &key);
                crate_box.buttons_ready = true;
                crate_box.pull_busy = true;
                crate_box.sent_keys = kept_keys;
                crate_box.reveal_gen = kept_gen;
                crate_box.credits = kept_credits;
                crate_box.reason.clear();
                return;
            }
            let playable = !landed.failed
                && !is_short(&reply.fault)
                && ((ten && !landed.pods.is_empty()) || (!ten && landed.rarity.is_some()));
            if playable {
                match reveal_show(&content, &landed) {
                    Ok(show) => {
                        if let Some(playback) = playback.as_mut() {
                            playback.show = show;
                            playback.key = key.clone();
                        } else {
                            commands.insert_resource(RevealPlayback { show, key: key.clone() });
                        }
                        commands.insert_resource(reveal_ui::RevealFrom::Crates);
                        next.set_if_different(AppState::Reveal);
                        if let Some(pulled_rank) = pulled_rank {
                            rank.rank = pulled_rank;
                        }
                        landed.credits = Some(reply.credits);
                        landed.reason.clear();
                        keep_session(&mut landed, kept_keys, true, kept_gen + 1, false);
                        *crate_box = landed;
                        prefs.pull_key = key;
                        prefs.pull_ten = ten;
                        prefs.pull_awaiting = false;
                        save_settings(&prefs);
                    }
                    Err(message) => {
                        eprintln!("ten pull reveal failed: {message}");
                        if let Some(pulled_rank) = pulled_rank {
                            rank.rank = pulled_rank;
                        }
                        crate_box.credits = Some(reply.credits);
                        crate_box.reason.clear();
                        crate_box.buttons_ready = true;
                        crate_box.pull_busy = true;
                        crate_box.sent_keys = kept_keys;
                        crate_box.reveal_gen = kept_gen;
                    }
                }
                return;
            }
            if is_short(&reply.fault) {
                clear_pull_memory(&mut prefs);
                crate_box.reason = landed.reason;
                crate_box.buttons_ready = true;
                crate_box.pull_busy = true;
                crate_box.sent_keys = kept_keys;
                crate_box.reveal_gen = kept_gen;
                crate_box.credits = Some(reply.credits);
                return;
            }
            clear_pull_memory(&mut prefs);
            crate_box.reason = if landed.reason.is_empty() { content.ui.copy.pull_failed.clone() } else { landed.reason };
            crate_box.buttons_ready = true;
            crate_box.pull_busy = true;
            crate_box.sent_keys = kept_keys;
            crate_box.reveal_gen = kept_gen;
            crate_box.credits = kept_credits;
            if landed.failed {
                audio::ui_cue(&mut commands, &assets, &content, &prefs, &failed_cue);
            }
        }
        Err(err) => {
            eprintln!("crate pull failed: {err}");
            mark_lost(&mut prefs);
            crate_box.reason = transport_lost(&err, &content.ui.copy);
            crate_box.buttons_ready = true;
            crate_box.pull_busy = true;
            crate_box.sent_keys = kept_keys;
            crate_box.reveal_gen = kept_gen;
            crate_box.credits = kept_credits;
            audio::ui_cue(&mut commands, &assets, &content, &prefs, &failed_cue);
        }
    }
}

fn keep_session(landed: &mut ServerCrate, keys: Vec<String>, busy: bool, gen: u64, ready: bool) {
    landed.sent_keys = keys;
    landed.pull_busy = busy;
    landed.reveal_gen = gen;
    landed.buttons_ready = ready;
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
    clock: Res<'w, BankClock>,
    crate_box: Option<Res<'w, ServerCrate>>,
    game: Option<Res<'w, Game>>,
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
        view.clock.shown,
        view.crate_box.as_deref(),
        view.game.as_ref().map(|game| game.sim.connected_count()).unwrap_or(1),
        view.ready.0,
    );
    let Some(shot) = shot else {
        view.hits.0.clear();
        return;
    };
    let live = matches!(shot, MenuShot::Pause { live: true, .. });
    let MenuPaint { rgb, alpha, hits: painted } = paint_menu_masked(&content, &shot, width, height);
    view.hits.0 = painted;
    let target = view
        .menu_cam
        .iter()
        .next()
        .or_else(|| view.ui_cam.iter().next())
        .expect("menu UI has no camera");
    if live {
        let veil = content.palette.key(&content.ui.colors.veil);
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgba(veil[0], veil[1], veil[2], content.ui.layout.coop_pause_dim)),
            GlobalZIndex(0),
            Pickable::IGNORE,
            Visibility::Visible,
            MenuRoot,
            UiTargetCamera(target),
        ));
    }
    if let Some(images) = view.images.as_mut() {
        let handle = images.add(image_from_rgba(&rgb, &alpha, width, height));
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
            MenuRoot,
            UiTargetCamera(target),
        ));
    }
    for hit in &view.hits.0 {
        commands.spawn((
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
            MenuRoot,
            UiTargetCamera(target),
        )).observe(on_press);
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
    shown: f32,
    crate_box: Option<&ServerCrate>,
    connected: usize,
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
            return Some(MenuShot::Pause { focus, hero_id: hero.to_string(), live: connected > 1 });
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
                show: summary.show,
                waves: summary.waves,
                kills: summary.kills,
                seconds: summary.seconds,
                earned: shown,
                bonus: summary.bonus,
                balance: summary.balance,
            }
        }
        Page::Crates => MenuShot::Crates {
            focus,
            rarity: crate_box.and_then(|row| row.rarity.clone()),
            failed: crate_box.is_some_and(|row| row.failed),
            credits: crate_box.and_then(|row| row.credits),
            reason: crate_box.map(|row| row.reason.clone()).unwrap_or_default(),
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

fn save_settings(prefs: &Prefs) {
    if let Err(err) = settings::store(&Content::embedded(), prefs) {
        eprintln!("settings save failed: {err}");
    }
}

/// The key a press will send. An unseen saved key is resent, whichever button was pressed.
/// A new key is written before the request so a crash still has it.
fn claim_key(prefs: &mut Prefs, ten: bool) -> (String, bool) {
    if !prefs.pull_key.is_empty() && !has_seen(prefs, &prefs.pull_key) {
        return (prefs.pull_key.clone(), prefs.pull_ten);
    }
    let key = pull_key();
    prefs.pull_key = key.clone();
    prefs.pull_ten = ten;
    prefs.pull_awaiting = false;
    // On disk before the request leaves, so a crash during the send still has the key.
    save_settings(prefs);
    (key, ten)
}

fn mark_lost(prefs: &mut Prefs) {
    prefs.pull_awaiting = true;
    save_settings(prefs);
}

fn clear_pull_memory(prefs: &mut Prefs) {
    prefs.pull_key.clear();
    prefs.pull_ten = false;
    prefs.pull_awaiting = false;
    save_settings(prefs);
}

/// A successful reply for `key` ends the retry. A seen key leaves the unseen slot.
fn settle_success(prefs: &mut Prefs, key: &str) {
    prefs.pull_awaiting = false;
    if prefs.pull_key == key {
        prefs.pull_key.clear();
        prefs.pull_ten = false;
    }
    save_settings(prefs);
}

fn has_seen(prefs: &Prefs, key: &str) -> bool {
    !key.is_empty() && prefs.pull_seen_keys.iter().any(|row| row == key)
}

/// Remember a key whose best card has flipped. The list stays inside `saved.max_keys`.
pub(crate) fn note_seen(prefs: &mut Prefs, key: &str) {
    if key.is_empty() || has_seen(prefs, key) {
        return;
    }
    let max = Content::embedded().banners.saved.max_keys as usize;
    prefs.pull_seen_keys.push(key.to_string());
    if prefs.pull_seen_keys.len() > max {
        let drop_n = prefs.pull_seen_keys.len() - max;
        prefs.pull_seen_keys.drain(0..drop_n);
    }
}

/// Keys this session sent. The oldest drop once the list passes `saved.max_keys`.
pub(crate) fn push_sent(keys: &mut Vec<String>, key: String) {
    keys.push(key);
    let max = Content::embedded().banners.saved.max_keys as usize;
    if keys.len() > max {
        let drop_n = keys.len() - max;
        keys.drain(0..drop_n);
    }
}

/// The best card of `showing` has flipped. Only that key leaves the local save.
/// A pull that is still in flight, or a different key saved for retry, stays.
/// Nothing is sent to the server.
pub(crate) fn mark_pull_seen(prefs: &mut Prefs, showing: &str, in_flight: bool) {
    if in_flight || prefs.pull_awaiting || showing.is_empty() || prefs.pull_key != showing {
        return;
    }
    note_seen(prefs, showing);
    prefs.pull_key.clear();
    prefs.pull_ten = false;
    prefs.pull_awaiting = false;
    save_settings(prefs);
}

fn transport_lost(err: &crate::pull_host::PullHostError, copy: &gemini_sim::content::UiCopy) -> String {
    match err {
        crate::pull_host::PullHostError::Corrupt(_)
        | crate::pull_host::PullHostError::Unreadable(_)
        | crate::pull_host::PullHostError::Unavailable(_)
        | crate::pull_host::PullHostError::Lost(_) => copy.connection_lost.clone(),
    }
}

fn is_refund(fault: &Option<CratePullFault>) -> bool {
    matches!(fault, Some(CratePullFault::Pod { .. } | CratePullFault::Shards))
}

fn is_short(fault: &Option<CratePullFault>) -> bool {
    matches!(fault, Some(CratePullFault::Short { .. }))
}

fn show_result(content: &Content, landed: &ServerCrate, key: &str, commands: &mut Commands, next: &mut NextState<AppState>) -> Result<(), String> {
    commands.insert_resource(RevealPlayback {
        show: reveal_show(content, landed)?,
        key: key.to_string(),
    });
    commands.insert_resource(reveal_ui::RevealFrom::Crates);
    next.set_if_different(AppState::Reveal);
    Ok(())
}

fn reveal_show(content: &Content, landed: &ServerCrate) -> Result<RevealShow, String> {
    if landed.ten {
        let cards: Vec<GridCard> = landed
            .pods
            .iter()
            .map(|pod| GridCard {
                rarity: pod.rarity.clone(),
                fakeout: pod.fakeout.clone(),
                item: pod.item.clone(),
            })
            .collect();
        Ok(RevealShow::Grid(GridRun::start(content, &cards)?))
    } else {
        let rarity = landed.rarity.clone().ok_or_else(|| "single pull has no rarity".to_string())?;
        Ok(RevealShow::Single(RevealRun::start(content, &rarity, landed.fakeout.as_deref())?))
    }
}

/// A new 128-bit key for one pull. A retry of a lost connection does not call this.
pub(crate) fn pull_key() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("os randomness for a pull key");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn pods_of(reply: &CratePullReply) -> Vec<ServerPod> {
    reply
        .pods
        .iter()
        .map(|pod| ServerPod {
            rarity: pod.rarity.clone(),
            fakeout: pod.fakeout.clone(),
            item: pod.item.clone(),
        })
        .collect()
}

pub(crate) fn crate_from_reply(content: &Content, reply: &CratePullReply) -> (ServerCrate, Option<u32>) {
    let copy = &content.ui.copy;
    let places = content.ui.layout.currency_decimals;
    let pods = pods_of(reply);
    let base = |failed: bool, reason: String, rarity: Option<String>, fakeout: Option<String>, pods: Vec<ServerPod>| ServerCrate {
        rarity,
        fakeout,
        failed,
        reason,
        pods,
        ten: reply.ten,
        fresh: reply.fresh && !failed,
        credits: Some(reply.credits),
        pull_busy: false,
        reveal_gen: 0,
        sent_keys: Vec::new(),
        buttons_ready: false,
    };
    match &reply.fault {
        None => {
            let failed = if reply.ten { reply.pods.is_empty() } else { reply.rarity.is_none() };
            let reason = if failed { copy.pull_failed.clone() } else { String::new() };
            let rank = (!failed).then_some(reply.rank);
            (base(failed, reason, reply.rarity.clone(), reply.fakeout.clone(), pods), rank)
        }
        Some(CratePullFault::Short { cost, have }) => {
            let missing = (cost - have).max(0.0);
            (
                base(
                    false,
                    shortfall_line(copy, places, missing),
                    None,
                    None,
                    Vec::new(),
                ),
                None,
            )
        }
        Some(CratePullFault::Banner)
        | Some(CratePullFault::Pod { .. })
        | Some(CratePullFault::Key)
        | Some(CratePullFault::Mismatch)
        | Some(CratePullFault::Item)
        | Some(CratePullFault::Seen) => {
            (base(true, copy.pull_failed.clone(), None, None, Vec::new()), None)
        }
        Some(CratePullFault::Unavailable) => (base(true, copy.pull_needs_server.clone(), None, None, Vec::new()), None),
        Some(CratePullFault::Shards) => (base(true, copy.pull_no_shards.clone(), None, None, Vec::new()), None),
    }
}

fn image_from_rgba(rgb: &[u8], alpha: &[u8], width: u32, height: u32) -> Image {
    let pixels = (width as usize).saturating_mul(height as usize);
    assert_eq!(rgb.len(), pixels * 3, "menu image rgb length");
    assert_eq!(alpha.len(), pixels, "menu alpha mask length");
    let mut rgba = Vec::with_capacity(pixels * 4);
    for (index, px) in rgb.chunks_exact(3).enumerate() {
        let cover = alpha[index];
        rgba.extend_from_slice(&[px[0], px[1], px[2], cover]);
    }
    Image::new(
        Extent3d { width, height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn lock_cursor(cursor: &mut Query<&mut CursorOptions>, pause: &mut PauseMenu, lock: bool) {
    if let Ok(mut cursor) = cursor.single_mut() {
        if lock {
            if cursor.grab_mode != CursorGrabMode::Locked {
                pause.swallow_fire = true;
            }
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

