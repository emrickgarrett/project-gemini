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
fn ledge_catch_fails_when_missing_or_not_positive() {
    let base = gemini_sim::content::embedded_sources();
    let load = |controller: &str| {
        Content::parse(&gemini_sim::content::Sources {
            controller,
            ..sources_with(&base, base.coop)
        })
    };
    let missing = base
        .controller
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with("[ledge_catch]")
                || trimmed.starts_with("snap ")
                || trimmed.starts_with("fall_speed ")
                || trimmed.starts_with("above ")
                || trimmed.starts_with("below "))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let err = load(&missing).expect_err("missing ledge_catch");
    assert!(err.contains("ledge_catch"), "{err}");
    for (from, to, key) in [
        ("snap = 0.4", "snap = 0", "snap"),
        ("snap = 0.4", "snap = nan", "snap"),
        ("fall_speed = 0.15", "fall_speed = -1", "fall_speed"),
        ("above = 0.25", "above = 0", "above"),
        ("below = 1.3", "below = nan", "below"),
    ] {
        let text = base.controller.replace(from, to);
        let err = load(&text).expect_err(key);
        assert!(err.contains(key), "{key}: {err}");
    }
}

#[test]
fn revive_times_reject_zero() {
    let base = gemini_sim::content::embedded_sources();
    let load = |controller: &str| {
        Content::parse(&gemini_sim::content::Sources {
            controller,
            ..sources_with(&base, base.coop)
        })
    };
    for (from, key) in [("revive_time = 3.0", "revive_time"), ("solo_self_revive_time = 5.0", "solo_self_revive_time")] {
        let text = base.controller.replace(from, &format!("{key} = 0.0"));
        let err = load(&text).expect_err(key);
        assert!(err.contains(key), "{key}: {err}");
        let missing = base
            .controller
            .lines()
            .filter(|line| !line.trim_start().starts_with(&format!("{key} ")))
            .collect::<Vec<_>>()
            .join("\n");
        let err = load(&missing).expect_err(key);
        assert!(err.contains(key), "missing {key}: {err}");
    }
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
    for _ in 0..60 {
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
    for _ in 0..60 {
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
    for _ in 0..70 {
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
    // South of the platform so the deck does not block the 8 m gap.
    sim.players[0].pos = Vec3::new(0.0, 0.0, 20.0);
    sim.enemies.push(spitter_at(28.0));
    sim.step(&content, PlayerInput::default());
    let back = sim.enemies.iter().find(|e| e.id == 42).unwrap();
    assert!(back.pos.z > 28.0, "inside the back-away ratio, z moved to {}", back.pos.z);
    assert!(matches!(back.ai, AiState::Chase), "8 < 20 * 0.5, so it must not spit");

    content.combat.keep_away_ratios = [0.5, 0.2];
    let mut sim = Sim::new(&content, "normal", 5);
    sim.enemies.clear();
    sim.director.pool = 0.0;
    sim.director.parked = true;
    sim.players[0].pos = Vec3::new(0.0, 0.0, 20.0);
    sim.enemies.push(spitter_at(28.0));
    sim.step(&content, PlayerInput::default());
    let firing = sim.enemies.iter().find(|e| e.id == 42).unwrap();
    assert!(matches!(firing.ai, AiState::Telegraph { .. }));

    content.combat.spitter_min_range = 4.0;
    content.combat.keep_away_ratios = [0.2, 0.1];
    let mut sim = Sim::new(&content, "normal", 6);
    sim.enemies.clear();
    sim.director.pool = 0.0;
    sim.director.parked = true;
    sim.players[0].pos = Vec3::new(0.0, 0.0, 20.0);
    sim.enemies.push(spitter_at(28.0));
    let before = 28.0;
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
        climbing: false,
        last_hit_by: 0,
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
        climbing: false,
        last_hit_by: 0,
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
        climbing: false,
        last_hit_by: 0,
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
            coop: base.coop,
            cosmetics: base.cosmetics,
        ui: base.ui,
            heroes_visual: base.heroes_visual,
            arena_visual: base.arena_visual,
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
        coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
        heroes_visual: base.heroes_visual,
        arena_visual: base.arena_visual,
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
        coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
        heroes_visual: base.heroes_visual,
        arena_visual: base.arena_visual,
    })
    .expect_err("spawn_radius inside the body");
    assert!(err.contains("spawn_radius"), "{err}");
    assert!(err.contains("body radius"), "{err}");

    assert!((brood.hatch_telegraph - 1.5).abs() < 1e-4, "telegraph {}", brood.hatch_telegraph);
    let zero = include_str!("../../../content/bosses.toml").replace("hatch_telegraph = 1.5", "hatch_telegraph = 0");
    let err = Content::parse(&gemini_sim::content::Sources {
        waves: base.waves,
        difficulty: base.difficulty,
        controller: base.controller,
        run_upgrades: base.run_upgrades,
        endless: base.endless,
        currency: base.currency,
        banners: base.banners,
        duplicate_ranks: base.duplicate_ranks,
        bosses: &zero,
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
        coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
        heroes_visual: base.heroes_visual,
        arena_visual: base.arena_visual,
    })
    .expect_err("zero hatch telegraph");
    assert!(err.contains("hatch_telegraph"), "{err}");
    let missing = include_str!("../../../content/bosses.toml")
        .lines()
        .filter(|line| !line.contains("hatch_telegraph"))
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
        bosses: &missing,
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
        coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
        heroes_visual: base.heroes_visual,
        arena_visual: base.arena_visual,
    })
    .expect_err("missing hatch telegraph");
    assert!(err.contains("hatch_telegraph"), "{err}");
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
            coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
            heroes_visual: base.heroes_visual,
            arena_visual: base.arena_visual,
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

    let no_phase = bugs.replacen("ring_phase=0.0, ", "", 1);
    let err = load(&no_phase).expect_err("a pod ring without ring_phase must fail load");
    assert!(err.contains("ring_phase"), "{err}");

    let no_radius = bugs.replacen("pod = { shape = \"sphere\", radius = 0.65 }", "pod = { shape = \"sphere\" }", 1);
    let err = load(&no_radius).expect_err("a pod hitbox without a radius must fail load");
    assert!(err.contains("pod radius"), "{err}");
}

#[test]
fn a_typo_in_a_rarity_color_fails_load() {
    let base = gemini_sim::content::embedded_sources();
    let bugs = include_str!("../../../content/bugs.toml");
    let typo = bugs.replacen("color=\"swarmer_amber\"", "color=\"rarity_commmon\"", 1);
    assert_ne!(typo, bugs, "the rarity color typo must change the bug table");
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
        bugs: &typo,
        crate_reveal: base.crate_reveal,
        coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
        heroes_visual: base.heroes_visual,
        arena_visual: base.arena_visual,
    })
    .expect_err("a typo'd rarity color must fail load");
    assert!(err.contains("rarity_commmon"), "{err}");
    assert!(err.contains("does not resolve"), "{err}");
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
    assert!((reveal.hint.text_px - 14.0).abs() < 1e-4);
    assert!((reveal.hint.opacity - 0.60).abs() < 1e-4);
    assert!((reveal.hint.margin_x - 28.0).abs() < 1e-4);
    assert!((reveal.hint.margin_y - 22.0).abs() < 1e-4);
    assert!((reveal.card.title_pad - 20.0).abs() < 1e-4);
    assert!((reveal.layout.gem_height - 0.30).abs() < 1e-4);
    assert_eq!(reveal.layout.border_px, 8);

    let base = gemini_sim::content::embedded_sources();
    let missing_hint = base.crate_reveal.lines().filter(|line| !line.contains("text_px")).collect::<Vec<_>>().join("\n");
    let err = Content::parse(&sources_reveal(&base, &missing_hint)).expect_err("missing hint text_px");
    assert!(err.contains("text_px"), "{err}");
    let flat = base.crate_reveal.replacen("opacity = 0.60", "opacity = 0.0", 1);
    let err = Content::parse(&sources_reveal(&base, &flat)).expect_err("zero hint opacity");
    assert!(err.contains("hint.opacity"), "{err}");
    for key in ["border_px", "border_mix", "gem_height", "gem_glow", "divider_px"] {
        let text = base
            .crate_reveal
            .lines()
            .filter(|line| !line.trim_start().starts_with(&format!("{key} ")))
            .collect::<Vec<_>>()
            .join("\n");
        let err = Content::parse(&sources_reveal(&base, &text)).expect_err(key);
        assert!(err.contains(key), "{key}: {err}");
    }
}

