//! One sculpted mesh set per bug, built from the visual anchors in `content/bugs.toml`.
//! Parts stay separate so a joint transform can pose them. The distant mesh is one bake.

use std::sync::{Arc, OnceLock};

use serde::Deserialize;

use crate::geom;
use crate::pose::{pose, AnimInput, Xform};
use crate::MeshData;

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

#[derive(Clone, Debug)]
pub struct EnemyVisual {
    pub id: String,
    pub height: f32,
    pub lod_distance: f32,
    pub gait: String,
    pub bob_amp: f32,
    pub bob_hz: f32,
    pub leg_swing_deg: f32,
    pub sac_charge_scale: f32,
    pub sac_charge_glow: f32,
    pub charge_lean_deg: f32,
    pub sac_breathe: [f32; 2],
    pub frenzy_speed: f32,
    pub glow_pulse: f32,
    pub parts: Vec<Part>,
    pub lod: MeshData,
    pub lod_key: String,
}

#[derive(Clone, Debug)]
pub struct Part {
    pub name: String,
    pub parent: Option<usize>,
    pub rest_pos: [f32; 3],
    pub rest_euler: [f32; 3],
    pub mesh: Arc<MeshData>,
    pub palette: String,
    pub emissive: bool,
    pub joint: Joint,
    pub mesh_key: String,
    pub gate: Gate,
    pub foot: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Joint {
    Static,
    Leg { index: u32, segment: u32 },
    Mandible { side: f32 },
    Sac,
    Claw { side: f32 },
    Scythe { side: f32 },
    Tail { index: u32 },
    Foot { index: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gate {
    Always,
    Vulnerable,
    /// `phase_max` from the table. 1 means the first phase only (sim phase 0).
    PhaseMax(u32),
    /// `pulse_phase` from the table. Visible once the design phase reaches this number.
    PhaseMin(u32),
}

impl EnemyVisual {
    pub fn tri_count(&self) -> usize {
        self.parts.iter().map(|p| p.mesh.tri_count()).sum()
    }
}

pub fn catalog() -> &'static [EnemyVisual] {
    static CELL: OnceLock<Vec<EnemyVisual>> = OnceLock::new();
    CELL.get_or_init(build_all)
}

pub fn bug(id: &str) -> &'static EnemyVisual {
    catalog()
        .iter()
        .find(|b| b.id == id)
        .unwrap_or_else(|| panic!("no visual for {id}"))
}

fn build_all() -> Vec<EnemyVisual> {
    let file: Book = toml::from_str(include_str!("../../../content/bugs.toml")).expect("bugs.toml");
    ["swarmer", "spitter", "brute", "brood_mother", "hive_tyrant"]
        .into_iter()
        .map(|id| sculpt(id, file.row(id)))
        .collect()
}

fn sculpt(id: &str, row: &Row) -> EnemyVisual {
    let dens = dens_for(id, false);
    let mut parts = Vec::new();
    let mut shell_anchor = [0.0_f32, row.visual.height * 0.5, 0.0];
    let mut shell_size = [1.0_f32, 1.0, 1.0];

    for spec in &row.visual.parts {
        match spec.shape.as_str() {
            "sphere" | "capsule" | "cone" | "box" if spec.name == "leg" || spec.ring_radius > 0.0 => {
                add_legs(&mut parts, id, spec, &dens, row.visual.height);
            }
            "capsule" if spec.chain => add_tail(&mut parts, id, spec, &dens),
            "sphere" | "capsule" | "cone" | "box" => {
                add_body_part(&mut parts, id, spec, &dens);
            }
            "decal" => add_decal(&mut parts, id, spec, &dens, shell_anchor, shell_size),
            _ => {}
        }
        if matches!(spec.name.as_str(), "shell" | "thorax" | "torso") {
            shell_anchor = spec.offset;
            shell_size = size3(spec);
        }
    }
    for spec in &row.visual.parts {
        if spec.name == "veins" && !parts.iter().any(|p| p.name.starts_with("vein")) {
            add_veins(&mut parts, id, spec);
        }
    }

    let lod_parts = {
        let low = dens_for(id, true);
        let mut low_parts = Vec::new();
        for spec in &row.visual.parts {
            if spec.gate_kind() == GateKind::Vulnerable || spec.gate_kind() == GateKind::PhaseMin {
                continue;
            }
            match spec.shape.as_str() {
                "sphere" | "capsule" | "cone" | "box" if spec.name == "leg" || spec.ring_radius > 0.0 => {
                    add_legs(&mut low_parts, id, spec, &low, row.visual.height);
                }
                "capsule" if spec.chain => add_tail(&mut low_parts, id, spec, &low),
                "sphere" | "capsule" | "cone" | "box" => add_body_part(&mut low_parts, id, spec, &low),
                _ => {}
            }
        }
        low_parts
    };
    let visual_stub_gait = row.visual.anim.gait.clone();
    let mut baked = EnemyVisual {
        id: id.to_string(),
        height: row.visual.height,
        lod_distance: row.visual.lod_distance,
        gait: visual_stub_gait,
        bob_amp: row.visual.anim.bob[0],
        bob_hz: row.visual.anim.bob[1].max(0.2),
        leg_swing_deg: row.visual.anim.leg_swing,
        sac_charge_scale: row.visual.anim.sac_charge_scale,
        sac_charge_glow: row.visual.anim.sac_charge_glow,
        charge_lean_deg: row.visual.anim.charge_lean_deg,
        sac_breathe: row.visual.anim.sac_breathe,
        frenzy_speed: if row.visual.anim.frenzy_speed > 0.0 {
            row.visual.anim.frenzy_speed
        } else {
            1.0
        },
        glow_pulse: row.visual.anim.glow_pulse,
        parts: lod_parts,
        lod: MeshData::new(),
        lod_key: format!("{id}:lod"),
    };
        let rest = pose(
        &baked,
        AnimInput {
            time: 0.0,
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
    let mut lod = MeshData::new();
    for (part, xf) in baked.parts.iter().zip(rest.xforms.iter()) {
        if !xf.visible {
            continue;
        }
        let mut mesh = (*part.mesh).clone();
        bake_xform(&mut mesh, xf);
        geom::append(&mut lod, &mesh);
    }
    baked.parts = parts;
    baked.lod = lod;
    baked
}

fn add_body_part(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens) {
    if spec.name == "pod" {
        add_sac_orbs(parts, id, spec, dens, false);
        return;
    }
    if spec.name == "bud" || spec.grows_into_pod_on.is_some() {
        add_sac_orbs(parts, id, spec, dens, true);
        return;
    }
    if spec.name == "crown" {
        add_crown(parts, id, spec, dens);
        return;
    }
    if spec.name == "cannon" || spec.at.as_deref() == Some("tail_end") {
        add_cannon(parts, id, spec, dens);
        return;
    }
    let gate = spec.gate();
    let emissive = spec.emissive;
    let palette = spec.color.clone();
    let count = if spec.mirror { 2 } else { 1 };
    let size = size3(spec);
    for i in 0..count {
        let side = if i == 0 { 1.0 } else { -1.0 };
        let mut pos = spec.offset;
        if spec.mirror {
            pos[0] = spec.offset[0].abs() * side;
        }
        let mut euler = [0.0_f32, 0.0, 0.0];
        let (mesh, joint) = match (spec.name.as_str(), spec.shape.as_str()) {
            // Weak points are clean gems at the authored diameter so they read
            // even when bloom is off. No plates on top of them.
            ("sac" | "abdomen" | "gland" | "core", _) => (
                gem_mesh([size[0] * 0.5, size[1] * 0.5, size[2] * 0.5], dens),
                Joint::Sac,
            ),
            ("eyes", _) => (gem_mesh([size[0] * 0.5, size[1] * 0.5, size[2] * 0.5], &Dens::small()), Joint::Static),
            ("shell", _) => (dome_shell(&size, dens, id == "swarmer"), Joint::Static),
            ("thorax", _) => {
                // Negative tilt raises the back (the sac) and drops the head.
                euler[0] = -spec.tilt_deg.to_radians();
                (thorax_mesh(&size, dens), Joint::Static)
            }
            ("torso", _) => {
                euler[0] = spec.tilt_deg.to_radians();
                (torso_mesh(&size, dens), Joint::Static)
            }
            ("head", "cone") => {
                let len = size[1].max(size[0]);
                let rad = size[0].max(0.02);
                let sides = dens.sides.max(5);
                if id == "hive_tyrant" {
                    (cone_up(len, rad, sides), Joint::Static)
                } else {
                    (spitter_head(len, rad, sides), Joint::Static)
                }
            },
            ("head", _) => {
                // Brood's head stays small so the gland hangs in front of it.
                let k = if id == "brood_mother" { 0.36 } else { 0.5 };
                (gem_mesh([size[0] * k, size[1] * k, size[2] * k], dens), Joint::Static)
            }
            ("horn", "cone") => {
                euler[0] = -spec.tilt_deg.to_radians();
                (horn_mesh(size[1].max(size[0]), size[0].max(0.02), dens.sides.max(5)), Joint::Static)
            }
            ("mandible", "cone") => {
                euler[1] = side * 0.55;
                euler[0] = 0.25;
                (cone_z(size[1].max(0.08), size[0].max(0.02), dens.sides.max(4)), Joint::Mandible { side })
            }
            ("armor", _) => (armor_frame(&size), Joint::Static),
            ("forelimb", _) => {
                euler = aim_down([side * 1.2, -0.42, 1.05]);
                let len = size[1].max(0.3);
                let rad = size[0] * 0.40;
                (segment_mesh(len * 0.52, rad, rad * 0.62, dens.sides), Joint::Claw { side })
            }
            ("scythe", _) => {
                euler = aim_down([side * 0.72, 1.2, 0.38]);
                let len = size[1].max(0.4);
                let rad = size[0] * 0.36;
                (segment_mesh(len * 0.46, rad, rad * 0.42, dens.sides), Joint::Scythe { side })
            }
            (_, "cone") => (cone_z(size[1].max(0.1), size[0].max(0.02), dens.sides.max(4)), Joint::Static),
            (_, "box") => (box_origin(&size), Joint::Static),
            (_, "capsule") => (capsule_z(size[1].max(0.2), size[0].max(0.04), dens), Joint::Static),
            _ => (gem_mesh([size[0] * 0.5, size[1] * 0.5, size[2] * 0.5], dens), Joint::Static),
        };
        let parent = if spec.name == "forelimb" || spec.name == "scythe" {
            None
        } else {
            None
        };
        let key = format!("{id}:{}:{}", spec.name, if spec.mirror { "m" } else { "s" });
        let idx = parts.len();
        parts.push(Part {
            name: if spec.mirror {
                format!("{}{i}", spec.name)
            } else {
                spec.name.clone()
            },
            parent,
            rest_pos: pos,
            rest_euler: euler,
            mesh: Arc::new(mesh),
            palette: palette.clone(),
            emissive,
            joint,
            mesh_key: key,
            gate,
            foot: None,
        });
        if spec.name == "forelimb" || spec.name == "scythe" {
            let len = size[1].max(0.3);
            let rad = size[0] * if spec.name == "scythe" { 0.36 } else { 0.40 };
            let upper = len * if spec.name == "scythe" { 0.46 } else { 0.52 };
            let (blade_len, blade_euler, r0, r1) = if spec.name == "scythe" {
                (len * 0.62, [-1.2, side * 0.12, side * 0.2], rad * 0.48, rad * 0.07)
            } else {
                (len * 0.46, [1.05, 0.0, side * 0.18], rad * 0.72, rad * 0.1)
            };
            let blade = segment_mesh(blade_len, r0, r1, dens.sides.max(4));
            parts.push(Part {
                name: format!("{}_blade{i}", spec.name),
                parent: Some(idx),
                rest_pos: [0.0, -upper * 0.9, 0.0],
                rest_euler: blade_euler,
                mesh: Arc::new(blade),
                palette: palette.clone(),
                emissive: false,
                joint: if spec.name == "scythe" {
                    Joint::Scythe { side }
                } else {
                    Joint::Claw { side }
                },
                mesh_key: format!("{id}:{}:blade", spec.name),
                gate,
                foot: None,
            });
        }
    }
}

fn add_legs(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens, body_height: f32) {
    let count = spec.count.max(1);
    let rows = (count + 1) / 2;
    let length = spec.size.get(1).copied().unwrap_or(0.4);
    let radius = spec.size.first().copied().unwrap_or(0.05).max(0.02);
    let ring = spec.ring_radius.max(0.15);
    let femur_len = length * if dens.segs > 1 { 0.46 } else { 0.92 };
    let tibia_len = length * 0.40;
    let foot_len = length * 0.16;
    let femur_mesh = Arc::new(segment_mesh(femur_len, radius * 1.85, radius * 1.05, dens.sides));
    let tibia_mesh = Arc::new(segment_mesh(tibia_len, radius * 1.05, radius * 0.58, dens.sides.max(4)));
    let foot_mesh = Arc::new(segment_mesh(foot_len, radius * 0.7, radius * 0.28, 4));
    // Hips sit on the underside of the body. Feet plant wide of the shell.
    let hip_y = body_height * if count <= 4 { 0.46 } else { 0.36 };
    for i in 0..count {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let row = i / 2;
        let t = if rows <= 1 { 0.5 } else { row as f32 / (rows as f32 - 1.0) };
        let along = 0.42 - t;
        let hip = [side * ring * 0.50, hip_y, along * ring * 0.75];
        let plant = [side * ring * 1.95, 0.0, along * ring * 1.6];
        let drop = plant[1] - hip[1];
        let femur_dir = normalize([
            plant[0] - hip[0],
            drop * 0.30,
            (plant[2] - hip[2]) * 0.5,
        ]);
        let femur_euler = if dens.segs > 1 {
            aim_down(femur_dir)
        } else {
            aim_down([plant[0] - hip[0], drop, plant[2] - hip[2]])
        };
        let femur_i = parts.len();
        parts.push(Part {
            name: format!("leg{i}_femur"),
            parent: None,
            rest_pos: hip,
            rest_euler: femur_euler,
            mesh: femur_mesh.clone(),
            palette: spec.color.clone(),
            emissive: false,
            joint: Joint::Leg { index: i, segment: 0 },
            mesh_key: format!("{id}:leg:femur"),
            gate: Gate::Always,
            foot: None,
        });
        if dens.segs == 1 {
            continue;
        }
        let knee = [
            hip[0] + femur_dir[0] * femur_len,
            hip[1] + femur_dir[1] * femur_len,
            hip[2] + femur_dir[2] * femur_len,
        ];
        let tibia_world = normalize([plant[0] - knee[0], plant[1] - knee[1], plant[2] - knee[2]]);
        let femur_q = crate::pose::quat_from_euler_xyz(femur_euler[0], femur_euler[1], femur_euler[2]);
        let inv = [-femur_q[0], -femur_q[1], -femur_q[2], femur_q[3]];
        let tibia_euler = aim_down(crate::pose::rotate_vec(inv, tibia_world));
        let tibia_i = parts.len();
        parts.push(Part {
            name: format!("leg{i}_tibia"),
            parent: Some(femur_i),
            rest_pos: [0.0, -femur_len * 0.92, 0.0],
            rest_euler: tibia_euler,
            mesh: tibia_mesh.clone(),
            palette: spec.color.clone(),
            emissive: false,
            joint: Joint::Leg { index: i, segment: 1 },
            mesh_key: format!("{id}:leg:tibia"),
            gate: Gate::Always,
            foot: None,
        });
        parts.push(Part {
            name: format!("leg{i}_foot"),
            parent: Some(tibia_i),
            rest_pos: [0.0, -tibia_len * 0.9, 0.0],
            rest_euler: [0.45, 0.0, 0.0],
            mesh: foot_mesh.clone(),
            palette: spec.color.clone(),
            emissive: false,
            joint: Joint::Foot { index: i },
            mesh_key: format!("{id}:leg:foot"),
            gate: Gate::Always,
            foot: Some(i),
        });
    }
}

fn add_tail(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens) {
    let count = spec.count.max(1).min(if dens.plates == 0 { 3 } else { 12 });
    let len = spec.size.get(1).copied().unwrap_or(0.8);
    let radius = spec.size.first().copied().unwrap_or(0.3);
    let mesh = Arc::new(segment_mesh(len * 0.9, radius, radius * 0.78, dens.sides.max(4)));
    // Limb mesh points along -Y. Positive X rotation swings that toward -Z and then up,
    // so the chain curls over the back instead of pitching forward.
    let step = spec.curl_deg.to_radians() / count as f32;
    let base = 2.05_f32;
    let mut parent = None;
    for i in 0..count {
        let idx = parts.len();
        let pos = if i == 0 { spec.offset } else { [0.0, -len * 0.82, 0.0] };
        parts.push(Part {
            name: format!("tail{i}"),
            parent,
            rest_pos: pos,
            rest_euler: [if i == 0 { base } else { step * 0.70 }, 0.0, 0.0],
            mesh: mesh.clone(),
            palette: spec.color.clone(),
            emissive: false,
            joint: Joint::Tail { index: i },
            mesh_key: format!("{id}:tail"),
            gate: Gate::Always,
            foot: None,
        });
        parent = Some(idx);
    }
}

fn add_cannon(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens) {
    let parent = parts.iter().rposition(|p| p.name.starts_with("tail"));
    let size = size3(spec);
    let rad = size[0] * 0.5;
    let mut mesh = gem_mesh([rad, rad, rad * 0.92], dens);
    // Barrel continues along the tail (-Y) and sits clear of the last segment's cap.
    let mut nozzle = cone_z(rad * 1.15, rad * 0.48, dens.sides.max(5));
    geom::rotate_x(&mut nozzle, std::f32::consts::FRAC_PI_2);
    geom::translate(&mut nozzle, [0.0, -(rad + 0.02), 0.0]);
    geom::append(&mut mesh, &nozzle);
    let seg_len = parent
        .map(|i| {
            parts[i]
                .mesh
                .positions
                .iter()
                .map(|p| -p[1])
                .fold(0.0_f32, f32::max)
        })
        .unwrap_or(0.4);
    let pos = if parent.is_some() {
        [0.0, -(seg_len + rad + 0.02), 0.0]
    } else {
        spec.offset
    };
    parts.push(Part {
        name: "cannon".into(),
        parent,
        rest_pos: pos,
        rest_euler: [0.0, 0.0, 0.0],
        mesh: Arc::new(mesh),
        palette: spec.color.clone(),
        emissive: spec.emissive,
        joint: Joint::Sac,
        mesh_key: format!("{id}:cannon"),
        gate: spec.gate(),
        foot: None,
    });
}

fn add_sac_orbs(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens, nestled: bool) {
    let count = spec.count.max(1);
    let size = size3(spec);
    let detail = if dens.slices <= 4 {
        Dens::small()
    } else if nestled {
        Dens { slices: 4, stacks: 2, ..Dens::small() }
    } else {
        Dens { slices: 6, stacks: 4, ..Dens::small() }
    };
    let mesh = Arc::new(gem_mesh([size[0] * 0.5, size[1] * 0.5, size[2] * 0.5], &detail));
    let (center, radii) = sac_anchor_from(parts);
    let sink = if nestled { 0.80 } else { 0.98 };
    let phase = if nestled { 0.5 } else { 0.0 };
    for i in 0..count {
        let a = (i as f32 + phase) / count as f32 * std::f32::consts::TAU + 0.4;
        let ring = if nestled { 0.48 } else { 0.62 };
        let nx = a.cos() * ring;
        let nz = a.sin() * ring;
        let ny = (1.0 - nx * nx - nz * nz).max(0.25).sqrt();
        let pos = [
            center[0] + nx * radii[0] * sink,
            center[1] + ny * radii[1] * sink,
            center[2] + nz * radii[2] * sink,
        ];
        let name = if nestled { format!("bud{i}") } else { format!("pod{i}") };
        parts.push(Part {
            name,
            parent: None,
            rest_pos: pos,
            rest_euler: [0.0, 0.0, 0.0],
            mesh: mesh.clone(),
            palette: spec.color.clone(),
            emissive: spec.emissive && !nestled,
            joint: if nestled { Joint::Static } else { Joint::Sac },
            mesh_key: format!("{id}:{}", if nestled { "bud" } else { "pod" }),
            gate: Gate::Always,
            foot: None,
        });
    }
}

fn add_crown(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens) {
    let count = spec.count.max(1);
    let head = parts
        .iter()
        .find(|p| p.name == "head" || p.name.starts_with("head"))
        .map(|p| p.rest_pos)
        .unwrap_or([0.0, 1.0, 0.5]);
    let len = spec.size.get(1).copied().unwrap_or(0.4);
    let rad = spec.size.first().copied().unwrap_or(0.1);
    let mesh = Arc::new(cone_up(len, rad, dens.sides.max(4).min(5)));
    for i in 0..count {
        let a = i as f32 / count as f32 * std::f32::consts::TAU;
        let pos = [
            head[0] + a.sin() * rad * 2.6,
            head[1] + len * 0.25,
            head[2] + a.cos() * rad * 1.6,
        ];
        parts.push(Part {
            name: format!("crown{i}"),
            parent: None,
            rest_pos: pos,
            rest_euler: [0.85, a, 0.0],
            mesh: mesh.clone(),
            palette: spec.color.clone(),
            emissive: false,
            joint: Joint::Static,
            mesh_key: format!("{id}:crown"),
            gate: Gate::Always,
            foot: None,
        });
    }
}

fn add_decal(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens, shell: [f32; 3], shell_size: [f32; 3]) {
    if spec.name == "veins" || spec.on.as_deref() == Some("legs") {
        add_veins(parts, id, spec);
        return;
    }
    // Raised fissures sit in the gaps between dorsal plates, clear of the shell.
    let radii = shell_radii(&shell_size, false);
    let bands = dorsal_bands(&radii, dens.plates.max(4), false);
    let mut mesh = MeshData::new();
    let gap = plate_gap(radii[1]);
    let arch = plate_arch(radii[1]);
    for i in 0..bands.len().saturating_sub(1) {
        let z = (bands[i].0 + bands[i + 1].0) * 0.5;
        let spacing = bands[i].2;
        let len = (spacing * 0.28).max(0.04);
        let thick = (radii[1] * 0.12).max(0.04);
        // Three scars across the gap, separated in X so they never share a face.
        for (k, x) in [-0.40_f32, 0.0, 0.40].into_iter().enumerate() {
            let x = radii[0] * x;
            let y = ellipse_y(&radii, x, z);
            let width = radii[0] * if k == 1 { 0.22 } else { 0.28 };
            geom::box_mesh(
                &mut mesh,
                [x, y + gap + arch + thick * 0.5 + 0.015, z],
                [width, thick, len],
                WHITE,
            );
        }
    }
    parts.push(Part {
        name: spec.name.clone(),
        parent: None,
        rest_pos: shell,
        rest_euler: [0.0, 0.0, 0.0],
        mesh: Arc::new(mesh),
        palette: spec.color.clone(),
        emissive: spec.emissive,
        joint: Joint::Static,
        mesh_key: format!("{id}:{}", spec.name),
        gate: spec.gate(),
        foot: None,
    });
}

fn add_veins(parts: &mut Vec<Part>, id: &str, spec: &VPart) {
    let legs: Vec<usize> = parts
        .iter()
        .enumerate()
        .filter_map(|(i, p)| match p.joint {
            Joint::Leg { segment: 0, .. } => Some(i),
            _ => None,
        })
        .collect();
    if legs.is_empty() {
        return;
    }
    let mut mesh = MeshData::new();
    geom::box_mesh(&mut mesh, [0.0, -0.55, 0.0], [0.16, 1.05, 0.1], WHITE);
    let mesh = Arc::new(mesh);
    for (n, idx) in legs.iter().enumerate() {
        let parent_r = parts[*idx]
            .mesh
            .positions
            .iter()
            .map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt())
            .fold(0.0_f32, f32::max);
        parts.push(Part {
            name: format!("vein{n}"),
            parent: Some(*idx),
            rest_pos: [0.0, -0.12, parent_r + 0.08],
            rest_euler: [0.0, 0.0, 0.0],
            mesh: mesh.clone(),
            palette: spec.color.clone(),
            emissive: true,
            joint: Joint::Static,
            mesh_key: format!("{id}:vein"),
            gate: spec.gate(),
            foot: None,
        });
    }
}

fn sac_anchor_from(parts: &[Part]) -> ([f32; 3], [f32; 3]) {
    if let Some(sac) = parts.iter().find(|p| p.name == "sac") {
        let mut max = [0.2_f32, 0.2, 0.2];
        for p in &sac.mesh.positions {
            max[0] = max[0].max(p[0].abs());
            max[1] = max[1].max(p[1].abs());
            max[2] = max[2].max(p[2].abs());
        }
        (sac.rest_pos, [max[0] * 0.92, max[1] * 0.92, max[2] * 0.92])
    } else {
        ([0.0, 1.0, 0.0], [0.6, 0.5, 0.6])
    }
}

/// Round shell plus dorsal plates. Each plate is shorter than its slot and lifted
/// off the surface, so neighboring plates never share a face.
fn dome_shell(size: &[f32; 3], dens: &Dens, rear_open: bool) -> MeshData {
    let mut mesh = MeshData::new();
    let radii = shell_radii(size, rear_open);
    geom::gem(&mut mesh, [0.0, 0.0, 0.0], radii, dens.slices, dens.stacks, WHITE);
    if dens.plates == 0 {
        return mesh;
    }
    let gap = plate_gap(radii[1]);
    let arch = plate_arch(radii[1]);
    for (n, (z, y, spacing)) in dorsal_bands(&radii, dens.plates, rear_open).into_iter().enumerate() {
        geom::plate(
            &mut mesh,
            [0.0, y + gap, z],
            (spacing * 0.64).max(0.02),
            radii[0] * 1.02,
            arch,
            dens.nu.max(1),
            dens.nv.max(1),
            WHITE,
        );
        if dens.spines > 0 && n % 2 == 0 && (n as u32) < dens.spines * 2 {
            geom::spine(
                &mut mesh,
                [0.0, y + gap + arch + 0.012, z],
                radii[1] * 0.28,
                radii[0] * 0.05,
                WHITE,
            );
        }
    }
    mesh
}

fn thorax_mesh(size: &[f32; 3], dens: &Dens) -> MeshData {
    let mut mesh = MeshData::new();
    let radius = size[0] * 0.42;
    let length = size[1].max(0.4);
    let mut body = segment_mesh(length, radius * 0.92, radius, dens.sides.max(5));
    geom::translate(&mut body, [0.0, length * 0.5, 0.0]);
    geom::rotate_x(&mut body, -std::f32::consts::FRAC_PI_2);
    geom::append(&mut mesh, &body);
    let plates = dens.plates.max(1);
    let gap = (radius * 0.1).max(0.012);
    for i in 0..plates {
        let u = (i as f32 + 0.5) / plates as f32;
        // Stay on the headward half so the glowing sac behind the origin stays clear.
        let z = (u * 0.85 - 0.05) * length;
        let slot = length * 0.85 / plates as f32;
        geom::plate(
            &mut mesh,
            [0.0, radius + gap, z],
            slot * 0.62,
            radius * 1.65,
            radius * 0.18,
            dens.nu.max(1),
            dens.nv.max(1),
            WHITE,
        );
    }
    mesh
}

fn torso_mesh(size: &[f32; 3], dens: &Dens) -> MeshData {
    let mut mesh = MeshData::new();
    // Narrow in Z so the chest core sits on the front instead of inside the body.
    let radii = [size[0] * 0.40, size[1] * 0.46, size[2] * 0.26];
    geom::gem(&mut mesh, [0.0, 0.0, 0.0], radii, dens.slices, dens.stacks, WHITE);
    let neck_len = (1.95 - radii[1] * 0.62).max(0.35);
    let mut neck = segment_mesh(neck_len, radii[0] * 0.42, radii[0] * 0.30, dens.sides.max(5));
    geom::rotate_x(&mut neck, std::f32::consts::PI);
    geom::translate(&mut neck, [0.0, radii[1] * 0.62, 0.04]);
    geom::append(&mut mesh, &neck);
    let plates = dens.plates.min(5);
    if plates > 0 {
        let gap = plate_gap(radii[2]);
        for i in 0..plates {
            let u = (i as f32 + 0.5) / plates as f32;
            let y = (u - 0.45) * radii[1] * 1.2;
            let z_surf = radii[2] * (1.0 - (y / radii[1]).powi(2)).max(0.0).sqrt();
            let mut plate = MeshData::new();
            let slot = radii[1] * 1.2 / plates as f32;
            geom::plate(
                &mut plate,
                [0.0, 0.0, 0.0],
                slot * 0.62,
                radii[0] * 0.7,
                radii[2] * 0.16,
                dens.nu.max(1),
                dens.nv.max(1),
                WHITE,
            );
            // Arch (+Y) swings to -Z so the plate sits on the back, off the chest core.
            geom::rotate_x(&mut plate, -std::f32::consts::FRAC_PI_2);
            geom::translate(&mut plate, [0.0, y, -(z_surf + gap)]);
            geom::append(&mut mesh, &plate);
        }
    }
    mesh
}

/// Hollow chest frame. Bars are separated so they don't share faces, and the
/// whole frame sits in front of the core instead of covering it.
fn armor_frame(size: &[f32; 3]) -> MeshData {
    let mut mesh = MeshData::new();
    let hx = size[0] * 0.5;
    let hy = size[1] * 0.5;
    let t = (size[0].min(size[1]) * 0.16).max(0.08);
    let gap = 0.04;
    let depth = (size[2] * 0.28).max(0.06);
    let z = size[2] * 0.95;
    geom::box_mesh(&mut mesh, [0.0, hy - t * 0.5, z], [size[0], t, depth], WHITE);
    geom::box_mesh(&mut mesh, [0.0, -hy + t * 0.5, z], [size[0], t, depth], WHITE);
    let side_h = (size[1] - t * 2.0 - gap * 2.0).max(t);
    geom::box_mesh(&mut mesh, [-hx + t * 0.5, 0.0, z], [t, side_h, depth], WHITE);
    geom::box_mesh(&mut mesh, [hx - t * 0.5, 0.0, z], [t, side_h, depth], WHITE);
    mesh
}

fn spitter_head(length: f32, radius: f32, sides: u32) -> MeshData {
    let mut mesh = cone_z(length, radius, sides);
    for side in [-1.0_f32, 1.0] {
        let mut fang = cone_z(length * 0.42, radius * 0.22, 4);
        geom::rotate_x(&mut fang, 0.6);
        geom::translate(&mut fang, [side * radius * 0.42, -radius * 0.2, length * 0.62]);
        geom::append(&mut mesh, &fang);
    }
    mesh
}

fn horn_mesh(length: f32, radius: f32, sides: u32) -> MeshData {
    let mut mesh = cone_z(length, radius, sides);
    for side in [-1.0_f32, 1.0] {
        let mut tusk = cone_z(length * 0.42, radius * 0.42, sides.min(5).max(4));
        geom::translate(&mut tusk, [side * radius * 1.35, -radius * 0.15, length * 0.12]);
        geom::append(&mut mesh, &tusk);
    }
    mesh
}

fn shell_radii(size: &[f32; 3], rear_open: bool) -> [f32; 3] {
    let mut radii = [size[0] * 0.47, size[1] * 0.47, size[2] * 0.47];
    if rear_open {
        radii[2] *= 0.70;
    }
    radii
}

fn plate_gap(radius: f32) -> f32 {
    (radius * 0.045).max(0.012)
}

fn plate_arch(radius: f32) -> f32 {
    (radius * 0.06).max(0.01)
}

/// (z, surface y, slot spacing) along the dorsal ridge. Slots do not overlap.
fn dorsal_bands(radii: &[f32; 3], count: u32, rear_open: bool) -> Vec<(f32, f32, f32)> {
    let count = count.max(1);
    let span = if rear_open { 0.46 } else { 0.66 };
    let spacing = span * radii[2] * 2.0 / count as f32;
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        let u = (i as f32 + 0.5) / count as f32;
        let u2 = if rear_open { 0.50 + u * span } else { 0.17 + u * span };
        let z = (u2 - 0.5) * radii[2] * 2.0;
        out.push((z, ellipse_y(radii, 0.0, z), spacing));
    }
    out
}

fn ellipse_y(radii: &[f32; 3], x: f32, z: f32) -> f32 {
    let nx = (x / radii[0]).powi(2);
    let nz = (z / radii[2]).powi(2);
    radii[1] * (1.0 - nx - nz).max(0.0).sqrt()
}

/// Aim a limb whose mesh points along local -Y.
fn aim_down(dir: [f32; 3]) -> [f32; 3] {
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt().max(1e-5);
    let dx = (dir[0] / len).clamp(-0.98, 0.98);
    let dy = dir[1] / len;
    let dz = dir[2] / len;
    let roll = dx.asin();
    let pitch = (-dz).atan2(-dy);
    [pitch, 0.0, roll]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-5);
    [v[0] / len, v[1] / len, v[2] / len]
}

