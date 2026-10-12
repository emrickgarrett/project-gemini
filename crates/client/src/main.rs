//! Project Gemini, milestone M1. WebGPU build, with a separate WebGL2 build.

mod audio;
mod flow;
mod hud;
#[cfg(not(target_arch = "wasm32"))]
mod net;
mod reveal_ui;
mod session;
mod settings;
mod world;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;

#[derive(States, Clone, Eq, PartialEq, Hash, Debug, Default)]
enum AppState {
    #[default]
    Menu,
    Playing,
    Reveal,
}

fn main() {
    let mut app = App::new();
    app.add_plugins(
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
    );
    app.insert_resource(ClearColor(Color::srgb(0.07, 0.06, 0.09)));
    audio::register(&mut app);
    install_match(&mut app);
    app.run();
}

fn install_match(app: &mut App) {
    app.init_state::<AppState>()
        .insert_resource(session::PendingDifficulty("normal".into()))
        .insert_resource(session::SimClock::default())
        .insert_resource(session::Account { rank: 1 })
        .insert_resource(session::launch_from_env());
    flow::register(app);
    app.add_systems(OnEnter(AppState::Menu), audio::menu_theme)
        .add_systems(OnExit(AppState::Menu), audio::stop_all)
        .add_systems(OnEnter(AppState::Playing), (session::boot, world::scene, audio::boot).chain())
        .add_systems(OnExit(AppState::Playing), (despawn_play, session::shutdown, audio::stop_all))
        .add_systems(
            Update,
            (
                (flow::pause_keys, flow::navigate, flow::hover, flow::apply, session::gather_and_step).chain(),
                (
                    audio::play,
                    audio::hold_loops,
                    audio::attach_duck,
                    audio::sync_gains,
                    audio::sync_crowd,
                    world::sync,
                    world::sync_viewmodel,
                    world::camera,
                    world::grade,
                    hud::draw,
                )
                    .chain(),
                (flow::rebuild, flow::end_of_match, flow::click_prompt).chain(),
            )
                .chain()
                .run_if(in_state(AppState::Playing)),
        )
        .add_systems(
            Update,
            (session::autoplay, audio::menu_levels, audio::sync_gains, audio::attach_duck).run_if(in_state(AppState::Menu)),
        )
        .add_systems(Update, reveal_ui::tick.run_if(in_state(AppState::Reveal)))
        .add_systems(OnExit(AppState::Reveal), reveal_ui::close);
}

