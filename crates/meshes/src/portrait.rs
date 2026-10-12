//! Full-body hero cards. Geometry comes from `content/heroes_visual.toml`.
//! Framing (yaw, pitch, fit) stays on the portrait row in characters.toml.

use std::sync::OnceLock;

use crate::geom;
use crate::MeshData;

pub struct PortraitPart {
    pub mesh: MeshData,
    pub palette: String,
    pub glow: bool,
}

const INK: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

struct Body {
    id: String,
    place: [f32; 3],
    parts: Vec<Spec>,
}

struct Spec {
    shape: String,
    size: [f32; 3],
    radii: [f32; 3],
    slices: u32,
    stacks: u32,
    length: f32,
    r0: f32,
    r1: f32,
    sides: u32,
    at: [f32; 3],
    yaw_deg: f32,
    pitch_deg: f32,
    color: String,
    glow: bool,
}

pub fn build(mesh_id: &str) -> Option<Vec<PortraitPart>> {
    let book = book();
    let body = book.iter().find(|body| body.id == mesh_id)?;
    Some(assemble(body))
}

#[cfg(test)]
pub fn ids() -> Vec<String> {
    book().iter().map(|body| body.id.clone()).collect()
}

/// Yaw about Y, then pitch about X. Zero of both leaves the vertices unchanged.
pub fn turn_mesh(mesh: &MeshData, yaw_deg: f32, pitch_deg: f32) -> MeshData {
    let (ys, yc) = yaw_deg.to_radians().sin_cos();
    let (ps, pc) = pitch_deg.to_radians().sin_cos();
    let mut mesh = mesh.clone();
    let spin = |x: f32, y: f32, z: f32| {
        let x1 = yc * x + ys * z;
        let z1 = -ys * x + yc * z;
        let y2 = pc * y - ps * z1;
        let z2 = ps * y + pc * z1;
        [x1, y2, z2]
    };
    for p in &mut mesh.positions {
        *p = spin(p[0], p[1], p[2]);
    }
    for n in &mut mesh.normals {
        *n = spin(n[0], n[1], n[2]);
    }
    mesh
}

/// Yaw, then pitch, about the body's own bounds center. The portrait row in
/// characters.toml is what frames the card; turning about the camera origin
/// would swing the body out of that frame.
pub fn turn(parts: &[PortraitPart], yaw_deg: f32, pitch_deg: f32) -> Vec<PortraitPart> {
    let center = pivot(parts);
    parts
        .iter()
        .map(|part| {
            let mut mesh = part.mesh.clone();
            for p in &mut mesh.positions {
                p[0] -= center[0];
                p[1] -= center[1];
                p[2] -= center[2];
            }
            let mut mesh = turn_mesh(&mesh, yaw_deg, pitch_deg);
            for p in &mut mesh.positions {
                p[0] += center[0];
                p[1] += center[1];
                p[2] += center[2];
            }
            PortraitPart {
                mesh,
                palette: part.palette.clone(),
                glow: part.glow,
            }
        })
        .collect()
}

fn pivot(parts: &[PortraitPart]) -> [f32; 3] {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for part in parts {
        for p in &part.mesh.positions {
            for axis in 0..3 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
    }
    [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ]
}

fn book() -> &'static [Body] {
    static CELL: OnceLock<Vec<Body>> = OnceLock::new();
    CELL.get_or_init(|| parse(include_str!("../../../content/heroes_visual.toml")))
}

