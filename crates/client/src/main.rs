//! Project Gemini, milestone M1. WebGPU build, with a separate WebGL2 build.

mod audio;
mod hud;
mod session;
mod world;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

#[derive(States, Clone, Eq, PartialEq, Hash, Debug, Default)]
enum AppState {
    #[default]
    Menu,
    Playing,
}

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Project Gemini".into(),
                        fit_canvas_to_parent: true,
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    // cargo run sets the base to crates/client. The binary's base is
                    // target/debug. Both resolve this relative path to the repo assets folder.
                    // Wasm keeps the URL path `assets`, which build-web.sh copies into web/.
                    file_path: if cfg!(target_arch = "wasm32") {
                        "assets".to_string()
                    } else {
                        "../../assets".to_string()
                    },
                    ..default()
                }),
        )
        .init_state::<AppState>()
        .insert_resource(ClearColor(Color::srgb(0.07, 0.06, 0.09)))
        .insert_resource(session::PendingDifficulty("normal".into()))
        .add_systems(OnEnter(AppState::Menu), (menu, audio::menu_theme))
        .add_systems(OnExit(AppState::Menu), (despawn_menu, audio::stop_all))
        .add_systems(OnEnter(AppState::Playing), (session::boot, world::scene, audio::boot).chain())
        .add_systems(OnExit(AppState::Playing), (despawn_play, session::shutdown, audio::stop_all))
        .add_systems(
            Update,
            (
                session::gather_and_step,
                audio::play,
                audio::sync_gains,
                audio::sync_crowd,
                world::sync,
                world::sync_viewmodel,
                world::camera,
                hud::draw,
            )
                .chain()
                .run_if(in_state(AppState::Playing)),
        )
        .add_systems(Update, menu_keys.run_if(in_state(AppState::Menu)))
        .run();
}

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct DiffButton(&'static str);

fn menu(mut commands: Commands) {
    let content = gemini_sim::content::Content::embedded();
    let teal = srgb(&content, "hero_teal");
    let white = srgb(&content, "hero_white");
    let accent = srgb(&content, "hero_accent");
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(12.0),
            ..default()
        },
        MenuRoot,
        children![
            (
                Text::new("PROJECT GEMINI"),
                TextFont::from_font_size(42.0),
                TextColor(teal),
            ),
            (
                Text::new("1 NORMAL    2 HARD    3 HEROIC    4 LEGENDARY"),
                TextFont::from_font_size(16.0),
                TextColor(white),
            ),
        ],
    ));
    for (id, label) in [("normal", "NORMAL"), ("hard", "HARD"), ("heroic", "HEROIC"), ("legendary", "LEGENDARY")] {
        commands
            .spawn((
                Button,
                DiffButton(id),
                Node {
                    width: Val::Px(240.0),
                    height: Val::Px(42.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(accent),
                MenuRoot,
                children![(Text::new(label), TextFont::from_font_size(18.0), TextColor(white))],
            ))
            .observe(pick_difficulty);
    }
}

fn pick_difficulty(
    activate: On<Activate>,
    buttons: Query<&DiffButton>,
    mut pending: ResMut<session::PendingDifficulty>,
    mut next: ResMut<NextState<AppState>>,
) {
    if let Ok(diff) = buttons.get(activate.entity) {
        pending.0 = diff.0.to_string();
        next.set(AppState::Playing);
    }
}

fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut pending: ResMut<session::PendingDifficulty>,
    mut next: ResMut<NextState<AppState>>,
) {
    let pick = if keys.just_pressed(KeyCode::Digit1) {
        Some("normal")
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some("hard")
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some("heroic")
    } else if keys.just_pressed(KeyCode::Digit4) {
        Some("legendary")
    } else {
        None
    };
    if let Some(id) = pick {
        pending.0 = id.to_string();
        next.set(AppState::Playing);
    }
}

fn despawn_menu(mut commands: Commands, roots: Query<Entity, With<MenuRoot>>) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}

fn despawn_play(mut commands: Commands, pieces: Query<Entity, With<world::PlayPiece>>) {
    for entity in &pieces {
        commands.entity(entity).despawn();
    }
}

fn srgb(content: &gemini_sim::content::Content, key: &str) -> Color {
    let c = content.palette.key(key);
    Color::srgb(c[0], c[1], c[2])
}