fn despawn_play(mut commands: Commands, pieces: Query<Entity, With<world::PlayPiece>>) {
    for entity in &pieces {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::audio::{AudioPlayer, AudioSource};
    use bevy::input::mouse::AccumulatedMouseMotion;
    use bevy::pbr::StandardMaterial;
    use bevy::prelude::*;
    use bevy::time::TimeUpdateStrategy;

    use super::*;

    #[derive(Resource, Default)]
    struct MatchEntries(u32);

    fn count_match_enter(mut entries: ResMut<MatchEntries>) {
        entries.0 += 1;
    }

    fn durable_ids(app: &mut App) -> Vec<Entity> {
        let mut query = app
            .world_mut()
            .query_filtered::<Entity, Without<AudioPlayer<AudioSource>>>();
        query.iter(app.world()).collect()
    }

    #[test]
    fn menu_start_runs_the_match_once_and_the_sim_advances() {
        let _guard = settings_lock();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::state::app::StatesPlugin,
            AssetPlugin {
                file_path: "../../assets".to_string(),
                ..default()
            },
        ));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<AudioSource>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<AccumulatedMouseMotion>();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 60.0)));
        app.insert_resource(MatchEntries::default());
        app.add_message::<bevy::window::WindowOccluded>();
        install_match(&mut app);
        app.add_systems(OnEnter(AppState::Playing), count_match_enter);

        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::KeyW);
        drop(keys);
        app.world_mut().resource_mut::<session::PendingDifficulty>().0 = "normal".into();
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Playing);
        app.world_mut().resource_mut::<session::SimClock>().set_accumulated(30.0);

        let mut wave_starts = 0u32;
        let mut music_restarts = 0u32;
        let mut match_music: Option<Vec<Entity>> = None;
        let mut player_at_start = None;
        let mut bug_at_start: Option<(u32, f32, f32)> = None;
        let mut durable_settled = Vec::new();
        let mut checked_boot_clock = false;

        for frame in 0..120 {
            app.update();
            let playing = matches!(app.world().resource::<State<AppState>>().get(), AppState::Playing);
            if let Some(fx) = app.world().get_resource::<session::FrameFx>() {
                wave_starts += fx
                    .sounds
                    .iter()
                    .filter(|sound| sound.id == "wave" || sound.id == "wave_start")
                    .count() as u32;
            }
            if playing {
                if let Some(music) = app.world().get_resource::<audio::Music>() {
                    if let Some(previous) = &match_music {
                        if previous != &music.entities {
                            music_restarts += 1;
                        }
                    } else if !music.entities.is_empty() {
                        match_music = Some(music.entities.clone());
                    }
                }
                if let Some(game) = app.world().get_resource::<session::Game>() {
                    if !checked_boot_clock {
                        checked_boot_clock = true;
                        assert_eq!(game.sim.tick, 0, "match start should drop leftover sim time");
                        let acc = app.world().resource::<session::SimClock>().accumulated();
                        assert!(acc < 0.05, "accumulator after boot {acc}");
                    }
                    if player_at_start.is_none() {
                        player_at_start = Some(game.sim.players[0].pos);
                    }
                    if bug_at_start.is_none() {
                        if let Some(bug) = game.sim.enemies.iter().find(|enemy| enemy.alive) {
                            bug_at_start = Some((bug.id, bug.pos.x, bug.pos.z));
                        }
                    }
                }
            }
            if frame == 100 {
                durable_settled = durable_ids(&mut app);
            }
        }

        let later = durable_ids(&mut app);
        let kept = durable_settled.iter().filter(|id| later.contains(id)).count();
        assert_eq!(
            kept,
            durable_settled.len(),
            "match entities respawned after the wave settled ({kept} of {} still alive, now {})",
            durable_settled.len(),
            later.len()
        );
        assert_eq!(
            later.len(),
            durable_settled.len(),
            "match entity count changed after the wave settled"
        );
        assert!(durable_settled.len() > 20, "scene did not spawn, count {}", durable_settled.len());

        assert_eq!(app.world().resource::<MatchEntries>().0, 1, "match setup should run once");
        assert!(matches!(app.world().resource::<State<AppState>>().get(), AppState::Playing));
        assert_eq!(wave_starts, 1, "wave_start cues");
        assert_eq!(music_restarts, 0, "music track restarted during the match");
        assert!(match_music.as_ref().is_some_and(|ids| !ids.is_empty()), "music track did not start");

        let game = app.world().resource::<session::Game>();
        assert!(game.sim.time > 1.0, "wave clock {}", game.sim.time);
        assert!(game.sim.director.wave >= 1);
        assert!(game.sim.director.spawned_this_wave > 0, "wave 1 did not spawn");
        let start = player_at_start.expect("player");
        let pos = game.sim.players[0].pos;
        let dx = pos.x - start.x;
        let dz = pos.z - start.z;
        assert!(dx * dx + dz * dz > 0.25, "player did not move: {start:?} -> {pos:?}");
        let (bug_id, bx, bz) = bug_at_start.expect("a bug should spawn");
        let bug = game.sim.enemies.iter().find(|enemy| enemy.id == bug_id).expect("bug still in the sim");
        let moved = (bug.pos.x - bx) * (bug.pos.x - bx) + (bug.pos.z - bz) * (bug.pos.z - bz);
        assert!(moved > 0.25, "bug {bug_id} did not move ({bx},{bz}) -> ({},{})", bug.pos.x, bug.pos.z);
        assert!(checked_boot_clock, "match never started");

        let tick_before = game.sim.tick;
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(30)));
        app.update();
        let tick_after = app.world().resource::<session::Game>().sim.tick;
        assert_eq!(tick_after - tick_before, 5, "a 30s frame must run only max_sim_steps_per_frame");

        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 60.0)));
        app.update();
        let tick_small = app.world().resource::<session::Game>().sim.tick;
        assert!(
            tick_small - tick_after <= 1,
            "leftover hitch time must be discarded, advanced {}",
            tick_small - tick_after
        );

        app.world_mut().resource_mut::<session::SimClock>().set_accumulated(4.0);
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Menu);
        app.update();
        assert_eq!(app.world().resource::<session::SimClock>().accumulated(), 0.0);
        assert!(matches!(app.world().resource::<State<AppState>>().get(), AppState::Menu));
    }

    #[test]
    fn three_clicks_start_the_match_and_play_again_skips_the_pickers() {
        let _guard = settings_lock();
        let mut app = menu_app();
        app.update();
        assert!(matches!(app.world().resource::<State<AppState>>().get(), AppState::Menu));
        choose(&mut app, "play");
        assert!(matches!(*app.world().resource::<flow::Page>(), flow::Page::Heroes));
        let hero = gemini_sim::content::Content::embedded().hero.id.clone();
        choose(&mut app, &format!("hero:{hero}"));
        assert!(matches!(*app.world().resource::<flow::Page>(), flow::Page::Difficulty));
        choose(&mut app, "tier:normal");
        assert!(matches!(app.world().resource::<State<AppState>>().get(), AppState::Playing));

        let tick = app.world().resource::<session::Game>().sim.tick;
        app.world_mut().resource_mut::<session::PauseMenu>().open = true;
        for _ in 0..8 {
            app.update();
        }
        assert_eq!(app.world().resource::<session::Game>().sim.tick, tick, "solo pause must stop the sim");
        app.world_mut().resource_mut::<session::PauseMenu>().open = false;
        app.update();
        assert!(app.world().resource::<session::Game>().sim.tick > tick, "closing pause resumes the sim");

        app.world_mut().resource_mut::<session::Game>().sim.phase = gemini_sim::sim::Phase::Victory;
        app.update();
        app.update();
        assert!(matches!(*app.world().resource::<flow::Page>(), flow::Page::Results));
        assert!(app.world().get_resource::<flow::MatchSummary>().is_some());
        choose(&mut app, "play_again");
        assert!(matches!(app.world().resource::<State<AppState>>().get(), AppState::Playing));
    }

    #[test]
    fn a_bad_remembered_hero_opens_the_picker_again() {
        let _guard = settings_lock();
        let mut app = menu_app();
        app.update();
        {
            let mut prefs = app.world_mut().resource_mut::<settings::Prefs>();
            prefs.character = Some("not-a-hero".into());
            prefs.difficulty = Some("normal".into());
        }
        app.world_mut().insert_resource(flow::MatchSummary {
            victory: true,
            waves: 1,
            kills: 1,
            seconds: 10.0,
            currency: 5.0,
        });
        app.world_mut().resource_mut::<flow::Page>().clone_from(&flow::Page::Results);
        choose(&mut app, "play_again");
        assert!(matches!(*app.world().resource::<flow::Page>(), flow::Page::Heroes));
        assert!(app.world().resource::<settings::Prefs>().character.is_none());
        assert!(matches!(app.world().resource::<State<AppState>>().get(), AppState::Menu));
    }

    #[test]
    fn solo_pause_is_local_and_coop_keeps_running() {
        assert!(flow::freeze_solo(true, 1));
        assert!(!flow::freeze_solo(true, 2), "co-op pause must not stop the shared match");
        assert!(!flow::freeze_solo(false, 1));
    }

    fn menu_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::state::app::StatesPlugin,
            AssetPlugin {
                file_path: "../../assets".to_string(),
                ..default()
            },
        ));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<bevy::image::Image>();
        app.init_asset::<bevy::audio::AudioSource>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<bevy::input::mouse::AccumulatedMouseMotion>();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 60.0)));
        app.add_message::<bevy::window::WindowOccluded>();
        install_match(&mut app);
        app
    }

    fn choose(app: &mut App, id: &str) {
        app.world_mut().resource_mut::<flow::PendingAction>().0 = Some(id.to_string());
        // The click lands in this update. A state change is applied on the next one.
        app.update();
        app.update();
    }

    fn settings_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let path = std::env::temp_dir().join(format!("gemini-ui-test-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("GEMINI_SETTINGS", &path);
        guard
    }
}
