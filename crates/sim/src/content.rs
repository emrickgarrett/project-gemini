//! Loads the TOML tables. Balance numbers stay in `content/`, not in systems.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::math::{Aabb, Vec3};
use crate::music::{MusicDirectorCfg, MusicRotation, TrackDef, Transitions};

#[derive(Clone, Debug)]
pub struct Content {
    pub fingerprint: u64,
    pub waves: WavesFile,
    pub difficulty: Vec<Tier>,
    pub controller: Controller,
    pub upgrades: Vec<UpgradeDef>,
    pub endless: Endless,
    pub currency: Currency,
    pub banners: Banners,
    pub duplicate: DuplicateRanks,
    pub bosses: Vec<BossDef>,
    pub weapons: WeaponFile,
    pub hero: Hero,
    pub arena: Arena,
    pub presentation: Presentation,
    pub combat: Combat,
    pub palette: Palette,
    pub music: MusicFile,
    pub audio: AudioTable,
    pub bugs: crate::bugs::BugBook,
    pub crate_reveal: CrateReveal,
}

pub struct Sources<'a> {
    pub waves: &'a str,
    pub difficulty: &'a str,
    pub controller: &'a str,
    pub run_upgrades: &'a str,
    pub endless: &'a str,
    pub currency: &'a str,
    pub banners: &'a str,
    pub duplicate_ranks: &'a str,
    pub bosses: &'a str,
    pub weapons: &'a str,
    pub hero: &'a str,
    pub arena: &'a str,
    pub presentation: &'a str,
    pub combat: &'a str,
    pub palette: &'a str,
    pub music: &'a str,
    pub audio: &'a str,
    pub bugs: &'a str,
    pub crate_reveal: &'a str,
}

pub fn embedded_sources() -> Sources<'static> {
    Sources {
        waves: include_str!("../../../content/waves.toml"),
        difficulty: include_str!("../../../content/difficulty.toml"),
        controller: include_str!("../../../content/controller.toml"),
        run_upgrades: include_str!("../../../content/run_upgrades.toml"),
        endless: include_str!("../../../content/endless.toml"),
        currency: include_str!("../../../content/currency.toml"),
        banners: include_str!("../../../content/banners.toml"),
        duplicate_ranks: include_str!("../../../content/duplicate_ranks.toml"),
        bosses: include_str!("../../../content/bosses.toml"),
        weapons: include_str!("../../../content/weapons.toml"),
        hero: include_str!("../../../content/characters.toml"),
        arena: include_str!("../../../content/arena.toml"),
        presentation: include_str!("../../../content/presentation.toml"),
        combat: include_str!("../../../content/combat.toml"),
        palette: include_str!("../../../content/palette.toml"),
        music: include_str!("../../../content/music.toml"),
        audio: include_str!("../../../content/audio_mix.toml"),
        bugs: include_str!("../../../content/bugs.toml"),
        crate_reveal: include_str!("../../../content/crate_reveal.toml"),
    }
}

impl Content {
    pub fn embedded() -> Self {
        Self::parse(&embedded_sources()).expect("embedded content tables")
    }

    pub fn parse(src: &Sources<'_>) -> Result<Self, String> {
        let fingerprint = fnv_cat(&[
            src.waves,
            src.difficulty,
            src.controller,
            src.run_upgrades,
            src.endless,
            src.currency,
            src.banners,
            src.duplicate_ranks,
            src.bosses,
            src.weapons,
            src.hero,
            src.arena,
            src.presentation,
            src.combat,
            src.palette,
            src.music,
            src.audio,
            src.bugs,
            src.crate_reveal,
        ]);
        let mut bugs: crate::bugs::BugBook = parse("bugs.toml", src.bugs)?;
        crate::bugs::bind_weak_anchors(&mut bugs).map_err(|err| format!("bugs.toml: {err}"))?;
        crate::bugs::require_leg_layout(&bugs).map_err(|err| format!("bugs.toml: {err}"))?;
        let bosses = load_bosses(src.bosses).map_err(|err| format!("bosses.toml: {err}"))?;
        require_spawn_clearance(&bosses, &bugs).map_err(|err| format!("bosses.toml: {err}"))?;
        let (hero, guns) = load_characters(src.hero)?;
        let mut weapons = parse::<WeaponFile>("weapons.toml", src.weapons)?;
        weapons.weapon = guns;
        Ok(Self {
            fingerprint,
            waves: parse("waves.toml", src.waves)?,
            difficulty: parse::<DifficultyFile>("difficulty.toml", src.difficulty)?.tier,
            controller: {
                let controller: Controller = parse("controller.toml", src.controller)?;
                if controller.sim.max_sim_steps_per_frame == 0 {
                    return Err("controller.toml: max_sim_steps_per_frame must be at least 1".into());
                }
                controller
            },
            upgrades: parse::<UpgradeFile>("run_upgrades.toml", src.run_upgrades)?.upgrade,
            endless: parse("endless.toml", src.endless)?,
            currency: parse("currency.toml", src.currency)?,
            banners: parse("banners.toml", src.banners)?,
            duplicate: parse("duplicate_ranks.toml", src.duplicate_ranks)?,
            bosses,
            weapons,
            hero,
            arena: parse("arena.toml", src.arena)?,
            presentation: parse("presentation.toml", src.presentation)?,
            combat: parse("combat.toml", src.combat)?,
            palette: parse("palette.toml", src.palette)?,
            music: {
                let loaded = crate::music::load_music(src.music).map_err(|err| format!("music.toml: {err}"))?;
                MusicFile {
                    director: loaded.director,
                    track: loaded.track,
                    transitions: loaded.transitions,
                    rotation: loaded.rotation,
                }
            },
            audio: crate::audio_mix::load_mix(src.audio).map_err(|err| format!("audio_mix.toml: {err}"))?,
            bugs,
            crate_reveal: parse("crate_reveal.toml", src.crate_reveal)?,
        })
    }