fn sources_reveal<'a>(base: &'a gemini_sim::content::Sources<'a>, crate_reveal: &'a str) -> gemini_sim::content::Sources<'a> {
    gemini_sim::content::Sources {
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
        bugs: base.bugs,
        crate_reveal,
        coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
        heroes_visual: base.heroes_visual,
        arena_visual: base.arena_visual,
    }
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
        "vent_warn".into(),
        "vent_burst".into(),
        "perch_climb_warn".into(),
        "fuel_regen_paused".into(),
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
    let stem = |path: &str| {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        name.trim_end_matches(".ogg").to_string()
    };
    let reveal = &content.crate_reveal;
    let mut reveal_ids = Vec::new();
    for tier in [&reveal.common, &reveal.rare, &reveal.epic, &reveal.legendary] {
        reveal_ids.push(stem(&tier.tease));
        reveal_ids.push(stem(&tier.sting));
    }
    let fakeout = stem(&reveal.fakeout_sfx);
    assert_eq!(fakeout, "crate_fakeout");
    reveal_ids.push(fakeout);

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    for id in ids {
        let sfx = content.sfx(&id).unwrap_or_else(|| panic!("{id} does not resolve in audio_mix.toml"));
        let path = root.join(&sfx.path);
        assert!(path.is_file(), "{id} resolved to {} but that file is missing", path.display());
    }
    for id in &reveal_ids {
        let path = content
            .cue_path(id)
            .unwrap_or_else(|| panic!("{id} does not resolve in audio_mix.toml"));
        assert!(
            root.join(path).is_file(),
            "{id} resolved to {} but that file is missing",
            path
        );
        let volume = content.cue_volume(id).unwrap_or_else(|| panic!("{id} mix row is missing volume_db"));
        assert!(volume.is_finite() && volume > 0.0, "{id} volume {volume}");
    }
    let gain = |db: f32| gemini_sim::audio_mix::db_to_linear(db);
    for id in ["crate_common", "crate_rare", "crate_epic"] {
        let volume = content.cue_volume(id).unwrap();
        assert!((volume - gain(-4.0)).abs() < 1e-4, "{id} volume {volume}");
    }
    let legendary = content.cue_volume("crate_legendary").unwrap();
    assert!((legendary - gain(-2.0)).abs() < 1e-4, "crate_legendary volume {legendary}");

    let base = gemini_sim::content::embedded_sources();
    let load = |audio: &str| {
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
            audio,
            bugs: base.bugs,
            crate_reveal: base.crate_reveal,
            coop: base.coop,
            cosmetics: base.cosmetics,
        ui: base.ui,
            heroes_visual: base.heroes_visual,
            arena_visual: base.arena_visual,
        })
    };
    let without_gain = base.audio.replacen(
        "[sting.crate_common]\npath = \"audio/stings/crate_common.ogg\"\nvolume_db = -4.0\n",
        "[sting.crate_common]\npath = \"audio/stings/crate_common.ogg\"\n",
        1,
    );
    assert_ne!(without_gain, base.audio, "removing volume_db must change the mix");
    let err = load(&without_gain).expect_err("a sting with no volume_db must fail load");
    assert!(err.contains("crate_common"), "{err}");
    assert!(err.contains("volume_db"), "{err}");

    let without_file = base.audio.replacen(
        "path = \"audio/stings/crate_legendary.ogg\"",
        "path = \"audio/stings/crate_legendary_missing.ogg\"",
        1,
    );
    assert_ne!(without_file, base.audio, "retargeting the sting path must change the mix");
    let err = load(&without_file).expect_err("a reveal cue with no file must fail load");
    assert!(err.contains("crate_legendary"), "{err}");
    assert!(err.contains("missing"), "{err}");
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
        coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
        heroes_visual: base.heroes_visual,
        arena_visual: base.arena_visual,
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
    assert_eq!(content.hero.accent, content.palette.hex("hero_vex").unwrap());
    assert!(!content.ui.settings.unlock_all, "unlock_all is off unless the content flag says otherwise");
    assert!(content.hero_selectable("vex"));
    assert!(!content.hero_selectable("nyra"));
    assert!(content.heroes.iter().any(|hero| hero.id == "nyra" && !hero.owned));
    assert!((content.hero.max_hp - 100.0).abs() < 1e-4);
    assert!((content.hero.move_speed_mult - 1.0).abs() < 1e-4);
    assert!((content.hero.ability_cooldown - 20.0).abs() < 1e-4);
    assert!((content.hero.cooldown_floor - 0.1).abs() < 1e-4);
    assert!((content.hero.overdrive_duration - 6.0).abs() < 1e-4);
    assert!((content.hero.overdrive_fire_rate - 0.35).abs() < 1e-4);
    assert!(content.hero.infinite_magazine);
    assert!((content.hero.kill_cd_refund - 1.0).abs() < 1e-4);
    let off = content.hero.viewmodel_offset;
    assert!(off[0] > 0.0 && off[1] < 0.0, "viewmodel should sit lower right, got {off:?}");
    assert!((off[0] - 0.44).abs() < 1e-4 && (off[1] + 0.28).abs() < 1e-4 && (off[2] + 0.48).abs() < 1e-4);
    assert!((content.hero.viewmodel_fov - 40.0).abs() < 1e-4);
    assert!((content.hero.muzzle_flash_secs - 0.05).abs() < 1e-4);
    assert!((content.hero.muzzle_flash_radius - 0.11).abs() < 1e-4);
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
        coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
        heroes_visual: base.heroes_visual,
        arena_visual: base.arena_visual,
    })
    .expect_err("missing overdrive duration");
    assert!(err.contains("duration"), "{err}");

    let load_hero = |hero: &str| {
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
            hero,
            arena: base.arena,
            presentation: base.presentation,
            combat: base.combat,
            palette: base.palette,
            music: base.music,
            audio: base.audio,
            bugs: base.bugs,
            crate_reveal: base.crate_reveal,
            coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
            heroes_visual: base.heroes_visual,
            arena_visual: base.arena_visual,
        })
    };
    let text = include_str!("../../../content/characters.toml");
    let missing_floor = text.replacen("cooldown_floor = 0.1, ", "", 1);
    let err = load_hero(&missing_floor).expect_err("missing cooldown_floor");
    assert!(err.contains("cooldown_floor"), "{err}");
    for bad in ["cooldown_floor = 0", "cooldown_floor = nan", "cooldown_floor = -0.1"] {
        let hero = text.replacen("cooldown_floor = 0.1", bad, 1);
        let err = load_hero(&hero).expect_err(bad);
        assert!(err.contains("cooldown_floor"), "{err}");
    }
    for key in ["viewmodel_offset", "viewmodel_fov", "muzzle_flash_secs", "muzzle_flash_radius"] {
        let stripped = text.lines().filter(|line| !line.contains(key)).collect::<Vec<_>>().join("\n");
        let err = load_hero(&stripped).expect_err(key);
        assert!(err.contains(key), "{err}");
    }
    let zero = text.replace("viewmodel_fov = 40", "viewmodel_fov = 0");
    let err = load_hero(&zero).expect_err("zero viewmodel fov");
    assert!(err.contains("viewmodel_fov"), "{err}");
    for (key, sample) in [("muzzle_flash_secs", "0.05"), ("muzzle_flash_radius", "0.11")] {
        let zeroed = text.replace(&format!("{key} = {sample}"), &format!("{key} = 0"));
        let err = load_hero(&zeroed).expect_err(key);
        assert!(err.contains(key), "{err}");
        let negative = text.replace(&format!("{key} = {sample}"), &format!("{key} = -1"));
        let err = load_hero(&negative).expect_err(key);
        assert!(err.contains(key), "{err}");
    }

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
    sim.enemies[0].last_hit_by = sim.players[0].id;
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

#[test]
fn arena_01_vents_perches_and_pickups() {
    use gemini_sim::content::{EnemyKind, WaveDef};
    let content = Content::embedded();
    assert_eq!(content.arena.shape, "circle");
    assert!((content.arena.radius - 40.0).abs() < 1e-3);
    assert!((content.arena.size[0] - 80.0).abs() < 1e-3);
    assert_eq!(content.arena.vents.len(), 6);
    assert!((content.arena.vents[0].telegraph - 1.5).abs() < 1e-4);
    assert!((content.arena.vents[1].bearing_deg - 60.0).abs() < 1e-3);
    assert!((content.arena.platform.height - 4.0).abs() < 1e-4);
    assert!((content.arena.platform.railing_height - 0.6).abs() < 1e-4);
    assert_eq!(content.arena.platform.ramps.len(), 2);
    assert!((content.arena.floor_y(0.0, 0.0) - 4.0).abs() < 1e-3);
    assert!((content.arena.floor_y(12.0, 0.0) - 2.0).abs() < 0.05);
    assert!(content.arena.floor_y(30.0, 10.0).abs() < 1e-3);
    assert!((content.arena.boss_area.center.z - (-24.0)).abs() < 1e-3);
    assert!((content.arena.boss_area.radius - 12.5).abs() < 1e-3);
    assert_eq!(content.arena.ledges.len(), 4);
    assert!(content.arena.ledges.iter().all(|ledge| ledge.jetpack_only));
    assert!(content.arena.pickups.iter().all(|spot| spot.spawns_during == "breather"));
    assert!((content.arena.player_spawn.y - 4.0).abs() < 1e-4);
    assert_eq!(content.arena.perch_rules.flow_field_targets, "nearest_grounded_player");
    assert!(!content.arena.perch_rules.fuel_regen_on_perch);
    assert!((content.arena.perch_rules.climb_speed_mult - 0.5).abs() < 1e-4);
    assert!((content.arena.perch_rules.all_perched_grace - 8.0).abs() < 1e-4);
    assert!((content.arena.perch_rules.budget_shift_to_spitters - 0.3).abs() < 1e-4);
    assert!((content.arena.perch_rules.perch_radius - 1.5).abs() < 1e-4);
    assert!((content.arena.perch_rules.perch_height_slack - 0.45).abs() < 1e-4);
    assert!((content.arena.perch_rules.climb_trigger_distance - 1.8).abs() < 1e-4);
    assert!((content.arena.perch_rules.climb_slide_rate - 0.4).abs() < 1e-4);
    assert!((content.arena.perch_rules.climb_clearance - 0.5).abs() < 1e-4);
    assert!((content.arena.perch_rules.ground_chase_pad - 0.5).abs() < 1e-4);
    assert_eq!(content.arena.paint.floor_light, "arena_light");
    assert_eq!(content.arena.paint.floor_dark, "arena_dark");
    assert_eq!(content.arena.paint.band_x, 48);
    assert_eq!(content.arena.paint.band_y, 36);
    assert!((content.arena.paint.shade - 0.45).abs() < 1e-4);
    assert_eq!(content.arena.edge_wash.sky, "arena_sky");
    assert_eq!(content.palette.hex("arena_sky"), Some("#080A10"));
    assert!((content.arena.edge_wash.horizon - 0.42).abs() < 1e-4);
    assert!((content.arena.edge_wash.mix - 0.25).abs() < 1e-4);
    assert!((content.arena.edge_wash.span - 0.45).abs() < 1e-4);
    let catch = &content.controller.ledge_catch;
    assert!((catch.snap - 0.4).abs() < 1e-4);
    assert!((catch.fall_speed - 0.15).abs() < 1e-4);
    assert!((catch.above - 0.25).abs() < 1e-4);
    assert!((catch.below - 1.3).abs() < 1e-4);
    let tower = content.arena.ledges.iter().find(|ledge| ledge.id == "tower").unwrap().pos;
    assert!(content.arena.on_perch(Vec3::new(tower.x + 1.4, tower.y, tower.z)));
    assert!(content.arena.on_perch(Vec3::new(tower.x, tower.y + 0.4, tower.z)));
    assert!(!content.arena.on_perch(Vec3::new(tower.x, tower.y + 0.5, tower.z)));
    assert!((content.arena.support_y(tower.x + 1.3, tower.y, tower.z, 0.0, catch) - tower.y).abs() < 1e-3);
    assert!(content.arena.support_y(tower.x + 1.6, tower.y, tower.z, 0.0, catch).abs() < 1e-3);
    assert!((content.controller.camera.eye_height - 1.62).abs() < 1e-4);
    assert!((content.controller.camera.crouch_eye - 1.02).abs() < 1e-4);

    let base = gemini_sim::content::embedded_sources();
    let parse = |arena: &str| {
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
            arena,
            presentation: base.presentation,
            combat: base.combat,
            palette: base.palette,
            music: base.music,
            audio: base.audio,
            bugs: base.bugs,
            crate_reveal: base.crate_reveal,
            coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
            heroes_visual: base.heroes_visual,
            arena_visual: base.arena_visual,
        })
    };
    for key in [
        "perch_radius",
        "perch_height_slack",
        "climb_trigger_distance",
        "climb_slide_rate",
        "climb_clearance",
        "ground_chase_pad",
        "floor_light",
        "floor_dark",
        "band_x",
        "band_y",
        "shade",
        "horizon",
        "span",
        "sky",
        "mix",
    ] {
        let missing_key = include_str!("../../../content/arena_01.toml")
            .lines()
            .filter(|line| !line.trim_start().starts_with(&format!("{key} ")))
            .collect::<Vec<_>>()
            .join("\n");
        let err = parse(&missing_key).expect_err(key);
        assert!(err.contains(key), "{err}");
    }
    for bad in ["climb_clearance = 0", "climb_clearance = nan", "climb_clearance = -1"] {
        let text = include_str!("../../../content/arena_01.toml").replacen("climb_clearance = 0.5", bad, 1);
        let err = parse(&text).expect_err(bad);
        assert!(err.contains("climb_clearance"), "{err}");
    }
    for bad in ["ground_chase_pad = 0", "ground_chase_pad = nan", "ground_chase_pad = -1"] {
        let text = include_str!("../../../content/arena_01.toml").replacen("ground_chase_pad = 0.5", bad, 1);
        let err = parse(&text).expect_err(bad);
        assert!(err.contains("ground_chase_pad"), "{err}");
    }
    for bad in ["perch_height_slack = 0", "perch_height_slack = nan", "perch_height_slack = -0.1"] {
        let text = include_str!("../../../content/arena_01.toml").replacen("perch_height_slack = 0.45", bad, 1);
        let err = parse(&text).expect_err(bad);
        assert!(err.contains("perch_height_slack"), "{err}");
    }
    let revived = include_str!("../../../content/arena_01.toml").replacen(
        "climb_clearance = 0.5",
        "climb_target_radius = 2.0\nclimb_clearance = 0.5",
        1,
    );
    let err = parse(&revived).expect_err("climb_target_radius");
    assert!(err.contains("climb_target_radius"), "{err}");
    let bad_color = include_str!("../../../content/arena_01.toml").replacen("floor_light = \"arena_light\"", "floor_light = \"not_a_color\"", 1);
    let err = parse(&bad_color).expect_err("unknown floor color");
    assert!(err.contains("does not resolve"), "{err}");
    let bad_sky = include_str!("../../../content/arena_01.toml").replacen("sky = \"arena_sky\"", "sky = \"not_a_color\"", 1);
    let err = parse(&bad_sky).expect_err("unknown sky");
    assert!(err.contains("does not resolve"), "{err}");
    let missing_sky = include_str!("../../../content/arena_01.toml").replacen("sky = \"arena_sky\"\n", "", 1);
    let err = parse(&missing_sky).expect_err("missing sky");
    assert!(err.contains("sky"), "{err}");
    let missing = include_str!("../../../content/arena_01.toml").replacen("telegraph = 1.5\n", "", 1);
    let err = parse(&missing).expect_err("missing telegraph");
    assert!(err.contains("telegraph"), "{err}");
    let inside = include_str!("../../../content/arena_01.toml").replacen("pos = [0.0, 0, -38.0]", "pos = [0.0, 0, -24.0]", 1);
    let err = parse(&inside).expect_err("vent in the boss area");
    assert!(err.contains("boss"), "{err}");

    let mut boss_content = Content::embedded();
    boss_content.waves.wave = vec![WaveDef {
        n: 1,
        mix: [1.0, 0.0, 0.0],
        boss: Some("brood_mother".into()),
    }];
    let boss_sim = Sim::new(&boss_content, "normal", 1);
    let boss = boss_sim
        .enemies
        .iter()
        .find(|enemy| enemy.kind == EnemyKind::BroodMother)
        .unwrap();
    assert!((boss.pos.x - content.arena.boss_area.center.x).abs() < 1e-3);
    assert!((boss.pos.z - content.arena.boss_area.center.z).abs() < 1e-3);

    let mut sim = Sim::new(&content, "normal", 2);
    assert!((sim.players[0].pos.y - 4.0).abs() < 1e-3);
    sim.step(&content, PlayerInput::default());
    assert!(sim.sounds.iter().any(|sound| sound.id == "vent_warn"));
    assert!(sim.pending_vents.iter().all(|vent| vent.kind == EnemyKind::Swarmer));
    assert!(sim.enemies.is_empty());
    let mut heard_burst = false;
    for _ in 0..60 {
        sim.step(&content, PlayerInput::default());
        if sim.sounds.iter().any(|sound| sound.id == "vent_burst") {
            heard_burst = true;
        }
    }
    assert!(heard_burst);
    assert!(sim.enemies.iter().any(|enemy| enemy.kind == EnemyKind::Swarmer));

    let mut sim = Sim::new(&content, "normal", 4);
    sim.phase = Phase::Upgrade {
        options: vec!["rapid_cells".into()],
    };
    sim.step(
        &content,
        PlayerInput {
            upgrade_choice: Some(0),
            ..PlayerInput::default()
        },
    );
    assert!(matches!(sim.phase, Phase::Breather { .. }));
    assert_eq!(sim.pickups.len(), 4);

    let ledge = content.arena.ledges.iter().find(|ledge| ledge.id == "shelf_e").unwrap().pos;
    let mut sim = Sim::new(&content, "normal", 5);
    sim.enemies.clear();
    sim.pending_vents.clear();
    sim.director.pool = 0.0;
    sim.director.parked = true;
    sim.players[0].pos = ledge;
    sim.players[0].vel = Vec3::ZERO;
    sim.players[0].grounded = true;
    sim.players[0].fuel = 10.0;
    sim.players[0].fuel_delay = 0.0;
    sim.all_perched_for = content.arena.perch_rules.all_perched_grace;
    let mut climber = bug(EnemyKind::Swarmer);
    climber.id = 3;
    climber.speed = 4.0;
    climber.pos = Vec3::new(ledge.x, 0.0, ledge.z);
    sim.enemies.push(climber);
    let mut warned = false;
    for _ in 0..6 {
        sim.step(&content, PlayerInput::default());
        if sim.sounds.iter().any(|sound| sound.id == "perch_climb_warn") {
            warned = true;
        }
    }
    assert!(warned);
    assert!((sim.players[0].fuel - 10.0).abs() < 1e-3, "fuel {}", sim.players[0].fuel);
    assert!(sim.enemies[0].pos.y > 0.2, "y {}", sim.enemies[0].pos.y);

    let mut shifted = Content::embedded();
    shifted.combat.group_budget = 80.0;
    let mut sim = Sim::new(&shifted, "normal", 8);
    sim.players[0].pos = ledge;
    sim.players[0].grounded = true;
    sim.director.pool = 80.0;
    sim.director.group_left = 80.0;
    sim.director.parked = false;
    sim.director.spawn_wait = 0.0;
    sim.pending_vents.clear();
    sim.step(&shifted, PlayerInput::default());
    assert!(
        sim.pending_vents.iter().any(|vent| vent.kind == EnemyKind::Spitter),
        "perched budget should move a share onto spitters"
    );
}

