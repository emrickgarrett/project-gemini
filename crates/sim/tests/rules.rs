use std::collections::BTreeMap;

use gemini_sim::audio_mix::{select_voices, SoundEvent};
use gemini_sim::content::{wave_budget, Content};
use gemini_sim::math::Vec3;
use gemini_sim::rng::Rng;
use gemini_sim::upgrade::{roll_offers, stacked_value};
use gemini_sim::{Phase, PlayerInput, Sim};

fn script(tick: u64) -> PlayerInput {
    PlayerInput {
        move_z: 1.0,
        yaw: tick as f32 * 0.02,
        pitch: 0.1,
        fire: tick % 4 == 0,
        jump_pressed: tick % 45 == 3,
        sprint: tick % 80 < 20,
        boost_pressed: tick % 70 == 5,
        boost_held: (tick % 70) > 5 && (tick % 70) < 18,
        ..PlayerInput::default()
    }
}

#[test]
fn tables_drive_the_controller_and_clocks() {
    let content = Content::embedded();
    assert!((content.controller.movement.walk_speed - 6.5).abs() < 1e-4);
    assert!((content.controller.jump.coyote_time - 0.12).abs() < 1e-4);
    assert!((content.controller.jump.jump_buffer - 0.12).abs() < 1e-4);
    assert!((content.controller.boost.thrust_up - 26.0).abs() < 1e-4);
    assert!((content.controller.downed.bleedout_time - 30.0).abs() < 1e-4);
    assert_eq!(content.controller.downed.solo_self_revives_per_wave, 1);
    assert!((content.controller.downed.solo_self_revive_time - 5.0).abs() < 1e-4);
    assert!((content.controller.downed.solo_self_revive_hp_fraction - 0.5).abs() < 1e-4);
    assert!((content.combat.spitter_min_range - 6.0).abs() < 1e-4);
    assert_eq!(content.combat.keep_away_ratios, [0.55, 0.35]);
    assert!(!content.controller.aim.mouse_aim_assist);
    assert_eq!(content.waves.budget.live_enemy_cap, 300);
    assert!((content.waves.budget.queue_cap_waves - 1.0).abs() < 1e-4);
    let normal = wave_budget(&content.waves.budget, 1, 1, &content.endless, false);
    assert!((normal - 20.0).abs() < 1e-3);
    let wave2 = wave_budget(&content.waves.budget, 2, 1, &content.endless, false);
    assert!((wave2 - 20.0 * 1.25 * 1.03).abs() < 1e-3);
    let duo = wave_budget(&content.waves.budget, 2, 2, &content.endless, false);
    assert!((duo - wave2 * 1.6).abs() < 1e-2);
}

#[test]
fn jump_reads_controller() {
    let mut content = Content::embedded();
    content.controller.jump.jump_velocity = 30.0;
    let mut sim = Sim::new(&content, "normal", 3);
    let mut input = PlayerInput::default();
    input.jump_pressed = true;
    sim.step(&content, input);
    assert!((sim.players[0].vel.y - 30.0).abs() < 0.05, "{}", sim.players[0].vel.y);
}

#[test]
fn coyote_and_buffer_come_from_the_table() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 1);
    sim.players[0].grounded = false;
    sim.players[0].coyote = content.controller.jump.coyote_time;
    sim.players[0].pos.y = 2.0;
    let mut input = PlayerInput::default();
    input.jump_pressed = true;
    sim.step(&content, input);
    assert!(
        (sim.players[0].vel.y - content.controller.jump.jump_velocity).abs() < 0.05,
        "coyote {}",
        sim.players[0].vel.y
    );

    let mut sim = Sim::new(&content, "normal", 1);
    sim.players[0].grounded = false;
    sim.players[0].coyote = 0.0;
    sim.players[0].pos.y = 0.04;
    sim.players[0].vel.y = 0.0;
    input.jump_pressed = true;
    sim.step(&content, input);
    input.jump_pressed = false;
    for _ in 0..6 {
        sim.step(&content, input);
    }
    assert!(
        sim.players[0].pos.y > 0.2,
        "buffered jump should have left the floor, y={}",
        sim.players[0].pos.y
    );
}

#[test]
fn sprint_blocks_primary_fire() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 1);
    let before = sim.players[0].primary.ammo;
    let input = PlayerInput {
        move_z: 1.0,
        sprint: true,
        fire: true,
        ..PlayerInput::default()
    };
    for _ in 0..10 {
        sim.step(&content, input);
    }
    assert_eq!(sim.players[0].primary.ammo, before);
}

#[test]
fn upgrade_stacks_match_the_rules() {
    let content = Content::embedded();
    let rapid = content.upgrade("rapid_cells").unwrap();
    let expect = 0.6 * (1.0 - 1.0 / (1.0 + 0.12));
    assert!((stacked_value(rapid, 1) - expect).abs() < 1e-5);
    let hollow = content.upgrade("hollow_points").unwrap();
    assert!((stacked_value(hollow, 2) - 0.24).abs() < 1e-5);
    assert!((stacked_value(hollow, 100) - 1.2).abs() < 1e-4);
    let last = content.upgrade("last_stand").unwrap();
    assert_eq!(stacked_value(last, 0), 0.0);
    assert_eq!(stacked_value(last, 1), 1.0);
    let mut owned = BTreeMap::new();
    owned.insert("last_stand".into(), 1);
    owned.insert("plating".into(), 10);
    let offers = roll_offers(&mut Rng::new(4), &content, &owned, 8);
    assert!(!offers.iter().any(|id| id == "last_stand"));
    assert!(!offers.iter().any(|id| id == "plating"));
    assert!(offers.len() >= 3);
}

