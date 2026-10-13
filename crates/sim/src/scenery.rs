//! Arena scenery rules, cover colliders, and the visual record.
//! Mesh building stays in `gemini-meshes`. This module stores authored numbers.

use crate::content::{self, Arena, BossDef, Combat, Palette, WavesFile};
use crate::math::Vec3;

#[derive(Clone, Debug)]
pub struct Play {
    pub rules: SceneryRules,
    pub surfaces: Vec<Surface>,
    pub lanes: Vec<Corridor>,
    pub routes: Vec<Corridor>,
    pub revive_spots: Vec<ReviveSpot>,
    pub cover: Vec<Cover>,
}

#[derive(Clone, Debug)]
pub struct SceneryRules {
    pub min_hue_distance_deg: f32,
    pub min_saturation: f32,
    pub weakpoint: String,
    pub overdrive: String,
    pub yellow: String,
    pub actor_base: Vec<String>,
    pub fog_start: f32,
    pub fog_end: f32,
    pub fog_max_density: f32,
    pub max_fog_at_attack: f32,
    pub max_clutter_height: f32,
    pub bounds_slack: f32,
    pub overlap_epsilon: f32,
    pub cover_tie: String,
    pub spawn_clearance: f32,
    pub clutter_kinds: Vec<String>,
    pub outside_kinds: Vec<String>,
    pub required_cameras: Vec<String>,
    pub surface_kinds: Vec<String>,
    pub luma_weights: [f32; 3],
    pub mover_height: f32,
    pub cover_solve_passes: u32,
}

#[derive(Clone, Debug)]
pub struct Surface {
    pub id: String,
    pub kind: String,
}

#[derive(Clone, Debug)]
pub struct Corridor {
    pub id: String,
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub half_width: f32,
}

#[derive(Clone, Debug)]
pub struct ReviveSpot {
    pub id: String,
    pub pos: [f32; 2],
    pub radius: f32,
}

/// Axis-aligned solid. `pos` is the center. `size` is the full extent.
#[derive(Clone, Debug)]
pub struct Cover {
    pub id: String,
    pub pos: Vec3,
    pub size: [f32; 3],
    pub surface: String,
}

#[derive(Clone, Debug)]
pub struct ArenaScene {
    pub floor_sides: u32,
    pub boulder_slices: u32,
    pub boulder_stacks: u32,
    pub cover_chamfer: f32,
    pub nest_sides: u32,
    pub flora_sides: u32,
    pub trim_height: f32,
    pub trim_outset: f32,
    pub trim_lift: f32,
    pub panel_length: f32,
    pub panel_thickness: f32,
    pub panel_lift: f32,
    pub panel_count: u32,
    pub vent_size: [f32; 3],
    pub vent_center_y: f32,
    pub ledge_size: [f32; 3],
    pub ledge_thickness: f32,
    pub max_triangles: u32,
    pub max_bytes: u32,
    pub sha256: String,
    pub review_ambient: f32,
    pub review_key: f32,
    pub review_fill: f32,
    pub review_gain: f32,
    pub exposure_ev: f32,
    pub light: SceneLight,
    pub materials: Vec<SceneMaterial>,
    pub floors: Vec<FloorBand>,
    pub cliffs: Vec<Cliff>,
    pub props: Vec<Prop>,
    pub nests: Vec<Nest>,
    pub towers: Vec<Tower>,
    pub cameras: Vec<SceneCamera>,
}

#[derive(Clone, Debug)]
pub struct SceneLight {
    pub key_color: String,
    pub key_illuminance: f32,
    pub key_yaw_deg: f32,
    pub key_pitch_deg: f32,
    pub fill_color: String,
    pub fill_illuminance: f32,
    pub fill_yaw_deg: f32,
    pub fill_pitch_deg: f32,
    pub ambient_color: String,
    pub ambient_brightness: f32,
    pub shadow_filter: String,
    pub fog_color: String,
    pub sky_color: String,
}

#[derive(Clone, Debug)]
pub struct SceneMaterial {
    pub id: String,
    pub color: String,
    pub bloom: bool,
    pub glow: bool,
    pub lit: bool,
    pub roughness: f32,
    pub metallic: f32,
}

#[derive(Clone, Debug)]
pub struct FloorBand {
    pub id: String,
    pub inner: f32,
    pub outer: f32,
    pub y: f32,
    pub material: String,
}

#[derive(Clone, Debug)]
pub struct Cliff {
    pub id: String,
    pub angle_deg: f32,
    pub span_deg: f32,
    pub inner: f32,
    pub outer: f32,
    pub inner_height: f32,
    pub outer_height: f32,
    pub material: String,
}

#[derive(Clone, Debug)]
pub struct Prop {
    pub id: String,
    pub kind: String,
    pub pos: [f32; 3],
    pub size: [f32; 3],
    pub yaw_deg: f32,
    pub material: String,
}