#[test]
fn perch_disk_pauses_fuel_once_and_ground_chase_stays_outside_it() {
    use gemini_sim::content::EnemyKind;
    let content = Content::embedded();
    let tower = content
        .arena
        .ledges
        .iter()
        .find(|ledge| ledge.id == "tower")
        .unwrap()
        .pos;
    assert!(content.arena.on_perch(Vec3::new(tower.x + 1.4, tower.y, tower.z)));
    assert!(!content.arena.on_perch(Vec3::new(tower.x + 1.6, tower.y, tower.z)));

    let mut perched = Sim::new(&content, "normal", 11);
    perched.enemies.clear();
    perched.pending_vents.clear();
    perched.director.pool = 0.0;
    perched.director.parked = true;
    perched.players[0].pos = Vec3::new(tower.x + 1.4, tower.y, tower.z);
    perched.players[0].vel = Vec3::ZERO;
    perched.players[0].grounded = true;
    perched.players[0].fuel = 40.0;
    perched.players[0].fuel_delay = 0.0;
    perched.players[0].regen_pause_cue = false;
    perched.step(&content, PlayerInput::default());
    assert_eq!(
        perched.sounds.iter().filter(|sound| sound.id == "fuel_regen_paused").count(),
        1
    );
    assert!(content.arena.on_perch(perched.players[0].pos));
    assert!((perched.players[0].fuel - 40.0).abs() < 1e-3, "fuel {}", perched.players[0].fuel);
    perched.step(&content, PlayerInput::default());
    assert_eq!(
        perched.sounds.iter().filter(|sound| sound.id == "fuel_regen_paused").count(),
        0
    );
    assert!((perched.players[0].fuel - 40.0).abs() < 1e-3, "fuel {}", perched.players[0].fuel);

    let mut grounded = Sim::new(&content, "normal", 12);
    grounded.enemies.clear();
    grounded.pending_vents.clear();
    grounded.director.pool = 0.0;
    grounded.director.parked = true;
    grounded.players[0].pos = Vec3::new(tower.x + 1.6, 0.0, tower.z);
    grounded.players[0].vel = Vec3::ZERO;
    grounded.players[0].grounded = true;
    grounded.players[0].fuel = 40.0;
    grounded.players[0].fuel_delay = 0.0;
    grounded.players[0].regen_pause_cue = false;
    grounded.all_perched_for = content.arena.perch_rules.all_perched_grace;
    let mut swarmer = bug(EnemyKind::Swarmer);
    swarmer.speed = 4.0;
    swarmer.pos = Vec3::new(tower.x + 1.6, 0.0, tower.z + 6.0);
    grounded.enemies.push(swarmer);
    let start = grounded.enemies[0].pos;
    for _ in 0..8 {
        grounded.step(&content, PlayerInput::default());
        assert!(
            !grounded.sounds.iter().any(|sound| sound.id == "fuel_regen_paused"),
            "fuel pause outside the perch disk"
        );
    }
    let player = grounded.players[0].pos;
    assert!(grounded.players[0].grounded);
    assert!(!content.arena.on_perch(player));
    assert!(grounded.players[0].fuel > 40.0, "fuel {}", grounded.players[0].fuel);
    assert!(!grounded.players[0].regen_pause_cue);
    let enemy = &grounded.enemies[0];
    assert!(!enemy.climbing);
    assert!(enemy.pos.y.abs() < 1e-3, "y {}", enemy.pos.y);
    let before = (start.x - player.x).powi(2) + (start.z - player.z).powi(2);
    let after = (enemy.pos.x - player.x).powi(2) + (enemy.pos.z - player.z).powi(2);
    assert!(after < before, "ground chase {before} -> {after}");
}

#[test]
fn coop_table_rejects_missing_keys() {
    let content = Content::embedded();
    let sim = Sim::new(&content, "normal", 1);
    assert!(sim.lobby_ready(&content));
    assert_eq!(content.coop.max_players, 8);
    assert_eq!(content.coop.min_players, 1);
    assert!((content.coop.slot_hold_seconds - 90.0).abs() < 1e-3);
    assert!((content.coop.multi_reviver_speedup - 0.35).abs() < 1e-4);
    assert_eq!(content.coop.catchup_pick_mode, "choose_from_3");
    assert_eq!(content.coop.catchup_pick_count, 3);
    assert_eq!(content.coop.max_extra_revivers, 2);
    assert_eq!(content.coop.upgrade_catchup, "match_lowest");
    assert_eq!(content.coop.join_allowed_during, "breather_only");
    assert_eq!(content.coop.budget_rescale, "next_wave");
    assert_eq!(content.coop.squad_wipe, "all_downed");
    assert_eq!(content.coop.currency_credit, "per_round_present");
    assert_eq!(content.coop.host_leave, "match_ends_rounds_kept");
    assert!((content.coop.host_away.detect_after_seconds - 3.0).abs() < 1e-3);
    assert!((content.coop.host_away.end_after_seconds - 60.0).abs() < 1e-3);
    assert!((content.coop.host_away.resume_countdown_seconds - 3.0).abs() < 1e-3);
    assert!((content.coop.host_away.away_timer_reset_after_play_seconds - 10.0).abs() < 1e-3);
    assert_eq!(content.coop.host_away.freeze, "everything");
    assert!(!content.coop.friendly_fire);
    assert!(!content.coop.downed_on_return);
    assert!(content.coop.bleedout_pause_while_reviving);
    assert!(content.coop.leave_keeps_earned);

    let base = gemini_sim::content::embedded_sources();
    let stripped = base
        .coop
        .lines()
        .filter(|line| !line.contains("max_players"))
        .collect::<Vec<_>>()
        .join("\n");
    let err = Content::parse(&sources_with(&base, &stripped)).expect_err("missing max_players");
    assert!(err.contains("max_players"), "{err}");
    let stripped_extra = base
        .coop
        .lines()
        .filter(|line| !line.contains("max_extra_revivers"))
        .collect::<Vec<_>>()
        .join("\n");
    let err = Content::parse(&sources_with(&base, &stripped_extra)).expect_err("missing max_extra_revivers");
    assert!(err.contains("max_extra_revivers"), "{err}");

    for key in [
        "detect_after_seconds",
        "end_after_seconds",
        "resume_countdown_seconds",
        "away_timer_reset_after_play_seconds",
    ] {
        let stripped_away = base
            .coop
            .lines()
            .filter(|line| !line.contains(key))
            .collect::<Vec<_>>()
            .join("\n");
        let err = Content::parse(&sources_with(&base, &stripped_away)).expect_err("missing host_away key");
        assert!(err.contains(key), "{err}");
    }
    let zero_away = base.coop.replace("end_after_seconds = 60.0", "end_after_seconds = 0");
    let err = Content::parse(&sources_with(&base, &zero_away)).expect_err("zero end");
    assert!(err.contains("end_after_seconds"), "{err}");
    let negative_away = base.coop.replace("detect_after_seconds = 3.0", "detect_after_seconds = -1");
    let err = Content::parse(&sources_with(&base, &negative_away)).expect_err("negative detect");
    assert!(err.contains("detect_after_seconds"), "{err}");
    let negative_count = base
        .coop
        .replace("resume_countdown_seconds = 3.0", "resume_countdown_seconds = -2.0");
    let err = Content::parse(&sources_with(&base, &negative_count)).expect_err("negative countdown");
    assert!(err.contains("resume_countdown_seconds"), "{err}");
    let nan_away = base.coop.replace("detect_after_seconds = 3.0", "detect_after_seconds = nan");
    let err = Content::parse(&sources_with(&base, &nan_away)).expect_err("nan detect");
    assert!(err.contains("detect_after_seconds"), "{err}");
    let zero_reset = base.coop.replace(
        "away_timer_reset_after_play_seconds = 10.0",
        "away_timer_reset_after_play_seconds = 0",
    );
    let err = Content::parse(&sources_with(&base, &zero_reset)).expect_err("zero reset");
    assert!(err.contains("away_timer_reset_after_play_seconds"), "{err}");
    let nan_reset = base.coop.replace(
        "away_timer_reset_after_play_seconds = 10.0",
        "away_timer_reset_after_play_seconds = nan",
    );
    let err = Content::parse(&sources_with(&base, &nan_reset)).expect_err("nan reset");
    assert!(err.contains("away_timer_reset_after_play_seconds"), "{err}");
    let bad_freeze = base.coop.replace("freeze = \"everything\"", "freeze = \"enemies\"");
    let err = Content::parse(&sources_with(&base, &bad_freeze)).expect_err("freeze tag");
    assert!(err.contains("everything"), "{err}");
    let bad_slate = base.presentation.replace("slate = \"ui_panel_slate\"", "slate = \"not_a_color\"");
    let err = Content::parse(&sources_present(&base, &bad_slate)).expect_err("palette typo");
    assert!(err.contains("not_a_color"), "{err}");
    assert!((content.presentation.hud.away.away_desaturation - 0.6).abs() < 1e-4);
    assert!((content.presentation.hud.away.away_dim - 0.3).abs() < 1e-4);
    assert!((content.presentation.hud.away.music_duck_db - -12.0).abs() < 1e-4);
    assert!((content.presentation.hud.away.music_duck_lowpass_hz - 800.0).abs() < 1e-3);
    assert!((content.presentation.hud.away.bar_track - 0.45).abs() < 1e-4);
    assert!((content.presentation.hud.away.dim_floor - 0.05).abs() < 1e-4);
    assert!(!content.presentation.hud.away.count_shadow_samples.is_empty());
    assert!((content.presentation.hud.away.count_outline_px - 4.0).abs() < 1e-4);

    let unknown = format!("{}\n[extra]\nnope = 1\n", base.coop);
    let err = Content::parse(&sources_with(&base, &unknown)).expect_err("unknown table");
    assert!(err.contains("unknown key"), "{err}");
}