fn parse(text: &str) -> Vec<Body> {
    let root: toml::Value = toml::from_str(text).unwrap_or_else(|err| panic!("heroes_visual.toml: {err}"));
    let table = root.as_table().unwrap_or_else(|| panic!("heroes_visual.toml is not a table"));
    expect_keys(table, &["mesh"], "heroes_visual.toml");
    let rows = table
        .get("mesh")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("heroes_visual.toml is missing [[mesh]]"));
    if rows.is_empty() {
        panic!("heroes_visual.toml has no [[mesh]]");
    }
    let mut bodies: Vec<Body> = Vec::new();
    for row in rows {
        let map = row.as_table().unwrap_or_else(|| panic!("heroes_visual mesh is not a table"));
        expect_keys(map, &["id", "place", "parts"], "heroes_visual mesh");
        let id = req_str(map, "id");
        if id.is_empty() || bodies.iter().any(|body| body.id == id) {
            panic!("heroes_visual duplicate or empty mesh id \"{id}\"");
        }
        let place = req_vec3(map, "place");
        let parts_v = map.get("parts").and_then(|v| v.as_array()).unwrap_or_else(|| panic!("{id} is missing parts"));
        if parts_v.is_empty() {
            panic!("{id} has no parts");
        }
        let mut parts = Vec::new();
        for part in parts_v {
            let spec = part.as_table().unwrap_or_else(|| panic!("{id} part is not a table"));
            let shape = req_str(spec, "shape").to_string();
            let keys: &[&str] = match shape.as_str() {
                "box" => &["shape", "size", "at", "yaw_deg", "pitch_deg", "color", "glow"],
                "gem" => &["shape", "radii", "slices", "stacks", "at", "yaw_deg", "pitch_deg", "color", "glow"],
                "limb" => &["shape", "length", "r0", "r1", "sides", "at", "yaw_deg", "pitch_deg", "color", "glow"],
                other => panic!("{id} shape \"{other}\" is unknown"),
            };
            expect_keys(spec, keys, &format!("{id} {shape}"));
            let color = req_str(spec, "color").to_string();
            if color != "accent" && gemini_sim::content::Content::embedded().palette.hex(&color).is_none() {
                panic!("{id} color \"{color}\" is not a palette key");
            }
            let (size, radii, slices, stacks, length, r0, r1, sides) = match shape.as_str() {
                "box" => {
                    let size = req_vec3(spec, "size");
                    if size.iter().any(|n| !(*n > 0.0)) {
                        panic!("{id} box size must be > 0");
                    }
                    (size, [0.0; 3], 0, 0, 0.0, 0.0, 0.0, 0)
                }
                "gem" => {
                    let radii = req_vec3(spec, "radii");
                    let slices = req_u32(spec, "slices");
                    let stacks = req_u32(spec, "stacks");
                    if radii.iter().any(|n| !(*n > 0.0)) || slices < 3 || stacks < 2 {
                        panic!("{id} gem radii, slices, and stacks are out of range");
                    }
                    ([0.0; 3], radii, slices, stacks, 0.0, 0.0, 0.0, 0)
                }
                "limb" => {
                    let length = req_f32(spec, "length");
                    let r0 = req_f32(spec, "r0");
                    let r1 = req_f32(spec, "r1");
                    let sides = req_u32(spec, "sides");
                    if !(length > 0.0 && r0 > 0.0 && r1 > 0.0) || sides < 3 {
                        panic!("{id} limb length, radii, and sides are out of range");
                    }
                    ([0.0; 3], [0.0; 3], 0, 0, length, r0, r1, sides)
                }
                _ => unreachable!(),
            };
            parts.push(Spec {
                shape,
                size,
                radii,
                slices,
                stacks,
                length,
                r0,
                r1,
                sides,
                at: req_vec3(spec, "at"),
                yaw_deg: req_f32(spec, "yaw_deg"),
                pitch_deg: req_f32(spec, "pitch_deg"),
                color,
                glow: req_bool(spec, "glow"),
            });
        }
        bodies.push(Body { id: id.to_string(), place, parts });
    }
    bodies
}

fn assemble(body: &Body) -> Vec<PortraitPart> {
    let mut out = Vec::new();
    for spec in &body.parts {
        let mut mesh = MeshData::new();
        match spec.shape.as_str() {
            "box" => outward_box(&mut mesh, spec.size),
            "gem" => geom::gem(&mut mesh, [0.0, 0.0, 0.0], spec.radii, spec.slices, spec.stacks, INK),
            "limb" => geom::limb(&mut mesh, spec.length, spec.r0, spec.r1, spec.sides, INK),
            other => panic!("{} shape \"{other}\" is unknown", body.id),
        }
        let mut mesh = turn_mesh(&mesh, spec.yaw_deg, spec.pitch_deg);
        geom::translate(&mut mesh, spec.at);
        geom::translate(&mut mesh, body.place);
        out.push(PortraitPart {
            mesh,
            palette: spec.color.clone(),
            glow: spec.glow,
        });
    }
    out
}

/// `box_mesh` winds inward, so the camera-facing side is unlit. Flip those normals.
fn outward_box(mesh: &mut MeshData, size: [f32; 3]) {
    let mut tmp = MeshData::new();
    geom::box_mesh(&mut tmp, [0.0, 0.0, 0.0], size, INK);
    for n in &mut tmp.normals {
        n[0] = -n[0];
        n[1] = -n[1];
        n[2] = -n[2];
    }
    geom::append(mesh, &tmp);
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

fn req_str<'a>(table: &'a toml::map::Map<String, toml::Value>, key: &str) -> &'a str {
    table.get(key).and_then(|v| v.as_str()).unwrap_or_else(|| panic!("missing {key}"))
}

fn req_f32(table: &toml::map::Map<String, toml::Value>, key: &str) -> f32 {
    let n = table
        .get(key)
        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        .unwrap_or_else(|| panic!("missing {key}")) as f32;
    if !n.is_finite() {
        panic!("{key} is not finite");
    }
    n
}

fn req_u32(table: &toml::map::Map<String, toml::Value>, key: &str) -> u32 {
    table
        .get(key)
        .and_then(|v| v.as_integer())
        .filter(|n| *n >= 0)
        .unwrap_or_else(|| panic!("missing {key}")) as u32
}

fn req_bool(table: &toml::map::Map<String, toml::Value>, key: &str) -> bool {
    table.get(key).and_then(|v| v.as_bool()).unwrap_or_else(|| panic!("missing {key}"))
}

fn req_vec3(table: &toml::map::Map<String, toml::Value>, key: &str) -> [f32; 3] {
    let arr = table.get(key).and_then(|v| v.as_array()).unwrap_or_else(|| panic!("missing {key}"));
    if arr.len() != 3 {
        panic!("{key} must have 3 numbers");
    }
    let mut out = [0.0; 3];
    for (i, value) in arr.iter().enumerate() {
        let n = value.as_float().or_else(|| value.as_integer().map(|n| n as f64)).unwrap_or_else(|| panic!("{key}[{i}]")) as f32;
        if !n.is_finite() {
            panic!("{key}[{i}] is not finite");
        }
        out[i] = n;
    }
    out
}