#[test]
fn spawner_respects_cap_and_one_wave_queue() {
    let mut content = Content::embedded();
    content.waves.budget.live_enemy_cap = 5;
    content.waves.budget.base = 80.0;
    content.combat.spawn_interval = 0.01;
    content.combat.group_budget = 40.0;
    let mut sim = Sim::new(&content, "normal", 2);
    for _ in 0..30 {
        sim.step(&content, PlayerInput::default());
    }
    assert!(sim.enemies.len() <= 5, "alive {}", sim.enemies.len());
    assert!(sim.director.parked, "budget should park at the cap");
    assert!((sim.director.queue - 75.0).abs() < 0.6, "queue {}", sim.director.queue);
    assert!(sim.director.dropped < 0.1);

    for enemy in &mut sim.enemies {
        enemy.hp = 0.0;
    }
    sim.step(&content, PlayerInput::default());
    assert!(matches!(sim.phase, Phase::Upgrade { .. }));
    sim.step(
        &content,
        PlayerInput {
            upgrade_choice: Some(0),
            ..PlayerInput::default()
        },
    );
    if let Phase::Breather { left } = &mut sim.phase {
        *left = 0.01;
    }
    sim.step(&content, PlayerInput::default());
    assert_eq!(sim.director.wave, 2);
    for _ in 0..20 {
        sim.step(&content, PlayerInput::default());
    }
    assert!(sim.enemies.len() <= 5);
    assert!(sim.director.parked);
    let cap = sim.director.wave_budget;
    assert!((sim.director.queue - cap).abs() < 0.6, "queue {} cap {}", sim.director.queue, cap);
    assert!(sim.director.dropped > 1.0, "excess should drop");
}

#[test]
fn difficulty_scales_spawned_hp() {
    let content = Content::embedded();
    let mut normal = Sim::new(&content, "normal", 5);
    let mut heroic = Sim::new(&content, "heroic", 5);
    for _ in 0..40 {
        normal.step(&content, PlayerInput::default());
        heroic.step(&content, PlayerInput::default());
    }
    let n = normal.enemies.iter().find(|e| e.kind == gemini_sim::content::EnemyKind::Swarmer).unwrap();
    let h = heroic.enemies.iter().find(|e| e.kind == gemini_sim::content::EnemyKind::Swarmer).unwrap();
    assert!((n.max_hp - 30.0).abs() < 0.1, "{}", n.max_hp);
    assert!((h.max_hp - 75.0).abs() < 0.1, "{}", h.max_hp);
}

#[test]
fn snapshot_round_trips_and_resumes() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "hard", 11);
    for tick in 0..80 {
        sim.step(&content, script(tick));
    }
    let bytes = sim.to_bytes().unwrap();
    let restored = Sim::restore(&content, &bytes).unwrap();
    assert_eq!(restored.to_bytes().unwrap(), bytes);
    let mut resumed = restored;
    let mut live = Sim::restore(&content, &bytes).unwrap();
    for tick in 80..160 {
        let input = script(tick);
        resumed.step(&content, input);
        live.step(&content, input);
    }
    assert_eq!(resumed.to_bytes().unwrap(), live.to_bytes().unwrap());
}

#[test]
fn fixed_seed_is_deterministic() {
    let content = Content::embedded();
    let mut a = Sim::new(&content, "legendary", 42);
    let mut b = Sim::new(&content, "legendary", 42);
    for tick in 0..150 {
        let input = script(tick);
        a.step(&content, input);
        b.step(&content, input);
    }
    assert_eq!(a.to_bytes().unwrap(), b.to_bytes().unwrap());
    b.step(&content, PlayerInput { yaw: 1.0, ..script(150) });
    a.step(&content, PlayerInput { yaw: 2.0, ..script(150) });
    assert_ne!(a.to_bytes().unwrap(), b.to_bytes().unwrap());
}

#[test]
fn solo_self_revive_once_per_wave() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 1);
    sim.enemies.clear();
    sim.players[0].hp = 5.0;
    sim.players[0].invuln = 0.0;
    hurt(&mut sim, &content);
    assert!(sim.players[0].downed);
    assert!(matches!(sim.phase, Phase::Combat));
    let sidearm_before = sim.players[0].sidearm.ammo;
    sim.enemies.clear();
    let input = PlayerInput {
        interact_held: true,
        fire: true,
        ..PlayerInput::default()
    };
    let ticks = (content.controller.downed.solo_self_revive_time * 30.0).ceil() as i32 + 2;
    for _ in 0..ticks {
        sim.step(&content, input);
    }
    assert!(!sim.players[0].downed, "hold should self-revive");
    assert!(sim.players[0].alive);
    let frac = content.controller.downed.solo_self_revive_hp_fraction;
    assert!((sim.players[0].hp - sim.players[0].max_hp * frac).abs() < 0.05);
    assert!(sim.players[0].sidearm.ammo < sidearm_before);
    assert_eq!(sim.players[0].self_revives_used, 1);

    sim.players[0].invuln = 0.0;
    sim.players[0].hp = 5.0;
    hurt(&mut sim, &content);
    assert!(matches!(sim.phase, Phase::Defeat));
    assert!(!sim.players[0].alive);
}