#[test]
fn host_away_mix_row_and_sync_at_fail_load() {
    let base = gemini_sim::content::embedded_sources();
    let missing_row = base.audio.replace("[host_away]\n", "[host_away_missing]\n");
    let mut sources = sources_with(&base, base.coop);
    sources.audio = &missing_row;
    let err = Content::parse(&sources).expect_err("missing host_away row");
    assert!(err.contains("host_away"), "{err}");

    let missing_sync = base.audio.replace("sync_at = 3.0\n", "");
    sources.audio = &missing_sync;
    let err = Content::parse(&sources).expect_err("missing sync_at");
    assert!(err.contains("sync_at"), "{err}");

    let late_sync = base.audio.replace("sync_at = 3.0", "sync_at = 9.0");
    sources.audio = &late_sync;
    let err = Content::parse(&sources).expect_err("sync_at far from the countdown");
    assert!(err.contains("sync_at"), "{err}");
}

#[test]
fn away_look_and_duck_fail_load() {
    let base = gemini_sim::content::embedded_sources();
    for key in [
        "away_desaturation",
        "away_dim",
        "music_duck_db",
        "music_duck_lowpass_hz",
        "count_shadow",
        "count_shadow_px",
        "count_shadow_soft_px",
        "count_shadow_alpha",
        "count_shadow_samples",
        "bar_track",
        "dim_floor",
    ] {
        let stripped = base
            .presentation
            .lines()
            .filter(|line| !line.contains(&format!("{key} =")))
            .collect::<Vec<_>>()
            .join("\n");
        let err = Content::parse(&sources_present(&base, &stripped)).expect_err(key);
        assert!(err.contains(key), "{key}: {err}");
    }
    let cases = [
        ("away_desaturation = 0.6", "away_desaturation = 1.4", "away_desaturation"),
        ("away_desaturation = 0.6", "away_desaturation = nan", "away_desaturation"),
        ("away_dim = 0.3", "away_dim = -0.1", "away_dim"),
        ("away_dim = 0.3", "away_dim = 1.5", "away_dim"),
        ("music_duck_db = -12.0", "music_duck_db = 4.0", "music_duck_db"),
        ("music_duck_db = -12.0", "music_duck_db = nan", "music_duck_db"),
        ("count_shadow_px = 14.0", "count_shadow_px = 0.0", "count_shadow_px"),
        ("count_shadow_alpha = 0.40", "count_shadow_alpha = 0.0", "count_shadow_alpha"),
        ("count_shadow_soft_px = 8.0", "count_shadow_soft_px = -1", "count_shadow_soft_px"),
        ("music_duck_lowpass_hz = 800.0", "music_duck_lowpass_hz = 0.0", "music_duck_lowpass_hz"),
        ("music_duck_lowpass_hz = 800.0", "music_duck_lowpass_hz = nan", "music_duck_lowpass_hz"),
        ("music_duck_lowpass_hz = 800.0", "music_duck_lowpass_hz = -4.0", "music_duck_lowpass_hz"),
        ("bar_track = 0.45", "bar_track = 0.0", "bar_track"),
        ("bar_track = 0.45", "bar_track = nan", "bar_track"),
        ("dim_floor = 0.05", "dim_floor = 0.0", "dim_floor"),
        ("dim_floor = 0.05", "dim_floor = nan", "dim_floor"),
        ("weight = 0.45", "weight = 0.0", "count_shadow_samples"),
        ("weight = 0.45", "weight = nan", "count_shadow_samples"),
    ];
    for (from, to, key) in cases {
        let text = base.presentation.replace(from, to);
        let err = Content::parse(&sources_present(&base, &text)).expect_err(key);
        assert!(err.contains(key), "{key}: {err}");
    }
}

#[test]
fn empty_palette_color_name_fails_load() {
    let base = gemini_sim::content::embedded_sources();
    let text = base.presentation.replace("slate = \"ui_panel_slate\"", "slate = \"\"");
    let err = Content::parse(&sources_present(&base, &text)).expect_err("empty slate");
    assert!(err.contains("does not resolve"), "{err}");
    let part = base.bugs.replace("color=\"swarmer_amber\"", "color=\"\"");
    let sources = gemini_sim::content::Sources {
        bugs: &part,
        ..sources_with(&base, base.coop)
    };
    let err = Content::parse(&sources).expect_err("empty part color");
    assert!(err.contains("does not resolve"), "{err}");
}

#[test]
fn reveal_layout_fails_when_missing_or_invalid() {
    let base = gemini_sim::content::embedded_sources();
    let stripped = base
        .crate_reveal
        .lines()
        .filter(|line| !line.contains("panel_width ="))
        .collect::<Vec<_>>()
        .join("\n");
    let sources = gemini_sim::content::Sources {
        crate_reveal: &stripped,
        ..sources_with(&base, base.coop)
    };
    let err = Content::parse(&sources).expect_err("missing panel");
    assert!(err.contains("panel_width"), "{err}");
    let zero = base.crate_reveal.replace("panel_width = 420.0", "panel_width = nan");
    let sources = gemini_sim::content::Sources {
        crate_reveal: &zero,
        ..sources_with(&base, base.coop)
    };
    let err = Content::parse(&sources).expect_err("nan panel");
    assert!(err.contains("panel_width"), "{err}");
    let content = Content::embedded();
    let layout = &content.crate_reveal.layout;
    assert!((layout.panel_width - 420.0).abs() < 1e-3);
    assert!((layout.panel_height - 240.0).abs() < 1e-3);
    assert!((layout.ink_luma - 0.62).abs() < 1e-4);
    assert_eq!(layout.title_pad_px, 8);
    assert_eq!(layout.glyph_rows, 7);
    assert_eq!(layout.title_glyph_px, 6);
    let stage = content.palette.key(&layout.stage);
    let painted = [
        (stage[0] * 255.0) as u8 as f32 + layout.stage_base[0],
        (stage[1] * 255.0) as u8 as f32 + layout.stage_base[1],
        (stage[2] * 255.0) as u8 as f32 + layout.stage_base[2],
    ];
    assert_eq!(painted, [16.0, 12.0, 22.0]);
    let missing_rows = base.crate_reveal.replacen("glyph_rows = 7\n", "", 1);
    let err = Content::parse(&sources_reveal(&base, &missing_rows)).expect_err("missing glyph_rows");
    assert!(err.contains("glyph_rows"), "{err}");
    let zero_rows = base.crate_reveal.replacen("glyph_rows = 7", "glyph_rows = 0", 1);
    let err = Content::parse(&sources_reveal(&base, &zero_rows)).expect_err("zero glyph_rows");
    assert!(err.contains("glyph_rows"), "{err}");
    assert_eq!(layout.title_height_div, 16);
    assert_eq!(layout.title_scale_min, 2);
    assert_eq!(layout.title_scale_max, 14);
    assert!((layout.stage_y - 0.42).abs() < 1e-4);
    assert!((layout.stage_lift_pow - 1.35).abs() < 1e-4);
    assert!((layout.stage_vignette_pow - 1.6).abs() < 1e-4);
}