#[derive(Clone, Debug)]
pub struct Nest {
    pub id: String,
    pub vent: String,
    pub outset: f32,
    pub size: [f32; 3],
    pub material: String,
    pub goo: String,
    pub goo_scale: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct Tower {
    pub id: String,
    pub ledge: String,
    pub base: [f32; 3],
    pub mast: [f32; 3],
    pub head: [f32; 3],
    pub beacon: [f32; 3],
    pub material: String,
    pub beacon_material: String,
}

#[derive(Clone, Debug)]
pub struct SceneCamera {
    pub id: String,
    pub eye: [f32; 3],
    pub look: [f32; 3],
    pub fov: f32,
    pub width: u32,
    pub height: u32,
}

pub fn fog_factor(distance: f32, start: f32, end: f32) -> f32 {
    if distance <= start {
        0.0
    } else if distance >= end {
        1.0
    } else {
        (distance - start) / (end - start)
    }
}

pub fn load_play(table: &toml::map::Map<String, toml::Value>) -> Result<Play, String> {
    let rules = load_rules(table.get("scenery").unwrap())?;
    let surfaces = load_named(table.get("surface").unwrap(), "surface", load_surface)?;
    let lanes = load_named(table.get("lane").unwrap(), "lane", load_corridor)?;
    let routes = load_named(table.get("route").unwrap(), "route", load_corridor)?;
    let revive_spots = load_named(
        table.get("revive_spot").unwrap(),
        "revive_spot",
        load_revive,
    )?;
    let cover = load_named(table.get("cover").unwrap(), "cover", load_cover)?;
    if surfaces.is_empty()
        || lanes.is_empty()
        || routes.is_empty()
        || revive_spots.is_empty()
        || cover.is_empty()
    {
        return Err(
            "arena_01 scenery needs surfaces, lanes, routes, revive spots, and cover".into(),
        );
    }
    let mut ids = std::collections::BTreeSet::new();
    for id in surfaces
        .iter()
        .map(|row| row.id.as_str())
        .chain(lanes.iter().map(|row| row.id.as_str()))
        .chain(routes.iter().map(|row| row.id.as_str()))
        .chain(revive_spots.iter().map(|row| row.id.as_str()))
        .chain(cover.iter().map(|row| row.id.as_str()))
    {
        if !ids.insert(id.to_string()) {
            return Err(format!("arena_01 duplicate scenery id {id}"));
        }
    }
    for row in &surfaces {
        if !rules.surface_kinds.iter().any(|kind| kind == &row.kind) {
            return Err(format!(
                "surface {} kind \"{}\" is not in surface_kinds",
                row.id, row.kind
            ));
        }
    }
    for row in &cover {
        if !surfaces.iter().any(|surface| surface.id == row.surface) {
            return Err(format!(
                "cover {} surface \"{}\" is not a surface id",
                row.id, row.surface
            ));
        }
        if row.size.iter().any(|n| !(*n > 0.0)) {
            return Err(format!("cover {} size must be > 0", row.id));
        }
    }
    Ok(Play {
        rules,
        surfaces,
        lanes,
        routes,
        revive_spots,
        cover,
    })
}

pub fn load_scene(table: &toml::map::Map<String, toml::Value>) -> Result<ArenaScene, String> {
    let what = "arena_visual.toml";
    let scene = ArenaScene {
        floor_sides: content::vis_u32(table, "floor_sides", what)?,
        boulder_slices: content::vis_u32(table, "boulder_slices", what)?,
        boulder_stacks: content::vis_u32(table, "boulder_stacks", what)?,
        cover_chamfer: content::vis_f32(table, "cover_chamfer", what)?,
        nest_sides: content::vis_u32(table, "nest_sides", what)?,
        flora_sides: content::vis_u32(table, "flora_sides", what)?,
        trim_height: content::vis_f32(table, "trim_height", what)?,
        trim_outset: content::vis_f32(table, "trim_outset", what)?,
        trim_lift: content::vis_f32(table, "trim_lift", what)?,
        panel_length: content::vis_f32(table, "panel_length", what)?,
        panel_thickness: content::vis_f32(table, "panel_thickness", what)?,
        panel_lift: content::vis_f32(table, "panel_lift", what)?,
        panel_count: content::vis_u32(table, "panel_count", what)?,
        vent_size: content::vis_vec3(table, "vent_size", what)?,
        vent_center_y: content::vis_f32(table, "vent_center_y", what)?,
        ledge_size: content::vis_vec3(table, "ledge_size", what)?,
        ledge_thickness: content::vis_f32(table, "ledge_thickness", what)?,
        max_triangles: content::vis_u32(table, "max_triangles", what)?,
        max_bytes: content::vis_u32(table, "max_bytes", what)?,
        sha256: content::vis_str(table, "sha256", what)?.to_ascii_lowercase(),
        review_ambient: content::vis_f32(table, "review_ambient", what)?,
        review_key: content::vis_f32(table, "review_key", what)?,
        review_fill: content::vis_f32(table, "review_fill", what)?,
        review_gain: content::vis_f32(table, "review_gain", what)?,
        exposure_ev: content::vis_f32(table, "exposure_ev", what)?,
        light: load_light(table.get("light").unwrap())?,
        materials: load_named(table.get("material").unwrap(), "material", load_material)?,
        floors: load_named(table.get("floor").unwrap(), "floor", load_floor)?,
        cliffs: load_named(table.get("cliff").unwrap(), "cliff", load_cliff)?,
        props: load_named(table.get("prop").unwrap(), "prop", load_prop)?,
        nests: load_named(table.get("nest").unwrap(), "nest", load_nest)?,
        towers: load_named(table.get("tower").unwrap(), "tower", load_tower)?,
        cameras: load_named(table.get("camera").unwrap(), "camera", load_camera)?,
    };
    if scene.floor_sides < 8
        || scene.boulder_slices < 3
        || scene.boulder_stacks < 2
        || scene.nest_sides < 3
        || scene.flora_sides < 3
        || scene.panel_count < 1
    {
        return Err("arena_visual.toml tessellation counts are out of range".into());
    }
    if !(scene.cover_chamfer > 0.0)
        || !(scene.trim_height > 0.0)
        || !(scene.trim_outset > 0.0)
        || !(scene.panel_length > 0.0)
        || !(scene.panel_thickness > 0.0)
        || !(scene.ledge_thickness > 0.0)
        || scene.vent_size.iter().any(|n| !(*n > 0.0))
        || scene.ledge_size.iter().any(|n| !(*n > 0.0))
        || scene.max_triangles == 0
        || scene.max_bytes == 0
        || !(scene.review_gain > 0.0)
    {
        return Err("arena_visual.toml scenery sizes must be > 0".into());
    }
    if scene.sha256.len() != 64 || !scene.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("arena_visual.toml sha256 must be 64 hex characters".into());
    }
    if scene.light.shadow_filter != "gaussian" {
        return Err(format!(
            "arena_visual.toml shadow_filter \"{}\" is unknown",
            scene.light.shadow_filter
        ));
    }
    if scene.materials.is_empty()
        || scene.floors.is_empty()
        || scene.cliffs.is_empty()
        || scene.cameras.is_empty()
    {
        return Err("arena_visual.toml is missing materials, floors, cliffs, or cameras".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for id in scene
        .materials
        .iter()
        .map(|row| row.id.as_str())
        .chain(scene.floors.iter().map(|row| row.id.as_str()))
        .chain(scene.cliffs.iter().map(|row| row.id.as_str()))
        .chain(scene.props.iter().map(|row| row.id.as_str()))
        .chain(scene.nests.iter().map(|row| row.id.as_str()))
        .chain(scene.towers.iter().map(|row| row.id.as_str()))
        .chain(scene.cameras.iter().map(|row| row.id.as_str()))
    {
        if !ids.insert(id.to_string()) {
            return Err(format!("arena_visual.toml duplicate id {id}"));
        }
    }
    Ok(scene)
}

pub fn validate(
    arena: &Arena,
    scene: &ArenaScene,
    palette: &Palette,
    eye_height: f32,
    waves: &WavesFile,
    bosses: &[BossDef],
    combat: &Combat,
) -> Result<(), String> {
    let rules = &arena.scenery;
    if (rules.mover_height - eye_height).abs() > rules.overlap_epsilon {
        return Err("arena_01 mover_height must match controller eye_height".into());
    }
    let mut ranges = vec![
        ("spitter".to_string(), waves.enemies.spitter.range),
        ("swarmer".to_string(), waves.enemies.swarmer.range),
        ("brute".to_string(), waves.enemies.brute.range),
        ("drone".to_string(), combat.drone_range),
        ("chain".to_string(), combat.chain_range),
        ("ricochet".to_string(), combat.ricochet_range),
        ("ability".to_string(), combat.ability_radius),
        ("revive".to_string(), combat.revive_range),
        ("nova".to_string(), combat.nova_radius),
        ("squad_aura".to_string(), combat.squad_aura_radius),
        (
            "spit_travel".to_string(),
            combat.spit_speed * combat.spit_ttl,
        ),
    ];
    for boss in bosses {
        if boss.range > 0.0 {
            ranges.push((format!("{} range", boss.id), boss.range));
        }
        for (index, phase) in boss.phases.iter().enumerate() {
            if phase.scythe_range > 0.0 {
                ranges.push((
                    format!("{} phase {index} scythe", boss.id),
                    phase.scythe_range,
                ));
            }
        }
    }
    for (name, range) in &ranges {
        if *range <= 0.0 {
            continue;
        }
        let factor = fog_factor(*range, rules.fog_start, rules.fog_end);
        if factor > rules.max_fog_at_attack {
            return Err(format!(
                "fog at {name} range {range} is {factor}, above max_fog_at_attack {}",
                rules.max_fog_at_attack
            ));
        }
    }
    let play = max_play_distance(arena, eye_height);
    if rules.fog_start < play {
        return Err(format!(
            "fog_start {} is inside the play view {play}; fog would dim the arena",
            rules.fog_start
        ));
    }
    let density = 1.0 / (rules.fog_end - rules.fog_start);
    if density > rules.fog_max_density {
        return Err(format!(
            "fog density {density} is above fog_max_density {}",
            rules.fog_max_density
        ));
    }

    let forbidden = [
        ("weakpoint", rules.weakpoint.as_str()),
        ("overdrive", rules.overdrive.as_str()),
        ("yellow", rules.yellow.as_str()),
    ];
    let mut forbidden_hue = Vec::new();
    for (role, name) in forbidden {
        let rgb = rgb_of(palette, name, role)?;
        forbidden_hue.push(hue_sat(rgb).0);
    }
    let mut actor_luma = f32::MAX;
    if rules.actor_base.is_empty() {
        return Err("actor_base is empty".into());
    }
    for name in &rules.actor_base {
        let rgb = rgb_of(palette, name, "actor_base")?;
        actor_luma = actor_luma.min(luma(rgb, rules.luma_weights));
    }
    for material in &scene.materials {
        if material.bloom {
            return Err(format!(
                "material {} sets bloom; scenery does not bloom",
                material.id
            ));
        }
        if material.glow && material.lit {
            return Err(format!(
                "material {} cannot be both glow and lit",
                material.id
            ));
        }
        if !(material.roughness >= 0.0 && material.roughness <= 1.0)
            || !(material.metallic >= 0.0 && material.metallic <= 1.0)
        {
            return Err(format!(
                "material {} roughness and metallic must be in [0, 1]",
                material.id
            ));
        }
        let rgb = rgb_of(
            palette,
            &material.color,
            &format!("material {}", material.id),
        )?;
        let painted = luma(rgb, rules.luma_weights);
        if painted >= actor_luma {
            return Err(format!(
                "material {} color \"{}\" luma {painted} is not below the darkest actor base {actor_luma}",
                material.id, material.color
            ));
        }
        let (hue, sat) = hue_sat(rgb);
        if sat >= rules.min_saturation {
            for (index, forbidden) in forbidden_hue.iter().enumerate() {
                let distance = hue_distance(hue, *forbidden);
                if distance < rules.min_hue_distance_deg {
                    let role = ["weakpoint", "overdrive", "yellow"][index];
                    return Err(format!(
                        "material {} hue is {distance} deg from {role}, under min_hue_distance_deg {}",
                        material.id, rules.min_hue_distance_deg
                    ));
                }
            }
        }
    }
    for (role, name) in [
        ("key", scene.light.key_color.as_str()),
        ("fill", scene.light.fill_color.as_str()),
        ("ambient", scene.light.ambient_color.as_str()),
    ] {
        // Lights are not scenery albedos. They still have to name a real palette key.
        rgb_of(palette, name, role)?;
    }
    for (role, name) in [
        ("fog", scene.light.fog_color.as_str()),
        ("sky", scene.light.sky_color.as_str()),
    ] {
        let rgb = rgb_of(palette, name, role)?;
        let (hue, sat) = hue_sat(rgb);
        if sat >= rules.min_saturation {
            for (index, forbidden) in forbidden_hue.iter().enumerate() {
                let distance = hue_distance(hue, *forbidden);
                if distance < rules.min_hue_distance_deg {
                    let label = ["weakpoint", "overdrive", "yellow"][index];
                    return Err(format!(
                        "{role} hue is {distance} deg from {label}, under min_hue_distance_deg {}",
                        rules.min_hue_distance_deg
                    ));
                }
            }
        }
    }

    let material = |id: &str| scene.materials.iter().find(|row| row.id == id);
    let require_mat = |id: &str, owner: &str| -> Result<(), String> {
        if material(id).is_none() {
            return Err(format!("{owner} material \"{id}\" is missing"));
        }
        Ok(())
    };
    for floor in &scene.floors {
        require_mat(&floor.material, &format!("floor {}", floor.id))?;
        if floor.y.abs() > rules.overlap_epsilon {
            return Err(format!(
                "floor {} y must be 0 so it matches the sim dirt height",
                floor.id
            ));
        }
        if !(floor.outer > floor.inner && floor.inner >= 0.0) {
            return Err(format!("floor {} radii are out of order", floor.id));
        }
    }
    cover_walkable_dirt(arena, scene)?;
    for cliff in &scene.cliffs {
        require_mat(&cliff.material, &format!("cliff {}", cliff.id))?;
        if cliff.inner + rules.overlap_epsilon < arena.radius {
            return Err(format!(
                "cliff {} inner radius is inside the play disc",
                cliff.id
            ));
        }
        if !(cliff.outer > cliff.inner)
            || cliff.span_deg <= 0.0
            || cliff.inner_height <= 0.0
            || cliff.outer_height <= 0.0
        {
            return Err(format!("cliff {} sizes must be > 0", cliff.id));
        }
    }
    abut_cliffs(&scene.cliffs, rules.overlap_epsilon)?;
    for prop in &scene.props {
        require_mat(&prop.material, &format!("prop {}", prop.id))?;
        let known = rules.clutter_kinds.iter().any(|kind| kind == &prop.kind)
            || rules.outside_kinds.iter().any(|kind| kind == &prop.kind);
        if !known {
            return Err(format!(
                "prop {} kind \"{}\" is unknown",
                prop.id, prop.kind
            ));
        }
        if prop.size.iter().any(|n| !(*n > 0.0)) {
            return Err(format!("prop {} size must be > 0", prop.id));
        }
        if rules.clutter_kinds.iter().any(|kind| kind == &prop.kind)
            && prop.size[1] > rules.max_clutter_height
        {
            return Err(format!(
                "prop {} height {} snags movement; max_clutter_height is {}",
                prop.id, prop.size[1], rules.max_clutter_height
            ));
        }
        if rules.outside_kinds.iter().any(|kind| kind == &prop.kind) {
            let reach = (prop.size[0] * prop.size[0] + prop.size[2] * prop.size[2]).sqrt() * 0.5;
            let dist = (prop.pos[0] * prop.pos[0] + prop.pos[2] * prop.pos[2]).sqrt();
            if dist - reach < arena.radius {
                return Err(format!("prop {} is inside the play disc", prop.id));
            }
        }
    }
    for nest in &scene.nests {
        require_mat(&nest.material, &format!("nest {}", nest.id))?;
        require_mat(&nest.goo, &format!("nest {} goo", nest.id))?;
        if !arena.vents.iter().any(|vent| vent.id == nest.vent) {
            return Err(format!(
                "nest {} vent \"{}\" is missing",
                nest.id, nest.vent
            ));
        }
        if !(nest.outset > 0.0)
            || nest.size.iter().any(|n| !(*n > 0.0))
            || nest.goo_scale.iter().any(|n| !(*n > 0.0 && *n < 1.0))
        {
            return Err(format!("nest {} sizes are out of range", nest.id));
        }
        let goo = material(&nest.goo).unwrap();
        if !goo.glow {
            return Err(format!("nest {} goo material must be glow", nest.id));
        }
    }
    for tower in &scene.towers {
        require_mat(&tower.material, &format!("tower {}", tower.id))?;
        require_mat(&tower.beacon_material, &format!("tower {}", tower.id))?;
        if !arena.ledges.iter().any(|ledge| ledge.id == tower.ledge) {
            return Err(format!(
                "tower {} ledge \"{}\" is missing",
                tower.id, tower.ledge
            ));
        }
        for (name, size) in [
            ("base", tower.base),
            ("mast", tower.mast),
            ("head", tower.head),
            ("beacon", tower.beacon),
        ] {
            if size.iter().any(|n| !(*n > 0.0)) {
                return Err(format!("tower {} {name} size must be > 0", tower.id));
            }
        }
        let beacon = material(&tower.beacon_material).unwrap();
        if !beacon.glow {
            return Err(format!("tower {} beacon must be a glow material", tower.id));
        }
    }
    for id in &rules.required_cameras {
        if !scene.cameras.iter().any(|camera| camera.id == *id) {
            return Err(format!("arena_visual.toml is missing camera {id}"));
        }
    }
    for camera in &scene.cameras {
        if !(camera.fov > 1.0 && camera.fov < 170.0) || camera.width < 8 || camera.height < 8 {
            return Err(format!("camera {} is out of range", camera.id));
        }
    }
    let half = scene.cover_chamfer;
    for cover in &arena.cover {
        let min_half = cover.size[0].min(cover.size[2]) * 0.5;
        if half >= min_half {
            return Err(format!("cover_chamfer does not fit cover {}", cover.id));
        }
    }
    clear_cover(arena)?;
    clear_props(arena, scene)?;
    Ok(())
}

fn cover_walkable_dirt(arena: &Arena, scene: &ArenaScene) -> Result<(), String> {
    let mut bands: Vec<&FloorBand> = scene.floors.iter().collect();
    bands.sort_by(|a, b| {
        a.inner
            .partial_cmp(&b.inner)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let start = arena.platform.radius;
    let end = arena.radius;
    if bands.is_empty() || bands[0].inner > start + arena.scenery.overlap_epsilon {
        return Err(
            "floor bands must cover the dirt from the platform edge to the play radius".into(),
        );
    }
    let mut cursor = bands[0].inner;
    for band in &bands {
        if band.inner > cursor + arena.scenery.overlap_epsilon {
            return Err(format!("floor {} leaves a gap in the dirt", band.id));
        }
        if band.outer > cursor {
            cursor = band.outer;
        }
    }
    if cursor + arena.scenery.overlap_epsilon < end {
        return Err("floor bands stop inside the play radius".into());
    }
    Ok(())
}

fn abut_cliffs(cliffs: &[Cliff], epsilon: f32) -> Result<(), String> {
    if cliffs.is_empty() {
        return Err("arena_visual.toml is missing cliff".into());
    }
    let mut rows: Vec<&Cliff> = cliffs.iter().collect();
    rows.sort_by(|a, b| {
        a.angle_deg
            .partial_cmp(&b.angle_deg)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut cursor = rows[0].angle_deg;
    for cliff in &rows {
        if (cliff.angle_deg - cursor).abs() > epsilon {
            return Err(format!("cliff {} leaves a gap in the rim", cliff.id));
        }
        cursor += cliff.span_deg;
    }
    let end = rows[0].angle_deg + 360.0;
    if (cursor - end).abs() > epsilon {
        return Err("cliff spans must close the rim".into());
    }
    Ok(())
}

fn clear_cover(arena: &Arena) -> Result<(), String> {
    let rules = &arena.scenery;
    for cover in &arena.cover {
        let (min, max) = xz_bounds(cover.pos.x, cover.pos.y, cover.pos.z, cover.size, 0.0);
        for lane in arena.lanes.iter().chain(arena.routes.iter()) {
            if corridor_hits(lane, min, max, rules.overlap_epsilon) {
                return Err(format!("cover {} blocks {}", cover.id, lane.id));
            }
        }
        for spot in &arena.revive_spots {
            if disc_hits(spot.pos, spot.radius, min, max) {
                return Err(format!("cover {} blocks revive spot {}", cover.id, spot.id));
            }
        }
        for vent in &arena.vents {
            if disc_hits([vent.pos.x, vent.pos.z], rules.spawn_clearance, min, max) {
                return Err(format!("cover {} blocks {}", cover.id, vent.id));
            }
        }
        if disc_hits(
            [arena.player_spawn.x, arena.player_spawn.z],
            rules.spawn_clearance,
            min,
            max,
        ) {
            return Err(format!("cover {} blocks the player spawn", cover.id));
        }
        if disc_hits(
            [arena.boss_area.center.x, arena.boss_area.center.z],
            arena.boss_area.radius,
            min,
            max,
        ) {
            return Err(format!("cover {} blocks the boss area", cover.id));
        }
        for other in &arena.cover {
            if other.id <= cover.id {
                continue;
            }
            let (omin, omax) = xz_bounds(other.pos.x, other.pos.y, other.pos.z, other.size, 0.0);
            if aabb_hits(min, max, omin, omax, rules.overlap_epsilon) {
                return Err(format!("cover {} overlaps {}", cover.id, other.id));
            }
        }
    }
    Ok(())
}

fn clear_props(arena: &Arena, scene: &ArenaScene) -> Result<(), String> {
    for prop in &scene.props {
        if !arena
            .scenery
            .clutter_kinds
            .iter()
            .any(|kind| kind == &prop.kind)
        {
            continue;
        }
        let (min, max) = xz_bounds(
            prop.pos[0],
            prop.pos[1],
            prop.pos[2],
            prop.size,
            prop.yaw_deg,
        );
        for lane in arena.lanes.iter().chain(arena.routes.iter()) {
            if corridor_hits(lane, min, max, arena.scenery.overlap_epsilon) {
                return Err(format!("prop {} blocks {}", prop.id, lane.id));
            }
        }
        for spot in &arena.revive_spots {
            if disc_hits(spot.pos, spot.radius, min, max) {
                return Err(format!("prop {} blocks revive spot {}", prop.id, spot.id));
            }
        }
    }
    Ok(())
}

pub fn max_play_distance(arena: &Arena, eye_height: f32) -> f32 {
    let mut stand = arena.platform.height;
    for ledge in &arena.ledges {
        if ledge.pos.y > stand {
            stand = ledge.pos.y;
        }
    }
    if arena.player_spawn.y > stand {
        stand = arena.player_spawn.y;
    }
    let rise = stand + eye_height;
    let span = arena.radius * 2.0;
    (span * span + rise * rise).sqrt()
}

impl Arena {
    pub fn push_cover(&self, pos: &mut Vec3, radius: f32) {
        let passes = self.scenery.cover_solve_passes;
        for _ in 0..passes {
            let mut hit = false;
            for cover in &self.cover {
                if push_one(
                    pos,
                    cover,
                    radius,
                    self.scenery.mover_height,
                    &self.scenery.cover_tie,
                ) {
                    hit = true;
                }
            }
            if !hit {
                break;
            }
        }
    }

    pub fn cover_blocks_segment(&self, p0: Vec3, p1: Vec3) -> bool {
        self.cover
            .iter()
            .any(|cover| segment_hits_box(p0, p1, cover, self.scenery.overlap_epsilon))
    }
}

fn push_one(pos: &mut Vec3, cover: &Cover, radius: f32, height: f32, tie: &str) -> bool {
    let hx = cover.size[0] * 0.5 + radius;
    let hz = cover.size[2] * 0.5 + radius;
    let bottom = cover.pos.y - cover.size[1] * 0.5;
    let top = cover.pos.y + cover.size[1] * 0.5;
    let head = pos.y + height;
    if head <= bottom || pos.y >= top {
        return false;
    }
    let min_x = cover.pos.x - hx;
    let max_x = cover.pos.x + hx;
    let min_z = cover.pos.z - hz;
    let max_z = cover.pos.z + hz;
    if pos.x <= min_x || pos.x >= max_x || pos.z <= min_z || pos.z >= max_z {
        return false;
    }
    let pen_x = (max_x - pos.x).min(pos.x - min_x);
    let pen_z = (max_z - pos.z).min(pos.z - min_z);
    let use_x = if (pen_x - pen_z).abs() <= f32::EPSILON {
        tie == "x"
    } else {
        pen_x < pen_z
    };
    if use_x {
        pos.x = if pos.x < cover.pos.x { min_x } else { max_x };
    } else {
        pos.z = if pos.z < cover.pos.z { min_z } else { max_z };
    }
    true
}

fn segment_hits_box(p0: Vec3, p1: Vec3, cover: &Cover, epsilon: f32) -> bool {
    let min = [
        cover.pos.x - cover.size[0] * 0.5 - epsilon,
        cover.pos.y - cover.size[1] * 0.5 - epsilon,
        cover.pos.z - cover.size[2] * 0.5 - epsilon,
    ];
    let max = [
        cover.pos.x + cover.size[0] * 0.5 + epsilon,
        cover.pos.y + cover.size[1] * 0.5 + epsilon,
        cover.pos.z + cover.size[2] * 0.5 + epsilon,
    ];
    let origin = [p0.x, p0.y, p0.z];
    let delta = [p1.x - p0.x, p1.y - p0.y, p1.z - p0.z];
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    for axis in 0..3 {
        if delta[axis].abs() <= epsilon {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return false;
            }
            continue;
        }
        let inv = 1.0 / delta[axis];
        let mut ta = (min[axis] - origin[axis]) * inv;
        let mut tb = (max[axis] - origin[axis]) * inv;
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        if ta > t0 {
            t0 = ta;
        }
        if tb < t1 {
            t1 = tb;
        }
        if t0 > t1 {
            return false;
        }
    }
    true
}

fn xz_bounds(x: f32, _y: f32, z: f32, size: [f32; 3], yaw_deg: f32) -> ([f32; 2], [f32; 2]) {
    let (s, c) = yaw_deg.to_radians().sin_cos();
    let hx = size[0] * 0.5;
    let hz = size[2] * 0.5;
    let mut min = [f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN];
    for local in [[hx, hz], [hx, -hz], [-hx, hz], [-hx, -hz]] {
        let wx = x + local[0] * c + local[1] * s;
        let wz = z - local[0] * s + local[1] * c;
        min[0] = min[0].min(wx);
        min[1] = min[1].min(wz);
        max[0] = max[0].max(wx);
        max[1] = max[1].max(wz);
    }
    (min, max)
}

fn corridor_hits(lane: &Corridor, min: [f32; 2], max: [f32; 2], epsilon: f32) -> bool {
    if segment_hits_aabb_xz(lane.from, lane.to, min, max, epsilon) {
        return true;
    }
    let corners = [
        [min[0], min[1]],
        [max[0], min[1]],
        [min[0], max[1]],
        [max[0], max[1]],
    ];
    for corner in corners {
        if dist_point_segment(corner, lane.from, lane.to) <= lane.half_width + epsilon {
            return true;
        }
    }
    for end in [lane.from, lane.to] {
        if dist_point_aabb(end, min, max) <= lane.half_width + epsilon {
            return true;
        }
    }
    false
}

fn segment_hits_aabb_xz(
    a: [f32; 2],
    b: [f32; 2],
    min: [f32; 2],
    max: [f32; 2],
    epsilon: f32,
) -> bool {
    let delta = [b[0] - a[0], b[1] - a[1]];
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    for axis in 0..2 {
        if delta[axis].abs() <= epsilon {
            if a[axis] < min[axis] || a[axis] > max[axis] {
                return false;
            }
            continue;
        }
        let inv = 1.0 / delta[axis];
        let mut ta = (min[axis] - a[axis]) * inv;
        let mut tb = (max[axis] - a[axis]) * inv;
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        if ta > t0 {
            t0 = ta;
        }
        if tb < t1 {
            t1 = tb;
        }
        if t0 > t1 {
            return false;
        }
    }
    true
}

fn dist_point_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let abx = b[0] - a[0];
    let abz = b[1] - a[1];
    let len2 = abx * abx + abz * abz;
    if len2 == 0.0 {
        return (p[0] - a[0]).hypot(p[1] - a[1]);
    }
    let t = ((p[0] - a[0]) * abx + (p[1] - a[1]) * abz) / len2;
    let t = if t < 0.0 {
        0.0
    } else if t > 1.0 {
        1.0
    } else {
        t
    };
    let qx = a[0] + abx * t;
    let qz = a[1] + abz * t;
    (p[0] - qx).hypot(p[1] - qz)
}

fn dist_point_aabb(p: [f32; 2], min: [f32; 2], max: [f32; 2]) -> f32 {
    let x = if p[0] < min[0] {
        min[0] - p[0]
    } else if p[0] > max[0] {
        p[0] - max[0]
    } else {
        0.0
    };
    let z = if p[1] < min[1] {
        min[1] - p[1]
    } else if p[1] > max[1] {
        p[1] - max[1]
    } else {
        0.0
    };
    x.hypot(z)
}

fn disc_hits(center: [f32; 2], radius: f32, min: [f32; 2], max: [f32; 2]) -> bool {
    dist_point_aabb(center, min, max) <= radius
}

fn aabb_hits(a0: [f32; 2], a1: [f32; 2], b0: [f32; 2], b1: [f32; 2], epsilon: f32) -> bool {
    a0[0] < b1[0] + epsilon
        && a1[0] > b0[0] - epsilon
        && a0[1] < b1[1] + epsilon
        && a1[1] > b0[1] - epsilon
}

fn rgb_of(palette: &Palette, name: &str, owner: &str) -> Result<[f32; 3], String> {
    let hex = palette
        .hex(name)
        .ok_or_else(|| format!("{owner} color \"{name}\" does not resolve"))?;
    let digits = hex.trim().trim_start_matches('#');
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{owner} color \"{name}\" must be #RRGGBB"));
    }
    let channel = |index: usize| {
        u8::from_str_radix(&digits[index..index + 2], 16).unwrap_or(0) as f32 / 255.0
    };
    Ok([channel(0), channel(2), channel(4)])
}

fn luma(rgb: [f32; 3], weights: [f32; 3]) -> f32 {
    rgb[0] * weights[0] + rgb[1] * weights[1] + rgb[2] * weights[2]
}

fn hue_sat(rgb: [f32; 3]) -> (f32, f32) {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let delta = max - min;
    let sat = if max <= 0.0 { 0.0 } else { delta / max };
    if delta == 0.0 {
        return (0.0, sat);
    }
    let hue = if max == rgb[0] {
        60.0 * (((rgb[1] - rgb[2]) / delta).rem_euclid(6.0))
    } else if max == rgb[1] {
        60.0 * (((rgb[2] - rgb[0]) / delta) + 2.0)
    } else {
        60.0 * (((rgb[0] - rgb[1]) / delta) + 4.0)
    };
    (hue.rem_euclid(360.0), sat)
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs();
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

fn load_rules(value: &toml::Value) -> Result<SceneryRules, String> {
    let table = value
        .as_table()
        .ok_or_else(|| "scenery is not a table".to_string())?;
    content::expect_keys(
        table,
        &[
            "min_hue_distance_deg",
            "min_saturation",
            "weakpoint",
            "overdrive",
            "yellow",
            "actor_base",
            "fog_start",
            "fog_end",
            "fog_max_density",
            "max_fog_at_attack",
            "max_clutter_height",
            "bounds_slack",
            "overlap_epsilon",
            "cover_tie",
            "spawn_clearance",
            "clutter_kinds",
            "outside_kinds",
            "required_cameras",
            "surface_kinds",
            "luma_weights",
            "mover_height",
            "cover_solve_passes",
        ],
        "scenery",
    )?;
    let min_hue_distance_deg = content::num(
        table.get("min_hue_distance_deg").unwrap(),
        "min_hue_distance_deg",
    )?;
    let min_saturation = content::num(table.get("min_saturation").unwrap(), "min_saturation")?;
    let fog_start = content::num(table.get("fog_start").unwrap(), "fog_start")?;
    let fog_end = content::num(table.get("fog_end").unwrap(), "fog_end")?;
    let fog_max_density = content::num(table.get("fog_max_density").unwrap(), "fog_max_density")?;
    let max_fog_at_attack =
        content::num(table.get("max_fog_at_attack").unwrap(), "max_fog_at_attack")?;
    let max_clutter_height = content::num(
        table.get("max_clutter_height").unwrap(),
        "max_clutter_height",
    )?;
    let bounds_slack = content::num(table.get("bounds_slack").unwrap(), "bounds_slack")?;
    let overlap_epsilon = content::num(table.get("overlap_epsilon").unwrap(), "overlap_epsilon")?;
    let spawn_clearance = content::num(table.get("spawn_clearance").unwrap(), "spawn_clearance")?;
    let mover_height = content::num(table.get("mover_height").unwrap(), "mover_height")?;
    let cover_tie = table
        .get("cover_tie")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "cover_tie must be a string".to_string())?
        .to_string();
    if cover_tie != "x" && cover_tie != "z" {
        return Err(format!("cover_tie \"{cover_tie}\" is unknown"));
    }
    let luma_weights = content::nums3(table.get("luma_weights").unwrap(), "luma_weights")?;
    if luma_weights.iter().any(|n| *n < 0.0) || luma_weights.iter().sum::<f32>() <= 0.0 {
        return Err("luma_weights must be non-negative and sum to > 0".into());
    }
    if !(min_hue_distance_deg > 0.0 && min_hue_distance_deg < 180.0) {
        return Err("min_hue_distance_deg must be in (0, 180)".into());
    }
    if !(min_saturation > 0.0 && min_saturation < 1.0) {
        return Err("min_saturation must be in (0, 1)".into());
    }
    if !(fog_end > fog_start)
        || fog_max_density <= 0.0
        || max_fog_at_attack < 0.0
        || max_fog_at_attack > 1.0
    {
        return Err("fog_start, fog_end, and fog limits are out of range".into());
    }
    if max_clutter_height <= 0.0
        || bounds_slack < 0.0
        || overlap_epsilon < 0.0
        || spawn_clearance < 0.0
        || mover_height <= 0.0
    {
        return Err("scenery clearances must be >= 0".into());
    }
    let passes = table
        .get("cover_solve_passes")
        .and_then(|v| v.as_integer())
        .ok_or_else(|| "cover_solve_passes must be an integer".to_string())?;
    if passes < 1 {
        return Err("cover_solve_passes must be >= 1".into());
    }
    Ok(SceneryRules {
        min_hue_distance_deg,
        min_saturation,
        weakpoint: req_name(table, "weakpoint")?,
        overdrive: req_name(table, "overdrive")?,
        yellow: req_name(table, "yellow")?,
        actor_base: string_list(table.get("actor_base").unwrap(), "actor_base")?,
        fog_start,
        fog_end,
        fog_max_density,
        max_fog_at_attack,
        max_clutter_height,
        bounds_slack,
        overlap_epsilon,
        cover_tie,
        spawn_clearance,
        clutter_kinds: string_list(table.get("clutter_kinds").unwrap(), "clutter_kinds")?,
        outside_kinds: string_list(table.get("outside_kinds").unwrap(), "outside_kinds")?,
        required_cameras: string_list(table.get("required_cameras").unwrap(), "required_cameras")?,
        surface_kinds: string_list(table.get("surface_kinds").unwrap(), "surface_kinds")?,
        luma_weights,
        mover_height,
        cover_solve_passes: passes as u32,
    })
}

fn load_light(value: &toml::Value) -> Result<SceneLight, String> {
    let table = value
        .as_table()
        .ok_or_else(|| "light is not a table".to_string())?;
    content::expect_keys(
        table,
        &[
            "key_color",
            "key_illuminance",
            "key_yaw_deg",
            "key_pitch_deg",
            "fill_color",
            "fill_illuminance",
            "fill_yaw_deg",
            "fill_pitch_deg",
            "ambient_color",
            "ambient_brightness",
            "shadow_filter",
            "fog_color",
            "sky_color",
        ],
        "light",
    )?;
    let light = SceneLight {
        key_color: content::vis_str(table, "key_color", "light")?.to_string(),
        key_illuminance: content::vis_f32(table, "key_illuminance", "light")?,
        key_yaw_deg: content::vis_f32(table, "key_yaw_deg", "light")?,
        key_pitch_deg: content::vis_f32(table, "key_pitch_deg", "light")?,
        fill_color: content::vis_str(table, "fill_color", "light")?.to_string(),
        fill_illuminance: content::vis_f32(table, "fill_illuminance", "light")?,
        fill_yaw_deg: content::vis_f32(table, "fill_yaw_deg", "light")?,
        fill_pitch_deg: content::vis_f32(table, "fill_pitch_deg", "light")?,
        ambient_color: content::vis_str(table, "ambient_color", "light")?.to_string(),
        ambient_brightness: content::vis_f32(table, "ambient_brightness", "light")?,
        shadow_filter: content::vis_str(table, "shadow_filter", "light")?.to_string(),
        fog_color: content::vis_str(table, "fog_color", "light")?.to_string(),
        sky_color: content::vis_str(table, "sky_color", "light")?.to_string(),
    };
    if light.key_illuminance <= 0.0
        || light.fill_illuminance <= 0.0
        || light.ambient_brightness <= 0.0
    {
        return Err("light illuminance must be > 0".into());
    }
    Ok(light)
}

fn load_surface(table: &toml::map::Map<String, toml::Value>) -> Result<Surface, String> {
    content::expect_keys(table, &["id", "kind"], "surface")?;
    Ok(Surface {
        id: req_id(table, "surface")?,
        kind: req_name(table, "kind")?,
    })
}

fn load_corridor(table: &toml::map::Map<String, toml::Value>) -> Result<Corridor, String> {
    content::expect_keys(table, &["id", "from", "to", "half_width"], "corridor")?;
    let from = content::nums2(table.get("from").unwrap(), "from")?;
    let to = content::nums2(table.get("to").unwrap(), "to")?;
    let half_width = content::num(table.get("half_width").unwrap(), "half_width")?;
    if half_width <= 0.0 || (from[0] - to[0]).hypot(from[1] - to[1]) <= 0.0 {
        return Err("corridor half_width and length must be > 0".into());
    }
    Ok(Corridor {
        id: req_id(table, "corridor")?,
        from,
        to,
        half_width,
    })
}

fn load_revive(table: &toml::map::Map<String, toml::Value>) -> Result<ReviveSpot, String> {
    content::expect_keys(table, &["id", "pos", "radius"], "revive_spot")?;
    let radius = content::num(table.get("radius").unwrap(), "radius")?;
    if radius <= 0.0 {
        return Err("revive_spot radius must be > 0".into());
    }
    Ok(ReviveSpot {
        id: req_id(table, "revive_spot")?,
        pos: content::nums2(table.get("pos").unwrap(), "pos")?,
        radius,
    })
}

fn load_cover(table: &toml::map::Map<String, toml::Value>) -> Result<Cover, String> {
    content::expect_keys(table, &["id", "pos", "size", "surface"], "cover")?;
    let pos = content::nums3(table.get("pos").unwrap(), "pos")?;
    let size = content::nums3(table.get("size").unwrap(), "size")?;
    Ok(Cover {
        id: req_id(table, "cover")?,
        pos: Vec3::new(pos[0], pos[1], pos[2]),
        size,
        surface: req_name(table, "surface")?,
    })
}

fn load_material(table: &toml::map::Map<String, toml::Value>) -> Result<SceneMaterial, String> {
    content::expect_keys(
        table,
        &[
            "id",
            "color",
            "bloom",
            "glow",
            "lit",
            "roughness",
            "metallic",
        ],
        "material",
    )?;
    Ok(SceneMaterial {
        id: req_id(table, "material")?,
        color: req_name(table, "color")?,
        bloom: content::vis_bool(table, "bloom", "material")?,
        glow: content::vis_bool(table, "glow", "material")?,
        lit: content::vis_bool(table, "lit", "material")?,
        roughness: content::vis_f32(table, "roughness", "material")?,
        metallic: content::vis_f32(table, "metallic", "material")?,
    })
}

fn load_floor(table: &toml::map::Map<String, toml::Value>) -> Result<FloorBand, String> {
    content::expect_keys(table, &["id", "inner", "outer", "y", "material"], "floor")?;
    Ok(FloorBand {
        id: req_id(table, "floor")?,
        inner: content::num(table.get("inner").unwrap(), "inner")?,
        outer: content::num(table.get("outer").unwrap(), "outer")?,
        y: content::num(table.get("y").unwrap(), "y")?,
        material: req_name(table, "material")?,
    })
}

fn load_cliff(table: &toml::map::Map<String, toml::Value>) -> Result<Cliff, String> {
    content::expect_keys(
        table,
        &[
            "id",
            "angle_deg",
            "span_deg",
            "inner",
            "outer",
            "inner_height",
            "outer_height",
            "material",
        ],
        "cliff",
    )?;
    Ok(Cliff {
        id: req_id(table, "cliff")?,
        angle_deg: content::num(table.get("angle_deg").unwrap(), "angle_deg")?,
        span_deg: content::num(table.get("span_deg").unwrap(), "span_deg")?,
        inner: content::num(table.get("inner").unwrap(), "inner")?,
        outer: content::num(table.get("outer").unwrap(), "outer")?,
        inner_height: content::num(table.get("inner_height").unwrap(), "inner_height")?,
        outer_height: content::num(table.get("outer_height").unwrap(), "outer_height")?,
        material: req_name(table, "material")?,
    })
}

fn load_prop(table: &toml::map::Map<String, toml::Value>) -> Result<Prop, String> {
    content::expect_keys(
        table,
        &["id", "kind", "pos", "size", "yaw_deg", "material"],
        "prop",
    )?;
    Ok(Prop {
        id: req_id(table, "prop")?,
        kind: req_name(table, "kind")?,
        pos: content::nums3(table.get("pos").unwrap(), "pos")?,
        size: content::nums3(table.get("size").unwrap(), "size")?,
        yaw_deg: content::num(table.get("yaw_deg").unwrap(), "yaw_deg")?,
        material: req_name(table, "material")?,
    })
}

fn load_nest(table: &toml::map::Map<String, toml::Value>) -> Result<Nest, String> {
    content::expect_keys(
        table,
        &[
            "id",
            "vent",
            "outset",
            "size",
            "material",
            "goo",
            "goo_scale",
        ],
        "nest",
    )?;
    Ok(Nest {
        id: req_id(table, "nest")?,
        vent: req_name(table, "vent")?,
        outset: content::num(table.get("outset").unwrap(), "outset")?,
        size: content::nums3(table.get("size").unwrap(), "size")?,
        material: req_name(table, "material")?,
        goo: req_name(table, "goo")?,
        goo_scale: content::nums3(table.get("goo_scale").unwrap(), "goo_scale")?,
    })
}

fn load_tower(table: &toml::map::Map<String, toml::Value>) -> Result<Tower, String> {
    content::expect_keys(
        table,
        &[
            "id",
            "ledge",
            "base",
            "mast",
            "head",
            "beacon",
            "material",
            "beacon_material",
        ],
        "tower",
    )?;
    Ok(Tower {
        id: req_id(table, "tower")?,
        ledge: req_name(table, "ledge")?,
        base: content::nums3(table.get("base").unwrap(), "base")?,
        mast: content::nums3(table.get("mast").unwrap(), "mast")?,
        head: content::nums3(table.get("head").unwrap(), "head")?,
        beacon: content::nums3(table.get("beacon").unwrap(), "beacon")?,
        material: req_name(table, "material")?,
        beacon_material: req_name(table, "beacon_material")?,
    })
}

fn load_camera(table: &toml::map::Map<String, toml::Value>) -> Result<SceneCamera, String> {
    content::expect_keys(
        table,
        &["id", "eye", "look", "fov", "width", "height"],
        "camera",
    )?;
    let width = table
        .get("width")
        .and_then(|v| v.as_integer())
        .ok_or_else(|| "camera width must be an integer".to_string())?;
    let height = table
        .get("height")
        .and_then(|v| v.as_integer())
        .ok_or_else(|| "camera height must be an integer".to_string())?;
    if width < 0 || height < 0 {
        return Err("camera width and height must be >= 0".into());
    }
    Ok(SceneCamera {
        id: req_id(table, "camera")?,
        eye: content::nums3(table.get("eye").unwrap(), "eye")?,
        look: content::nums3(table.get("look").unwrap(), "look")?,
        fov: content::num(table.get("fov").unwrap(), "fov")?,
        width: width as u32,
        height: height as u32,
    })
}

fn load_named<T>(
    value: &toml::Value,
    what: &str,
    load: fn(&toml::map::Map<String, toml::Value>) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let rows = value
        .as_array()
        .ok_or_else(|| format!("{what} must be an array"))?;
    let mut out = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let table = row
            .as_table()
            .ok_or_else(|| format!("{what}[{index}] is not a table"))?;
        out.push(load(table)?);
    }
    Ok(out)
}

fn string_list(value: &toml::Value, what: &str) -> Result<Vec<String>, String> {
    let rows = value
        .as_array()
        .ok_or_else(|| format!("{what} must be an array"))?;
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

fn req_name(table: &toml::map::Map<String, toml::Value>, key: &str) -> Result<String, String> {
    table
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|text| !text.is_empty())
        .map(|text| text.to_string())
        .ok_or_else(|| format!("{key} must be a string"))
}

fn req_id(table: &toml::map::Map<String, toml::Value>, what: &str) -> Result<String, String> {
    req_name(table, "id").map_err(|_| format!("{what} is missing id"))
}