fn gem_mesh(radii: [f32; 3], dens: &Dens) -> MeshData {
    let mut mesh = MeshData::new();
    geom::gem(&mut mesh, [0.0, 0.0, 0.0], radii, dens.slices, dens.stacks, WHITE);
    mesh
}

fn cone_z(length: f32, radius: f32, sides: u32) -> MeshData {
    let mut mesh = MeshData::new();
    geom::cone(&mut mesh, length.max(0.04), radius.max(0.01), sides.max(3), WHITE);
    geom::rotate_x(&mut mesh, -std::f32::consts::FRAC_PI_2);
    mesh
}

fn cone_up(length: f32, radius: f32, sides: u32) -> MeshData {
    let mut mesh = MeshData::new();
    geom::cone(&mut mesh, length.max(0.04), radius.max(0.01), sides.max(3), WHITE);
    geom::rotate_x(&mut mesh, std::f32::consts::PI);
    mesh
}

fn capsule_z(length: f32, radius: f32, dens: &Dens) -> MeshData {
    let mut mesh = MeshData::new();
    geom::limb(&mut mesh, length, radius, radius * 0.85, dens.sides.max(4), WHITE);
    geom::rotate_x(&mut mesh, -std::f32::consts::FRAC_PI_2);
    mesh
}

fn segment_mesh(length: f32, r0: f32, r1: f32, sides: u32) -> MeshData {
    let mut mesh = MeshData::new();
    geom::limb(&mut mesh, length.max(0.04), r0.max(0.01), r1.max(0.008), sides.max(3), WHITE);
    mesh
}