#[test]
fn ui_spacing_and_font_coverage_fail_closed() {
    let content = Content::embedded();
    assert!((content.ui.layout.leading - 2.0).abs() < 1e-4);
    assert!((content.ui.layout.spacing - 4.0).abs() < 1e-4);
    assert!((content.ui.layout.spacing_role - 6.0).abs() < 1e-4);
    assert!((content.ui.layout.spacing_block - 8.0).abs() < 1e-4);
    assert!((content.ui.layout.inset_x - 12.0).abs() < 1e-4);
    assert!((content.ui.layout.inset_y - 6.0).abs() < 1e-4);
    assert!((content.ui.layout.inset_bar - 8.0).abs() < 1e-4);
    assert!((content.ui.layout.inset_portrait - 4.0).abs() < 1e-4);
    assert!((content.ui.layout.inset_min - 1.0).abs() < 1e-4);
    assert!((content.ui.layout.panel_alpha - 0.85).abs() < 1e-4);
    assert!((content.ui.layout.stepper_min_px - 8.0).abs() < 1e-4);
    assert_eq!(content.ui.layout.stepper_width_divisor, 5);
    assert_eq!(content.ui.layout.stepper_border_divisor, 8);
    assert_eq!(content.ui.colors.panel, "ui_panel_slate");
    assert_eq!(content.ui.portrait.bg, "portrait_field");
    assert_eq!(content.ui.portrait.raster, 320);
    assert!((content.ui.portrait.fov_floor - 10.0).abs() < 1e-4);
    assert!((content.ui.portrait.fov_limit - 150.0).abs() < 1e-4);
    assert!((content.ui.portrait.fov_step - 4.0).abs() < 1e-4);
    assert!((content.ui.portrait.near - 0.08).abs() < 1e-4);
    assert!(gemini_sim::content::font_covers(';'));
    assert!(!gemini_sim::content::font_covers('_'));
    assert!(!gemini_sim::content::font_covers('?'));

    let base = gemini_sim::content::embedded_sources();
    let load_ui = |ui: &str| {
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
            bugs: base.bugs,
            crate_reveal: base.crate_reveal,
            coop: base.coop,
            cosmetics: base.cosmetics,
            ui,
            heroes_visual: base.heroes_visual,
            arena_visual: base.arena_visual,
        })
    };
    let clear = base.ui.replacen("panel_alpha = 0.85", "panel_alpha = 0.0", 1);
    let err = load_ui(&clear).expect_err("clear panel");
    assert!(err.contains("panel_alpha"), "{err}");
    let missing_alpha = base.ui.replacen("panel_alpha = 0.85\n", "", 1);
    let err = load_ui(&missing_alpha).expect_err("missing panel alpha");
    assert!(err.contains("panel_alpha"), "{err}");
    let tiny = base.ui.replacen("stepper_min_px = 8.0", "stepper_min_px = 0.0", 1);
    let err = load_ui(&tiny).expect_err("zero stepper");
    assert!(err.contains("stepper_min_px"), "{err}");
    let wide = base.ui.replacen("stepper_width_divisor = 5", "stepper_width_divisor = 0", 1);
    let err = load_ui(&wide).expect_err("zero stepper cap");
    assert!(err.contains("stepper_width_divisor"), "{err}");
    let border = base.ui.replacen("stepper_border_divisor = 8", "stepper_border_divisor = 1", 1);
    let err = load_ui(&border).expect_err("thin stepper border");
    assert!(err.contains("stepper_border_divisor"), "{err}");
    for key in ["stepper_min_px = 8.0\n", "stepper_width_divisor = 5\n", "stepper_border_divisor = 8\n"] {
        let missing = base.ui.replacen(key, "", 1);
        let err = load_ui(&missing).expect_err(key);
        let name = key.split_whitespace().next().unwrap();
        assert!(err.contains(name), "{key}: {err}");
    }
    let negative = base.ui.replacen("leading = 2.0", "leading = -1.0", 1);
    let err = load_ui(&negative).expect_err("negative leading");
    assert!(err.contains("leading"), "{err}");
    let missing = base.ui.replacen("inset_x = 12.0\n", "", 1);
    let err = load_ui(&missing).expect_err("missing inset");
    assert!(err.contains("inset_x"), "{err}");
    let glyph = base.ui.replacen("title = \"PROJECT GEMINI\"", "title = \"PROJECT GEMINI?\"", 1);
    let err = load_ui(&glyph).expect_err("question mark");
    assert!(err.contains("title"), "{err}");
    assert!(err.contains("'?'"), "{err}");
    let hero = base.hero.replacen("name = \"Nyra Sol\"", "name = \"Nyra_Sol\"", 1);
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
        coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
        heroes_visual: base.heroes_visual,
        arena_visual: base.arena_visual,
    })
    .expect_err("underscore in a hero name");
    assert!(err.contains("nyra"), "{err}");
    assert!(err.contains("'_'"), "{err}");

    let tiers = base.difficulty.replacen("id=\"normal\"", "id=\"normal?\"", 1);
    let ui = base.ui.replacen("default_difficulty = \"normal\"", "default_difficulty = \"normal?\"", 1);
    assert_ne!(tiers, base.difficulty);
    let err = Content::parse(&gemini_sim::content::Sources {
        difficulty: &tiers,
        ui: &ui,
        ..sources_with(&base, base.coop)
    })
    .expect_err("tier id");
    assert!(err.contains("normal?"), "{err}");
    assert!(err.contains("'?'"), "{err}");

    let rarity = format!("{}\n[mythic_tier]\ntease = \"x\"\nsting = \"y\"\n", base.crate_reveal);
    let err = Content::parse(&gemini_sim::content::Sources {
        crate_reveal: &rarity,
        ..sources_with(&base, base.coop)
    })
    .expect_err("rarity name");
    assert!(err.contains("mythic_tier"), "{err}");
    assert!(err.contains("'_'"), "{err}");
}

#[test]
fn muzzle_flash_rejects_nan_and_non_positive() {
    let base = gemini_sim::content::embedded_sources();
    let cases = [
        ("muzzle_flash_secs = 0.05", "muzzle_flash_secs = nan", "muzzle_flash_secs"),
        ("muzzle_flash_secs = 0.05", "muzzle_flash_secs = 0.0", "muzzle_flash_secs"),
        ("muzzle_flash_radius = 0.11", "muzzle_flash_radius = -1", "muzzle_flash_radius"),
        ("muzzle_flash_core = 0.40", "muzzle_flash_core = nan", "muzzle_flash_core"),
        ("muzzle_flash_falloff = 1.65", "muzzle_flash_falloff = 0.0", "muzzle_flash_falloff"),
        ("muzzle_flash_tip_px = 5.0", "muzzle_flash_tip_px = nan", "muzzle_flash_tip_px"),
    ];
    for (from, to, key) in cases {
        let hero = base.hero.replace(from, to);
        let sources = gemini_sim::content::Sources {
            hero: &hero,
            ..sources_with(&base, base.coop)
        };
        let err = Content::parse(&sources).expect_err(key);
        assert!(err.contains(key), "{key}: {err}");
    }
    let hero = &Content::embedded().hero;
    assert!((hero.muzzle_flash_secs - 0.05).abs() < 1e-4);
    assert!((hero.muzzle_flash_tip_px - 5.0).abs() < 1e-4);
    assert!(hero.muzzle_flash_core > 0.0 && hero.muzzle_flash_core <= 1.0);
}

fn sources_present<'a>(base: &'a gemini_sim::content::Sources<'a>, presentation: &'a str) -> gemini_sim::content::Sources<'a> {
    gemini_sim::content::Sources {
        presentation,
        ..sources_with(base, base.coop)
    }
}

fn sources_with<'a>(base: &'a gemini_sim::content::Sources<'a>, coop: &'a str) -> gemini_sim::content::Sources<'a> {
    gemini_sim::content::Sources {
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
        bugs: base.bugs,
        crate_reveal: base.crate_reveal,
        coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
        heroes_visual: base.heroes_visual,
        arena_visual: base.arena_visual,
    }
}

fn quiet_breather(sim: &mut Sim) {
    sim.enemies.clear();
    sim.pending_vents.clear();
    sim.director.pool = 0.0;
    sim.director.parked = true;
    sim.phase = Phase::Breather { left: 10_000.0 };
}

#[test]
fn join_in_progress_waits_for_breather_and_matches_lowest() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 5);
    sim.players[0].upgrades.insert("rapid_cells".into(), 1);
    sim.players[0].upgrades.insert("hollow_points".into(), 1);
    let token = sim.request_join(&content).expect("queue");
    assert!(sim.pending_joins.iter().any(|join| join.token == token));
    assert!(sim.players.iter().all(|player| player.token != token));

    sim.phase = Phase::Upgrade {
        options: vec!["servo_legs".into(), "rapid_cells".into(), "hollow_points".into()],
    };
    sim.step(
        &content,
        PlayerInput {
            upgrade_choice: Some(0),
            ..PlayerInput::default()
        },
    );
    assert!(matches!(sim.phase, Phase::Breather { .. }));
    let joiner = sim.players.iter().find(|player| player.token == token).expect("spawned");
    let lowest = sim.players[0].upgrades.values().sum::<u32>();
    assert_eq!(joiner.catchup_left, lowest);
    assert_eq!(joiner.catchup_options.len(), content.coop.catchup_pick_count as usize);
    assert!(lowest >= 2, "host should still be the lowest stack count");

    sim.choose_catchup(&content, token, 0).unwrap();
    let joiner = sim.players.iter().find(|player| player.token == token).unwrap();
    assert_eq!(joiner.catchup_left, lowest - 1);
    assert_eq!(joiner.catchup_options.len(), content.coop.catchup_pick_count as usize);
    assert_eq!(joiner.upgrades.values().sum::<u32>(), 1);

    sim.phase = Phase::Breather { left: 0.01 };
    sim.step(&content, PlayerInput::default());
    assert_eq!(sim.player_count, sim.connected_count() as u32);
    assert_eq!(sim.player_count, 2);
}

#[test]
fn reconnect_holds_the_slot_then_expires() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 9);
    quiet_breather(&mut sim);
    sim.players[0].pos = Vec3::new(4.0, 4.0, -6.0);
    let token = sim.request_join(&content).unwrap();
    {
        let joiner = sim.players.iter_mut().find(|player| player.token == token).unwrap();
        joiner.upgrades.insert("servo_legs".into(), 1);
        joiner.hp = 1.0;
        joiner.downed = true;
    }
    sim.disconnect_token(&content, token).unwrap();
    {
        let joiner = sim.players.iter().find(|player| player.token == token).unwrap();
        assert!(!joiner.connected);
        assert!((joiner.hold_left - content.coop.slot_hold_seconds).abs() < 1e-3);
        assert_eq!(joiner.upgrades.get("servo_legs"), Some(&1));
        assert!(joiner.downed);
    }
    sim.step(&content, PlayerInput::default());
    let held = sim.players.iter().find(|player| player.token == token).unwrap().hold_left;
    assert!(held < content.coop.slot_hold_seconds);

    let seat = sim.reconnect_token(&content, token).unwrap();
    let joiner = &sim.players[seat];
    assert!(joiner.connected);
    assert!(!joiner.downed);
    assert!(joiner.alive);
    assert!((joiner.hp - joiner.max_hp).abs() < 1e-2);
    assert!((joiner.pos.x - 4.0).abs() < 1e-3);
    assert!((joiner.pos.z + 6.0).abs() < 1e-3);
    assert_eq!(joiner.upgrades.get("servo_legs"), Some(&1));

    sim.credits.insert(token, vec![1]);
    sim.disconnect_token(&content, token).unwrap();
    let steps = (content.coop.slot_hold_seconds / gemini_sim::SIM_DT).ceil() as u32 + 1;
    for _ in 0..steps {
        sim.step(&content, PlayerInput::default());
    }
    assert!(sim.players.iter().all(|player| player.token != token));
    assert_eq!(sim.credits.get(&token), Some(&vec![1]));
    assert!(sim.players.iter().any(|player| player.is_host && player.connected));
    assert!(matches!(sim.phase, Phase::Breather { .. }));
}