#[test]
fn self_revive_hp_and_spitter_standoff_come_from_content() {
    use gemini_sim::sim::AiState;

    let mut content = Content::embedded();
    content.controller.downed.solo_self_revive_hp_fraction = 0.25;
    let mut sim = Sim::new(&content, "normal", 3);
    sim.enemies.clear();
    sim.director.pool = 1.0;
    sim.director.parked = true;
    sim.players[0].hp = 5.0;
    sim.players[0].invuln = 0.0;
    hurt(&mut sim, &content);
    sim.enemies.clear();
    sim.director.pool = 1.0;
    sim.director.parked = true;
    let input = PlayerInput {
        interact_held: true,
        ..PlayerInput::default()
    };
    let ticks = (content.controller.downed.solo_self_revive_time * 30.0).ceil() as i32 + 2;
    for _ in 0..ticks {
        sim.step(&content, input);
    }
    assert!(!sim.players[0].downed);
    assert!((sim.players[0].hp - sim.players[0].max_hp * 0.25).abs() < 0.05);

    content.combat.spitter_min_range = 20.0;
    content.combat.keep_away_ratios = [0.6, 0.5];
    let mut sim = Sim::new(&content, "normal", 4);
    sim.enemies.clear();
    sim.director.pool = 0.0;
    sim.director.parked = true;
    sim.enemies.push(spitter_at(8.0));
    sim.step(&content, PlayerInput::default());
    let back = sim.enemies.iter().find(|e| e.id == 42).unwrap();
    assert!(back.pos.z > 8.0, "inside the back-away ratio, z moved to {}", back.pos.z);
    assert!(matches!(back.ai, AiState::Chase), "8 < 20 * 0.5, so it must not spit");

    content.combat.keep_away_ratios = [0.5, 0.2];
    let mut sim = Sim::new(&content, "normal", 5);
    sim.enemies.clear();
    sim.director.pool = 0.0;
    sim.director.parked = true;
    sim.enemies.push(spitter_at(8.0));
    sim.step(&content, PlayerInput::default());
    let firing = sim.enemies.iter().find(|e| e.id == 42).unwrap();
    assert!(matches!(firing.ai, AiState::Telegraph { .. }));

    content.combat.spitter_min_range = 4.0;
    content.combat.keep_away_ratios = [0.2, 0.1];
    let mut sim = Sim::new(&content, "normal", 6);
    sim.enemies.clear();
    sim.director.pool = 0.0;
    sim.director.parked = true;
    sim.enemies.push(spitter_at(8.0));
    let before = 8.0;
    sim.step(&content, PlayerInput::default());
    let close = sim.enemies.iter().find(|e| e.id == 42).unwrap();
    assert!(close.pos.z < before, "outside keep-away, it should close, z={}", close.pos.z);
}

fn spitter_at(z: f32) -> gemini_sim::sim::Enemy {
    use gemini_sim::content::EnemyKind;
    use gemini_sim::sim::AiState;
    gemini_sim::sim::Enemy {
        id: 42,
        kind: EnemyKind::Spitter,
        pos: Vec3::new(0.0, 0.0, z),
        vel: Vec3::ZERO,
        yaw: 0.0,
        hp: 90.0,
        max_hp: 90.0,
        speed: 4.0,
        contact: 8.0,
        range: 4.0,
        weak_mult: 2.0,
        attack_cd: 0.0,
        ai: AiState::Chase,
        weak_exposed: true,
        phase: 0,
        spawn_acc: 0.0,
        spawn_kind: EnemyKind::Spitter,
        spawn_count: 0,
        spawn_interval: 0.0,
        alive: true,
        open_left: 0.0,
        just_opened: false,
        pods: Vec::new(),
        anim_time: 0.0,
        cycle_left: 0.0,
        warn_left: 0.0,
        warn_sfx: false,
        warn_pod: 255,
        reserved: 0,
        enraged: false,
        invuln: 0.0,
        tail_hp: 0.0,
        tail_broken: false,
        acid_cd: 0.0,
        acid_warn: 0.0,
        scythe_cd: 0.0,
        reinforce_cd: 0.0,
        reinforce_warn: 0.0,
        cue: gemini_sim::sim::EnemyCue::None,
    }
}

fn hurt(sim: &mut Sim, content: &Content) {
    use gemini_sim::content::EnemyKind;
    use gemini_sim::sim::AiState;
    sim.enemies.push(gemini_sim::sim::Enemy {
        id: 9_000,
        kind: EnemyKind::Swarmer,
        pos: sim.players[0].pos,
        vel: Vec3::ZERO,
        yaw: 0.0,
        hp: 30.0,
        max_hp: 30.0,
        speed: 0.0,
        contact: 500.0,
        range: 1.0,
        weak_mult: 2.0,
        attack_cd: 0.0,
        ai: AiState::Chase,
        weak_exposed: true,
        phase: 0,
        spawn_acc: 0.0,
        spawn_kind: EnemyKind::Swarmer,
        spawn_count: 0,
        spawn_interval: 0.0,
        alive: true,
        open_left: 0.0,
        just_opened: false,
        pods: Vec::new(),
        anim_time: 0.0,
        cycle_left: 0.0,
        warn_left: 0.0,
        warn_sfx: false,
        warn_pod: 255,
        reserved: 0,
        enraged: false,
        invuln: 0.0,
        tail_hp: 0.0,
        tail_broken: false,
        acid_cd: 0.0,
        acid_warn: 0.0,
        scythe_cd: 0.0,
        reinforce_cd: 0.0,
        reinforce_warn: 0.0,
        cue: gemini_sim::sim::EnemyCue::None,
    });
    sim.step(content, PlayerInput::default());
}