fn box_origin(size: &[f32; 3]) -> MeshData {
    let mut mesh = MeshData::new();
    geom::box_mesh(&mut mesh, [0.0, 0.0, 0.0], *size, WHITE);
    mesh
}

fn bake_xform(mesh: &mut MeshData, xf: &Xform) {
    for p in &mut mesh.positions {
        p[0] *= xf.scale[0];
        p[1] *= xf.scale[1];
        p[2] *= xf.scale[2];
        *p = crate::pose::rotate_vec(xf.rot, *p);
        p[0] += xf.pos[0];
        p[1] += xf.pos[1];
        p[2] += xf.pos[2];
    }
    for n in &mut mesh.normals {
        *n = crate::pose::rotate_vec(xf.rot, *n);
    }
}

#[derive(Clone, Copy)]
struct Dens {
    slices: u32,
    stacks: u32,
    plates: u32,
    nu: u32,
    nv: u32,
    sides: u32,
    segs: u32,
    spines: u32,
    #[allow(dead_code)]
    extra_plates: u32,
}

impl Dens {
    fn small() -> Self {
        Self {
            slices: 4,
            stacks: 2,
            plates: 0,
            nu: 1,
            nv: 1,
            sides: 4,
            segs: 1,
            spines: 0,
            extra_plates: 0,
        }
    }
}

