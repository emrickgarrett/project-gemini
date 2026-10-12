//! Arena dressing. Radii and heights come from arena_01. Tessellation and rocks
//! come from arena_visual.toml. Nothing here is a collider.

use crate::geom;
use crate::MeshData;
use gemini_sim::content::Content;

const INK: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

pub struct ArenaPiece {
    pub mesh: MeshData,
    pub palette: String,
}

struct Visual {
    deck_sides: u32,
    rail_posts: u32,
    rail_thickness: f32,
    review_stand_gap: f32,
    rocks: Vec<Rock>,
}

struct Rock {
    at: [f32; 2],
    size: [f32; 3],
}

pub fn review_stand_gap() -> f32 {
    visual().review_stand_gap
}

pub fn pieces() -> Vec<ArenaPiece> {
    let content = Content::embedded();
    let vis = visual();
    let platform = &content.arena.platform;
    let mut out = Vec::new();
    let mut deck = MeshData::new();
    cylinder(
        &mut deck,
        [platform.center.x, platform.height * 0.5, platform.center.z],
        platform.radius,
        platform.height,
        vis.deck_sides,
    );
    out.push(ArenaPiece { mesh: deck, palette: "arena_light".into() });
    out.extend(rails(
        &vis,
        [platform.center.x, platform.center.z],
        platform.radius,
        platform.height,
        platform.railing_height,
        &platform.ramps,
    ));
    for ramp in &platform.ramps {
        out.push(ArenaPiece {
            mesh: ramp_wedge([ramp.from.x, ramp.from.z], [ramp.to.x, ramp.to.z], ramp.width, platform.height),
            palette: "arena_light".into(),
        });
    }
    for rock in &vis.rocks {
        let mut mesh = MeshData::new();
        let radii = [rock.size[0] * 0.5, rock.size[1] * 0.5, rock.size[2] * 0.5];
        geom::gem(&mut mesh, [rock.at[0], radii[1], rock.at[1]], radii, 6, 4, INK);
        out.push(ArenaPiece { mesh, palette: "arena_dark".into() });
    }
    out
}

/// Ground disc used by the in-arena still. The client keeps its own ground box.
pub fn review_ground() -> MeshData {
    let content = Content::embedded();
    let mut mesh = MeshData::new();
    disc(&mut mesh, [0.0, 0.02, 0.0], content.arena.radius, visual().deck_sides);
    mesh
}

fn visual() -> Visual {
    let text = include_str!("../../../content/arena_visual.toml");
    let root: toml::Value = toml::from_str(text).unwrap_or_else(|err| panic!("arena_visual.toml: {err}"));
    let table = root.as_table().unwrap_or_else(|| panic!("arena_visual.toml is not a table"));
    expect_keys(table, &["deck_sides", "rail_posts", "rail_thickness", "review_stand_gap", "rock"], "arena_visual.toml");
    let deck_sides = req_u32(table, "deck_sides");
    let rail_posts = req_u32(table, "rail_posts");
    let rail_thickness = req_f32(table, "rail_thickness");
    let review_stand_gap = req_f32(table, "review_stand_gap");
    if deck_sides < 8 || rail_posts < 8 || !(rail_thickness > 0.0) || !(review_stand_gap > 0.0) {
        panic!("arena_visual.toml deck, rail, or stand gap is out of range");
    }
    let rows = table.get("rock").and_then(|v| v.as_array()).unwrap_or_else(|| panic!("arena_visual.toml is missing [[rock]]"));
    let content = Content::embedded();
    let boss = &content.arena.boss_area;
    let mut rocks = Vec::new();
    for row in rows {
        let map = row.as_table().unwrap_or_else(|| panic!("rock is not a table"));
        expect_keys(map, &["at", "size"], "rock");
        let at = req_vec2(map, "at");
        let size = req_vec3(map, "size");
        if size.iter().any(|n| !(*n > 0.0)) {
            panic!("rock size must be > 0");
        }
        let dx = at[0] - boss.center.x;
        let dz = at[1] - boss.center.z;
        if (dx * dx + dz * dz).sqrt() <= boss.radius {
            panic!("rock at {at:?} sits in the boss area");
        }
        let deck = content.arena.platform.radius;
        if (at[0] * at[0] + at[1] * at[1]).sqrt() <= deck {
            panic!("rock at {at:?} sits on the platform");
        }
        rocks.push(Rock { at, size });
    }
    Visual { deck_sides, rail_posts, rail_thickness, review_stand_gap, rocks }
}

fn rails(
    vis: &Visual,
    center: [f32; 2],
    radius: f32,
    deck_top: f32,
    rail_h: f32,
    ramps: &[gemini_sim::content::Ramp],
) -> Vec<ArenaPiece> {
    let mut mesh = MeshData::new();
    let thick = vis.rail_thickness;
    for i in 0..vis.rail_posts {
        let ang = i as f32 / vis.rail_posts as f32 * std::f32::consts::TAU;
        let dir = [ang.cos(), ang.sin()];
        if ramps.iter().any(|ramp| near_ramp(center, dir, ramp)) {
            continue;
        }
        let x = center[0] + dir[0] * (radius - thick * 0.5);
        let z = center[1] + dir[1] * (radius - thick * 0.5);
        let mut post = MeshData::new();
        geom::box_mesh(&mut post, [x, deck_top + rail_h * 0.5, z], [thick, rail_h, thick], INK);
        for n in &mut post.normals {
            n[0] = -n[0];
            n[1] = -n[1];
            n[2] = -n[2];
        }
        geom::append(&mut mesh, &post);
    }
    vec![ArenaPiece { mesh, palette: "arena_light".into() }]
}

