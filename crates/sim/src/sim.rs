//! Tick the match. Everything that changes gameplay state lives here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::audio_mix::SoundEvent;
use crate::content::{
    body_lift_of, hit_radius_of, wave_budget, weakpoint_of, Content, EnemyKind, WeaponDef,
};
use crate::content::{endless_hp_dmg_mult, rank_bonus};
use crate::math::{
    flat_forward, flat_right, look_dir, rotate_y, segment_hits_aabb, segment_hits_sphere, Vec3,
};
use crate::rng::Rng;
use crate::spatial::SpatialGrid;
use crate::upgrade::{self, Modifiers};

pub const SIM_HZ: u32 = 30;
pub const SIM_DT: f32 = 1.0 / SIM_HZ as f32;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlayerInput {
    pub move_x: f32,
    pub move_z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub look_dx_deg: f32,
    pub look_dy_deg: f32,
    pub jump_pressed: bool,
    pub sprint: bool,
    pub crouch: bool,
    pub boost_held: bool,
    pub boost_pressed: bool,
    pub fire: bool,
    pub reload: bool,
    pub ability: bool,
    pub interact_held: bool,
    pub upgrade_choice: Option<u8>,
    pub using_gamepad: bool,
}

impl Default for PlayerInput {
    fn default() -> Self {
        Self {
            move_x: 0.0,
            move_z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            look_dx_deg: 0.0,
            look_dy_deg: 0.0,
            jump_pressed: false,
            sprint: false,
            crouch: false,
            boost_held: false,
            boost_pressed: false,
            fire: false,
            reload: false,
            ability: false,
            interact_held: false,
            upgrade_choice: None,
            using_gamepad: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Phase {
    Combat,
    Upgrade { options: Vec<String> },
    Breather { left: f32 },
    Victory,
    Defeat,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Director {
    pub wave: u32,
    pub pool: f32,
    pub queue: f32,
    pub wave_budget: f32,
    pub parked: bool,
    pub spawn_wait: f32,
    pub group_left: f32,
    pub dropped: f32,
    /// Enemies spawned during the current wave, including the boss. HUD fill uses this.
    pub spawned_this_wave: u32,
}

#[derive(Clone, Debug)]
pub enum MusicMoment {
    MatchStart,
    MatchEnd,
    BossIntro { boss_id: String },
    BossDown { boss_id: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gun {
    pub weapon_id: String,
    pub ammo: u32,
    pub reserve: u32,
    pub mag_size: u32,
    pub cooldown: f32,
    pub reload: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub id: u32,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub shield: f32,
    pub max_shield: f32,
    pub shield_delay: f32,
    pub fuel: f32,
    pub fuel_delay: f32,
    pub coyote: f32,
    pub jump_buffer: f32,
    pub grounded: bool,
    pub sprinting: bool,
    pub was_sprinting: bool,
    pub sprint_fire_lock: f32,
    pub crouching: bool,
    pub downed: bool,
    pub bleedout: f32,
    pub self_revives_used: u32,
    pub self_revive_held: f32,
    pub invuln: f32,
    pub revive_progress: f32,
    pub primary: Gun,
    pub sidearm: Gun,
    pub upgrades: BTreeMap<String, u32>,
    pub last_stand_wave: u32,
    pub ability_cd: f32,
    pub kills_since_nova: u32,
    pub drone_angle: f32,
    pub alive: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AiState {
    Chase,
    Telegraph { left: f32 },
    Lunge { left: f32 },
    Recover { left: f32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Enemy {
    pub id: u32,
    pub kind: EnemyKind,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub speed: f32,
    pub contact: f32,
    pub range: f32,
    pub weak_mult: f32,
    pub attack_cd: f32,
    pub ai: AiState,
    pub weak_exposed: bool,
    /// Seconds the brute's cracks stay open after a charge. 0 while the shell is closed.
    #[serde(default)]
    pub open_left: f32,
    /// Set for the tick the shell opens, so audio can emit `shell_open` once.
    #[serde(default)]
    pub just_opened: bool,
    /// Brood pod HP. Empty on bugs that have no pods. Independent of the mesh.
    #[serde(default)]
    pub pods: Vec<f32>,
    pub phase: u8,
    pub spawn_acc: f32,
    pub spawn_kind: EnemyKind,
    pub spawn_count: u32,
    pub spawn_interval: f32,
    pub alive: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Team {
    Player,
    Bug,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ShotKind {
    Coil,
    Sidearm,
    Acid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Projectile {
    pub id: u32,
    pub pos: Vec3,
    pub vel: Vec3,
    pub damage: f32,
    pub radius: f32,
    pub team: Team,
    pub ttl: f32,
    pub pierce: u32,
    pub bounces: u32,
    pub hit: Vec<u32>,
    pub kind: ShotKind,
    pub acid_ward: bool,
    pub owner: u32,
    pub crit_mult: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pickup {
    pub id: u32,
    pub pos: Vec3,
    pub ammo: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sim {
    pub fingerprint: u64,
    pub seed: u64,
    pub tick: u64,
    pub time: f32,
    pub difficulty: String,
    pub player_count: u32,
    pub duplicate_rank: u32,
    pub rng: Rng,
    pub phase: Phase,
    pub players: Vec<Player>,
    pub enemies: Vec<Enemy>,
    pub projectiles: Vec<Projectile>,
    pub pickups: Vec<Pickup>,
    pub director: Director,
    pub next_id: u32,
    pub boss_kills: u32,
    pub endless_mode: bool,
    #[serde(skip)]
    pub spatial: SpatialGrid,
    #[serde(skip)]
    pub sounds: Vec<SoundEvent>,
    /// Charge cues to cut this tick. The client drops that owner's voice on the same frame.
    #[serde(skip)]
    pub stops: Vec<SoundStop>,
    #[serde(skip)]
    pub pings: Vec<HitPing>,
    #[serde(skip)]
    pub music: Vec<MusicMoment>,
    #[serde(skip)]
    pub landed_pulse: f32,
}

/// A floating damage number and hit flash. Weak hits are drawn larger.
#[derive(Clone, Debug)]
pub struct HitPing {
    pub pos: Vec3,
    pub amount: f32,
    pub weak: bool,
}

#[derive(Clone, Debug)]
pub struct SoundStop {
    pub id: String,
    pub owner: u32,
}

impl Sim {
    pub fn new(content: &Content, difficulty: &str, seed: u64) -> Self {
        let mut sim = Self {
            fingerprint: content.fingerprint,
            seed,
            tick: 0,
            time: 0.0,
            difficulty: difficulty.to_string(),
            player_count: 1,
            duplicate_rank: 1,
            rng: Rng::new(seed),
            phase: Phase::Combat,
            players: Vec::new(),
            enemies: Vec::new(),
            projectiles: Vec::new(),
            pickups: Vec::new(),
            director: Director {
                wave: 0,
                pool: 0.0,
                queue: 0.0,
                wave_budget: 0.0,
                parked: false,
                spawn_wait: 0.0,
                group_left: 0.0,
                dropped: 0.0,
                spawned_this_wave: 0,
            },
            next_id: 1,
            boss_kills: 0,
            endless_mode: false,
            spatial: SpatialGrid::new(content.combat.cell_size, content.arena.half_extent),
            sounds: Vec::new(),
            stops: Vec::new(),
            pings: Vec::new(),
            music: Vec::new(),
            landed_pulse: 0.0,
        };
        let player = sim.make_player(content);
        sim.players.push(player);
        sim.music.push(MusicMoment::MatchStart);
        sim.begin_wave(content);
        sim
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    pub fn restore(content: &Content, bytes: &[u8]) -> Result<Self, String> {
        let mut sim: Sim = bincode::deserialize(bytes).map_err(|err| err.to_string())?;
        if sim.fingerprint != content.fingerprint {
            return Err("content fingerprint does not match this build".into());
        }
        sim.spatial = SpatialGrid::new(content.combat.cell_size, content.arena.half_extent);
        sim.sounds.clear();
        sim.stops.clear();
        sim.pings.clear();
        sim.music.clear();
        sim.landed_pulse = 0.0;
        Ok(sim)
    }

    pub fn take_sounds(&mut self) -> Vec<SoundEvent> {
        std::mem::take(&mut self.sounds)
    }

    pub fn take_music(&mut self) -> Vec<MusicMoment> {
        std::mem::take(&mut self.music)
    }

    pub fn enemies_left(&self) -> usize {
        self.enemies.iter().filter(|e| e.alive).count()
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    fn make_player(&mut self, content: &Content) -> Player {
        let primary = content.weapon(&content.hero.primary).expect("primary");
        let sidearm = content.weapon(&content.hero.sidearm).expect("sidearm");
        let mods = Modifiers::default();
        let max_hp = max_hp(content, &mods, self.duplicate_rank);
        let max_fuel = content.controller.boost.fuel_max;
        Player {
            id: self.alloc_id(),
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            hp: max_hp,
            max_hp,
            shield: 0.0,
            max_shield: 0.0,
            shield_delay: 0.0,
            fuel: max_fuel,
            fuel_delay: 0.0,
            coyote: content.controller.jump.coyote_time,
            jump_buffer: 0.0,
            grounded: true,
            sprinting: false,
            was_sprinting: false,
            sprint_fire_lock: 0.0,
            crouching: false,
            downed: false,
            bleedout: content.controller.downed.bleedout_time,
            self_revives_used: 0,
            self_revive_held: 0.0,
            invuln: 0.0,
            revive_progress: 0.0,
            primary: gun_from(primary),
            sidearm: gun_from(sidearm),
            upgrades: BTreeMap::new(),
            last_stand_wave: 0,
            ability_cd: 0.0,
            kills_since_nova: 0,
            drone_angle: 0.0,
            alive: true,
        }
    }

    pub fn step(&mut self, content: &Content, input: PlayerInput) {
        self.sounds.clear();
        self.stops.clear();
        self.pings.clear();
        self.music.clear();
        self.landed_pulse = 0.0;
        self.tick = self.tick.wrapping_add(1);
        self.time += SIM_DT;

        if matches!(self.phase, Phase::Victory | Phase::Defeat) {
            return;
        }

        if let Phase::Upgrade { options } = &self.phase {
            if let Some(choice) = input.upgrade_choice {
                if let Some(id) = options.get(choice as usize).cloned() {
                    self.grant_upgrade(content, &id);
                    self.phase = Phase::Breather {
                        left: content.waves.budget.breather_seconds,
                    };
                }
            }
            return;
        }

        let dt = SIM_DT;
        self.tick_player(content, input, dt);
        if !matches!(self.phase, Phase::Combat) {
            self.tick_breather(content, dt);
            return;
        }
        self.tick_enemies(content, dt);
        launch_pending_shots(self, content);
        self.rebuild_grid(content);
        self.tick_melee(content);
        self.tick_weapons(content, input, dt);
        self.tick_projectiles(content, dt);
        self.tick_drones(content, dt);
        self.tick_pickups(content);
        self.tick_boss_spawns(content);
        self.tick_spawner(content, dt);
        self.reap(content);
        self.tick_downed(content, input.interact_held, dt);
        self.emit_crowd();
        self.finish_wave(content);
    }

    fn tick_breather(&mut self, content: &Content, dt: f32) {
        if let Phase::Breather { left } = &mut self.phase {
            *left -= dt;
            if *left <= 0.0 {
                self.director.wave += 1;
                self.phase = Phase::Combat;
                self.begin_wave(content);
            }
        }
    }

    fn begin_wave(&mut self, content: &Content) {
        if self.director.wave == 0 {
            self.director.wave = 1;
        }
        let wave_n = self.director.wave;
        let budget = wave_budget(
            &content.waves.budget,
            wave_n,
            self.player_count,
            &content.endless,
            self.endless_mode,
        );
        let cap = content.waves.budget.queue_cap_waves * budget;
        let carried = self.director.queue.min(cap);
        self.director.queue = 0.0;
        self.director.dropped = 0.0;
        self.director.wave_budget = budget;
        self.director.pool = budget + carried;
        self.director.parked = false;
        self.director.spawn_wait = 0.0;
        self.director.group_left = content.combat.group_budget;
        self.director.spawned_this_wave = 0;
        for player in &mut self.players {
            player.self_revives_used = 0;
            player.self_revive_held = 0.0;
        }
        self.sound_at("wave", Vec3::ZERO);
        if let Some(def) = content.wave(wave_n) {
            if let Some(boss_id) = def.boss.clone() {
                self.spawn_boss(content, &boss_id);
            }
        }
    }

    /// Move unspent budget into the queue, capped at one wave of *this* wave's budget.
    pub fn park_budget(&mut self, queue_cap_waves: f32) {
        let cap = (queue_cap_waves * self.director.wave_budget).max(0.0);
        let queued = self.director.pool.min(cap).max(0.0);
        self.director.dropped += (self.director.pool - queued).max(0.0);
        self.director.queue = queued;
        self.director.pool = 0.0;
        self.director.parked = true;
    }

    fn tick_player(&mut self, content: &Content, input: PlayerInput, dt: f32) {
        let Some(player) = self.players.first_mut() else {
            return;
        };
        if !player.alive {
            return;
        }
        let mods = upgrade::collect(content, &player.upgrades);
        refresh_defense(content, player, &mods, self.duplicate_rank);
        apply_look(player, &input, content, &self.enemies);
        let ctl = &content.controller;
        player.crouching = input.crouch && !player.downed;
        let wish = wish_dir(player.yaw, input.move_x, input.move_z);
        let wish_len = Vec3::new(wish.x, 0.0, wish.z).length();
        let speed_mult = 1.0 + mods.move_speed_pct;
        let mut max_speed = ctl.movement.walk_speed;
        player.sprinting = false;
        if player.downed || player.crouching {
            max_speed = ctl.movement.crouch_speed;
        } else if input.sprint && player.grounded && wish_len > 0.05 {
            max_speed = ctl.movement.sprint_speed;
            player.sprinting = true;
        }
        max_speed *= speed_mult;
        if player.sprinting {
            player.was_sprinting = true;
        } else if player.was_sprinting {
            player.was_sprinting = false;
            player.sprint_fire_lock = content.weapons.sprint_to_fire_delay;
        } else {
            player.sprint_fire_lock = (player.sprint_fire_lock - dt).max(0.0);
        }

        let accel = if player.grounded {
            ctl.movement.accel_ground
        } else {
            ctl.movement.accel_air
        };
        let target = wish * max_speed;
        let hv = Vec3::new(player.vel.x, 0.0, player.vel.z);
        let delta = target - hv;
        let max_delta = accel * dt;
        let new_hv = if wish_len < 0.05 && player.grounded {
            let keep = (1.0 - ctl.movement.friction * dt).clamp(0.0, 1.0);
            hv * keep
        } else if delta.length() <= max_delta {
            target
        } else {
            hv + delta.normalized() * max_delta
        };
        player.vel.x = new_hv.x;
        player.vel.z = new_hv.z;

        if input.jump_pressed {
            player.jump_buffer = ctl.jump.jump_buffer;
        } else {
            player.jump_buffer = (player.jump_buffer - dt).max(0.0);
        }
        let can_jump = player.jump_buffer > 0.0
            && (player.grounded || player.coyote > 0.0)
            && !player.downed;
        if can_jump {
            player.jump_buffer = 0.0;
            player.coyote = 0.0;
            player.grounded = false;
        }

        player.vel.y -= ctl.jump.gravity * dt;
        let mut burning = false;
        if input.boost_pressed
            && player.fuel >= ctl.boost.dash_cost
            && !player.downed
        {
            let dash_dir = if wish_len > 0.05 {
                wish
            } else {
                flat_forward(player.yaw)
            };
            player.vel.x += dash_dir.x * ctl.boost.dash_impulse;
            player.vel.z += dash_dir.z * ctl.boost.dash_impulse;
            player.fuel -= ctl.boost.dash_cost;
            burning = true;
            self.sounds.push(SoundEvent {
                id: "dash".into(),
                pos: player.pos,
                owner: 0,
            });
        }
        if input.boost_held && player.fuel > 0.0 && !player.downed {
            player.vel.y += ctl.boost.thrust_up * dt;
            player.fuel = (player.fuel - ctl.boost.fuel_burn_per_s * dt).max(0.0);
            burning = true;
        }
        let max_fuel = ctl.boost.fuel_max * (1.0 + mods.jetpack_fuel_pct);
        if burning {
            player.fuel_delay = ctl.boost.regen_delay;
        } else if player.fuel_delay > 0.0 {
            player.fuel_delay -= dt;
        } else {
            player.fuel = (player.fuel + ctl.boost.fuel_regen_per_s * dt).min(max_fuel);
        }
        if can_jump {
            player.vel.y = ctl.jump.jump_velocity;
            self.sounds.push(SoundEvent {
                id: "jump".into(),
                pos: player.pos,
                owner: 0,
            });
        }

        let was_grounded = player.grounded;
        player.pos += player.vel * dt;
        let radius = content.combat.hit_radius.player;
        let half = content.arena.half_extent - radius;
        for obstacle in &content.arena.obstacles {
            player.pos = obstacle.push_circle_xz(player.pos, radius);
        }
        player.pos.x = player.pos.x.clamp(-half, half);
        player.pos.z = player.pos.z.clamp(-half, half);
        let mut just_landed = false;
        if player.pos.y <= 0.0 {
            if !was_grounded && player.vel.y <= 0.0 {
                just_landed = true;
            }
            player.pos.y = 0.0;
            if player.vel.y < 0.0 {
                player.vel.y = 0.0;
            }
            player.grounded = true;
            player.coyote = ctl.jump.coyote_time;
        } else {
            player.grounded = false;
            player.coyote = (player.coyote - dt).max(0.0);
        }
        if player.invuln > 0.0 {
            player.invuln = (player.invuln - dt).max(0.0);
        }
        if player.ability_cd > 0.0 {
            player.ability_cd = (player.ability_cd - dt).max(0.0);
        }
        if player.shield_delay > 0.0 {
            player.shield_delay = (player.shield_delay - dt).max(0.0);
        } else if player.shield < player.max_shield {
            player.shield =
                (player.shield + content.combat.shield_regen_per_s * dt).min(player.max_shield);
        }
        if just_landed {
            self.landed_pulse = 1.0;
            let radius = mods.landing_knockback_radius_m;
            if radius > 0.0 {
                self.shockwave(content, radius);
            }
        }
        let _ = was_grounded;
    }

    fn shockwave(&mut self, content: &Content, radius: f32) {
        let origin = self.players[0].pos;
        let dmg = content.combat.shockwave_damage;
        let impulse = content.combat.shockwave_impulse;
        for enemy in &mut self.enemies {
            if !enemy.alive {
                continue;
            }
            let away = enemy.pos - origin;
            let dist = Vec3::new(away.x, 0.0, away.z).length();
            if dist > radius || dist < 1e-3 {
                continue;
            }
            let scale = if enemy.kind == EnemyKind::BroodMother || enemy.kind == EnemyKind::HiveTyrant
            {
                content.combat.knockback_resist_boss
            } else {
                1.0
            };
            let push = away.xz_normalized() * impulse * scale;
            enemy.vel.x += push.x;
            enemy.vel.z += push.z;
            enemy.hp -= dmg;
        }
    }

    fn tick_enemies(&mut self, content: &Content, dt: f32) {
        let player_pos = self.players.first().map(|p| p.pos).unwrap_or(Vec3::ZERO);
        let player_alive = self.players.first().map(|p| p.alive && !p.downed).unwrap_or(false);
        self.rebuild_grid(content);
        let separation = content.combat.separation_radius;
        let mut pushes = vec![Vec3::ZERO; self.enemies.len()];
        for i in 0..self.enemies.len() {
            if !self.enemies[i].alive {
                continue;
            }
            let pos = self.enemies[i].pos;
            let near = self.spatial.query_radius(
                &self.enemies,
                pos.x,
                pos.z,
                separation,
                |e| e.pos,
            );
            for j in near {
                if i == j || !self.enemies[j].alive {
                    continue;
                }
                let away = pos - self.enemies[j].pos;
                let dist = Vec3::new(away.x, 0.0, away.z).length().max(0.05);
                pushes[i] += away.xz_normalized() * (content.combat.separation_strength / dist);
            }
        }
        let mut opened: Vec<(u32, Vec3)> = Vec::new();
        for (enemy, push) in self.enemies.iter_mut().zip(pushes) {
            if !enemy.alive {
                continue;
            }
            step_enemy(enemy, content, player_pos, player_alive, push, dt);
            if enemy.just_opened {
                enemy.just_opened = false;
                opened.push((enemy.id, enemy.pos));
            }
        }
        let shell = content.bug(EnemyKind::Brute).audio.shell_open.clone();
        for (owner, pos) in opened {
            self.sound_owned(&shell, pos, owner);
        }
    }

    fn tick_melee(&mut self, content: &Content) {
        let Some(player) = self.players.first() else {
            return;
        };
        if !player.alive {
            return;
        }
        let player_pos = player.pos;
        let player_r = content.combat.hit_radius.player;
        let mut hits: Vec<(f32, bool)> = Vec::new();
        for enemy in &mut self.enemies {
            if !enemy.alive || enemy.attack_cd > 0.0 {
                continue;
            }
            let reach = hit_radius_of(content, enemy.kind) + player_r + 0.15;
            let dist = Vec3::new(enemy.pos.x - player_pos.x, 0.0, enemy.pos.z - player_pos.z).length();
            let lunging = matches!(enemy.ai, AiState::Lunge { .. });
            let melee_kind = matches!(
                enemy.kind,
                EnemyKind::Swarmer | EnemyKind::Brute | EnemyKind::BroodMother
            );
            if melee_kind && (lunging || dist < reach) && !matches!(enemy.ai, AiState::Telegraph { .. } | AiState::Recover { .. })
            {
                let acid = false;
                hits.push((enemy.contact, acid));
                enemy.attack_cd = match enemy.kind {
                    EnemyKind::Swarmer => content.combat.swarmer_melee_interval,
                    EnemyKind::Brute => content.combat.brute_cooldown,
                    _ => content.combat.boss_melee_interval,
                };
            }
        }
        for (dmg, acid) in hits {
            self.hurt_player(content, dmg, acid);
        }
    }

    fn tick_weapons(&mut self, content: &Content, input: PlayerInput, dt: f32) {
        let mods = {
            let player = &self.players[0];
            upgrade::collect(content, &player.upgrades)
        };
        {
            let player = &mut self.players[0];
            player.primary.cooldown = (player.primary.cooldown - dt).max(0.0);
            player.sidearm.cooldown = (player.sidearm.cooldown - dt).max(0.0);
            tick_reload(&mut player.primary, dt);
            tick_reload(&mut player.sidearm, dt);
        }
        if input.reload {
            self.arm_reload(content, &mods, self.players[0].downed);
        }
        if input.ability && self.players[0].ability_cd <= 0.0 && !self.players[0].downed {
            self.cast_ability(content, &mods);
        }
        let (alive, downed, sprinting, lock, weapon_id) = {
            let player = &self.players[0];
            let downed = player.downed;
            let weapon_id = if downed {
                player.sidearm.weapon_id.clone()
            } else {
                player.primary.weapon_id.clone()
            };
            (
                player.alive,
                downed,
                player.sprinting,
                player.sprint_fire_lock,
                weapon_id,
            )
        };
        if !alive || !input.fire {
            return;
        }
        if downed && !content.controller.downed.downed_can_fire_sidearm {
            return;
        }
        if sprinting || lock > 0.0 {
            return;
        }
        let Some(spec) = content.weapon(&weapon_id).cloned() else {
            return;
        };
        let (busy, empty) = {
            let gun = if downed {
                &self.players[0].sidearm
            } else {
                &self.players[0].primary
            };
            (gun.reload > 0.0 || gun.cooldown > 0.0, gun.ammo == 0)
        };
        if busy {
            return;
        }
        if empty {
            self.arm_reload(content, &mods, downed);
            return;
        }
        {
            let gun = if downed {
                &mut self.players[0].sidearm
            } else {
                &mut self.players[0].primary
            };
            gun.ammo -= 1;
            gun.cooldown = spec.fire_interval / (1.0 + mods.fire_rate_pct).max(0.05);
        }
        let kind = if downed { ShotKind::Sidearm } else { ShotKind::Coil };
        self.spawn_player_shot(content, &spec, &mods, kind);
    }

    fn spawn_player_shot(
        &mut self,
        content: &Content,
        spec: &WeaponDef,
        mods: &Modifiers,
        kind: ShotKind,
    ) {
        let (mut dir, eye, origin_base, owner) = {
            let player = &self.players[0];
            let eye = if player.crouching || player.downed {
                content.arena.crouch_eye
            } else {
                content.arena.eye_height
            };
            (look_dir(player.yaw, player.pitch), eye, player.pos, player.id)
        };
        if spec.spread > 0.0 {
            let yaw = self.players[0].yaw + (self.rng.next_f32() - 0.5) * 2.0 * spec.spread;
            let pitch = self.players[0].pitch + (self.rng.next_f32() - 0.5) * 2.0 * spec.spread;
            dir = look_dir(yaw, pitch);
        }
        let origin = origin_base + Vec3::new(0.0, eye, 0.0) + dir * 0.4;
        let (_, rank_dmg) = rank_bonus(&content.duplicate, self.duplicate_rank);
        let mut damage = spec.damage * (1.0 + mods.damage_pct + rank_dmg + mods.squad_damage_pct);
        if mods.glass_cannon {
            damage *= 1.0 + content.combat.glass_cannon_damage_pct;
        }
        let crit = if mods.crit_chance > 0.0 && self.rng.next_f32() < mods.crit_chance {
            content.combat.crit_multiplier
        } else {
            1.0
        };
        let id = self.alloc_id();
        self.sound_at("shot", origin);
        self.projectiles.push(Projectile {
            id,
            pos: origin,
            vel: dir * spec.projectile_speed,
            damage: damage * crit,
            radius: spec.projectile_radius,
            team: Team::Player,
            ttl: content.combat.player_projectile_ttl,
            pierce: mods.pierce_count.round().max(0.0) as u32,
            bounces: mods.bounce_targets.round().max(0.0) as u32,
            hit: Vec::new(),
            kind,
            acid_ward: false,
            owner,
            crit_mult: crit,
        });
    }

    fn cast_ability(&mut self, content: &Content, mods: &Modifiers) {
        let cdr = mods.ability_cooldown_reduction.clamp(0.0, 0.95);
        self.players[0].ability_cd = content.controller.ability.default_cooldown * (1.0 - cdr);
        let origin = self.players[0].pos;
        let radius = content.combat.ability_radius;
        let dmg = content.combat.ability_damage * (1.0 + mods.damage_pct);
        self.sound_at("ability", origin);
        for enemy in &mut self.enemies {
            if !enemy.alive {
                continue;
            }
            let d = Vec3::new(enemy.pos.x - origin.x, 0.0, enemy.pos.z - origin.z).length();
            if d <= radius {
                enemy.hp -= dmg;
            }
        }
    }

    fn tick_projectiles(&mut self, content: &Content, dt: f32) {
        let mut i = 0;
        while i < self.projectiles.len() {
            let p0 = self.projectiles[i].pos;
            let vel = self.projectiles[i].vel;
            let p1 = p0 + vel * dt;
            let team = self.projectiles[i].team;
            let blocked = content.arena.obstacles.iter().any(|b| segment_hits_aabb(p0, p1, *b))
                || p1.x.abs() > content.arena.half_extent
                || p1.z.abs() > content.arena.half_extent
                || p1.y < 0.0
                || p1.y > 40.0;
            if team == Team::Player {
                let hits = gather_enemy_hits(content, &self.enemies, &self.projectiles[i], p0, p1);
                let mut remove = blocked;
                if !hits.is_empty() {
                    remove = self.apply_player_hits(content, i, &hits) || blocked;
                }
                if remove {
                    self.projectiles.swap_remove(i);
                    continue;
                }
            } else if self.acid_hits_player(content, i, p0, p1) || blocked {
                self.projectiles.swap_remove(i);
                continue;
            }
            if i >= self.projectiles.len() {
                break;
            }
            self.projectiles[i].pos = p1;
            self.projectiles[i].ttl -= dt;
            if self.projectiles[i].ttl <= 0.0 {
                self.projectiles.swap_remove(i);
                continue;
            }
            i += 1;
        }
    }

    fn apply_player_hits(&mut self, content: &Content, proj_index: usize, hits: &[(usize, f32, HitSpot)]) -> bool {
        let mods = upgrade::collect(content, &self.players[0].upgrades);
        let mut remove = false;
        for &(enemy_index, _t, spot) in hits {
            let enemy_id = self.enemies[enemy_index].id;
            if self.projectiles[proj_index].hit.contains(&enemy_id) || !self.enemies[enemy_index].alive {
                continue;
            }
            let mut dmg = self.projectiles[proj_index].damage;
            let mut weak_ok = false;
            let mut ping_pos = self.enemies[enemy_index].pos;
            match spot {
                HitSpot::Pod(pi) => {
                    if let Some(hp) = self.enemies[enemy_index].pods.get_mut(pi) {
                        *hp = (*hp - dmg).max(0.0);
                    }
                    ping_pos = pod_world(content, &self.enemies[enemy_index], pi);
                }
                HitSpot::Weak => {
                    weak_ok = true;
                    let mult = self.enemies[enemy_index].weak_mult * (1.0 + mods.weakpoint_damage_pct);
                    dmg *= mult;
                    let charging = matches!(self.enemies[enemy_index].ai, AiState::Telegraph { .. });
                    let pop = content.bug(self.enemies[enemy_index].kind).hitbox.sac_pop_cancels_shot;
                    if pop && charging {
                        let charge_id = content.bug(EnemyKind::Spitter).audio.charge.clone();
                        let pop_id = content.bug(EnemyKind::Spitter).audio.sac_pop.clone();
                        let pos = self.enemies[enemy_index].pos;
                        self.enemies[enemy_index].ai = AiState::Chase;
                        self.enemies[enemy_index].attack_cd = content.combat.spitter_cooldown;
                        self.enemies[enemy_index].vel.y = 0.0;
                        self.stops.push(SoundStop {
                            id: charge_id,
                            owner: enemy_id,
                        });
                        self.sound_owned(&pop_id, pos, enemy_id);
                    }
                    self.enemies[enemy_index].hp -= dmg;
                    ping_pos = weak_world(content, &self.enemies[enemy_index]);
                }
                HitSpot::Body => {
                    let closed = self.enemies[enemy_index].kind == EnemyKind::Brute
                        && self.enemies[enemy_index].open_left <= 0.0;
                    let armor = content.bug(self.enemies[enemy_index].kind).hitbox.armor_mult_when_closed;
                    if closed && armor > 0.0 {
                        dmg *= armor;
                    }
                    self.enemies[enemy_index].hp -= dmg;
                }
            }
            self.projectiles[proj_index].hit.push(enemy_id);
            self.pings.push(HitPing {
                pos: ping_pos,
                amount: dmg,
                weak: weak_ok,
            });
            if weak_ok {
                let id = content.bugs.feedback.weakpoint_hit.clone();
                self.sound_owned(&id, ping_pos, enemy_id);
            } else if !matches!(spot, HitSpot::Pod(_)) {
                self.sound_at("hit", ping_pos);
            }
            if !matches!(spot, HitSpot::Pod(_)) {
                self.try_chain(content, &mods, enemy_index, dmg);
            }
            if self.projectiles[proj_index].pierce > 0 {
                self.projectiles[proj_index].pierce -= 1;
                continue;
            }
            if self.projectiles[proj_index].bounces > 0 {
                if self.ricochet(content, proj_index, enemy_index) {
                    return false;
                }
            }
            remove = true;
            break;
        }
        remove
    }

    fn try_chain(&mut self, content: &Content, mods: &Modifiers, from: usize, damage: f32) {
        if mods.arc_proc_chance <= 0.0 || self.rng.next_f32() > mods.arc_proc_chance {
            return;
        }
        let origin = self.enemies[from].pos;
        let mut near = self.spatial.query_radius(
            &self.enemies,
            origin.x,
            origin.z,
            content.combat.chain_range,
            |e| e.pos,
        );
        near.retain(|&idx| idx != from && self.enemies[idx].alive);
        near.sort_by_key(|&idx| self.enemies[idx].id);
        let arc = content.combat.chain_damage_pct * damage;
        for idx in near.into_iter().take(content.combat.chain_targets as usize) {
            self.enemies[idx].hp -= arc;
        }
    }

    fn ricochet(&mut self, content: &Content, proj_index: usize, from: usize) -> bool {
        let origin = self.enemies[from].pos;
        let already = self.projectiles[proj_index].hit.clone();
        let mut near = self.spatial.query_radius(
            &self.enemies,
            origin.x,
            origin.z,
            content.combat.ricochet_range,
            |e| e.pos,
        );
        near.retain(|&idx| self.enemies[idx].alive && !already.contains(&self.enemies[idx].id));
        near.sort_by_key(|&idx| self.enemies[idx].id);
        let Some(next) = near.first().copied() else {
            return false;
        };
        let dest = self.enemies[next].pos + Vec3::new(0.0, body_lift_of(content, self.enemies[next].kind), 0.0);
        let dir = (dest - origin).normalized();
        if dir.length_squared() < 1e-6 {
            return false;
        }
        let speed = self.projectiles[proj_index].vel.length().max(1.0);
        self.projectiles[proj_index].vel = dir * speed;
        self.projectiles[proj_index].pos = origin;
        self.projectiles[proj_index].damage *= content.combat.ricochet_damage_pct;
        self.projectiles[proj_index].bounces -= 1;
        true
    }

    fn acid_hits_player(&mut self, content: &Content, proj_index: usize, p0: Vec3, p1: Vec3) -> bool {
        let player = &self.players[0];
        if !player.alive {
            return true;
        }
        let center = player.pos + Vec3::new(0.0, content.arena.eye_height * 0.5, 0.0);
        let radius = content.combat.hit_radius.player + self.projectiles[proj_index].radius;
        if segment_hits_sphere(p0, p1, center, radius).is_some() {
            let dmg = self.projectiles[proj_index].damage;
            let ward = self.projectiles[proj_index].acid_ward;
            self.hurt_player(content, dmg, ward);
            true
        } else {
            false
        }
    }

    fn hurt_player(&mut self, content: &Content, mut amount: f32, acid_ward: bool) {
        let player = &mut self.players[0];
        if !player.alive || player.invuln > 0.0 || amount <= 0.0 {
            return;
        }
        if acid_ward {
            let mods = upgrade::collect(content, &player.upgrades);
            amount *= 1.0 - mods.spitter_damage_reduction.clamp(0.0, 0.95);
        }
        let mods = upgrade::collect(content, &player.upgrades);
        if player.shield > 0.0 {
            let absorbed = amount.min(player.shield);
            player.shield -= absorbed;
            amount -= absorbed;
            player.shield_delay = content.controller.health.shield_regen_delay;
        }
        if amount <= 0.0 {
            return;
        }
        let pos = player.pos;
        let mut downed_now = false;
        let mut defeated = false;
        if amount >= player.hp && mods.last_stand && player.last_stand_wave != self.director.wave {
            player.hp = 1.0;
            player.last_stand_wave = self.director.wave;
        } else {
            player.hp -= amount;
            if player.hp <= 0.0 {
                player.hp = 0.0;
                if !player.downed {
                    let solo = self.player_count <= 1;
                    let used = player.self_revives_used;
                    let cap = content.controller.downed.solo_self_revives_per_wave;
                    player.sprinting = false;
                    if solo && used >= cap {
                        player.alive = false;
                        defeated = true;
                    } else {
                        player.downed = true;
                        player.bleedout = content.controller.downed.bleedout_time;
                        player.self_revive_held = 0.0;
                        downed_now = true;
                    }
                }
            }
        }
        let _ = player;
        self.sound_at("hurt", pos);
        if downed_now || defeated {
            self.sound_at("downed", pos);
        }
        if defeated {
            self.phase = Phase::Defeat;
            self.music.push(MusicMoment::MatchEnd);
        }
    }

    fn tick_drones(&mut self, content: &Content, dt: f32) {
        let mods = upgrade::collect(content, &self.players[0].upgrades);
        let count = mods.drone_count.round().max(0.0) as i32;
        if count <= 0 {
            return;
        }
        self.players[0].drone_angle += content.combat.drone_orbit_speed * dt;
        let origin = self.players[0].pos;
        let dps = content.combat.drone_dps * dt;
        for i in 0..count {
            let ang = self.players[0].drone_angle
                + i as f32 * std::f32::consts::TAU / count as f32;
            let pos = origin
                + Vec3::new(
                    ang.cos() * content.combat.drone_orbit_radius,
                    content.combat.drone_height,
                    ang.sin() * content.combat.drone_orbit_radius,
                );
            let mut best: Option<(usize, f32)> = None;
            for (idx, enemy) in self.enemies.iter().enumerate() {
                if !enemy.alive {
                    continue;
                }
                let d = (enemy.pos - pos).length();
                if d <= content.combat.drone_range && best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some((idx, d));
                }
            }
            if let Some((idx, _)) = best {
                self.enemies[idx].hp -= dps;
            }
        }
    }

    fn tick_pickups(&mut self, content: &Content) {
        let mods = upgrade::collect(content, &self.players[0].upgrades);
        let radius = content.combat.pickup_base_radius * (1.0 + mods.pickup_radius_pct);
        let pos = self.players[0].pos;
        let mut i = 0;
        while i < self.pickups.len() {
            let d = Vec3::new(self.pickups[i].pos.x - pos.x, 0.0, self.pickups[i].pos.z - pos.z).length();
            if d <= radius {
                self.players[0].primary.reserve += self.pickups[i].ammo;
                self.sound_at("pickup", self.pickups[i].pos);
                self.pickups.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    fn tick_boss_spawns(&mut self, content: &Content) {
        let cap = content.waves.budget.live_enemy_cap as usize;
        let dt = SIM_DT;
        for i in 0..self.enemies.len() {
            if !self.enemies[i].alive || self.enemies[i].spawn_interval <= 0.0 {
                continue;
            }
            self.enemies[i].spawn_acc += dt;
            if self.enemies[i].spawn_acc < self.enemies[i].spawn_interval {
                continue;
            }
            self.enemies[i].spawn_acc = 0.0;
            let kind = self.enemies[i].spawn_kind;
            let count = self.enemies[i].spawn_count;
            let origin = self.enemies[i].pos;
            for n in 0..count {
                if self.enemies.iter().filter(|e| e.alive).count() >= cap {
                    break;
                }
                let ang = n as f32 / count.max(1) as f32 * std::f32::consts::TAU;
                let pos = origin + Vec3::new(ang.cos() * 2.2, 0.0, ang.sin() * 2.2);
                self.spawn_trash(content, kind, pos);
            }
        }
    }

    fn tick_spawner(&mut self, content: &Content, dt: f32) {
        if self.director.parked {
            return;
        }
        let wave_n = self.director.wave;
        let Some(wave) = content.wave(wave_n).cloned() else {
            return;
        };
        let cheapest = cheapest_cost(content, &wave.mix);
        if cheapest.is_infinite() {
            self.director.pool = 0.0;
            return;
        }
        if self.director.pool < cheapest {
            self.director.pool = 0.0;
            return;
        }
        self.director.spawn_wait -= dt;
        if self.director.spawn_wait > 0.0 {
            return;
        }
        self.director.spawn_wait = content.combat.spawn_interval;
        self.director.group_left = content.combat.group_budget;
        let cap = content.waves.budget.live_enemy_cap;
        loop {
            if self.director.group_left <= 0.0 || self.director.parked {
                break;
            }
            let Some(kind) = self.pick_mix(content, &wave.mix, cheapest) else {
                break;
            };
            let cost = trash_cost(content, kind);
            if self.director.pool + 1e-3 < cost {
                break;
            }
            if self.enemies.iter().filter(|e| e.alive).count() >= cap as usize {
                self.park_budget(content.waves.budget.queue_cap_waves);
                break;
            }
            self.director.pool -= cost;
            self.director.group_left -= cost;
            let pos = self.ring_pos(content);
            self.spawn_trash(content, kind, pos);
        }
    }

    fn pick_mix(&mut self, content: &Content, mix: &[f32; 3], _cheapest: f32) -> Option<EnemyKind> {
        let kinds = [EnemyKind::Swarmer, EnemyKind::Spitter, EnemyKind::Brute];
        let mut total = 0.0;
        let mut weights = [0.0; 3];
        for i in 0..3 {
            let cost = trash_cost(content, kinds[i]);
            if mix[i] > 0.0 && cost <= self.director.pool + 1e-3 {
                weights[i] = mix[i];
                total += mix[i];
            }
        }
        if total <= 0.0 {
            return None;
        }
        let mut needle = self.rng.next_f32() * total;
        for i in 0..3 {
            needle -= weights[i];
            if needle <= 0.0 && weights[i] > 0.0 {
                return Some(kinds[i]);
            }
        }
        kinds.into_iter().rev().find(|k| weights[kind_index(*k)] > 0.0)
    }

    fn ring_pos(&mut self, content: &Content) -> Vec3 {
        let ang = self.rng.next_f32() * std::f32::consts::TAU;
        let r = content.arena.spawn_ring;
        Vec3::new(ang.sin() * r, 0.0, ang.cos() * r)
    }

    fn spawn_trash(&mut self, content: &Content, kind: EnemyKind, pos: Vec3) {
        let stats = content.enemy(kind).clone();
        let (hp, dmg) = scaled_stats(content, self, stats.hp, stats.dmg);
        let weak = authored_weak_mult(content, kind, None);
        let exposed = kind != EnemyKind::Brute;
        let id = self.alloc_id();
        self.director.spawned_this_wave = self.director.spawned_this_wave.saturating_add(1);
        self.enemies.push(Enemy {
            id,
            kind,
            pos,
            vel: Vec3::ZERO,
            yaw: 0.0,
            hp,
            max_hp: hp,
            speed: stats.speed,
            contact: dmg,
            range: stats.range,
            weak_mult: weak,
            attack_cd: 0.4,
            ai: AiState::Chase,
            weak_exposed: exposed,
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
    }

    fn spawn_boss(&mut self, content: &Content, boss_id: &str) {
        let Some(def) = content.boss(boss_id).cloned() else {
            return;
        };
        let Some(kind) = EnemyKind::from_boss_id(boss_id) else {
            return;
        };
        let players = self.player_count.max(1) as f32;
        let hp_scale = 1.0 + content.waves.budget.boss_hp_per_extra_player * (players - 1.0);
        let (mut hp, dmg) = scaled_stats(content, self, def.hp, def.dmg);
        hp *= hp_scale;
        let pos = Vec3::new(0.0, 0.0, -content.arena.spawn_ring * 0.65);
        let spawn_kind = EnemyKind::from_name(&def.spawn_kind).unwrap_or(EnemyKind::Swarmer);
        let id = self.alloc_id();
        self.director.spawned_this_wave = self.director.spawned_this_wave.saturating_add(1);
        self.enemies.push(Enemy {
            id,
            kind,
            pos,
            vel: Vec3::ZERO,
            yaw: 0.0,
            hp,
            max_hp: hp,
            speed: def.speed,
            contact: dmg,
            range: def.range,
            weak_mult: authored_weak_mult(content, kind, Some(def.weakpoint_mult)),
            attack_cd: 1.0,
            ai: AiState::Chase,
            weak_exposed: true,
            phase: 0,
            spawn_acc: def.spawn_interval * 0.5,
            spawn_kind,
            spawn_count: def.spawn_count,
            spawn_interval: def.spawn_interval,
            alive: true,
            open_left: 0.0,
            just_opened: false,
            pods: if kind == EnemyKind::BroodMother {
                let n = content.bug(kind).hitbox.pods.len().max(def.pod_count as usize);
                vec![def.pod_hp; n]
            } else {
                Vec::new()
            },
        });
        let _ = dmg;
        self.sound_at("boss", pos);
        self.music.push(MusicMoment::BossIntro {
            boss_id: boss_id.to_string(),
        });
    }

    fn reap(&mut self, content: &Content) {
        let mut i = 0;
        while i < self.enemies.len() {
            if self.enemies[i].alive && self.enemies[i].hp <= 0.0 {
                self.enemies[i].alive = false;
                self.enemies[i].hp = 0.0;
                self.on_kill(content, i);
            }
            if !self.enemies[i].alive {
                self.enemies.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    fn on_kill(&mut self, content: &Content, index: usize) {
        let kind = self.enemies[index].kind;
        let pos = self.enemies[index].pos;
        let boss_id = match kind {
            EnemyKind::BroodMother => Some("brood_mother"),
            EnemyKind::HiveTyrant => Some("hive_tyrant"),
            _ => None,
        };
        if boss_id.is_some() {
            self.boss_kills += 1;
        }
        let mods = upgrade::collect(content, &self.players[0].upgrades);
        if mods.heal_per_kill_flat > 0.0 {
            self.players[0].hp = (self.players[0].hp + mods.heal_per_kill_flat).min(self.players[0].max_hp);
        }
        if mods.nova_kill_interval > 0.0 {
            self.players[0].kills_since_nova += 1;
            if self.players[0].kills_since_nova as f32 >= mods.nova_kill_interval {
                self.players[0].kills_since_nova = 0;
                self.nova(content, pos);
            }
        }
        if mods.ammo_drop_chance > 0.0 && self.rng.next_f32() < mods.ammo_drop_chance {
            let id = self.alloc_id();
            self.pickups.push(Pickup {
                id,
                pos,
                ammo: content.combat.ammo_pickup_amount,
            });
        }
        if let Some(boss_id) = boss_id {
            self.music.push(MusicMoment::BossDown {
                boss_id: boss_id.to_string(),
            });
        }
    }

    fn nova(&mut self, content: &Content, pos: Vec3) {
        let radius = content.combat.nova_radius;
        let dmg = content.combat.nova_damage;
        for enemy in &mut self.enemies {
            if !enemy.alive {
                continue;
            }
            let d = Vec3::new(enemy.pos.x - pos.x, 0.0, enemy.pos.z - pos.z).length();
            if d <= radius {
                enemy.hp -= dmg;
            }
        }
    }

    fn tick_downed(&mut self, content: &Content, interact_held: bool, dt: f32) {
        let solo = self.player_count <= 1;
        let cap = content.controller.downed.solo_self_revives_per_wave;
        let hold_needed = content.controller.downed.solo_self_revive_time;
        let player = &mut self.players[0];
        if !player.downed || !player.alive {
            return;
        }
        if solo && player.self_revives_used < cap && interact_held {
            player.self_revive_held += dt;
            if player.self_revive_held >= hold_needed {
                player.downed = false;
                player.self_revives_used += 1;
                player.self_revive_held = 0.0;
                player.invuln = content.controller.downed.revive_invuln;
                let frac = content.controller.downed.solo_self_revive_hp_fraction;
                player.hp = (player.max_hp * frac).max(1.0);
                player.bleedout = content.controller.downed.bleedout_time;
                return;
            }
        } else {
            player.self_revive_held = 0.0;
        }
        player.bleedout -= dt;
        let defeated = player.bleedout <= 0.0;
        if defeated {
            player.alive = false;
            player.hp = 0.0;
        }
        let _ = player;
        if defeated {
            self.phase = Phase::Defeat;
            self.music.push(MusicMoment::MatchEnd);
        }
    }

    fn emit_crowd(&mut self) {
        for enemy in &self.enemies {
            if enemy.alive && enemy.kind == EnemyKind::Swarmer {
                self.sounds.push(SoundEvent {
                    id: "swarmer_crowd".into(),
                    pos: enemy.pos,
                    owner: enemy.id,
                });
            }
        }
    }

    fn finish_wave(&mut self, content: &Content) {
        if self.phase != Phase::Combat {
            return;
        }
        if self.enemies.iter().any(|e| e.alive) || self.director.pool > 0.0 {
            return;
        }
        let last = content.waves.wave.iter().map(|w| w.n).max().unwrap_or(1);
        if self.director.wave >= last && !self.endless_mode {
            self.phase = Phase::Victory;
            self.music.push(MusicMoment::MatchEnd);
            return;
        }
        let mods_owned = self.players[0].upgrades.clone();
        let n = content.waves.budget.upgrade_choices as usize;
        let options = upgrade::roll_offers(&mut self.rng, content, &mods_owned, n);
        self.phase = Phase::Upgrade { options };
    }

    fn grant_upgrade(&mut self, content: &Content, id: &str) {
        let player = &mut self.players[0];
        let entry = player.upgrades.entry(id.to_string()).or_insert(0);
        *entry += 1;
        let mods = upgrade::collect(content, &player.upgrades);
        let new_max = max_hp(content, &mods, self.duplicate_rank);
        if new_max > player.max_hp {
            player.hp += new_max - player.max_hp;
        }
        player.max_hp = new_max;
        player.hp = player.hp.min(player.max_hp).max(1.0);
        player.max_shield = mods.shield_flat;
        if let Some(spec) = content.weapon(&player.primary.weapon_id) {
            player.primary.mag_size = mag_size(spec.magazine, mods.magazine_pct);
        }
        if let Some(spec) = content.weapon(&player.sidearm.weapon_id) {
            player.sidearm.mag_size = mag_size(spec.magazine, mods.magazine_pct);
        }
    }

    fn rebuild_grid(&mut self, content: &Content) {
        if self.spatial.cell != content.combat.cell_size {
            self.spatial = SpatialGrid::new(content.combat.cell_size, content.arena.half_extent);
        }
        self.spatial.rebuild(&self.enemies, |e| e.pos);
    }

    fn sound_at(&mut self, id: &str, pos: Vec3) {
        self.sound_owned(id, pos, 0);
    }

    fn sound_owned(&mut self, id: &str, pos: Vec3, owner: u32) {
        self.sounds.push(SoundEvent {
            id: id.to_string(),
            pos,
            owner,
        });
    }
}

fn gun_from(spec: &WeaponDef) -> Gun {
    Gun {
        weapon_id: spec.id.clone(),
        ammo: spec.magazine,
        reserve: spec.reserve,
        mag_size: spec.magazine,
        cooldown: 0.0,
        reload: 0.0,
    }
}

fn mag_size(base: u32, pct: f32) -> u32 {
    ((base as f32) * (1.0 + pct)).round().max(1.0) as u32
}

fn max_hp(content: &Content, mods: &Modifiers, rank: u32) -> f32 {
    let (hp_pct, _) = rank_bonus(&content.duplicate, rank);
    let mut max = (content.controller.health.max_hp + mods.max_hp_flat) * (1.0 + hp_pct);
    if mods.glass_cannon {
        max *= 1.0 + content.combat.glass_cannon_max_hp_pct;
    }
    max.max(1.0)
}

fn refresh_defense(content: &Content, player: &mut Player, mods: &Modifiers, rank: u32) {
    let new_max = max_hp(content, mods, rank);
    if (new_max - player.max_hp).abs() > 0.01 && player.hp > 0.0 {
        let ratio = player.hp / player.max_hp.max(1.0);
        player.max_hp = new_max;
        player.hp = (ratio * new_max).min(new_max);
    } else {
        player.max_hp = new_max;
    }
    player.max_shield = mods.shield_flat;
}

fn wish_dir(yaw: f32, x: f32, z: f32) -> Vec3 {
    let x = x.clamp(-1.0, 1.0);
    let z = z.clamp(-1.0, 1.0);
    let wish = flat_forward(yaw) * z + flat_right(yaw) * x;
    let len = wish.length();
    if len > 1.0 { wish * (1.0 / len) } else { wish }
}

fn apply_look(player: &mut Player, input: &PlayerInput, content: &Content, enemies: &[Enemy]) {
    let limit = content.presentation.pitch_limit_deg.to_radians();
    let assist = input.using_gamepad || content.controller.aim.mouse_aim_assist;
    if !assist {
        player.yaw = input.yaw;
        player.pitch = input.pitch.clamp(-limit, limit);
        return;
    }
    let mut dx = input.look_dx_deg;
    let mut dy = input.look_dy_deg;
    if let Some((ty, tp, ang)) = aim_target(player, content, enemies) {
        let slow = content.controller.aim.pad_slowdown_radius_deg.to_radians();
        if ang < slow {
            let factor = 1.0 - content.controller.aim.pad_slowdown_strength.clamp(0.0, 1.0);
            dx *= factor;
            dy *= factor;
        }
        let mag = content.controller.aim.pad_magnetism_deg.to_radians();
        if ang < mag {
            let blend = content.presentation.pad_magnetism_blend;
            let yaw_err = wrap_pi(ty - player.yaw).to_degrees();
            let pitch_err = (tp - player.pitch).to_degrees();
            dx += yaw_err * blend;
            dy += pitch_err * blend;
        }
    }
    player.yaw += dx.to_radians();
    player.pitch = (player.pitch + dy.to_radians()).clamp(-limit, limit);
}

fn aim_target(player: &Player, content: &Content, enemies: &[Enemy]) -> Option<(f32, f32, f32)> {
    let look = look_dir(player.yaw, player.pitch);
    let eye = player.pos + Vec3::new(0.0, content.arena.eye_height, 0.0);
    let mut best: Option<(f32, f32, f32)> = None;
    for enemy in enemies {
        if !enemy.alive {
            continue;
        }
        let point = enemy.pos + rotate_y(enemy.yaw, weakpoint_of(content, enemy.kind).offset_vec());
        let to = point - eye;
        let dir = to.normalized();
        if dir.length_squared() < 1e-6 {
            continue;
        }
        let ang = look.dot(dir).clamp(-1.0, 1.0).acos();
        if best.map(|(a, _, _)| ang < a).unwrap_or(true) {
            let yaw = (-dir.x).atan2(-dir.z);
            let pitch = dir.y.asin();
            best = Some((ang, yaw, pitch));
        }
    }
    best.map(|(ang, yaw, pitch)| (yaw, pitch, ang))
}

fn wrap_pi(a: f32) -> f32 {
    let mut v = a;
    while v > std::f32::consts::PI {
        v -= std::f32::consts::TAU;
    }
    while v < -std::f32::consts::PI {
        v += std::f32::consts::TAU;
    }
    v
}

fn step_enemy(
    enemy: &mut Enemy,
    content: &Content,
    player_pos: Vec3,
    player_alive: bool,
    push: Vec3,
    dt: f32,
) {
    enemy.attack_cd = (enemy.attack_cd - dt).max(0.0);
    enemy.vel.x *= 0.9;
    enemy.vel.z *= 0.9;
    let to_player = player_pos - enemy.pos;
    let dist = Vec3::new(to_player.x, 0.0, to_player.z).length().max(0.001);
    let dir = to_player.xz_normalized();
    if dist > 0.2 {
        enemy.yaw = (-dir.x).atan2(-dir.z);
    }
    let mut speed = enemy.speed;
    if enemy.kind == EnemyKind::HiveTyrant {
        if let Some(def) = content.boss("hive_tyrant") {
            let frac = if enemy.max_hp > 0.0 { enemy.hp / enemy.max_hp } else { 1.0 };
            let mut phase = 0usize;
            for (i, gate) in def.phase_thresholds.iter().enumerate() {
                if frac <= *gate {
                    phase = i + 1;
                }
            }
            enemy.phase = phase as u8;
            if let Some(m) = def.phase_speed_mult.get(phase) {
                speed *= *m;
            }
        }
    }
    if enemy.open_left > 0.0 {
        enemy.open_left = (enemy.open_left - dt).max(0.0);
    }
    enemy.weak_exposed = match enemy.kind {
        EnemyKind::Brute => enemy.open_left > 0.0,
        _ => true,
    };
    match enemy.ai {
        AiState::Telegraph { left } => {
            let left = left - dt;
            if left <= 0.0 {
                match enemy.kind {
                    EnemyKind::Brute => {
                        enemy.ai = AiState::Lunge {
                            left: content.combat.brute_lunge_seconds,
                        };
                    }
                    EnemyKind::Spitter | EnemyKind::HiveTyrant => {
                        enemy.attack_cd = if enemy.kind == EnemyKind::Spitter {
                            content.combat.spitter_cooldown
                        } else {
                            content.combat.boss_melee_interval
                        };
                        enemy.ai = AiState::Recover { left: 0.2 };
                        enemy.vel.y = 999.0;
                    }
                    _ => {
                        enemy.ai = AiState::Lunge { left: 0.12 };
                        enemy.attack_cd = 0.0;
                    }
                }
            } else {
                enemy.ai = AiState::Telegraph { left };
            }
            enemy.pos.x += push.x * dt;
            enemy.pos.z += push.z * dt;
            return;
        }
        AiState::Lunge { left } => {
            let step = dir * speed * 2.6 * dt;
            enemy.pos += step;
            enemy.pos.x += push.x * dt * 0.25;
            enemy.pos.z += push.z * dt * 0.25;
            let left = left - dt;
            if left <= 0.0 && enemy.kind == EnemyKind::Brute {
                let dur = content.bug(EnemyKind::Brute).hitbox.vulnerable_after_charge;
                if dur > 0.0 && enemy.open_left <= 0.0 {
                    enemy.just_opened = true;
                }
                enemy.open_left = dur.max(enemy.open_left);
                enemy.weak_exposed = enemy.open_left > 0.0;
            }
            enemy.ai = if left <= 0.0 {
                AiState::Recover {
                    left: content.combat.brute_recover_seconds,
                }
            } else {
                AiState::Lunge { left }
            };
            return;
        }
        AiState::Recover { left } => {
            let left = left - dt;
            enemy.pos += dir * speed * 0.25 * dt;
            enemy.ai = if left <= 0.0 {
                AiState::Chase
            } else {
                AiState::Recover { left }
            };
            return;
        }
        AiState::Chase => {}
    }

    if !player_alive {
        enemy.pos += enemy.vel * dt;
        return;
    }

    let mut move_dir = dir + push * dt;
    match enemy.kind {
        EnemyKind::Spitter => {
            let preferred = enemy.range.max(content.combat.spitter_min_range);
            let [back_away, fire_floor] = content.combat.keep_away_ratios;
            if dist < preferred * back_away {
                move_dir = dir * -1.0 + push * dt;
            } else if dist < preferred {
                move_dir = push * dt;
            }
            if enemy.attack_cd <= 0.0 && dist <= preferred && dist >= preferred * fire_floor {
                let charge = content.bug(EnemyKind::Spitter).hitbox.charge_seconds;
                enemy.ai = AiState::Telegraph {
                    left: if charge > 0.0 {
                        charge
                    } else {
                        content.combat.telegraph_seconds
                    },
                };
                // Charge cue is emitted by the sim after movement; mark with vel.y = 1 for one check.
                enemy.vel.y = 1.0;
            }
        }
        EnemyKind::Brute => {
            let reach = hit_radius_of(content, enemy.kind) + content.combat.hit_radius.player + 0.8;
            if dist < reach && enemy.attack_cd <= 0.0 {
                enemy.ai = AiState::Telegraph {
                    left: content.combat.telegraph_seconds,
                };
                enemy.vel.y = 2.0;
            }
        }
        EnemyKind::HiveTyrant => {
            if dist <= enemy.range && enemy.attack_cd <= 0.0 {
                enemy.ai = AiState::Telegraph {
                    left: content.combat.telegraph_seconds,
                };
                enemy.vel.y = 1.0;
            }
        }
        EnemyKind::BroodMother => {
            let reach = hit_radius_of(content, enemy.kind) + 0.4;
            if dist < reach && enemy.attack_cd <= 0.0 {
                enemy.ai = AiState::Telegraph {
                    left: content.combat.telegraph_seconds,
                };
                enemy.vel.y = 3.0;
            }
        }
        EnemyKind::Swarmer => {}
    }
    let step = move_dir.normalized() * speed * dt + enemy.vel * dt;
    enemy.pos.x += step.x;
    enemy.pos.z += step.z;
    let half = content.arena.half_extent - 0.5;
    enemy.pos.x = enemy.pos.x.clamp(-half, half);
    enemy.pos.z = enemy.pos.z.clamp(-half, half);
    enemy.pos.y = 0.0;
}

#[derive(Clone, Copy, Debug)]
enum HitSpot {
    Body,
    Weak,
    Pod(usize),
}

fn gather_enemy_hits(
    content: &Content,
    enemies: &[Enemy],
    projectile: &Projectile,
    p0: Vec3,
    p1: Vec3,
) -> Vec<(usize, f32, HitSpot)> {
    let mut hits = Vec::new();
    let bonus = content.controller.aim.hitscan_weakpoint_bonus_radius;
    for (idx, enemy) in enemies.iter().enumerate() {
        if !enemy.alive || projectile.hit.contains(&enemy.id) {
            continue;
        }
        let book = content.bug(enemy.kind);
        let charging = matches!(enemy.ai, AiState::Telegraph { .. });
        let mut best: Option<(f32, HitSpot)> = None;
        let consider = |best: &mut Option<(f32, HitSpot)>, t: f32, spot: HitSpot| {
            if best.map(|(bt, _)| t < bt).unwrap_or(true) {
                *best = Some((t, spot));
            }
        };
        if let Some(t) = shape_hit(&book.hitbox.body, enemy, p0, p1, projectile.radius, 1.0) {
            consider(&mut best, t, HitSpot::Body);
        }
        if let Some(tail) = &book.hitbox.tail {
            if crate::bugs::phase_allows(tail.active_phase, enemy.phase) {
                if let Some(t) = shape_hit(tail, enemy, p0, p1, projectile.radius, 1.0) {
                    consider(&mut best, t, HitSpot::Body);
                }
            }
        }
        if let Some(weak) = book.hitbox.weak() {
            let gated = weak.active_only_when == "vulnerable" && !enemy.weak_exposed;
            if !gated && crate::bugs::phase_allows(weak.active_phase, enemy.phase) {
                let grow = if charging && weak.grows_during_charge > 1.0 {
                    weak.grows_during_charge
                } else {
                    1.0
                };
                if let Some(t) = shape_hit(weak, enemy, p0, p1, projectile.radius + bonus, grow) {
                    consider(&mut best, t, HitSpot::Weak);
                }
            }
        }
        let pod_r = book.hitbox.pod_radius();
        for (pi, place) in book.hitbox.pods.iter().enumerate() {
            if enemy.pods.get(pi).copied().unwrap_or(0.0) <= 0.0 {
                continue;
            }
            let center = enemy.pos + rotate_y(enemy.yaw, Vec3::new(place.offset[0], place.offset[1], place.offset[2]));
            if let Some(t) = segment_hits_sphere(p0, p1, center, pod_r + projectile.radius) {
                consider(&mut best, t, HitSpot::Pod(pi));
            }
        }
        if let Some((t, spot)) = best {
            hits.push((idx, t, spot));
        }
    }
    hits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    hits
}

fn shape_hit(shape: &crate::bugs::ShapeDef, enemy: &Enemy, p0: Vec3, p1: Vec3, pad: f32, grow: f32) -> Option<f32> {
    let local0 = rotate_y(-enemy.yaw, p0 - enemy.pos);
    let local1 = rotate_y(-enemy.yaw, p1 - enemy.pos);
    let offset = shape.offset_vec();
    match shape.shape.as_str() {
        "box" => {
            let h = Vec3::new(
                shape.half_extents[0] * grow + pad,
                shape.half_extents[1] * grow + pad,
                shape.half_extents[2] * grow + pad,
            );
            segment_aabb_t(local0, local1, offset - h, offset + h)
        }
        "capsule" => {
            let radius = shape.radius * grow + pad;
            let half = (shape.height * 0.5 * grow - shape.radius * grow).max(0.0);
            let a = offset + Vec3::new(0.0, -half, 0.0);
            let b = offset + Vec3::new(0.0, half, 0.0);
            let (s, t) = closest_seg_seg(local0, local1, a, b);
            let point = local0 + (local1 - local0) * s;
            let core = a + (b - a) * t;
            ((point - core).length_squared() <= radius * radius).then_some(s)
        }
        _ => {
            let radius = shape.radius * grow + pad;
            segment_hits_sphere(local0, local1, offset, radius)
        }
    }
}

fn segment_aabb_t(p0: Vec3, p1: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let d = p1 - p0;
    let mut tmin = 0.0_f32;
    let mut tmax = 1.0_f32;
    for (origin, dir, bmin, bmax) in [
        (p0.x, d.x, min.x, max.x),
        (p0.y, d.y, min.y, max.y),
        (p0.z, d.z, min.z, max.z),
    ] {
        if dir.abs() <= 1e-8 {
            if origin < bmin || origin > bmax {
                return None;
            }
        } else {
            let inv = 1.0 / dir;
            let mut t1 = (bmin - origin) * inv;
            let mut t2 = (bmax - origin) * inv;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }
    if tmax < 0.0 {
        None
    } else {
        Some(tmin.clamp(0.0, 1.0))
    }
}

fn closest_seg_seg(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> (f32, f32) {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let eps = 1e-8;
    if a <= eps && e <= eps {
        return (0.0, 0.0);
    }
    if a <= eps {
        return (0.0, (f / e).clamp(0.0, 1.0));
    }
    let c = d1.dot(r);
    if e <= eps {
        return ((-c / a).clamp(0.0, 1.0), 0.0);
    }
    let b = d1.dot(d2);
    let denom = a * e - b * b;
    let mut s = if denom > eps {
        ((b * f - c * e) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = (b * s + f) / e;
    if t < 0.0 {
        t = 0.0;
        s = (-c / a).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((b - c) / a).clamp(0.0, 1.0);
    }
    (s, t)
}

fn weak_world(content: &Content, enemy: &Enemy) -> Vec3 {
    let Some(shape) = content.bug(enemy.kind).hitbox.weak() else {
        return enemy.pos;
    };
    enemy.pos + rotate_y(enemy.yaw, shape.offset_vec())
}

fn pod_world(content: &Content, enemy: &Enemy, index: usize) -> Vec3 {
    let Some(place) = content.bug(enemy.kind).hitbox.pods.get(index) else {
        return enemy.pos;
    };
    enemy.pos + rotate_y(enemy.yaw, Vec3::new(place.offset[0], place.offset[1], place.offset[2]))
}

fn tick_reload(gun: &mut Gun, dt: f32) {
    if gun.reload <= 0.0 {
        return;
    }
    gun.reload -= dt;
    if gun.reload <= 0.0 {
        let need = gun.mag_size.saturating_sub(gun.ammo);
        let take = need.min(gun.reserve);
        gun.ammo += take;
        gun.reserve -= take;
        gun.reload = 0.0;
    }
}

impl Sim {
    fn arm_reload(&mut self, content: &Content, mods: &Modifiers, downed: bool) {
        let weapon_id = {
            let player = &self.players[0];
            if downed {
                player.sidearm.weapon_id.clone()
            } else {
                player.primary.weapon_id.clone()
            }
        };
        let Some(spec) = content.weapon(&weapon_id) else {
            return;
        };
        let time = spec.reload_time / (1.0 + mods.reload_speed_pct).max(0.05);
        let pos = self.players[0].pos;
        let gun = if downed {
            &mut self.players[0].sidearm
        } else {
            &mut self.players[0].primary
        };
        if gun.reload > 0.0 || gun.ammo >= gun.mag_size || gun.reserve == 0 {
            return;
        }
        gun.reload = time;
        let _ = gun;
        self.sound_at("reload", pos);
    }
}

fn scaled_stats(content: &Content, sim: &Sim, hp: f32, dmg: f32) -> (f32, f32) {
    let tier = content.tier(&sim.difficulty);
    let endless = if sim.endless_mode {
        endless_hp_dmg_mult(&content.endless, sim.director.wave.max(1)) as f32
    } else {
        1.0
    };
    (hp * tier.enemy_hp * endless, dmg * tier.enemy_dmg * endless)
}

fn authored_weak_mult(content: &Content, kind: EnemyKind, boss_fallback: Option<f32>) -> f32 {
    if let Some(shape) = content.bug(kind).hitbox.weak() {
        if shape.mult > 0.0 {
            return shape.mult;
        }
    }
    boss_fallback.unwrap_or(content.combat.default_weakpoint_mult)
}

fn trash_cost(content: &Content, kind: EnemyKind) -> f32 {
    content.enemy(kind).cost
}

fn cheapest_cost(content: &Content, mix: &[f32; 3]) -> f32 {
    let kinds = [EnemyKind::Swarmer, EnemyKind::Spitter, EnemyKind::Brute];
    let mut best = f32::INFINITY;
    for i in 0..3 {
        if mix[i] > 0.0 {
            best = best.min(trash_cost(content, kinds[i]));
        }
    }
    best
}

fn kind_index(kind: EnemyKind) -> usize {
    match kind {
        EnemyKind::Swarmer => 0,
        EnemyKind::Spitter => 1,
        _ => 2,
    }
}

/// Spitter / hive shots are spawned when an enemy asks for one via `vel.y`.
/// Called from `tick_enemies` after movement so the cue and the projectile share a tick.
pub fn launch_pending_shots(sim: &mut Sim, content: &Content) {
    let player_pos = sim.players.first().map(|p| p.pos).unwrap_or(Vec3::ZERO);
    let mut spawned: Vec<Projectile> = Vec::new();
    let mut cues: Vec<SoundEvent> = Vec::new();
    for enemy in &mut sim.enemies {
        let marker = enemy.vel.y;
        if marker == 0.0 {
            continue;
        }
        enemy.vel.y = 0.0;
        if marker == 1.0 && matches!(enemy.ai, AiState::Telegraph { .. }) {
            if enemy.kind == EnemyKind::Spitter {
                let id = content.bug(enemy.kind).audio.charge.clone();
                cues.push(SoundEvent {
                    id,
                    pos: enemy.pos,
                    owner: enemy.id,
                });
            } else if enemy.kind == EnemyKind::HiveTyrant {
                let id = content.bug(enemy.kind).audio.attack.clone();
                cues.push(SoundEvent {
                    id,
                    pos: enemy.pos,
                    owner: enemy.id,
                });
            }
        } else if marker == 2.0 {
            let id = content.bug(EnemyKind::Brute).audio.attack.clone();
            cues.push(SoundEvent {
                id,
                pos: enemy.pos,
                owner: enemy.id,
            });
        } else if marker == 3.0 {
            let id = content.bug(EnemyKind::BroodMother).audio.attack.clone();
            cues.push(SoundEvent {
                id,
                pos: enemy.pos,
                owner: enemy.id,
            });
        } else if (marker - 999.0).abs() < 0.1
            && (enemy.kind == EnemyKind::Spitter || enemy.kind == EnemyKind::HiveTyrant)
        {
            let origin = enemy.pos + Vec3::new(0.0, body_lift_of(content, enemy.kind), 0.0);
            let aim = (player_pos + Vec3::new(0.0, content.arena.eye_height * 0.6, 0.0) - origin)
                .normalized();
            let mut damage = enemy.contact;
            if enemy.kind == EnemyKind::HiveTyrant {
                if let Some(def) = content.boss("hive_tyrant") {
                    let phase = enemy.phase as usize;
                    let base = def.phase_dmg_mult.first().copied().unwrap_or(1.0).max(0.01);
                    let mult = def.phase_dmg_mult.get(phase).copied().unwrap_or(base);
                    damage *= mult / base;
                }
            }
            spawned.push(Projectile {
                id: 0,
                pos: origin,
                vel: aim * content.combat.spit_speed,
                damage,
                radius: content.combat.spit_radius,
                team: Team::Bug,
                ttl: content.combat.spit_ttl,
                pierce: 0,
                bounces: 0,
                hit: Vec::new(),
                kind: ShotKind::Acid,
                acid_ward: enemy.kind == EnemyKind::Spitter,
                owner: enemy.id,
                crit_mult: 1.0,
            });
            let id = if enemy.kind == EnemyKind::Spitter {
                content.bug(EnemyKind::Spitter).audio.fire.clone()
            } else {
                content.bug(enemy.kind).audio.attack.clone()
            };
            cues.push(SoundEvent {
                id,
                pos: origin,
                owner: enemy.id,
            });
        }
    }
    for cue in cues {
        sim.sounds.push(cue);
    }
    for mut shot in spawned {
        shot.id = sim.alloc_id();
        sim.projectiles.push(shot);
    }
}
