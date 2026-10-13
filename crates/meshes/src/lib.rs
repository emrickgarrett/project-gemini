//! Procedural placeholder meshes. One shared mesh set per bug type.
//! Colors are filled in by the client from `content/palette.toml`.

mod arena;
mod away_still;
mod cast;
mod geom;
mod glyph;
mod menu_still;
mod portrait;
mod pose;
mod preview;
mod reveal_still;
mod visual;

pub use cast::{
    bake_overdrive_emissive, bake_textures, body_pivot, cast, casts, hero_pieces, hero_placement, load_catalog, model_bytes, muzzle_tip, paint_approved_gun, place_point, read_model_file,
    CastMesh, PaintCam, Placement,
};

pub use menu_still::{paint_menu, raster_hero_portrait, Hit, MenuShot, PaintPrefs};

pub use arena::{pieces as arena_pieces, review_ground, ArenaPiece};
pub use away_still::{paint_away, AwayShot};
pub use reveal_still::{paint_reveal, reveal_still_is_flat, still_color_stats};

pub use pose::{pose, root_motion, AnimInput, Pose, Xform};
pub use preview::{
    missing_glow_keys, missing_glow_keys_open, painted_pixels, raster, raster_bug, raster_crown, raster_embedded, raster_heroes_at_15m, raster_in_arena, raster_model, raster_rail_edge, raster_top,
    raster_crowd, raster_viewmodel, still,
    still_open, still_top, ViewmodelFrame,
};
pub use visual::{bug, catalog, flat_shaded, EnemyVisual, Gate, Joint, Part, ShotCamera};

/// Rec.709 luma shared by the away grade and locked hero cards.
pub fn luma(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

#[derive(Clone, Debug)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            colors: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn tri_count(&self) -> usize {
        self.indices.len() / 3
    }
}

