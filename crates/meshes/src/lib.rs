//! Procedural placeholder meshes. One shared mesh set per bug type.
//! Colors are filled in by the client from `content/palette.toml`.

mod geom;
mod pose;
mod preview;
mod visual;

pub use pose::{pose, AnimInput, Pose, Xform};
pub use preview::{missing_glow_keys, painted_pixels, raster, raster_bug, raster_crown, still, still_open};
pub use visual::{bug, catalog, flat_shaded, EnemyVisual, Gate, Joint, Part, ShotCamera};

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

pub fn arms_and_gun(cobalt: Rgba, white: Rgba, accent: Rgba, _energy: Rgba) -> MeshData {
    let mut m = MeshData::new();
    geom::gem(&mut m, [0.22, -0.16, -0.02], [0.07, 0.18, 0.07], 10, 8, cobalt.0);
    geom::gem(&mut m, [0.24, -0.28, -0.22], [0.06, 0.14, 0.06], 10, 8, cobalt.0);
    geom::gem(&mut m, [0.26, -0.34, -0.4], [0.09, 0.07, 0.08], 8, 6, white.0);
    for i in 0..8 {
        let z = -0.15 - i as f32 * 0.08;
        geom::plate(
            &mut m,
            [0.18, -0.2, z],
            0.1,
            0.16,
            0.05,
            3,
            2,
            if i % 3 == 0 { accent.0 } else { cobalt.0 },
        );
    }
    let mut barrel = MeshData::new();
    geom::limb(&mut barrel, 0.42, 0.035, 0.02, 8, white.0);
    geom::rotate_x(&mut barrel, -std::f32::consts::FRAC_PI_2);
    geom::translate(&mut barrel, [0.18, -0.18, -0.55]);
    geom::append(&mut m, &barrel);
    for i in 0..6 {
        let mut ring = MeshData::new();
        geom::limb(&mut ring, 0.04, 0.07, 0.07, 8, accent.0);
        geom::rotate_x(&mut ring, -std::f32::consts::FRAC_PI_2);
        geom::translate(&mut ring, [0.18, -0.16, -0.48 - i as f32 * 0.06]);
        geom::append(&mut m, &ring);
    }
    // Faceted receiver plates keep the viewmodel near the 3000-tri budget.
    for i in 0..26 {
        let z = -0.05 - (i % 7) as f32 * 0.07;
        let y = -0.1 - (i / 7) as f32 * 0.055;
        geom::gem(&mut m, [0.18, y, z], [0.045, 0.03, 0.045], 8, 5, cobalt.0);
    }
    m
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
    }

    #[test]
    fn anchors_follow_the_visual_table() {
        let sp = bug("spitter");
        let sac = sp.parts.iter().find(|p| p.name == "sac").expect("sac");
        assert!(near(sac.rest_pos, [0.0, 1.02, -0.02], 0.02), "{:?}", sac.rest_pos);
        assert!(sac.emissive);
        let gland = bug("brood_mother").parts.iter().find(|p| p.name == "gland").expect("gland");
        assert!(near(gland.rest_pos, [0.0, 0.9, 2.0], 0.02));
        assert!(gland.emissive);
        let core = bug("hive_tyrant").parts.iter().find(|p| p.name == "core").expect("core");
        assert!(near(core.rest_pos, [0.0, 3.4, 1.0], 0.02));
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
    fn catalog_is_shared() {
        let a = bug("swarmer") as *const EnemyVisual;
        let b = bug("swarmer") as *const EnemyVisual;
        assert_eq!(a, b);
    }
}