#[test]
fn client_with_a_silent_host_banners_at_3s_and_ends_at_60s_keeping_credits() {
    use gemini_sim::audio_mix::db_to_linear;
    use gemini_sim::host_away::HostAwayClock;
    use gemini_sim::sim::MusicMoment;

    let content = Content::embedded();
    for id in ["host_away", "host_back"] {
        let row = content.sfx(id).unwrap_or_else(|| panic!("{id} mix row"));
        assert_eq!(row.path, format!("audio/sfx/{id}.ogg"));
        assert!(!row.crowd);
        assert!(row.pitch_semitones.abs() < 1e-6);
        assert!((row.volume - db_to_linear(-8.0)).abs() < 1e-4);
        if id == "host_back" {
            let sync = row.sync_at.expect("host_back sync_at");
            assert!((sync - content.coop.host_away.resume_countdown_seconds).abs() <= gemini_sim::SIM_DT);
        }
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .join(&row.path);
        assert!(file.is_file(), "{}", file.display());
    }

    let mut sim = Sim::new(&content, "normal", 8);
    quiet_breather(&mut sim);
    sim.request_join(&content).expect("second player");
    assert!(sim.connected_count() > 1);
    sim.phase = Phase::Combat;
    sim.director.parked = true;
    sim.director.pool = 0.0;
    sim.director.queue = 12.0;
    let token = sim.players[0].token;
    let credited = sim.credits.get(&token).cloned().unwrap();
    sim.players[0].ability_cd = 8.0;
    sim.players[0].downed = true;
    sim.players[0].bleedout = 4.0;
    let mut enemy = bug(gemini_sim::content::EnemyKind::Swarmer);
    enemy.pos = Vec3::new(2.0, 0.0, 4.0);
    enemy.vel = Vec3::new(4.0, 0.0, 0.0);
    enemy.warn_left = 1.4;
    sim.enemies.push(enemy);

    let rules = &content.coop.host_away;
    let mut packet = HostAwayClock::new(false);
    packet.note_away_packet(rules.detect_after_seconds);
    assert!(!packet.sample(rules).banner, "the packet alone does not raise the banner");
    let from_packet = packet.tick(0.0, rules, &sim);
    assert!(from_packet.banner);
    assert!((from_packet.timer_fill - (1.0 - rules.detect_after_seconds / rules.end_after_seconds)).abs() < 1e-4);

    let mut once = HostAwayClock::new(false);
    once.set_silent(true);
    assert!(once.tick(rules.end_after_seconds, rules, &sim).ended);

    let mut clock = HostAwayClock::new(false);
    clock.set_silent(true);
    let banner = clock.tick(rules.detect_after_seconds, rules, &sim);
    assert!(banner.banner);
    assert!((banner.timer_fill - (1.0 - rules.detect_after_seconds / rules.end_after_seconds)).abs() < 1e-4);
    assert!(banner.cues.iter().any(|cue| cue == "host_away"));
    assert!(!banner.ended);

    let time = sim.time;
    let tick = sim.tick;
    let pos = sim.enemies[0].pos;
    let warn = sim.enemies[0].warn_left;
    let cd = sim.players[0].ability_cd;
    let bleed = sim.players[0].bleedout;
    let queue = sim.director.queue;
    sim.host_away = true;
    sim.credits.get_mut(&token).unwrap().push(99);
    for _ in 0..40 {
        sim.step(&content, PlayerInput::default());
    }
    assert!((sim.time - time).abs() < 1e-6);
    assert_eq!(sim.tick, tick);
    assert_eq!(sim.enemies[0].pos, pos);
    assert!((sim.enemies[0].warn_left - warn).abs() < 1e-6);
    assert!((sim.players[0].ability_cd - cd).abs() < 1e-6);
    assert!((sim.players[0].bleedout - bleed).abs() < 1e-6);
    assert!((sim.director.queue - queue).abs() < 1e-6);
    assert!(matches!(sim.phase, Phase::Combat));

    let fill_before = clock.sample(rules).timer_fill;
    clock.note_snapshot(true, rules.resume_countdown_seconds);
    assert!((clock.sample(rules).timer_fill - fill_before).abs() < 1e-6);

    let ended = clock.tick(rules.end_after_seconds - rules.detect_after_seconds, rules, &sim);
    assert!(ended.ended);
    clock.finish(&mut sim);
    clock.finish(&mut sim);
    assert!(matches!(sim.phase, Phase::Defeat));
    assert_eq!(sim.credits.get(&token).unwrap(), &credited);
    assert!(sim.players.iter().any(|player| player.is_host && player.connected));
    assert_eq!(sim.music.iter().filter(|moment| matches!(moment, MusicMoment::MatchEnd)).count(), 1);
    clock.set_page_hidden(true);
    assert!(clock.tick(0.0, rules, &sim).cues.is_empty());
}

#[test]
fn solo_host_hidden_five_minutes_resumes_intact() {
    use gemini_sim::host_away::HostAwayClock;

    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 3);
    quiet_breather(&mut sim);
    sim.phase = Phase::Combat;
    sim.director.parked = true;
    let before = sim.to_bytes().expect("bytes");
    let rules = &content.coop.host_away;
    let mut clock = HostAwayClock::new(true);
    clock.set_silent(true);
    clock.set_page_hidden(true);
    let hidden = clock.tick(3.0, rules, &sim);
    assert!(hidden.banner);
    clock.set_solo(false);
    let still = clock.tick(300.0, rules, &sim);
    assert!(!still.ended);
    assert!(still.banner);
    assert!(still.digit.is_none());
    assert_eq!(sim.to_bytes().expect("bytes"), before);

    clock.host_shown(rules.resume_countdown_seconds);
    let back = clock.tick(0.0, rules, &sim);
    assert_eq!(back.cues, vec!["host_back".to_string()]);
    assert_eq!(back.digit, Some(3));
    assert!(back.digit_u.abs() < 1e-4);
    assert!(!back.banner);
    assert!(back.frozen);
    let done = clock.tick(rules.resume_countdown_seconds, rules, &sim);
    assert!(!done.frozen);
    assert!(!done.ended);
    assert_eq!(sim.to_bytes().expect("bytes"), before);
    let time = sim.time;
    sim.step(&content, PlayerInput::default());
    assert!(sim.time > time);
}

#[test]
fn host_back_fires_once_at_countdown_start() {
    use gemini_sim::host_away::HostAwayClock;

    let content = Content::embedded();
    let sim = Sim::new(&content, "normal", 6);
    let rules = &content.coop.host_away;
    let mut clock = HostAwayClock::new(false);
    clock.set_silent(true);
    clock.tick(rules.detect_after_seconds, rules, &sim);
    clock.host_shown(rules.resume_countdown_seconds);
    let start = clock.tick(0.0, rules, &sim);
    assert_eq!(start.cues, vec!["host_back".to_string()]);
    assert_eq!(start.digit, Some(3));
    assert!(start.digit_u.abs() < 1e-4);
    let again = clock.tick(0.0, rules, &sim);
    assert!(again.cues.is_empty());
    let next = clock.tick(1.0, rules, &sim);
    assert_eq!(next.digit, Some(2));
    assert!(next.cues.is_empty());

    let mut from_snap = HostAwayClock::new(false);
    from_snap.set_silent(true);
    from_snap.tick(rules.detect_after_seconds, rules, &sim);
    from_snap.set_silent(false);
    from_snap.note_snapshot(false, rules.resume_countdown_seconds);
    let snapped = from_snap.tick(0.0, rules, &sim);
    assert_eq!(snapped.cues, vec!["host_back".to_string()]);
    assert_eq!(snapped.digit, Some(3));

    clock.set_page_hidden(true);
    let paused = clock.tick(0.0, rules, &sim);
    assert!(paused.banner);
    assert!(paused.digit.is_none());
    assert!(paused.cues.iter().any(|cue| cue == "host_away"));
    assert!(paused.stops.iter().any(|stop| stop == "host_back"));
}

#[test]
fn on_host_visibility_changed_is_the_web_and_native_clock() {
    use gemini_sim::host_away::on_host_visibility_changed;

    let content = Content::embedded();
    let sim = Sim::new(&content, "normal", 6);
    let rules = &content.coop.host_away;
    let mut clock = gemini_sim::host_away::HostAwayClock::new(false);

    on_host_visibility_changed(&mut clock, true, 0.0, rules, &sim);
    let away = on_host_visibility_changed(&mut clock, true, 50.0, rules, &sim);
    assert!(away.frame.banner, "50s hidden raises the banner");
    assert!(!away.frame.ended);
    assert!((clock.silence() - 50.0).abs() < 1e-3);
    assert!(away.send_host_away);

    let back = on_host_visibility_changed(&mut clock, false, 50.0, rules, &sim);
    assert!(back.frame.cues.iter().any(|cue| cue == "host_back"));
    assert_eq!(back.frame.digit, Some(3));
    assert!((clock.silence() - 50.0).abs() < 1e-3);

    let again = on_host_visibility_changed(&mut clock, true, 50.0, rules, &sim);
    assert!(again.frame.stops.iter().any(|stop| stop == "host_back"));
    assert!(again.frame.banner);
    assert!(again.frame.digit.is_none());
    assert!((clock.silence() - 50.0).abs() < 1e-3, "re-hide keeps the away timer");

    let ended = on_host_visibility_changed(&mut clock, true, 60.0, rules, &sim);
    assert!(ended.frame.ended, "the remaining 10s finishes the 60s timer");
}

#[test]
fn away_timer_resets_only_after_continuous_play() {
    use gemini_sim::host_away::on_host_visibility_changed;

    let content = Content::embedded();
    let sim = Sim::new(&content, "normal", 6);
    let rules = &content.coop.host_away;
    let mut clock = gemini_sim::host_away::HostAwayClock::new(false);

    on_host_visibility_changed(&mut clock, true, 0.0, rules, &sim);
    on_host_visibility_changed(&mut clock, true, 50.0, rules, &sim);
    on_host_visibility_changed(&mut clock, false, 50.0, rules, &sim);
    let live = on_host_visibility_changed(&mut clock, false, 53.0, rules, &sim);
    assert!(!live.frame.frozen, "the 3s countdown ends");
    assert!((clock.silence() - 50.0).abs() < 1e-3, "countdown does not clear silence");
    on_host_visibility_changed(&mut clock, false, 63.0, rules, &sim);
    assert!(clock.silence().abs() < 1e-3, "10s of play clears the timer");
    on_host_visibility_changed(&mut clock, true, 63.0, rules, &sim);
    let later = on_host_visibility_changed(&mut clock, true, 73.0, rules, &sim);
    assert!(!later.frame.ended);
    assert!((clock.silence() - 10.0).abs() < 1e-2);
}

#[test]
fn a_hidden_host_freezes_bleedout_revive_and_the_breather() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 6);
    sim.phase = Phase::Breather { left: 8.0 };
    sim.players[0].downed = true;
    sim.players[0].hp = 0.0;
    sim.players[0].bleedout = 12.0;
    sim.players[0].revive_progress = 1.25;
    sim.players[0].ability_cd = 4.5;
    sim.host_away = true;
    let bleed = sim.players[0].bleedout;
    let revive = sim.players[0].revive_progress;
    let cd = sim.players[0].ability_cd;
    let inputs = [PlayerInput {
        interact_held: true,
        ..PlayerInput::default()
    }];
    for _ in 0..30 {
        sim.step_peers(&content, &inputs);
    }
    assert!((sim.players[0].bleedout - bleed).abs() < 1e-6);
    assert!((sim.players[0].revive_progress - revive).abs() < 1e-6);
    assert!((sim.players[0].ability_cd - cd).abs() < 1e-6);
    match sim.phase {
        Phase::Breather { left } => assert!((left - 8.0).abs() < 1e-6),
        other => panic!("breather moved to {other:?}"),
    }
}

#[test]
fn countdown_at_one_leaves_positions_and_timers() {
    use gemini_sim::host_away::on_host_visibility_changed;

    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 6);
    sim.phase = Phase::Breather { left: 8.0 };
    sim.players[0].pos = Vec3::new(3.0, 1.0, 4.0);
    sim.players[0].vel = Vec3::ZERO;
    sim.players[0].ability_cd = 4.5;
    sim.players[0].primary.cooldown = 0.35;
    sim.director.spawn_wait = 1.25;
    let rules = &content.coop.host_away;
    let mut clock = gemini_sim::host_away::HostAwayClock::new(false);
    on_host_visibility_changed(&mut clock, true, 0.0, rules, &sim);
    on_host_visibility_changed(&mut clock, true, 50.0, rules, &sim);
    on_host_visibility_changed(&mut clock, false, 50.0, rules, &sim);
    let frame = on_host_visibility_changed(&mut clock, false, 52.0, rules, &sim);
    assert_eq!(frame.frame.digit, Some(1), "two seconds of a three-second countdown shows 1");
    assert!((frame.frame.look - 1.0 / rules.resume_countdown_seconds).abs() < 1e-3);
    assert!(frame.frame.frozen);

    let pos = sim.players[0].pos;
    let cd = sim.players[0].ability_cd;
    let ammo = sim.players[0].primary.ammo;
    let cooldown = sim.players[0].primary.cooldown;
    let tick = sim.tick;
    let time = sim.time;
    let wait = sim.director.spawn_wait;
    sim.present_countdown = true;
    let input = PlayerInput {
        move_x: 1.0,
        move_z: 1.0,
        yaw: 1.2,
        pitch: -0.25,
        fire: true,
        ability: true,
        jump_pressed: true,
        sprint: true,
        ..PlayerInput::default()
    };
    for _ in 0..30 {
        sim.step_peers(&content, &[input]);
    }
    assert!((sim.players[0].pos.x - pos.x).abs() < 1e-6);
    assert!((sim.players[0].pos.y - pos.y).abs() < 1e-6);
    assert!((sim.players[0].pos.z - pos.z).abs() < 1e-6);
    assert!((sim.players[0].ability_cd - cd).abs() < 1e-6);
    assert_eq!(sim.players[0].primary.ammo, ammo);
    assert!((sim.players[0].primary.cooldown - cooldown).abs() < 1e-6);
    assert_eq!(sim.tick, tick);
    assert!((sim.time - time).abs() < 1e-6);
    assert!((sim.director.spawn_wait - wait).abs() < 1e-6);
    match sim.phase {
        Phase::Breather { left } => assert!((left - 8.0).abs() < 1e-6),
        other => panic!("phase moved to {other:?}"),
    }
}

