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
                add_legs(&mut parts, id, spec, &dens);
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
                    add_legs(&mut low_parts, id, spec, &low);
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
        add_pods(parts, id, spec, dens);
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
        let mut euler = [spec.tilt_deg.to_radians(), 0.0, 0.0];
        let (mesh, joint) = match (spec.name.as_str(), spec.shape.as_str()) {
            ("sac", _) => {
                let mut d = *dens;
                d.plates = (dens.plates / 2).max(2);
                d.extra_plates = 0;
                d.spines = 0;
                (carapace_mesh(&size, &d, true), Joint::Sac)
            }
            ("shell" | "thorax" | "torso", _) => (carapace_mesh(&size, dens, false), Joint::Static),
            ("head", "cone") | ("horn", "cone") | ("mandible", "cone") => {
                if spec.name == "mandible" {
                    euler[1] = side * 0.45;
                }
                (cone_z(size[1].max(size[0]), size[0].max(0.02), dens.sides.max(4)), joint_for(spec, side))
            }
            ("head", _) => (gem_mesh([size[0] * 0.5, size[1] * 0.5, size[2] * 0.5], dens), Joint::Static),
            ("eyes", _) => (gem_mesh([size[0] * 0.5, size[1] * 0.5, size[2] * 0.5], &Dens::small()), Joint::Static),
            ("gland" | "core" | "abdomen", _) => (
                gem_mesh([size[0] * 0.5, size[1] * 0.5, size[2] * 0.5], dens),
                Joint::Sac,
            ),
            ("armor", _) => (armor_mesh(&size, dens), Joint::Static),
            ("forelimb", _) => {
                euler[2] = side * -0.4;
                (segment_mesh(size[1].max(0.2) * 0.55, size[0], size[0] * 0.62, dens.sides), Joint::Claw { side })
            }
            ("scythe", _) => {
                euler[2] = side * 0.15;
                (segment_mesh(size[1].max(0.4) * 0.62, size[0], size[0] * 0.28, dens.sides), Joint::Scythe { side })
            }
            ("cone", _) | (_, "cone") => (cone_z(size[1].max(0.1), size[0].max(0.02), dens.sides.max(4)), Joint::Static),
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
            let blade_len = size[1].max(0.3) * 0.48;
            let blade = segment_mesh(blade_len, size[0] * 0.55, size[0] * 0.16, dens.sides);
            parts.push(Part {
                name: format!("{}_blade{i}", spec.name),
                parent: Some(idx),
                rest_pos: [0.0, -size[1].max(0.3) * 0.48, 0.0],
                rest_euler: [0.9, 0.0, 0.0],
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

fn add_legs(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens) {
    let count = spec.count.max(1);
    let rows = (count + 1) / 2;
    let length = spec.size.get(1).copied().unwrap_or(0.4);
    let radius = spec.size.first().copied().unwrap_or(0.05).max(0.02);
    let femur = length * if dens.segs > 1 { 0.46 } else { 0.92 };
    let tibia = length * 0.42;
    let foot_len = length * 0.16;
    let femur_mesh = Arc::new(segment_mesh(femur, radius * 1.55, radius * 0.95, dens.sides));
    let tibia_mesh = Arc::new(segment_mesh(tibia, radius * 0.95, radius * 0.55, dens.sides));
    let foot_mesh = Arc::new({
        let mut m = MeshData::new();
        geom::spine(&mut m, [0.0, 0.0, 0.0], foot_len, radius * 0.7, WHITE);
        geom::rotate_x(&mut m, std::f32::consts::FRAC_PI_2);
        m
    });
    let hip_y = length * 0.72;
    for i in 0..count {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let row = i / 2;
        let t = if rows <= 1 {
            0.5
        } else {
            row as f32 / (rows as f32 - 1.0)
        };
        let z = (0.45 - t) * spec.ring_radius.max(0.2);
        let x = side * spec.ring_radius.max(0.2);
        let splay = side * -0.55;
        let femur_i = parts.len();
        parts.push(Part {
            name: format!("leg{i}_femur"),
            parent: None,
            rest_pos: [x, hip_y, z],
            rest_euler: [0.45, 0.0, splay],
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
        let tibia_i = parts.len();
        parts.push(Part {
            name: format!("leg{i}_tibia"),
            parent: Some(femur_i),
            rest_pos: [0.0, -femur * 0.86, 0.0],
            rest_euler: [0.65, 0.0, 0.0],
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
            rest_pos: [0.0, -tibia * 0.88, 0.0],
            rest_euler: [-0.35, 0.0, 0.0],
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
    let mesh = Arc::new(segment_mesh(len * 0.92, radius, radius * 0.82, dens.sides.max(4)));
    let step = spec.curl_deg.to_radians() / count as f32;
    let mut parent = None;
    for i in 0..count {
        let idx = parts.len();
        let pos = if i == 0 {
            spec.offset
        } else {
            [0.0, 0.0, -len * 0.82]
        };
        // Limb mesh points down (-Y). Pitch it so -Y swings toward -Z (back) and then up.
        parts.push(Part {
            name: format!("tail{i}"),
            parent,
            rest_pos: pos,
            rest_euler: [step - 1.15, 0.0, 0.0],
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
    let mut mesh = MeshData::new();
    geom::gem(
        &mut mesh,
        [0.0, 0.0, 0.0],
        [size[0] * 0.5, size[1] * 0.5, size[2] * 0.55],
        dens.slices.max(4),
        dens.stacks.max(2),
        WHITE,
    );
    let pos = if parent.is_some() {
        [0.0, -0.15, -spec.size.get(1).copied().unwrap_or(0.5)]
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

fn add_pods(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens) {
    let count = spec.count.max(1);
    let size = size3(spec);
    let mesh = Arc::new(gem_mesh(
        [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5],
        &Dens {
            slices: dens.slices.min(6),
            stacks: dens.stacks.min(4),
            ..*dens
        },
    ));
    let (center, radii) = sac_anchor_from(parts);
    for i in 0..count {
        let y = 1.0 - (i as f32 + 0.5) / count as f32 * 2.0;
        if y < -0.15 {
            continue;
        }
        let ring = (1.0 - y * y).max(0.0).sqrt();
        let theta = i as f32 * 2.3999632;
        let pos = [
            center[0] + theta.cos() * ring * radii[0],
            center[1] + y * radii[1],
            center[2] + theta.sin() * ring * radii[2],
        ];
        parts.push(Part {
            name: format!("pod{i}"),
            parent: None,
            rest_pos: pos,
            rest_euler: [0.0, 0.0, 0.0],
            mesh: mesh.clone(),
            palette: spec.color.clone(),
            emissive: spec.emissive,
            joint: Joint::Sac,
            mesh_key: format!("{id}:pod"),
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
    let mesh = Arc::new(cone_z(len, rad, dens.sides.max(4).min(5)));
    for i in 0..count {
        let a = i as f32 / count as f32 * std::f32::consts::TAU;
        let pos = [
            head[0] + a.sin() * rad * 2.2,
            head[1] + len * 0.15,
            head[2] + a.cos() * rad * 1.4,
        ];
        parts.push(Part {
            name: format!("crown{i}"),
            parent: None,
            rest_pos: pos,
            rest_euler: [-0.5, a, 0.0],
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
    let mut mesh = MeshData::new();
    let plates = if dens.plates == 0 { 1 } else { 5 };
    for i in 0..plates {
        let z = -shell_size[2] * 0.18 + i as f32 * 0.02;
        geom::plate(
            &mut mesh,
            [0.0, 0.0, z],
            shell_size[2] * 0.22,
            shell_size[0] * 0.28,
            shell_size[1] * 0.12,
            2,
            2,
            WHITE,
        );
    }
    let pos = [
        shell[0],
        shell[1] + shell_size[1] * 0.42,
        shell[2] - shell_size[2] * 0.12,
    ];
    parts.push(Part {
        name: spec.name.clone(),
        parent: None,
        rest_pos: pos,
        rest_euler: [0.15, 0.0, 0.0],
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
    geom::box_mesh(&mut mesh, [0.0, -0.35, 0.0], [0.04, 0.55, 0.03], WHITE);
    let mesh = Arc::new(mesh);
    for (n, idx) in legs.iter().enumerate() {
        parts.push(Part {
            name: format!("vein{n}"),
            parent: Some(*idx),
            rest_pos: [0.0, -0.05, 0.04],
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

fn joint_for(spec: &VPart, side: f32) -> Joint {
    if spec.name == "mandible" {
        Joint::Mandible { side }
    } else {
        Joint::Static
    }
}

fn carapace_mesh(size: &[f32; 3], dens: &Dens, glow: bool) -> MeshData {
    let mut mesh = MeshData::new();
    let radii = [size[0] * 0.46, size[1] * 0.40, size[2] * 0.46];
    geom::gem(&mut mesh, [0.0, 0.0, 0.0], radii, dens.slices, dens.stacks, WHITE);
    let plates = dens.plates;
    for i in 0..plates {
        let u = (i as f32 + 0.5) / plates.max(1) as f32;
        let z = (u - 0.5) * size[2] * 0.78;
        let overlap = size[2] / plates.max(1) as f32 * 1.55;
        let width = size[0] * (0.98 - (u - 0.5).abs() * 0.25);
        let arch = size[1] * if glow { 0.28 } else { 0.38 };
        geom::plate(
            &mut mesh,
            [0.0, size[1] * 0.06, z],
            overlap.max(0.05),
            width.max(0.05),
            arch.max(0.02),
            dens.nu,
            dens.nv,
            WHITE,
        );
    }
    for i in 0..dens.extra_plates {
        let u = (i as f32 + 0.5) / dens.extra_plates.max(1) as f32;
        let z = (u - 0.5) * size[2] * 0.7;
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        geom::plate(
            &mut mesh,
            [side * size[0] * 0.28, -size[1] * 0.02, z],
            size[2] / dens.extra_plates.max(1) as f32 * 1.4,
            size[0] * 0.42,
            size[1] * 0.16,
            dens.nu.max(2),
            2,
            WHITE,
        );
    }
    if dens.spines > 0 && !glow {
        for i in 0..dens.spines {
            let u = (i as f32 + 0.5) / dens.spines as f32;
            let z = (u - 0.5) * size[2] * 0.7;
            geom::spine(
                &mut mesh,
                [0.0, size[1] * 0.28, z],
                size[1] * 0.28,
                size[0] * 0.04,
                WHITE,
            );
        }
    }
    mesh
}

fn armor_mesh(size: &[f32; 3], dens: &Dens) -> MeshData {
    let mut mesh = MeshData::new();
    geom::box_mesh(&mut mesh, [0.0, 0.0, 0.0], *size, WHITE);
    let plates = dens.plates.min(4).max(2);
    for i in 0..plates {
        let y = (i as f32 - plates as f32 * 0.5) * size[1] / plates as f32;
        geom::plate(
            &mut mesh,
            [0.0, y, size[2] * 0.55],
            size[1] / plates as f32 * 1.3,
            size[0] * 0.92,
            size[2] * 0.35,
            3,
            2,
            WHITE,
        );
    }
    mesh
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
            spines: 2,
            extra_plates: 0,
        },
        "spitter" => Dens {
            slices: 8,
            stacks: 4,
            plates: 8,
            nu: 4,
            nv: 2,
            sides: 6,
            segs: 3,
            spines: 6,
            extra_plates: 4,
        },
        "brute" => Dens {
            slices: 10,
            stacks: 6,
            plates: 30,
            nu: 4,
            nv: 3,
            sides: 7,
            segs: 3,
            spines: 8,
            extra_plates: 16,
        },
        "brood_mother" => Dens {
            slices: 8,
            stacks: 5,
            plates: 8,
            nu: 4,
            nv: 3,
            sides: 5,
            segs: 2,
            spines: 4,
            extra_plates: 2,
        },
        _ => Dens {
            slices: 8,
            stacks: 5,
            plates: 6,
            nu: 3,
            nv: 2,
            sides: 5,
            segs: 2,
            spines: 4,
            extra_plates: 2,
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
