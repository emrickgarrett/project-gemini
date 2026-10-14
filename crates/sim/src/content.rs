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
    /// Lit scenery. Mesh building stays in the mesh crate.
    pub arena_scene: crate::scenery::ArenaScene,
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
        crate::bugs::require_point_names_are_not_parts(&bugs).map_err(|err| format!("bugs.toml: {err}"))?;
        crate::bugs::require_glow_hit_centers(&bugs).map_err(|err| format!("bugs.toml: {err}"))?;
        crate::bugs::require_leg_layout(&bugs).map_err(|err| format!("bugs.toml: {err}"))?;
        let bosses = load_bosses(src.bosses).map_err(|err| format!("bosses.toml: {err}"))?;
        require_spawn_clearance(&bosses, &bugs).map_err(|err| format!("bosses.toml: {err}"))?;
        let mut palette: Palette = parse("palette.toml", src.palette)?;
        validate_lit_keys(&palette).map_err(|err| format!("palette.toml: {err}"))?;
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
        crate::bugs::require_tail_glow_radius(&bugs, cast.tail_glow_radius)
            .map_err(|err| format!("bugs.toml: {err}"))?;
        for mesh in &heroes_visual.meshes {
            if !heroes.iter().any(|card| card.portrait.mesh == mesh.id) {
                return Err(format!(
                    "heroes_visual.toml mesh \"{}\" is not a characters.toml portrait",
                    mesh.id
                ));
            }
        }
        let (arena_dressing, arena_scene) = load_arena_dressing(src.arena_visual)?;
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
        let waves: WavesFile = parse("waves.toml", src.waves)?;
        let combat: Combat = parse("combat.toml", src.combat)?;
        let arena = load_arena(src.arena).map_err(|err| format!("arena_01.toml: {err}"))?;
        require_arena_colors(&palette, &arena).map_err(|err| format!("arena_01.toml: {err}"))?;
        if (arena_dressing.rail_height - arena.platform.railing_height).abs() > arena.scenery.overlap_epsilon {
            return Err(
                "arena_visual.toml rail_height must match platform.railing_height".into(),
            );
        }
        crate::scenery::validate(
            &arena,
            &arena_scene,
            &palette,
            controller.camera.eye_height,
            &waves,
            &bosses,
            &combat,
        )
        .map_err(|err| format!("arena scenery: {err}"))?;
        validate_arena_audio(&arena, &audio).map_err(|err| format!("audio_mix.toml: {err}"))?;
        require_emitted_game_voices(&audio, &bugs, &bosses, &heroes, &ui)?;
        validate_stage_offset(&palette, &crate_reveal.layout)?;
        Ok(Self {
            fingerprint,
            waves,
            difficulty,
            controller,
            upgrades: parse::<UpgradeFile>("run_upgrades.toml", src.run_upgrades)?.upgrade,
            endless: parse("endless.toml", src.endless)?,
            currency: {
                let currency: Currency = parse("currency.toml", src.currency)?;
                validate_currency(&currency)?;
                currency
            },
            banners: {
                let banners = load_banners(src.banners)?;
                validate_ten_grid(&crate_reveal, &banners, &ui, &audio, &palette)?;
                banners
            },
            duplicate: load_duplicate(src.duplicate_ranks)?,
            bosses,
            weapons,
            hero,
            roster,
            heroes,
            warnings,
            arena,
            presentation,
            combat,
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
            arena_scene,
            cast,
        })
    }

    /// The award named by a server pull. `None` when the id is not in the catalog.
    pub fn crate_item(&self, id: &str) -> Option<&CrateItem> {
        self.banners.items.iter().find(|item| item.id == id)
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

    pub fn part_under(&self, pos: Vec3) -> Result<String, String> {
        self.arena.part_under(&self.arena_scene, &self.arena_dressing, pos)
    }

    pub fn surface_kind(&self, part: &str) -> Result<String, String> {
        self.arena
            .surfaces
            .iter()
            .find(|row| row.part == part)
            .map(|row| row.kind.clone())
            .ok_or_else(|| format!("surface part \"{part}\" is missing"))
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
        ids.push(format!("crate_{name}_hit"));
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
    pub flat_after_wave: u32,
    pub endless_milestone_every: u32,
    pub endless_milestone_bonus: f32,
    pub match_complete_bonus: f32,
    pub boss_kill_bonus: f32,
    pub per_round_cap_mult: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Banners {
    pub id: String,
    pub rates: BannerRates,
    pub pity: BannerPity,
    pub published: BannerPublished,
    pub cost: BannerCost,
    pub saved: BannerSaved,
    pub dev: BannerDev,
    /// `[[item]]` rows. The server picks one of the rolled rarity.
    #[serde(default, rename = "item")]
    pub items: Vec<CrateItem>,
}

/// One crate award. `icon` is rows of `#` (ink) and `.` (paper). `ink` is not a rarity color.
#[derive(Clone, Debug, Deserialize)]
pub struct CrateItem {
    pub id: String,
    pub name: String,
    pub rarity: String,
    pub ink: String,
    pub icon: Vec<String>,
}

/// Dev-build test wallet. The backend reads this. A release account does not.
#[derive(Clone, Debug, Deserialize)]
pub struct BannerDev {
    pub starting_credits: f32,
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
    pub pods: u32,
    pub ten_pull_cost: u32,
}

/// How many pull receipts a save keeps. Older keys are dropped.
#[derive(Clone, Debug, Deserialize)]
pub struct BannerSaved {
    pub max_keys: u32,
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
    missing: u8,
}

impl Cosmetics {
    /// Drops one tier's shard row. A later max-rank pull faults before the roll.
    pub fn forget_shard_row(&mut self, rarity: &str) -> Result<(), String> {
        let bit = shard_bit(rarity)?;
        self.shard_cost.missing |= bit;
        Ok(())
    }

    pub fn shards_for(&self, rarity: &str) -> Result<u32, String> {
        let (bit, cost) = match rarity {
            "common" => (1u8, self.shard_cost.common),
            "rare" => (2, self.shard_cost.rare),
            "epic" => (4, self.shard_cost.epic),
            "legendary" => (8, self.shard_cost.legendary),
            other => return Err(format!("unknown rarity \"{other}\"")),
        };
        if self.shard_cost.missing & bit != 0 {
            return Err(format!("cosmetics.toml shard_cost is missing {rarity}"));
        }
        Ok(cost)
    }
}

fn shard_bit(rarity: &str) -> Result<u8, String> {
    match rarity {
        "common" => Ok(1),
        "rare" => Ok(2),
        "epic" => Ok(4),
        "legendary" => Ok(8),
        other => Err(format!("unknown rarity \"{other}\"")),
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
    /// Hero id for a weapon. Empty on bodies, shields, and bugs.
    pub hero: String,
    pub file: String,
    /// SHA-256 of the shipped glb. The mesh crate checks the file bytes.
    pub sha256: String,
    /// The glb parents the mesh under a `*_grip_root` node. Guns and the shield.
    pub grip_root: bool,
    /// Cleaned mask PNG under assets/. Red is accent, green is glow.
    pub mask: String,
    pub triangles: u32,
    pub bytes: u32,
    pub source: String,
    pub license: String,
    pub task: String,
    pub base: String,
    /// Mask red. Empty on a bug cast.
    pub primary: String,
    /// Mask green. Empty on a bug cast.
    pub secondary: String,
    pub accent: String,
    /// Mask alpha, a fourth color. Empty when that channel has no weight.
    pub fourth: String,
    /// sRGB shade map under assets/. Empty on a bug cast.
    pub shade: String,
    pub glow: String,
    pub rim: String,
    pub fit: String,
    pub size: f32,
    pub yaw_deg: f32,
    pub at: [f32; 3],
    pub grip: [f32; 3],
    pub view: [f32; 3],
    /// Pitch of a weapon around its grip, in degrees, before `viewmodel_offset`.
    /// Positive raises the muzzle. Bodies and bugs stay at 0.
    pub view_pitch_deg: f32,
    /// Stock panel UV on TEXCOORD_0. `v = 0` is the top row. Weapons only.
    pub stock_uv: Option<[f32; 2]>,
    /// `primary` or `secondary`. Empty when this model has no stock point.
    pub stock_channel: String,
    /// Rail panel UV on TEXCOORD_0. Weapons only.
    pub rail_uv: Option<[f32; 2]>,
    /// `primary` or `secondary`. Empty when this model has no rail point.
    pub rail_channel: String,
}

/// Tint scalars and the side-sheet camera. Colors stay palette keys.
#[derive(Clone, Debug)]
pub struct CastLook {
    /// Linear divisor for a shade map. Albedo is `(shade / shade_mid_lin) * sum(weight * color)`.
    pub shade_mid_lin: f32,
    /// A fitted cast axis must land within this fraction of its body hitbox.
    pub hitbox_fill: f32,
    /// Max allowed max(axis scale) / min(axis scale), checked before the fit stretches.
    pub axis_scale_limit: f32,
    /// Authored radius of the tyrant tail glow. A larger cannon glow fails the load.
    pub tail_glow_radius: f32,
    /// Smallest share of a UV island that must match its face's mask channel.
    pub min_island_coverage: f32,
    /// Meters a glow center may sit off the cast. Past this, the mesh load fails.
    pub hit_surface_gap: f32,
    pub rim_strength: f32,
    /// Facing share of the rim color on an unmasked bug texel. The fresnel takes the rest.
    pub rim_fill: f32,
    /// Mask channel Overdrive lights on the equipped gun: primary, secondary, accent, or fourth.
    pub overdrive_emissive_channel: String,
    /// HDR multiplier on `hero_<id>` while that channel is emissive.
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
    /// Radial distance, in radii, inside which a glow core is pulled forward.
    pub glow_core_perp: f32,
    /// Closest a pulled glow core may sit to the camera, in the same units as the radius.
    pub glow_core_depth: f32,
    /// Longest edge, as a fraction of the model extent, before a triangle can be a needle.
    pub needle_extent: f32,
    /// Area over longest-edge squared below which that long triangle is a needle.
    pub needle_area: f32,
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
        let fit_peek = map.get("fit").and_then(|v| v.as_str()).unwrap_or("");
        let channeled = matches!(fit_peek, "hero" | "weapon" | "shield");
        let mut keys = vec![
            "id", "file", "sha256", "mask", "triangles", "bytes", "source", "license", "task", "base", "accent", "rim", "fit",
            "grip_root", "size", "yaw_deg", "at", "grip", "view",
        ];
        if channeled {
            keys.extend(["primary", "secondary", "fourth", "shade"]);
        } else {
            keys.push("glow");
        }
        if fit_peek == "weapon" {
            keys.extend(["hero", "stock_uv", "stock_channel", "rail_uv", "rail_channel", "view_pitch_deg"]);
        }
        expect_keys(map, &keys, &format!("{what} model"))?;
        let id = vis_str(map, "id", &format!("{what} model"))?.to_string();
        if id.is_empty() || models.iter().any(|model| model.id == id) {
            return Err(format!("{what} duplicate or empty model id \"{id}\""));
        }
        let file = vis_str(map, "file", &format!("{what} {id}"))?.to_string();
        let expected = format!("models/{id}.glb");
        if file != expected {
            return Err(format!("{what} {id} file \"{file}\" is unknown"));
        }
        let sha256 = vis_str(map, "sha256", &format!("{what} {id}"))?.to_ascii_lowercase();
        if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("{what} {id} sha256 must be 64 hex characters"));
        }
        let mask = vis_str(map, "mask", &format!("{what} {id}"))?.to_string();
        let expected_mask = if channeled {
            format!("masks/{id}_mask_rgba.png")
        } else {
            format!("masks/{id}_mask.png")
        };
        if mask != expected_mask {
            return Err(format!("{what} {id} mask \"{mask}\" is unknown"));
        }
        let shade = if channeled {
            let shade = vis_str(map, "shade", &format!("{what} {id}"))?.to_string();
            let expected_shade = format!("masks/{id}_shade.png");
            if shade != expected_shade {
                return Err(format!("{what} {id} shade \"{shade}\" is unknown"));
            }
            shade
        } else {
            String::new()
        };
        let fit = vis_str(map, "fit", &format!("{what} {id}"))?.to_string();
        if !matches!(fit.as_str(), "hero" | "hitbox" | "weapon" | "shield") {
            return Err(format!("{what} {id} fit \"{fit}\" is unknown"));
        }
        let grip_root = vis_bool(map, "grip_root", &format!("{what} {id}"))?;
        if grip_root && !matches!(fit.as_str(), "weapon" | "shield") {
            return Err(format!("{what} {id} grip_root is only for a weapon or a shield"));
        }
        if !grip_root && matches!(fit.as_str(), "weapon" | "shield") {
            return Err(format!("{what} {id} needs grip_root; there is no +X fallback"));
        }
        let size = vis_f32(map, "size", &format!("{what} {id}"))?;
        if fit != "hitbox" && !(size > 0.0) {
            return Err(format!("{what} {id} size must be > 0"));
        }
        let labeled = id.clone();
        let color = |key: &str| -> Result<String, String> {
            let name = vis_str(map, key, &format!("{what} {labeled}"))?.to_string();
            if name.is_empty() {
                if key == "rim" || (channeled && (key == "accent" || key == "fourth")) {
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
        let hero = if fit == "weapon" {
            let hero = vis_str(map, "hero", &format!("{what} {id}"))?.to_string();
            if hero.is_empty() {
                return Err(format!("{what} {id} hero is empty"));
            }
            hero
        } else {
            String::new()
        };
        let (stock_uv, stock_channel, rail_uv, rail_channel) = if fit == "weapon" {
            let stock_uv = vis_vec2(map, "stock_uv", &format!("{what} {id}"))?;
            let rail_uv = vis_vec2(map, "rail_uv", &format!("{what} {id}"))?;
            let stock_channel = vis_str(map, "stock_channel", &format!("{what} {id}"))?.to_string();
            let rail_channel = vis_str(map, "rail_channel", &format!("{what} {id}"))?.to_string();
            for (name, uv) in [("stock_uv", stock_uv), ("rail_uv", rail_uv)] {
                if uv.iter().any(|n| !n.is_finite() || !(0.0..=1.0).contains(n)) {
                    return Err(format!("{what} {id} {name} must be finite and in 0..=1"));
                }
            }
            for (name, channel) in [("stock_channel", &stock_channel), ("rail_channel", &rail_channel)] {
                if !matches!(channel.as_str(), "primary" | "secondary") {
                    return Err(format!("{what} {id} {name} \"{channel}\" is unknown"));
                }
            }
            (Some(stock_uv), stock_channel, Some(rail_uv), rail_channel)
        } else {
            (None, String::new(), None, String::new())
        };
        let view_pitch_deg = if fit == "weapon" {
            let pitch = vis_f32(map, "view_pitch_deg", &format!("{what} {labeled}"))?;
            if !pitch.is_finite() || !(-80.0..=80.0).contains(&pitch) {
                return Err(format!("{what} {labeled} view_pitch_deg must be finite and in -80..=80"));
            }
            pitch
        } else {
            0.0
        };
        models.push(CastModel {
            id,
            hero,
            file,
            sha256,
            mask,
            triangles: vis_u32(map, "triangles", &format!("{what} model"))?,
            bytes: vis_u32(map, "bytes", &format!("{what} model"))?,
            source,
            license,
            task,
            base: color("base")?,
            primary: if channeled { color("primary")? } else { String::new() },
            secondary: if channeled { color("secondary")? } else { String::new() },
            accent: color("accent")?,
            fourth: if channeled { color("fourth")? } else { String::new() },
            shade,
            glow: if channeled { String::new() } else { color("glow")? },
            rim: color("rim")?,
            fit,
            grip_root,
            size,
            yaw_deg,
            at: vis_vec3(map, "at", &format!("{what} model"))?,
            grip: vis_vec3(map, "grip", &format!("{what} model"))?,
            view: vis_vec3(map, "view", &format!("{what} model"))?,
            view_pitch_deg,
            stock_uv,
            stock_channel,
            rail_uv,
            rail_channel,
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
            "shade_mid_lin",
            "hitbox_fill",
            "axis_scale_limit",
            "tail_glow_radius",
            "min_island_coverage",
            "hit_surface_gap",
            "rim_strength",
            "rim_fill",
            "overdrive_emissive_channel",
            "overdrive_emissive_gain",
            "pod_glow_key",
            "pod_glow_base",
            "pod_glow_per_glow",
            "glow_falloff",
            "glow_show_through",
            "glow_core_perp",
            "glow_core_depth",
            "needle_extent",
            "needle_area",
            "review_yaw_deg",
            "review_elev_deg",
            "review_distance",
            "review_fov_deg",
        ],
        "cast.toml",
    )?;
    let shade_mid_lin = vis_f32(table, "shade_mid_lin", "cast.toml")?;
    let rim_strength = vis_f32(table, "rim_strength", "cast.toml")?;
    if !(shade_mid_lin > 0.0 && shade_mid_lin <= 1.0) {
        return Err("cast.toml shade_mid_lin must be > 0 and <= 1".into());
    }
    let hitbox_fill = vis_f32(table, "hitbox_fill", "cast.toml")?;
    if !(hitbox_fill >= 0.0 && hitbox_fill < 0.5) {
        return Err("cast.toml hitbox_fill must be >= 0 and < 0.5".into());
    }
    let axis_scale_limit = vis_f32(table, "axis_scale_limit", "cast.toml")?;
    if !(axis_scale_limit >= 1.0 && axis_scale_limit <= 3.0) {
        return Err("cast.toml axis_scale_limit must be >= 1 and <= 3".into());
    }
    let tail_glow_radius = vis_f32(table, "tail_glow_radius", "cast.toml")?;
    if !(tail_glow_radius > 0.0 && tail_glow_radius < 2.0) {
        return Err("cast.toml tail_glow_radius must be > 0 and < 2".into());
    }
    let min_island_coverage = vis_f32(table, "min_island_coverage", "cast.toml")?;
    if !(min_island_coverage >= 0.98 && min_island_coverage <= 1.0) {
        return Err("cast.toml min_island_coverage must be >= 0.98 and <= 1".into());
    }
    let hit_surface_gap = vis_f32(table, "hit_surface_gap", "cast.toml")?;
    if !(hit_surface_gap > 0.0 && hit_surface_gap <= 0.2) {
        return Err("cast.toml hit_surface_gap must be > 0 and <= 0.2".into());
    }
    if !(rim_strength >= 0.0 && rim_strength <= 1.0) {
        return Err("cast.toml rim_strength must be between 0 and 1".into());
    }
    let rim_fill = vis_f32(table, "rim_fill", "cast.toml")?;
    if !(rim_fill >= 0.0 && rim_fill < 1.0) {
        return Err("cast.toml rim_fill must be >= 0 and < 1".into());
    }
    let overdrive_emissive_channel = vis_str(table, "overdrive_emissive_channel", "cast.toml")?.to_string();
    if channel_index(&overdrive_emissive_channel).is_none() {
        return Err(format!(
            "cast.toml overdrive_emissive_channel \"{overdrive_emissive_channel}\" is unknown"
        ));
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
    let glow_core_perp = vis_f32(table, "glow_core_perp", "cast.toml")?;
    let glow_core_depth = vis_f32(table, "glow_core_depth", "cast.toml")?;
    if !(glow_core_perp > 0.0 && glow_core_perp <= 1.0) {
        return Err("cast.toml glow_core_perp must be > 0 and <= 1".into());
    }
    if !(glow_core_depth > 0.0 && glow_core_depth < 1.0) {
        return Err("cast.toml glow_core_depth must be > 0 and < 1".into());
    }
    let needle_extent = vis_f32(table, "needle_extent", "cast.toml")?;
    let needle_area = vis_f32(table, "needle_area", "cast.toml")?;
    if !(needle_extent > 0.0 && needle_extent <= 1.0) {
        return Err("cast.toml needle_extent must be > 0 and <= 1".into());
    }
    if !(needle_area > 0.0 && needle_area < 1.0) {
        return Err("cast.toml needle_area must be > 0 and < 1".into());
    }
    let review_yaw_deg = vis_f32(table, "review_yaw_deg", "cast.toml")?;
    let review_elev_deg = vis_f32(table, "review_elev_deg", "cast.toml")?;
    let review_distance = vis_f32(table, "review_distance", "cast.toml")?;
    let review_fov_deg = vis_f32(table, "review_fov_deg", "cast.toml")?;
    if !(review_distance > 0.0) || !(review_fov_deg > 0.0 && review_fov_deg < 180.0) {
        return Err("cast.toml review camera is out of range".into());
    }
    Ok(CastLook {
        shade_mid_lin,
        hitbox_fill,
        axis_scale_limit,
        tail_glow_radius,
        min_island_coverage,
        hit_surface_gap,
        rim_strength,
        rim_fill,
        overdrive_emissive_channel,
        overdrive_emissive_gain,
        pod_glow_key,
        pod_glow_base,
        pod_glow_per_glow,
        glow_falloff,
        glow_show_through,
        glow_core_perp,
        glow_core_depth,
        needle_extent,
        needle_area,
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

fn load_arena_dressing(text: &str) -> Result<(ArenaDressing, crate::scenery::ArenaScene), String> {
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
            "floor_sides",
            "boulder_slices",
            "boulder_stacks",
            "cover_chamfer",
            "nest_sides",
            "flora_sides",
            "trim_height",
            "trim_outset",
            "trim_lift",
            "panel_length",
            "panel_thickness",
            "panel_depth_scale",
            "panel_lift",
            "panel_count",
            "vent_size",
            "vent_center_y",
            "vent_tell_size",
            "vent_tell_height",
            "vent_tell_emissive",
            "ledge_size",
            "ledge_thickness",
            "ledge_skirt_scale",
            "ledge_plinth_run",
            "ledge_plinth_sides",
            "ledge_plinth_steps",
            "bulb_chamfer_scale",
            "bulb_stacks",
            "clutter_chamfer_scale",
            "dome_radius_scale",
            "dome_height_scale",
            "ramp_skip_pad",
            "min_floor_sides",
            "min_slice_count",
            "min_stack_count",
            "min_panel_count",
            "camera_fov_min",
            "camera_fov_max",
            "min_camera_pixels",
            "navy",
            "pale",
            "trim",
            "boulder",
            "beacon_glow_period",
            "prop_mesh",
            "platform_sides",
            "deck_phase_deg",
            "deck_step_outset",
            "deck_step_height",
            "deck_open_deg",
            "stair_count",
            "stair_tread",
            "stair_thickness",
            "rim_rows",
            "rim_boulders",
            "rim_fill",
            "rim_inner_scale",
            "rim_stagger_deg",
            "rim_min_arc_deg",
            "chunk_size",
            "nest_lift",
            "nest_gap_deg",
            "nest_ribs",
            "nest_rib_scale",
            "review_shadow",
            "shadow_normal_y",
            "shadow_min_t",
            "shadow_max_t",
            "shadow_self_pad",
            "max_triangles",
            "max_bytes",
            "sha256",
            "review_ambient",
            "review_key",
            "review_fill",
            "review_gain",
            "exposure_ev",
            "light",
            "material",
            "floor",
            "cliff",
            "prop",
            "nest",
            "tower",
            "camera",
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
    if !(dress.rail_height > 0.0) {
        return Err("arena_visual.toml rail_height must be > 0".into());
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
    let scene = crate::scenery::load_scene(table)?;
    Ok((dress, scene))
}

pub(crate) fn vis_str<'a>(table: &'a toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<&'a str, String> {
    table.get(key).and_then(|v| v.as_str()).ok_or_else(|| format!("{what} is missing {key}"))
}

pub(crate) fn vis_f32(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<f32, String> {
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

pub(crate) fn vis_u32(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<u32, String> {
    let n = table
        .get(key)
        .and_then(|v| v.as_integer())
        .ok_or_else(|| format!("{what} {key} must be an integer"))?;
    if n < 0 {
        return Err(format!("{what} {key} must be >= 0"));
    }
    Ok(n as u32)
}

pub(crate) fn vis_bool(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<bool, String> {
    table
        .get(key)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| format!("{what} {key} must be a bool"))
}

pub(crate) fn vis_vec2(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<[f32; 2], String> {
    let arr = table
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("{what} {key} must be 2 numbers"))?;
    if arr.len() != 2 {
        return Err(format!("{what} {key} must be 2 numbers"));
    }
    let mut out = [0.0; 2];
    for (i, slot) in arr.iter().enumerate() {
        if let Some(n) = slot.as_float() {
            out[i] = n as f32;
        } else if let Some(n) = slot.as_integer() {
            out[i] = n as f32;
        } else {
            return Err(format!("{what} {key} must be 2 numbers"));
        }
        if !out[i].is_finite() {
            return Err(format!("{what} {key} is not finite"));
        }
    }
    Ok(out)
}

pub(crate) fn vis_vec3(table: &toml::map::Map<String, toml::Value>, key: &str, what: &str) -> Result<[f32; 3], String> {
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
            missing: 0,
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
    pub scenery: crate::scenery::SceneryRules,
    pub surfaces: Vec<crate::scenery::Surface>,
    pub lanes: Vec<crate::scenery::Corridor>,
    pub routes: Vec<crate::scenery::Corridor>,
    pub revive_spots: Vec<crate::scenery::ReviveSpot>,
    pub cover: Vec<crate::scenery::Cover>,
    /// Stance tests, first match wins. Each name has a test in `part_under`.
    pub stance_order: Vec<String>,
    /// Stance name to content part id (`surface.part`).
    pub stance_part: BTreeMap<String, String>,
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
        if self.cover_blocks_segment(p0, p1) {
            return true;
        }
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

    pub fn shove_bug(&self, pos: &mut Vec3, radius: f32) {
        if pos.y > self.platform.height - self.scenery.deck_shove_drop
            || self.ramp_height(pos.x, pos.z).is_some()
        {
            self.push_cover(pos, radius);
            return;
        }
        if self.in_disc(pos.x, pos.z) {
            let dx = pos.x - self.platform.center.x;
            let dz = pos.z - self.platform.center.z;
            let d = (dx * dx + dz * dz)
                .sqrt()
                .max(self.scenery.shove_center_epsilon);
            let rim = self.platform.radius + self.scenery.shove_rim_outset;
            pos.x = self.platform.center.x + dx / d * rim;
            pos.z = self.platform.center.z + dz / d * rim;
        }
        self.push_cover(pos, radius);
    }

    /// Keep the player on the disc, off the railing, and out of the deck volume.
    pub fn resolve_player(&self, pos: &mut Vec3, old: Vec3, pad: f32) {
        self.clamp_play(pos, pad);
        self.hold_railing(pos, old);
        self.push_out_of_deck(pos, old);
        self.push_cover(pos, pad);
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

    pub fn solid_at(&self, p: Vec3) -> bool {
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

    /// Content part id under `pos`. The first stance in `stance_order` whose test hits wins.
    pub fn part_under(
        &self,
        scene: &crate::scenery::ArenaScene,
        dress: &ArenaDressing,
        pos: Vec3,
    ) -> Result<String, String> {
        for name in &self.stance_order {
            if !stance_supported(name) {
                return Err(format!("stance \"{name}\" has no test"));
            }
            if self.stance_test(name, scene, dress, pos)? {
                let part = self
                    .stance_part
                    .get(name)
                    .ok_or_else(|| format!("stance \"{name}\" has no part"))?;
                return Ok(part.clone());
            }
        }
        Err(format!("no stance under ({}, {}, {})", pos.x, pos.y, pos.z))
    }

    fn stance_test(
        &self,
        name: &str,
        scene: &crate::scenery::ArenaScene,
        dress: &ArenaDressing,
        pos: Vec3,
    ) -> Result<bool, String> {
        if name == "ramp" {
            return Ok(self.ramp_height(pos.x, pos.z).is_some());
        }
        if name == "cover" {
            let slack = self.scenery.bounds_slack;
            for cover in &self.cover {
                let hx = cover.size[0] * 0.5 + slack;
                let hz = cover.size[2] * 0.5 + slack;
                let top = cover.pos.y + cover.size[1] * 0.5;
                let on_xz = (pos.x - cover.pos.x).abs() <= hx && (pos.z - cover.pos.z).abs() <= hz;
                let on_top = (pos.y - top).abs() <= slack;
                if on_xz && on_top {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        if name == "nest" {
            for nest in &scene.nests {
                let vent = self
                    .vents
                    .iter()
                    .find(|vent| vent.id == nest.vent)
                    .ok_or_else(|| format!("nest {} vent \"{}\" is missing", nest.id, nest.vent))?;
                let anchor = crate::scenery::nest_anchor(vent.pos.x, vent.pos.z, nest.outset, scene.nest_lift);
                let hx = nest.size[0] * 0.5;
                let hz = nest.size[2] * 0.5;
                if (pos.x - anchor[0]).abs() <= hx && (pos.z - anchor[2]).abs() <= hz {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        if name == "rail" {
            if !(dress.rail_thickness > 0.0) {
                return Err("rail_thickness must be > 0".into());
            }
            let dist = self.disc_dist(pos.x, pos.z);
            let inner = self.platform.radius - dress.rail_thickness;
            return Ok(self.in_disc(pos.x, pos.z) && dist >= inner);
        }
        if name == "deck" {
            return Ok(self.in_disc(pos.x, pos.z));
        }
        if name == "rim" {
            let dist = (pos.x * pos.x + pos.z * pos.z).sqrt();
            return Ok(dist > self.radius);
        }
        if name == "ground" {
            let dist = (pos.x * pos.x + pos.z * pos.z).sqrt();
            return Ok(dist <= self.radius);
        }
        Err(format!("stance \"{name}\" has no test"))
    }
}

fn stance_supported(name: &str) -> bool {
    name == "ramp"
        || name == "cover"
        || name == "nest"
        || name == "rail"
        || name == "deck"
        || name == "rim"
        || name == "ground"
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
            "scenery",
            "surface",
            "lane",
            "route",
            "revive_spot",
            "cover",
            "stance_order",
            "stance_part",
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
    let play = crate::scenery::load_play(table)?;
    let stance_order = string_list(table.get("stance_order").unwrap(), "stance_order")?;
    let stance_rows = table
        .get("stance_part")
        .and_then(|value| value.as_table())
        .ok_or_else(|| "arena_01 is missing stance_part".to_string())?;
    let mut stance_part = BTreeMap::new();
    for (name, value) in stance_rows {
        let part = value
            .as_str()
            .filter(|text| !text.is_empty())
            .ok_or_else(|| format!("stance_part {name} must be a string"))?;
        stance_part.insert(name.clone(), part.to_string());
    }
    validate_stance_map(&stance_order, &stance_part, &play.surfaces, &play.cover)?;
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
        scenery: play.rules,
        surfaces: play.surfaces,
        lanes: play.lanes,
        routes: play.routes,
        revive_spots: play.revive_spots,
        cover: play.cover,
        stance_order,
        stance_part,
    })
}

fn string_list(value: &toml::Value, what: &str) -> Result<Vec<String>, String> {
    let rows = value.as_array().ok_or_else(|| format!("{what} must be an array"))?;
    if rows.is_empty() {
        return Err(format!("{what} is empty"));
    }
    let mut out = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let text = row
            .as_str()
            .filter(|text| !text.is_empty())
            .ok_or_else(|| format!("{what}[{index}] must be a string"))?;
        out.push(text.to_string());
    }
    Ok(out)
}

fn validate_stance_map(
    order: &[String],
    parts: &BTreeMap<String, String>,
    surfaces: &[crate::scenery::Surface],
    cover: &[crate::scenery::Cover],
) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for name in order {
        if !seen.insert(name.clone()) {
            return Err(format!("stance \"{name}\" is duplicated"));
        }
        if !stance_supported(name) {
            return Err(format!("stance \"{name}\" has no test"));
        }
        if !parts.contains_key(name) {
            return Err(format!("stance \"{name}\" has no part"));
        }
    }
    for name in parts.keys() {
        if !order.iter().any(|row| row == name) {
            return Err(format!("stance \"{name}\" is not in stance_order"));
        }
    }
    for part in parts.values() {
        if !surfaces.iter().any(|row| row.part == *part) {
            return Err(format!("stance part \"{part}\" is not a surface part"));
        }
    }
    for surface in surfaces {
        if !parts.values().any(|part| part == &surface.part) {
            return Err(format!("surface part \"{}\" has no stance", surface.part));
        }
    }
    let cover_part = parts.get("cover").ok_or_else(|| "stance \"cover\" has no part".to_string())?;
    for row in cover {
        if row.surface != *cover_part {
            return Err(format!(
                "cover {} surface \"{}\" does not match stance part \"{cover_part}\"",
                row.id, row.surface
            ));
        }
    }
    Ok(())
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

pub(crate) fn expect_keys(table: &toml::map::Map<String, toml::Value>, allowed: &[&str], what: &str) -> Result<(), String> {
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

pub(crate) fn num(value: &toml::Value, what: &str) -> Result<f32, String> {
    match value {
        toml::Value::Float(n) => Ok(*n as f32),
        toml::Value::Integer(n) => Ok(*n as f32),
        _ => Err(format!("{what} must be a number")),
    }
}

pub(crate) fn nums2(value: &toml::Value, what: &str) -> Result<[f32; 2], String> {
    let arr = value.as_array().ok_or_else(|| format!("{what} must be an array"))?;
    if arr.len() != 2 {
        return Err(format!("{what} must have 2 numbers"));
    }
    Ok([num(&arr[0], what)?, num(&arr[1], what)?])
}

pub(crate) fn nums3(value: &toml::Value, what: &str) -> Result<[f32; 3], String> {
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

/// A palette color that declares whether it is emissive. The name is not the flag.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LitKey {
    pub hex: String,
    pub emissive: bool,
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
    pub spitter_rim: String,
    pub hero_plate: String,
    pub portrait_field: String,
    pub arena_sky: String,
    pub hero_vex_primary: String,
    pub hero_vex_secondary: String,
    pub hero_vex_accent: String,
    pub hero_vex_glow: String,
    pub hero_nyra_primary: String,
    pub hero_nyra_secondary: String,
    pub hero_nyra_accent: String,
    pub hero_nyra_glow: String,
    pub hero_brakk_primary: String,
    pub hero_brakk_secondary: String,
    pub hero_brakk_accent: String,
    pub hero_brakk_glow: String,
    pub hero_kip_primary: String,
    pub hero_kip_secondary: String,
    pub hero_kip_accent: String,
    pub hero_kip_glow: String,
    pub gear_vex_gun_primary: String,
    pub gear_vex_gun_secondary: String,
    pub gear_vex_gun_accent: String,
    pub gear_nyra_gun_primary: String,
    pub gear_nyra_gun_secondary: String,
    pub gear_nyra_gun_accent: String,
    pub gear_nyra_gun_glow: LitKey,
    pub gear_brakk_gun_primary: String,
    pub gear_brakk_gun_secondary: String,
    pub gear_brakk_gun_glow: LitKey,
    pub gear_kip_gun_primary: String,
    pub gear_kip_gun_secondary: String,
    pub gear_kip_gun_accent: String,
    pub gear_brakk_shield_primary: String,
    pub gear_brakk_shield_secondary: String,
    pub gear_brakk_shield_accent: String,
    pub dirt_mauve: String,
    pub dirt_outer: String,
    pub rim_violet: String,
    pub rim_violet_hi: String,
    pub boulder_violet: String,
    pub platform_pale: String,
    pub platform_navy: String,
    pub trim_cyan: String,
    pub nest_brass: String,
    pub nest_goo: String,
    pub beacon_cyan: String,
    pub rubble_violet: String,
    pub crack_dark: String,
    pub vein_resin: String,
    pub vent_tell: String,
    pub flora_stalk: String,
    pub flora_bulb: String,
    pub fog_violet: String,
    pub sky_haze: String,
    pub key_warm: String,
    pub mark_ink: String,
    pub fill_cool: String,
    pub ambient_haze: String,
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
            "spitter_rim" => self.keys.spitter_rim.as_str(),
            "hero_plate" => self.keys.hero_plate.as_str(),
            "portrait_field" => self.keys.portrait_field.as_str(),
            "arena_sky" => self.keys.arena_sky.as_str(),
            "hero_vex_primary" => self.keys.hero_vex_primary.as_str(),
            "hero_vex_secondary" => self.keys.hero_vex_secondary.as_str(),
            "hero_vex_accent" => self.keys.hero_vex_accent.as_str(),
            "hero_vex_glow" => self.keys.hero_vex_glow.as_str(),
            "hero_nyra_primary" => self.keys.hero_nyra_primary.as_str(),
            "hero_nyra_secondary" => self.keys.hero_nyra_secondary.as_str(),
            "hero_nyra_accent" => self.keys.hero_nyra_accent.as_str(),
            "hero_nyra_glow" => self.keys.hero_nyra_glow.as_str(),
            "hero_brakk_primary" => self.keys.hero_brakk_primary.as_str(),
            "hero_brakk_secondary" => self.keys.hero_brakk_secondary.as_str(),
            "hero_brakk_accent" => self.keys.hero_brakk_accent.as_str(),
            "hero_brakk_glow" => self.keys.hero_brakk_glow.as_str(),
            "hero_kip_primary" => self.keys.hero_kip_primary.as_str(),
            "hero_kip_secondary" => self.keys.hero_kip_secondary.as_str(),
            "hero_kip_accent" => self.keys.hero_kip_accent.as_str(),
            "hero_kip_glow" => self.keys.hero_kip_glow.as_str(),
            "gear_vex_gun_primary" => self.keys.gear_vex_gun_primary.as_str(),
            "gear_vex_gun_secondary" => self.keys.gear_vex_gun_secondary.as_str(),
            "gear_vex_gun_accent" => self.keys.gear_vex_gun_accent.as_str(),
            "gear_nyra_gun_primary" => self.keys.gear_nyra_gun_primary.as_str(),
            "gear_nyra_gun_secondary" => self.keys.gear_nyra_gun_secondary.as_str(),
            "gear_nyra_gun_accent" => self.keys.gear_nyra_gun_accent.as_str(),
            "gear_nyra_gun_glow" => self.keys.gear_nyra_gun_glow.hex.as_str(),
            "gear_brakk_gun_primary" => self.keys.gear_brakk_gun_primary.as_str(),
            "gear_brakk_gun_secondary" => self.keys.gear_brakk_gun_secondary.as_str(),
            "gear_brakk_gun_glow" => self.keys.gear_brakk_gun_glow.hex.as_str(),
            "gear_kip_gun_primary" => self.keys.gear_kip_gun_primary.as_str(),
            "gear_kip_gun_secondary" => self.keys.gear_kip_gun_secondary.as_str(),
            "gear_kip_gun_accent" => self.keys.gear_kip_gun_accent.as_str(),
            "gear_brakk_shield_primary" => self.keys.gear_brakk_shield_primary.as_str(),
            "gear_brakk_shield_secondary" => self.keys.gear_brakk_shield_secondary.as_str(),
            "gear_brakk_shield_accent" => self.keys.gear_brakk_shield_accent.as_str(),
            "dirt_mauve" => self.keys.dirt_mauve.as_str(),
            "dirt_outer" => self.keys.dirt_outer.as_str(),
            "rim_violet" => self.keys.rim_violet.as_str(),
            "rim_violet_hi" => self.keys.rim_violet_hi.as_str(),
            "boulder_violet" => self.keys.boulder_violet.as_str(),
            "platform_pale" => self.keys.platform_pale.as_str(),
            "platform_navy" => self.keys.platform_navy.as_str(),
            "trim_cyan" => self.keys.trim_cyan.as_str(),
            "nest_brass" => self.keys.nest_brass.as_str(),
            "nest_goo" => self.keys.nest_goo.as_str(),
            "beacon_cyan" => self.keys.beacon_cyan.as_str(),
            "rubble_violet" => self.keys.rubble_violet.as_str(),
            "crack_dark" => self.keys.crack_dark.as_str(),
            "vein_resin" => self.keys.vein_resin.as_str(),
            "vent_tell" => self.keys.vent_tell.as_str(),
            "flora_stalk" => self.keys.flora_stalk.as_str(),
            "flora_bulb" => self.keys.flora_bulb.as_str(),
            "fog_violet" => self.keys.fog_violet.as_str(),
            "sky_haze" => self.keys.sky_haze.as_str(),
            "key_warm" => self.keys.key_warm.as_str(),
            "mark_ink" => self.keys.mark_ink.as_str(),
            "fill_cool" => self.keys.fill_cool.as_str(),
            "ambient_haze" => self.keys.ambient_haze.as_str(),
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

    /// True only when that entry's `emissive` field is true. A `_glow` suffix is not enough.
    /// An unknown name panics, the same way `key` does.
    pub fn emissive(&self, name: &str) -> bool {
        if self.hex(name).is_none() {
            panic!("unknown palette key {name}");
        }
        match name {
            "gear_nyra_gun_glow" => self.keys.gear_nyra_gun_glow.emissive,
            "gear_brakk_gun_glow" => self.keys.gear_brakk_gun_glow.emissive,
            _ => false,
        }
    }
}

/// Mask slot. `primary` is 0, `secondary` is 1, `accent` is 2, `fourth` is 3.
pub fn channel_index(name: &str) -> Option<usize> {
    match name {
        "primary" => Some(0),
        "secondary" => Some(1),
        "accent" => Some(2),
        "fourth" => Some(3),
        _ => None,
    }
}

impl CastLook {
    /// Mask slot for `overdrive_emissive_channel`. Load rejects any other name.
    pub fn overdrive_channel_index(&self) -> usize {
        channel_index(&self.overdrive_emissive_channel).unwrap_or_else(|| {
            panic!(
                "cast.toml overdrive_emissive_channel \"{}\" is unknown",
                self.overdrive_emissive_channel
            )
        })
    }
}

fn validate_lit_keys(palette: &Palette) -> Result<(), String> {
    for (name, lit) in [
        ("gear_nyra_gun_glow", &palette.keys.gear_nyra_gun_glow),
        ("gear_brakk_gun_glow", &palette.keys.gear_brakk_gun_glow),
    ] {
        if lit.hex.trim().trim_start_matches('#').len() < 6 {
            return Err(format!("{name} hex does not resolve"));
        }
        if !lit.emissive {
            return Err(format!("{name} emissive must be true"));
        }
    }
    Ok(())
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
    pub grid: CrateGrid,
    pub hint: RevealHint,
    pub card: CardTitle,
    pub icon: CrateIcon,
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
    pub divider_blend: f32,
    pub gem_edge_blend: f32,
    pub crack_glow_blend: f32,
    /// Card divider width is `face * rule_num / rule_den`.
    pub rule_num: u32,
    pub rule_den: u32,
    /// Dropped crate width is the frame width divided by this.
    pub crate_w_div: u32,
    /// Smallest dropped crate, in pixels.
    pub crate_w_min: u32,
    /// Crack length as a fraction of the crate width, times the crack amount.
    pub crack_reach: f32,
    /// Crack origin as a fraction of the crate height.
    pub crack_mid: f32,
    /// Core of a crack, mixed from the tier color toward the light.
    pub crack_hot_blend: f32,
    /// `(dx, dy)` pairs. Each ray is drawn both ways from the crate midline.
    pub crack_rays: Vec<[f32; 2]>,
    /// Pixels kept between a failure line and the bottom of the frame.
    pub failure_clear: f32,
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
    for (name, blend) in [
        ("divider_blend", layout.divider_blend),
        ("gem_edge_blend", layout.gem_edge_blend),
        ("crack_glow_blend", layout.crack_glow_blend),
    ] {
        if !(blend > 0.0 && blend <= 1.0) {
            return Err(format!("crate_reveal.toml layout {name} must be in (0, 1]"));
        }
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
    if layout.rule_den == 0 || layout.rule_num == 0 || layout.rule_num > layout.rule_den {
        return Err("crate_reveal.toml layout rule_num / rule_den must be a fraction in (0, 1]".into());
    }
    if layout.crate_w_div == 0 || layout.crate_w_min == 0 {
        return Err("crate_reveal.toml layout crate_w_div and crate_w_min must be > 0".into());
    }
    if !(layout.crack_reach > 0.0 && layout.crack_reach <= 1.0) {
        return Err("crate_reveal.toml layout crack_reach must be in (0, 1]".into());
    }
    if !(layout.crack_mid > 0.0 && layout.crack_mid < 1.0) {
        return Err("crate_reveal.toml layout crack_mid must be in (0, 1)".into());
    }
    if !(layout.crack_hot_blend > 0.0 && layout.crack_hot_blend <= 1.0) {
        return Err("crate_reveal.toml layout crack_hot_blend must be in (0, 1]".into());
    }
    if layout.crack_rays.is_empty() {
        return Err("crate_reveal.toml layout crack_rays is empty".into());
    }
    for ray in &layout.crack_rays {
        if !ray[0].is_finite() || !ray[1].is_finite() {
            return Err("crate_reveal.toml layout crack_rays must be finite".into());
        }
    }
    if !(layout.failure_clear > 0.0) {
        return Err("crate_reveal.toml layout failure_clear must be > 0".into());
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
    pub rank: u32,
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

#[derive(Clone, Debug, Deserialize)]
pub struct CrateGrid {
    pub fast_flip: f32,
    pub columns: u32,
    pub rows: u32,
    pub gap: f32,
    pub card_w: f32,
    pub card_h: f32,
    pub back: String,
    pub frame: String,
    pub face: String,
    pub refund_message: String,
    pub refund_w: f32,
    pub refund_h: f32,
    pub refund_face: String,
    pub refund_ink: String,
    pub refund_tick: String,
    pub flip_cue: String,
    pub pull_failed: String,
    pub failure_ink: String,
    pub display: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CrateIcon {
    pub width_num: u32,
    pub width_den: u32,
    pub height_num: u32,
    pub height_den: u32,
    pub highlight_div: u32,
    pub highlight_blend: f32,
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
    /// Slots that stay available for hits after every warning instance and every crowd loop.
    pub hit_reserve: usize,
    /// Seconds a replaced warning voice spends fading out.
    pub warning_fade: f32,
    /// Seconds a stolen loop fades back in when a voice frees.
    pub loop_resume_fade: f32,
    pub crowd_radius: f32,
    pub crowd_full_count: u32,
    pub min_distance: f32,
    #[serde(default)]
    pub sfx: Vec<SfxDef>,
    #[serde(default)]
    pub sting: Vec<StingDef>,
    #[serde(default)]
    pub pitch: Vec<PitchDef>,
    #[serde(default)]
    pub footstep: Footsteps,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Footsteps {
    pub stride: f32,
    pub min_speed: f32,
    pub remote_priority: i32,
    /// Shared cap for every player step row, own and teammate.
    pub max_voices: u32,
    pub rows: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SfxDef {
    pub id: String,
    /// Mix-table key that owns `max_instances`. Aliases share the source row.
    pub row_id: String,
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
    /// A full voice list may stop this voice to make room.
    /// The mix row is the only source. A warning may take a stealable prop loop.
    pub stealable: bool,
    /// Telegraphs and other cues that must play out. Loading rejects a warning that is stealable.
    pub is_warning: bool,
    /// True when the cue is not positioned in the world.
    pub flat: bool,
    /// Higher values stay in the mix. Voice picking reads this. It is not a category if-chain.
    #[serde(default)]
    pub priority: i32,
    /// Mix row category. `category_ui` stays the ui flag.
    #[serde(default)]
    pub category: String,
    /// UI bus cues that do not take a `voice_cap` slot. `load_mix` sets this from the row.
    pub outside_voice_cap: bool,
}

fn require_emitted_game_voices(
    audio: &AudioTable,
    bugs: &crate::bugs::BugBook,
    bosses: &[BossDef],
    heroes: &[HeroCard],
    ui: &Ui,
) -> Result<(), String> {
    let mut ids = Vec::new();
    let mut push = |what: &str, id: &str| -> Result<(), String> {
        if id.is_empty() {
            return Err(format!("{what} voice id is empty"));
        }
        ids.push(id.to_string());
        Ok(())
    };
    for (name, row) in bugs.rows() {
        push(&format!("{name} footstep"), &row.audio.footstep)?;
    }
    let spitter = bugs.of(EnemyKind::Spitter);
    push("spitter charge", &spitter.audio.charge)?;
    push("spitter fire", &spitter.audio.fire)?;
    push("spitter sac_pop", &spitter.audio.sac_pop)?;
    let brute = bugs.of(EnemyKind::Brute);
    push("brute attack", &brute.audio.attack)?;
    push("brute shell_open", &brute.audio.shell_open)?;
    push("brood_mother attack", &bugs.of(EnemyKind::BroodMother).audio.attack)?;
    push("hive_tyrant attack", &bugs.of(EnemyKind::HiveTyrant).audio.attack)?;
    push("weakpoint_hit", &bugs.feedback.weakpoint_hit)?;
    for hero in heroes {
        for suffix in ["fire", "reload", "ability", "ability_end"] {
            push(&format!("{} {suffix}", hero.id), &format!("{}_{suffix}", hero.id))?;
        }
    }
    for boss in bosses {
        for (field, sfx) in [
            ("intro_sfx", &boss.intro_sfx),
            ("death_sfx", &boss.death_sfx),
            ("fizzle_sfx", &boss.fizzle_sfx),
            ("charge_sfx", &boss.charge_sfx),
            ("hatch_sfx", &boss.hatch_sfx),
            ("acid_sfx", &boss.acid_sfx),
            ("scythe_sfx", &boss.scythe_sfx),
        ] {
            if sfx.is_empty() {
                continue;
            }
            push(&format!("{} {field}", boss.id), sfx)?;
        }
    }
    for (slot, id) in ui.sounds.ids() {
        push(&format!("ui {slot}"), id)?;
    }
    crate::audio_mix::require_emitted_voices(audio, &ids)
}

fn validate_arena_audio(arena: &Arena, audio: &AudioTable) -> Result<(), String> {
    if !(audio.footstep.stride > 0.0) {
        return Err("footstep stride must be > 0".into());
    }
    if !(audio.footstep.min_speed > 0.0) {
        return Err("footstep min_speed must be > 0".into());
    }
    if audio.footstep.max_voices == 0 {
        return Err("footstep max_voices must be > 0".into());
    }
    for kind in &arena.scenery.surface_kinds {
        let Some(rows) = audio.footstep.rows.get(kind) else {
            return Err(format!("footstep.rows missing {kind}"));
        };
        if !(3..=4).contains(&rows.len()) {
            return Err(format!("footstep.rows {kind} needs 3 or 4 variations"));
        }
        for id in rows {
            let def = audio
                .sfx
                .iter()
                .find(|row| row.id == *id)
                .ok_or_else(|| format!("footstep.rows {kind} names missing file {id}"))?;
            if def.category != "player" || !def.stealable || def.flat || def.looped || (def.pitch_semitones - 1.0).abs() > 1e-4 {
                return Err(format!(
                    "footstep {id} must be a stealable spatial player one-shot with pitch_rand_semitones = 1"
                ));
            }
        }
    }
    for kind in audio.footstep.rows.keys() {
        if !arena.scenery.surface_kinds.iter().any(|row| row == kind) {
            return Err(format!("footstep.rows has extra kind {kind}"));
        }
    }
    for surface in &arena.surfaces {
        if !audio.footstep.rows.contains_key(&surface.kind) {
            return Err(format!("surface {} kind \"{}\" has no step row", surface.id, surface.kind));
        }
    }
    let bed = audio
        .sfx
        .iter()
        .find(|row| row.id == "arena_01_bed")
        .ok_or_else(|| "arena_01_bed is missing".to_string())?;
    if bed.category != "music" || !bed.looped || !bed.flat {
        return Err("arena_01_bed must be a flat looping music row".into());
    }
    for id in ["hive_nest_idle", "relay_tower_hum"] {
        let row = audio
            .sfx
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| format!("{id} is missing"))?;
        if !row.looped || row.category != "prop" || row.flat || row.max_instances == 0 || !row.stealable {
            return Err(format!(
                "{id} must be a stealable spatial prop loop with max_instances > 0"
            ));
        }
    }
    let pulse = audio
        .sfx
        .iter()
        .find(|row| row.id == "beacon_pulse")
        .ok_or_else(|| "beacon_pulse is missing".to_string())?;
    if pulse.looped || pulse.category != "prop" || pulse.flat || pulse.max_instances == 0 {
        return Err("beacon_pulse must be a spatial prop one-shot with max_instances > 0".into());
    }
    Ok(())
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

/// Per-wave pay for a round the player was in at the start. Flat after `flat_after_wave`.
pub fn wave_payout(currency: &Currency, wave: u32, tier_currency: f32) -> f32 {
    if wave == 0 || wave > currency.flat_after_wave {
        0.0
    } else {
        currency.per_wave * tier_currency
    }
}

/// Milestone pay. It starts after `flat_after_wave`, every `endless_milestone_every` waves.
pub fn milestone_payout(currency: &Currency, wave: u32, tier_currency: f32) -> f32 {
    if wave <= currency.flat_after_wave {
        return 0.0;
    }
    if wave % currency.endless_milestone_every == 0 {
        currency.endless_milestone_bonus * tier_currency
    } else {
        0.0
    }
}

/// Win bonus. A quit, a disconnect, and a defeat do not take it.
pub fn win_payout(currency: &Currency, tier_currency: f32) -> f32 {
    currency.match_complete_bonus * tier_currency
}

/// Currency the server's match record will recognize for waves `1..=wave_reached`.
pub fn currency_reward(
    currency: &Currency,
    wave_reached: u32,
    tier_currency: f32,
    completed: bool,
    boss_kills: u32,
) -> f32 {
    let mut total = 0.0;
    let mut wave = 1u32;
    while wave <= wave_reached {
        total += wave_payout(currency, wave, tier_currency);
        total += milestone_payout(currency, wave, tier_currency);
        wave += 1;
    }
    total += boss_kills as f32 * currency.boss_kill_bonus;
    if completed {
        total += win_payout(currency, tier_currency);
    }
    total
}

fn validate_currency(currency: &Currency) -> Result<(), String> {
    for (name, value) in [
        ("per_wave", currency.per_wave),
        ("endless_milestone_bonus", currency.endless_milestone_bonus),
        ("match_complete_bonus", currency.match_complete_bonus),
        ("boss_kill_bonus", currency.boss_kill_bonus),
        ("per_round_cap_mult", currency.per_round_cap_mult),
    ] {
        if !(value.is_finite() && value >= 0.0) {
            return Err(format!("currency.toml {name} must be >= 0"));
        }
    }
    if currency.flat_after_wave == 0 {
        return Err("currency.toml flat_after_wave must be > 0".into());
    }
    if currency.endless_milestone_every == 0 {
        return Err("currency.toml endless_milestone_every must be > 0".into());
    }
    Ok(())
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
        ' ' | 'A'..='Z' | '0'..='9' | '+' | '-' | '%' | ':' | '.' | ',' | '/' | ';' | '·'
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

/// `NEED <n> MORE CREDITS`. `missing` is the shortfall, not the sticker price.
pub fn shortfall_line(copy: &UiCopy, places: u32, missing: f32) -> String {
    format!(
        "{} {} {} {}",
        copy.pull_need,
        format_fixed(missing.max(0.0), places),
        copy.pull_more,
        copy.pull_credits
    )
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
    pub bank: UiBank,
}

impl Ui {
    pub fn color_keys(&self) -> [(&str, &str); 15] {
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
            ("ui.colors.bevel_light", self.colors.bevel_light.as_str()),
            ("ui.colors.bevel_shadow", self.colors.bevel_shadow.as_str()),
            ("ui.colors.veil", self.colors.veil.as_str()),
            ("ui.colors.bank_count", self.colors.bank_count.as_str()),
            ("ui.colors.bank_bonus", self.colors.bank_bonus.as_str()),
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
    /// Lit edge of the pause bevel.
    pub bevel_light: String,
    /// Shadow edge of the pause bevel.
    pub bevel_shadow: String,
    /// Co-op pause veil. The match shows through at `layout.coop_pause_dim`.
    pub veil: String,
    /// Count-up digits. This key is not legendary gold.
    pub bank_count: String,
    /// Bonus line. This key is not legendary gold.
    pub bank_bonus: String,
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
    pub crates_ten: String,
    pub crates_open: String,
    pub crates_continue: String,
    pub crates_pull_again: String,
    pub pull_failed: String,
    pub connection_lost: String,
    pub pull_need: String,
    pub pull_more: String,
    pub pull_have: String,
    pub pull_credits: String,
    pub pull_needs_server: String,
    pub pull_no_shards: String,
    pub bank_earned: String,
    pub bank_bonus: String,
    pub bank_quit: String,
    pub bank_disconnect: String,
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
    /// Chroma floor before a locked portrait pixel can count as the accent.
    pub locked_accent_chroma: f32,
    /// Value floor before a locked portrait pixel can count as the accent.
    pub locked_accent_value: f32,
    /// Hue window, in degrees, around the UI accent.
    pub locked_hue_window: f32,
    /// Non-accent portrait pixels desaturate this far on a locked card. 1 is fully gray.
    pub locked_body_desaturation: f32,
    pub card_pad: f32,
    pub pause_keep: f32,
    /// Upgrade-card fill alpha. The RGB comes from `colors.panel`.
    pub panel_alpha: f32,
    pub pause_panel_width: f32,
    pub pause_panel_height: f32,
    /// Corner radius of the pause panel, in 720p pixels.
    pub panel_radius: f32,
    /// Corner radius of a pause button, in 720p pixels.
    pub button_radius: f32,
    /// Width of the lit and shadowed bevel inside the pause outline.
    pub bevel_px: f32,
    /// Bold outline around the pause panel and its buttons.
    pub pause_outline_px: f32,
    /// Alpha of the co-op pause veil. The live match stays visible underneath.
    pub coop_pause_dim: f32,
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
    pub bank_panel_width: f32,
    pub bank_panel_height: f32,
}

/// Match-end count-up. Both times come from `ui.toml`.
#[derive(Clone, Debug, Deserialize)]
pub struct UiBank {
    pub count_secs: f32,
    pub count_tick_secs: f32,
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
    pub count_tick: String,
    pub bank_chime: String,
}

impl UiSounds {
    pub fn ids(&self) -> [(&str, &str); 13] {
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
            ("count_tick", self.count_tick.as_str()),
            ("bank_chime", self.bank_chime.as_str()),
        ]
    }
}

fn load_ui(text: &str) -> Result<Ui, String> {
    parse("ui.toml", text)
}

fn load_banners(text: &str) -> Result<Banners, String> {
    let mut value: toml::Value = toml::from_str(text).map_err(|err| format!("banners.toml: {err}"))?;
    let cost = value
        .get_mut("cost")
        .and_then(|item| item.as_table_mut())
        .ok_or("banners.toml is missing cost")?;
    if cost.get("single").is_none() {
        return Err("banners.toml is missing cost.single".into());
    }
    if cost.get("pods").is_none() {
        return Err("banners.toml is missing cost.pods".into());
    }
    if cost.get("ten_pull_cost").is_none() {
        return Err("banners.toml is missing cost.ten_pull_cost".into());
    }
    let saved = value
        .get("saved")
        .and_then(|item| item.as_table())
        .ok_or("banners.toml is missing saved")?;
    if saved.get("max_keys").is_none() {
        return Err("banners.toml is missing saved.max_keys".into());
    }
    if value.get("item").and_then(|row| row.as_array()).is_none() {
        return Err("banners.toml is missing item".into());
    }
    let banners: Banners = value.try_into().map_err(|err| format!("banners.toml: {err}"))?;
    validate_banners(&banners)?;
    Ok(banners)
}

fn validate_items(banners: &Banners) -> Result<(), String> {
    if banners.items.is_empty() {
        return Err("banners.toml is missing item".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for item in &banners.items {
        if item.id.is_empty() || !seen.insert(item.id.clone()) {
            return Err(format!("banners.toml item id \"{}\" is missing or repeated", item.id));
        }
        if item.name.is_empty() {
            return Err(format!("banners.toml item \"{}\" is missing name", item.id));
        }
        require_font(&item.name, &format!("banners.toml item \"{}\"", item.id))?;
        if !matches!(item.rarity.as_str(), "common" | "rare" | "epic" | "legendary") {
            return Err(format!("banners.toml item \"{}\" has unknown rarity {}", item.id, item.rarity));
        }
        if item.ink.is_empty() || item.ink.starts_with("rarity_") {
            return Err(format!("banners.toml item \"{}\" ink must be set and must not be a rarity color", item.id));
        }
        if item.icon.is_empty() {
            return Err(format!("banners.toml item \"{}\" is missing icon", item.id));
        }
        let width = item.icon[0].chars().count();
        if width == 0 {
            return Err(format!("banners.toml item \"{}\" icon is empty", item.id));
        }
        let mut inked = false;
        for row in &item.icon {
            if row.chars().count() != width {
                return Err(format!("banners.toml item \"{}\" icon rows differ in width", item.id));
            }
            for ch in row.chars() {
                match ch {
                    '#' => inked = true,
                    '.' => {}
                    other => return Err(format!("banners.toml item \"{}\" icon has '{other}'", item.id)),
                }
            }
        }
        if !inked {
            return Err(format!("banners.toml item \"{}\" icon has no ink", item.id));
        }
    }
    for rarity in ["common", "rare", "epic", "legendary"] {
        if !banners.items.iter().any(|item| item.rarity == rarity) {
            return Err(format!("banners.toml has no item of rarity {rarity}"));
        }
    }
    Ok(())
}

fn validate_item_inks(banners: &Banners, palette: &Palette, reveal: &CrateReveal) -> Result<(), String> {
    for item in &banners.items {
        if palette.hex(&item.ink).is_none() {
            return Err(format!("banners.toml item \"{}\" ink \"{}\" is not a palette key", item.id, item.ink));
        }
    }
    let glyph = reveal.layout.title_glyph_px.max(1) as f32;
    let longest = banners.items.iter().map(|item| item.name.chars().count()).max().unwrap_or(1) as f32;
    let scale = reveal.layout.title_scale_min.max(1) as f32;
    let grid = &reveal.grid;
    let band = (reveal.layout.border_px as f32) * 2.0;
    let face = grid.card_w - band;
    if longest * glyph * scale > face {
        return Err(format!("banners.toml item names do not fit a {}px card", grid.card_w));
    }
    Ok(())
}

fn validate_banners(banners: &Banners) -> Result<(), String> {
    if banners.id.is_empty() {
        return Err("banners.toml id must be set".into());
    }
    if banners.cost.single == 0 || banners.cost.pods == 0 || banners.cost.ten_pull_cost == 0 {
        return Err("banners.toml costs must be > 0".into());
    }
    if banners.saved.max_keys == 0 {
        return Err("banners.toml saved.max_keys must be > 0".into());
    }
    validate_items(banners)?;
    let grant = banners.dev.starting_credits;
    if !(grant.is_finite() && grant + 1e-3 >= banners.cost.ten_pull_cost as f32) {
        return Err("banners.toml dev.starting_credits must cover cost.ten_pull_cost".into());
    }
    Ok(())
}

fn validate_ten_grid(reveal: &CrateReveal, banners: &Banners, ui: &Ui, audio: &AudioTable, palette: &Palette) -> Result<(), String> {
    let multi = &reveal.multi_pull;
    if !(multi.pod_interval > 0.0) {
        return Err("crate_reveal.toml multi_pull.pod_interval must be > 0".into());
    }
    if multi.full_sequence_for != "highest_rarity" {
        return Err("crate_reveal.toml multi_pull.full_sequence_for must be highest_rarity".into());
    }
    if !multi.summary_grid {
        return Err("crate_reveal.toml multi_pull.summary_grid must be true".into());
    }
    let ranks = [
        ("common", reveal.common.rank),
        ("rare", reveal.rare.rank),
        ("epic", reveal.epic.rank),
        ("legendary", reveal.legendary.rank),
    ];
    let mut previous = 0u32;
    for (index, (name, rank)) in ranks.iter().enumerate() {
        if *rank == 0 {
            return Err(format!("crate_reveal.toml {name}.rank must be > 0"));
        }
        if index > 0 && *rank <= previous {
            return Err(format!("crate_reveal.toml {name}.rank must be above the previous tier"));
        }
        previous = *rank;
    }
    let grid = &reveal.grid;
    if !(grid.fast_flip > 0.0) {
        return Err("crate_reveal.toml grid.fast_flip must be > 0".into());
    }
    if grid.columns == 0 || grid.rows == 0 {
        return Err("crate_reveal.toml grid columns and rows must be > 0".into());
    }
    let cells = grid
        .columns
        .checked_mul(grid.rows)
        .ok_or("crate_reveal.toml grid columns * rows overflows")?;
    if cells != banners.cost.pods {
        return Err(format!(
            "crate_reveal.toml grid is {cells} cards and banners.toml cost.pods is {}",
            banners.cost.pods
        ));
    }
    if !(grid.gap >= 0.0) || !(grid.card_w > 0.0) || !(grid.card_h > 0.0) {
        return Err("crate_reveal.toml grid card size must be positive and gap must be >= 0".into());
    }
    if !(grid.refund_w > 0.0) || !(grid.refund_h > 0.0) {
        return Err("crate_reveal.toml grid refund panel must be positive".into());
    }
    if grid.refund_message.is_empty() {
        return Err("crate_reveal.toml grid.refund_message must be set".into());
    }
    validate_item_inks(banners, palette, reveal)?;
    if grid.display != "roll" {
        return Err(format!("crate_reveal.toml grid.display {} is not roll", grid.display));
    }
    let grid_w = grid.columns as f32 * grid.card_w + (grid.columns - 1) as f32 * grid.gap;
    let grid_h = grid.rows as f32 * grid.card_h + (grid.rows - 1) as f32 * grid.gap;
    if grid_w > ui.layout.fit_width || grid_h > ui.layout.fit_height {
        return Err("crate_reveal.toml grid does not fit the ui fit frame".into());
    }
    let border = (reveal.layout.border_px as f32 * grid.card_h / reveal.layout.panel_height).round();
    if border < 1.0 || border * 2.0 >= grid.card_w || border * 2.0 >= grid.card_h {
        return Err("crate_reveal.toml grid card border does not leave a face".into());
    }
    let face_w = grid.card_w - border * 2.0;
    let legend = "LEGENDARY".chars().count() as f32 * reveal.layout.title_glyph_px as f32 * reveal.layout.title_scale_min as f32;
    if face_w <= legend {
        return Err("crate_reveal.toml grid card is too narrow for LEGENDARY".into());
    }
    let refund_border = (reveal.layout.border_px as f32 * grid.refund_h / reveal.layout.panel_height).round();
    if refund_border < 1.0 || refund_border * 2.0 >= grid.refund_w || refund_border * 2.0 >= grid.refund_h {
        return Err("crate_reveal.toml grid refund border does not leave a face".into());
    }
    let refund_face_w = grid.refund_w - refund_border * 2.0;
    let message_w = grid.refund_message.chars().count() as f32 * reveal.layout.title_glyph_px as f32 * reveal.layout.title_scale_min as f32;
    if refund_face_w <= message_w {
        return Err("crate_reveal.toml grid.refund_message does not fit the refund panel".into());
    }
    for (name, key) in [
        ("back", grid.back.as_str()),
        ("frame", grid.frame.as_str()),
        ("face", grid.face.as_str()),
        ("refund_face", grid.refund_face.as_str()),
        ("refund_ink", grid.refund_ink.as_str()),
    ] {
        if key.starts_with("rarity_") {
            return Err(format!("crate_reveal.toml grid.{name} must not be a rarity color"));
        }
        if palette.hex(key).is_none() {
            return Err(format!("crate_reveal.toml grid.{name} color \"{key}\" does not resolve"));
        }
    }
    let flip = audio
        .sfx
        .iter()
        .find(|row| row.id == grid.flip_cue)
        .ok_or_else(|| format!("crate_reveal.toml grid.flip_cue {} is missing from the mix", grid.flip_cue))?;
    if !flip.category_ui || !flip.outside_voice_cap || flip.max_instances != 1 {
        return Err(format!(
            "audio_mix.toml {} must be a UI bus row, outside_voice_cap, max_instances 1",
            grid.flip_cue
        ));
    }
    let tick = audio
        .sfx
        .iter()
        .find(|row| row.id == grid.refund_tick)
        .ok_or_else(|| format!("crate_reveal.toml grid.refund_tick {} is missing from the mix", grid.refund_tick))?;
    if !tick.category_ui {
        return Err(format!("audio_mix.toml {} must be a UI sound", grid.refund_tick));
    }
    if grid.refund_tick == ui.sounds.bank_chime || grid.refund_tick == grid.flip_cue {
        return Err("crate_reveal.toml grid.refund_tick must be a neutral UI tick".into());
    }
    let failed_cue = audio
        .sfx
        .iter()
        .find(|row| row.id == grid.pull_failed)
        .ok_or_else(|| format!("crate_reveal.toml grid.pull_failed {} is missing from the mix", grid.pull_failed))?;
    if !failed_cue.category_ui {
        return Err(format!("audio_mix.toml {} must be a UI sound", grid.pull_failed));
    }
    if grid.pull_failed == ui.sounds.bank_chime
        || grid.pull_failed == ui.sounds.count_tick
        || grid.pull_failed == grid.flip_cue
        || grid.pull_failed == grid.refund_tick
    {
        return Err("crate_reveal.toml grid.pull_failed must not be the chime, the flip, or the tick".into());
    }
    if grid.failure_ink != reveal.layout.ink_light {
        return Err("crate_reveal.toml grid.failure_ink must be the paper color".into());
    }
    if grid.failure_ink.starts_with("rarity_") || grid.failure_ink == ui.colors.defeat {
        return Err("crate_reveal.toml grid.failure_ink must not be a rarity or alarm color".into());
    }
    if palette.hex(&grid.failure_ink).is_none() {
        return Err(format!("crate_reveal.toml grid.failure_ink \"{}\" does not resolve", grid.failure_ink));
    }
    let icon = &reveal.icon;
    if icon.width_den == 0 || icon.height_den == 0 || icon.highlight_div == 0 {
        return Err("crate_reveal.toml icon divisors must be > 0".into());
    }
    if icon.width_num == 0 || icon.width_num > icon.width_den || icon.height_num == 0 || icon.height_num > icon.height_den {
        return Err("crate_reveal.toml icon box must be a positive fraction of the face".into());
    }
    if !(icon.highlight_blend > 0.0 && icon.highlight_blend <= 1.0) {
        return Err("crate_reveal.toml icon.highlight_blend must be in (0, 1]".into());
    }
    if audio.sting.iter().any(|row| row.id == grid.refund_tick || row.id == grid.flip_cue || row.id == grid.pull_failed) {
        return Err("crate_reveal.toml grid cues must not be crate stings".into());
    }
    let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    if assets.is_dir() {
        for row in [flip, tick, failed_cue] {
            if !assets.join(&row.path).is_file() {
                return Err(format!("{} file {} is missing", row.id, row.path));
            }
        }
    }
    Ok(())
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
    if !(ui.bank.count_secs > 0.0) || !(ui.bank.count_tick_secs > 0.0) {
        return Err("ui.toml bank count_secs and count_tick_secs must be > 0".into());
    }
    if ui.bank.count_secs < ui.bank.count_tick_secs {
        return Err("ui.toml bank count_secs must cover one count_tick_secs".into());
    }
    if ui.colors.bank_count == "rarity_legendary" || ui.colors.bank_bonus == "rarity_legendary" {
        return Err("ui.toml bank count and bonus must not use legendary gold".into());
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
        ("panel_radius", layout.panel_radius),
        ("button_radius", layout.button_radius),
        ("bevel_px", layout.bevel_px),
        ("pause_outline_px", layout.pause_outline_px),
        ("slider_width", layout.slider_width),
        ("slider_height", layout.slider_height),
        ("settings_row_height", layout.settings_row_height),
        ("fit_width", layout.fit_width),
        ("fit_height", layout.fit_height),
        ("bank_panel_width", layout.bank_panel_width),
        ("bank_panel_height", layout.bank_panel_height),
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
    if !(layout.coop_pause_dim > 0.0 && layout.coop_pause_dim < 1.0) {
        return Err("ui.toml layout.coop_pause_dim must be in (0, 1)".into());
    }
    if layout.panel_radius * 2.0 >= layout.pause_panel_width.min(layout.pause_panel_height) {
        return Err("ui.toml layout.panel_radius must leave a flat edge on the pause panel".into());
    }
    if layout.button_radius * 2.0 >= layout.button_width.min(layout.button_height) {
        return Err("ui.toml layout.button_radius must leave a flat edge on a button".into());
    }
    if layout.pause_outline_px + layout.bevel_px >= layout.button_height * 0.5 {
        return Err("ui.toml pause outline and bevel must fit inside a button".into());
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
    if !(layout.locked_accent_chroma >= 1.0 && layout.locked_accent_chroma <= 64.0) {
        return Err("ui.toml layout.locked_accent_chroma must be in [1, 64]".into());
    }
    if !(layout.locked_accent_value >= 1.0 && layout.locked_accent_value <= 64.0) {
        return Err("ui.toml layout.locked_accent_value must be in [1, 64]".into());
    }
    if !(layout.locked_hue_window > 0.0 && layout.locked_hue_window <= 45.0) {
        return Err("ui.toml layout.locked_hue_window must be in (0, 45]".into());
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
    for slot in ["count_tick", "bank_chime"] {
        let id = match slot {
            "count_tick" => ui.sounds.count_tick.as_str(),
            "bank_chime" => ui.sounds.bank_chime.as_str(),
            _ => return Err(format!("ui.toml sounds.{slot} is not a bank cue")),
        };
        let row = audio
            .sfx
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| format!("ui.toml sounds.{slot} \"{id}\" is missing from audio_mix.toml"))?;
        if !row.outside_voice_cap {
            return Err(format!("audio_mix.toml {id} must be outside the voice cap"));
        }
        if !row.stealable || row.looped || !row.flat {
            return Err(format!("audio_mix.toml {id} must be a flat stealable one-shot"));
        }
    }
    let glyph_px = glyphs.title_glyph_px.max(1);
    let bank_inner = layout.bank_panel_width - layout.pause_outline_px * 2.0 - layout.bevel_px * 2.0;
    for label in [&ui.copy.bank_quit, &ui.copy.bank_disconnect, &ui.copy.victory, &ui.copy.defeat] {
        require_fit(label, layout.title_scale, glyph_px, bank_inner, "bank title")?;
    }
    let line_h = layout.button_scale as f32 * glyphs.glyph_rows as f32 + layout.spacing_block;
    let title_h = layout.title_scale as f32 * glyphs.glyph_rows as f32;
    let stack = layout.margin_y * 2.0
        + title_h
        + layout.title_gap
        + line_h * 6.0
        + layout.row_gap
        + layout.button_height * 2.0
        + layout.row_gap;
    if stack > layout.bank_panel_height {
        return Err(format!("ui.toml bank panel is {stack}px tall and bank_panel_height is {}", layout.bank_panel_height));
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
        &ui.copy.crates_ten,
        &ui.copy.crates_open,
        &ui.copy.crates_continue,
        &ui.copy.crates_pull_again,
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

#[cfg(test)]
mod cast_look_tests {
    use super::{load_cast_look, Content};

    #[test]
    fn overdrive_channel_must_name_a_mask_slot() {
        let palette = Content::embedded().palette;
        let text = include_str!("../../../content/cast.toml");
        let look = load_cast_look(text, &palette).expect("cast.toml");
        assert_eq!(look.overdrive_emissive_channel, "accent");
        assert_eq!(look.overdrive_channel_index(), 2);
        let missing = text.replacen("overdrive_emissive_channel = \"accent\"\n", "", 1);
        assert_ne!(missing, text);
        let err = load_cast_look(&missing, &palette).expect_err("a missing channel must fail");
        assert!(err.contains("overdrive_emissive_channel"), "{err}");
        let unknown = text.replacen(
            "overdrive_emissive_channel = \"accent\"",
            "overdrive_emissive_channel = \"glow\"",
            1,
        );
        let err = load_cast_look(&unknown, &palette).expect_err("an unknown channel must fail");
        assert!(err.contains("unknown"), "{err}");
        assert!((look.hit_surface_gap - 0.05).abs() < 1.0e-6, "hit_surface_gap is 5 cm");
        let missing = text.replacen("hit_surface_gap = 0.05\n", "", 1);
        assert_ne!(missing, text);
        let err = load_cast_look(&missing, &palette).expect_err("a missing gap must fail");
        assert!(err.contains("hit_surface_gap"), "{err}");
        let wide = text.replacen("hit_surface_gap = 0.05", "hit_surface_gap = 1.0", 1);
        let err = load_cast_look(&wide, &palette).expect_err("a meter-wide gap must fail");
        assert!(err.contains("hit_surface_gap"), "{err}");
    }
}

#[cfg(test)]
mod ten_pull_keys {
    use super::{embedded_sources, Content};

    #[test]
    fn ten_pull_price_follows_the_key_and_missing_keys_fail() {
        let src = embedded_sources();
        let stripped = src.banners.replace("ten_pull_cost = 1000\n", "");
        assert_ne!(stripped, src.banners);
        let mut sources = embedded_sources();
        sources.banners = &stripped;
        let err = Content::parse(&sources).expect_err("ten_pull_cost is required");
        assert!(err.contains("ten_pull_cost"), "{err}");

        let bumped = src.banners.replace("ten_pull_cost = 1000", "ten_pull_cost = 1100");
        sources.banners = &bumped;
        let content = Content::parse(&sources).expect("an explicit ten_pull_cost is the price");
        let product = content.banners.cost.single * content.banners.cost.pods;
        assert_eq!(content.banners.cost.ten_pull_cost, product + content.banners.cost.single);
        assert_ne!(content.banners.cost.ten_pull_cost, product);
        assert_eq!(content.banners.saved.max_keys, 32);

        let missing_cap = src.banners.replace("max_keys = 32\n", "");
        sources.banners = &missing_cap;
        let err = Content::parse(&sources).expect_err("max_keys is required");
        assert!(err.contains("max_keys"), "{err}");

        let missing_pods = src.banners.replace("pods = 10\n", "");
        sources.banners = &missing_pods;
        let err = Content::parse(&sources).expect_err("pods is required");
        assert!(err.contains("pods"), "{err}");

        sources.banners = src.banners;
        let missing_flip = src.crate_reveal.replace("fast_flip = 0.125\n", "");
        assert_ne!(missing_flip, src.crate_reveal);
        sources.crate_reveal = &missing_flip;
        let err = Content::parse(&sources).expect_err("fast_flip is required");
        assert!(err.contains("fast_flip"), "{err}");

        let missing_rank = src.crate_reveal.replacen("rank = 1\n", "", 1);
        assert_ne!(missing_rank, src.crate_reveal);
        sources.crate_reveal = &missing_rank;
        let err = Content::parse(&sources).expect_err("rank is required");
        assert!(err.contains("rank"), "{err}");

        let missing_icon = src.crate_reveal.replace("highlight_blend = 0.45\n", "");
        assert_ne!(missing_icon, src.crate_reveal);
        sources.crate_reveal = &missing_icon;
        let err = Content::parse(&sources).expect_err("highlight_blend is required");
        assert!(err.contains("highlight_blend"), "{err}");

        let missing_cue = src.crate_reveal.replace("pull_failed = \"ui_pull_failed\"\n", "");
        assert_ne!(missing_cue, src.crate_reveal);
        sources.crate_reveal = &missing_cue;
        let err = Content::parse(&sources).expect_err("pull_failed is required");
        assert!(err.contains("pull_failed"), "{err}");

        let missing_ink = src.crate_reveal.replace("failure_ink = \"reveal_paper\"\n", "");
        assert_ne!(missing_ink, src.crate_reveal);
        sources.crate_reveal = &missing_ink;
        let err = Content::parse(&sources).expect_err("failure_ink is required");
        assert!(err.contains("failure_ink"), "{err}");

        let missing_rule = src.crate_reveal.replace("divider_blend = 0.45\n", "");
        assert_ne!(missing_rule, src.crate_reveal);
        sources.crate_reveal = &missing_rule;
        let err = Content::parse(&sources).expect_err("divider_blend is required");
        assert!(err.contains("divider_blend"), "{err}");

        let flip_cue = src.crate_reveal.replace("pull_failed = \"ui_pull_failed\"", "pull_failed = \"crate_flip\"");
        sources.crate_reveal = &flip_cue;
        let err = Content::parse(&sources).expect_err("the failure cue is not the flip");
        assert!(err.contains("chime") && err.contains("flip"), "{err}");

        let missing_rule_num = src.crate_reveal.replace("rule_num = 2\n", "");
        assert_ne!(missing_rule_num, src.crate_reveal);
        sources.crate_reveal = &missing_rule_num;
        let err = Content::parse(&sources).expect_err("rule_num is required");
        assert!(err.contains("rule_num"), "{err}");

        let missing_crate = src.crate_reveal.replace("crate_w_div = 6\n", "");
        assert_ne!(missing_crate, src.crate_reveal);
        sources.crate_reveal = &missing_crate;
        let err = Content::parse(&sources).expect_err("crate_w_div is required");
        assert!(err.contains("crate_w_div"), "{err}");

        let missing_rays = src.crate_reveal.replace("crack_reach = 0.62\n", "");
        assert_ne!(missing_rays, src.crate_reveal);
        sources.crate_reveal = &missing_rays;
        let err = Content::parse(&sources).expect_err("crack_reach is required");
        assert!(err.contains("crack_reach"), "{err}");

        let missing_clear = src.crate_reveal.replace("failure_clear = 28.0\n", "");
        assert_ne!(missing_clear, src.crate_reveal);
        sources.crate_reveal = &missing_clear;
        let err = Content::parse(&sources).expect_err("failure_clear is required");
        assert!(err.contains("failure_clear"), "{err}");

        let missing_lost = src.ui.replace("connection_lost = \"CONNECTION LOST, RETRY\"\n", "");
        assert_ne!(missing_lost, src.ui);
        sources.crate_reveal = src.crate_reveal;
        sources.ui = &missing_lost;
        let err = Content::parse(&sources).expect_err("connection_lost is required");
        assert!(err.contains("connection_lost"), "{err}");
        sources.ui = src.ui;

        let rarity_ink = src.crate_reveal.replace("failure_ink = \"reveal_paper\"", "failure_ink = \"rarity_legendary\"");
        sources.crate_reveal = &rarity_ink;
        let err = Content::parse(&sources).expect_err("the failure line is not a rarity color");
        assert!(err.contains("failure_ink"), "{err}");

        sources.crate_reveal = src.crate_reveal;
        sources.banners = src.banners;
        let content = Content::parse(&sources).expect("items load");
        let visor = content.crate_item("visor").expect("visor");
        let grip = content.crate_item("grip").expect("grip");
        assert_eq!(visor.rarity, "common");
        assert_eq!(grip.rarity, "common");
        assert_ne!(visor.ink, grip.ink);
        let stripped = src.banners.split("[[item]]").next().unwrap();
        sources.banners = stripped;
        let err = Content::parse(&sources).expect_err("items are required");
        assert!(err.contains("item"), "{err}");
    }
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