#[test]
fn countdown_input_changes_the_camera_only() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 4);
    sim.players[0].pos = Vec3::new(2.0, 0.0, -3.0);
    sim.players[0].yaw = 0.15;
    sim.players[0].pitch = 0.05;
    sim.players[0].ability_cd = 2.0;
    let ammo = sim.players[0].primary.ammo;
    let pos = sim.players[0].pos;
    let tick = sim.tick;
    let time = sim.time;
    sim.present_countdown = true;
    let input = PlayerInput {
        move_x: 1.0,
        move_z: -1.0,
        yaw: 1.35,
        pitch: -0.4,
        fire: true,
        ability: true,
        jump_pressed: true,
        reload: true,
        interact_held: true,
        sprint: true,
        ..PlayerInput::default()
    };
    let aimed = gemini_sim::sim::countdown_aim(input);
    assert!(!aimed.fire);
    assert!(!aimed.ability);
    assert!(!aimed.jump_pressed);
    assert_eq!(aimed.move_x, 0.0);
    assert_eq!(aimed.move_z, 0.0);
    assert!((aimed.yaw - input.yaw).abs() < 1e-6);
    sim.step_peers(&content, &[input]);
    assert!((sim.players[0].yaw - input.yaw).abs() < 1e-4, "aim follows the look");
    assert!((sim.players[0].pitch - input.pitch).abs() < 1e-4);
    assert!((sim.players[0].pos.x - pos.x).abs() < 1e-6);
    assert!((sim.players[0].pos.z - pos.z).abs() < 1e-6);
    assert_eq!(sim.players[0].primary.ammo, ammo);
    assert!((sim.players[0].ability_cd - 2.0).abs() < 1e-6);
    assert_eq!(sim.tick, tick);
    assert!((sim.time - time).abs() < 1e-6);
    assert!(sim.projectiles.is_empty());
}

#[test]
fn away_mix_ducks_music_and_keeps_the_beat() {
    use gemini_sim::music::{away_mix, StemMixer};

    let content = Content::embedded();
    let duck_db = content.presentation.hud.away.music_duck_db;
    let duck_hz = content.presentation.hud.away.music_duck_lowpass_hz;
    let ducked = away_mix(1.0, duck_db, duck_hz);
    let full = 10f32.powf(duck_db / 20.0);
    assert!((ducked.gain - full).abs() < 1e-3, "{}", ducked.gain);
    assert!((ducked.lowpass_hz - duck_hz).abs() < 1.0);
    let open = away_mix(0.0, duck_db, duck_hz);
    assert!((open.gain - 1.0).abs() < 1e-4);
    assert!(!open.lowpass_hz.is_finite(), "look 0 bypasses the filter");
    let at_one = away_mix(1.0 / content.coop.host_away.resume_countdown_seconds, duck_db, duck_hz);
    assert!(at_one.gain > ducked.gain && at_one.gain < open.gain);
    assert!(at_one.lowpass_hz > ducked.lowpass_hz && at_one.lowpass_hz.is_finite());

    let track = content.track("combat_01").expect("combat_01");
    let loops = vec![8.0; track.layer.len().max(1)];
    let mut mixer = StemMixer::start(track, &loops, 0.8);
    for _ in 0..30 {
        let _mix = away_mix(1.0, duck_db, duck_hz);
        mixer.advance(track, gemini_sim::SIM_DT, 0.8, 4, None);
    }
    assert!(
        (mixer.voices[0].elapsed - 1.0).abs() < 1e-3,
        "the beat clock keeps moving and does not restart, elapsed {}",
        mixer.voices[0].elapsed
    );

    let low = sine_rms(120.0, duck_hz);
    let high = sine_rms(4000.0, duck_hz);
    assert!(low > high * 3.0, "lowpass keeps {low} and cuts {high}");
    let high_open = sine_rms(4000.0, f32::INFINITY);
    assert!(high_open > 0.5, "{high_open}");

    let back = content.sfx("host_back").expect("host_back");
    assert!((back.sync_at.expect("sync_at") - content.coop.host_away.resume_countdown_seconds).abs() <= gemini_sim::SIM_DT);
}

fn sine_rms(freq: f32, cutoff: f32) -> f32 {
    let mut lowpass = gemini_sim::music::Lowpass::new();
    let rate = 44_100.0;
    let start = 2_205usize;
    let n = 4_410usize;
    let mut acc = 0.0;
    for i in 0..n {
        let t = i as f32 / rate;
        let sample = (t * freq * std::f32::consts::TAU).sin();
        let y = lowpass.process(sample, cutoff, rate);
        if i >= start {
            acc += y * y;
        }
    }
    (acc / (n - start) as f32).sqrt()
}

#[test]
fn host_leave_ends_the_match_and_keeps_credits() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 4);
    let token = sim.players[0].token;
    assert!(sim.credits.get(&token).unwrap().contains(&1));
    sim.disconnect_token(&content, token).unwrap();
    assert!(matches!(sim.phase, Phase::Defeat));
    assert_eq!(sim.credits.get(&token).unwrap(), &vec![1]);
}

#[test]
fn squad_wipe_and_reviver_speed_come_from_coop() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 6);
    quiet_breather(&mut sim);
    let second = sim.request_join(&content).unwrap();
    let third = sim.request_join(&content).unwrap();
    let fourth = sim.request_join(&content).unwrap();
    sim.phase = Phase::Combat;
    sim.director.parked = true;
    sim.director.pool = 50.0;
    for player in &mut sim.players {
        player.pos = Vec3::new(1.0, 4.0, 1.0);
    }
    let downed = sim.players.iter().position(|player| player.token == fourth).unwrap();
    sim.players[downed].downed = true;
    sim.players[downed].hp = 0.0;
    let bleed = sim.players[downed].bleedout;
    let mut inputs = vec![
        PlayerInput {
            interact_held: true,
            ..PlayerInput::default()
        };
        sim.players.len()
    ];
    inputs[downed].interact_held = false;
    sim.step_peers(&content, &inputs);
    let revived = &sim.players[downed];
    let revivers = 3u32;
    let extra = revivers.saturating_sub(1).min(content.coop.max_extra_revivers);
    let speed = 1.0 + content.coop.multi_reviver_speedup * extra as f32;
    assert!((revived.revive_progress - gemini_sim::SIM_DT * speed).abs() < 1e-3, "{}", revived.revive_progress);
    assert!((revived.bleedout - bleed).abs() < 1e-3, "bleedout should pause while a teammate revives");
    assert!(revived.downed);
    let _ = (second, third);

    for player in &mut sim.players {
        player.downed = true;
        player.alive = true;
    }
    sim.step_peers(&content, &vec![PlayerInput::default(); sim.players.len()]);
    assert!(matches!(sim.phase, Phase::Defeat), "all_downed ends a squad");
}

#[test]
fn friendly_fire_off_does_not_damage_a_teammate() {
    let content = Content::embedded();
    assert!(!content.coop.friendly_fire);
    let mut sim = Sim::new(&content, "normal", 2);
    quiet_breather(&mut sim);
    let token = sim.request_join(&content).unwrap();
    sim.phase = Phase::Combat;
    sim.director.parked = true;
    sim.director.pool = 40.0;
    let joiner = sim.players.iter().position(|player| player.token == token).unwrap();
    let hp = sim.players[joiner].hp;
    sim.players[joiner].pos = sim.players[0].pos + Vec3::new(0.0, 0.0, -2.0);
    let owner = sim.players[0].id;
    sim.projectiles.push(gemini_sim::sim::Projectile {
        id: 50,
        pos: sim.players[0].pos + Vec3::new(0.0, 1.0, 0.0),
        vel: Vec3::new(0.0, 0.0, -80.0),
        damage: 80.0,
        radius: 0.3,
        team: gemini_sim::sim::Team::Player,
        ttl: 2.0,
        pierce: 0,
        bounces: 0,
        hit: Vec::new(),
        kind: gemini_sim::sim::ShotKind::Coil,
        acid_ward: false,
        owner,
        crit_mult: 1.0,
    });
    sim.step(&content, PlayerInput::default());
    let joiner = sim.players.iter().find(|player| player.token == token).unwrap();
    assert!((joiner.hp - hp).abs() < 1e-3, "hp {}", joiner.hp);
}

#[test]
fn overdrive_is_per_player_and_the_killer_gets_the_refund() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 4);
    quiet_breather(&mut sim);
    let token = sim.request_join(&content).unwrap();
    sim.phase = Phase::Combat;
    sim.director.parked = true;
    sim.director.pool = 0.0;
    sim.enemies.clear();
    let mate = sim.players.iter().position(|player| player.token == token).unwrap();
    sim.players[0].overdrive_left = 0.0;
    sim.players[0].primary.cooldown = 0.0;
    sim.players[0].primary.reload = 0.0;
    sim.players[mate].overdrive_left = content.hero.overdrive_duration;
    sim.players[mate].primary.cooldown = 0.0;
    sim.players[mate].primary.reload = 0.0;
    let host_ammo = sim.players[0].primary.ammo;
    let mate_ammo = sim.players[mate].primary.ammo;
    let mut host_fire = PlayerInput::default();
    host_fire.fire = true;
    let mut mate_fire = PlayerInput::default();
    mate_fire.fire = true;
    sim.step_peers(&content, &[host_fire, mate_fire]);
    let mate = sim.players.iter().position(|player| player.token == token).unwrap();
    assert_eq!(sim.players[0].primary.ammo, host_ammo - 1);
    assert_eq!(sim.players[mate].primary.ammo, mate_ammo, "teammate overdrive spends no ammo");
    let plain = content.weapon("pulse_rifle").unwrap().fire_interval;
    let boosted = plain / (1.0 + content.hero.overdrive_fire_rate);
    assert!((sim.players[0].primary.cooldown - plain).abs() < 1e-3);
    assert!((sim.players[mate].primary.cooldown - boosted).abs() < 1e-3);

    // The host is standing on the bug, so a nearest-player credit would pay them.
    // The teammate landed the killing hit, and only that player is refunded.
    sim.projectiles.clear();
    sim.enemies.clear();
    sim.phase = Phase::Combat;
    sim.director.parked = true;
    sim.director.pool = 0.0;
    let host_id = sim.players[0].id;
    let mate_id = sim.players[mate].id;
    sim.players[0].pos = Vec3::new(0.0, 4.0, 0.0);
    sim.players[0].overdrive_left = content.hero.overdrive_duration;
    sim.players[0].ability_cd = 10.0;
    sim.players[mate].pos = Vec3::new(18.0, 4.0, 0.0);
    sim.players[mate].overdrive_left = content.hero.overdrive_duration;
    sim.players[mate].ability_cd = 10.0;
    let mut victim = bug(gemini_sim::content::EnemyKind::Swarmer);
    victim.hp = 0.0;
    victim.alive = true;
    victim.contact = 0.0;
    victim.pos = sim.players[0].pos;
    victim.last_hit_by = mate_id;
    sim.enemies.push(victim);
    sim.step_peers(&content, &[PlayerInput::default(), PlayerInput::default()]);
    let ticked = 10.0 - gemini_sim::SIM_DT;
    let refunded = ticked - content.hero.kill_cd_refund;
    let host = sim.players.iter().find(|player| player.id == host_id).unwrap();
    let killer = sim.players.iter().find(|player| player.id == mate_id).unwrap();
    assert!((host.ability_cd - ticked).abs() < 1e-3, "assist cd {}", host.ability_cd);
    assert!((killer.ability_cd - refunded).abs() < 1e-3, "killer cd {}", killer.ability_cd);
}

