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
    });
    sim.step(content, PlayerInput::default());
}

#[test]
fn mixer_caps_voices_and_keeps_the_charge_cue() {
    let audio = &Content::embedded().audio;
    let mut events = Vec::new();
    for i in 0..40 {
        events.push(SoundEvent {
            id: "shot".into(),
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
    let shot = content.sfx("shot").unwrap();
    assert!(shot.volume > 0.0);
    assert!(content.pitch_range("shot").is_some());
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
    spit.vel.y = 1.0;
    spit.weak_mult = content.bug(EnemyKind::Spitter).hitbox.weak().unwrap().mult;
    let sac = content.bug(EnemyKind::Spitter).hitbox.weak().unwrap().offset;
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
    let cracks = content.bug(EnemyKind::Brute).hitbox.weak().unwrap().offset;
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
    let tail = content.bug(EnemyKind::HiveTyrant).hitbox.tail.as_ref().unwrap().offset;
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