#[test]
fn mixer_caps_voices_and_keeps_the_charge_cue() {
    let audio = &Content::embedded().audio;
    let mut events = Vec::new();
    for i in 0..40 {
        events.push(SoundEvent {
            id: "vex_fire".into(),
            pos: Vec3::new(i as f32, 0.0, 0.0),
            owner: 0,
        });
    }
    events.push(SoundEvent {
        id: "spitter_charge".into(),
        pos: Vec3::new(80.0, 0.0, 0.0),
        owner: 1,
    });
    for i in 0..25 {
        events.push(SoundEvent {
            id: "swarmer_crowd".into(),
            pos: Vec3::new(0.2 * i as f32, 0.0, 1.0),
            owner: 0,
        });
    }
    let voices = select_voices(audio, &events, Vec3::ZERO);
    let crowds = voices.iter().filter(|v| v.crowd).count();
    assert_eq!(crowds, 1);
    assert!(voices.iter().any(|v| v.id == "spitter_charge" && v.critical));
    let non_critical = voices.iter().filter(|v| !v.critical).count();
    assert!(non_critical <= audio.voice_cap);
}

#[test]
fn spatial_grid_finds_neighbors_in_order() {
    use gemini_sim::spatial::SpatialGrid;
    let mut grid = SpatialGrid::new(4.0, 32.0);
    let points = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(10.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.5),
    ];
    grid.rebuild(&points, |p| *p);
    let hit = grid.query_radius(&points, 0.0, 0.0, 2.0, |p| *p);
    assert_eq!(hit, vec![0, 2]);
}

fn bug(kind: gemini_sim::content::EnemyKind) -> gemini_sim::sim::Enemy {
    use gemini_sim::content::EnemyKind;
    use gemini_sim::sim::{AiState, Enemy};
    Enemy {
        id: 40,
        kind,
        pos: Vec3::ZERO,
        vel: Vec3::ZERO,
        yaw: 0.0,
        hp: 500.0,
        max_hp: 500.0,
        speed: 0.0,
        contact: 1.0,
        range: 20.0,
        weak_mult: 2.0,
        attack_cd: 0.0,
        ai: AiState::Chase,
        weak_exposed: kind != EnemyKind::Brute,
        phase: 0,
        spawn_acc: 0.0,
        spawn_kind: EnemyKind::Swarmer,
        spawn_count: 0,
        spawn_interval: 0.0,
        alive: true,
        open_left: 0.0,
        just_opened: false,
        pods: Vec::new(),
        anim_time: 0.0,
        cycle_left: 0.0,
        warn_left: 0.0,
        warn_sfx: false,
        warn_pod: 255,
        reserved: 0,
        enraged: false,
        invuln: 0.0,
        tail_hp: 0.0,
        tail_broken: false,
        acid_cd: 0.0,
        acid_warn: 0.0,
        scythe_cd: 0.0,
        reinforce_cd: 0.0,
        reinforce_warn: 0.0,
        cue: gemini_sim::sim::EnemyCue::None,
    }
}

fn coil(pos: Vec3, vel: Vec3, damage: f32) -> gemini_sim::sim::Projectile {
    use gemini_sim::sim::{Projectile, ShotKind, Team};
    Projectile {
        id: 7,
        pos,
        vel,
        damage,
        radius: 0.08,
        team: Team::Player,
        ttl: 2.0,
        pierce: 0,
        bounces: 0,
        hit: Vec::new(),
        kind: ShotKind::Coil,
        acid_ward: false,
        owner: 1,
        crit_mult: 1.0,
    }
}

#[test]
fn bugs_table_owns_hitboxes_and_mix() {
    let content = Content::embedded();
    assert_eq!(content.palette.keys.brood_sac, "#5A6B2A");
    assert!((content.bug(gemini_sim::content::EnemyKind::Swarmer).hitbox.weak().unwrap().mult - 1.5).abs() < 1e-4);
    assert!(content.bug(gemini_sim::content::EnemyKind::Spitter).hitbox.sac_pop_cancels_shot);
    assert!((content.bug(gemini_sim::content::EnemyKind::Spitter).hitbox.charge_seconds - 1.0).abs() < 1e-4);
    assert!((content.bug(gemini_sim::content::EnemyKind::Brute).hitbox.armor_mult_when_closed - 0.5).abs() < 1e-4);
    assert_eq!(content.bug(gemini_sim::content::EnemyKind::Brute).audio.shell_open, "brute_shell_open");
    assert_eq!(content.bugs.feedback.weakpoint_hit, "weakpoint_hit");
    let shot = content.sfx("vex_fire").unwrap();
    assert!(shot.volume > 0.0);
    assert!(content.pitch_range("vex_fire").is_some());
    assert!(content.bug(gemini_sim::content::EnemyKind::Swarmer).visual.height < 0.7);
}