#[test]
fn persistent_rank_comes_from_outside_and_the_cap_is_loaded() {
    let content = Content::embedded();
    assert_eq!(content.roster, ["vex", "nyra", "brakk", "kip"]);
    assert!(content.cosmetics.shard_shop);
    assert_eq!(content.cosmetics.shard_cost.legendary, 600);
    assert_eq!(content.cosmetics.shards_for("legendary").unwrap(), 600);
    let err = content.cosmetics.shards_for("mythic").expect_err("unknown rarity");
    assert!(err.contains("mythic"), "{err}");
    assert_eq!(content.duplicate.max_rank, 6);
    assert_eq!(content.duplicate.rank.len() as u32, content.duplicate.max_rank);

    let base = gemini_sim::content::embedded_sources();
    let missing = base
        .duplicate_ranks
        .lines()
        .filter(|line| !line.trim_start().starts_with("max_rank"))
        .collect::<Vec<_>>()
        .join("\n");
    let err = Content::parse(&sources_ranks(&base, &missing)).expect_err("missing max_rank");
    assert!(err.contains("max_rank"), "{err}");
    let short = base.duplicate_ranks.replacen("max_rank = 6", "max_rank = 5", 1);
    let err = Content::parse(&sources_ranks(&base, &short)).expect_err("max_rank must match rows");
    assert!(err.contains("max_rank"), "{err}");
    assert!(err.contains("[[rank]]"), "{err}");

    let cap = content.duplicate.max_rank;
    let mut earned = Sim::new(&content, "normal", 3);
    earned.players[0].upgrades.insert("glass_cannon".into(), 1);
    earned.apply_persistent_rank(&content, cap);
    let ranked_hp = earned.players[0].max_hp;

    let mut fresh = Sim::new(&content, "normal", 4);
    fresh.apply_persistent_rank(&content, cap + 10);
    assert!(fresh.players[0].upgrades.is_empty(), "run upgrades clear every match");
    assert_eq!(fresh.duplicate_rank, cap);
    assert!((fresh.players[0].max_hp - ranked_hp).abs() < 1e-3);
    let base_sim = Sim::new(&content, "normal", 5);
    assert!(fresh.players[0].max_hp > base_sim.players[0].max_hp);

    let mut floor = Sim::new(&content, "normal", 6);
    floor.apply_persistent_rank(&content, 0);
    assert_eq!(floor.duplicate_rank, 1);
}

#[test]
fn warnings_and_infinite_magazine_fail_closed() {
    let content = Content::embedded();
    assert!((content.warnings.low_hp_fraction - 0.30).abs() < 1e-4);
    assert!((content.warnings.low_hp_rearm_fraction - 0.40).abs() < 1e-4);
    assert!((content.warnings.low_hp_repeat_s - 1.2).abs() < 1e-4);
    assert!((content.warnings.ammo_low_fraction - 0.25).abs() < 1e-4);
    let vex = content.ability_of("vex").unwrap();
    assert!(vex.infinite_magazine);
    assert!((vex.cooldown - content.hero.ability_cooldown).abs() < 1e-4);
    for (id, cooldown) in [("nyra", 24.0), ("brakk", 18.0), ("kip", 16.0)] {
        let row = content.ability_of(id).unwrap();
        assert!(!row.infinite_magazine, "{id}");
        assert!((row.cooldown - cooldown).abs() < 1e-4, "{id}");
    }
    let base = gemini_sim::content::embedded_sources();
    let load = |hero: &str| {
        Content::parse(&gemini_sim::content::Sources {
            hero,
            ..sources_with(&base, base.coop)
        })
    };
    let missing = base.hero.replacen("infinite_magazine = false, ", "", 1);
    let err = load(&missing).expect_err("nyra magazine");
    assert!(err.contains("nyra"), "{err}");
    assert!(err.contains("infinite_magazine"), "{err}");
    let bad = base.hero.replacen("infinite_magazine = false", "infinite_magazine = \"no\"", 1);
    let err = load(&bad).expect_err("magazine type");
    assert!(err.contains("infinite_magazine"), "{err}");
    let no_table = base.hero.replacen("[warnings]\n", "", 1);
    let err = load(&no_table).expect_err("warnings");
    assert!(err.contains("warnings"), "{err}");
    let low = base.hero.replacen("low_hp_fraction = 0.30", "low_hp_fraction = 1", 1);
    let err = load(&low).expect_err("fraction");
    assert!(err.contains("low_hp_fraction"), "{err}");
    let rearm = base.hero.replacen("low_hp_rearm_fraction = 0.40", "low_hp_rearm_fraction = 0.30", 1);
    let err = load(&rearm).expect_err("rearm");
    assert!(err.contains("low_hp_rearm_fraction"), "{err}");
    let repeat = base.hero.replacen("low_hp_repeat_s = 1.2", "low_hp_repeat_s = 0", 1);
    let err = load(&repeat).expect_err("repeat");
    assert!(err.contains("low_hp_repeat_s"), "{err}");

    for id in ["revive_start", "low_hp_warn", "ammo_low_warn", "ability_ready"] {
        assert!(content.sfx(id).is_some(), "{id}");
        let header = format!("[{id}]\n");
        let start = base.audio.find(&header).unwrap_or_else(|| panic!("{id} row"));
        let rest = &base.audio[start + header.len()..];
        let end = rest.find("\n[").map(|index| start + header.len() + index + 1).unwrap_or(base.audio.len());
        let audio = format!("{}{}", &base.audio[..start], &base.audio[end..]);
        assert_ne!(audio, base.audio, "{id}");
        let err = Content::parse(&gemini_sim::content::Sources {
            audio: &audio,
            ..sources_with(&base, base.coop)
        })
        .expect_err(id);
        assert!(err.contains(id), "{err}");
        assert!(err.contains("missing"), "{err}");
    }
}

#[test]
fn hero_portraits_fail_closed() {
    let content = Content::embedded();
    let expect = [
        ("vex", "vex", 20.0, 0.0, 0.86),
        ("nyra", "nyra", -18.0, 12.0, 0.86),
        ("brakk", "brakk", 22.0, 8.0, 0.88),
        ("kip", "kip", -14.0, 10.0, 0.86),
    ];
    for (id, mesh, yaw, pitch, fit) in expect {
        let card = content.heroes.iter().find(|hero| hero.id == id).unwrap();
        assert_eq!(card.portrait.mesh, mesh);
        assert!((card.portrait.yaw_deg - yaw).abs() < 1e-4, "{id}");
        assert!((card.portrait.pitch_deg - pitch).abs() < 1e-4, "{id}");
        assert!((card.portrait.fit - fit).abs() < 1e-4, "{id}");
    }
    assert!((content.ui.layout.locked_accent_mix - 0.35).abs() < 1e-4);
    let base = gemini_sim::content::embedded_sources();
    let load = |hero: &str| {
        Content::parse(&gemini_sim::content::Sources {
            hero,
            ..sources_with(&base, base.coop)
        })
    };
    let missing = base.hero.replacen(
        "portrait = { mesh = \"nyra\", yaw_deg = -18.0, pitch_deg = 12.0, fit = 0.86 }\n",
        "",
        1,
    );
    let err = load(&missing).expect_err("nyra portrait");
    assert!(err.contains("nyra"), "{err}");
    assert!(err.contains("portrait"), "{err}");
    let unknown = base.hero.replacen("mesh = \"kip\"", "mesh = \"laser_knife\"", 1);
    let err = load(&unknown).expect_err("unknown mesh");
    assert!(err.contains("kip"), "{err}");
    assert!(err.contains("laser_knife"), "{err}");
    let steep = base.hero.replacen("pitch_deg = 12.0", "pitch_deg = 90.1", 1);
    let err = load(&steep).expect_err("pitch past a quarter turn");
    assert!(err.contains("nyra"), "{err}");
    assert!(err.contains("pitch_deg"), "{err}");
    let flipped = base.hero.replacen("pitch_deg = 0.0", "pitch_deg = -90.1", 1);
    let err = load(&flipped).expect_err("pitch past a quarter turn");
    assert!(err.contains("vex"), "{err}");
    assert!(err.contains("[-90, 90]"), "{err}");
    let mix = base.ui.replacen("locked_accent_mix = 0.35", "locked_accent_mix = 1", 1);
    let err = Content::parse(&gemini_sim::content::Sources {
        ui: &mix,
        ..sources_with(&base, base.coop)
    })
    .expect_err("mix");
    assert!(err.contains("locked_accent_mix"), "{err}");
    let blank_white = base.palette.replacen("white = \"#F2F2F2\"\n", "", 1);
    let err = Content::parse(&gemini_sim::content::Sources {
        palette: &blank_white,
        ..sources_with(&base, base.coop)
    })
    .expect_err("missing hero_white");
    assert!(err.contains("hero_white"), "{err}");
    assert!(err.contains("does not resolve"), "{err}");

    let dress = &content.arena_dressing;
    assert!((dress.rail_height - 0.45).abs() < 1e-4);
    assert!(dress.rail_height <= 0.5);
    assert!((dress.ramp_gap_dot - 0.86).abs() < 1e-4);
    assert!((dress.review_distance - 15.0).abs() < 1e-4);
    assert!((dress.review_look_height - 0.45).abs() < 1e-4);
    assert!((dress.review_ground_y - 0.02).abs() < 1e-4);
    let tall = base.arena_visual.replacen("rail_height = 0.45", "rail_height = 0.8", 1);
    let err = Content::parse(&gemini_sim::content::Sources {
        arena_visual: &tall,
        ..sources_with(&base, base.coop)
    })
    .expect_err("tall curb");
    assert!(err.contains("rail_height"), "{err}");
    let rocks = format!("{}\n[[rock]]\nat = [18.0, 14.0]\nsize = [1.0, 1.0, 1.0]\n", base.arena_visual);
    let err = Content::parse(&gemini_sim::content::Sources {
        arena_visual: &rocks,
        ..sources_with(&base, base.coop)
    })
    .expect_err("rocks");
    assert!(err.contains("rock"), "{err}");
    let spare = base.heroes_visual.replacen("[[mesh]]", "spare = 1\n[[mesh]]", 1);
    let err = Content::parse(&gemini_sim::content::Sources {
        heroes_visual: &spare,
        ..sources_with(&base, base.coop)
    })
    .expect_err("unknown visual key");
    assert!(err.contains("spare"), "{err}");
    let bad_color = base.heroes_visual.replacen("color = \"hero_white\"", "color = \"not_a_color\"", 1);
    let err = Content::parse(&gemini_sim::content::Sources {
        heroes_visual: &bad_color,
        ..sources_with(&base, base.coop)
    })
    .expect_err("bad color");
    assert!(err.contains("not_a_color"), "{err}");
}

fn sources_ranks<'a>(
    base: &'a gemini_sim::content::Sources<'a>,
    duplicate_ranks: &'a str,
) -> gemini_sim::content::Sources<'a> {
    gemini_sim::content::Sources {
        waves: base.waves,
        difficulty: base.difficulty,
        controller: base.controller,
        run_upgrades: base.run_upgrades,
        endless: base.endless,
        currency: base.currency,
        banners: base.banners,
        duplicate_ranks,
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
        coop: base.coop,
        cosmetics: base.cosmetics,
        ui: base.ui,
        heroes_visual: base.heroes_visual,
        arena_visual: base.arena_visual,
    }
}