    pub fn tier(&self, id: &str) -> &Tier {
        self.difficulty
            .iter()
            .find(|t| t.id == id)
            .unwrap_or(&self.difficulty[0])
    }

    pub fn weapon(&self, id: &str) -> Option<&WeaponDef> {
        self.weapons.weapon.iter().find(|w| w.id == id)
    }

    pub fn boss(&self, id: &str) -> Option<&BossDef> {
        self.bosses.iter().find(|b| b.id == id)
    }

    pub fn upgrade(&self, id: &str) -> Option<&UpgradeDef> {
        self.upgrades.iter().find(|u| u.id == id)
    }

    pub fn wave(&self, n: u32) -> Option<&WaveDef> {
        self.waves.wave.iter().find(|w| w.n == n)
    }

    pub fn enemy(&self, kind: EnemyKind) -> &EnemyStats {
        match kind {
            EnemyKind::Swarmer => &self.waves.enemies.swarmer,
            EnemyKind::Spitter => &self.waves.enemies.spitter,
            EnemyKind::Brute => &self.waves.enemies.brute,
            EnemyKind::BroodMother | EnemyKind::HiveTyrant => &self.waves.enemies.swarmer,
        }
    }

    pub fn track(&self, id: &str) -> Option<&TrackDef> {
        self.music.track.get(id)
    }

    pub fn combat_track(&self) -> &TrackDef {
        self.music
            .track
            .values()
            .find(|t| t.role == "combat")
            .expect("a combat track")
    }

    pub fn menu_track(&self) -> &TrackDef {
        self.music
            .track
            .values()
            .find(|t| t.role == "menu")
            .expect("a menu track")
    }

    pub fn boss_track(&self, boss_id: &str) -> Option<&TrackDef> {
        self.music
            .track
            .values()
            .find(|t| t.role == "boss" && t.boss_id.as_deref() == Some(boss_id))
    }

    pub fn boss_track_id(&self, boss_id: &str) -> Option<&str> {
        self.music
            .track
            .iter()
            .find(|(_, t)| t.role == "boss" && t.boss_id.as_deref() == Some(boss_id))
            .map(|(id, _)| id.as_str())
    }

    pub fn phase_track_id(&self, boss_id: &str, phase: u8) -> Option<&str> {
        let want = format!("phase{}", phase + 1);
        self.music.track.iter().find(|(id, t)| {
            t.boss_id.as_deref() == Some(boss_id) && (t.role == "boss" || t.role == "phase") && id.ends_with(&want)
        }).map(|(id, _)| id.as_str())
    }

    pub fn enrage_track_id(&self, boss_id: &str) -> Option<&str> {
        self.music
            .track
            .iter()
            .find(|(_, t)| t.role == "enrage" && t.boss_id.as_deref() == Some(boss_id))
            .map(|(id, _)| id.as_str())
    }

    /// Combat loop for this wave.
    /// `slice_waves_1_5` covers waves 1–5 and `slice_waves_6_10` covers the rest of a slice.
    /// Endless rotates by `endless_rotate_every_waves`. Load rejects an empty rotation.
    pub fn combat_loop_id(&self, wave: u32, endless: bool) -> &str {
        let rot = &self.music.rotation;
        if endless {
            let index = (wave.saturating_sub(1) / rot.endless_every) as usize;
            rot.loops[index % rot.loops.len()].as_str()
        } else if wave <= 5 {
            rot.slice_early.as_str()
        } else {
            rot.slice_late.as_str()
        }
    }

    pub fn sfx(&self, id: &str) -> Option<&SfxDef> {
        self.audio.sfx.iter().find(|s| s.id == id)
    }

    pub fn pitch_range(&self, id: &str) -> Option<(f32, f32)> {
        let semi = self.sfx(id)?.pitch_semitones;
        if semi <= 0.0 {
            return None;
        }
        let ratio = 2f32.powf(semi / 12.0);
        Some((1.0 / ratio, ratio))
    }

    pub fn bug(&self, kind: EnemyKind) -> &crate::bugs::BugRow {
        self.bugs.of(kind)
    }
}

fn parse<T: for<'de> Deserialize<'de>>(name: &str, text: &str) -> Result<T, String> {
    toml::from_str(text).map_err(|err| format!("{name}: {err}"))
}