impl Default for MeshData {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rgba(pub [f32; 4]);

pub fn blob_shadow() -> MeshData {
    let mut m = MeshData::new();
    disc(&mut m, [0.0, 0.02, 0.0], 0.55, [0.05, 0.04, 0.06, 0.45], 8);
    m
}

/// First-person gloves and pulse rifle, split so each hero can tint `accent`.
#[derive(Clone, Debug)]
pub struct Viewmodel {
    pub teal: MeshData,
    pub cobalt: MeshData,
    pub white: MeshData,
    pub accent: MeshData,
    pub core: MeshData,
    /// Outer barrel tube. Idle stays cobalt; Overdrive tints this orange.
    pub barrel: MeshData,
}

impl Viewmodel {
    pub fn tri_count(&self) -> usize {
        self.teal.tri_count()
            + self.cobalt.tri_count()
            + self.white.tri_count()
            + self.accent.tri_count()
            + self.core.tri_count()
            + self.barrel.tri_count()
    }
}

/// Barrel tip in viewmodel mesh space, before the screen offset from characters.toml.
/// Vex's muzzle in viewmodel mesh space, before `viewmodel_offset`.
/// The fitted gun keeps its grip at the origin and its muzzle down -Z.
pub fn muzzle_local() -> [f32; 3] {
    let gun = cast("vex_gun");
    let tip = muzzle_tip(gun);
    let content = gemini_sim::content::Content::embedded();
    let view = content
        .models
        .iter()
        .find(|model| model.id == "vex_gun")
        .unwrap_or_else(|| panic!("vex_gun has no cast record"))
        .view;
    [view[0] + tip[0], view[1] + tip[1], view[2] + tip[2]]
}

/// Round muzzle glow, centered on the origin. Scale it by the radius from characters.toml.
pub fn muzzle_glow(radius: f32) -> MeshData {
    let mut mesh = MeshData::new();
    let radius = if radius > 0.0 { radius } else { 1.0 };
    geom::gem(&mut mesh, [0.0, 0.0, 0.0], [radius, radius, radius], 16, 10, [1.0, 1.0, 1.0, 1.0]);
    mesh
}

/// Move `tip` back along +Z until it sits `tip_px` screen pixels onto the barrel.
pub fn muzzle_on_barrel(tip: [f32; 3], fov_deg: f32, width: f32, height: f32, tip_px: f32) -> [f32; 3] {
    if !(tip_px > 0.0) || !(width > 1.0) || !(height > 1.0) {
        return tip;
    }
    let step = 0.05;
    let back = [tip[0], tip[1], tip[2] + step];
    let Some(a) = project_view(tip, fov_deg, width, height) else {
        return tip;
    };
    let Some(b) = project_view(back, fov_deg, width, height) else {
        return tip;
    };
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dist = (dx * dx + dy * dy).sqrt();
    if !(dist > 1e-4) {
        return tip;
    }
    let scale = tip_px / dist;
    [tip[0], tip[1], tip[2] + step * scale]
}

/// Screen radius of a viewmodel-space sphere at `depth` (camera forward distance).
pub fn radius_px(radius: f32, depth: f32, fov_deg: f32, height: f32) -> f32 {
    let focal = 1.0 / (fov_deg.to_radians() * 0.5).tan();
    let z = if depth > 0.05 { depth } else { 0.05 };
    radius / z * focal * 0.5 * height
}

/// Camera at the origin, looking down -Z. Matches the first-person still.
pub fn project_view(p: [f32; 3], fov_deg: f32, width: f32, height: f32) -> Option<[f32; 3]> {
    let z = -p[2];
    if z < 0.05 {
        return None;
    }
    let focal = 1.0 / (fov_deg.to_radians() * 0.5).tan();
    let aspect = width / height;
    let x = p[0] / z * focal / aspect;
    let y = p[1] / z * focal;
    Some([(x * 0.5 + 0.5) * width, (1.0 - (y * 0.5 + 0.5)) * height, z])
}

pub fn viewmodel(spec: &gemini_sim::content::GloveSpec) -> Viewmodel {
    let mut vm = Viewmodel {
        teal: MeshData::new(),
        cobalt: MeshData::new(),
        white: MeshData::new(),
        accent: MeshData::new(),
        core: MeshData::new(),
        barrel: MeshData::new(),
    };
    let ink = [1.0, 1.0, 1.0, 1.0];
    glove(&mut vm, [0.2, -0.04, -0.08], 1.0, ink, spec);
    glove(&mut vm, [-0.06, -0.05, -0.5], -1.0, ink, spec);
    rifle(&mut vm, ink);
    vm
}

pub fn arms_and_gun(cobalt: Rgba, white: Rgba, accent: Rgba, energy: Rgba) -> MeshData {
    let glove = gemini_sim::content::Content::embedded().heroes_visual.glove;
    let vm = viewmodel(&glove);
    let teal = [0.12, 0.71, 0.66, 1.0];
    let mut m = MeshData::new();
    geom::append(&mut m, &recolor(&vm.teal, teal));
    geom::append(&mut m, &recolor(&vm.cobalt, cobalt.0));
    geom::append(&mut m, &recolor(&vm.white, white.0));
    geom::append(&mut m, &recolor(&vm.accent, accent.0));
    geom::append(&mut m, &recolor(&vm.core, energy.0));
    geom::append(&mut m, &recolor(&vm.barrel, cobalt.0));
    m
}

fn recolor(src: &MeshData, color: [f32; 4]) -> MeshData {
    let mut mesh = src.clone();
    for slot in &mut mesh.colors {
        *slot = color;
    }
    mesh
}

fn glove(vm: &mut Viewmodel, anchor: [f32; 3], side: f32, ink: [f32; 4], g: &gemini_sim::content::GloveSpec) {
    // Cobalt sleeve. Teal plates sit on the forearm; the glove and fingers are teal.
    geom::gem(
        &mut vm.cobalt,
        [anchor[0] + g.sleeve_at[0], anchor[1] + g.sleeve_at[1], anchor[2] + g.sleeve_at[2]],
        g.sleeve_radii,
        g.sleeve_slices,
        g.sleeve_stacks,
        ink,
    );
    geom::plate(
        &mut vm.teal,
        [anchor[0] + g.plate_at[0], anchor[1] + g.plate_at[1], anchor[2] + g.plate_at[2]],
        g.plate_length,
        g.plate_width,
        g.plate_arch,
        g.plate_nu,
        g.plate_nv,
        ink,
    );
    geom::gem(
        &mut vm.teal,
        [anchor[0] - side * g.hand_at[0], anchor[1] + g.hand_at[1], anchor[2] + g.hand_at[2]],
        g.hand_radii,
        g.hand_slices,
        g.hand_stacks,
        ink,
    );
    let center = (g.finger_count - 1) as f32 * 0.5;
    for i in 0..g.finger_count {
        let spread = (i as f32 - center) * g.finger_spread;
        spoke(
            &mut vm.teal,
            g.finger_length,
            g.finger_r0,
            g.finger_r1,
            g.finger_sides,
            ink,
            [anchor[0] + spread + g.finger_at[0], anchor[1] + g.finger_at[1], anchor[2] + g.finger_at[2]],
            g.finger_pitch,
            side * g.finger_yaw,
        );
    }
    spoke(
        &mut vm.teal,
        g.thumb_length,
        g.thumb_r0,
        g.thumb_r1,
        g.thumb_sides,
        ink,
        [anchor[0] - side * g.thumb_at[0], anchor[1] + g.thumb_at[1], anchor[2] + g.thumb_at[2]],
        g.thumb_pitch,
        -side * g.thumb_yaw,
    );
}

fn rifle(vm: &mut Viewmodel, ink: [f32; 4]) {
    // Chunkier receiver, a magazine under it, and a teal strip along the side.
    geom::gem(&mut vm.cobalt, [0.03, -0.02, -0.28], [0.12, 0.11, 0.28], 12, 8, ink);
    geom::plate(&mut vm.cobalt, [0.03, 0.07, -0.28], 0.5, 0.18, 0.04, 10, 6, ink);
    geom::plate(&mut vm.cobalt, [0.12, -0.01, -0.28], 0.46, 0.12, 0.035, 8, 5, ink);
    geom::box_mesh(&mut vm.cobalt, [0.03, -0.15, -0.26], [0.07, 0.15, 0.09], ink);
    // On the top face, toward the camera, so the strip is not buried in the receiver.
    geom::box_mesh(&mut vm.core, [0.03, 0.14, -0.24], [0.14, 0.02, 0.2], ink);
    geom::gem(&mut vm.core, [0.03, 0.15, -0.12], [0.04, 0.016, 0.05], 6, 4, ink);
    for i in 0..3 {
        let z = -0.18 - i as f32 * 0.12;
        geom::plate(&mut vm.cobalt, [0.03, 0.02, z], 0.12, 0.2, 0.04, 8, 6, ink);
    }
    spoke(
        &mut vm.barrel,
        0.52,
        0.048,
        0.034,
        10,
        ink,
        [0.03, -0.005, -0.52],
        std::f32::consts::FRAC_PI_2,
        0.0,
    );
    // Orange stripe along the top of the cobalt tube.
    spoke(
        &mut vm.accent,
        0.48,
        0.016,
        0.012,
        6,
        ink,
        [0.03, 0.028, -0.54],
        std::f32::consts::FRAC_PI_2,
        0.0,
    );
    spoke(
        &mut vm.cobalt,
        0.16,
        0.05,
        0.032,
        8,
        ink,
        [0.03, -0.06, -0.14],
        2.35,
        0.15,
    );
    for i in 0..4 {
        let z = -0.16 - i as f32 * 0.1;
        geom::plate(&mut vm.accent, [0.03, 0.1, z], 0.07, 0.16, 0.018, 6, 4, ink);
    }
}

fn spoke(
    mesh: &mut MeshData,
    length: f32,
    r0: f32,
    r1: f32,
    sides: u32,
    color: [f32; 4],
    at: [f32; 3],
    pitch: f32,
    yaw: f32,
) {
    let mut part = MeshData::new();
    geom::limb(&mut part, length, r0, r1, sides, color);
    geom::rotate_x(&mut part, pitch);
    geom::rotate_y(&mut part, yaw);
    geom::translate(&mut part, at);
    geom::append(mesh, &part);
}

pub fn gun_core(energy: Rgba) -> MeshData {
    let mut m = MeshData::new();
    geom::gem(&mut m, [0.18, -0.2, -0.62], [0.045, 0.045, 0.07], 6, 4, energy.0);
    m
}

pub fn tracer(color: Rgba) -> MeshData {
    let mut m = MeshData::new();
    geom::box_mesh(&mut m, [0.0, 0.0, 0.0], [0.04, 0.04, 0.35], color.0);
    m
}

pub fn pickup_beam(color: Rgba) -> MeshData {
    let mut m = MeshData::new();
    geom::box_mesh(&mut m, [0.0, 0.8, 0.0], [0.08, 1.6, 0.08], color.0);
    geom::gem(&mut m, [0.0, 0.35, 0.0], [0.16, 0.16, 0.16], 6, 4, color.0);
    m
}

pub fn rock(color: Rgba, scale: [f32; 3]) -> MeshData {
    let mut m = MeshData::new();
    geom::gem(&mut m, [0.0, scale[1] * 0.45, 0.0], scale, 5, 4, color.0);
    m
}

fn disc(mesh: &mut MeshData, center: [f32; 3], radius: f32, color: [f32; 4], sides: u32) {
    let base = mesh.positions.len() as u32;
    mesh.positions.push(center);
    mesh.normals.push([0.0, 1.0, 0.0]);
    mesh.colors.push(color);
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        mesh.positions.push([center[0] + a.cos() * radius, center[1], center[2] + a.sin() * radius]);
        mesh.normals.push([0.0, 1.0, 0.0]);
        mesh.colors.push(color);
    }
    for i in 0..sides {
        let n = (i + 1) % sides;
        mesh.indices.extend_from_slice(&[base, base + 1 + i, base + 1 + n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
        (a[0] - b[0]).abs() < tol && (a[1] - b[1]).abs() < tol && (a[2] - b[2]).abs() < tol
    }

    #[test]
    fn triangle_budgets_and_flat_normals() {
        let sw = bug("swarmer");
        let sp = bug("spitter");
        let br = bug("brute");
        let brood = bug("brood_mother");
        let hive = bug("hive_tyrant");
        let arms = arms_and_gun(
            Rgba([0.12, 0.25, 0.69, 1.0]),
            Rgba([0.95, 0.95, 0.95, 1.0]),
            Rgba([0.98, 0.45, 0.09, 1.0]),
            Rgba([0.18, 0.88, 0.82, 1.0]),
        );
        for visual in [sw, sp, br, brood, hive] {
            for part in &visual.parts {
                assert!(flat_shaded(&part.mesh), "{} {} smooth", visual.id, part.name);
            }
            assert!(flat_shaded(&visual.lod), "{} lod smooth", visual.id);
            assert!(
                visual.lod.tri_count() * 2 < visual.tri_count(),
                "{} lod {} vs {}",
                visual.id,
                visual.lod.tri_count(),
                visual.tri_count()
            );
        }
        assert!(sw.height < 0.7, "swarmer knee-high, got {}", sw.height);
        assert!((br.height - 1.8).abs() < 0.05, "brute shoulder, got {}", br.height);
        assert!((240..=480).contains(&sw.tri_count()), "swarmer {}", sw.tri_count());
        assert!((640..=1000).contains(&sp.tri_count()), "spitter {}", sp.tri_count());
        assert!((1200..=1800).contains(&br.tri_count()), "brute {}", br.tri_count());
        assert!(brood.tri_count() <= 8000 && brood.tri_count() > 600, "brood {}", brood.tri_count());
        assert!(hive.tri_count() <= 8000 && hive.tri_count() > 600, "hive {}", hive.tri_count());
        assert!((1800..=3600).contains(&arms.tri_count()), "arms {}", arms.tri_count());
        assert!(flat_shaded(&arms));
        let vm = viewmodel(&gemini_sim::content::Content::embedded().heroes_visual.glove);
        assert_eq!(vm.tri_count(), arms.tri_count());
        assert!(vm.accent.tri_count() > 40, "accent {}", vm.accent.tri_count());
        assert!(vm.core.tri_count() > 20, "core {}", vm.core.tri_count());
        assert!(vm.teal.tri_count() > 80 && vm.cobalt.tri_count() > 80);
        assert!(vm.barrel.tri_count() >= 40, "barrel {}", vm.barrel.tri_count());
    }

    #[test]
    fn anchors_follow_the_visual_table() {
        let sp = bug("spitter");
        let sac = sp.parts.iter().find(|p| p.name == "sac").expect("sac");
        assert!(near(sac.rest_pos, [0.06444810, 0.62343204, -0.29764500], 0.02), "{:?}", sac.rest_pos);
        assert!(sac.emissive);
        let gland = bug("brood_mother").parts.iter().find(|p| p.name == "gland").expect("gland");
        assert!(near(gland.rest_pos, [-0.01607708, 1.45470980, 1.18440960], 0.02));
        assert!(gland.emissive);
        let core = bug("hive_tyrant").parts.iter().find(|p| p.name == "core").expect("core");
        assert!(near(core.rest_pos, [0.04576531, 2.95827794, 0.92303598], 0.02));
        assert!(core.emissive);
        let armor = bug("hive_tyrant").parts.iter().find(|p| p.name == "armor").expect("armor");
        assert!(matches!(armor.gate, Gate::PhaseMax(1)));
    }

    #[test]
    fn gait_plants_both_tripod_sets() {
        let sw = bug("swarmer");
        let mut prev = Vec::new();
        let mut planted = Vec::new();
        let period = 1.0 / sw.bob_hz.max(0.2);
        for step in 0..48 {
            let t = step as f32 * period / 16.0;
            let posed = pose(
                sw,
                AnimInput {
                    time: t,
                    moving: true,
                    charging: false,
                    vulnerable: false,
                    phase: 0,
                    frenzy: false,
                    preview_glow: false,
                    regrow: 0,
                hatch_pod: 255,
                hatch_charge: 0.0,
                },
                &prev,
            );
            planted.extend(posed.planted.iter().copied());
            prev = posed.foot_lift;
        }
        assert!(planted.iter().any(|i| i % 2 == 0), "even feet {planted:?}");
        assert!(planted.iter().any(|i| i % 2 == 1), "odd feet {planted:?}");
    }

    #[test]
    fn charge_glow_cuts_when_the_telegraph_ends() {
        let sp = bug("spitter");
        let on = pose(
            sp,
            AnimInput {
                time: 0.2,
                moving: false,
                charging: true,
                vulnerable: false,
                phase: 0,
                frenzy: false,
                preview_glow: false,
                regrow: 0,
                hatch_pod: 255,
                hatch_charge: 0.0,
            },
            &[],
        );
        let off = pose(
            sp,
            AnimInput {
                time: 0.2,
                moving: false,
                charging: false,
                vulnerable: false,
                phase: 0,
                frenzy: false,
                preview_glow: false,
                regrow: 0,
                hatch_pod: 255,
                hatch_charge: 0.0,
            },
            &[],
        );
        let sac = sp.parts.iter().position(|p| p.name == "sac").unwrap();
        assert!(on.xforms[sac].glow > off.xforms[sac].glow + 0.5);
        assert!(on.xforms[sac].scale[0] > off.xforms[sac].scale[0]);
    }

    #[test]
    fn stills_include_each_glow_color() {
        for id in ["swarmer", "spitter", "brute", "brood_mother", "hive_tyrant"] {
            let rgb = raster_bug(id, 360, 360);
            let missing = missing_glow_keys(id, &rgb);
            assert!(missing.is_empty(), "{id} glow missing {missing:?}");
            assert!(painted_pixels(&rgb) > 80, "{id} still is blank");
        }
    }

    #[test]
    fn front_three_quarter_preview_is_not_blank() {
        let rgb = raster_bug("spitter", 96, 96);
        assert!(painted_pixels(&rgb) > 40, "spitter preview painted {}", painted_pixels(&rgb));
        let brute = raster_bug("brute", 96, 96);
        assert!(painted_pixels(&brute) > 40);
    }

    #[test]
    fn colors_and_brood_pods_follow_the_palette() {
        let sp = bug("spitter");
        assert_eq!(sp.parts.iter().find(|p| p.name == "thorax").unwrap().palette, "spitter_carapace");
        assert_eq!(sp.parts.iter().find(|p| p.name == "sac").unwrap().palette, "spitter_glow");
        assert!(sp.parts.iter().find(|p| p.name == "sac").unwrap().emissive);
        let brood = bug("brood_mother");
        assert_eq!(brood.parts.iter().find(|p| p.name == "sac").unwrap().palette, "brood_sac");
        assert_eq!(brood.parts.iter().filter(|p| p.name.starts_with("pod")).count(), 6);
        assert_eq!(brood.parts.iter().filter(|p| p.name.starts_with("bud")).count(), 6);
        let bud = brood.parts.iter().find(|p| p.name == "bud0").unwrap();
        assert!(!bud.emissive);
        assert_eq!(bud.palette, "brood_sac");
        let abd = bug("swarmer").parts.iter().find(|p| p.name == "abdomen").unwrap();
        assert!(abd.emissive);
        assert_eq!(abd.palette, "swarmer_glow");
        let posed = pose(
            brood,
            AnimInput {
                time: 0.2,
                moving: false,
                charging: false,
                vulnerable: false,
                phase: 0,
                frenzy: false,
                preview_glow: false,
                regrow: 2,
                hatch_pod: 255,
                hatch_charge: 0.0,
            },
            &[],
        );
        let b0 = brood.parts.iter().position(|p| p.name == "bud0").unwrap();
        let b3 = brood.parts.iter().position(|p| p.name == "bud3").unwrap();
        assert!(posed.xforms[b0].scale[0] > 2.0);
        assert!(posed.xforms[b0].glow > 1.5);
        assert!(posed.xforms[b3].scale[0] < 1.5);
        assert!(posed.xforms[b3].glow < 0.5);
    }

    #[test]
    fn open_brute_pods_and_tyrant_spikes() {
        let brute = bug("brute");
        let left = brute.parts.iter().position(|p| p.name == "shell_plates_l").unwrap();
        let right = brute.parts.iter().position(|p| p.name == "shell_plates_r").unwrap();
        let cracks = brute.parts.iter().position(|p| p.name == "cracks").unwrap();
        let core = brute.parts.iter().position(|p| p.name == "core").unwrap();
        let shut = pose(
            brute,
            AnimInput {
                time: 0.2,
                moving: false,
                charging: false,
                vulnerable: false,
                phase: 0,
                frenzy: false,
                preview_glow: false,
                regrow: 0,
                hatch_pod: 255,
                hatch_charge: 0.0,
            },
            &[],
        );
        let open = pose(
            brute,
            AnimInput {
                time: 0.2,
                moving: false,
                charging: false,
                vulnerable: true,
                phase: 0,
                frenzy: false,
                preview_glow: false,
                regrow: 0,
                hatch_pod: 255,
                hatch_charge: 0.0,
            },
            &[],
        );
        let shut_gap = shut.xforms[right].pos[0] - shut.xforms[left].pos[0];
        let open_gap = open.xforms[right].pos[0] - open.xforms[left].pos[0];
        assert!(shut_gap.abs() < 0.05, "shut plates share an origin, gap {shut_gap}");
        assert!(open_gap > shut_gap + 0.5, "open gap {open_gap} shut {shut_gap}");
        assert!(shut.xforms[cracks].visible);
        assert!(shut.xforms[cracks].glow < 1.0, "shut cracks should stay dim");
        assert!(!shut.xforms[core].visible);
        assert!(open.xforms[core].visible);
        assert!(open.xforms[core].glow > 3.0);

        let brood = bug("brood_mother");
        assert_eq!(brood.parts.iter().filter(|p| p.name.starts_with("pod")).count(), 6);
        let pod = brood.parts.iter().find(|p| p.name == "pod0").unwrap();
        let rad = pod
            .mesh
            .positions
            .iter()
            .map(|p| p[0].abs().max(p[1].abs()).max(p[2].abs()))
            .fold(0.0_f32, f32::max);
        assert!((rad - 0.52).abs() < 0.02, "pod radius {rad}");
        let posed = pose(
            brood,
            AnimInput {
                time: 0.2,
                moving: false,
                charging: false,
                vulnerable: false,
                phase: 0,
                frenzy: false,
                preview_glow: false,
                regrow: 0,
                hatch_pod: 255,
                hatch_charge: 0.0,
            },
            &[],
        );
        let p0 = brood.parts.iter().position(|p| p.name == "pod0").unwrap();
        assert!(posed.xforms[p0].glow > 2.0);
        let charged = pose(
            brood,
            AnimInput {
                time: 0.2,
                moving: false,
                charging: false,
                vulnerable: false,
                phase: 0,
                frenzy: false,
                preview_glow: false,
                regrow: 0,
                hatch_pod: 0,
                hatch_charge: 1.0,
            },
            &[],
        );
        assert!(charged.xforms[p0].glow > posed.xforms[p0].glow + 1.0);

        assert!(
            bug("hive_tyrant").parts.iter().all(|part| !part.name.starts_with("vein")),
            "hive lime veins are leftover glow geometry, not projectiles"
        );
    }

    #[test]
    fn muzzle_flash_sits_about_five_pixels_onto_the_barrel() {
        let content = gemini_sim::content::Content::embedded();
        let hero = &content.hero;
        let local = muzzle_local();
        let tip = [
            hero.viewmodel_offset[0] + local[0],
            hero.viewmodel_offset[1] + local[1],
            hero.viewmodel_offset[2] + local[2],
        ];
        let moved = muzzle_on_barrel(tip, hero.viewmodel_fov, 1280.0, 720.0, hero.muzzle_flash_tip_px);
        assert!(moved[2] > tip[2], "the glow moves back onto the barrel");
        let a = project_view(tip, hero.viewmodel_fov, 1280.0, 720.0).unwrap();
        let b = project_view(moved, hero.viewmodel_fov, 1280.0, 720.0).unwrap();
        let dist = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        assert!((dist - hero.muzzle_flash_tip_px).abs() < 0.35, "screen shift {dist}");
        let frame = ViewmodelFrame {
            accent: [1.0, 0.45, 0.1, 1.0],
            offset: hero.viewmodel_offset,
            fov: hero.viewmodel_fov,
            overdrive: true,
            glow_radius: hero.muzzle_flash_radius,
            glow_core: hero.muzzle_flash_core,
            glow_falloff: hero.muzzle_flash_falloff,
            glow_tip_px: hero.muzzle_flash_tip_px,
            glow_color: [
                hero.muzzle_flash_color[0],
                hero.muzzle_flash_color[1],
                hero.muzzle_flash_color[2],
                1.0,
            ],
        };
        let rgb = raster_viewmodel(1280, 720, &frame);
        let cx = b[0].round() as i32;
        let cy = b[1].round() as i32;
        let at = |x: i32, y: i32| {
            let i = ((y as u32 * 1280) + x as u32) as usize * 3;
            [rgb[i], rgb[i + 1], rgb[i + 2]]
        };
        let core = at(cx, cy);
        assert!(core[0] > 240 && core[1] > 220, "bright core {core:?} at {cx},{cy}");
        let halo = at(cx + 28, cy);
        assert!(halo[0] > 160 && halo[2] < 160, "soft halo {halo:?}");
        assert!(core[0] as i32 + core[1] as i32 > halo[0] as i32 + halo[1] as i32);
        // Shots leave the camera along -Z and hit the crosshair. The bore is that axis.
        let mesh_tip = muzzle_tip(cast("vex_gun"));
        let radial = (mesh_tip[0] * mesh_tip[0] + mesh_tip[1] * mesh_tip[1]).sqrt();
        assert!(
            mesh_tip[2] < -0.2 && radial < mesh_tip[2].abs() * 0.25,
            "muzzle {mesh_tip:?} is off the shot axis"
        );
    }

    /// The pre-swap rifle covered 63563 idle pixels and left the crosshair and the
    /// lower center empty. The cast gun must not cover more.
    #[test]
    fn vex_gun_covers_no_more_screen_than_the_old_rifle() {
        let hero = &gemini_sim::content::Content::embedded().hero;
        let frame = ViewmodelFrame {
            accent: [1.0, 0.45, 0.1, 1.0],
            offset: hero.viewmodel_offset,
            fov: hero.viewmodel_fov,
            overdrive: false,
            glow_radius: hero.muzzle_flash_radius,
            glow_core: hero.muzzle_flash_core,
            glow_falloff: hero.muzzle_flash_falloff,
            glow_tip_px: hero.muzzle_flash_tip_px,
            glow_color: [1.0, 0.72, 0.28, 1.0],
        };
        let rgb = raster_viewmodel(1280, 720, &frame);
        let bg = [0x8Au8, 0x8A, 0x8A];
        let mut covered = 0usize;
        let mut cross = 0usize;
        let mut lower = 0usize;
        for y in 0..720u32 {
            for x in 0..1280u32 {
                let i = ((y * 1280 + x) * 3) as usize;
                if rgb[i..i + 3] == bg {
                    continue;
                }
                covered += 1;
                if (x as i32 - 640).abs() <= 24 && (y as i32 - 360).abs() <= 24 {
                    cross += 1;
                }
                if (448..832).contains(&x) && y >= 432 {
                    lower += 1;
                }
            }
        }
        assert!(covered <= 63_563, "idle coverage {covered} px exceeds the old rifle");
        assert_eq!(cross, 0, "crosshair is covered");
        assert_eq!(lower, 0, "lower center is covered");
    }

    #[test]
    fn catalog_is_shared() {
        let a = bug("swarmer") as *const EnemyVisual;
        let b = bug("swarmer") as *const EnemyVisual;
        assert_eq!(a, b);
    }

    #[test]
    fn brood_pods_alternate_with_buds_and_sit_on_hitboxes() {
        let content = gemini_sim::Content::embedded();
        let boss = content.boss("brood_mother").expect("brood mother");
        let visual = bug("brood_mother");
        let row = content.bug(gemini_sim::content::EnemyKind::BroodMother);
        let glowing: Vec<_> = visual
            .parts
            .iter()
            .filter(|part| part.emissive && part.name.starts_with("pod"))
            .collect();
        assert_eq!(
            glowing.len() as u32, boss.pod_count,
            "glowing pod parts must match bosses.toml pod_count"
        );
        let authored_buds = row
            .visual
            .parts
            .iter()
            .find(|part| part.name == "bud")
            .expect("visual bud row")
            .count;
        let buds: Vec<_> = visual.parts.iter().filter(|part| part.name.starts_with("bud")).collect();
        assert_eq!(buds.len() as u32, authored_buds, "bud parts must match the visual bud count");
        assert!(buds.iter().all(|part| !part.emissive), "buds stay visual-only");

        assert_eq!(row.hitbox.pods.len(), glowing.len());
        for (i, place) in row.hitbox.pods.iter().enumerate() {
            let part = visual
                .parts
                .iter()
                .find(|part| part.name == format!("pod{i}"))
                .unwrap_or_else(|| panic!("missing pod{i}"));
            assert_eq!(part.pod_slot, Some(i as u32));
            let anchor = part.rest_pos;
            let hit = place.offset;
            let d = ((anchor[0] - hit[0]).powi(2) + (anchor[1] - hit[1]).powi(2) + (anchor[2] - hit[2]).powi(2)).sqrt();
            assert!(
                d < 1e-3,
                "pod{i} hitbox {hit:?} is off its mesh anchor {anchor:?} by {d}"
            );
        }

        let sac = visual.parts.iter().find(|part| part.name == "sac").expect("sac").rest_pos;
        let mut ring = Vec::new();
        for part in visual.parts.iter() {
            let live = part.emissive && part.name.starts_with("pod");
            let bud = part.name.starts_with("bud");
            if !live && !bud {
                continue;
            }
            let dx = part.rest_pos[0] - sac[0];
            let dz = part.rest_pos[2] - sac[2];
            ring.push((dz.atan2(dx), live));
        }
        ring.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert_eq!(ring.len(), glowing.len() + buds.len());
        for pair in ring.windows(2) {
            assert_ne!(pair[0].1, pair[1].1, "pods and buds must alternate around the sac");
            assert!(pair[1].0 - pair[0].0 > 0.2, "ring gap {}", pair[1].0 - pair[0].0);
        }
        assert_ne!(ring.first().unwrap().1, ring.last().unwrap().1);

        let pod = visual.parts.iter().find(|part| part.name == "pod0").unwrap();
        let mesh_r = mesh_radius(&pod.mesh);
        let hit_r = row.hitbox.pod_radius().expect("pod radius");
        assert!(
            hit_r + 1e-3 >= mesh_r * content.bugs.hitbox_margin,
            "pod hitbox {hit_r:.3} < mesh {mesh_r:.3} * {}",
            content.bugs.hitbox_margin
        );
        eprintln!("pod mesh radius {mesh_r:.4} hitbox radius {hit_r:.4}");
    }

    #[test]
    fn muzzle_glow_is_a_round_sphere() {
        let content = gemini_sim::Content::embedded();
        let radius = content.hero.muzzle_flash_radius;
        assert!(radius > 0.0, "muzzle glow radius comes from characters.toml");
        let mesh = muzzle_glow(radius);
        let mut extent = [0.0f32; 3];
        for p in &mesh.positions {
            for axis in 0..3 {
                extent[axis] = extent[axis].max(p[axis].abs());
            }
        }
        for axis in extent {
            assert!((axis - radius).abs() < 1e-4, "extent {extent:?} radius {radius}");
        }
        let flat = extent[0].max(extent[1]).max(extent[2]) - extent[0].min(extent[1]).min(extent[2]);
        assert!(flat < 1e-4, "muzzle glow is a flat disc {extent:?}");
    }

    #[test]
    fn weakpoint_hitboxes_cover_the_glow_mesh() {
        let text = include_str!("../../../content/bugs.toml");
        let root: toml::Value = toml::from_str(text).expect("bugs.toml");
        let margin = root
            .get("hitbox_margin")
            .and_then(|v| v.as_float())
            .expect("hitbox_margin") as f32;
        assert!((margin - 1.1).abs() < 1e-6, "hitbox_margin {margin}");
        for visual in catalog() {
            let hit = root
                .get(&visual.id)
                .and_then(|row| row.get("hitbox"))
                .unwrap_or_else(|| panic!("{} hitbox", visual.id));
            for key in ["weakpoint", "core", "cannon", "tail"] {
                let Some(shape) = hit.get(key) else { continue };
                let Some(anchor) = shape
                    .get("anchor")
                    .and_then(|v| v.as_str())
                    .or_else(|| shape.get("name").and_then(|v| v.as_str()))
                else {
                    continue;
                };
                let Some(part) = visual.parts.iter().find(|p| p.name == anchor) else {
                    continue;
                };
                if !part.emissive {
                    continue;
                }
                let mesh_r = mesh_radius(&part.mesh);
                let hit_r = shape_radius(shape);
                assert!(
                    hit_r + 1e-3 >= mesh_r * margin,
                    "{} {key} hitbox {hit_r:.3} < mesh {mesh_r:.3} * {margin}",
                    visual.id
                );
            }
            if let Some(shape) = hit.get("pod") {
                let part = visual
                    .parts
                    .iter()
                    .find(|p| p.name == "pod0")
                    .unwrap_or_else(|| panic!("{} pod mesh", visual.id));
                let mesh_r = mesh_radius(&part.mesh);
                let hit_r = shape_radius(shape);
                assert!(
                    hit_r + 1e-3 >= mesh_r * margin,
                    "{} pod hitbox {hit_r:.3} < mesh {mesh_r:.3} * {margin}",
                    visual.id
                );
            }
        }
    }

    /// Wall clock for one software raster of 300 swarmers. That is the headless
    /// stand-in for a client frame. It is not run in CI.
    /// `cargo test -p gemini-meshes --release -- --ignored --nocapture crowd_frame`
    #[test]
    #[ignore = "one-time release measurement of a software frame with 300 bugs"]
    fn crowd_frame_time_release_at_three_hundred_bugs() {
        use std::time::Instant;
        let cap = gemini_sim::content::Content::embedded().waves.budget.live_enemy_cap as usize;
        assert_eq!(cap, 300);
        let start = Instant::now();
        let rgb = raster_crowd(cap, 960, 540);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let painted = painted_pixels(&rgb);
        eprintln!("release software frame, 300 swarmers, 960x540: {ms:.1} ms, {painted} painted px");
        assert!(painted > 1000, "the crowd frame is blank ({painted} px)");
    }

    fn mesh_radius(mesh: &MeshData) -> f32 {
    mesh.positions
        .iter()
        .map(|p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt())
        .fold(0.0, f32::max)
}

fn shape_radius(shape: &toml::Value) -> f32 {
    let kind = shape.get("shape").and_then(|v| v.as_str()).unwrap_or("sphere");
    if kind == "box" {
        let halves = shape
            .get("half_extents")
            .and_then(|v| v.as_array())
            .expect("box half_extents");
        let sum: f64 = halves.iter().map(|v| v.as_float().unwrap_or(0.0).powi(2)).sum();
        return sum.sqrt() as f32;
    }
    shape.get("radius").and_then(|v| v.as_float()).unwrap_or(0.0) as f32
    }
}