fn near_ramp(center: [f32; 2], dir: [f32; 2], ramp: &gemini_sim::content::Ramp) -> bool {
    let mouth = [ramp.from.x - center[0], ramp.from.z - center[1]];
    let len = (mouth[0] * mouth[0] + mouth[1] * mouth[1]).sqrt().max(1e-4);
    let dot = (dir[0] * mouth[0] + dir[1] * mouth[1]) / len;
    dot > 0.86
}

fn ramp_wedge(from: [f32; 2], to: [f32; 2], width: f32, height: f32) -> MeshData {
    let mut mesh = MeshData::new();
    let dx = to[0] - from[0];
    let dz = to[1] - from[1];
    let len = (dx * dx + dz * dz).sqrt().max(1e-4);
    let side = [-dz / len * width * 0.5, dx / len * width * 0.5];
    let hi = |sign: f32, y: f32| [from[0] + side[0] * sign, y, from[1] + side[1] * sign];
    let lo = |sign: f32| [to[0] + side[0] * sign, 0.0, to[1] + side[1] * sign];
    let a0 = hi(1.0, height);
    let b0 = hi(-1.0, height);
    let a0g = hi(1.0, 0.0);
    let b0g = hi(-1.0, 0.0);
    let a1 = lo(1.0);
    let b1 = lo(-1.0);
    geom::add_quad(&mut mesh, a0, b0, b1, a1, INK);
    geom::add_quad(&mut mesh, a0, a0g, b0g, b0, INK);
    geom::add_quad(&mut mesh, a0g, a1, b1, b0g, INK);
    geom::add_face(&mut mesh, a0, a1, a0g, INK);
    geom::add_face(&mut mesh, b0, b0g, b1, INK);
    mesh
}

fn cylinder(mesh: &mut MeshData, center: [f32; 3], radius: f32, height: f32, sides: u32) {
    let y0 = center[1] - height * 0.5;
    let y1 = center[1] + height * 0.5;
    let mut top = Vec::new();
    let mut bot = Vec::new();
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        top.push([center[0] + c * radius, y1, center[2] + s * radius]);
        bot.push([center[0] + c * radius, y0, center[2] + s * radius]);
    }
    for i in 0..sides as usize {
        let j = (i + 1) % sides as usize;
        geom::add_quad(mesh, top[i], bot[i], bot[j], top[j], INK);
        geom::add_face(mesh, [center[0], y1, center[2]], top[j], top[i], INK);
        geom::add_face(mesh, [center[0], y0, center[2]], bot[i], bot[j], INK);
    }
}

fn disc(mesh: &mut MeshData, center: [f32; 3], radius: f32, sides: u32) {
    let mut ring = Vec::new();
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        ring.push([center[0] + c * radius, center[1], center[2] + s * radius]);
    }
    for i in 0..sides as usize {
        let j = (i + 1) % sides as usize;
        geom::add_face(mesh, center, ring[i], ring[j], INK);
    }
}

fn expect_keys(table: &toml::map::Map<String, toml::Value>, allowed: &[&str], what: &str) {
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            panic!("{what} has unknown key {key}");
        }
    }
    for key in allowed {
        if !table.contains_key(*key) {
            panic!("{what} is missing {key}");
        }
    }
}

fn req_f32(table: &toml::map::Map<String, toml::Value>, key: &str) -> f32 {
    let n = table
        .get(key)
        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        .unwrap_or_else(|| panic!("arena_visual.toml is missing {key}")) as f32;
    if !n.is_finite() {
        panic!("arena_visual.toml {key} is not finite");
    }
    n
}

fn req_u32(table: &toml::map::Map<String, toml::Value>, key: &str) -> u32 {
    table
        .get(key)
        .and_then(|v| v.as_integer())
        .filter(|n| *n >= 0)
        .unwrap_or_else(|| panic!("arena_visual.toml is missing {key}")) as u32
}

fn req_vec2(table: &toml::map::Map<String, toml::Value>, key: &str) -> [f32; 2] {
    let v = req_nums(table, key);
    if v.len() != 2 {
        panic!("{key} must have 2 numbers");
    }
    [v[0], v[1]]
}

fn req_vec3(table: &toml::map::Map<String, toml::Value>, key: &str) -> [f32; 3] {
    let v = req_nums(table, key);
    if v.len() != 3 {
        panic!("{key} must have 3 numbers");
    }
    [v[0], v[1], v[2]]
}

fn req_nums(table: &toml::map::Map<String, toml::Value>, key: &str) -> Vec<f32> {
    let arr = table.get(key).and_then(|v| v.as_array()).unwrap_or_else(|| panic!("missing {key}"));
    arr.iter()
        .map(|value| {
            let n = value.as_float().or_else(|| value.as_integer().map(|i| i as f64)).unwrap_or_else(|| panic!("{key}")) as f32;
            if !n.is_finite() {
                panic!("{key} is not finite");
            }
            n
        })
        .collect()
}
