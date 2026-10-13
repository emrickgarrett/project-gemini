//! Project Gemini, milestone M1. WebGPU build, with a separate WebGL2 build.

mod act_now;
mod audio;
mod flow;
mod hud;
mod pull_host;
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
    pull_host::boot();
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
                    audio::sync_bed,
                    audio::sync_crowd,
                    world::sync,
                    world::sync_viewmodel,
                    world::camera,
                    world::grade,
                    hud::draw,
                    hud::resume_prompt,
                )
                    .chain(),
                (flow::rebuild, flow::end_of_match, flow::click_prompt, act_now::dispatch, audio::flush_ui_cues).chain(),
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
    use bevy::window::{CursorGrabMode, CursorOptions};

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
        app.init_asset::<bevy::image::Image>();
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

    #[test]
    fn pause_buttons_unlock_the_cursor_and_sit_above_the_rifle() {
        let _guard = settings_lock();
        let mut app = menu_app();
        app.world_mut().spawn(locked_cursor());
        app.update();
        press_hit(&mut app, "play");
        assert!(matches!(*app.world().resource::<flow::Page>(), flow::Page::Heroes));
        press_hit(&mut app, "back");
        assert!(matches!(*app.world().resource::<flow::Page>(), flow::Page::Title));
        press_hit(&mut app, "settings");
        assert!(matches!(*app.world().resource::<flow::Page>(), flow::Page::Settings));
        press_hit(&mut app, "back");

        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Playing);
        app.update();
        assert_eq!(cursor_state(&mut app).0, CursorGrabMode::Locked);
        tap(&mut app, KeyCode::Escape);
        let (grab, shown) = cursor_state(&mut app);
        assert_eq!(grab, CursorGrabMode::None, "pause must release the cursor");
        assert!(shown);
        assert!(app.world().resource::<session::PauseMenu>().open);

        let picking = app.world().resource::<bevy::picking::mesh_picking::MeshPickingSettings>();
        assert!(picking.require_markers);
        let mut world_order = None;
        let mut view_order = None;
        let mut ui_order = None;
        {
            let mut query = app.world_mut().query::<(
                &Camera,
                Option<&world::ViewmodelCamera>,
                Option<&bevy::picking::mesh_picking::MeshPickingCamera>,
                Option<&IsDefaultUiCamera>,
            )>();
            for (camera, viewmodel, mesh, ui) in query.iter(app.world()) {
                if viewmodel.is_some() {
                    assert!(mesh.is_none(), "the viewmodel camera must not pick meshes");
                    view_order = Some(camera.order);
                } else if ui.is_some() {
                    assert!(mesh.is_none());
                    ui_order = Some(camera.order);
                } else {
                    assert!(mesh.is_some(), "the world camera keeps mesh picking");
                    world_order = Some(camera.order);
                }
            }
        }
        let world_order = world_order.expect("world camera");
        let view_order = view_order.expect("viewmodel camera");
        let ui_order = ui_order.expect("ui camera");
        assert!(ui_order > view_order && view_order > world_order);

        let mut images = app.world_mut().query::<(&ImageNode, &bevy::picking::Pickable)>();
        assert!(images.iter(app.world()).any(|(_, pick)| pick == &bevy::picking::Pickable::IGNORE));
        let mut hud_roots = app.world_mut().query::<(&hud::HudNode, &bevy::picking::Pickable, Option<&ChildOf>)>();
        assert!(hud_roots.iter(app.world()).any(|(_, pick, parent)| parent.is_none() && pick == &bevy::picking::Pickable::IGNORE));

        press_hit(&mut app, "settings");
        assert!(app.world().resource::<session::PauseMenu>().settings);
        tap(&mut app, KeyCode::Escape);
        assert!(!app.world().resource::<session::PauseMenu>().settings);
        assert!(app.world().resource::<session::PauseMenu>().open);
        press_hit(&mut app, "resume");
        assert!(!app.world().resource::<session::PauseMenu>().open);
        tap(&mut app, KeyCode::Escape);
        press_hit(&mut app, "quit_to_menu");
        assert!(matches!(app.world().resource::<State<AppState>>().get(), AppState::Menu));
    }

    #[test]
    fn paused_coop_hero_still_takes_damage() {
        let _guard = settings_lock();
        let mut app = menu_app();
        app.update();
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Playing);
        app.update();
        place_tyrant(&mut app);
        let hp = app.world().resource::<session::Game>().sim.players[0].hp;
        let tick = app.world().resource::<session::Game>().sim.tick;
        app.world_mut().resource_mut::<session::PauseMenu>().open = true;
        app.update();
        assert_eq!(app.world().resource::<session::Game>().sim.tick, tick, "solo pause freezes the match");
        assert_eq!(app.world().resource::<session::Game>().sim.players[0].hp, hp);
        assert!(!has_pause_veil(&mut app), "solo pause keeps the dimmed still");

        app.world_mut().resource_mut::<session::PauseMenu>().open = false;
        join_second(&mut app);
        place_tyrant(&mut app);
        let hp = app.world().resource::<session::Game>().sim.players[0].hp;
        let tick = app.world().resource::<session::Game>().sim.tick;
        app.world_mut().resource_mut::<session::PauseMenu>().open = true;
        app.update();
        let game = app.world().resource::<session::Game>();
        assert!(game.sim.tick > tick, "co-op pause keeps the match running");
        assert!(game.sim.players[0].hp < hp, "the hero stays in the world and can take damage");
        assert!(has_pause_veil(&mut app), "co-op pause dims the live match");
    }

    #[test]
    fn a_pull_is_a_proto_request_and_only_a_reply_fills_the_crate() {
        let _guard = settings_lock();
        let content = gemini_sim::content::Content::embedded();
        let bytes = pull_host::request_bytes(&content.banners.id).unwrap();
        let packet = gemini_proto::Packet::decode(&bytes).unwrap();
        let gemini_proto::Packet::CratePull(request) = packet else {
            panic!("a pull is a crate-pull packet");
        };
        assert_eq!(request.banner, content.banners.id);

        let mut app = menu_app();
        app.update();
        choose(&mut app, "crates");
        choose(&mut app, "open");
        assert!(app.world().get_resource::<reveal_ui::RevealPlayback>().is_none(), "open does nothing before a server result");
        choose(&mut app, "pull");
        let missed = app.world().resource::<flow::ServerCrate>().clone();
        assert!(missed.failed);
        assert!(missed.rarity.is_none());
        assert_eq!(missed.reason, content.ui.copy.pull_needs_server);

        let short = gemini_proto::CratePullReply {
            version: gemini_proto::PROTOCOL_VERSION,
            rarity: None,
            fakeout: None,
            rank: 0,
            fault: Some(gemini_proto::CratePullFault::Short { cost: content.banners.cost.single as f32, have: 0.0 }),
        };
        let (failed, _) = flow::crate_from_reply(&content, &short);
        assert!(failed.failed);
        assert!(failed.rarity.is_none());
        assert!(failed.reason.starts_with(&content.ui.copy.pull_need), "{}", failed.reason);
        assert!(failed.reason.contains(&content.ui.copy.pull_have), "{}", failed.reason);
        app.insert_resource(failed);
        choose(&mut app, "open");
        assert!(matches!(app.world().resource::<State<AppState>>().get(), AppState::Menu));
        assert!(app.world().get_resource::<reveal_ui::RevealPlayback>().is_none());

        let shards = gemini_proto::CratePullReply {
            version: gemini_proto::PROTOCOL_VERSION,
            rarity: None,
            fakeout: None,
            rank: 0,
            fault: Some(gemini_proto::CratePullFault::Shards),
        };
        let (missing, _) = flow::crate_from_reply(&content, &shards);
        assert!(missing.failed);
        assert_eq!(missing.reason, content.ui.copy.pull_no_shards);
        assert_ne!(missing.reason, content.ui.copy.pull_needs_server);

        let reply = gemini_proto::CratePullReply {
            version: gemini_proto::PROTOCOL_VERSION,
            rarity: Some("rare".into()),
            fakeout: Some("epic".into()),
            rank: 2,
            fault: None,
        };
        let (filled, rank) = flow::crate_from_reply(&content, &reply);
        assert!(!filled.failed);
        assert_eq!(rank, Some(2));
        app.insert_resource(filled);
        app.world_mut().resource_mut::<session::Account>().rank = rank.unwrap();
        choose(&mut app, "open");
        let playback = app.world().get_resource::<reveal_ui::RevealPlayback>().expect("reveal starts after the server result");
        assert_eq!(playback.0.plan.rarity, "rare");
    }

    #[test]
    fn skip_cuts_the_reveal_down_to_the_card_sound() {
        let _guard = settings_lock();
        let mut app = menu_app();
        let content = gemini_sim::content::Content::embedded();
        let run = gemini_sim::reveal::RevealRun::start(&content, "rare", Some("epic")).unwrap();
        app.insert_resource(reveal_ui::RevealPlayback(run));
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Reveal);
        app.update();
        app.world_mut().spawn((reveal_ui::RevealVoice { id: "crate_tease".into() }, OldVoice));
        tap(&mut app, KeyCode::Space);
        let mut old = app.world_mut().query::<&OldVoice>();
        assert_eq!(old.iter(app.world()).count(), 0, "skip stops whatever was already playing");
        let mut voices = app.world_mut().query::<&reveal_ui::RevealVoice>();
        let left: Vec<_> = voices.iter(app.world()).map(|voice| voice.id.clone()).collect();
        assert_eq!(left, vec!["crate_rare_hit".to_string()], "skip plays only the rolled tier hit");
    }

    #[test]
    fn menu_camera_draws_ui_on_boot_and_after_a_match() {
        let _guard = settings_lock();
        let mut app = menu_app();
        app.update();
        assert_menu_shown(&mut app);

        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Playing);
        app.update();
        assert!(matches!(app.world().resource::<State<AppState>>().get(), AppState::Playing));
        assert_eq!(menu_cameras(&mut app).len(), 0, "the menu camera must leave with the menu");
        assert!(camera_count(&mut app) > 0, "the match should spawn a world camera");

        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Menu);
        app.update();
        assert!(matches!(app.world().resource::<State<AppState>>().get(), AppState::Menu));
        assert_eq!(camera_count(&mut app), 0, "returning to the menu must despawn the match cameras");
        assert_menu_shown(&mut app);
    }

    #[test]
    fn holding_e_while_downed_self_revives_and_the_prompt_tracks_it() {
        let _guard = settings_lock();
        let mut app = menu_app();
        app.update();
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Playing);
        app.update();
        {
            let mut game = app.world_mut().resource_mut::<session::Game>();
            game.sim.enemies.clear();
            game.sim.director.pool = 50.0;
            game.sim.director.parked = true;
            let player = &mut game.sim.players[0];
            player.hp = 0.0;
            player.downed = true;
            player.self_revives_used = 0;
            player.self_revive_held = 0.0;
            player.invuln = 0.0;
        }
        app.update();
        let idle = prompt_line(&mut app);
        assert!(idle.contains("HOLD E TO REVIVE"), "{idle}");
        assert!(idle.contains("0%"), "a fresh down should start at 0%: {idle}");

        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyE);
        app.world_mut().resource_mut::<session::SimClock>().set_accumulated(1.0);
        app.update();
        let held = app.world().resource::<session::Game>().sim.players[0].self_revive_held;
        assert!(held > 0.1, "holding E did not drive the self-revive ({held})");
        let moving = prompt_line(&mut app);
        assert!(moving.contains("HOLD E TO REVIVE"), "{moving}");
        assert!(!moving.contains("  0%"), "prompt stuck at 0% while held {held}: {moving}");

        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(0.2)));
        for _ in 0..40 {
            app.update();
        }
        let player = &app.world().resource::<session::Game>().sim.players[0];
        assert!(!player.downed, "holding E should self-revive");
        assert!(player.alive);
        assert!(player.hp > 1.0, "self-revive should restore hp, got {}", player.hp);
    }

    #[test]
    fn upgrade_keys_latch_until_the_sim_ticks_and_the_cursor_comes_back() {
        let _guard = settings_lock();
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::Digit1);
        assert_eq!(session::upgrade_index(&keys, 3), Some(0));
        keys = ButtonInput::default();
        keys.press(KeyCode::Numpad2);
        assert_eq!(session::upgrade_index(&keys, 3), Some(1));
        keys = ButtonInput::default();
        keys.press(KeyCode::Digit4);
        assert_eq!(session::upgrade_index(&keys, 3), None);
        keys.press(KeyCode::Numpad4);
        assert_eq!(session::upgrade_index(&keys, 4), Some(3));

        let mut app = menu_app();
        app.world_mut().spawn(locked_cursor());
        app.update();
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Playing);
        app.update();
        let (grab, visible) = cursor_state(&mut app);
        assert_eq!(grab, CursorGrabMode::Locked);
        assert!(!visible);

        offer_upgrades(&mut app);
        app.world_mut().resource_mut::<session::SimClock>().set_accumulated(0.0);
        app.update();
        let (grab, visible) = cursor_state(&mut app);
        assert_eq!(grab, CursorGrabMode::None, "the picker must release the cursor");
        assert!(visible, "the picker must show the cursor");
        assert!(hud_text(&mut app).iter().any(|line| line.contains("PRESS 1")), "cards should show the key hint");

        // A 60 Hz frame does not reach the 30 Hz sim tick. The press has to survive that frame.
        app.world_mut().resource_mut::<session::SimClock>().set_accumulated(0.0);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Digit1);
        app.update();
        assert!(matches!(app.world().resource::<session::Game>().sim.phase, gemini_sim::sim::Phase::Upgrade { .. }));
        assert_eq!(app.world().resource::<session::Game>().pending_pick, Some(0));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::Digit1);
        app.update();
        {
            let game = app.world().resource::<session::Game>();
            assert!(game.sim.players[0].upgrades.contains_key("rapid_cells"), "{:?}", game.sim.players[0].upgrades);
            assert!(matches!(game.sim.phase, gemini_sim::sim::Phase::Breather { .. }));
        }
        let (grab, visible) = cursor_state(&mut app);
        assert_eq!(grab, CursorGrabMode::Locked, "play should grab the cursor again");
        assert!(!visible);

        offer_upgrades(&mut app);
        app.world_mut().resource_mut::<session::SimClock>().set_accumulated(0.0);
        app.update();
        app.world_mut().resource_mut::<session::SimClock>().set_accumulated(0.0);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Numpad2);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::Numpad2);
        app.update();
        let game = app.world().resource::<session::Game>();
        assert!(game.sim.players[0].upgrades.contains_key("hollow_points"), "{:?}", game.sim.players[0].upgrades);
    }

    #[test]
    fn clicking_an_upgrade_card_picks_it() {
        let _guard = settings_lock();
        let mut app = menu_app();
        app.world_mut().spawn(locked_cursor());
        app.update();
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Playing);
        app.update();
        offer_upgrades(&mut app);
        app.update();
        let content = gemini_sim::content::Content::embedded();
        let rgb = content.palette.key(&content.ui.colors.panel);
        let alpha = content.ui.layout.panel_alpha;
        let card = {
            let mut query = app.world_mut().query::<(Entity, &hud::UpgradeCard, &BackgroundColor)>();
            let found: Vec<_> = query.iter(app.world()).map(|(entity, card, fill)| (entity, card.0, *fill)).collect();
            assert!(found.iter().any(|(_, index, _)| *index == 2), "card 3 should exist: {found:?}");
            for (_, _, BackgroundColor(fill)) in &found {
                let fill = fill.to_srgba();
                assert!((fill.red - rgb[0]).abs() < 1e-4 && (fill.green - rgb[1]).abs() < 1e-4 && (fill.blue - rgb[2]).abs() < 1e-4, "card fill should be the panel palette");
                assert!((fill.alpha - alpha).abs() < 1e-4, "card alpha should be ui layout.panel_alpha");
            }
            found.into_iter().find(|(_, index, _)| *index == 2).unwrap().0
        };
        app.world_mut().trigger(bevy::ui_widgets::Activate { entity: card });
        app.world_mut().resource_mut::<session::SimClock>().set_accumulated(1.0);
        app.update();
        let game = app.world().resource::<session::Game>();
        assert!(game.sim.players[0].upgrades.contains_key("servo_legs"), "{:?}", game.sim.players[0].upgrades);
        assert!(matches!(game.sim.phase, gemini_sim::sim::Phase::Breather { .. }));
    }

    #[test]
    fn relock_click_does_not_fire_until_the_next_frame() {
        let _guard = settings_lock();
        let mut app = menu_app();
        app.world_mut().spawn(locked_cursor());
        app.update();
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Playing);
        app.update();
        {
            let mut query = app.world_mut().query::<&mut CursorOptions>();
            let mut cursor = query.single_mut(app.world_mut()).expect("cursor");
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
        let before = app.world().resource::<session::Game>().sim.players[0].primary.ammo;
        assert!(before > 0, "the starter gun should be loaded");
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.world_mut().resource_mut::<session::SimClock>().set_accumulated(1.0);
        app.update();
        let (grab, visible) = cursor_state(&mut app);
        assert_eq!(grab, CursorGrabMode::Locked);
        assert!(!visible);
        let grabbed = app.world().resource::<session::Game>().sim.players[0].primary.ammo;
        assert_eq!(grabbed, before, "the click that locks the pointer must not fire");

        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear_just_pressed(MouseButton::Left);
        app.world_mut().resource_mut::<session::SimClock>().set_accumulated(1.0);
        app.update();
        let held = app.world().resource::<session::Game>().sim.players[0].primary.ammo;
        assert!(held < before, "a hold after the grab should fire, ammo {before} -> {held}");
    }

    #[test]
    fn settings_decrease_with_keys_and_the_minus_button() {
        let _guard = settings_lock();
        let mut app = menu_app();
        app.update();
        choose(&mut app, "settings");
        assert!(matches!(*app.world().resource::<flow::Page>(), flow::Page::Settings));
        let start = app.world().resource::<settings::Prefs>().sensitivity;
        tap(&mut app, KeyCode::ArrowLeft);
        let left = app.world().resource::<settings::Prefs>().sensitivity;
        assert!(left < start, "left arrow should decrease {start} -> {left}");
        tap(&mut app, KeyCode::KeyA);
        let nudged = app.world().resource::<settings::Prefs>().sensitivity;
        assert!(nudged < left, "A should decrease {left} -> {nudged}");
        choose(&mut app, "slider:sensitivity:-");
        let minus = app.world().resource::<settings::Prefs>().sensitivity;
        assert!(minus < nudged, "minus button should decrease {nudged} -> {minus}");
        tap(&mut app, KeyCode::ArrowRight);
        let right = app.world().resource::<settings::Prefs>().sensitivity;
        assert!(right > minus, "right arrow should increase");
        tap(&mut app, KeyCode::KeyD);
        let keyed = app.world().resource::<settings::Prefs>().sensitivity;
        assert!(keyed > right, "D should increase");

        let content = gemini_sim::content::Content::embedded();
        {
            let mut prefs = app.world_mut().resource_mut::<settings::Prefs>();
            prefs.sensitivity = content.ui.settings.sensitivity_min;
        }
        tap(&mut app, KeyCode::ArrowLeft);
        tap(&mut app, KeyCode::KeyA);
        let stuck = app.world().resource::<settings::Prefs>().sensitivity;
        assert!((stuck - content.ui.settings.sensitivity_min).abs() < 1e-4, "min clamp {stuck}");
        {
            let mut prefs = app.world_mut().resource_mut::<settings::Prefs>();
            prefs.sensitivity = content.ui.settings.sensitivity_max;
        }
        tap(&mut app, KeyCode::ArrowRight);
        tap(&mut app, KeyCode::KeyD);
        choose(&mut app, "slider:sensitivity:+");
        let capped = app.world().resource::<settings::Prefs>().sensitivity;
        assert!((capped - content.ui.settings.sensitivity_max).abs() < 1e-4, "max clamp {capped}");
        choose(&mut app, "slider:sensitivity:-");
        let dropped = app.world().resource::<settings::Prefs>().sensitivity;
        assert!(dropped < content.ui.settings.sensitivity_max);
    }

    fn assert_menu_shown(app: &mut App) {
        let cams = menu_cameras(app);
        assert_eq!(cams.len(), 1, "the menu needs its own camera");
        let (cam, clears) = cams[0];
        assert!(clears, "the menu camera must clear, or the last match frame stays frozen");
        let mut nodes = app
            .world_mut()
            .query_filtered::<(&Node, &UiTargetCamera, &Visibility, Option<&ImageNode>), With<flow::MenuRoot>>();
        let mut image = false;
        let mut count = 0;
        for (node, target, visibility, pictured) in nodes.iter(app.world()) {
            count += 1;
            assert_eq!(target.0, cam, "menu UI must target the menu camera");
            assert_ne!(*visibility, Visibility::Hidden);
            let Val::Px(width) = node.width else { panic!("menu node width {:?}", node.width) };
            let Val::Px(height) = node.height else { panic!("menu node height {:?}", node.height) };
            assert!(width > 0.0 && height > 0.0, "menu node is {width}x{height}");
            image |= pictured.is_some();
        }
        assert!(count > 1, "menu UI nodes missing");
        assert!(image, "the painted menu image is missing");
    }

    fn menu_cameras(app: &mut App) -> Vec<(Entity, bool)> {
        let mut query = app
            .world_mut()
            .query_filtered::<(Entity, &Camera), (With<flow::MenuCamera>, With<Camera2d>)>();
        query
            .iter(app.world())
            .map(|(entity, camera)| (entity, !matches!(camera.clear_color, bevy::camera::ClearColorConfig::None)))
            .collect()
    }

    fn camera_count(app: &mut App) -> usize {
        let mut query = app.world_mut().query_filtered::<Entity, With<Camera3d>>();
        query.iter(app.world()).count()
    }

    fn prompt_line(app: &mut App) -> String {
        hud_text(app).into_iter().find(|line| line.contains("HOLD E") || line.contains("REVIVE") || line == "DOWN").unwrap_or_default()
    }

    fn hud_text(app: &mut App) -> Vec<String> {
        let mut query = app.world_mut().query::<&Text>();
        query.iter(app.world()).map(|text| text.0.clone()).collect()
    }

    #[derive(Component)]
    struct OldVoice;

    fn press_hit(app: &mut App, id: &str) {
        let entity = {
            let mut query = app.world_mut().query::<(Entity, &flow::HitButton)>();
            query
                .iter(app.world())
                .find(|(_, hit)| hit.0 == id)
                .map(|(entity, _)| entity)
                .unwrap_or_else(|| panic!("missing button {id}"))
        };
        app.world_mut().trigger(bevy::ui_widgets::Activate { entity });
        app.update();
        app.update();
    }

    fn place_tyrant(app: &mut App) {
        let mut game = app.world_mut().resource_mut::<session::Game>();
        let pos = game.sim.players[0].pos;
        game.sim.players[0].invuln = 0.0;
        game.sim.director.parked = true;
        game.sim.enemies.clear();
        game.sim.projectiles.clear();
        game.sim.enemies.push(gemini_sim::sim::Enemy {
            id: 9001,
            kind: gemini_sim::content::EnemyKind::HiveTyrant,
            pos,
            vel: gemini_sim::math::Vec3::ZERO,
            yaw: 0.0,
            hp: 1000.0,
            max_hp: 1000.0,
            speed: 0.0,
            contact: 0.0,
            range: 0.0,
            weak_mult: 1.0,
            attack_cd: 10.0,
            ai: gemini_sim::sim::AiState::Chase,
            weak_exposed: false,
            open_left: 0.0,
            just_opened: false,
            pods: Vec::new(),
            phase: 0,
            spawn_acc: 0.0,
            spawn_kind: gemini_sim::content::EnemyKind::Swarmer,
            spawn_count: 0,
            spawn_interval: 0.0,
            alive: true,
            anim_time: 0.0,
            cycle_left: 0.0,
            warn_left: 0.0,
            warn_sfx: false,
            warn_pod: 255,
            reserved: 0,
            enraged: false,
            invuln: 0.0,
            tail_hp: 10_000.0,
            tail_broken: false,
            acid_cd: 99.0,
            acid_warn: 0.0,
            scythe_cd: 0.0,
            reinforce_cd: 99.0,
            reinforce_warn: 0.0,
            cue: gemini_sim::sim::EnemyCue::None,
            climbing: false,
            last_hit_by: 0,
        });
    }

    fn join_second(app: &mut App) {
        let content = gemini_sim::content::Content::embedded();
        let mut game = app.world_mut().resource_mut::<session::Game>();
        game.sim.phase = gemini_sim::sim::Phase::Breather { left: 30.0 };
        game.sim.request_join(&content).expect("a second player can join");
        game.sim.phase = gemini_sim::sim::Phase::Combat;
        if let Some(joiner) = game.sim.players.get_mut(1) {
            joiner.pos.x += 80.0;
        }
        assert!(game.sim.connected_count() > 1);
    }

    fn has_pause_veil(app: &mut App) -> bool {
        let alpha = gemini_sim::content::Content::embedded().ui.layout.coop_pause_dim;
        let mut query = app.world_mut().query::<&BackgroundColor>();
        query.iter(app.world()).any(|BackgroundColor(color)| (color.to_srgba().alpha - alpha).abs() < 1e-4)
    }

    fn locked_cursor() -> CursorOptions {
        let mut cursor = CursorOptions::default();
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
        cursor
    }

    fn cursor_state(app: &mut App) -> (CursorGrabMode, bool) {
        let mut query = app.world_mut().query::<&CursorOptions>();
        let cursor = query.single(app.world()).expect("cursor");
        (cursor.grab_mode, cursor.visible)
    }

    fn offer_upgrades(app: &mut App) {
        let mut game = app.world_mut().resource_mut::<session::Game>();
        game.sim.phase = gemini_sim::sim::Phase::Upgrade {
            options: vec!["rapid_cells".into(), "hollow_points".into(), "servo_legs".into()],
        };
    }

    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        // MinimalPlugins does not clear `just_pressed`, so a leftover left press
        // would keep winning over the next right press.
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset(key);
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