#[test]
fn spitter_sac_pop_cancels_charge_same_tick() {
    use gemini_sim::content::EnemyKind;
    use gemini_sim::sim::AiState;
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 4);
    sim.enemies.clear();
    let mut spit = bug(EnemyKind::Spitter);
    spit.id = 11;
    spit.ai = AiState::Telegraph { left: 0.8 };
    spit.cue = gemini_sim::sim::EnemyCue::Charge;
    spit.weak_mult = content.bug(EnemyKind::Spitter).hitbox.weak().unwrap().mult;
    let book = content.bug(EnemyKind::Spitter);
    let sac = book.hitbox.weak().unwrap().offset;
    sim.enemies.push(spit);
    sim.projectiles.push(coil(
        Vec3::new(sac[0], sac[1], sac[2] - 1.5),
        Vec3::new(0.0, 0.0, 90.0),
        15.0,
    ));
    let before = sim.enemies[0].hp;
    sim.step(&content, PlayerInput::default());
    assert!(matches!(sim.enemies[0].ai, AiState::Chase), "sac hit cancels the spit");
    assert!(sim.enemies[0].hp < before);
    let pop = &content.bug(EnemyKind::Spitter).audio.sac_pop;
    let charge = &content.bug(EnemyKind::Spitter).audio.charge;
    assert!(sim.sounds.iter().any(|s| s.id == *pop && s.owner == 11));
    assert!(sim.stops.iter().any(|s| s.id == *charge && s.owner == 11));
    assert!(sim.pings.iter().any(|p| p.weak));
}

#[test]
fn sim_step_cap_comes_from_controller() {
    let content = Content::embedded();
    assert_eq!(content.controller.sim.max_sim_steps_per_frame, 5);
    let base = gemini_sim::content::embedded_sources();
    let load = |controller: &str| {
        Content::parse(&gemini_sim::content::Sources {
            waves: base.waves,
            difficulty: base.difficulty,
            controller,
            run_upgrades: base.run_upgrades,
            endless: base.endless,
            currency: base.currency,
            banners: base.banners,
            duplicate_ranks: base.duplicate_ranks,
            bosses: base.bosses,
            weapons: base.weapons,
            hero: base.hero,
            arena: base.arena,
            presentation: base.presentation,
            combat: base.combat,
            palette: base.palette,
            music: base.music,
            audio: base.audio,
            bugs: base.bugs,
            crate_reveal: base.crate_reveal,
        })
    };
    let missing = base
        .controller
        .lines()
        .filter(|line| !line.contains("max_sim_steps_per_frame"))
        .collect::<Vec<_>>()
        .join("\n");
    let err = load(&missing).expect_err("missing max_sim_steps_per_frame");
    assert!(err.contains("max_sim_steps_per_frame"), "{err}");
    let zero = base.controller.replace("max_sim_steps_per_frame = 5", "max_sim_steps_per_frame = 0");
    let err = load(&zero).expect_err("a zero step cap must fail load");
    assert!(err.contains("max_sim_steps_per_frame"), "{err}");
}

#[test]
fn hatch_fan_and_spawn_radius_come_from_bosses() {
    let content = Content::embedded();
    let brood = content.boss("brood_mother").unwrap();
    let hive = content.boss("hive_tyrant").unwrap();
    assert!((brood.fan_step - 1.7).abs() < 1e-4);
    assert!((hive.fan_step - 1.7).abs() < 1e-4);
    assert!((brood.spawn_radius - 2.8).abs() < 1e-4, "brood {}", brood.spawn_radius);
    assert!((hive.spawn_radius - 4.0).abs() < 1e-4, "hive {}", hive.spawn_radius);
    let base = gemini_sim::content::embedded_sources();
    let bosses = include_str!("../../../content/bosses.toml")
        .lines()
        .filter(|line| !line.trim_start().starts_with("hatch_fan_step"))
        .collect::<Vec<_>>()
        .join("\n");
    let err = Content::parse(&gemini_sim::content::Sources {
        waves: base.waves,
        difficulty: base.difficulty,
        controller: base.controller,
        run_upgrades: base.run_upgrades,
        endless: base.endless,
        currency: base.currency,
        banners: base.banners,
        duplicate_ranks: base.duplicate_ranks,
        bosses: &bosses,
        weapons: base.weapons,
        hero: base.hero,
        arena: base.arena,
        presentation: base.presentation,
        combat: base.combat,
        palette: base.palette,
        music: base.music,
        audio: base.audio,
        bugs: base.bugs,
        crate_reveal: base.crate_reveal,
    })
    .expect_err("missing hatch_fan_step");
    assert!(err.contains("hatch_fan_step"), "{err}");

    let tight = include_str!("../../../content/bosses.toml").replacen("spawn_radius = 2.8", "spawn_radius = 1.9", 1);
    let err = Content::parse(&gemini_sim::content::Sources {
        waves: base.waves,
        difficulty: base.difficulty,
        controller: base.controller,
        run_upgrades: base.run_upgrades,
        endless: base.endless,
        currency: base.currency,
        banners: base.banners,
        duplicate_ranks: base.duplicate_ranks,
        bosses: &tight,
        weapons: base.weapons,
        hero: base.hero,
        arena: base.arena,
        presentation: base.presentation,
        combat: base.combat,
        palette: base.palette,
        music: base.music,
        audio: base.audio,
        bugs: base.bugs,
        crate_reveal: base.crate_reveal,
    })
    .expect_err("spawn_radius inside the body");
    assert!(err.contains("spawn_radius"), "{err}");
    assert!(err.contains("body radius"), "{err}");
}