fn dens_for(id: &str, lod: bool) -> Dens {
    if lod {
        return Dens {
            slices: 4,
            stacks: 2,
            plates: 0,
            nu: 1,
            nv: 1,
            sides: 4,
            segs: 1,
            spines: 0,
            extra_plates: 0,
        };
    }
    match id {
        "swarmer" => Dens {
            slices: 5,
            stacks: 3,
            plates: 3,
            nu: 2,
            nv: 2,
            sides: 4,
            segs: 2,
            spines: 0,
            extra_plates: 0,
        },
        "spitter" => Dens {
            slices: 12,
            stacks: 7,
            plates: 8,
            nu: 4,
            nv: 2,
            sides: 8,
            segs: 2,
            spines: 0,
            extra_plates: 0,
        },
        "brute" => Dens {
            slices: 16,
            stacks: 10,
            plates: 10,
            nu: 4,
            nv: 3,
            sides: 9,
            segs: 2,
            spines: 4,
            extra_plates: 0,
        },
        "brood_mother" => Dens {
            slices: 8,
            stacks: 5,
            plates: 5,
            nu: 3,
            nv: 2,
            sides: 5,
            segs: 2,
            spines: 0,
            extra_plates: 0,
        },
        _ => Dens {
            slices: 8,
            stacks: 6,
            plates: 4,
            nu: 3,
            nv: 2,
            sides: 6,
            segs: 2,
            spines: 0,
            extra_plates: 0,
        },
    }
}