fn fnv_cat(parts: &[&str]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EnemyKind {
    Swarmer,
    Spitter,
    Brute,
    BroodMother,
    HiveTyrant,
}

impl EnemyKind {
    pub fn from_boss_id(id: &str) -> Option<Self> {
        match id {
            "brood_mother" => Some(Self::BroodMother),
            "hive_tyrant" => Some(Self::HiveTyrant),
            _ => None,
        }
    }

    pub fn from_name(id: &str) -> Option<Self> {
        match id {
            "swarmer" => Some(Self::Swarmer),
            "spitter" => Some(Self::Spitter),
            "brute" => Some(Self::Brute),
            "brood_mother" => Some(Self::BroodMother),
            "hive_tyrant" => Some(Self::HiveTyrant),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Swarmer => "swarmer",
            Self::Spitter => "spitter",
            Self::Brute => "brute",
            Self::BroodMother => "brood_mother",
            Self::HiveTyrant => "hive_tyrant",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct WavesFile {
    pub budget: Budget,
    pub enemies: EnemyTable,
    pub wave: Vec<WaveDef>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Budget {
    pub base: f32,
    pub linear: f32,
    pub compound: f32,
    pub per_extra_player: f32,
    pub live_enemy_cap: u32,
    pub queue_cap_waves: f32,
    pub breather_seconds: f32,
    pub upgrade_choices: u32,
    pub boss_hp_per_extra_player: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EnemyTable {
    pub swarmer: EnemyStats,
    pub spitter: EnemyStats,
    pub brute: EnemyStats,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EnemyStats {
    pub cost: f32,
    pub hp: f32,
    pub dmg: f32,
    pub speed: f32,
    #[serde(default)]
    pub range: f32,
    #[serde(default)]
    pub weakpoint_mult: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WaveDef {
    pub n: u32,
    pub mix: [f32; 3],
    #[serde(default)]
    pub boss: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct DifficultyFile {
    tier: Vec<Tier>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Tier {
    pub id: String,
    pub enemy_hp: f32,
    pub enemy_dmg: f32,
    pub currency: f32,
    pub recommended_power: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Controller {
    #[serde(rename = "move")]
    pub movement: MoveCfg,
    pub jump: JumpCfg,
    pub boost: BoostCfg,
    pub camera: CameraCfg,
    pub aim: AimCfg,
    pub health: HealthCfg,
    pub downed: DownedCfg,
    pub ability: AbilityCfg,
    pub sim: SimCfg,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SimCfg {
    pub max_sim_steps_per_frame: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MoveCfg {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub accel_ground: f32,
    pub accel_air: f32,
    pub friction: f32,
    pub crouch_speed: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct JumpCfg {
    pub jump_velocity: f32,
    pub gravity: f32,
    pub coyote_time: f32,
    pub jump_buffer: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BoostCfg {
    pub fuel_max: f32,
    pub fuel_burn_per_s: f32,
    pub fuel_regen_per_s: f32,
    pub regen_delay: f32,
    pub thrust_up: f32,
    pub dash_impulse: f32,
    pub dash_cost: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CameraCfg {
    pub fov_default: f32,
    pub fov_sprint_add: f32,
    pub view_bob_amplitude: f32,
    pub landing_dip: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AimCfg {
    pub mouse_aim_assist: bool,
    pub pad_slowdown_radius_deg: f32,
    pub pad_slowdown_strength: f32,
    pub pad_magnetism_deg: f32,
    pub hitscan_weakpoint_bonus_radius: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HealthCfg {
    pub max_hp: f32,
    pub shield_regen_delay: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DownedCfg {
    pub bleedout_time: f32,
    pub revive_time: f32,
    pub downed_can_fire_sidearm: bool,
    pub revive_invuln: f32,
    pub solo_self_revives_per_wave: u32,
    pub solo_self_revive_time: f32,
    pub solo_self_revive_hp_fraction: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AbilityCfg {
    pub default_cooldown: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StackRule {
    Linear,
    Diminishing,
    Unique,
}

#[derive(Clone, Debug, Deserialize)]
struct UpgradeFile {
    upgrade: Vec<UpgradeDef>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpgradeDef {
    pub id: String,
    pub rarity: String,
    pub stat: String,
    pub stack: StackRule,
    pub value: f32,
    pub k: f32,
    pub cap: f32,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub secondary_stat: Option<String>,
    #[serde(default)]
    pub secondary_value: Option<f32>,
    #[serde(default)]
    pub secondary_cap: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Endless {
    pub count_cap_wave: u32,
    pub hp_dmg_growth_per_wave: f32,
    pub boss_every: u32,
    pub modifier_every: u32,
    pub modifiers: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Currency {
    pub per_wave: f32,
    pub endless_milestone_every: u32,
    pub match_complete_bonus: f32,
    pub boss_kill_bonus: f32,
    pub per_round_cap_mult: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Banners {
    pub rates: BannerRates,
    pub pity: BannerPity,
    pub published: BannerPublished,
    pub cost: BannerCost,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BannerRates {
    pub common: f32,
    pub rare: f32,
    pub epic: f32,
    pub legendary: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BannerPity {
    pub legendary_soft_start: u32,
    pub legendary_soft_step: f32,
    pub legendary_hard: u32,
    pub epic_or_better_guarantee: u32,
    pub carries_between_banners: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BannerPublished {
    pub legendary_overall_rate: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BannerCost {
    pub single: u32,
    pub ten_pull: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DuplicateRanks {
    pub power_per_rank: f32,
    pub rank: Vec<RankRow>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RankRow {
    pub r: u32,
    pub hp: f32,
    pub dmg: f32,
    pub unlock: String,
}

#[derive(Clone, Debug)]
pub struct BossPhase {
    pub move_speed: f32,
    pub core_damage_mult: f32,
    pub tail_hp: f32,
    pub tail_weakpoint_mult: f32,
    pub acid_volley_interval: f32,
    pub acid_telegraph: f32,
    pub acid_damage: f32,
    pub scythe_range: f32,
    pub scythe_damage: f32,
    pub scythe_interval: f32,
    pub reinforce_interval: f32,
    pub reinforce_telegraph: f32,
    pub reinforce_budget_fraction: f32,
    pub reinforce_reserve: bool,
    pub reinforce_queue_on_cap: bool,
}

#[derive(Clone, Debug)]
pub struct BossDef {
    pub id: String,
    pub hp: f32,
    pub speed: f32,
    pub dmg: f32,
    pub range: f32,
    pub spawn_kind: String,
    pub spawn_count: u32,
    pub spawn_interval: f32,
    pub weakpoint_mult: f32,
    pub pod_count: u32,
    pub pod_hp: f32,
    pub phase_thresholds: Vec<f32>,
    pub hatch_telegraph: f32,
    pub hatch_sfx_peak_offset: f32,
    pub swarmers_per_pod: u32,
    pub swarmer_player_scale: f32,
    pub enrage_below_hp: f32,
    pub enrage_hatch_interval: f32,
    pub reserve_on_telegraph: bool,
    pub queue_on_cap: bool,
    pub transition_invuln: f32,
    pub phases: Vec<BossPhase>,
    pub intro_sfx: String,
    pub death_sfx: String,
    pub fizzle_sfx: String,
    pub charge_sfx: String,
    pub hatch_sfx: String,
    pub acid_sfx: String,
    pub scythe_sfx: String,
    pub reinforce_kind: String,
    pub fan_step: f32,
    pub spawn_radius: f32,
}

impl BossDef {
    pub fn phase(&self, index: u8) -> Option<&BossPhase> {
        self.phases.get(index as usize)
    }
}

fn load_bosses(text: &str) -> Result<Vec<BossDef>, String> {
    #[derive(Deserialize)]
    struct File {
        brood_mother: BroodSrc,
        hive_tyrant: HiveSrc,
    }
    #[derive(Deserialize)]
    struct BroodSrc {
        hp: f32,
        weakpoint_mult: f32,
        move_speed: f32,
        dmg: f32,
        range: f32,
        pod_count: u32,
        pod_hp: f32,
        hatch_interval: f32,
        hatch_telegraph: f32,
        hatch_sfx_peak_offset: f32,
        swarmers_per_pod: u32,
        swarmer_player_scale: f32,
        enrage_below_hp: f32,
        enrage_hatch_interval: f32,
        reserve_on_telegraph: bool,
        queue_on_cap: bool,
        spawn_kind: String,
        intro_sfx: String,
        death_sfx: String,
        fizzle_sfx: String,
        charge_sfx: String,
        hatch_sfx: String,
        hatch_fan_step: f32,
        spawn_radius: f32,
    }
    #[derive(Deserialize)]
    struct HiveSrc {
        hp: f32,
        weakpoint_mult: f32,
        transition_invuln: f32,
        phase_thresholds: Vec<f32>,
        spawn_kind: String,
        intro_sfx: String,
        death_sfx: String,
        fizzle_sfx: String,
        acid_sfx: String,
        scythe_sfx: String,
        reinforce_kind: String,
        hatch_fan_step: f32,
        spawn_radius: f32,
        phase1: PhaseSrc,
        phase2: PhaseSrc,
        phase3: PhaseSrc,
    }
    #[derive(Deserialize, Default)]
    struct PhaseSrc {
        #[serde(default)]
        move_speed: f32,
        #[serde(default)]
        core_damage_mult: f32,
        #[serde(default)]
        tail_hp: f32,
        #[serde(default)]
        tail_weakpoint_mult: f32,
        #[serde(default)]
        acid_volley_interval: f32,
        #[serde(default)]
        acid_telegraph: f32,
        #[serde(default)]
        acid_damage: f32,
        #[serde(default)]
        scythe_range: f32,
        #[serde(default)]
        scythe_damage: f32,
        #[serde(default)]
        scythe_interval: f32,
        #[serde(default)]
        reinforce_interval: f32,
        #[serde(default)]
        reinforce_telegraph: f32,
        #[serde(default)]
        reinforce_budget_fraction: f32,
        #[serde(default)]
        reinforce_reserve_on_telegraph: bool,
        #[serde(default)]
        reinforce_queue_on_cap: bool,
    }
    let file: File = toml::from_str(text).map_err(|err| err.to_string())?;
    let brood = file.brood_mother;
    let hive = file.hive_tyrant;
    if EnemyKind::from_name(&brood.spawn_kind).is_none() {
        return Err(format!("brood_mother spawn_kind \"{}\" is unknown", brood.spawn_kind));
    }
    if EnemyKind::from_name(&hive.spawn_kind).is_none() {
        return Err(format!("hive_tyrant spawn_kind \"{}\" is unknown", hive.spawn_kind));
    }
    if EnemyKind::from_name(&hive.reinforce_kind).is_none() {
        return Err(format!("hive_tyrant reinforce_kind \"{}\" is unknown", hive.reinforce_kind));
    }
    if brood.hatch_fan_step <= 0.0 {
        return Err("brood_mother hatch_fan_step must be > 0".into());
    }
    if brood.spawn_radius <= 0.0 {
        return Err("brood_mother spawn_radius must be > 0".into());
    }
    if hive.hatch_fan_step <= 0.0 {
        return Err("hive_tyrant hatch_fan_step must be > 0".into());
    }
    if hive.spawn_radius <= 0.0 {
        return Err("hive_tyrant spawn_radius must be > 0".into());
    }
    let phase = |src: PhaseSrc| BossPhase {
        move_speed: src.move_speed,
        core_damage_mult: src.core_damage_mult,
        tail_hp: src.tail_hp,
        tail_weakpoint_mult: src.tail_weakpoint_mult,
        acid_volley_interval: src.acid_volley_interval,
        acid_telegraph: src.acid_telegraph,
        acid_damage: src.acid_damage,
        scythe_range: src.scythe_range,
        scythe_damage: src.scythe_damage,
        scythe_interval: src.scythe_interval,
        reinforce_interval: src.reinforce_interval,
        reinforce_telegraph: src.reinforce_telegraph,
        reinforce_budget_fraction: src.reinforce_budget_fraction,
        reinforce_reserve: src.reinforce_reserve_on_telegraph,
        reinforce_queue_on_cap: src.reinforce_queue_on_cap,
    };
    let phases = vec![phase(hive.phase1), phase(hive.phase2), phase(hive.phase3)];
    if phases[0].tail_weakpoint_mult <= 0.0 {
        return Err("hive_tyrant.phase1.tail_weakpoint_mult must be > 0".into());
    }
    for (i, row) in phases.iter().enumerate() {
        if row.core_damage_mult <= 0.0 {
            return Err(format!("hive_tyrant phase {} core_damage_mult must be > 0", i + 1));
        }
    }
    let first = &phases[0];
    Ok(vec![
        BossDef {
            id: "brood_mother".into(),
            hp: brood.hp,
            speed: brood.move_speed,
            dmg: brood.dmg,
            range: brood.range,
            spawn_kind: brood.spawn_kind,
            spawn_count: brood.swarmers_per_pod,
            spawn_interval: brood.hatch_interval,
            weakpoint_mult: brood.weakpoint_mult,
            pod_count: brood.pod_count,
            pod_hp: brood.pod_hp,
            phase_thresholds: Vec::new(),
            hatch_telegraph: brood.hatch_telegraph,
            hatch_sfx_peak_offset: brood.hatch_sfx_peak_offset,
            swarmers_per_pod: brood.swarmers_per_pod,
            swarmer_player_scale: brood.swarmer_player_scale,
            enrage_below_hp: brood.enrage_below_hp,
            enrage_hatch_interval: brood.enrage_hatch_interval,
            reserve_on_telegraph: brood.reserve_on_telegraph,
            queue_on_cap: brood.queue_on_cap,
            transition_invuln: 0.0,
            phases: Vec::new(),
            intro_sfx: brood.intro_sfx,
            death_sfx: brood.death_sfx,
            fizzle_sfx: brood.fizzle_sfx,
            charge_sfx: brood.charge_sfx,
            hatch_sfx: brood.hatch_sfx,
            acid_sfx: String::new(),
            scythe_sfx: String::new(),
            reinforce_kind: String::new(),
            fan_step: brood.hatch_fan_step,
            spawn_radius: brood.spawn_radius,
        },
        BossDef {
            id: "hive_tyrant".into(),
            hp: hive.hp,
            speed: first.move_speed,
            dmg: first.scythe_damage,
            range: first.scythe_range,
            spawn_kind: hive.spawn_kind,
            spawn_count: 0,
            spawn_interval: 0.0,
            weakpoint_mult: hive.weakpoint_mult,
            pod_count: 0,
            pod_hp: 0.0,
            phase_thresholds: hive.phase_thresholds,
            hatch_telegraph: 0.0,
            hatch_sfx_peak_offset: 0.0,
            swarmers_per_pod: 0,
            swarmer_player_scale: 0.0,
            enrage_below_hp: 0.0,
            enrage_hatch_interval: 0.0,
            reserve_on_telegraph: false,
            queue_on_cap: false,
            transition_invuln: hive.transition_invuln,
            phases,
            intro_sfx: hive.intro_sfx,
            death_sfx: hive.death_sfx,
            fizzle_sfx: hive.fizzle_sfx,
            charge_sfx: String::new(),
            hatch_sfx: String::new(),
            acid_sfx: hive.acid_sfx,
            scythe_sfx: hive.scythe_sfx,
            reinforce_kind: hive.reinforce_kind,
            fan_step: hive.hatch_fan_step,
            spawn_radius: hive.spawn_radius,
        },
    ])
}

fn body_hit_radius(shape: &crate::bugs::ShapeDef) -> f32 {
    if shape.radius > 0.0 {
        shape.radius
    } else {
        shape.half_extents[0].max(shape.half_extents[1]).max(shape.half_extents[2])
    }
}

/// Adds must spawn outside the boss body. `spawn_radius` has to clear both radii.
fn require_spawn_clearance(bosses: &[BossDef], bugs: &crate::bugs::BugBook) -> Result<(), String> {
    for boss in bosses {
        let boss_kind = EnemyKind::from_name(&boss.id).ok_or_else(|| format!("{} is not a bug", boss.id))?;
        let body_r = body_hit_radius(&bugs.of(boss_kind).hitbox.body);
        for add_name in [&boss.spawn_kind, &boss.reinforce_kind] {
            if add_name.is_empty() {
                continue;
            }
            let add_kind = EnemyKind::from_name(add_name)
                .ok_or_else(|| format!("{} add \"{add_name}\" is unknown", boss.id))?;
            let add_r = body_hit_radius(&bugs.of(add_kind).hitbox.body);
            let limit = body_r + add_r;
            if boss.spawn_radius <= limit {
                return Err(format!(
                    "{} spawn_radius {} must be > body radius {} + {} radius {}",
                    boss.id, boss.spawn_radius, body_r, add_name, add_r
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
pub struct WeaponFile {
    pub sprint_to_fire_delay: f32,
    pub weapon: Vec<WeaponDef>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WeaponDef {
    pub id: String,
    pub slot: String,
    pub damage: f32,
    pub fire_interval: f32,
    pub magazine: u32,
    pub reserve: u32,
    pub reload_time: f32,
    pub projectile_speed: f32,
    pub projectile_radius: f32,
    pub spread: f32,
}

#[derive(Clone, Debug)]
pub struct Hero {
    pub id: String,
    pub name: String,
    pub primary: String,
    pub sidearm: String,
    pub accent: String,
    pub max_hp: f32,
    pub move_speed_mult: f32,
    pub ability_id: String,
    pub ability_cooldown: f32,
    pub overdrive_duration: f32,
    pub overdrive_fire_rate: f32,
    pub infinite_magazine: bool,
    pub kill_cd_refund: f32,
    /// Camera-local viewmodel position. +X is screen right, -Y is screen down.
    pub viewmodel_offset: [f32; 3],
    pub viewmodel_fov: f32,
}

fn load_characters(text: &str) -> Result<(Hero, Vec<WeaponDef>), String> {
    let root: toml::Value = toml::from_str(text).map_err(|err| err.to_string())?;
    let table = root.as_table().ok_or_else(|| "characters.toml is not a table".to_string())?;
    let heroes = table
        .get("hero")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "characters.toml is missing [[hero]]".to_string())?;
    let unlock = table
        .get("unlock")
        .and_then(|v| v.as_table())
        .ok_or_else(|| "characters.toml is missing [unlock]".to_string())?;
    let starter = unlock
        .get("starter")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "characters.toml [unlock] is missing starter".to_string())?;
    let weapons = table
        .get("weapons")
        .and_then(|v| v.as_table())
        .ok_or_else(|| "characters.toml is missing [weapons]".to_string())?;
    let row = heroes
        .iter()
        .find(|hero| hero.get("id").and_then(|v| v.as_str()) == Some(starter))
        .ok_or_else(|| format!("characters.toml starter \"{starter}\" is not a [[hero]]"))?;
    let row = row.as_table().ok_or_else(|| "characters.toml hero row is not a table".to_string())?;
    let ability = row
        .get("ability")
        .and_then(|v| v.as_table())
        .ok_or_else(|| format!("characters.toml {starter} is missing ability"))?;
    let ability_id = req_str(ability, "id", &format!("{starter} ability"))?;
    if ability_id != "overdrive" {
        return Err(format!("starter ability \"{ability_id}\" has no implementation"));
    }
    let weapon_id = req_str(row, "weapon", starter)?;
    let primary = character_weapon(weapons, weapon_id, "primary")?;
    let sidearm = character_weapon(weapons, "sidearm", "sidearm")?;
    let hero = Hero {
        id: starter.to_string(),
        name: req_str(row, "name", starter)?.to_string(),
        primary: weapon_id.to_string(),
        sidearm: "sidearm".into(),
        accent: req_str(row, "accent", starter)?.to_string(),
        max_hp: req_f32(row, "max_hp", starter)?,
        move_speed_mult: req_f32(row, "move_speed_mult", starter)?,
        ability_id: ability_id.to_string(),
        ability_cooldown: req_f32(ability, "cooldown", "overdrive")?,
        overdrive_duration: req_f32(ability, "duration", "overdrive")?,
        overdrive_fire_rate: req_f32(ability, "fire_rate", "overdrive")?,
        infinite_magazine: ability
            .get("infinite_magazine")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| "overdrive is missing infinite_magazine".to_string())?,
        kill_cd_refund: req_f32(ability, "kill_cd_refund", "overdrive")?,
        viewmodel_offset: req_vec3(row, "viewmodel_offset", starter)?,
        viewmodel_fov: req_f32(row, "viewmodel_fov", starter)?,
    };
    if hero.ability_cooldown <= 0.0 || hero.overdrive_duration <= 0.0 {
        return Err("overdrive cooldown and duration must be > 0".into());
    }
    if hero.max_hp <= 0.0 {
        return Err(format!("{starter} max_hp must be > 0"));
    }
    if hero.move_speed_mult <= 0.0 {
        return Err(format!("{starter} move_speed_mult must be > 0"));
    }
    if hero.viewmodel_fov <= 1.0 || hero.viewmodel_fov >= 170.0 {
        return Err(format!(
            "characters.toml {starter} viewmodel_fov must be between 1 and 170"
        ));
    }
    parse_accent(&hero.accent)?;
    Ok((hero, vec![primary, sidearm]))
}

fn parse_accent(hex: &str) -> Result<(), String> {
    let digits = hex.trim().trim_start_matches('#');
    if digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(());
    }
    Err(format!("accent \"{hex}\" must be #RRGGBB"))
}

fn character_weapon(weapons: &toml::map::Map<String, toml::Value>, id: &str, slot: &str) -> Result<WeaponDef, String> {
    let row = weapons
        .get(id)
        .and_then(|v| v.as_table())
        .ok_or_else(|| format!("characters.toml is missing [weapons.{id}]"))?;
    let rps = req_f32(row, "rps", id)?;
    if rps <= 0.0 {
        return Err(format!("characters.toml {id} rps must be > 0"));
    }
    let spread_deg = req_f32(row, "spread_deg", id)?;
    Ok(WeaponDef {
        id: id.to_string(),
        slot: slot.to_string(),
        damage: req_f32(row, "dmg", id)?,
        fire_interval: 1.0 / rps,
        magazine: req_u32(row, "mag", id)?,
        reserve: req_u32(row, "reserve", id)?,
        reload_time: req_f32(row, "reload", id)?,
        projectile_speed: req_f32(row, "projectile_speed", id)?,
        projectile_radius: req_f32(row, "projectile_radius", id)?,
        spread: spread_deg.to_radians(),
    })
}

fn req_str<'a>(table: &'a toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<&'a str, String> {
    table
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("characters.toml {what} is missing {key}"))
}

fn req_vec3(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<[f32; 3], String> {
    let value = table
        .get(key)
        .ok_or_else(|| format!("characters.toml {what} is missing {key}"))?;
    let arr = value
        .as_array()
        .ok_or_else(|| format!("characters.toml {what} {key} must be 3 numbers"))?;
    if arr.len() != 3 {
        return Err(format!("characters.toml {what} {key} must be 3 numbers"));
    }
    let mut out = [0.0; 3];
    for (i, slot) in arr.iter().enumerate() {
        if let Some(n) = slot.as_float() {
            out[i] = n as f32;
        } else if let Some(n) = slot.as_integer() {
            out[i] = n as f32;
        } else {
            return Err(format!("characters.toml {what} {key} must be 3 numbers"));
        }
    }
    Ok(out)
}

fn req_f32(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<f32, String> {
    let value = table.get(key).ok_or_else(|| format!("characters.toml {what} is missing {key}"))?;
    if let Some(n) = value.as_float() {
        return Ok(n as f32);
    }
    if let Some(n) = value.as_integer() {
        return Ok(n as f32);
    }
    Err(format!("characters.toml {what} {key} must be a number"))
}

fn req_u32(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<u32, String> {
    let n = req_f32(table, key, what)?;
    if n < 0.0 {
        return Err(format!("characters.toml {what} {key} must be >= 0"));
    }
    Ok(n.round() as u32)
}

#[derive(Clone, Debug, Deserialize)]
pub struct ArenaFile {
    pub half_extent: f32,
    pub spawn_ring: f32,
    pub eye_height: f32,
    pub crouch_eye: f32,
    #[serde(default)]
    pub obstacle: Vec<ObstacleRaw>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ObstacleRaw {
    min: [f32; 3],
    max: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct Arena {
    pub half_extent: f32,
    pub spawn_ring: f32,
    pub eye_height: f32,
    pub crouch_eye: f32,
    pub obstacles: Vec<Aabb>,
}

impl<'de> Deserialize<'de> for Arena {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = ArenaFile::deserialize(deserializer)?;
        Ok(Self {
            half_extent: raw.half_extent,
            spawn_ring: raw.spawn_ring,
            eye_height: raw.eye_height,
            crouch_eye: raw.crouch_eye,
            obstacles: raw
                .obstacle
                .into_iter()
                .map(|o| Aabb {
                    min: Vec3::new(o.min[0], o.min[1], o.min[2]),
                    max: Vec3::new(o.max[0], o.max[1], o.max[2]),
                })
                .collect(),
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Presentation {
    pub fov_min: f32,
    pub fov_max: f32,
    pub mouse_sensitivity: f32,
    pub gamepad_look_deg_per_sec: f32,
    pub view_bob_hz: f32,
    pub pad_magnetism_blend: f32,
    pub pitch_limit_deg: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Combat {
    pub spawn_interval: f32,
    pub group_budget: f32,
    pub cell_size: f32,
    pub telegraph_seconds: f32,
    pub spitter_cooldown: f32,
    pub spitter_min_range: f32,
    pub keep_away_ratios: [f32; 2],
    pub swarmer_melee_interval: f32,
    pub brute_cooldown: f32,
    pub brute_lunge_seconds: f32,
    pub boss_melee_interval: f32,
    pub brute_recover_seconds: f32,
    pub brute_vulnerable_hp_fraction: f32,
    pub separation_radius: f32,
    pub separation_strength: f32,
    pub spit_speed: f32,
    pub spit_radius: f32,
    pub spit_ttl: f32,
    pub player_projectile_ttl: f32,
    pub crit_multiplier: f32,
    pub ricochet_damage_pct: f32,
    pub ricochet_range: f32,
    pub chain_targets: u32,
    pub chain_damage_pct: f32,
    pub chain_range: f32,
    pub nova_damage: f32,
    pub nova_radius: f32,
    pub drone_dps: f32,
    pub drone_range: f32,
    pub drone_orbit_speed: f32,
    pub drone_orbit_radius: f32,
    pub drone_height: f32,
    pub shockwave_damage: f32,
    pub shockwave_impulse: f32,
    pub glass_cannon_damage_pct: f32,
    pub glass_cannon_max_hp_pct: f32,
    pub squad_aura_radius: f32,
    pub ability_damage: f32,
    pub ability_radius: f32,
    pub shield_regen_per_s: f32,
    pub ammo_pickup_amount: u32,
    pub pickup_base_radius: f32,
    pub revive_range: f32,
    pub default_weakpoint_mult: f32,
    pub knockback_resist_boss: f32,
    pub rarity_weights: RarityWeights,
    pub hit_radius: HitRadius,
    pub body_lift: BodyLift,
    pub weakpoint: Weakpoints,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RarityWeights {
    pub common: f32,
    pub rare: f32,
    pub epic: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HitRadius {
    pub player: f32,
    pub swarmer: f32,
    pub spitter: f32,
    pub brute: f32,
    pub brood_mother: f32,
    pub hive_tyrant: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BodyLift {
    pub swarmer: f32,
    pub spitter: f32,
    pub brute: f32,
    pub brood_mother: f32,
    pub hive_tyrant: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Weakpoints {
    pub swarmer: Weakpoint,
    pub spitter: Weakpoint,
    pub brute: Weakpoint,
    pub brood_mother: Weakpoint,
    pub hive_tyrant: Weakpoint,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Weakpoint {
    pub offset: [f32; 3],
    pub radius: f32,
}

impl Weakpoint {
    pub fn offset_vec(&self) -> Vec3 {
        Vec3::new(self.offset[0], self.offset[1], self.offset[2])
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Palette {
    pub hero: HeroColors,
    pub shots: ShotColors,
    pub swarmer: SwarmerColors,
    pub spitter: SpitterColors,
    pub brute: BruteColors,
    pub arena: ArenaColors,
    pub rarity: RarityColors,
    pub bugs: BugColors,
    pub keys: PaletteKeys,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HeroColors {
    pub teal: String,
    pub cobalt: String,
    pub white: String,
    pub accent: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ShotColors {
    pub player: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SwarmerColors {
    pub shell: String,
    pub glow: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SpitterColors {
    pub carapace: String,
    pub glow: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BruteColors {
    pub shell: String,
    pub cracks: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ArenaColors {
    pub rock_dark: String,
    pub rock_light: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RarityColors {
    pub common: String,
    pub rare: String,
    pub epic: String,
    pub legendary: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BugColors {
    pub acid: String,
    pub amber: String,
    pub magenta: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PaletteKeys {
    pub swarmer_amber: String,
    pub swarmer_glow: String,
    pub spitter_carapace: String,
    pub spitter_glow: String,
    pub brute_amber: String,
    pub brute_cracks: String,
    pub brood_sac: String,
    pub bud_glow: String,
}

impl Palette {
    pub fn key(&self, name: &str) -> [f32; 4] {
        let hex = match name {
            "swarmer_amber" => self.keys.swarmer_amber.as_str(),
            "swarmer_glow" => self.keys.swarmer_glow.as_str(),
            "spitter_carapace" => self.keys.spitter_carapace.as_str(),
            "spitter_glow" => self.keys.spitter_glow.as_str(),
            "brute_amber" => self.keys.brute_amber.as_str(),
            "brute_cracks" => self.keys.brute_cracks.as_str(),
            "brood_sac" => self.keys.brood_sac.as_str(),
            "bud_glow" => self.keys.bud_glow.as_str(),
            "hero_teal" | "teal" => self.hero.teal.as_str(),
            "hero_cobalt" | "cobalt" => self.hero.cobalt.as_str(),
            "hero_white" | "white" => self.hero.white.as_str(),
            "hero_accent" | "accent" => self.hero.accent.as_str(),
            "shots_player" => self.shots.player.as_str(),
            "arena_dark" => self.arena.rock_dark.as_str(),
            "arena_light" => self.arena.rock_light.as_str(),
            "rarity_common" => self.rarity.common.as_str(),
            "rarity_rare" => self.rarity.rare.as_str(),
            "rarity_epic" => self.rarity.epic.as_str(),
            "rarity_legendary" => self.rarity.legendary.as_str(),
            _ => "#FFFFFF",
        };
        hex_srgb(hex)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct CrateReveal {
    pub fakeout_enabled: bool,
    pub common: CrateTier,
    pub rare: CrateTier,
    pub epic: CrateTier,
    pub legendary: CrateTier,
    pub multi_pull: CrateMulti,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CrateTier {
    pub tease: String,
    pub sting: String,
    pub drop: [f32; 2],
    pub crack: [f32; 2],
    pub burst: f32,
    pub rise: [f32; 2],
    pub card: f32,
    pub fakeout_allowed: bool,
    pub fakeout_chance: f32,
    pub fakeout_window: [f32; 2],
    #[serde(default)]
    pub freeze: Option<[f32; 2]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CrateMulti {
    pub pod_interval: f32,
    pub full_sequence_for: String,
    pub summary_grid: bool,
}

impl CrateReveal {
    pub fn tier(&self, id: &str) -> Option<&CrateTier> {
        match id {
            "common" => Some(&self.common),
            "rare" => Some(&self.rare),
            "epic" => Some(&self.epic),
            "legendary" => Some(&self.legendary),
            _ => None,
        }
    }

    pub fn tease(&self, id: &str) -> Option<&str> {
        self.tier(id).map(|tier| tier.tease.as_str())
    }

    /// True while `t` is inside the tier's fake-out window and the global switch allows it.
    pub fn fakeout_open(&self, id: &str, t: f32) -> bool {
        let Some(tier) = self.tier(id) else {
            return false;
        };
        if !self.fakeout_enabled || !tier.fakeout_allowed || tier.fakeout_chance <= 0.0 {
            return false;
        }
        t >= tier.fakeout_window[0] && t <= tier.fakeout_window[1]
    }
}

#[derive(Clone, Debug)]
pub struct MusicFile {
    pub director: MusicDirectorCfg,
    pub track: BTreeMap<String, TrackDef>,
    pub transitions: Transitions,
    pub rotation: MusicRotation,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AudioTable {
    pub voice_cap: usize,
    pub crowd_radius: f32,
    pub crowd_full_count: u32,
    pub min_distance: f32,
    #[serde(default)]
    pub sfx: Vec<SfxDef>,
    #[serde(default)]
    pub sting: Vec<StingDef>,
    #[serde(default)]
    pub pitch: Vec<PitchDef>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SfxDef {
    pub id: String,
    pub path: String,
    pub loudness: f32,
    /// Playback mix. Distinct from `loudness`, which is only voice priority.
    #[serde(default)]
    pub volume: f32,
    #[serde(default)]
    pub crowd: bool,
    #[serde(default)]
    pub critical: bool,
    #[serde(default)]
    pub max_instances: u32,
    #[serde(default)]
    pub looped: bool,
    /// Half-range of the deterministic pitch offset, in semitones.
    #[serde(default)]
    pub pitch_semitones: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StingDef {
    pub id: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PitchDef {
    pub id: String,
    pub min: f32,
    pub max: f32,
}

pub fn hex_srgb(hex: &str) -> [f32; 4] {
    let hex = hex.trim().trim_start_matches('#');
    let parse = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0) as f32 / 255.0;
    if hex.len() >= 6 {
        [parse(0), parse(2), parse(4), 1.0]
    } else {
        [1.0, 1.0, 1.0, 1.0]
    }
}

pub fn wave_budget(budget: &Budget, wave: u32, players: u32, endless: &Endless, endless_mode: bool) -> f32 {
    let wave_for_count = if endless_mode {
        wave.min(endless.count_cap_wave).max(1)
    } else {
        wave.max(1)
    };
    let w = wave_for_count as f64;
    let players = players.max(1) as f64;
    let value = budget.base as f64
        * (1.0 + budget.linear as f64 * (w - 1.0))
        * crate::math::powi(budget.compound, wave_for_count.saturating_sub(1)) as f64
        * (1.0 + budget.per_extra_player as f64 * (players - 1.0));
    value.clamp(0.0, 1.0e12) as f32
}

pub fn endless_hp_dmg_mult(endless: &Endless, wave: u32) -> f64 {
    if wave <= endless.count_cap_wave {
        1.0
    } else {
        1.0 + endless.hp_dmg_growth_per_wave as f64 * (wave - endless.count_cap_wave) as f64
    }
}

pub fn rank_bonus(table: &DuplicateRanks, rank: u32) -> (f32, f32) {
    table
        .rank
        .iter()
        .filter(|row| row.r <= rank)
        .max_by_key(|row| row.r)
        .map(|row| (row.hp, row.dmg))
        .unwrap_or((0.0, 0.0))
}

pub fn power_level(table: &DuplicateRanks, rank: u32) -> f32 {
    rank as f32 * table.power_per_rank
}

/// Currency the backend formula will recognize. The flat endless milestone
/// bonus lives only in a comment in `currency.toml`, so it is not applied here.
pub fn currency_reward(
    currency: &Currency,
    wave_reached: u32,
    tier_currency: f32,
    completed: bool,
    boss_kills: u32,
) -> f32 {
    let mut total = currency.per_wave * (wave_reached.min(20) as f32) * tier_currency;
    total += boss_kills as f32 * currency.boss_kill_bonus;
    if completed {
        total += currency.match_complete_bonus * tier_currency;
    }
    total
}

pub fn weakpoint_of(content: &Content, kind: EnemyKind) -> Weakpoint {
    let shape = content.bug(kind).hitbox.weak().expect("bug weak volume");
    Weakpoint {
        offset: shape.offset,
        radius: shape.bounding_radius().max(shape.radius),
    }
}

pub fn hit_radius_of(content: &Content, kind: EnemyKind) -> f32 {
    content.bug(kind).hitbox.body.bounding_radius()
}

pub fn body_lift_of(content: &Content, kind: EnemyKind) -> f32 {
    content.bug(kind).hitbox.body.offset[1]
}