#[test]
fn brute_shell_and_hive_tail_follow_the_table() {
    use gemini_sim::content::EnemyKind;
    use gemini_sim::sim::AiState;
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 5);
    sim.enemies.clear();
    let mut brute = bug(EnemyKind::Brute);
    brute.id = 12;
    brute.weak_mult = 2.5;
    brute.weak_exposed = false;
    sim.enemies.push(brute);
    let body = content.bug(EnemyKind::Brute).hitbox.body.offset;
    sim.projectiles.push(coil(
        Vec3::new(body[0], body[1], body[2] + 3.0),
        Vec3::new(0.0, 0.0, -80.0),
        20.0,
    ));
    sim.step(&content, PlayerInput::default());
    let dropped = 500.0 - sim.enemies[0].hp;
    assert!((dropped - 10.0).abs() < 0.2, "closed shell is 0.5x, dropped {dropped}");

    sim.enemies.clear();
    sim.projectiles.clear();
    let mut brute = bug(EnemyKind::Brute);
    brute.id = 13;
    brute.weak_mult = 2.5;
    brute.ai = AiState::Lunge { left: 0.01 };
    sim.enemies.push(brute);
    sim.step(&content, PlayerInput::default());
    assert!(sim.enemies[0].open_left > 1.5, "cracks stay open for 2s");
    assert!(sim.enemies[0].weak_exposed);
    assert!(sim.sounds.iter().any(|s| s.id == "brute_shell_open"));

    sim.enemies[0].hp = 500.0;
    let book = content.bug(EnemyKind::Brute);
    let cracks = book.hitbox.weak().unwrap().offset;
    sim.projectiles.push(coil(
        Vec3::new(cracks[0], cracks[1] + 2.2, cracks[2]),
        Vec3::new(0.0, -140.0, 0.0),
        10.0,
    ));
    sim.step(&content, PlayerInput::default());
    let weak_drop = 500.0 - sim.enemies[0].hp;
    assert!(weak_drop > 20.0, "open cracks are 2.5x, dropped {weak_drop}");

    sim.enemies.clear();
    sim.projectiles.clear();
    let mut hive = bug(EnemyKind::HiveTyrant);
    hive.id = 14;
    hive.phase = 0;
    let book = content.bug(EnemyKind::HiveTyrant);
    let cannon = book.hitbox.cannon.as_ref().expect("hive cannon");
    let tail = cannon.offset;
    sim.enemies.push(hive);
    sim.projectiles.push(coil(
        Vec3::new(tail[0] - 2.0, tail[1], tail[2]),
        Vec3::new(40.0, 0.0, 0.0),
        10.0,
    ));
    sim.step(&content, PlayerInput::default());
    assert!(sim.enemies[0].hp < 500.0, "phase 1 tail is live");

    sim.enemies[0].hp = 100.0;
    sim.enemies[0].max_hp = 500.0;
    sim.projectiles.push(coil(
        Vec3::new(tail[0] - 2.0, tail[1], tail[2]),
        Vec3::new(40.0, 0.0, 0.0),
        10.0,
    ));
    sim.step(&content, PlayerInput::default());
    assert!((sim.enemies[0].hp - 100.0).abs() < 0.01, "tail is gone after phase 1");
}

#[test]
fn weakpoint_hitboxes_overlap_their_glowing_parts() {
    use gemini_sim::bugs::{glow_bounds, hit_bounds, visual_anchor};
    use gemini_sim::content::EnemyKind;
    let content = Content::embedded();
    let brood = &content.bug(EnemyKind::BroodMother).visual;
    assert_eq!(brood.bud_color, "brood_sac");
    assert_eq!(brood.grown_color, "bud_glow");

    let mut saw = 0;
    for (bug, row) in content.bugs.rows() {
        for (slot, shape) in row.hitbox.anchored() {
            saw += 1;
            let name = shape.anchor.as_str();
            let anchor = visual_anchor(&row.visual, name)
                .unwrap_or_else(|| panic!("{bug} {slot} anchor {name:?} did not resolve after load"));
            assert_eq!(
                shape.offset, anchor,
                "{bug} {slot} was not baked from its visual anchor"
            );
            let glow = glow_bounds(&row.visual, name)
                .unwrap_or_else(|| panic!("{bug} {slot} anchor {name} has no glowing volume"));
            let hit = hit_bounds(shape);
            assert!(
                hit.overlaps(glow),
                "{bug} {slot} hitbox {hit:?} does not overlap its glowing part {glow:?}"
            );
            if let Some(part) = row.visual.parts.iter().find(|part| part.name == name) {
                if let Some(phase) = part.phase_max {
                    assert_eq!(shape.active_phase, phase, "{bug} {slot} phase");
                }
            }
        }
    }
    assert!(saw >= 6, "expected every glowing weak point, saw {saw}");
}

#[test]
fn missing_weakpoint_anchor_fails_content_load() {
    let base = gemini_sim::content::embedded_sources();
    let bugs = include_str!("../../../content/bugs.toml");
    let load = |bugs: &str| {
        Content::parse(&gemini_sim::content::Sources {
            waves: base.waves,
            difficulty: base.difficulty,
            controller: base.controller,
            run_upgrades: base.run_upgrades,
            endless: base.endless,
            currency: base.currency,
            banners: base.banners,
            duplicate_ranks: base.duplicate_ranks,
            bosses: base.bosses,
            weapons: base.weapons,
            hero: base.hero,
            arena: base.arena,
            presentation: base.presentation,
            combat: base.combat,
            palette: base.palette,
            music: base.music,
            audio: base.audio,
            bugs,
            crate_reveal: base.crate_reveal,
        })
    };

    let misspelled = bugs.replacen("anchor = \"abdomen\"", "anchor = \"abdomenn\"", 1);
    let err = load(&misspelled).expect_err("a misspelled anchor must fail load");
    assert!(err.contains("abdomenn"), "{err}");
    assert!(err.contains("does not resolve"), "{err}");

    let missing = bugs.replacen("anchor = \"abdomen\"", "anchor = \"\"", 1);
    let missing = missing.replacen("name = \"abdomen\"", "name = \"\"", 1);
    let err = load(&missing).expect_err("a weak point with no name must fail load");
    assert!(err.contains("no anchor name"), "{err}");

    let authored = bugs.replacen(
        "anchor = \"abdomen\", shape = \"sphere\", radius = 0.15",
        "anchor = \"abdomen\", shape = \"sphere\", radius = 0.15, offset = [0, 0.35, -0.25]",
        1,
    );
    let err = load(&authored).expect_err("an authored weak-point offset must fail load");
    assert!(err.contains("must not author an offset"), "{err}");
}