fn size3(spec: &VPart) -> [f32; 3] {
    match spec.size.as_slice() {
        [x, y, z, ..] => [*x, *y, *z],
        [r, h] => [*r * 2.0, *h, *r * 2.0],
        [r] => [*r, *r, *r],
        _ => [0.2, 0.2, 0.2],
    }
}

#[derive(Clone, Copy, PartialEq)]
enum GateKind {
    Always,
    Vulnerable,
    PhaseMin,
}

#[derive(Deserialize)]
struct Book {
    swarmer: Row,
    spitter: Row,
    brute: Row,
    brood_mother: Row,
    hive_tyrant: Row,
}

impl Book {
    fn row(&self, id: &str) -> &Row {
        match id {
            "swarmer" => &self.swarmer,
            "spitter" => &self.spitter,
            "brute" => &self.brute,
            "brood_mother" => &self.brood_mother,
            _ => &self.hive_tyrant,
        }
    }
}

#[derive(Deserialize)]
struct Row {
    visual: VVisual,
}

#[derive(Deserialize)]
struct VVisual {
    height: f32,
    #[serde(default)]
    lod_distance: f32,
    #[serde(default)]
    parts: Vec<VPart>,
    anim: VAnim,
}

#[derive(Deserialize)]
struct VPart {
    name: String,
    #[serde(default)]
    shape: String,
    #[serde(default)]
    size: Vec<f32>,
    #[serde(default)]
    offset: [f32; 3],
    #[serde(default)]
    color: String,
    #[serde(default)]
    emissive: bool,
    #[serde(default)]
    mirror: bool,
    #[serde(default)]
    count: u32,
    #[serde(default)]
    ring_radius: f32,
    #[serde(default)]
    tilt_deg: f32,
    #[serde(default)]
    on: Option<String>,
    #[serde(default)]
    active_when: Option<String>,
    #[serde(default)]
    phase_max: Option<u32>,
    #[serde(default)]
    chain: bool,
    #[serde(default)]
    curl_deg: f32,
    #[serde(default)]
    at: Option<String>,
    #[serde(default)]
    pulse_phase: Option<u32>,
    #[serde(default)]
    grows_into_pod_on: Option<String>,
}

