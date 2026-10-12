//! Loads the TOML tables. Balance numbers stay in `content/`, not in systems.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::math::Vec3;
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
    pub coop: CoopRules,
    /// `[[hero]]` ids in file order. Only the starter is playable.
    pub roster: Vec<String>,
    /// Every hero row, including locked ones. Ownership comes from `[unlock]`.
    pub heroes: Vec<HeroCard>,
    pub warnings: Warnings,
    pub cosmetics: Cosmetics,
    pub ui: Ui,
    /// Full-body card meshes and the first-person glove. One list with the portraits.
    pub heroes_visual: HeroesVisual,
    /// Meshy casts. Bodies, weapons, and bugs. The sim stores the record and does not read the files.
    pub models: Vec<CastModel>,
    /// Deck, low rim, and the review camera. Not a collider.
    pub arena_dressing: ArenaDressing,
    /// Mask threshold, rim strength, overdrive emissive, and the side-sheet orbit.
    pub cast: CastLook,
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
    pub coop: &'a str,
    pub cosmetics: &'a str,
    pub ui: &'a str,
    pub heroes_visual: &'a str,
    pub arena_visual: &'a str,
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
        arena: include_str!("../../../content/arena_01.toml"),
        presentation: include_str!("../../../content/presentation.toml"),
        combat: include_str!("../../../content/combat.toml"),
        palette: include_str!("../../../content/palette.toml"),
        music: include_str!("../../../content/music.toml"),
        audio: include_str!("../../../content/audio_mix.toml"),
        bugs: include_str!("../../../content/bugs.toml"),
        crate_reveal: include_str!("../../../content/crate_reveal.toml"),
        coop: include_str!("../../../content/coop.toml"),
        cosmetics: include_str!("../../../content/cosmetics.toml"),
        ui: include_str!("../../../content/ui.toml"),
        heroes_visual: include_str!("../../../content/heroes_visual.toml"),
        arena_visual: include_str!("../../../content/arena_visual.toml"),
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
            src.coop,
            src.cosmetics,
            src.ui,
            src.heroes_visual,
            src.arena_visual,
        ]);
        let mut bugs: crate::bugs::BugBook = parse("bugs.toml", src.bugs)?;
        crate::bugs::bind_weak_anchors(&mut bugs).map_err(|err| format!("bugs.toml: {err}"))?;
        crate::bugs::require_leg_layout(&bugs).map_err(|err| format!("bugs.toml: {err}"))?;
        let bosses = load_bosses(src.bosses).map_err(|err| format!("bosses.toml: {err}"))?;
        require_spawn_clearance(&bosses, &bugs).map_err(|err| format!("bosses.toml: {err}"))?;
        let mut palette: Palette = parse("palette.toml", src.palette)?;
        let heroes_visual = load_heroes_visual(src.heroes_visual, &palette)?;
        let bug_models = load_model_table(
            include_str!("../../../content/bugs_visual.toml"),
            &palette,
            "bugs_visual.toml",
        )?;
        let mut models = heroes_visual.models.clone();
        for model in bug_models {
            if models.iter().any(|have| have.id == model.id) {
                return Err(format!("duplicate cast model \"{}\"", model.id));
            }
            models.push(model);
        }
        let mesh_ids: Vec<String> = heroes_visual.meshes.iter().map(|mesh| mesh.id.clone()).collect();
        let (mut hero, guns, roster, heroes, warnings) = load_characters(src.hero, &mesh_ids)?;
        require_cast_roster(&models, &heroes, &bugs)?;
        let cast = load_cast_look(include_str!("../../../content/cast.toml"), &palette)?;
        for mesh in &heroes_visual.meshes {
            if !heroes.iter().any(|card| card.portrait.mesh == mesh.id) {
                return Err(format!(
                    "heroes_visual.toml mesh \"{}\" is not a characters.toml portrait",
                    mesh.id
                ));
            }
        }
        let arena_dressing = load_arena_dressing(src.arena_visual)?;
        let mut weapons = parse::<WeaponFile>("weapons.toml", src.weapons)?;
        weapons.weapon = guns;
        let audio = crate::audio_mix::load_mix(src.audio).map_err(|err| format!("audio_mix.toml: {err}"))?;
        let coop = load_coop(src.coop).map_err(|err| format!("coop.toml: {err}"))?;
        validate_host_away_cues(&audio, &coop)?;
        validate_act_now_cues(&audio)?;
        let crate_reveal: CrateReveal = parse("crate_reveal.toml", src.crate_reveal)?;
        validate_reveal_cues(&crate_reveal, &audio)?;
        validate_reveal_layout(&crate_reveal)?;
        let presentation: Presentation = parse("presentation.toml", src.presentation)?;
        validate_away_hud(&presentation.hud.away)?;
        let ui = load_ui(src.ui)?;
        let controller: Controller = parse("controller.toml", src.controller)?;
        if controller.sim.max_sim_steps_per_frame == 0 {
            return Err("controller.toml: max_sim_steps_per_frame must be at least 1".into());
        }
        validate_downed(&controller.downed)?;
        validate_ledge_catch(&controller.ledge_catch)?;
        let difficulty = parse::<DifficultyFile>("difficulty.toml", src.difficulty)?.tier;
        require_palette_keys(&palette, &bugs, &presentation.hud.away, &crate_reveal.layout, &ui, &heroes)
            .map_err(|err| format!("palette.toml: {err}"))?;
        let accent_key = heroes
            .iter()
            .find(|card| card.id == hero.id)
            .map(|card| card.accent_key.clone())
            .ok_or_else(|| format!("characters.toml starter {} has no card", hero.id))?;
        let accent = palette
            .hex(&accent_key)
            .ok_or_else(|| format!("characters.toml {} accent_key \"{accent_key}\" does not resolve", hero.id))?
            .to_string();
        parse_accent(&accent)?;
        hero.accent = accent.clone();
        // The palette key on the hero card is the only accent source.
        palette.hero.accent = accent;
        validate_ui(
            &ui,
            src.ui,
            &presentation,
            &controller,
            &audio,
            &difficulty,
            &crate_reveal.layout,
            src.crate_reveal,
            &heroes,
        )?;
        let arena = load_arena(src.arena).map_err(|err| format!("arena_01.toml: {err}"))?;
        require_arena_colors(&palette, &arena).map_err(|err| format!("arena_01.toml: {err}"))?;
        validate_stage_offset(&palette, &crate_reveal.layout)?;
        Ok(Self {
            fingerprint,
            waves: parse("waves.toml", src.waves)?,
            difficulty,
            controller,
            upgrades: parse::<UpgradeFile>("run_upgrades.toml", src.run_upgrades)?.upgrade,
            endless: parse("endless.toml", src.endless)?,
            currency: parse("currency.toml", src.currency)?,
            banners: parse("banners.toml", src.banners)?,
            duplicate: load_duplicate(src.duplicate_ranks)?,
            bosses,
            weapons,
            hero,
            roster,
            heroes,
            warnings,
            arena,
            presentation,
            combat: parse("combat.toml", src.combat)?,
            palette,
            music: {
                let loaded = crate::music::load_music(src.music).map_err(|err| format!("music.toml: {err}"))?;
                MusicFile {
                    director: loaded.director,
                    track: loaded.track,
                    transitions: loaded.transitions,
                    rotation: loaded.rotation,
                }
            },
            audio,
            bugs,
            crate_reveal,
            coop,
            cosmetics: load_cosmetics(src.cosmetics).map_err(|err| format!("cosmetics.toml: {err}"))?,
            ui,
            heroes_visual,
            models,
            arena_dressing,
            cast,
        })
    }

    /// Playtest selection. `unlock_all` does not grant ownership, currency, or crates.
    pub fn hero_selectable(&self, id: &str) -> bool {
        let Some(card) = self.heroes.iter().find(|hero| hero.id == id) else {
            return false;
        };
        self.ui.settings.unlock_all || (card.owned && card.id == self.hero.id)
    }

    /// Ability row for `hero_id`. Cooldown and infinite magazine come from that hero's card.
    pub fn ability_of(&self, hero_id: &str) -> Option<PlayedAbility> {
        self.heroes.iter().find(|hero| hero.id == hero_id).map(|card| PlayedAbility {
            cooldown: card.ability_cooldown,
            infinite_magazine: card.infinite_magazine,
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

    pub fn view_eye(&self, crouching: bool) -> f32 {
        if crouching {
            self.controller.camera.crouch_eye
        } else {
            self.controller.camera.eye_height
        }
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

    /// File for a reveal cue. Stings live in the sting table; tease and fakeout live in sfx.
    pub fn cue_path(&self, id: &str) -> Option<&str> {
        if let Some(sting) = self.audio.sting.iter().find(|row| row.id == id) {
            return Some(sting.path.as_str());
        }
        self.sfx(id).map(|row| row.path.as_str())
    }

    /// Playback gain for a mix row. `None` means the id is not in the mix.
    /// A sting row with no `volume_db` never reaches playback: content load rejects it.
    pub fn cue_volume(&self, id: &str) -> Option<f32> {
        if let Some(sting) = self.audio.sting.iter().find(|row| row.id == id) {
            return Some(sting.volume);
        }
        self.sfx(id).map(|row| row.volume)
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

/// Every tease, fakeout, and rarity sting must name a mix row that has `volume_db`
/// and a file under `assets/`. Playback does not skip a missing cue.
fn validate_reveal_cues(reveal: &CrateReveal, audio: &AudioTable) -> Result<(), String> {
    let mut ids = Vec::new();
    for name in ["common", "rare", "epic", "legendary"] {
        let tier = reveal.tier(name).ok_or_else(|| format!("crate_reveal.toml: missing tier {name}"))?;
        ids.push(cue_stem(&tier.tease));
        ids.push(cue_stem(&tier.sting));
    }
    let fakeout = cue_stem(&reveal.fakeout_sfx);
    if !fakeout.is_empty() {
        ids.push(fakeout);
    }
    let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let check_files = assets.is_dir();
    for id in ids {
        let path = reveal_cue_file(audio, &id)?;
        if check_files && !assets.join(path).is_file() {
            return Err(format!("reveal cue {id} file {path} is missing"));
        }
    }
    Ok(())
}

fn reveal_cue_file<'a>(audio: &'a AudioTable, id: &str) -> Result<&'a str, String> {
    if let Some(sting) = audio.sting.iter().find(|row| row.id == id) {
        return Ok(sting.path.as_str());
    }
    if let Some(sfx) = audio.sfx.iter().find(|row| row.id == id) {
        return Ok(sfx.path.as_str());
    }
    Err(format!("reveal cue {id} does not resolve to a mix row with volume_db"))
}

fn validate_act_now_cues(audio: &AudioTable) -> Result<(), String> {
    let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let check_files = assets.is_dir();
    for id in ["revive_start", "low_hp_warn", "ammo_low_warn", "ability_ready"] {
        let row = audio
            .sfx
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| format!("audio_mix.toml is missing {id}"))?;
        if check_files && !assets.join(&row.path).is_file() {
            return Err(format!("{id} file {} is missing", row.path));
        }
    }
    Ok(())
}

fn validate_host_away_cues(audio: &AudioTable, coop: &CoopRules) -> Result<(), String> {
    let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let check_files = assets.is_dir();
    for id in ["host_away", "host_back"] {
        let row = audio
            .sfx
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| format!("audio_mix.toml is missing {id}"))?;
        if check_files && !assets.join(&row.path).is_file() {
            return Err(format!("{id} file {} is missing", row.path));
        }
    }
    let back = audio.sfx.iter().find(|row| row.id == "host_back").expect("host_back row");
    let Some(sync_at) = back.sync_at else {
        return Err("audio_mix.toml host_back is missing sync_at".into());
    };
    let countdown = coop.host_away.resume_countdown_seconds;
    if (sync_at - countdown).abs() > crate::sim::SIM_DT {
        return Err(format!(
            "audio_mix.toml host_back sync_at {sync_at} must match resume_countdown_seconds {countdown} within one sim frame"
        ));
    }
    Ok(())
}

fn validate_away_hud(away: &AwayHud) -> Result<(), String> {
    if !(away.panel_pad >= 0.0) {
        return Err("presentation.toml hud.away panel_pad must be non-negative".into());
    }
    if !(away.count_outline_px > 0.0) {
        return Err("presentation.toml hud.away count_outline_px must be positive".into());
    }
    if !(away.bar_track > 0.0 && away.bar_track <= 1.0) {
        return Err("presentation.toml hud.away bar_track must be in (0, 1]".into());
    }
    if !(away.dim_floor > 0.0 && away.dim_floor <= 1.0) {
        return Err("presentation.toml hud.away dim_floor must be in (0, 1]".into());
    }
    if !(away.away_desaturation >= 0.0 && away.away_desaturation <= 1.0) {
        return Err("presentation.toml hud.away away_desaturation must be in [0, 1]".into());
    }
    if !(away.away_dim >= 0.0 && away.away_dim <= 1.0) {
        return Err("presentation.toml hud.away away_dim must be in [0, 1]".into());
    }
    if !(away.music_duck_db <= 0.0 && away.music_duck_db >= -60.0) {
        return Err("presentation.toml hud.away music_duck_db must be in [-60, 0]".into());
    }
    if !(away.solo_pause_duck_db <= 0.0 && away.solo_pause_duck_db >= -60.0) {
        return Err("presentation.toml hud.away solo_pause_duck_db must be in [-60, 0]".into());
    }
    if !(away.coop_pause_duck_db <= 0.0 && away.coop_pause_duck_db >= -60.0) {
        return Err("presentation.toml hud.away coop_pause_duck_db must be in [-60, 0]".into());
    }
    if !(away.count_shadow_px > 0.0) {
        return Err("presentation.toml hud.away count_shadow_px must be positive".into());
    }
    if !(away.count_shadow_soft_px >= 0.0) {
        return Err("presentation.toml hud.away count_shadow_soft_px must be non-negative".into());
    }
    if !(away.count_shadow_alpha > 0.0 && away.count_shadow_alpha <= 1.0) {
        return Err("presentation.toml hud.away count_shadow_alpha must be in (0, 1]".into());
    }
    if !(away.music_duck_lowpass_hz.is_finite() && away.music_duck_lowpass_hz > 0.0) {
        return Err("presentation.toml hud.away music_duck_lowpass_hz must be a positive frequency".into());
    }
    if away.count_shadow_samples.is_empty() {
        return Err("presentation.toml hud.away count_shadow_samples is empty".into());
    }
    for (i, sample) in away.count_shadow_samples.iter().enumerate() {
        if !(sample.ox.is_finite() && sample.oy.is_finite()) {
            return Err(format!("presentation.toml hud.away count_shadow_samples[{i}] offset is invalid"));
        }
        if !(sample.weight > 0.0 && sample.weight <= 1.0) {
            return Err(format!(
                "presentation.toml hud.away count_shadow_samples[{i}].weight must be in (0, 1]"
            ));
        }
    }
    Ok(())
}

fn cue_stem(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.trim_end_matches(".ogg").to_string()
}

fn validate_reveal_layout(reveal: &CrateReveal) -> Result<(), String> {
    let hint = &reveal.hint;
    if !(hint.text_px > 0.0) {
        return Err("crate_reveal.toml hint.text_px must be positive".into());
    }
    if !(hint.opacity > 0.0 && hint.opacity <= 1.0) {
        return Err("crate_reveal.toml hint.opacity must be in (0, 1]".into());
    }
    if !(hint.margin_x >= 0.0) || !(hint.margin_y >= 0.0) {
        return Err("crate_reveal.toml hint margins must be non-negative".into());
    }
    let card = &reveal.card;
    if !(card.title_pad >= 0.0) {
        return Err("crate_reveal.toml card.title_pad must be non-negative".into());
    }
    validate_reveal_panel(&reveal.layout)
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
    pub ledge_catch: LedgeCatch,
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
pub struct LedgeCatch {
    /// `|y - lip|` within this catches a jetpack ledge.
    pub snap: f32,
    /// `vel_y` at or below this can still catch while dropping onto the lip.
    pub fall_speed: f32,
    /// Meters above the lip included in that falling catch.
    pub above: f32,
    /// Meters below the lip included in that falling catch.
    pub below: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CameraCfg {
    pub fov_default: f32,
    pub fov_sprint_add: f32,
    pub view_bob_amplitude: f32,
    pub landing_dip: f32,
    pub eye_height: f32,
    pub crouch_eye: f32,
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

#[derive(Clone, Debug)]
pub struct Cosmetics {
    pub dupes: String,
    pub shard_shop: bool,
    pub shard_cost: ShardCost,
}

#[derive(Clone, Debug)]
pub struct ShardCost {
    pub common: u32,
    pub rare: u32,
    pub epic: u32,
    pub legendary: u32,
}

impl Cosmetics {
    pub fn shards_for(&self, rarity: &str) -> Result<u32, String> {
        match rarity {
            "common" => Ok(self.shard_cost.common),
            "rare" => Ok(self.shard_cost.rare),
            "epic" => Ok(self.shard_cost.epic),
            "legendary" => Ok(self.shard_cost.legendary),
            other => Err(format!("unknown rarity \"{other}\"")),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct DuplicateRanks {
    pub power_per_rank: f32,
    /// Highest duplicate rank. Must equal the number of `[[rank]]` rows.
    pub max_rank: u32,
    pub rank: Vec<RankRow>,
}

fn load_duplicate(text: &str) -> Result<DuplicateRanks, String> {
    let table: DuplicateRanks = parse("duplicate_ranks.toml", text)?;
    let rows = table.rank.len() as u32;
    if table.max_rank == 0 {
        return Err("duplicate_ranks.toml max_rank is missing".into());
    }
    if table.max_rank != rows {
        return Err(format!(
            "duplicate_ranks.toml max_rank {} must equal the number of [[rank]] rows ({rows})",
            table.max_rank
        ));
    }
    Ok(table)
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
    if !(brood.hatch_fan_step > 0.0) {
        return Err("brood_mother hatch_fan_step must be > 0".into());
    }
    if !(brood.hatch_telegraph > 0.0) {
        return Err("brood_mother hatch_telegraph must be > 0".into());
    }
    if !(brood.spawn_radius > 0.0) {
        return Err("brood_mother spawn_radius must be > 0".into());
    }
    if !(hive.hatch_fan_step > 0.0) {
        return Err("hive_tyrant hatch_fan_step must be > 0".into());
    }
    if !(hive.spawn_radius > 0.0) {
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
    if !(phases[0].tail_weakpoint_mult > 0.0) {
        return Err("hive_tyrant.phase1.tail_weakpoint_mult must be > 0".into());
    }
    for (i, row) in phases.iter().enumerate() {
        if !(row.core_damage_mult > 0.0) {
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
    /// HUD cooldown bar never divides by less than this.
    pub cooldown_floor: f32,
    pub overdrive_duration: f32,
    pub overdrive_fire_rate: f32,
    pub infinite_magazine: bool,
    pub kill_cd_refund: f32,
    /// Camera-local viewmodel position. +X is screen right, -Y is screen down.
    pub viewmodel_offset: [f32; 3],
    pub viewmodel_fov: f32,
    /// Seconds the muzzle glow stays up. The client counts elapsed time.
    pub muzzle_flash_secs: f32,
    /// Outer radius of the muzzle glow in viewmodel space.
    pub muzzle_flash_radius: f32,
    /// Fraction of `muzzle_flash_radius` that stays at full brightness.
    pub muzzle_flash_core: f32,
    /// Exponent of the soft radial falloff outside the core.
    pub muzzle_flash_falloff: f32,
    /// Screen-pixel shift from the bore back onto the barrel tip.
    pub muzzle_flash_tip_px: f32,
    /// Halo color. The core blends this toward white.
    pub muzzle_flash_color: [f32; 3],
}

/// Lobby card. Combat still runs the starter hero. Locked rows stay on the select screen.
#[derive(Clone, Debug)]
pub struct HeroCard {
    pub id: String,
    pub name: String,
    pub role: String,
    pub ability: String,
    pub max_hp: f32,
    pub move_speed_mult: f32,
    /// Palette key. The viewmodel hex is resolved from this key at load.
    pub accent_key: String,
    pub owned: bool,
    /// This hero's ability cooldown, in seconds.
    pub ability_cooldown: f32,
    /// True while the ability is running means the magazine does not empty.
    pub infinite_magazine: bool,
    pub portrait: HeroPortrait,
}

/// Hero-card portrait. `mesh` must be one of the known portrait meshes.
#[derive(Clone, Debug)]
pub struct HeroPortrait {
    pub mesh: String,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub fit: f32,
}

/// Card meshes plus the first-person glove. Parsed with the rest of the content.
#[derive(Clone, Debug)]
pub struct HeroesVisual {
    pub meshes: Vec<HeroMesh>,
    pub glove: GloveSpec,
    pub models: Vec<CastModel>,
}

/// One Meshy cast. Paths, budgets, and palette keys live here. The mesh crate reads the file.
#[derive(Clone, Debug)]
pub struct CastModel {
    pub id: String,
    pub file: String,
    /// Cleaned mask PNG under assets/. Red is accent, green is glow.
    pub mask: String,
    pub triangles: u32,
    pub bytes: u32,
    pub source: String,
    pub license: String,
    pub task: String,
    pub base: String,
    pub accent: String,
    pub glow: String,
    pub rim: String,
    pub fit: String,
    pub size: f32,
    pub yaw_deg: f32,
    pub at: [f32; 3],
    pub grip: [f32; 3],
    pub view: [f32; 3],
}

/// Tint scalars and the side-sheet camera. Colors stay palette keys.
#[derive(Clone, Debug)]
pub struct CastLook {
    /// Mid-gray the body is lifted to, before the base palette key.
    pub base_light: f32,
    /// Meshy gray that `base_light` replaces. A texel of this value stays at `base_light`.
    pub base_gray: f32,
    pub rim_strength: f32,
    /// Weapon grade. Compared against the cleaned mask, which is 0 or 255.
    pub view_gun_mask_threshold: f32,
    /// Fresnel on an unmasked weapon texel. Separate from `rim_strength`.
    pub view_gun_rim_strength: f32,
    /// Palette key. Overdrive multiplies this color by `overdrive_emissive_gain`.
    pub overdrive_emissive_key: String,
    pub overdrive_emissive_gain: [f32; 3],
    /// Palette key for a live brood pod.
    pub pod_glow_key: String,
    /// Emissive multiplier is `pod_glow_base + glow * pod_glow_per_glow`.
    pub pod_glow_base: f32,
    pub pod_glow_per_glow: f32,
    /// Exponent on the glow's radial edge. Higher keeps a solid core.
    pub glow_falloff: f32,
    /// How much of the painted sac shows through at the glow's edge. The core stays solid.
    pub glow_show_through: f32,
    pub review_yaw_deg: f32,
    pub review_elev_deg: f32,
    pub review_distance: f32,
    pub review_fov_deg: f32,
}

/// One hero's ability, as the act-now cues read it. No hero or ability id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayedAbility {
    pub cooldown: f32,
    pub infinite_magazine: bool,
}

/// Local warning thresholds from `characters.toml` `[warnings]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Warnings {
    pub low_hp_fraction: f32,
    pub low_hp_rearm_fraction: f32,
    pub low_hp_repeat_s: f32,
    pub ammo_low_fraction: f32,
}

#[derive(Clone, Debug)]
pub struct HeroMesh {
    pub id: String,
    pub place: [f32; 3],
    pub parts: Vec<HeroPart>,
}

/// One piece of a lobby body. Unused fields stay zero for the other shapes.
#[derive(Clone, Debug)]
pub struct HeroPart {
    pub shape: String,
    pub size: [f32; 3],
    pub radii: [f32; 3],
    pub chamfer: f32,
    pub slices: u32,
    pub stacks: u32,
    pub length: f32,
    pub r0: f32,
    pub r1: f32,
    pub sides: u32,
    pub width: f32,
    pub height: f32,
    pub thick: f32,
    pub bend_deg: f32,
    pub segments: u32,
    pub at: [f32; 3],
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub color: String,
    pub glow: bool,
}

/// Offsets are added to the viewmodel anchor. An X component is a side magnitude.
#[derive(Clone, Debug)]
pub struct GloveSpec {
    pub sleeve_at: [f32; 3],
    pub sleeve_radii: [f32; 3],
    pub sleeve_slices: u32,
    pub sleeve_stacks: u32,
    pub plate_at: [f32; 3],
    pub plate_length: f32,
    pub plate_width: f32,
    pub plate_arch: f32,
    pub plate_nu: u32,
    pub plate_nv: u32,
    pub hand_at: [f32; 3],
    pub hand_radii: [f32; 3],
    pub hand_slices: u32,
    pub hand_stacks: u32,
    pub finger_count: u32,
    pub finger_spread: f32,
    pub finger_length: f32,
    pub finger_r0: f32,
    pub finger_r1: f32,
    pub finger_sides: u32,
    pub finger_at: [f32; 3],
    pub finger_pitch: f32,
    pub finger_yaw: f32,
    pub thumb_length: f32,
    pub thumb_r0: f32,
    pub thumb_r1: f32,
    pub thumb_sides: u32,
    pub thumb_at: [f32; 3],
    pub thumb_pitch: f32,
    pub thumb_yaw: f32,
}

/// Visual deck, curb, and the review cameras. Not a collider.
#[derive(Clone, Debug)]
pub struct ArenaDressing {
    pub deck_sides: u32,
    pub rail_posts: u32,
    pub rail_thickness: f32,
    pub rail_height: f32,
    pub ramp_gap_dot: f32,
    pub review_stand_gap: f32,
    pub review_distance: f32,
    pub review_look_height: f32,
    pub review_ground_y: f32,
    pub rail_still_angle_deg: f32,
    pub rail_still_back: f32,
    pub rail_still_up: f32,
    pub rail_still_side: f32,
    pub rail_still_fov: f32,
    pub rail_still_inset: f32,
}

fn load_characters(
    text: &str,
    meshes: &[String],
) -> Result<(Hero, Vec<WeaponDef>, Vec<String>, Vec<HeroCard>, Warnings), String> {
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
    let owned_rows = unlock
        .get("owned")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "characters.toml [unlock] is missing owned".to_string())?;
    let mut owned = Vec::new();
    for row in owned_rows {
        let id = row
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| "characters.toml [unlock] owned entry is empty".to_string())?;
        if owned.iter().any(|have| have == id) {
            return Err(format!("characters.toml [unlock] duplicate owned id {id}"));
        }
        owned.push(id.to_string());
    }
    if !owned.iter().any(|id| id == starter) {
        return Err(format!("characters.toml starter \"{starter}\" is not in owned"));
    }
    let weapons = table
        .get("weapons")
        .and_then(|v| v.as_table())
        .ok_or_else(|| "characters.toml is missing [weapons]".to_string())?;
    let mut roster = Vec::new();
    let mut cards = Vec::new();
    for hero in heroes {
        let row = hero.as_table().ok_or_else(|| "characters.toml hero row is not a table".to_string())?;
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| "characters.toml [[hero]] is missing id".to_string())?;
        if roster.iter().any(|have| have == id) {
            return Err(format!("characters.toml duplicate hero id {id}"));
        }
        roster.push(id.to_string());
        let ability = row
            .get("ability")
            .and_then(|v| v.as_table())
            .ok_or_else(|| format!("characters.toml {id} is missing ability"))?;
        let ability_cooldown = req_f32(ability, "cooldown", &format!("{id} ability"))?;
        if !(ability_cooldown > 0.0) {
            return Err(format!("characters.toml {id} ability cooldown must be > 0"));
        }
        let infinite_magazine = match ability.get("infinite_magazine") {
            None => return Err(format!("characters.toml {id} ability is missing infinite_magazine")),
            Some(toml::Value::Boolean(value)) => *value,
            Some(_) => return Err(format!("characters.toml {id} ability infinite_magazine must be a bool")),
        };
        let portrait = load_portrait(row, id, meshes)?;
        let card = HeroCard {
            id: id.to_string(),
            name: req_str(row, "name", id)?.to_string(),
            role: req_str(row, "role", id)?.to_string(),
            ability: req_str(ability, "desc", &format!("{id} ability"))?.to_string(),
            max_hp: req_f32(row, "max_hp", id)?,
            move_speed_mult: req_f32(row, "move_speed_mult", id)?,
            accent_key: req_str(row, "accent_key", id)?.to_string(),
            owned: owned.iter().any(|have| have == id),
            ability_cooldown,
            infinite_magazine,
            portrait,
        };
        if !(card.max_hp > 0.0) {
            return Err(format!("characters.toml {id} max_hp must be > 0"));
        }
        if !(card.move_speed_mult > 0.0) {
            return Err(format!("characters.toml {id} move_speed_mult must be > 0"));
        }
        if card.ability.is_empty() || card.role.is_empty() || card.name.is_empty() {
            return Err(format!("characters.toml {id} name, role, and ability desc must be set"));
        }
        let fantasy = req_str(row, "fantasy", id)?;
        let passive = req_str(row, "passive", id)?;
        require_font(&card.name, &format!("characters.toml {id} name"))?;
        require_font(&card.role, &format!("characters.toml {id} role"))?;
        require_font(&card.ability, &format!("characters.toml {id} ability"))?;
        require_font(fantasy, &format!("characters.toml {id} fantasy"))?;
        require_font(passive, &format!("characters.toml {id} passive"))?;
        cards.push(card);
    }
    for id in &owned {
        if !roster.iter().any(|have| have == id) {
            return Err(format!("characters.toml owned \"{id}\" is not a [[hero]]"));
        }
    }
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
        accent: String::new(),
        max_hp: req_f32(row, "max_hp", starter)?,
        move_speed_mult: req_f32(row, "move_speed_mult", starter)?,
        ability_id: ability_id.to_string(),
        ability_cooldown: cards
            .iter()
            .find(|card| card.id == starter)
            .map(|card| card.ability_cooldown)
            .expect("starter card"),
        cooldown_floor: req_f32(ability, "cooldown_floor", "overdrive")?,
        overdrive_duration: req_f32(ability, "duration", "overdrive")?,
        overdrive_fire_rate: req_f32(ability, "fire_rate", "overdrive")?,
        infinite_magazine: cards
            .iter()
            .find(|card| card.id == starter)
            .map(|card| card.infinite_magazine)
            .expect("starter card"),
        kill_cd_refund: req_f32(ability, "kill_cd_refund", "overdrive")?,
        viewmodel_offset: req_vec3(row, "viewmodel_offset", starter)?,
        viewmodel_fov: req_f32(row, "viewmodel_fov", starter)?,
        muzzle_flash_secs: req_f32(row, "muzzle_flash_secs", starter)?,
        muzzle_flash_radius: req_f32(row, "muzzle_flash_radius", starter)?,
        muzzle_flash_core: req_f32(row, "muzzle_flash_core", starter)?,
        muzzle_flash_falloff: req_f32(row, "muzzle_flash_falloff", starter)?,
        muzzle_flash_tip_px: req_f32(row, "muzzle_flash_tip_px", starter)?,
        muzzle_flash_color: req_vec3(row, "muzzle_flash_color", starter)?,
    };
    if !(hero.ability_cooldown > 0.0) || !(hero.overdrive_duration > 0.0) {
        return Err("overdrive cooldown and duration must be > 0".into());
    }
    if !(hero.cooldown_floor > 0.0) {
        return Err("characters.toml overdrive cooldown_floor must be > 0".into());
    }
    if !(hero.max_hp > 0.0) {
        return Err(format!("{starter} max_hp must be > 0"));
    }
    if !(hero.move_speed_mult > 0.0) {
        return Err(format!("{starter} move_speed_mult must be > 0"));
    }
    if !(hero.muzzle_flash_secs > 0.0) {
        return Err(format!("characters.toml {starter} muzzle_flash_secs must be > 0"));
    }
    if !(hero.muzzle_flash_radius > 0.0) {
        return Err(format!("characters.toml {starter} muzzle_flash_radius must be > 0"));
    }
    if !(hero.muzzle_flash_core > 0.0 && hero.muzzle_flash_core <= 1.0) {
        return Err(format!("characters.toml {starter} muzzle_flash_core must be in (0, 1]"));
    }
    if !(hero.muzzle_flash_falloff > 0.0) {
        return Err(format!("characters.toml {starter} muzzle_flash_falloff must be > 0"));
    }
    if !(hero.muzzle_flash_tip_px > 0.0) {
        return Err(format!("characters.toml {starter} muzzle_flash_tip_px must be > 0"));
    }
    for channel in hero.muzzle_flash_color {
        if !(channel >= 0.0 && channel <= 1.0) {
            return Err(format!("characters.toml {starter} muzzle_flash_color channels must be in [0, 1]"));
        }
    }
    if hero.viewmodel_fov <= 1.0 || hero.viewmodel_fov >= 170.0 {
        return Err(format!(
            "characters.toml {starter} viewmodel_fov must be between 1 and 170"
        ));
    }
    let warnings = load_warnings(table)?;
    Ok((hero, vec![primary, sidearm], roster, cards, warnings))
}

fn load_warnings(root: &toml::map::Map<String, toml::Value>) -> Result<Warnings, String> {
    let table = root
        .get("warnings")
        .and_then(|value| value.as_table())
        .ok_or_else(|| "characters.toml is missing [warnings]".to_string())?;
    expect_keys(
        table,
        &[
            "low_hp_fraction",
            "low_hp_rearm_fraction",
            "low_hp_repeat_s",
            "ammo_low_fraction",
        ],
        "characters.toml warnings",
    )?;
    let low_hp_fraction = unit_fraction(req_f32(table, "low_hp_fraction", "warnings")?, "characters.toml warnings low_hp_fraction")?;
    let low_hp_rearm_fraction = unit_fraction(
        req_f32(table, "low_hp_rearm_fraction", "warnings")?,
        "characters.toml warnings low_hp_rearm_fraction",
    )?;
    if !(low_hp_rearm_fraction > low_hp_fraction) {
        return Err("characters.toml warnings low_hp_rearm_fraction must be above low_hp_fraction".into());
    }
    let low_hp_repeat_s = req_f32(table, "low_hp_repeat_s", "warnings")?;
    if !(low_hp_repeat_s.is_finite() && low_hp_repeat_s > 0.0) {
        return Err("characters.toml warnings low_hp_repeat_s must be > 0".into());
    }
    let ammo_low_fraction = unit_fraction(req_f32(table, "ammo_low_fraction", "warnings")?, "characters.toml warnings ammo_low_fraction")?;
    Ok(Warnings {
        low_hp_fraction,
        low_hp_rearm_fraction,
        low_hp_repeat_s,
        ammo_low_fraction,
    })
}

fn unit_fraction(value: f32, what: &str) -> Result<f32, String> {
    if value.is_finite() && value > 0.0 && value < 1.0 {
        Ok(value)
    } else {
        Err(format!("{what} must be in (0, 1)"))
    }
}

fn load_portrait(
    row: &toml::map::Map<String, toml::Value>,
    id: &str,
    meshes: &[String],
) -> Result<HeroPortrait, String> {
    let table = row
        .get("portrait")
        .and_then(|value| value.as_table())
        .ok_or_else(|| format!("characters.toml {id} is missing portrait"))?;
    expect_keys(
        table,
        &["mesh", "yaw_deg", "pitch_deg", "fit"],
        &format!("characters.toml {id} portrait"),
    )?;
    let mesh = req_str(table, "mesh", &format!("{id} portrait"))?.to_string();
    if mesh.is_empty() || !meshes.iter().any(|have| have == &mesh) {
        return Err(format!("characters.toml {id} portrait mesh \"{mesh}\" is unknown"));
    }
    let yaw_deg = req_f32(table, "yaw_deg", &format!("{id} portrait"))?;
    let pitch_deg = req_f32(table, "pitch_deg", &format!("{id} portrait"))?;
    let fit = req_f32(table, "fit", &format!("{id} portrait"))?;
    if !yaw_deg.is_finite() {
        return Err(format!("characters.toml {id} portrait yaw_deg must be finite"));
    }
    // Pitch past a quarter turn flips the portrait. That is the out-of-range case.
    if !pitch_deg.is_finite() || pitch_deg.abs() > 90.0 {
        return Err(format!("characters.toml {id} portrait pitch_deg must be in [-90, 90]"));
    }
    if !(fit > 0.0 && fit <= 1.0) {
        return Err(format!("characters.toml {id} portrait fit must be in (0, 1]"));
    }
    Ok(HeroPortrait { mesh, yaw_deg, pitch_deg, fit })
}

fn load_heroes_visual(text: &str, palette: &Palette) -> Result<HeroesVisual, String> {
    let root: toml::Value = toml::from_str(text).map_err(|err| format!("heroes_visual.toml: {err}"))?;
    let table = root.as_table().ok_or_else(|| "heroes_visual.toml is not a table".to_string())?;
    expect_keys(table, &["mesh", "glove", "model"], "heroes_visual.toml")?;
    let rows = table
        .get("mesh")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "heroes_visual.toml is missing [[mesh]]".to_string())?;
    if rows.is_empty() {
        return Err("heroes_visual.toml has no [[mesh]]".into());
    }
    let mut meshes: Vec<HeroMesh> = Vec::new();
    for row in rows {
        let map = row.as_table().ok_or_else(|| "heroes_visual.toml mesh is not a table".to_string())?;
        expect_keys(map, &["id", "place", "parts"], "heroes_visual.toml mesh")?;
        let id = vis_str(map, "id", "heroes_visual.toml mesh")?.to_string();
        if id.is_empty() || meshes.iter().any(|mesh| mesh.id == id) {
            return Err(format!("heroes_visual.toml duplicate or empty mesh id \"{id}\""));
        }
        let place = vis_vec3(map, "place", &format!("heroes_visual.toml {id}"))?;
        let parts_v = map
            .get("parts")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("heroes_visual.toml {id} is missing parts"))?;
        if parts_v.is_empty() {
            return Err(format!("heroes_visual.toml {id} has no parts"));
        }
        let mut parts = Vec::new();
        for part in parts_v {
            parts.push(load_hero_part(part, &id, palette)?);
        }
        meshes.push(HeroMesh { id, place, parts });
    }
    let glove_row = table
        .get("glove")
        .and_then(|v| v.as_table())
        .ok_or_else(|| "heroes_visual.toml is missing [glove]".to_string())?;
    let models = load_model_rows(
        table.get("model").and_then(|v| v.as_array()),
        palette,
        "heroes_visual.toml",
    )?;
    Ok(HeroesVisual {
        meshes,
        glove: load_glove(glove_row)?,
        models,
    })
}

fn load_model_table(text: &str, palette: &Palette, what: &str) -> Result<Vec<CastModel>, String> {
    let root: toml::Value = toml::from_str(text).map_err(|err| format!("{what}: {err}"))?;
    let table = root.as_table().ok_or_else(|| format!("{what} is not a table"))?;
    expect_keys(table, &["model"], what)?;
    load_model_rows(table.get("model").and_then(|v| v.as_array()), palette, what)
}

fn load_model_rows(
    rows: Option<&Vec<toml::Value>>,
    palette: &Palette,
    what: &str,
) -> Result<Vec<CastModel>, String> {
    let rows = rows.ok_or_else(|| format!("{what} is missing [[model]]"))?;
    if rows.is_empty() {
        return Err(format!("{what} has no [[model]]"));
    }
    let mut models: Vec<CastModel> = Vec::new();
    for row in rows {
        let map = row.as_table().ok_or_else(|| format!("{what} model is not a table"))?;
        expect_keys(
            map,
            &[
                "id", "file", "mask", "triangles", "bytes", "source", "license", "task", "base", "accent", "glow", "rim", "fit",
                "size", "yaw_deg", "at", "grip", "view",
            ],
            &format!("{what} model"),
        )?;
        let id = vis_str(map, "id", &format!("{what} model"))?.to_string();
        if id.is_empty() || models.iter().any(|model| model.id == id) {
            return Err(format!("{what} duplicate or empty model id \"{id}\""));
        }
        let file = vis_str(map, "file", &format!("{what} {id}"))?.to_string();
        let expected = format!("models/{id}.glb");
        if file != expected {
            return Err(format!("{what} {id} file \"{file}\" is unknown"));
        }
        let mask = vis_str(map, "mask", &format!("{what} {id}"))?.to_string();
        let expected_mask = format!("masks/{id}_mask.png");
        if mask != expected_mask {
            return Err(format!("{what} {id} mask \"{mask}\" is unknown"));
        }
        let fit = vis_str(map, "fit", &format!("{what} {id}"))?.to_string();
        if !matches!(fit.as_str(), "hero" | "hitbox" | "weapon" | "shield") {
            return Err(format!("{what} {id} fit \"{fit}\" is unknown"));
        }
        let size = vis_f32(map, "size", &format!("{what} {id}"))?;
        if fit != "hitbox" && !(size > 0.0) {
            return Err(format!("{what} {id} size must be > 0"));
        }
        let labeled = id.clone();
        let color = |key: &str| -> Result<String, String> {
            let name = vis_str(map, key, &format!("{what} {labeled}"))?.to_string();
            if name.is_empty() {
                if key == "rim" {
                    return Ok(name);
                }
                return Err(format!("{what} {labeled} {key} is empty"));
            }
            let resolved = palette.hex(&name).unwrap_or("");
            if resolved.trim().trim_start_matches('#').len() < 6 {
                return Err(format!("{what} {labeled} {key} \"{name}\" does not resolve"));
            }
            Ok(name)
        };
        let source = vis_str(map, "source", &format!("{what} {id}"))?.to_string();
        let license = vis_str(map, "license", &format!("{what} {id}"))?.to_string();
        let task = vis_str(map, "task", &format!("{what} {id}"))?.to_string();
        if source.is_empty() || license.is_empty() {
            return Err(format!("{what} {id} source and license are required"));
        }
        if task == "not-in-handoff" {
            return Err(format!(
                "{what} {id} has no task id; leave task empty instead of a placeholder"
            ));
        }
        let yaw_deg = vis_f32(map, "yaw_deg", &format!("{what} {id}"))?;
        if !yaw_deg.is_finite() {
            return Err(format!("{what} {id} yaw_deg must be finite"));
        }
        models.push(CastModel {
            id,
            file,
            mask,
            triangles: vis_u32(map, "triangles", &format!("{what} model"))?,
            bytes: vis_u32(map, "bytes", &format!("{what} model"))?,
            source,
            license,
            task,
            base: color("base")?,
            accent: color("accent")?,
            glow: color("glow")?,
            rim: color("rim")?,
            fit,
            size,
            yaw_deg,
            at: vis_vec3(map, "at", &format!("{what} model"))?,
            grip: vis_vec3(map, "grip", &format!("{what} model"))?,
            view: vis_vec3(map, "view", &format!("{what} model"))?,
        });
    }
    Ok(models)
}

fn require_cast_roster(models: &[CastModel], heroes: &[HeroCard], bugs: &crate::bugs::BugBook) -> Result<(), String> {
    let mut need = Vec::new();
    for hero in heroes {
        need.push(format!("{}_body", hero.id));
        need.push(format!("{}_gun", hero.id));
        if hero.id == "brakk" {
            need.push("brakk_shield".to_string());
        }
    }
    for (id, _) in bugs.rows() {
        need.push(id.to_string());
    }
    for id in &need {
        if !models.iter().any(|model| model.id == *id) {
            return Err(format!("cast model \"{id}\" is missing"));
        }
    }
    if models.len() != need.len() {
        let extra: Vec<&str> = models
            .iter()
            .filter(|model| !need.iter().any(|id| id == &model.id))
            .map(|model| model.id.as_str())
            .collect();
        return Err(format!(
            "cast roster has {} models, expected {} (extra {extra:?})",
            models.len(),
            need.len()
        ));
    }
    Ok(())
}

fn load_cast_look(text: &str, palette: &Palette) -> Result<CastLook, String> {
    let root: toml::Value = toml::from_str(text).map_err(|err| format!("cast.toml: {err}"))?;
    let table = root.as_table().ok_or_else(|| "cast.toml is not a table".to_string())?;
    expect_keys(
        table,
        &[
            "base_light",
            "base_gray",
            "rim_strength",
            "view_gun_mask_threshold",
            "view_gun_rim_strength",
            "overdrive_emissive_key",
            "overdrive_emissive_gain",
            "pod_glow_key",
            "pod_glow_base",
            "pod_glow_per_glow",
            "glow_falloff",
            "glow_show_through",
            "review_yaw_deg",
            "review_elev_deg",
            "review_distance",
            "review_fov_deg",
        ],
        "cast.toml",
    )?;
    let base_light = vis_f32(table, "base_light", "cast.toml")?;
    let base_gray = vis_f32(table, "base_gray", "cast.toml")?;
    let rim_strength = vis_f32(table, "rim_strength", "cast.toml")?;
    if !(base_light > 0.0 && base_light <= 1.0) {
        return Err("cast.toml base_light must be > 0 and <= 1".into());
    }
    if !(base_gray > 0.0 && base_gray <= 1.0) {
        return Err("cast.toml base_gray must be > 0 and <= 1".into());
    }
    if !(rim_strength >= 0.0 && rim_strength <= 1.0) {
        return Err("cast.toml rim_strength must be between 0 and 1".into());
    }
    let view_gun_mask_threshold = vis_f32(table, "view_gun_mask_threshold", "cast.toml")?;
    let view_gun_rim_strength = vis_f32(table, "view_gun_rim_strength", "cast.toml")?;
    if !(view_gun_mask_threshold > 0.0 && view_gun_mask_threshold < 1.0) {
        return Err("cast.toml view_gun_mask_threshold must be between 0 and 1".into());
    }
    if !(view_gun_rim_strength >= 0.0 && view_gun_rim_strength <= 1.0) {
        return Err("cast.toml view_gun_rim_strength must be between 0 and 1".into());
    }
    let overdrive_emissive_key = vis_str(table, "overdrive_emissive_key", "cast.toml")?.to_string();
    if palette.hex(&overdrive_emissive_key).is_none() {
        return Err(format!("cast.toml overdrive_emissive_key \"{overdrive_emissive_key}\" does not resolve"));
    }
    let gain_value = table
        .get("overdrive_emissive_gain")
        .ok_or_else(|| "cast.toml is missing overdrive_emissive_gain".to_string())?;
    let overdrive_emissive_gain = nums3(gain_value, "cast.toml overdrive_emissive_gain")?;
    if overdrive_emissive_gain.iter().any(|n| !n.is_finite() || *n < 0.0) {
        return Err("cast.toml overdrive_emissive_gain must be finite and >= 0".into());
    }
    let pod_glow_key = vis_str(table, "pod_glow_key", "cast.toml")?.to_string();
    if palette.hex(&pod_glow_key).is_none() {
        return Err(format!("cast.toml pod_glow_key \"{pod_glow_key}\" does not resolve"));
    }
    let pod_glow_base = vis_f32(table, "pod_glow_base", "cast.toml")?;
    let pod_glow_per_glow = vis_f32(table, "pod_glow_per_glow", "cast.toml")?;
    if !pod_glow_base.is_finite() || pod_glow_base < 0.0 || !pod_glow_per_glow.is_finite() || pod_glow_per_glow < 0.0 {
        return Err("cast.toml pod glow multipliers must be finite and >= 0".into());
    }
    let glow_falloff = vis_f32(table, "glow_falloff", "cast.toml")?;
    let glow_show_through = vis_f32(table, "glow_show_through", "cast.toml")?;
    if !(glow_falloff > 1.0 && glow_falloff <= 8.0) {
        return Err("cast.toml glow_falloff must be > 1 and <= 8".into());
    }
    if !(glow_show_through >= 0.0 && glow_show_through < 1.0) {
        return Err("cast.toml glow_show_through must be >= 0 and < 1".into());
    }
    let review_yaw_deg = vis_f32(table, "review_yaw_deg", "cast.toml")?;
    let review_elev_deg = vis_f32(table, "review_elev_deg", "cast.toml")?;
    let review_distance = vis_f32(table, "review_distance", "cast.toml")?;
    let review_fov_deg = vis_f32(table, "review_fov_deg", "cast.toml")?;
    if !(review_distance > 0.0) || !(review_fov_deg > 0.0 && review_fov_deg < 180.0) {
        return Err("cast.toml review camera is out of range".into());
    }
    Ok(CastLook {
        base_light,
        base_gray,
        rim_strength,
        view_gun_mask_threshold,
        view_gun_rim_strength,
        overdrive_emissive_key,
        overdrive_emissive_gain,
        pod_glow_key,
        pod_glow_base,
        pod_glow_per_glow,
        glow_falloff,
        glow_show_through,
        review_yaw_deg,
        review_elev_deg,
        review_distance,
        review_fov_deg,
    })
}

fn load_hero_part(value: &toml::Value, id: &str, palette: &Palette) -> Result<HeroPart, String> {
    let spec = value.as_table().ok_or_else(|| format!("heroes_visual.toml {id} part is not a table"))?;
    let shape = vis_str(spec, "shape", &format!("heroes_visual.toml {id} part"))?.to_string();
    let keys: &[&str] = match shape.as_str() {
        "box" => &["shape", "size", "at", "yaw_deg", "pitch_deg", "color", "glow"],
        "gem" => &["shape", "radii", "slices", "stacks", "at", "yaw_deg", "pitch_deg", "color", "glow"],
        "limb" => &["shape", "length", "r0", "r1", "sides", "at", "yaw_deg", "pitch_deg", "color", "glow"],
        "bevel" => &["shape", "size", "chamfer", "at", "yaw_deg", "pitch_deg", "color", "glow"],
        "arc" => &[
            "shape", "width", "height", "thick", "bend_deg", "segments", "at", "yaw_deg", "pitch_deg", "color", "glow",
        ],
        other => return Err(format!("heroes_visual.toml {id} shape \"{other}\" is unknown")),
    };
    expect_keys(spec, keys, &format!("heroes_visual.toml {id} {shape}"))?;
    let what = format!("heroes_visual.toml {id} {shape}");
    let color = vis_str(spec, "color", &what)?.to_string();
    if color != "accent" {
        let resolved = palette.hex(&color).unwrap_or("");
        if resolved.trim().trim_start_matches('#').len() < 6 {
            return Err(format!("heroes_visual.toml {id} color \"{color}\" does not resolve"));
        }
    }
    let mut part = HeroPart {
        shape,
        size: [0.0; 3],
        radii: [0.0; 3],
        chamfer: 0.0,
        slices: 0,
        stacks: 0,
        length: 0.0,
        r0: 0.0,
        r1: 0.0,
        sides: 0,
        width: 0.0,
        height: 0.0,
        thick: 0.0,
        bend_deg: 0.0,
        segments: 0,
        at: vis_vec3(spec, "at", &what)?,
        yaw_deg: vis_f32(spec, "yaw_deg", &what)?,
        pitch_deg: vis_f32(spec, "pitch_deg", &what)?,
        color,
        glow: vis_bool(spec, "glow", &what)?,
    };
    match part.shape.as_str() {
        "box" => {
            part.size = vis_vec3(spec, "size", &what)?;
            if part.size.iter().any(|n| !(*n > 0.0)) {
                return Err(format!("heroes_visual.toml {id} box size must be > 0"));
            }
        }
        "bevel" => {
            part.size = vis_vec3(spec, "size", &what)?;
            part.chamfer = vis_f32(spec, "chamfer", &what)?;
            if part.size.iter().any(|n| !(*n > 0.0)) {
                return Err(format!("heroes_visual.toml {id} bevel size must be > 0"));
            }
            if !(part.chamfer > 0.0) || part.chamfer >= part.size[0] * 0.5 || part.chamfer >= part.size[2] * 0.5 {
                return Err(format!(
                    "heroes_visual.toml {id} bevel chamfer must be > 0 and less than half the X and Z size"
                ));
            }
        }
        "gem" => {
            part.radii = vis_vec3(spec, "radii", &what)?;
            part.slices = vis_u32(spec, "slices", &what)?;
            part.stacks = vis_u32(spec, "stacks", &what)?;
            if part.radii.iter().any(|n| !(*n > 0.0)) || part.slices < 3 || part.stacks < 2 {
                return Err(format!("heroes_visual.toml {id} gem radii, slices, and stacks are out of range"));
            }
        }
        "limb" => {
            part.length = vis_f32(spec, "length", &what)?;
            part.r0 = vis_f32(spec, "r0", &what)?;
            part.r1 = vis_f32(spec, "r1", &what)?;
            part.sides = vis_u32(spec, "sides", &what)?;
            if !(part.length > 0.0 && part.r0 > 0.0 && part.r1 > 0.0) || part.sides < 3 {
                return Err(format!("heroes_visual.toml {id} limb length, radii, and sides are out of range"));
            }
        }
        "arc" => {
            part.width = vis_f32(spec, "width", &what)?;
            part.height = vis_f32(spec, "height", &what)?;
            part.thick = vis_f32(spec, "thick", &what)?;
            part.bend_deg = vis_f32(spec, "bend_deg", &what)?;
            part.segments = vis_u32(spec, "segments", &what)?;
            if !(part.width > 0.0 && part.height > 0.0 && part.thick > 0.0) {
                return Err(format!("heroes_visual.toml {id} arc width, height, and thick must be > 0"));
            }
            if !(part.bend_deg > 0.0 && part.bend_deg < 180.0) || part.segments < 2 {
                return Err(format!(
                    "heroes_visual.toml {id} arc bend_deg must be in (0, 180) and segments >= 2"
                ));
            }
        }
        _ => unreachable!(),
    }
    Ok(part)
}

fn load_glove(table: &toml::map::Map<String, toml::Value>) -> Result<GloveSpec, String> {
    expect_keys(
        table,
        &[
            "sleeve_at",
            "sleeve_radii",
            "sleeve_slices",
            "sleeve_stacks",
            "plate_at",
            "plate_length",
            "plate_width",
            "plate_arch",
            "plate_nu",
            "plate_nv",
            "hand_at",
            "hand_radii",
            "hand_slices",
            "hand_stacks",
            "finger_count",
            "finger_spread",
            "finger_length",
            "finger_r0",
            "finger_r1",
            "finger_sides",
            "finger_at",
            "finger_pitch",
            "finger_yaw",
            "thumb_length",
            "thumb_r0",
            "thumb_r1",
            "thumb_sides",
            "thumb_at",
            "thumb_pitch",
            "thumb_yaw",
        ],
        "heroes_visual.toml glove",
    )?;
    let what = "heroes_visual.toml glove";
    let glove = GloveSpec {
        sleeve_at: vis_vec3(table, "sleeve_at", what)?,
        sleeve_radii: vis_vec3(table, "sleeve_radii", what)?,
        sleeve_slices: vis_u32(table, "sleeve_slices", what)?,
        sleeve_stacks: vis_u32(table, "sleeve_stacks", what)?,
        plate_at: vis_vec3(table, "plate_at", what)?,
        plate_length: vis_f32(table, "plate_length", what)?,
        plate_width: vis_f32(table, "plate_width", what)?,
        plate_arch: vis_f32(table, "plate_arch", what)?,
        plate_nu: vis_u32(table, "plate_nu", what)?,
        plate_nv: vis_u32(table, "plate_nv", what)?,
        hand_at: vis_vec3(table, "hand_at", what)?,
        hand_radii: vis_vec3(table, "hand_radii", what)?,
        hand_slices: vis_u32(table, "hand_slices", what)?,
        hand_stacks: vis_u32(table, "hand_stacks", what)?,
        finger_count: vis_u32(table, "finger_count", what)?,
        finger_spread: vis_f32(table, "finger_spread", what)?,
        finger_length: vis_f32(table, "finger_length", what)?,
        finger_r0: vis_f32(table, "finger_r0", what)?,
        finger_r1: vis_f32(table, "finger_r1", what)?,
        finger_sides: vis_u32(table, "finger_sides", what)?,
        finger_at: vis_vec3(table, "finger_at", what)?,
        finger_pitch: vis_f32(table, "finger_pitch", what)?,
        finger_yaw: vis_f32(table, "finger_yaw", what)?,
        thumb_length: vis_f32(table, "thumb_length", what)?,
        thumb_r0: vis_f32(table, "thumb_r0", what)?,
        thumb_r1: vis_f32(table, "thumb_r1", what)?,
        thumb_sides: vis_u32(table, "thumb_sides", what)?,
        thumb_at: vis_vec3(table, "thumb_at", what)?,
        thumb_pitch: vis_f32(table, "thumb_pitch", what)?,
        thumb_yaw: vis_f32(table, "thumb_yaw", what)?,
    };
    let positive = glove.sleeve_radii.iter().all(|n| *n > 0.0)
        && glove.hand_radii.iter().all(|n| *n > 0.0)
        && glove.plate_length > 0.0
        && glove.plate_width > 0.0
        && glove.plate_arch > 0.0
        && glove.finger_length > 0.0
        && glove.finger_r0 > 0.0
        && glove.finger_r1 > 0.0
        && glove.thumb_length > 0.0
        && glove.thumb_r0 > 0.0
        && glove.thumb_r1 > 0.0
        && glove.finger_spread > 0.0;
    if !positive
        || glove.sleeve_slices < 3
        || glove.sleeve_stacks < 2
        || glove.hand_slices < 3
        || glove.hand_stacks < 2
        || glove.plate_nu < 1
        || glove.plate_nv < 1
        || glove.finger_count < 1
        || glove.finger_sides < 3
        || glove.thumb_sides < 3
    {
        return Err("heroes_visual.toml glove sizes and tessellation are out of range".into());
    }
    Ok(glove)
}

fn load_arena_dressing(text: &str) -> Result<ArenaDressing, String> {
    let root: toml::Value = toml::from_str(text).map_err(|err| format!("arena_visual.toml: {err}"))?;
    let table = root.as_table().ok_or_else(|| "arena_visual.toml is not a table".to_string())?;
    expect_keys(
        table,
        &[
            "deck_sides",
            "rail_posts",
            "rail_thickness",
            "rail_height",
            "ramp_gap_dot",
            "review_stand_gap",
            "review_distance",
            "review_look_height",
            "review_ground_y",
            "rail_still_angle_deg",
            "rail_still_back",
            "rail_still_up",
            "rail_still_side",
            "rail_still_fov",
            "rail_still_inset",
        ],
        "arena_visual.toml",
    )?;
    let what = "arena_visual.toml";
    let dress = ArenaDressing {
        deck_sides: vis_u32(table, "deck_sides", what)?,
        rail_posts: vis_u32(table, "rail_posts", what)?,
        rail_thickness: vis_f32(table, "rail_thickness", what)?,
        rail_height: vis_f32(table, "rail_height", what)?,
        ramp_gap_dot: vis_f32(table, "ramp_gap_dot", what)?,
        review_stand_gap: vis_f32(table, "review_stand_gap", what)?,
        review_distance: vis_f32(table, "review_distance", what)?,
        review_look_height: vis_f32(table, "review_look_height", what)?,
        review_ground_y: vis_f32(table, "review_ground_y", what)?,
        rail_still_angle_deg: vis_f32(table, "rail_still_angle_deg", what)?,
        rail_still_back: vis_f32(table, "rail_still_back", what)?,
        rail_still_up: vis_f32(table, "rail_still_up", what)?,
        rail_still_side: vis_f32(table, "rail_still_side", what)?,
        rail_still_fov: vis_f32(table, "rail_still_fov", what)?,
        rail_still_inset: vis_f32(table, "rail_still_inset", what)?,
    };
    if dress.deck_sides < 8 || dress.rail_posts < 8 {
        return Err("arena_visual.toml deck_sides and rail_posts must be >= 8".into());
    }
    if !(dress.rail_thickness > 0.0 && dress.rail_thickness <= 1.0) {
        return Err("arena_visual.toml rail_thickness must be in (0, 1]".into());
    }
    // Knee or below. The 0.6 collision band stays in arena_01.toml.
    if !(dress.rail_height > 0.0 && dress.rail_height <= 0.5) {
        return Err("arena_visual.toml rail_height must be in (0, 0.5]".into());
    }
    if !(dress.ramp_gap_dot > 0.0 && dress.ramp_gap_dot < 1.0) {
        return Err("arena_visual.toml ramp_gap_dot must be in (0, 1)".into());
    }
    if !(dress.review_stand_gap > 0.0) {
        return Err("arena_visual.toml review_stand_gap must be > 0".into());
    }
    if !(dress.review_distance > 0.0) {
        return Err("arena_visual.toml review_distance must be > 0".into());
    }
    if !(dress.review_look_height > 0.0 && dress.review_look_height < 1.0) {
        return Err("arena_visual.toml review_look_height must be in (0, 1)".into());
    }
    if !(dress.review_ground_y.is_finite() && dress.review_ground_y.abs() < 1.0) {
        return Err("arena_visual.toml review_ground_y must be a small offset".into());
    }
    if !(dress.rail_still_fov > 1.0 && dress.rail_still_fov < 170.0) {
        return Err("arena_visual.toml rail_still_fov must be between 1 and 170".into());
    }
    if !(dress.rail_still_inset > 0.0) {
        return Err("arena_visual.toml rail_still_inset must be > 0".into());
    }
    Ok(dress)
}

fn vis_str<'a>(table: &'a toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<&'a str, String> {
    table.get(key).and_then(|v| v.as_str()).ok_or_else(|| format!("{what} is missing {key}"))
}

fn vis_f32(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<f32, String> {
    let value = table.get(key).ok_or_else(|| format!("{what} is missing {key}"))?;
    let n = if let Some(n) = value.as_float() {
        n as f32
    } else if let Some(n) = value.as_integer() {
        n as f32
    } else {
        return Err(format!("{what} {key} must be a number"));
    };
    if !n.is_finite() {
        return Err(format!("{what} {key} is not finite"));
    }
    Ok(n)
}

fn vis_u32(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<u32, String> {
    let n = table
        .get(key)
        .and_then(|v| v.as_integer())
        .ok_or_else(|| format!("{what} {key} must be an integer"))?;
    if n < 0 {
        return Err(format!("{what} {key} must be >= 0"));
    }
    Ok(n as u32)
}

fn vis_bool(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<bool, String> {
    table
        .get(key)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| format!("{what} {key} must be a bool"))
}

fn vis_vec3(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<[f32; 3], String> {
    let arr = table
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("{what} {key} must be 3 numbers"))?;
    if arr.len() != 3 {
        return Err(format!("{what} {key} must be 3 numbers"));
    }
    let mut out = [0.0; 3];
    for (i, slot) in arr.iter().enumerate() {
        if let Some(n) = slot.as_float() {
            out[i] = n as f32;
        } else if let Some(n) = slot.as_integer() {
            out[i] = n as f32;
        } else {
            return Err(format!("{what} {key} must be 3 numbers"));
        }
        if !out[i].is_finite() {
            return Err(format!("{what} {key} is not finite"));
        }
    }
    Ok(out)
}

fn load_cosmetics(text: &str) -> Result<Cosmetics, String> {
    let root: toml::Value = toml::from_str(text).map_err(|err| err.to_string())?;
    let table = root.as_table().ok_or_else(|| "cosmetics.toml is not a table".to_string())?;
    let dupes = table
        .get("dupes")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "cosmetics.toml is missing dupes".to_string())?;
    if dupes != "convert_to_cosmetic_shards" {
        return Err("cosmetics.toml dupes must be convert_to_cosmetic_shards".into());
    }
    let shard_shop = table
        .get("shard_shop")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| "cosmetics.toml shard_shop must be a bool".to_string())?;
    let cost = table
        .get("shard_cost")
        .and_then(|v| v.as_table())
        .ok_or_else(|| "cosmetics.toml is missing shard_cost".to_string())?;
    expect_keys(
        cost,
        &["common", "rare", "epic", "legendary"],
        "cosmetics.toml shard_cost",
    )?;
    let shard = |key: &str| -> Result<u32, String> {
        let n = cost
            .get(key)
            .and_then(|v| v.as_integer())
            .ok_or_else(|| format!("cosmetics.toml shard_cost is missing {key}"))?;
        if n < 0 {
            return Err(format!("cosmetics.toml shard_cost {key} must be >= 0"));
        }
        Ok(n as u32)
    };
    Ok(Cosmetics {
        dupes: dupes.to_string(),
        shard_shop,
        shard_cost: ShardCost {
            common: shard("common")?,
            rare: shard("rare")?,
            epic: shard("epic")?,
            legendary: shard("legendary")?,
        },
    })
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
    if !(rps > 0.0) {
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

#[derive(Clone, Debug)]
pub struct Arena {
    pub size: [f32; 2],
    pub shape: String,
    /// Square half-extent of `size`, used by the spatial grid.
    pub half_extent: f32,
    pub radius: f32,
    pub platform: Platform,
    pub boss_area: BossArea,
    pub vents: Vec<Vent>,
    pub ledges: Vec<Ledge>,
    pub pickups: Vec<ArenaPickup>,
    pub player_spawn: Vec3,
    pub perch_rules: PerchRules,
    pub paint: ArenaPaint,
    pub edge_wash: ArenaEdgeWash,
}

#[derive(Clone, Debug)]
pub struct Platform {
    pub center: Vec3,
    pub radius: f32,
    pub height: f32,
    pub railing_height: f32,
    pub ramps: Vec<Ramp>,
}

#[derive(Clone, Debug)]
pub struct Ramp {
    pub from: Vec3,
    pub to: Vec3,
    pub width: f32,
}

#[derive(Clone, Debug)]
pub struct BossArea {
    pub center: Vec3,
    pub radius: f32,
}

#[derive(Clone, Debug)]
pub struct Vent {
    pub id: String,
    pub pos: Vec3,
    pub bearing_deg: f32,
    pub telegraph: f32,
}

#[derive(Clone, Debug)]
pub struct Ledge {
    pub id: String,
    pub pos: Vec3,
    pub jetpack_only: bool,
}

#[derive(Clone, Debug)]
pub struct ArenaPickup {
    pub id: String,
    pub pos: Vec3,
    pub spawns_during: String,
}

#[derive(Clone, Debug)]
pub struct PerchRules {
    pub perch_radius: f32,
    pub perch_height_slack: f32,
    pub climb_trigger_distance: f32,
    pub climb_slide_rate: f32,
    pub flow_field_targets: String,
    pub all_perched_grace: f32,
    pub climb_speed_mult: f32,
    /// Meters kept inside the play edge while a swarmer climbs.
    pub climb_clearance: f32,
    /// Meters kept inside the play edge while a swarmer chases on the ground.
    pub ground_chase_pad: f32,
    pub budget_shift_to_spitters: f32,
    pub fuel_regen_on_perch: bool,
}

/// Floor pattern of the host-away still. Color names are palette keys.
#[derive(Clone, Debug)]
pub struct ArenaPaint {
    pub floor_light: String,
    pub floor_dark: String,
    pub band_x: u32,
    pub band_y: u32,
    pub shade: f32,
}

/// Sky wash above the arena horizon in the host-away still.
/// `sky` is a palette key. The floor stays put below `horizon`.
/// Above it, the wash blends `mix + (1 - t) * span` from the floor dark toward that sky.
#[derive(Clone, Debug)]
pub struct ArenaEdgeWash {
    /// Palette key for the sky above the horizon.
    pub sky: String,
    pub horizon: f32,
    pub mix: f32,
    pub span: f32,
}

impl Vent {
    /// Yaw that aims local -Z back toward the arena center. `bearing_deg` 0 is +Z.
    pub fn inward_yaw(&self) -> f32 {
        let rad = self.bearing_deg.to_radians();
        let inward_x = -rad.sin();
        let inward_z = -rad.cos();
        (-inward_x).atan2(-inward_z)
    }
}

impl Arena {
    pub fn boss_spawn(&self) -> Vec3 {
        Vec3::new(self.boss_area.center.x, 0.0, self.boss_area.center.z)
    }

    /// Horizontal disk shared by detection, landing, the fuel pause, and the climb trigger.
    pub fn in_perch_disk(&self, x: f32, z: f32, ledge: &Ledge) -> bool {
        let dx = x - ledge.pos.x;
        let dz = z - ledge.pos.z;
        let r = self.perch_rules.perch_radius;
        dx * dx + dz * dz <= r * r
    }

    /// Ledge whose disk and vertical slack contain `pos`. Detection uses this.
    pub fn perch_under(&self, pos: Vec3) -> Option<&Ledge> {
        self.ledges.iter().find(|ledge| {
            self.in_perch_disk(pos.x, pos.z, ledge)
                && (pos.y - ledge.pos.y).abs() <= self.perch_rules.perch_height_slack
        })
    }

    pub fn on_perch(&self, pos: Vec3) -> bool {
        self.perch_under(pos).is_some()
    }

    pub fn nearest_ledge(&self, from: Vec3) -> Option<&Ledge> {
        self.ledges.iter().min_by(|a, b| {
            let da = (a.pos.x - from.x).powi(2) + (a.pos.z - from.z).powi(2);
            let db = (b.pos.x - from.x).powi(2) + (b.pos.z - from.z).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    pub fn floor_y(&self, x: f32, z: f32) -> f32 {
        if let Some(height) = self.ramp_height(x, z) {
            return height;
        }
        if self.in_disc(x, z) {
            return self.platform.height;
        }
        0.0
    }

    /// Surface the body should stand on. Jetpack ledges only catch you from just above.
    pub fn support_y(&self, x: f32, y: f32, z: f32, vel_y: f32, catch: &LedgeCatch) -> f32 {
        if let Some(top) = self.ledge_top(x, y, z, vel_y, catch) {
            return top;
        }
        let floor = self.floor_y(x, z);
        if self.in_disc(x, z) && self.ramp_height(x, z).is_none() && y < floor - 0.35 && vel_y <= 0.0 {
            return 0.0;
        }
        floor
    }

    pub fn clamp_play(&self, pos: &mut Vec3, pad: f32) {
        let hx = (self.size[0] * 0.5 - pad).max(0.5);
        let hz = (self.size[1] * 0.5 - pad).max(0.5);
        pos.x = pos.x.clamp(-hx, hx);
        pos.z = pos.z.clamp(-hz, hz);
        if self.shape == "circle" {
            let limit = (self.radius - pad).max(0.5);
            let d = (pos.x * pos.x + pos.z * pos.z).sqrt();
            if d > limit {
                let scale = limit / d;
                pos.x *= scale;
                pos.z *= scale;
            }
        }
    }

    pub fn inside_play(&self, pos: Vec3, pad: f32) -> bool {
        let hx = self.size[0] * 0.5 - pad;
        let hz = self.size[1] * 0.5 - pad;
        if pos.x.abs() > hx || pos.z.abs() > hz {
            return false;
        }
        if self.shape == "circle" {
            let limit = (self.radius - pad).max(0.0);
            return pos.x * pos.x + pos.z * pos.z <= limit * limit + 1e-4;
        }
        true
    }

    /// Deck, ramp volume, and the low railing. Shots along the walkable top pass.
    pub fn blocks_shot(&self, p0: Vec3, p1: Vec3) -> bool {
        if !self.inside_play(p1, 0.0) {
            return true;
        }
        for i in 0..=6 {
            let t = i as f32 / 6.0;
            let p = p0 + (p1 - p0) * t;
            if self.solid_at(p) {
                return true;
            }
        }
        false
    }

    pub fn shove_bug(&self, pos: &mut Vec3) {
        if pos.y > self.platform.height - 0.5 || self.ramp_height(pos.x, pos.z).is_some() {
            return;
        }
        if !self.in_disc(pos.x, pos.z) {
            return;
        }
        let dx = pos.x - self.platform.center.x;
        let dz = pos.z - self.platform.center.z;
        let d = (dx * dx + dz * dz).sqrt().max(0.001);
        let rim = self.platform.radius + 0.05;
        pos.x = self.platform.center.x + dx / d * rim;
        pos.z = self.platform.center.z + dz / d * rim;
    }

    /// Keep the player on the disc, off the railing, and out of the deck volume.
    pub fn resolve_player(&self, pos: &mut Vec3, old: Vec3, pad: f32) {
        self.clamp_play(pos, pad);
        self.hold_railing(pos, old);
        self.push_out_of_deck(pos, old);
    }

    fn hold_railing(&self, pos: &mut Vec3, old: Vec3) {
        let old_d = self.disc_dist(old.x, old.z);
        let new_d = self.disc_dist(pos.x, pos.z);
        if old_d > self.platform.radius + 0.15 || new_d <= self.platform.radius {
            return;
        }
        if pos.y > self.platform.height + self.platform.railing_height {
            return;
        }
        if old.y < self.platform.height - 0.3 {
            return;
        }
        if self.ramp_height(pos.x, pos.z).is_some() || self.ramp_height(old.x, old.z).is_some() {
            return;
        }
        let rim = (self.platform.radius - 0.02).max(0.1);
        let dx = pos.x - self.platform.center.x;
        let dz = pos.z - self.platform.center.z;
        pos.x = self.platform.center.x + dx / new_d.max(0.001) * rim;
        pos.z = self.platform.center.z + dz / new_d.max(0.001) * rim;
    }

    fn push_out_of_deck(&self, pos: &mut Vec3, old: Vec3) {
        if self.ramp_height(pos.x, pos.z).is_some() || !self.in_disc(pos.x, pos.z) {
            return;
        }
        if pos.y >= self.platform.height - 0.2 {
            return;
        }
        let old_inside = self.in_disc(old.x, old.z) && old.y < self.platform.height - 0.2;
        if old_inside {
            return;
        }
        let dx = pos.x - self.platform.center.x;
        let dz = pos.z - self.platform.center.z;
        let d = (dx * dx + dz * dz).sqrt().max(0.001);
        let rim = self.platform.radius + 0.02;
        pos.x = self.platform.center.x + dx / d * rim;
        pos.z = self.platform.center.z + dz / d * rim;
    }

    fn ledge_top(&self, x: f32, y: f32, z: f32, vel_y: f32, catch: &LedgeCatch) -> Option<f32> {
        for ledge in &self.ledges {
            if !self.in_perch_disk(x, z, ledge) {
                continue;
            }
            let top = ledge.pos.y;
            if !ledge.jetpack_only {
                if y <= top + 0.35 {
                    return Some(top);
                }
                continue;
            }
            let near = (y - top).abs() <= catch.snap
                || (vel_y <= catch.fall_speed && y <= top + catch.above && y >= top - catch.below);
            if near {
                return Some(top);
            }
        }
        None
    }

    fn ramp_height(&self, x: f32, z: f32) -> Option<f32> {
        for ramp in &self.platform.ramps {
            let abx = ramp.to.x - ramp.from.x;
            let abz = ramp.to.z - ramp.from.z;
            let len2 = abx * abx + abz * abz;
            if len2 < 1e-4 {
                continue;
            }
            let t = ((x - ramp.from.x) * abx + (z - ramp.from.z) * abz) / len2;
            if !(-0.02..=1.02).contains(&t) {
                continue;
            }
            let clamped = t.clamp(0.0, 1.0);
            let px = ramp.from.x + abx * clamped;
            let pz = ramp.from.z + abz * clamped;
            let lateral = ((x - px).powi(2) + (z - pz).powi(2)).sqrt();
            if lateral > ramp.width * 0.5 {
                continue;
            }
            return Some(self.platform.height * (1.0 - clamped));
        }
        None
    }

    fn in_disc(&self, x: f32, z: f32) -> bool {
        self.disc_dist(x, z) <= self.platform.radius
    }

    fn disc_dist(&self, x: f32, z: f32) -> f32 {
        let dx = x - self.platform.center.x;
        let dz = z - self.platform.center.z;
        (dx * dx + dz * dz).sqrt()
    }

    fn solid_at(&self, p: Vec3) -> bool {
        if let Some(height) = self.ramp_height(p.x, p.z) {
            if p.y >= -0.2 && p.y < height - 0.15 {
                return true;
            }
        }
        if self.in_disc(p.x, p.z) && p.y >= -0.2 && p.y < self.platform.height - 0.15 {
            return true;
        }
        let d = self.disc_dist(p.x, p.z);
        d - self.platform.radius <= 0.35
            && self.platform.radius - d <= 0.35
            && p.y >= self.platform.height
            && p.y <= self.platform.height + self.platform.railing_height
            && self.ramp_height(p.x, p.z).is_none()
    }
}

fn load_arena(text: &str) -> Result<Arena, String> {
    let root: toml::Value = toml::from_str(text).map_err(|err: toml::de::Error| err.to_string())?;
    let table = root.as_table().ok_or_else(|| "arena_01 is not a table".to_string())?;
    expect_keys(
        table,
        &[
            "size",
            "shape",
            "radius",
            "platform",
            "boss_area",
            "vent",
            "ledge",
            "pickup",
            "player_spawn",
            "perch_rules",
            "paint",
            "edge_wash",
        ],
        "arena_01",
    )?;
    let size = nums2(table.get("size").unwrap(), "size")?;
    let shape = table
        .get("shape")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "shape must be a string".to_string())?
        .to_string();
    if shape != "circle" {
        return Err(format!("shape \"{shape}\" is unknown"));
    }
    let radius = num(table.get("radius").unwrap(), "radius")?;
    if !(radius > 0.0) || size[0] + 1e-3 < radius * 2.0 || size[1] + 1e-3 < radius * 2.0 {
        return Err("size must contain the circle".into());
    }
    let platform = load_platform(table.get("platform").unwrap())?;
    let boss_area = load_boss_area(table.get("boss_area").unwrap())?;
    let vents = load_rows(table.get("vent").unwrap(), "vent", load_vent)?;
    let ledges = load_rows(table.get("ledge").unwrap(), "ledge", load_ledge)?;
    let pickups = load_rows(table.get("pickup").unwrap(), "pickup", load_pickup)?;
    if vents.is_empty() {
        return Err("arena_01 is missing vent".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for vent in &vents {
        if !seen.insert(vent.id.clone()) {
            return Err(format!("duplicate vent id {}", vent.id));
        }
        let dx = vent.pos.x - boss_area.center.x;
        let dz = vent.pos.z - boss_area.center.z;
        if (dx * dx + dz * dz).sqrt() <= boss_area.radius {
            return Err(format!("{} is inside the boss area", vent.id));
        }
    }
    for ledge in &ledges {
        if !seen.insert(ledge.id.clone()) {
            return Err(format!("duplicate ledge id {}", ledge.id));
        }
    }
    for pickup in &pickups {
        if !seen.insert(pickup.id.clone()) {
            return Err(format!("duplicate pickup id {}", pickup.id));
        }
    }
    let spawn = nums3(table.get("player_spawn").unwrap(), "player_spawn")?;
    let perch_rules = load_perch_rules(table.get("perch_rules").unwrap())?;
    let paint = load_arena_paint(table.get("paint").unwrap())?;
    let edge_wash = load_edge_wash(table.get("edge_wash").unwrap())?;
    Ok(Arena {
        half_extent: size[0].max(size[1]) * 0.5,
        size,
        shape,
        radius,
        platform,
        boss_area,
        vents,
        ledges,
        pickups,
        player_spawn: Vec3::new(spawn[0], spawn[1], spawn[2]),
        perch_rules,
        paint,
        edge_wash,
    })
}

fn load_platform(value: &toml::Value) -> Result<Platform, String> {
    let table = value.as_table().ok_or_else(|| "platform is not a table".to_string())?;
    expect_keys(table, &["center", "radius", "height", "railing_height", "ramps"], "platform")?;
    let center = nums2(table.get("center").unwrap(), "platform.center")?;
    let radius = num(table.get("radius").unwrap(), "platform.radius")?;
    let height = num(table.get("height").unwrap(), "platform.height")?;
    let railing_height = num(table.get("railing_height").unwrap(), "platform.railing_height")?;
    if !(radius > 0.0) || height < 0.0 || railing_height < 0.0 {
        return Err("platform radius, height, and railing_height must be >= 0".into());
    }
    let ramps_value = table.get("ramps").unwrap();
    let ramps = ramps_value
        .as_array()
        .ok_or_else(|| "platform.ramps must be an array".to_string())?;
    let mut loaded = Vec::new();
    for (i, ramp) in ramps.iter().enumerate() {
        let row = ramp.as_table().ok_or_else(|| format!("platform.ramps[{i}] is not a table"))?;
        expect_keys(row, &["from", "to", "width"], &format!("platform.ramps[{i}]"))?;
        let from = nums2(row.get("from").unwrap(), &format!("platform.ramps[{i}].from"))?;
        let to = nums2(row.get("to").unwrap(), &format!("platform.ramps[{i}].to"))?;
        let width = num(row.get("width").unwrap(), &format!("platform.ramps[{i}].width"))?;
        if !(width > 0.0) {
            return Err(format!("platform.ramps[{i}].width must be > 0"));
        }
        loaded.push(Ramp {
            from: Vec3::new(from[0], 0.0, from[1]),
            to: Vec3::new(to[0], 0.0, to[1]),
            width,
        });
    }
    if loaded.is_empty() {
        return Err("platform is missing ramps".into());
    }
    Ok(Platform {
        center: Vec3::new(center[0], 0.0, center[1]),
        radius,
        height,
        railing_height,
        ramps: loaded,
    })
}

fn load_boss_area(value: &toml::Value) -> Result<BossArea, String> {
    let table = value.as_table().ok_or_else(|| "boss_area is not a table".to_string())?;
    expect_keys(table, &["center", "radius"], "boss_area")?;
    let center = nums2(table.get("center").unwrap(), "boss_area.center")?;
    let radius = num(table.get("radius").unwrap(), "boss_area.radius")?;
    if !(radius > 0.0) {
        return Err("boss_area.radius must be > 0".into());
    }
    Ok(BossArea {
        center: Vec3::new(center[0], 0.0, center[1]),
        radius,
    })
}

fn load_vent(table: &toml::map::Map<String, toml::Value>) -> Result<Vent, String> {
    expect_keys(table, &["id", "pos", "bearing_deg", "telegraph"], "vent")?;
    let id = req_id(table, "vent")?;
    let pos = nums3(table.get("pos").unwrap(), &format!("{id}.pos"))?;
    let bearing_deg = num(table.get("bearing_deg").unwrap(), &format!("{id}.bearing_deg"))?;
    let telegraph = num(table.get("telegraph").unwrap(), &format!("{id}.telegraph"))?;
    if !(telegraph > 0.0) {
        return Err(format!("{id} telegraph must be > 0"));
    }
    Ok(Vent {
        id,
        pos: Vec3::new(pos[0], pos[1], pos[2]),
        bearing_deg,
        telegraph,
    })
}

fn load_ledge(table: &toml::map::Map<String, toml::Value>) -> Result<Ledge, String> {
    expect_keys(table, &["id", "pos", "jetpack_only"], "ledge")?;
    let id = req_id(table, "ledge")?;
    let pos = nums3(table.get("pos").unwrap(), &format!("{id}.pos"))?;
    let jetpack_only = table
        .get("jetpack_only")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| format!("{id} jetpack_only must be a bool"))?;
    Ok(Ledge {
        id,
        pos: Vec3::new(pos[0], pos[1], pos[2]),
        jetpack_only,
    })
}

fn load_pickup(table: &toml::map::Map<String, toml::Value>) -> Result<ArenaPickup, String> {
    expect_keys(table, &["id", "pos", "spawns_during"], "pickup")?;
    let id = req_id(table, "pickup")?;
    let pos = nums3(table.get("pos").unwrap(), &format!("{id}.pos"))?;
    let spawns_during = table
        .get("spawns_during")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{id} is missing spawns_during"))?
        .to_string();
    if spawns_during != "breather" {
        return Err(format!("{id} spawns_during \"{spawns_during}\" is unknown"));
    }
    Ok(ArenaPickup {
        id,
        pos: Vec3::new(pos[0], pos[1], pos[2]),
        spawns_during,
    })
}

fn validate_ledge_catch(catch: &LedgeCatch) -> Result<(), String> {
    for (key, value) in [
        ("snap", catch.snap),
        ("fall_speed", catch.fall_speed),
        ("above", catch.above),
        ("below", catch.below),
    ] {
        if !(value > 0.0) {
            return Err(format!("controller.toml [ledge_catch] {key} must be > 0"));
        }
    }
    Ok(())
}

fn load_perch_rules(value: &toml::Value) -> Result<PerchRules, String> {
    let table = value.as_table().ok_or_else(|| "perch_rules is not a table".to_string())?;
    expect_keys(
        table,
        &[
            "perch_radius",
            "perch_height_slack",
            "climb_trigger_distance",
            "climb_slide_rate",
            "flow_field_targets",
            "all_perched_grace",
            "climb_speed_mult",
            "climb_clearance",
            "ground_chase_pad",
            "budget_shift_to_spitters",
            "fuel_regen_on_perch",
        ],
        "perch_rules",
    )?;
    let flow_field_targets = table
        .get("flow_field_targets")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "perch_rules is missing flow_field_targets".to_string())?
        .to_string();
    if flow_field_targets != "nearest_grounded_player" {
        return Err(format!("flow_field_targets \"{flow_field_targets}\" is unknown"));
    }
    let perch_radius = num(table.get("perch_radius").unwrap(), "perch_radius")?;
    let perch_height_slack = num(table.get("perch_height_slack").unwrap(), "perch_height_slack")?;
    let climb_trigger_distance = num(table.get("climb_trigger_distance").unwrap(), "climb_trigger_distance")?;
    let climb_slide_rate = num(table.get("climb_slide_rate").unwrap(), "climb_slide_rate")?;
    let all_perched_grace = num(table.get("all_perched_grace").unwrap(), "all_perched_grace")?;
    let climb_speed_mult = num(table.get("climb_speed_mult").unwrap(), "climb_speed_mult")?;
    let climb_clearance = num(table.get("climb_clearance").unwrap(), "climb_clearance")?;
    let ground_chase_pad = num(table.get("ground_chase_pad").unwrap(), "ground_chase_pad")?;
    let budget_shift_to_spitters = num(table.get("budget_shift_to_spitters").unwrap(), "budget_shift_to_spitters")?;
    let fuel_regen_on_perch = table
        .get("fuel_regen_on_perch")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| "fuel_regen_on_perch must be a bool".to_string())?;
    if !(climb_clearance > 0.0) {
        return Err("climb_clearance must be > 0".into());
    }
    if !(ground_chase_pad > 0.0) {
        return Err("ground_chase_pad must be > 0".into());
    }
    if !(perch_height_slack > 0.0) {
        return Err("perch_height_slack must be > 0".into());
    }
    if !(perch_radius > 0.0)
        || !(climb_trigger_distance > 0.0)
        || climb_slide_rate < 0.0
        || all_perched_grace < 0.0
        || climb_speed_mult < 0.0
        || !(0.0..=1.0).contains(&budget_shift_to_spitters)
    {
        return Err("perch_rules numbers are out of range".into());
    }
    Ok(PerchRules {
        perch_radius,
        perch_height_slack,
        climb_trigger_distance,
        climb_slide_rate,
        flow_field_targets,
        all_perched_grace,
        climb_speed_mult,
        climb_clearance,
        ground_chase_pad,
        budget_shift_to_spitters,
        fuel_regen_on_perch,
    })
}

fn load_arena_paint(value: &toml::Value) -> Result<ArenaPaint, String> {
    let table = value.as_table().ok_or_else(|| "paint is not a table".to_string())?;
    expect_keys(
        table,
        &["floor_light", "floor_dark", "band_x", "band_y", "shade"],
        "paint",
    )?;
    let floor_light = req_string(table, "floor_light", "arena_01.toml paint")?;
    let floor_dark = req_string(table, "floor_dark", "arena_01.toml paint")?;
    let band_x = req_u32_key(table, "band_x", "paint")?;
    let band_y = req_u32_key(table, "band_y", "paint")?;
    let shade = num(table.get("shade").unwrap(), "shade")?;
    if floor_light.is_empty() || floor_dark.is_empty() {
        return Err("paint floor colors must be palette keys".into());
    }
    if band_x == 0 || band_y == 0 {
        return Err("paint band_x and band_y must be > 0".into());
    }
    if !(shade > 0.0 && shade <= 1.0) {
        return Err("paint shade must be in (0, 1]".into());
    }
    Ok(ArenaPaint {
        floor_light,
        floor_dark,
        band_x,
        band_y,
        shade,
    })
}

fn load_edge_wash(value: &toml::Value) -> Result<ArenaEdgeWash, String> {
    let table = value.as_table().ok_or_else(|| "edge_wash is not a table".to_string())?;
    expect_keys(table, &["sky", "horizon", "mix", "span"], "edge_wash")?;
    let sky = req_string(table, "sky", "arena_01.toml edge_wash")?;
    let horizon = num_at(table, "horizon", "edge_wash.horizon")?;
    let mix = num_at(table, "mix", "edge_wash.mix")?;
    let span = num_at(table, "span", "edge_wash.span")?;
    if sky.is_empty() {
        return Err("edge_wash sky must be a palette key".into());
    }
    if !(horizon > 0.0 && horizon < 1.0) {
        return Err("edge_wash.horizon must be in (0, 1)".into());
    }
    if !(mix >= 0.0 && mix <= 1.0) || !(span >= 0.0) {
        return Err("edge_wash mix and span are out of range".into());
    }
    Ok(ArenaEdgeWash { sky, horizon, mix, span })
}

fn num_at(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<f32, String> {
    num(table.get(key).unwrap(), what)
}

fn req_u32_key(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<u32, String> {
    let value = table.get(key).unwrap();
    let n = match value {
        toml::Value::Integer(n) if *n >= 0 && *n <= u32::MAX as i64 => *n as u32,
        toml::Value::Float(n) if n.is_finite() && *n >= 0.0 && n.fract() == 0.0 && *n <= u32::MAX as f64 => *n as u32,
        _ => return Err(format!("{what} {key} must be a non-negative integer")),
    };
    Ok(n)
}

fn load_rows<T>(
    value: &toml::Value,
    what: &str,
    load: fn(&toml::map::Map<String, toml::Value>) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let rows = value.as_array().ok_or_else(|| format!("{what} must be an array"))?;
    rows.iter()
        .map(|row| {
            let table = row.as_table().ok_or_else(|| format!("{what} row is not a table"))?;
            load(table)
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct CoopRules {
    pub max_players: u32,
    pub min_players: u32,
    pub join_allowed_during: String,
    pub upgrade_catchup: String,
    pub catchup_pick_mode: String,
    pub catchup_pick_count: u32,
    pub budget_rescale: String,
    pub slot_hold_seconds: f32,
    pub downed_on_return: bool,
    pub multi_reviver_speedup: f32,
    pub bleedout_pause_while_reviving: bool,
    pub squad_wipe: String,
    /// Extra revivers past the first that still add `multi_reviver_speedup`.
    pub max_extra_revivers: u32,
    pub currency_credit: String,
    pub leave_keeps_earned: bool,
    pub host_leave: String,
    pub host_away: crate::host_away::HostAwayRules,
    pub friendly_fire: bool,
}

fn load_coop(text: &str) -> Result<CoopRules, String> {
    let value: toml::Value = toml::from_str(text).map_err(|err: toml::de::Error| err.to_string())?;
    let root = value.as_table().ok_or("root must be a table")?;
    expect_keys(
        root,
        &[
            "lobby",
            "join_in_progress",
            "reconnect",
            "revive",
            "currency_share",
            "friendly_fire",
            "host_away",
        ],
        "coop.toml",
    )?;
    let lobby = req_table(root, "lobby", "coop.toml")?;
    expect_keys(lobby, &["max_players", "min_players"], "coop.toml lobby")?;
    let join = req_table(root, "join_in_progress", "coop.toml")?;
    expect_keys(
        join,
        &[
            "allowed_during",
            "upgrade_catchup",
            "catchup_pick_mode",
            "budget_rescale",
        ],
        "coop.toml join_in_progress",
    )?;
    let reconnect = req_table(root, "reconnect", "coop.toml")?;
    expect_keys(
        reconnect,
        &["slot_hold_seconds", "downed_on_return"],
        "coop.toml reconnect",
    )?;
    let revive = req_table(root, "revive", "coop.toml")?;
    expect_keys(
        revive,
        &[
            "multi_reviver_speedup",
            "max_extra_revivers",
            "bleedout_pause_while_reviving",
            "squad_wipe",
        ],
        "coop.toml revive",
    )?;
    let currency = req_table(root, "currency_share", "coop.toml")?;
    expect_keys(
        currency,
        &["credit", "leave_keeps_earned", "host_leave"],
        "coop.toml currency_share",
    )?;
    let fire = req_table(root, "friendly_fire", "coop.toml")?;
    expect_keys(fire, &["enabled"], "coop.toml friendly_fire")?;

    let join_allowed_during = req_string(join, "allowed_during", "coop.toml join_in_progress")?;
    let upgrade_catchup = req_string(join, "upgrade_catchup", "coop.toml join_in_progress")?;
    let catchup_pick_mode = req_string(join, "catchup_pick_mode", "coop.toml join_in_progress")?;
    let budget_rescale = req_string(join, "budget_rescale", "coop.toml join_in_progress")?;
    let squad_wipe = req_string(revive, "squad_wipe", "coop.toml revive")?;
    let currency_credit = req_string(currency, "credit", "coop.toml currency_share")?;
    let away = req_table(root, "host_away", "coop.toml")?;
    expect_keys(
        away,
        &[
            "detect_after_seconds",
            "end_after_seconds",
            "resume_countdown_seconds",
            "away_timer_reset_after_play_seconds",
            "freeze",
        ],
        "coop.toml host_away",
    )?;
    let detect_after_seconds = positive_seconds(away, "detect_after_seconds")?;
    let end_after_seconds = positive_seconds(away, "end_after_seconds")?;
    let resume_countdown_seconds = positive_seconds(away, "resume_countdown_seconds")?;
    let away_timer_reset_after_play_seconds = positive_seconds(away, "away_timer_reset_after_play_seconds")?;
    let freeze = req_string(away, "freeze", "coop.toml host_away")?;
    expect_tag(&freeze, "everything", "coop.toml host_away freeze")?;
    let host_leave = req_string(currency, "host_leave", "coop.toml currency_share")?;
    expect_tag(&join_allowed_during, "breather_only", "coop.toml allowed_during")?;
    expect_tag(&upgrade_catchup, "match_lowest", "coop.toml upgrade_catchup")?;
    expect_tag(&budget_rescale, "next_wave", "coop.toml budget_rescale")?;
    expect_tag(&squad_wipe, "all_downed", "coop.toml squad_wipe")?;
    expect_tag(&currency_credit, "per_round_present", "coop.toml credit")?;
    expect_tag(&host_leave, "match_ends_rounds_kept", "coop.toml host_leave")?;
    let catchup_pick_count = parse_choose_from(&catchup_pick_mode)?;

    let max_players = req_u32(lobby, "max_players", "coop.toml lobby")?;
    let min_players = req_u32(lobby, "min_players", "coop.toml lobby")?;
    if min_players == 0 || min_players > max_players {
        return Err("coop.toml lobby min_players must be in 1..=max_players".into());
    }
    Ok(CoopRules {
        max_players,
        min_players,
        join_allowed_during,
        upgrade_catchup,
        catchup_pick_mode,
        catchup_pick_count,
        budget_rescale,
        slot_hold_seconds: req_f32(reconnect, "slot_hold_seconds", "coop.toml reconnect")?,
        downed_on_return: req_bool(reconnect, "downed_on_return", "coop.toml reconnect")?,
        multi_reviver_speedup: req_f32(revive, "multi_reviver_speedup", "coop.toml revive")?,
        bleedout_pause_while_reviving: req_bool(revive, "bleedout_pause_while_reviving", "coop.toml revive")?,
        squad_wipe,
        max_extra_revivers: req_u32(revive, "max_extra_revivers", "coop.toml revive")?,
        currency_credit,
        leave_keeps_earned: req_bool(currency, "leave_keeps_earned", "coop.toml currency_share")?,
        host_leave,
        host_away: crate::host_away::HostAwayRules {
            detect_after_seconds,
            end_after_seconds,
            resume_countdown_seconds,
            away_timer_reset_after_play_seconds,
            freeze,
        },
        friendly_fire: req_bool(fire, "enabled", "coop.toml friendly_fire")?,
    })
}

fn positive_seconds(table: &toml::map::Map<String, toml::Value>, key: &str) -> Result<f32, String> {
    let n = req_f32(table, key, "coop.toml host_away")?;
    if !(n > 0.0) {
        return Err(format!("coop.toml host_away {key} must be positive"));
    }
    Ok(n)
}

fn req_table<'a>(
    table: &'a toml::map::Map<String, toml::Value>,
    key: &str,
    what: &str,
) -> Result<&'a toml::map::Map<String, toml::Value>, String> {
    table
        .get(key)
        .and_then(|value| value.as_table())
        .ok_or_else(|| format!("{what} is missing {key}"))
}

fn req_string(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<String, String> {
    table
        .get(key)
        .and_then(|value| value.as_str())
        .map(|text| text.to_string())
        .ok_or_else(|| format!("{what} is missing {key}"))
}

fn req_bool(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<bool, String> {
    table
        .get(key)
        .and_then(|value| value.as_bool())
        .ok_or_else(|| format!("{what} {key} must be a bool"))
}

fn expect_tag(value: &str, allowed: &str, what: &str) -> Result<(), String> {
    if value == allowed {
        Ok(())
    } else {
        Err(format!("{what} must be {allowed}"))
    }
}

fn parse_choose_from(mode: &str) -> Result<u32, String> {
    let Some(rest) = mode.strip_prefix("choose_from_") else {
        return Err(format!("catchup_pick_mode {mode} must be choose_from_N"));
    };
    let count: u32 = rest
        .parse()
        .map_err(|_| format!("catchup_pick_mode {mode} must be choose_from_N"))?;
    if count == 0 {
        return Err(format!("catchup_pick_mode {mode} needs at least one offer"));
    }
    Ok(count)
}

fn expect_keys(table: &toml::map::Map<String, toml::Value>, allowed: &[&str], what: &str) -> Result<(), String> {
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("{what} has unknown key {key}"));
        }
    }
    for key in allowed {
        if !table.contains_key(*key) {
            return Err(format!("{what} is missing {key}"));
        }
    }
    Ok(())
}

fn req_id(table: &toml::map::Map<String, toml::Value>, what: &str) -> Result<String, String> {
    table
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.is_empty())
        .map(|id| id.to_string())
        .ok_or_else(|| format!("{what} is missing id"))
}

fn num(value: &toml::Value, what: &str) -> Result<f32, String> {
    match value {
        toml::Value::Float(n) => Ok(*n as f32),
        toml::Value::Integer(n) => Ok(*n as f32),
        _ => Err(format!("{what} must be a number")),
    }
}

fn nums2(value: &toml::Value, what: &str) -> Result<[f32; 2], String> {
    let arr = value.as_array().ok_or_else(|| format!("{what} must be an array"))?;
    if arr.len() != 2 {
        return Err(format!("{what} must have 2 numbers"));
    }
    Ok([num(&arr[0], what)?, num(&arr[1], what)?])
}

fn nums3(value: &toml::Value, what: &str) -> Result<[f32; 3], String> {
    let arr = value.as_array().ok_or_else(|| format!("{what} must be an array"))?;
    if arr.len() != 3 {
        return Err(format!("{what} must have 3 numbers"));
    }
    Ok([num(&arr[0], what)?, num(&arr[1], what)?, num(&arr[2], what)?])
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
    pub hud: HudPresentation,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HudPresentation {
    pub away: AwayHud,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ShadowSample {
    pub ox: f32,
    pub oy: f32,
    pub weight: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AwayHud {
    pub slate: String,
    pub text: String,
    pub bar: String,
    pub count: String,
    pub count_outline: String,
    pub count_outline_px: f32,
    pub count_shadow: String,
    pub count_shadow_px: f32,
    pub count_shadow_soft_px: f32,
    pub count_shadow_alpha: f32,
    pub panel_opacity: f32,
    pub panel_width: f32,
    pub panel_height: f32,
    pub panel_top: f32,
    pub panel_pad: f32,
    pub title_px: f32,
    pub row_gap: f32,
    pub bar_height: f32,
    pub count_px: f32,
    pub count_scale_from: f32,
    pub count_scale_to: f32,
    /// 0 keeps full color. 1 is grayscale. The away look is partial so glows stay faintly colored.
    pub away_desaturation: f32,
    /// 0 is full brightness. 1 is black, clamped by `dim_floor` when the grade is applied.
    pub away_dim: f32,
    /// Music gain while the host is away, in dB. Restored across the resume countdown.
    pub music_duck_db: f32,
    /// Solo pause menu. Same duck as the away banner, including the low-pass below.
    pub solo_pause_duck_db: f32,
    /// Co-op pause menu. Music only, on the pausing client. Game sounds stay full.
    pub coop_pause_duck_db: f32,
    /// Low-pass cutoff while the host is away. Look 0 bypasses the filter.
    pub music_duck_lowpass_hz: f32,
    /// Timer-bar track brightness, as a fraction of the slate. The empty bar is never darker than this mix.
    pub bar_track: f32,
    /// Minimum remaining light while the away grade is on.
    pub dim_floor: f32,
    /// Soft-shadow taps. `ox`/`oy` scale `count_shadow_soft_px` and add to `count_shadow_px`.
    pub count_shadow_samples: Vec<ShadowSample>,
    pub coop: AwayHudCopy,
    pub solo: AwayHudCopy,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AwayHudCopy {
    pub text: String,
    pub show_timer: bool,
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
    /// Empty when the line is left out, so `require_palette_keys` can name `hero_white`.
    #[serde(default)]
    pub white: String,
    /// Filled at load from the starter hero's `accent_key`. Not a palette.toml entry.
    #[serde(default)]
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
    pub ui_panel_slate: String,
    pub reveal_stage: String,
    pub reveal_ink: String,
    pub reveal_paper: String,
    pub hero_vex: String,
    pub hero_nyra: String,
    pub hero_brakk: String,
    pub hero_kip: String,
    pub hero_vex_base: String,
    pub hero_nyra_base: String,
    pub hero_brakk_base: String,
    pub hero_kip_base: String,
    pub spitter_rim: String,
    pub hero_plate: String,
    pub portrait_field: String,
    pub arena_sky: String,
}

impl Palette {
    /// Hex for a known color name. `None` when the name is not in the palette.
    pub fn hex(&self, name: &str) -> Option<&str> {
        Some(match name {
            "swarmer_amber" => self.keys.swarmer_amber.as_str(),
            "swarmer_glow" => self.keys.swarmer_glow.as_str(),
            "spitter_carapace" => self.keys.spitter_carapace.as_str(),
            "spitter_glow" => self.keys.spitter_glow.as_str(),
            "brute_amber" => self.keys.brute_amber.as_str(),
            "brute_cracks" => self.keys.brute_cracks.as_str(),
            "brood_sac" => self.keys.brood_sac.as_str(),
            "bud_glow" => self.keys.bud_glow.as_str(),
            "ui_panel_slate" => self.keys.ui_panel_slate.as_str(),
            "reveal_stage" => self.keys.reveal_stage.as_str(),
            "reveal_ink" => self.keys.reveal_ink.as_str(),
            "reveal_paper" => self.keys.reveal_paper.as_str(),
            "hero_vex" => self.keys.hero_vex.as_str(),
            "hero_nyra" => self.keys.hero_nyra.as_str(),
            "hero_brakk" => self.keys.hero_brakk.as_str(),
            "hero_kip" => self.keys.hero_kip.as_str(),
            "hero_vex_base" => self.keys.hero_vex_base.as_str(),
            "hero_nyra_base" => self.keys.hero_nyra_base.as_str(),
            "hero_brakk_base" => self.keys.hero_brakk_base.as_str(),
            "hero_kip_base" => self.keys.hero_kip_base.as_str(),
            "spitter_rim" => self.keys.spitter_rim.as_str(),
            "hero_plate" => self.keys.hero_plate.as_str(),
            "portrait_field" => self.keys.portrait_field.as_str(),
            "arena_sky" => self.keys.arena_sky.as_str(),
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
            _ => return None,
        })
    }

    pub fn key(&self, name: &str) -> [f32; 4] {
        hex_srgb(self.hex(name).unwrap_or_else(|| panic!("unknown palette key {name}")))
    }
}

fn require_palette_keys(
    palette: &Palette,
    bugs: &crate::bugs::BugBook,
    away: &AwayHud,
    layout: &RevealLayout,
    ui: &Ui,
    heroes: &[HeroCard],
) -> Result<(), String> {
    let check = |owner: &str, name: &str| -> Result<(), String> {
        let hex = palette.hex(name).unwrap_or("");
        if hex.trim().trim_start_matches('#').len() >= 6 {
            return Ok(());
        }
        Err(format!("{owner} color \"{name}\" does not resolve"))
    };
    // Locked portraits paint the lit body with this key. Fail here, not in the painter.
    check("portrait", "hero_white")?;
    for (bug, row) in bugs.rows() {
        if !row.visual.bud_color.is_empty() {
            check(bug, &row.visual.bud_color)?;
        }
        if !row.visual.grown_color.is_empty() {
            check(bug, &row.visual.grown_color)?;
        }
        for part in &row.visual.parts {
            check(&format!("{bug} {}", part.name), &part.color)?;
        }
    }
    for rarity in ["common", "rare", "epic", "legendary"] {
        check("rarity", &format!("rarity_{rarity}"))?;
    }
    check("hud.away slate", &away.slate)?;
    check("hud.away text", &away.text)?;
    check("hud.away bar", &away.bar)?;
    check("hud.away count", &away.count)?;
    check("hud.away count_outline", &away.count_outline)?;
    check("hud.away count_shadow", &away.count_shadow)?;
    check("crate_reveal stage", &layout.stage)?;
    check("crate_reveal ink_dark", &layout.ink_dark)?;
    check("crate_reveal ink_light", &layout.ink_light)?;
    for (owner, name) in ui.color_keys() {
        check(owner, name)?;
    }
    for hero in heroes {
        check(&format!("characters.toml {}", hero.id), &hero.accent_key)?;
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
pub struct CrateReveal {
    pub fakeout_enabled: bool,
    pub fakeout_sfx: String,
    pub burst_fade: f32,
    pub freeze_dim: f32,
    pub common: CrateTier,
    pub rare: CrateTier,
    pub epic: CrateTier,
    pub legendary: CrateTier,
    pub multi_pull: CrateMulti,
    pub hint: RevealHint,
    pub card: CardTitle,
    pub layout: RevealLayout,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RevealHint {
    pub text_px: f32,
    pub opacity: f32,
    pub margin_x: f32,
    pub margin_y: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CardTitle {
    pub title_pad: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RevealLayout {
    pub panel_width: f32,
    pub panel_height: f32,
    pub title_px: f32,
    pub hint_px: f32,
    pub glyph_rows: u32,
    pub row_gap: f32,
    pub ink_luma: f32,
    pub title_pad_px: u32,
    pub title_glyph_px: u32,
    pub title_height_div: u32,
    pub title_scale_min: u32,
    pub title_scale_max: u32,
    pub border_px: u32,
    pub border_mix: f32,
    pub gem_height: f32,
    pub gem_glow: f32,
    pub divider_px: u32,
    pub stage: String,
    pub ink_dark: String,
    pub ink_light: String,
    pub stage_base: [f32; 3],
    pub stage_lift: [f32; 3],
    pub stage_vignette: [f32; 3],
    /// Backdrop center, as a fraction of the frame height.
    pub stage_y: f32,
    /// Exponent on the center lift.
    pub stage_lift_pow: f32,
    /// Exponent on the corner vignette.
    pub stage_vignette_pow: f32,
}

fn validate_reveal_panel(layout: &RevealLayout) -> Result<(), String> {
    if !(layout.panel_width > 0.0) || !(layout.panel_height > 0.0) {
        return Err("crate_reveal.toml layout panel_width and panel_height must be positive".into());
    }
    if !(layout.title_px > 0.0) || !(layout.hint_px > 0.0) {
        return Err("crate_reveal.toml layout title_px and hint_px must be positive".into());
    }
    if layout.glyph_rows == 0 {
        return Err("crate_reveal.toml layout glyph_rows must be > 0".into());
    }
    if !(layout.row_gap >= 0.0) {
        return Err("crate_reveal.toml layout row_gap must be non-negative".into());
    }
    if !(layout.ink_luma > 0.0 && layout.ink_luma <= 1.0) {
        return Err("crate_reveal.toml layout ink_luma must be in (0, 1]".into());
    }
    if layout.title_glyph_px == 0 || layout.title_height_div == 0 || layout.title_scale_min == 0 {
        return Err("crate_reveal.toml layout title scale formula must be positive".into());
    }
    if layout.title_scale_max < layout.title_scale_min {
        return Err("crate_reveal.toml layout title_scale_max must be >= title_scale_min".into());
    }
    if layout.border_px == 0 {
        return Err("crate_reveal.toml layout border_px must be > 0".into());
    }
    let border = layout.border_px as f32;
    if border * 2.0 >= layout.panel_width || border * 2.0 >= layout.panel_height {
        return Err("crate_reveal.toml layout border_px must leave a face inside the panel".into());
    }
    if !(layout.border_mix > 0.0 && layout.border_mix <= 1.0) {
        return Err("crate_reveal.toml layout border_mix must be in (0, 1]".into());
    }
    if !(layout.gem_height > 0.0 && layout.gem_height <= 1.0) {
        return Err("crate_reveal.toml layout gem_height must be in (0, 1]".into());
    }
    if !(layout.gem_glow > 0.0) {
        return Err("crate_reveal.toml layout gem_glow must be > 0".into());
    }
    if layout.divider_px == 0 {
        return Err("crate_reveal.toml layout divider_px must be > 0".into());
    }
    let face_w = layout.panel_width - border * 2.0;
    let legend = 9.0 * layout.title_glyph_px as f32;
    let pad = layout.title_pad_px as f32 * 2.0;
    if !(face_w > pad && (face_w - pad) / legend >= layout.title_scale_min as f32) {
        return Err("crate_reveal.toml layout title scale must fit LEGENDARY inside the padded face".into());
    }
    for channel in layout.stage_base {
        if !channel.is_finite() || !(-255.0..=255.0).contains(&channel) {
            return Err("crate_reveal.toml layout stage_base channels must be in [-255, 255]".into());
        }
    }
    for (name, rgb) in [("stage_lift", layout.stage_lift), ("stage_vignette", layout.stage_vignette)] {
        for channel in rgb {
            if !(channel >= 0.0 && channel <= 255.0) {
                return Err(format!("crate_reveal.toml layout {name} channels must be in [0, 255]"));
            }
        }
    }
    if !(layout.stage_y > 0.0 && layout.stage_y < 1.0) {
        return Err("crate_reveal.toml layout stage_y must be in (0, 1)".into());
    }
    if !(layout.stage_lift_pow > 0.0) || !(layout.stage_vignette_pow > 0.0) {
        return Err("crate_reveal.toml layout stage_lift_pow and stage_vignette_pow must be > 0".into());
    }
    Ok(())
}

fn validate_stage_offset(palette: &Palette, layout: &RevealLayout) -> Result<(), String> {
    let rgb = palette
        .hex(&layout.stage)
        .ok_or_else(|| format!("crate_reveal.toml layout stage color \"{}\" does not resolve", layout.stage))?;
    let bytes = palette_bytes(rgb);
    for (i, channel) in layout.stage_base.iter().enumerate() {
        let painted = bytes[i] as f32 + channel;
        if !(painted >= 0.0 && painted <= 255.0) {
            return Err("crate_reveal.toml layout stage_base pushes the stage color out of byte range".into());
        }
    }
    Ok(())
}

fn palette_bytes(hex: &str) -> [u8; 3] {
    let rgb = hex_srgb(hex);
    [(rgb[0] * 255.0) as u8, (rgb[1] * 255.0) as u8, (rgb[2] * 255.0) as u8]
}

fn require_arena_colors(palette: &Palette, arena: &Arena) -> Result<(), String> {
    let check = |owner: &str, name: &str| -> Result<(), String> {
        if palette.hex(name).is_some() {
            return Ok(());
        }
        Err(format!("{owner} color \"{name}\" does not resolve"))
    };
    check("paint floor_light", &arena.paint.floor_light)?;
    check("paint floor_dark", &arena.paint.floor_dark)?;
    check("edge_wash sky", &arena.edge_wash.sky)?;
    Ok(())
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
    /// When set, the cue's musical downbeat. `host_back` must match the resume countdown.
    pub sync_at: Option<f32>,
    /// True when the mix row's category is `ui`. Menu cues must be this category.
    pub category_ui: bool,
    /// A full voice list may stop this one-shot to make room for a UI cue.
    /// Looping rows are forced off when the mix loads.
    pub stealable: bool,
    /// Telegraphs and other cues that must play out. Loading rejects a warning that is stealable.
    pub is_warning: bool,
    /// True when the cue is not positioned in the world.
    pub flat: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StingDef {
    pub id: String,
    pub path: String,
    /// Linear gain from a required `volume_db`.
    pub volume: f32,
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

/// True when the crate-reveal pixel font can draw `ch`. Lower case counts as upper case.
pub fn font_covers(ch: char) -> bool {
    let ch = match ch {
        'a'..='z' => (ch as u8 - b'a' + b'A') as char,
        other => other,
    };
    matches!(
        ch,
        ' ' | 'A'..='Z' | '0'..='9' | '+' | '-' | '%' | ':' | '.' | ',' | '/' | ';'
    )
}

fn require_font(text: &str, what: &str) -> Result<(), String> {
    for ch in text.chars() {
        if !font_covers(ch) {
            return Err(format!("{what} is missing font glyph '{ch}'"));
        }
    }
    Ok(())
}

/// Fixed-decimal display. Does not change the stored value.
pub fn format_fixed(value: f32, decimals: u32) -> String {
    let decimals = decimals.min(6) as usize;
    format!("{value:.decimals$}")
}

#[derive(Clone, Debug, Deserialize)]
pub struct Ui {
    pub colors: UiColors,
    pub copy: UiCopy,
    pub layout: UiLayout,
    pub portrait: UiPortrait,
    pub settings: UiSettings,
    pub sounds: UiSounds,
}

impl Ui {
    pub fn color_keys(&self) -> [(&str, &str); 10] {
        [
            ("ui.colors.panel", self.colors.panel.as_str()),
            ("ui.colors.text", self.colors.text.as_str()),
            ("ui.colors.dim", self.colors.dim.as_str()),
            ("ui.colors.stage", self.colors.stage.as_str()),
            ("ui.colors.focus_before_hero", self.colors.focus_before_hero.as_str()),
            ("ui.colors.victory", self.colors.victory.as_str()),
            ("ui.colors.defeat", self.colors.defeat.as_str()),
            ("ui.colors.locked", self.colors.locked.as_str()),
            ("ui.colors.locked_text", self.colors.locked_text.as_str()),
            ("ui.portrait.bg", self.portrait.bg.as_str()),
        ]
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct UiColors {
    pub panel: String,
    pub text: String,
    pub dim: String,
    pub stage: String,
    pub focus_before_hero: String,
    pub victory: String,
    pub defeat: String,
    pub locked: String,
    /// Palette key for name, role, ability, and the HP/SPD line on a locked card.
    pub locked_text: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UiCopy {
    pub title: String,
    pub play: String,
    pub crates: String,
    pub settings: String,
    pub quit: String,
    pub back: String,
    pub resume: String,
    pub quit_to_menu: String,
    pub play_again: String,
    pub main_menu: String,
    pub click_to_play: String,
    pub host: String,
    pub join: String,
    pub coming_soon: String,
    pub heroes: String,
    pub difficulty: String,
    pub locked: String,
    pub hp: String,
    pub speed: String,
    pub dmg: String,
    pub power: String,
    pub waves: String,
    pub kills: String,
    pub time: String,
    pub currency: String,
    pub victory: String,
    pub defeat: String,
    pub sensitivity: String,
    pub fov: String,
    pub view_bob: String,
    pub invert_y: String,
    pub master: String,
    pub music: String,
    pub effects: String,
    pub on: String,
    pub off: String,
    pub reset_notice: String,
    pub crates_title: String,
    pub crates_empty: String,
    pub crates_pull: String,
    pub crates_open: String,
    pub pull_failed: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UiLayout {
    pub title_scale: u32,
    pub button_scale: u32,
    pub hint_scale: u32,
    pub border_px: f32,
    pub button_width: f32,
    pub button_height: f32,
    pub row_gap: f32,
    pub title_gap: f32,
    pub margin_x: f32,
    pub margin_y: f32,
    pub card_width: f32,
    pub card_height: f32,
    pub card_gap: f32,
    /// Portrait height as a fraction of the card. Centered in the top half.
    pub portrait_fraction: f32,
    /// Locked portrait glow mixes this far from grayscale back toward the accent.
    pub locked_accent_mix: f32,
    /// Non-accent portrait pixels desaturate this far on a locked card. 1 is fully gray.
    pub locked_body_desaturation: f32,
    pub card_pad: f32,
    pub pause_keep: f32,
    /// Upgrade-card fill alpha. The RGB comes from `colors.panel`.
    pub panel_alpha: f32,
    pub pause_panel_width: f32,
    pub pause_panel_height: f32,
    pub slider_width: f32,
    pub slider_height: f32,
    pub settings_row_height: f32,
    /// Smallest settings −/+ button, in screen pixels after the row is scaled.
    pub stepper_min_px: f32,
    /// Button side is at most the row width divided by this.
    pub stepper_width_divisor: u32,
    /// Border thickness is the button side divided by this.
    pub stepper_border_divisor: u32,
    pub stat_decimals: u32,
    pub mult_decimals: u32,
    pub currency_decimals: u32,
    pub nav_repeat_secs: f32,
    pub stick_deadzone: f32,
    /// How far a focused border mixes toward the paper color. 0 is the base color.
    pub focus_lift: f32,
    pub fit_width: f32,
    pub fit_height: f32,
    /// Extra pixels between wrapped glyph rows.
    pub leading: f32,
    /// Gap after a lock line, a name, and between the two lines of a tier button.
    pub spacing: f32,
    /// Gap after a hero role line.
    pub spacing_role: f32,
    /// Gap after an ability line, a results line, and between settings rows.
    pub spacing_block: f32,
    /// Slider label inset from the row's left edge.
    pub inset_x: f32,
    /// Slider label inset from the row's top edge.
    pub inset_y: f32,
    /// Gap between the slider track and the bottom of the row.
    pub inset_bar: f32,
    /// Padding kept between the portrait and the card's mid line.
    pub inset_portrait: f32,
    /// Smallest border inset, in screen pixels after scaling.
    pub inset_min: f32,
}

/// Framing for the hero-card portrait raster.
#[derive(Clone, Debug, Deserialize)]
pub struct UiPortrait {
    /// Palette key of the empty field. Pixels of this color are not copied onto the card.
    pub bg: String,
    pub raster: u32,
    pub fov_floor: f32,
    pub fov_limit: f32,
    pub fov_step: f32,
    /// Vertices closer than this, in viewmodel depth, do not fit.
    pub near: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UiSettings {
    pub sensitivity: f32,
    pub sensitivity_min: f32,
    pub sensitivity_max: f32,
    pub sensitivity_step: f32,
    pub fov: f32,
    pub fov_min: f32,
    pub fov_max: f32,
    pub fov_step: f32,
    pub invert_y: bool,
    pub view_bob: bool,
    pub master: f32,
    pub music: f32,
    pub effects: f32,
    pub volume_min: f32,
    pub volume_max: f32,
    pub volume_step: f32,
    pub default_difficulty: String,
    pub settings_file: String,
    pub settings_key: String,
    /// Playtest hero selection. Off by default. Does not grant ownership.
    pub unlock_all: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UiSounds {
    pub hover: String,
    pub confirm: String,
    pub back: String,
    pub error: String,
    pub slider: String,
    pub toggle: String,
    pub pause_open: String,
    pub pause_close: String,
    pub match_start: String,
    pub victory: String,
    pub defeat: String,
}

impl UiSounds {
    pub fn ids(&self) -> [(&str, &str); 11] {
        [
            ("hover", self.hover.as_str()),
            ("confirm", self.confirm.as_str()),
            ("back", self.back.as_str()),
            ("error", self.error.as_str()),
            ("slider", self.slider.as_str()),
            ("toggle", self.toggle.as_str()),
            ("pause_open", self.pause_open.as_str()),
            ("pause_close", self.pause_close.as_str()),
            ("match_start", self.match_start.as_str()),
            ("victory", self.victory.as_str()),
            ("defeat", self.defeat.as_str()),
        ]
    }
}

fn load_ui(text: &str) -> Result<Ui, String> {
    parse("ui.toml", text)
}

fn validate_downed(downed: &DownedCfg) -> Result<(), String> {
    if !(downed.revive_time.is_finite() && downed.revive_time > 0.0) {
        return Err("controller.toml downed.revive_time must be > 0".into());
    }
    if !(downed.solo_self_revive_time.is_finite() && downed.solo_self_revive_time > 0.0) {
        return Err("controller.toml downed.solo_self_revive_time must be > 0".into());
    }
    Ok(())
}

fn validate_ui(
    ui: &Ui,
    ui_text: &str,
    presentation: &Presentation,
    controller: &Controller,
    audio: &AudioTable,
    difficulty: &[Tier],
    glyphs: &RevealLayout,
    reveal_text: &str,
    heroes: &[HeroCard],
) -> Result<(), String> {
    let layout = &ui.layout;
    let settings = &ui.settings;
    if layout.title_scale == 0 || layout.button_scale == 0 || layout.hint_scale == 0 {
        return Err("ui.toml scales must be at least 1".into());
    }
    if !(layout.border_px > 0.0) {
        return Err("ui.toml layout.border_px must be > 0".into());
    }
    for (name, value) in [
        ("button_width", layout.button_width),
        ("button_height", layout.button_height),
        ("card_width", layout.card_width),
        ("card_height", layout.card_height),
        ("pause_panel_width", layout.pause_panel_width),
        ("pause_panel_height", layout.pause_panel_height),
        ("slider_width", layout.slider_width),
        ("slider_height", layout.slider_height),
        ("settings_row_height", layout.settings_row_height),
        ("fit_width", layout.fit_width),
        ("fit_height", layout.fit_height),
    ] {
        if !(value.is_finite() && value > 0.0) {
            return Err(format!("ui.toml layout.{name} must be > 0"));
        }
    }
    if (layout.fit_width - 1280.0).abs() > 0.1 || (layout.fit_height - 720.0).abs() > 0.1 {
        return Err("ui.toml fit frame must be 1280x720".into());
    }
    if !(layout.pause_keep > 0.0 && layout.pause_keep <= 1.0) {
        return Err("ui.toml layout.pause_keep must be in (0, 1]".into());
    }
    if !(layout.panel_alpha > 0.0 && layout.panel_alpha <= 1.0) {
        return Err("ui.toml layout.panel_alpha must be in (0, 1]".into());
    }
    if !(layout.stepper_min_px.is_finite() && layout.stepper_min_px >= 1.0) {
        return Err("ui.toml layout.stepper_min_px must be >= 1".into());
    }
    if layout.stepper_width_divisor < 2 {
        return Err("ui.toml layout.stepper_width_divisor must be >= 2".into());
    }
    if layout.stepper_border_divisor < 2 {
        return Err("ui.toml layout.stepper_border_divisor must be >= 2".into());
    }
    if !(layout.focus_lift > 0.0 && layout.focus_lift < 1.0) {
        return Err("ui.toml layout.focus_lift must be in (0, 1)".into());
    }
    if !(layout.nav_repeat_secs > 0.0) {
        return Err("ui.toml layout.nav_repeat_secs must be > 0".into());
    }
    if !(layout.stick_deadzone > 0.0 && layout.stick_deadzone < 1.0) {
        return Err("ui.toml layout.stick_deadzone must be in (0, 1)".into());
    }
    if layout.stat_decimals > 4 || layout.mult_decimals > 4 || layout.currency_decimals > 4 {
        return Err("ui.toml decimals must be in 0..=4".into());
    }
    if !(layout.portrait_fraction > 0.0 && layout.portrait_fraction <= 0.5) {
        return Err("ui.toml layout.portrait_fraction must be in (0, 0.5]".into());
    }
    for (name, value) in [
        ("leading", layout.leading),
        ("spacing", layout.spacing),
        ("spacing_role", layout.spacing_role),
        ("spacing_block", layout.spacing_block),
        ("inset_x", layout.inset_x),
        ("inset_y", layout.inset_y),
        ("inset_bar", layout.inset_bar),
        ("inset_portrait", layout.inset_portrait),
        ("inset_min", layout.inset_min),
    ] {
        if !(value.is_finite() && value >= 0.0) {
            return Err(format!("ui.toml layout.{name} must be >= 0"));
        }
    }
    let portrait = &ui.portrait;
    if portrait.bg.is_empty() {
        return Err("ui.toml portrait.bg must be a palette key".into());
    }
    if portrait.raster == 0 {
        return Err("ui.toml portrait.raster must be > 0".into());
    }
    if !(portrait.fov_floor > 0.0 && portrait.fov_limit > portrait.fov_floor && portrait.fov_limit < 180.0) {
        return Err("ui.toml portrait fov search must sit inside (0, 180)".into());
    }
    if !(portrait.fov_step > 0.0) {
        return Err("ui.toml portrait.fov_step must be > 0".into());
    }
    if !(portrait.near > 0.0) {
        return Err("ui.toml portrait.near must be > 0".into());
    }
    if !(layout.locked_accent_mix > 0.0 && layout.locked_accent_mix < 1.0) {
        return Err("ui.toml layout.locked_accent_mix must be in (0, 1)".into());
    }
    if !(layout.locked_body_desaturation >= 0.0 && layout.locked_body_desaturation <= 1.0) {
        return Err("ui.toml layout.locked_body_desaturation must be in [0, 1]".into());
    }
    check_range("sensitivity", settings.sensitivity, settings.sensitivity_min, settings.sensitivity_max, settings.sensitivity_step)?;
    check_range("fov", settings.fov, settings.fov_min, settings.fov_max, settings.fov_step)?;
    check_range("master", settings.master, settings.volume_min, settings.volume_max, settings.volume_step)?;
    check_range("music", settings.music, settings.volume_min, settings.volume_max, settings.volume_step)?;
    check_range("effects", settings.effects, settings.volume_min, settings.volume_max, settings.volume_step)?;
    if (settings.fov_min - presentation.fov_min).abs() > 0.01 || (settings.fov_max - presentation.fov_max).abs() > 0.01 {
        return Err("ui.toml fov range must match presentation.toml fov_min and fov_max".into());
    }
    if (settings.fov - controller.camera.fov_default).abs() > 0.01 {
        return Err("ui.toml settings.fov must match controller.toml fov_default".into());
    }
    if settings.default_difficulty.is_empty() || !difficulty.iter().any(|tier| tier.id == settings.default_difficulty) {
        return Err(format!(
            "ui.toml default_difficulty \"{}\" is not a difficulty.toml tier",
            settings.default_difficulty
        ));
    }
    if settings.settings_file.is_empty() || settings.settings_key.is_empty() {
        return Err("ui.toml settings file and key must be set".into());
    }
    let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let check_files = assets.is_dir();
    for (slot, id) in ui.sounds.ids() {
        let row = audio
            .sfx
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| format!("ui.toml sounds.{slot} \"{id}\" is missing from audio_mix.toml"))?;
        if !row.category_ui {
            return Err(format!("audio_mix.toml {id} category must be ui"));
        }
        if check_files && !assets.join(&row.path).is_file() {
            return Err(format!("ui sound {id} file {} is missing", row.path));
        }
    }
    let glyph = glyphs.title_glyph_px.max(1);
    let button_inner = layout.button_width - layout.border_px * 2.0;
    let title_inner = layout.fit_width - layout.margin_x * 2.0;
    let card_inner = layout.card_width - layout.border_px * 2.0 - layout.card_pad * 2.0;
    let buttons = [
        &ui.copy.play,
        &ui.copy.crates,
        &ui.copy.settings,
        &ui.copy.quit,
        &ui.copy.back,
        &ui.copy.resume,
        &ui.copy.quit_to_menu,
        &ui.copy.play_again,
        &ui.copy.main_menu,
        &ui.copy.click_to_play,
        &ui.copy.host,
        &ui.copy.join,
        &ui.copy.crates_pull,
        &ui.copy.crates_open,
        &ui.copy.on,
        &ui.copy.off,
    ];
    for label in buttons {
        require_fit(label, layout.button_scale, glyph, button_inner, "button")?;
    }
    for label in [
        &ui.copy.title,
        &ui.copy.heroes,
        &ui.copy.difficulty,
        &ui.copy.crates_title,
        &ui.copy.victory,
        &ui.copy.defeat,
        &ui.copy.settings,
        &ui.copy.coming_soon,
        &ui.copy.reset_notice,
        &ui.copy.click_to_play,
    ] {
        require_fit(label, layout.title_scale, glyph, title_inner, "title")?;
    }
    for tier in difficulty {
        require_fit(&tier.id.to_uppercase(), layout.button_scale, glyph, button_inner, "difficulty")?;
        let line = format!(
            "{} X{}  {} X{}",
            ui.copy.hp,
            format_fixed(tier.enemy_hp, layout.mult_decimals),
            ui.copy.dmg,
            format_fixed(tier.enemy_dmg, layout.mult_decimals)
        );
        require_fit(&line, layout.hint_scale, glyph, button_inner, "difficulty stats")?;
    }
    for hero in heroes {
        require_fit(&hero.name.to_uppercase(), layout.button_scale, glyph, card_inner, "hero name")?;
        require_fit(&hero.role.to_uppercase(), layout.hint_scale, glyph, card_inner, "hero role")?;
        for word in hero.ability.split_whitespace() {
            require_fit(&word.to_uppercase(), layout.hint_scale, glyph, card_inner, "hero ability")?;
        }
        let stats = format!(
            "{} {}  {} {}",
            ui.copy.hp,
            format_fixed(hero.max_hp, 0),
            ui.copy.speed,
            format_fixed(hero.move_speed_mult, layout.stat_decimals)
        );
        require_fit(&stats, layout.hint_scale, glyph, card_inner, "hero stats")?;
    }
    let rows = layout.margin_y * 2.0
        + layout.title_scale as f32 * glyphs.glyph_rows as f32
        + layout.title_gap
        + layout.card_height
        + layout.row_gap
        + layout.button_height;
    if rows > layout.fit_height {
        return Err(format!("ui.toml hero screen is {rows}px tall and the fit frame is {}", layout.fit_height));
    }
    require_drawn_menu_font(ui_text, ui, difficulty, heroes, reveal_text)?;
    Ok(())
}

/// Glyphs for every string `paint_menu` draws. Copy comes from the `[copy]` table,
/// not a field list. Tier ids and rarity names are the labels those screens paint.
fn require_drawn_menu_font(
    ui_text: &str,
    ui: &Ui,
    difficulty: &[Tier],
    heroes: &[HeroCard],
    reveal_text: &str,
) -> Result<(), String> {
    let root: toml::Value = toml::from_str(ui_text).map_err(|err| err.to_string())?;
    let copy = root
        .get("copy")
        .and_then(|value| value.as_table())
        .ok_or_else(|| "ui.toml is missing [copy]".to_string())?;
    for (field, value) in copy {
        let text = value
            .as_str()
            .ok_or_else(|| format!("ui.toml copy.{field} must be a string"))?;
        require_font(text, &format!("ui.toml copy.{field}"))?;
    }
    for tier in difficulty {
        require_font(&tier.id, &format!("difficulty.toml {}", tier.id))?;
        let line = format!(
            "{} X{}  {} X{}",
            ui.copy.hp,
            format_fixed(tier.enemy_hp, ui.layout.mult_decimals),
            ui.copy.dmg,
            format_fixed(tier.enemy_dmg, ui.layout.mult_decimals)
        );
        require_font(&line, &format!("difficulty.toml {} stats", tier.id))?;
    }
    for hero in heroes {
        require_font(&hero.name, &format!("characters.toml {} name", hero.id))?;
        require_font(&hero.role, &format!("characters.toml {} role", hero.id))?;
        require_font(&hero.ability, &format!("characters.toml {} ability", hero.id))?;
        let stats = format!(
            "{} {}  {} {}",
            ui.copy.hp,
            format_fixed(hero.max_hp, 0),
            ui.copy.speed,
            format_fixed(hero.move_speed_mult, ui.layout.stat_decimals)
        );
        require_font(&stats, &format!("characters.toml {} stats", hero.id))?;
    }
    let settings = &ui.settings;
    for (what, text) in [
        ("sensitivity", format_fixed(settings.sensitivity, 2)),
        ("fov", format_fixed(settings.fov, 0)),
        ("master", format_fixed(settings.master, 2)),
        ("music", format_fixed(settings.music, 2)),
        ("effects", format_fixed(settings.effects, 2)),
    ] {
        require_font(&text, &format!("ui.toml settings.{what}"))?;
    }
    require_font(&format!("{} 0:00", ui.copy.time), "ui.toml copy.time")?;
    require_font(
        &format!("{} {}", ui.copy.currency, format_fixed(0.0, ui.layout.currency_decimals)),
        "ui.toml copy.currency",
    )?;
    let reveal: toml::Value = toml::from_str(reveal_text).map_err(|err| err.to_string())?;
    let table = reveal
        .as_table()
        .ok_or_else(|| "crate_reveal.toml is not a table".to_string())?;
    let mut rarities = 0u32;
    for (name, value) in table {
        let Some(row) = value.as_table() else { continue };
        if !(row.contains_key("tease") && row.contains_key("sting")) {
            continue;
        }
        rarities += 1;
        require_font(name, &format!("crate_reveal.toml {name}"))?;
    }
    if rarities == 0 {
        return Err("crate_reveal.toml has no rarity tiers".into());
    }
    Ok(())
}

fn check_range(name: &str, value: f32, min: f32, max: f32, step: f32) -> Result<(), String> {
    if !(min.is_finite() && max.is_finite() && step.is_finite() && step > 0.0 && min < max) {
        return Err(format!("ui.toml {name} range is invalid"));
    }
    if !(value.is_finite() && value >= min && value <= max) {
        return Err(format!("ui.toml settings.{name} {value} is outside [{min}, {max}]"));
    }
    Ok(())
}

fn require_fit(label: &str, scale: u32, glyph: u32, inner: f32, what: &str) -> Result<(), String> {
    if label.is_empty() {
        return Err(format!("ui.toml {what} label is empty"));
    }
    let width = label.chars().count() as f32 * glyph as f32 * scale as f32;
    if width > inner {
        return Err(format!("ui.toml {what} \"{label}\" is {width}px and the slot is {inner}px"));
    }
    Ok(())
}