#[test]
fn crate_reveal_reads_tease_and_fakeout() {
    let reveal = &Content::embedded().crate_reveal;
    assert!(reveal.fakeout_enabled);
    for (id, tease) in [
        ("common", "sfx/crate_tease.ogg"),
        ("rare", "sfx/crate_tease.ogg"),
        ("epic", "sfx/crate_tease.ogg"),
        ("legendary", "sfx/crate_tease_legendary.ogg"),
    ] {
        assert_eq!(reveal.tease(id), Some(tease), "{id}");
        let tier = reveal.tier(id).unwrap();
        assert_eq!(tier.fakeout_window, [2.6, 3.0], "{id}");
    }
    assert!((reveal.tier("rare").unwrap().fakeout_chance - 0.15).abs() < 1e-4);
    assert!((reveal.tier("epic").unwrap().fakeout_chance - 0.2).abs() < 1e-4);
    assert_eq!(reveal.tier("common").unwrap().fakeout_chance, 0.0);
    assert!(reveal.fakeout_open("rare", 2.8));
    assert!(reveal.fakeout_open("epic", 2.7));
    assert!(!reveal.fakeout_open("common", 2.8));
    assert!(!reveal.fakeout_open("legendary", 2.8));
}

#[test]
fn every_emitted_sound_resolves_to_a_mix_file() {
    let content = Content::embedded();
    let mut ids = vec![
        "wave".to_string(),
        format!("{}_fire", content.hero.id),
        format!("{}_ability", content.hero.id),
        format!("{}_ability_end", content.hero.id),
        "hit".into(),
        "hurt".into(),
        "downed".into(),
        "pickup".into(),
        format!("{}_reload", content.hero.id),
        "jump".into(),
        "dash".into(),
        "swarmer_crowd".into(),
        content.bugs.feedback.weakpoint_hit.clone(),
    ];
    for (_, row) in content.bugs.rows() {
        for id in [
            &row.audio.footstep,
            &row.audio.attack,
            &row.audio.charge,
            &row.audio.fire,
            &row.audio.sac_pop,
            &row.audio.shell_open,
        ] {
            if !id.is_empty() {
                ids.push(id.clone());
            }
        }
    }
    for boss in &content.bosses {
        for id in [
            &boss.intro_sfx,
            &boss.death_sfx,
            &boss.fizzle_sfx,
            &boss.charge_sfx,
            &boss.hatch_sfx,
            &boss.acid_sfx,
            &boss.scythe_sfx,
        ] {
            if !id.is_empty() {
                ids.push(id.clone());
            }
        }
    }
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    for id in ids {
        let sfx = content.sfx(&id).unwrap_or_else(|| panic!("{id} does not resolve in audio_mix.toml"));
        let path = root.join(&sfx.path);
        assert!(path.is_file(), "{id} resolved to {} but that file is missing", path.display());
    }
}

#[test]
fn leg_layout_comes_from_bugs() {
    use gemini_sim::content::EnemyKind;
    let content = Content::embedded();
    let leg = |kind| {
        content
            .bug(kind)
            .visual
            .parts
            .iter()
            .find(|part| part.name == "leg")
            .unwrap_or_else(|| panic!("{kind:?} has no leg"))
    };
    for kind in [EnemyKind::Swarmer, EnemyKind::Spitter, EnemyKind::Brute] {
        assert_eq!(leg(kind).layout, "paired", "{kind:?}");
    }
    for kind in [EnemyKind::BroodMother, EnemyKind::HiveTyrant] {
        let part = leg(kind);
        assert_eq!(part.layout, "radial", "{kind:?}");
        assert!((part.hip_scale.expect("hip_scale") - 0.95).abs() < 1e-4, "{kind:?}");
        assert!((part.plant_scale.expect("plant_scale") - 1.55).abs() < 1e-4, "{kind:?}");
        assert!((part.ring_phase.expect("ring_phase") - 0.35).abs() < 1e-4, "{kind:?}");
    }

    let base = gemini_sim::content::embedded_sources();
    let bugs = include_str!("../../../content/bugs.toml").replacen("layout=\"paired\", ", "", 1);
    let err = Content::parse(&gemini_sim::content::Sources {
        waves: base.waves,
        difficulty: base.difficulty,
        controller: base.controller,
        run_upgrades: base.run_upgrades,
        endless: base.endless,
        currency: base.currency,
        banners: base.banners,
        duplicate_ranks: base.duplicate_ranks,
        bosses: base.bosses,
        weapons: base.weapons,
        hero: base.hero,
        arena: base.arena,
        presentation: base.presentation,
        combat: base.combat,
        palette: base.palette,
        music: base.music,
        audio: base.audio,
        bugs: &bugs,
        crate_reveal: base.crate_reveal,
    })
    .expect_err("missing layout");
    assert!(err.contains("layout"), "{err}");
    assert!(err.contains("swarmer"), "{err}");
}