impl VPart {
    fn gate(&self) -> Gate {
        if self.active_when.as_deref() == Some("vulnerable") {
            Gate::Vulnerable
        } else if let Some(max) = self.phase_max {
            Gate::PhaseMax(max)
        } else if let Some(min) = self.pulse_phase {
            Gate::PhaseMin(min)
        } else {
            Gate::Always
        }
    }

    fn gate_kind(&self) -> GateKind {
        match self.gate() {
            Gate::Vulnerable => GateKind::Vulnerable,
            Gate::PhaseMin(_) => GateKind::PhaseMin,
            _ => GateKind::Always,
        }
    }
}

#[derive(Deserialize)]
struct VAnim {
    #[serde(default)]
    bob: [f32; 2],
    #[serde(default)]
    leg_swing: f32,
    #[serde(default)]
    gait: String,
    #[serde(default)]
    glow_pulse: f32,
    #[serde(default)]
    sac_charge_scale: f32,
    #[serde(default)]
    sac_charge_glow: f32,
    #[serde(default)]
    charge_lean_deg: f32,
    #[serde(default)]
    sac_breathe: [f32; 2],
    #[serde(default)]
    frenzy_speed: f32,
}

pub fn flat_shaded(mesh: &MeshData) -> bool {
    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            return false;
        }
        let n0 = mesh.normals[tri[0] as usize];
        let n1 = mesh.normals[tri[1] as usize];
        let n2 = mesh.normals[tri[2] as usize];
        if n0 != n1 || n1 != n2 {
            return false;
        }
    }
    true
}