#[test]
fn vex_overdrive_comes_from_characters() {
    let content = Content::embedded();
    assert_eq!(content.hero.id, "vex");
    assert_eq!(content.hero.primary, "pulse_rifle");
    assert_eq!(content.hero.sidearm, "sidearm");
    assert_eq!(content.hero.accent, "#F97316");
    assert!((content.hero.max_hp - 100.0).abs() < 1e-4);
    assert!((content.hero.move_speed_mult - 1.0).abs() < 1e-4);
    assert!((content.hero.ability_cooldown - 20.0).abs() < 1e-4);
    assert!((content.hero.overdrive_duration - 6.0).abs() < 1e-4);
    assert!((content.hero.overdrive_fire_rate - 0.35).abs() < 1e-4);
    assert!(content.hero.infinite_magazine);
    assert!((content.hero.kill_cd_refund - 1.0).abs() < 1e-4);
    assert!(content.weapon("coil_rifle").is_none());
    let gun = content.weapon("pulse_rifle").unwrap();
    assert!((gun.fire_interval - 1.0 / 9.0).abs() < 1e-4);
    assert_eq!(gun.magazine, 36);
    assert_eq!(gun.reserve, 180);
    assert!((gun.projectile_speed - 88.0).abs() < 1e-4);
    assert!((gun.projectile_radius - 0.08).abs() < 1e-4);
    assert!((gun.spread - 1.2_f32.to_radians()).abs() < 1e-4);
    assert!((content.weapons.sprint_to_fire_delay - 0.15).abs() < 1e-4);

    let base = gemini_sim::content::embedded_sources();
    let hero = include_str!("../../../content/characters.toml").replacen("duration = 6.0, ", "", 1);
    let err = Content::parse(&gemini_sim::content::Sources {
        waves: base.waves,
        difficulty: base.difficulty,
        controller: base.controller,
        run_upgrades: base.run_upgrades,
        endless: base.endless,
        currency: base.currency,
        banners: base.banners,
        duplicate_ranks: base.duplicate_ranks,
        bosses: base.bosses,
        weapons: base.weapons,
        hero: &hero,
        arena: base.arena,
        presentation: base.presentation,
        combat: base.combat,
        palette: base.palette,
        music: base.music,
        audio: base.audio,
        bugs: base.bugs,
        crate_reveal: base.crate_reveal,
    })
    .expect_err("missing overdrive duration");
    assert!(err.contains("duration"), "{err}");

    let mut sim = Sim::new(&content, "normal", 2);
    assert!((sim.players[0].max_hp - content.hero.max_hp).abs() < 1e-3);
    let ammo = sim.players[0].primary.ammo;
    let mut input = PlayerInput::default();
    input.fire = true;
    sim.step(&content, input);
    assert!(sim.sounds.iter().any(|s| s.id == "vex_fire"));
    assert_eq!(sim.players[0].primary.ammo, ammo - 1);
    let plain = content.weapon("pulse_rifle").unwrap().fire_interval;
    assert!((sim.players[0].primary.cooldown - plain).abs() < 1e-3);

    input.fire = false;
    input.reload = true;
    sim.step(&content, input);
    assert!(sim.sounds.iter().any(|s| s.id == "vex_reload"));

    sim.players[0].primary.cooldown = 0.0;
    sim.players[0].primary.reload = 0.0;
    sim.enemies.clear();
    let mut nearby = bug(gemini_sim::content::EnemyKind::Swarmer);
    nearby.hp = 40.0;
    nearby.max_hp = 40.0;
    nearby.pos = sim.players[0].pos + Vec3::new(2.0, 0.0, 0.0);
    nearby.contact = 0.0;
    sim.enemies.push(nearby);
    let ammo = sim.players[0].primary.ammo;
    input.reload = false;
    input.ability = true;
    input.fire = true;
    sim.step(&content, input);
    assert!(sim.sounds.iter().any(|s| s.id == "vex_ability"));
    assert!((sim.players[0].overdrive_left - content.hero.overdrive_duration).abs() < 1e-4);
    assert!((sim.players[0].ability_cd - content.hero.ability_cooldown).abs() < 1e-4);
    assert_eq!(sim.players[0].primary.ammo, ammo, "overdrive does not spend ammo");
    let boosted = plain / (1.0 + content.hero.overdrive_fire_rate);
    assert!((sim.players[0].primary.cooldown - boosted).abs() < 1e-3);
    assert!((sim.enemies[0].hp - 40.0).abs() < 1e-3, "overdrive is not a nova");

    sim.players[0].ability_cd = 10.0;
    sim.players[0].overdrive_left = 1.0;
    sim.enemies[0].hp = 0.0;
    sim.enemies[0].alive = true;
    input.ability = false;
    input.fire = false;
    sim.step(&content, input);
    let expected = 10.0 - gemini_sim::SIM_DT - content.hero.kill_cd_refund;
    assert!((sim.players[0].ability_cd - expected).abs() < 1e-3, "{}", sim.players[0].ability_cd);

    sim.players[0].overdrive_left = gemini_sim::SIM_DT;
    input = PlayerInput::default();
    sim.step(&content, input);
    assert!(sim.sounds.iter().any(|s| s.id == "vex_ability_end"));
    assert_eq!(sim.players[0].overdrive_left, 0.0);
    sim.step(&content, PlayerInput::default());
    assert!(!sim.sounds.iter().any(|s| s.id == "vex_ability_end"));
}
