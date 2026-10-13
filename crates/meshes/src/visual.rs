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
    pub camera: ShotCamera,
    pub camera_open: Option<ShotCamera>,
    pub parts: Vec<Part>,
    pub lod: MeshData,
    pub lod_key: String,
}

/// Concept-sheet camera, baked from the visual table at sculpt time.
#[derive(Clone, Copy, Debug)]
pub struct ShotCamera {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub fov: f32,
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
    /// Bud that swells into a pod. The slot is the visual count index.
    pub grows: bool,
    pub grow_slot: Option<u32>,
    pub pod_slot: Option<u32>,
    /// Tail sway, copied from the visual tail part. Zero on every other part.
    pub sway_rate: f32,
    pub sway_spread: f32,
    pub sway_amp: f32,
    /// Joint oscillation from the visual part. Zero when this joint does not swing.
    pub swing_hz: f32,
    pub swing_amp: f32,
    /// Bud swell once it has grown into a pod. Zero on every other part.
    pub grown_scale: f32,
    pub grown_glow: f32,
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
    let text = include_str!("../../../content/bugs.toml");
    let rows = rows_with_visual(text).expect("bugs.toml");
    rows.into_iter()
        .map(|(id, row)| sculpt(&id, &row).unwrap_or_else(|err| panic!("{id} visual: {err}")))
        .collect()
}

fn rows_with_visual(text: &str) -> Result<Vec<(String, Row)>, String> {
    let root: toml::Value = toml::from_str(text).map_err(|err| err.to_string())?;
    let table = root.as_table().ok_or_else(|| "bugs.toml is not a table".to_string())?;
    let mut rows = Vec::new();
    for (id, value) in table {
        let Some(map) = value.as_table() else { continue };
        if !map.contains_key("visual") {
            continue;
        }
        let row: Row = value.clone().try_into().map_err(|err| format!("{id}: {err}"))?;
        rows.push((id.clone(), row));
    }
    if rows.is_empty() {
        return Err("bugs.toml has no visual tables".into());
    }
    Ok(rows)
}

fn sculpt(id: &str, row: &Row) -> Result<EnemyVisual, String> {
    let height = row.visual.height;
    let dens = dens_of(&row.visual.density)?;
    let mut parts = Vec::new();
    let mut shell = None;
    place_parts(&mut parts, id, row, &dens, height, &mut shell)?;

    let lod_parts = {
        let low = dens_of(&row.visual.lod)?;
        let mut low_parts = Vec::new();
        let mut ignored = None;
        for spec in &row.visual.parts {
            if spec.gate_kind() == GateKind::Vulnerable || spec.gate_kind() == GateKind::PhaseMin {
                continue;
            }
            place_one(&mut low_parts, id, spec, &low, &row.visual.parts, height, &mut ignored)?;
        }
        low_parts
    };
    let mut baked = EnemyVisual {
        id: id.to_string(),
        height,
        lod_distance: row.visual.lod_distance,
        gait: row.visual.anim.gait.clone(),
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
        camera: shot_camera(height, &row.visual.camera),
        camera_open: row.visual.camera_open.as_ref().map(|cam| open_camera(height, cam)),
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
            hatch_pod: 255,
            hatch_charge: 0.0,
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
    Ok(baked)
}

fn place_parts(
    parts: &mut Vec<Part>,
    id: &str,
    row: &Row,
    dens: &Dens,
    height: f32,
    shell: &mut Option<([f32; 3], [f32; 3])>,
) -> Result<(), String> {
    // Vein decals parent onto legs, which may be authored later in the table.
    let mut veins = Vec::new();
    for spec in &row.visual.parts {
        if spec.part == "decal" && spec.on.as_deref() == Some("legs") {
            veins.push(spec);
            continue;
        }
        place_one(parts, id, spec, dens, &row.visual.parts, height, shell)?;
    }
    for spec in veins {
        add_veins(parts, id, spec)?;
    }
    Ok(())
}

fn place_one(
    parts: &mut Vec<Part>,
    id: &str,
    spec: &VPart,
    dens: &Dens,
    catalog: &[VPart],
    height: f32,
    shell: &mut Option<([f32; 3], [f32; 3])>,
) -> Result<(), String> {
    if spec.part.is_empty() {
        return Err(format!("{id} {} is missing part", spec.name));
    }
    match spec.part.as_str() {
        "leg" => add_legs(parts, id, spec, dens, height)?,
        "tail" => add_tail(parts, id, spec, dens)?,
        "decal" => {
            if spec.on.as_deref() == Some("legs") {
                add_veins(parts, id, spec)?;
            } else if spec.visual_radius > 0.0 {
                add_glow_orb(parts, id, spec)?;
            } else {
                let (anchor, size) = shell.ok_or_else(|| format!("{id} {} decal has no shell", spec.name))?;
                add_decal(parts, id, spec, dens, anchor, size);
            }
        }
        "shell" | "gem" | "eyes" | "thorax" | "torso" | "head" | "horn" | "mandible" | "armor" | "forelimb"
        | "scythe" | "crown" | "pod" | "bud" | "cannon" => {
            add_body_part(parts, id, spec, dens, catalog)?;
        }
        other => return Err(format!("{id} {} has unknown part type \"{other}\"", spec.name)),
    }
    if matches!(spec.part.as_str(), "shell" | "thorax" | "torso") {
        *shell = Some((spec.offset, size3(id, spec)?));
    }
    Ok(())
}

fn shot_camera(height: f32, cam: &VCamera) -> ShotCamera {
    let dist = height * cam.dist_mul + cam.dist_add;
    ShotCamera {
        eye: [dist * cam.eye_x, height * cam.eye_y + cam.eye_y_add, dist * cam.eye_z],
        target: [0.0, height * cam.look_y, cam.look_z],
        fov: cam.fov,
    }
}

fn open_camera(height: f32, cam: &VCameraOpen) -> ShotCamera {
    ShotCamera {
        eye: [height * cam.eye_x, height * cam.eye_y, height * cam.eye_z],
        target: [0.0, height * cam.look_y, cam.look_z],
        fov: cam.fov,
    }
}

fn req<T>(id: &str, part: &str, field: &str, value: Option<T>) -> Result<T, String> {
    value.ok_or_else(|| format!("{id} {part} is missing {field}"))
}

fn req3(id: &str, part: &str, field: &str, value: Option<&[f32]>) -> Result<[f32; 3], String> {
    match value {
        Some([x, y, z, ..]) => Ok([*x, *y, *z]),
        _ => Err(format!("{id} {part} is missing {field}")),
    }
}

fn req2(id: &str, part: &str, field: &str, value: Option<&[f32]>) -> Result<[f32; 2], String> {
    match value {
        Some([x, y, ..]) => Ok([*x, *y]),
        _ => Err(format!("{id} {part} is missing {field}")),
    }
}

fn add_body_part(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens, catalog: &[VPart]) -> Result<(), String> {
    match spec.part.as_str() {
        "pod" | "bud" => return add_sac_orbs(parts, id, spec, dens, catalog),
        "crown" => return add_crown(parts, id, spec, dens),
        "cannon" => return add_cannon(parts, id, spec, dens, catalog),
        _ => {}
    }
    let gate = spec.gate();
    let emissive = spec.emissive;
    let palette = spec.color.clone();
    let count = if spec.mirror { 2 } else { 1 };
    let size = size3(id, spec)?;
    let (swing_hz, swing_amp) = match spec.part.as_str() {
        "mandible" | "forelimb" | "scythe" => (
            req(id, &spec.name, "swing_hz", spec.swing_hz)?,
            req(id, &spec.name, "swing_amp", spec.swing_amp)?,
        ),
        _ => (0.0, 0.0),
    };
    let limb = if matches!(spec.part.as_str(), "forelimb" | "scythe") {
        Some(LimbSpec {
            aim: req3(id, &spec.name, "aim", spec.aim.as_deref())?,
            seg: req(id, &spec.name, "seg", spec.seg)?,
            rad: req(id, &spec.name, "rad", spec.rad)?,
            taper: req(id, &spec.name, "taper", spec.taper)?,
            blade: req(id, &spec.name, "blade", spec.blade)?,
            blade_euler: req3(id, &spec.name, "blade_euler", spec.blade_euler.as_deref())?,
            blade_r: req2(id, &spec.name, "blade_r", spec.blade_r.as_deref())?,
        })
    } else {
        None
    };
    for i in 0..count {
        let side = if i == 0 { 1.0 } else { -1.0 };
        let mut pos = spec.offset;
        if spec.mirror {
            pos[0] = spec.offset[0].abs() * side;
        }
        let mut euler = [0.0_f32, 0.0, 0.0];
        let (mesh, joint) = match spec.part.as_str() {
            // Weak points are clean gems at the authored diameter so they read
            // even when bloom is off. No plates on top of them.
            "gem" => {
                let radii = if spec.emissive {
                    let r = glow_radius(id, spec)?;
                    [r, r, r]
                } else {
                    [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5]
                };
                (gem_mesh(radii, dens), Joint::Sac)
            }
            "eyes" => {
                let r = glow_radius(id, spec)?;
                (gem_mesh([r, r, r], &Dens::small()), Joint::Static)
            }
            "shell" => {
                let rear = req(id, &spec.name, "rear_open", spec.rear_open)?;
                (dome_shell(&size, dens, rear), Joint::Static)
            }
            "thorax" => {
                // Negative tilt raises the back (the sac) and drops the head.
                euler[0] = -spec.tilt_deg.to_radians();
                (thorax_mesh(&size, dens), Joint::Static)
            }
            "torso" => {
                euler[0] = spec.tilt_deg.to_radians();
                (torso_mesh(&size, dens), Joint::Static)
            }
            "head" if spec.shape == "cone" => {
                let point = req(id, &spec.name, "point", spec.point.clone())?;
                let len = size[1].max(size[0]);
                let rad = size[0].max(0.02);
                let sides = dens.sides.max(5);
                let mesh = match point.as_str() {
                    "up" => tyrant_head(len, rad, sides),
                    "forward" => spitter_head(len, rad, sides),
                    other => return Err(format!("{id} {} point \"{other}\" is unknown", spec.name)),
                };
                (mesh, Joint::Static)
            }
            "head" => {
                let k = req(id, &spec.name, "fit", spec.fit)?;
                (gem_mesh([size[0] * k, size[1] * k, size[2] * k], dens), Joint::Static)
            }
            "horn" => {
                euler[0] = -spec.tilt_deg.to_radians();
                (horn_mesh(size[1].max(size[0]), size[0].max(0.02), dens.sides.max(5)), Joint::Static)
            }
            "mandible" => {
                let yaw = req(id, &spec.name, "yaw", spec.yaw)?;
                let pitch = req(id, &spec.name, "pitch", spec.pitch)?;
                euler[1] = side * yaw;
                euler[0] = pitch;
                let rad = size[0].max(0.02);
                (
                    point_z(size[1].max(0.08), rad, rad * 0.42, dens.sides.max(4)),
                    Joint::Mandible { side },
                )
            }
            "armor" => (armor_wrap(&size, dens), Joint::Static),
            "forelimb" | "scythe" => {
                let limb = limb.as_ref().expect("limb");
                if size[1] <= 0.0 {
                    return Err(format!("{id} {} is missing its length", spec.name));
                }
                euler = aim_down([side * limb.aim[0], limb.aim[1], limb.aim[2]]);
                let rad = size[0] * limb.rad;
                let joint = if spec.part == "scythe" {
                    Joint::Scythe { side }
                } else {
                    Joint::Claw { side }
                };
                (segment_mesh(size[1] * limb.seg, rad, rad * limb.taper, dens.sides), joint)
            }
            _ => return Err(format!("{id} {} part type \"{}\" has no mesh", spec.name, spec.part)),
        };
        let key = format!("{id}:{}:{}", spec.name, if spec.mirror { "m" } else { "s" });
        let idx = parts.len();
        parts.push(Part {
            name: if spec.mirror {
                format!("{}{i}", spec.name)
            } else {
                spec.name.clone()
            },
            parent: None,
            rest_pos: pos,
            rest_euler: euler,
            mesh: Arc::new(mesh),
            palette: palette.clone(),
            emissive,
            joint,
            mesh_key: key,
            gate,
            foot: None,
            grows: false,
            grow_slot: None,
            pod_slot: None,
            sway_rate: 0.0,
            sway_spread: 0.0,
            sway_amp: 0.0,
            swing_hz,
            swing_amp,
            grown_scale: 0.0,
            grown_glow: 0.0,
        });
        if spec.part == "shell" {
            let rear = req(id, &spec.name, "rear_open", spec.rear_open)?;
            if dens.plates > 0 {
                for (name, side) in [("shell_plates_l", -1.0_f32), ("shell_plates_r", 1.0)] {
                    parts.push(Part {
                        name: name.into(),
                        parent: Some(idx),
                        rest_pos: [0.0, 0.0, 0.0],
                        rest_euler: [0.0, 0.0, 0.0],
                        mesh: Arc::new(flank_plates(&size, dens, rear, side)),
                        palette: palette.clone(),
                        emissive: false,
                        joint: Joint::Static,
                        mesh_key: format!("{id}:{name}"),
                        gate: Gate::Always,
                        foot: None,
                        grows: false,
                        grow_slot: None,
                        pod_slot: None,
                        sway_rate: 0.0,
                        sway_spread: 0.0,
                        sway_amp: 0.0,
                        swing_hz: 0.0,
                        swing_amp: 0.0,
                        grown_scale: 0.0,
                        grown_glow: 0.0,
                    });
                }
                // Magenta core between a closed shell's plates. Hidden until the shell opens.
                // An open rear (the swarmer) has no buried core.
                if !rear {
                let core_dens = Dens {
                    slices: 8,
                    stacks: 4,
                    ..Dens::small()
                };
                parts.push(Part {
                    name: "core".into(),
                    parent: Some(idx),
                    rest_pos: [0.0, 0.92, 0.08],
                    rest_euler: [0.0, 0.0, 0.0],
                    mesh: Arc::new(gem_mesh([0.55, 0.38, 0.62], &core_dens)),
                    palette: "brute_cracks".into(),
                    emissive: true,
                    joint: Joint::Static,
                    mesh_key: format!("{id}:core"),
                    gate: Gate::Vulnerable,
                    foot: None,
                    grows: false,
                    grow_slot: None,
                    pod_slot: None,
                    sway_rate: 0.0,
                    sway_spread: 0.0,
                    sway_amp: 0.0,
                    swing_hz: 0.0,
                    swing_amp: 0.0,
                    grown_scale: 0.0,
                    grown_glow: 0.0,
                });
                }
            }
        }
        if let Some(limb) = limb.as_ref() {
            let len = size[1];
            let rad = size[0] * limb.rad;
            let upper = len * limb.seg;
            let blade_len = len * limb.blade;
            let blade_euler = [limb.blade_euler[0], side * limb.blade_euler[1], side * limb.blade_euler[2]];
            let blade = if spec.part == "scythe" {
                curved_blade(blade_len, rad * limb.blade_r[0], (rad * limb.blade_r[1]).max(rad * 0.18))
            } else {
                segment_mesh(blade_len, rad * limb.blade_r[0], rad * limb.blade_r[1].max(rad * 0.22), dens.sides.max(4))
            };
            let joint = if spec.part == "scythe" {
                Joint::Scythe { side }
            } else {
                Joint::Claw { side }
            };
            parts.push(Part {
                name: format!("{}_blade{i}", spec.name),
                parent: Some(idx),
                rest_pos: [0.0, -upper * 0.9, 0.0],
                rest_euler: blade_euler,
                mesh: Arc::new(blade),
                palette: palette.clone(),
                emissive: false,
                joint,
                mesh_key: format!("{id}:{}:blade", spec.name),
                gate,
                foot: None,
                grows: false,
                grow_slot: None,
                pod_slot: None,
                sway_rate: 0.0,
                sway_spread: 0.0,
                sway_amp: 0.0,
                swing_hz,
                swing_amp,
                grown_scale: 0.0,
                grown_glow: 0.0,
            });
        }
    }
    Ok(())
}

struct LimbSpec {
    aim: [f32; 3],
    seg: f32,
    rad: f32,
    taper: f32,
    blade: f32,
    blade_euler: [f32; 3],
    blade_r: [f32; 2],
}

fn add_legs(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens, body_height: f32) -> Result<(), String> {
    if spec.count == 0 {
        return Err(format!("{id} {} is missing count", spec.name));
    }
    let count = spec.count;
    let rows = (count + 1) / 2;
    let length = spec
        .size
        .get(1)
        .copied()
        .filter(|n| *n > 0.0)
        .ok_or_else(|| format!("{id} {} is missing its length", spec.name))?;
    let radius = spec
        .size
        .first()
        .copied()
        .filter(|n| *n > 0.0)
        .ok_or_else(|| format!("{id} {} is missing its radius", spec.name))?;
    if spec.ring_radius <= 0.0 {
        return Err(format!("{id} {} is missing ring_radius", spec.name));
    }
    let ring = spec.ring_radius;
    let femur_frac = if dens.segs > 1 {
        req(id, &spec.name, "femur", spec.femur)?
    } else {
        req(id, &spec.name, "femur_lod", spec.femur_lod)?
    };
    let tibia_frac = req(id, &spec.name, "tibia", spec.tibia)?;
    let foot_frac = req(id, &spec.name, "foot", spec.foot)?;
    let hip = req(id, &spec.name, "hip", spec.hip)?;
    let femur_len = length * femur_frac;
    let tibia_len = length * tibia_frac;
    let foot_len = length * foot_frac;
    let femur_mesh = Arc::new(segment_mesh(femur_len, radius * 1.85, radius * 1.05, dens.sides));
    let tibia_mesh = Arc::new(segment_mesh(tibia_len, radius * 1.05, radius * 0.58, dens.sides.max(4)));
    let foot_mesh = Arc::new(segment_mesh(foot_len, radius * 0.7, radius * 0.28, 4));
    // Hips sit on the underside of the body. Feet plant wide of the shell.
    let hip_y = body_height * hip;
    let layout = spec.layout.as_str();
    if layout.is_empty() {
        return Err(format!("{id} {} is missing layout", spec.name));
    }
    let radial = match layout {
        "radial" => true,
        "paired" => false,
        other => return Err(format!("{id} {} layout \"{other}\" is unknown", spec.name)),
    };
    let hip_scale = if radial {
        req(id, &spec.name, "hip_scale", spec.hip_scale)?
    } else {
        0.0
    };
    let plant_scale = if radial {
        req(id, &spec.name, "plant_scale", spec.plant_scale)?
    } else {
        0.0
    };
    let phase = if radial {
        req(id, &spec.name, "ring_phase", spec.ring_phase)?
    } else {
        0.0
    };
    for i in 0..count {
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let row = i / 2;
        let t = if rows <= 1 { 0.5 } else { row as f32 / (rows as f32 - 1.0) };
        let along = 0.42 - t;
        let (hip, plant) = if radial {
            let ang = i as f32 / count as f32 * std::f32::consts::TAU + phase;
            (
                [ang.cos() * ring * hip_scale, hip_y, ang.sin() * ring * hip_scale],
                [ang.cos() * ring * plant_scale, 0.0, ang.sin() * ring * plant_scale],
            )
        } else {
            (
                [side * ring * 0.78, hip_y, along * ring * 0.72],
                [side * ring * 1.42, 0.0, along * ring * 1.15],
            )
        };
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
            grows: false,
            grow_slot: None,
            pod_slot: None,
            sway_rate: 0.0,
            sway_spread: 0.0,
            sway_amp: 0.0,
            swing_hz: 0.0,
            swing_amp: 0.0,
            grown_scale: 0.0,
            grown_glow: 0.0,
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
            grows: false,
            grow_slot: None,
            pod_slot: None,
            sway_rate: 0.0,
            sway_spread: 0.0,
            sway_amp: 0.0,
            swing_hz: 0.0,
            swing_amp: 0.0,
            grown_scale: 0.0,
            grown_glow: 0.0,
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
            grows: false,
            grow_slot: None,
            pod_slot: None,
            sway_rate: 0.0,
            sway_spread: 0.0,
            sway_amp: 0.0,
            swing_hz: 0.0,
            swing_amp: 0.0,
            grown_scale: 0.0,
            grown_glow: 0.0,
        });
    }
    Ok(())
}

fn add_tail(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens) -> Result<(), String> {
    if spec.segment_cap == 0 {
        return Err(format!("{id} tail is missing segment_cap"));
    }
    if spec.segment_scale <= 0.0 {
        return Err(format!("{id} tail is missing segment_scale"));
    }
    if spec.joint_span <= 0.0 {
        return Err(format!("{id} tail is missing joint_span"));
    }
    let len = spec.size.get(1).copied().ok_or_else(|| format!("{id} tail is missing its length"))?;
    if len <= 0.0 {
        return Err(format!("{id} tail length must be positive"));
    }
    let count = spec.count.max(1).min(spec.segment_cap);
    let radius = spec.size.first().copied().filter(|r| *r > 0.0).ok_or_else(|| format!("{id} tail is missing its radius"))?;
    let mesh = Arc::new(segment_mesh(
        len * spec.segment_scale,
        radius,
        radius * 0.78,
        dens.sides.max(4),
    ));
    // Limb mesh points along -Y. Positive X rotation swings that toward -Z and then up,
    // so the chain curls over the back instead of pitching forward.
    let step = spec.curl_deg.to_radians() / count as f32;
    let mut parent = None;
    for i in 0..count {
        let idx = parts.len();
        let pos = if i == 0 {
            spec.offset
        } else {
            [0.0, -len * spec.joint_span, 0.0]
        };
        let pitch = if i == 0 { spec.base_pitch } else { step * spec.curl_share };
        parts.push(Part {
            name: format!("tail{i}"),
            parent,
            rest_pos: pos,
            rest_euler: [pitch, 0.0, 0.0],
            mesh: mesh.clone(),
            palette: spec.color.clone(),
            emissive: false,
            joint: Joint::Tail { index: i },
            mesh_key: format!("{id}:tail"),
            gate: Gate::Always,
            foot: None,
            grows: false,
            grow_slot: None,
            pod_slot: None,
            sway_rate: spec.sway_rate,
            sway_spread: spec.sway_spread,
            sway_amp: spec.sway_amp,
            swing_hz: 0.0,
            swing_amp: 0.0,
            grown_scale: 0.0,
            grown_glow: 0.0,
        });
        parent = Some(idx);
    }
    Ok(())
}

fn add_cannon(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens, catalog: &[VPart]) -> Result<(), String> {
    let tail = catalog.iter().find(|part| part.name == "tail").ok_or_else(|| format!("{id} cannon has no tail part"))?;
    if tail.segment_scale <= 0.0 {
        return Err(format!("{id} cannon tail is missing segment_scale"));
    }
    let len = tail.size.get(1).copied().filter(|n| *n > 0.0).ok_or_else(|| format!("{id} cannon tail is missing its length"))?;
    let parent = parts.iter().rposition(|p| p.name.starts_with("tail")).ok_or_else(|| format!("{id} cannon has no tail segments"))?;
    let rad = glow_radius(id, spec)?;
    let tip_gap = tail.tip_gap;
    let mut mesh = gem_mesh([rad, rad, rad * 0.92], dens);
    // The barrel stays inside the glow sphere, so the mesh is no bigger than visual_radius.
    let tip_r = rad * 0.32;
    let along = (rad * rad - tip_r * tip_r).max(0.0).sqrt();
    let barrel = (along * 0.45).max(0.04);
    let mut nozzle = point_z(barrel, rad * 0.42, tip_r, dens.sides.max(5));
    geom::rotate_x(&mut nozzle, std::f32::consts::FRAC_PI_2);
    geom::translate(&mut nozzle, [0.0, barrel - along, 0.0]);
    geom::append(&mut mesh, &nozzle);
    // The hit center uses the authored size. A missing size must not fall back to the glow radius.
    let place_r = spec
        .size
        .first()
        .copied()
        .filter(|n| *n > 0.0)
        .expect("cannon size is missing a positive radius; refusing to fall back to the glow radius")
        * 0.5;
    let pos = [0.0, -(len * tail.segment_scale + place_r + tip_gap), 0.0];
    parts.push(Part {
        name: "cannon".into(),
        parent: Some(parent),
        rest_pos: pos,
        rest_euler: [0.0, 0.0, 0.0],
        mesh: Arc::new(mesh),
        palette: spec.color.clone(),
        emissive: spec.emissive,
        joint: Joint::Sac,
        mesh_key: format!("{id}:cannon"),
        gate: spec.gate(),
        foot: None,
            grows: false,
            grow_slot: None,
            pod_slot: None,
            sway_rate: 0.0,
            sway_spread: 0.0,
            sway_amp: 0.0,
            swing_hz: 0.0,
            swing_amp: 0.0,
            grown_scale: 0.0,
            grown_glow: 0.0,
    });
    Ok(())
}

fn add_sac_orbs(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens, catalog: &[VPart]) -> Result<(), String> {
    let nestled = spec.part == "bud";
    if spec.ring_radius <= 0.0 {
        return Err(format!("{id} {} is missing ring_radius", spec.name));
    }
    if spec.count == 0 {
        return Err(format!("{id} {} is missing count", spec.name));
    }
    let count = spec.count;
    let size = size3(id, spec)?;
    let sink = req(id, &spec.name, "sink", spec.sink)?;
    let phase = req(id, &spec.name, "ring_phase", spec.ring_phase)?;
    let ny_floor = req(id, &spec.name, "ny_floor", spec.ny_floor)?;
    let turn = req(id, &spec.name, "ring_turn", spec.ring_turn)?;
    let (grown_scale, grown_glow) = if nestled {
        (
            req(id, &spec.name, "grown_scale", spec.grown_scale)?,
            req(id, &spec.name, "grown_glow", spec.grown_glow)?,
        )
    } else {
        (0.0, 0.0)
    };
    let detail = if dens.slices <= 4 {
        Dens::small()
    } else if nestled {
        Dens { slices: 6, stacks: 4, ..Dens::small() }
    } else {
        Dens { slices: 6, stacks: 4, ..Dens::small() }
    };
    let rad = if spec.emissive {
        glow_radius(id, spec)?
    } else {
        size[0].max(size[1]).max(size[2]) * 0.5
    };
    let mesh = Arc::new(gem_mesh([rad, rad, rad], &detail));
    // Same ellipsoid as the sim pod bake: authored host half-extents, not the
    // faceted sac mesh. Pods and buds share that ring and alternate by phase.
    let host_name = spec.on.as_deref().ok_or_else(|| format!("{id} {} is missing on", spec.name))?;
    let host = catalog
        .iter()
        .find(|part| part.name == host_name)
        .ok_or_else(|| format!("{id} {} host \"{host_name}\" is missing", spec.name))?;
    let host_size = size3(id, host)?;
    let radii = [host_size[0] * 0.5, host_size[1] * 0.5, host_size[2] * 0.5];
    let center = host.offset;
    for i in 0..count {
        let a = (i as f32 + phase) / count as f32 * std::f32::consts::TAU + turn;
        let ring = spec.ring_radius;
        let nx = a.cos() * ring;
        let nz = a.sin() * ring;
        let ny = (1.0 - nx * nx - nz * nz).max(ny_floor).sqrt();
        let ring = [
            center[0] + nx * radii[0] * sink,
            center[1] + ny * radii[1] * sink,
            center[2] + nz * radii[2] * sink,
        ];
        let pos = spec
            .seats
            .iter()
            .find(|seat| seat.index == i)
            .map(|seat| seat.offset)
            .unwrap_or(ring);
        let name = if nestled { format!("bud{i}") } else { format!("pod{i}") };
        let pod_slot = if nestled { None } else { Some(i) };
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
            grows: nestled,
            grow_slot: if nestled { Some(i) } else { None },
            pod_slot,
            sway_rate: 0.0,
            sway_spread: 0.0,
            sway_amp: 0.0,
            swing_hz: 0.0,
            swing_amp: 0.0,
            grown_scale,
            grown_glow,
        });
        if !nestled {
            let toward = normalize([center[0] - pos[0], center[1] - pos[1], center[2] - pos[2]]);
            let pod_r = rad;
            parts.push(Part {
                name: format!("collar{i}"),
                parent: None,
                rest_pos: pos,
                rest_euler: [0.0, 0.0, 0.0],
                mesh: Arc::new(collar_mesh(toward, pod_r * 0.72, pod_r * 0.95, pod_r * 0.5)),
                palette: "spitter_carapace".into(),
                emissive: false,
                joint: Joint::Static,
                mesh_key: format!("{id}:collar{i}"),
                gate: Gate::Always,
                foot: None,
                grows: false,
                grow_slot: None,
                pod_slot,
                sway_rate: 0.0,
                sway_spread: 0.0,
                sway_amp: 0.0,
                swing_hz: 0.0,
                swing_amp: 0.0,
                grown_scale: 0.0,
                grown_glow: 0.0,
            });
        }
    }
    Ok(())
}

fn add_crown(parts: &mut Vec<Part>, id: &str, spec: &VPart, _dens: &Dens) -> Result<(), String> {
    if spec.count == 0 {
        return Err(format!("{id} {} is missing count", spec.name));
    }
    let count = spec.count;
    let host = spec.on.as_deref().ok_or_else(|| format!("{id} {} is missing on", spec.name))?;
    let head = parts
        .iter()
        .find(|p| p.name == host)
        .map(|p| p.rest_pos)
        .ok_or_else(|| format!("{id} {} host \"{host}\" is missing", spec.name))?;
    let len = spec
        .size
        .get(1)
        .copied()
        .filter(|n| *n > 0.0)
        .ok_or_else(|| format!("{id} {} is missing its length", spec.name))?;
    let rad = spec
        .size
        .first()
        .copied()
        .filter(|n| *n > 0.0)
        .ok_or_else(|| format!("{id} {} is missing its radius", spec.name))?;
    let spread = req3(id, &spec.name, "spread", spec.spread.as_deref())?;
    let pitch = req(id, &spec.name, "crown_pitch", spec.crown_pitch)?;
    let mesh = Arc::new(spike_up(len, rad * 0.85));
    for i in 0..count {
        let a = i as f32 / count as f32 * std::f32::consts::TAU;
        let pos = [
            head[0] + a.sin() * rad * spread[0],
            head[1] + len * spread[1],
            head[2] + a.cos() * rad * spread[2],
        ];
        parts.push(Part {
            name: format!("crown{i}"),
            parent: None,
            rest_pos: pos,
            rest_euler: [pitch, a, 0.0],
            mesh: mesh.clone(),
            palette: spec.color.clone(),
            emissive: false,
            joint: Joint::Static,
            mesh_key: format!("{id}:crown"),
            gate: Gate::Always,
            foot: None,
            grows: false,
            grow_slot: None,
            pod_slot: None,
            sway_rate: 0.0,
            sway_spread: 0.0,
            sway_amp: 0.0,
            swing_hz: 0.0,
            swing_amp: 0.0,
            grown_scale: 0.0,
            grown_glow: 0.0,
        });
    }
    Ok(())
}

fn add_decal(parts: &mut Vec<Part>, id: &str, spec: &VPart, dens: &Dens, shell: [f32; 3], shell_size: [f32; 3]) {
    // Thin lines in the closed seam. The open pose reveals the core instead of widening these.
    let radii = shell_radii(&shell_size, false);
    let bands = dorsal_bands(&radii, dens.plates.max(6), false);
    let mut seams = Vec::new();
    for i in 0..bands.len().saturating_sub(1) {
        let z = (bands[i].0 + bands[i + 1].0) * 0.5;
        if z > radii[2] * 0.02 {
            seams.push(z);
        }
    }
    if !(4..=6).contains(&seams.len()) {
        seams.clear();
        for n in 0..5 {
            let u = (n as f32 + 0.5) / 5.0;
            seams.push(radii[2] * (0.08 + u * 0.7));
        }
    }
    let mut mesh = MeshData::new();
    for seam in seams {
        seam_line(&mut mesh, &radii, seam);
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
            grows: false,
            grow_slot: None,
            pod_slot: None,
            sway_rate: 0.0,
            sway_spread: 0.0,
            sway_amp: 0.0,
            swing_hz: 0.0,
            swing_amp: 0.0,
            grown_scale: 0.0,
            grown_glow: 0.0,
    });
}

fn add_veins(parts: &mut Vec<Part>, id: &str, spec: &VPart) -> Result<(), String> {
    let legs: Vec<usize> = parts
        .iter()
        .enumerate()
        .filter_map(|(i, p)| match p.joint {
            Joint::Leg { segment: 0, .. } => Some(i),
            _ => None,
        })
        .collect();
    if legs.is_empty() {
        return Err(format!("{id} {} has no legs to attach to", spec.name));
    }
    // Thin enough that the head and chest sac stay readable past the legs.
    let mesh = Arc::new(segment_mesh(0.55, 0.04, 0.012, 4));
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
            rest_pos: [0.0, -0.2, parent_r + 0.06],
            rest_euler: [0.0, 0.0, 0.0],
            mesh: mesh.clone(),
            palette: spec.color.clone(),
            emissive: true,
            joint: Joint::Static,
            mesh_key: format!("{id}:vein"),
            gate: spec.gate(),
            foot: None,
            grows: false,
            grow_slot: None,
            pod_slot: None,
            sway_rate: 0.0,
            sway_spread: 0.0,
            sway_amp: 0.0,
            swing_hz: 0.0,
            swing_amp: 0.0,
            grown_scale: 0.0,
            grown_glow: 0.0,
        });
    }
    Ok(())
}

/// Round shell. Dorsal plates are a sibling mesh so the open pose can spread them.
fn dome_shell(size: &[f32; 3], dens: &Dens, rear_open: bool) -> MeshData {
    let mut mesh = MeshData::new();
    let radii = shell_radii(size, rear_open);
    if rear_open {
        // Low rounded carapace. A full gem at this size reads as a box.
        low_dome(&mut mesh, radii, dens.slices, dens.stacks);
    } else {
        geom::gem(&mut mesh, [0.0, 0.0, 0.0], radii, dens.slices, dens.stacks, WHITE);
    }
    mesh
}

/// One flank of the carapace. The two flanks meet on the midline while shut
/// and the open pose slides them apart.
fn flank_plates(size: &[f32; 3], dens: &Dens, rear_open: bool, side: f32) -> MeshData {
    let mut mesh = MeshData::new();
    if dens.plates == 0 {
        return mesh;
    }
    let radii = shell_radii(size, rear_open);
    let gap = plate_gap(radii[1]);
    let arch = plate_arch(radii[1]);
    let x = side * radii[0] * 0.32;
    let width = radii[0] * 0.5;
    for (n, (z, y, spacing)) in dorsal_bands(&radii, dens.plates, rear_open).into_iter().enumerate() {
        geom::plate(
            &mut mesh,
            [x, y + gap, z],
            (spacing * 0.7).max(0.02),
            width,
            arch,
            dens.nu.max(1),
            dens.nv.max(1),
            WHITE,
        );
        if dens.spines > 0 && n % 2 == 0 && (n as u32) < dens.spines * 2 {
            geom::spine(
                &mut mesh,
                [x, y + gap + arch + 0.012, z],
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

/// Curved plates that sit on the chest and leave a front window for the core.
fn armor_wrap(size: &[f32; 3], dens: &Dens) -> MeshData {
    let mut mesh = MeshData::new();
    let rx = size[0] * 0.36;
    let ry = size[1] * 0.40;
    let rz = (size[2] * 0.9).max(0.5);
    let bands = if dens.plates == 0 { 2 } else { 3 };
    let around = if dens.plates == 0 { 4 } else { 6 };
    for b in 0..bands {
        let v = (b as f32 + 0.5) / bands as f32;
        let y = (v - 0.5) * ry * 1.65;
        for k in 0..around {
            let u = (k as f32 + 0.5) / around as f32;
            let ang = (u - 0.5) * std::f32::consts::PI * 1.2;
            // The forward gap is the core window. Side and lower plates wrap the chest.
            if ang.abs() < 0.42 {
                continue;
            }
            let x = ang.sin() * rx;
            let z = ang.cos() * rz;
            let mut plate = MeshData::new();
            geom::plate(
                &mut plate,
                [0.0, 0.0, 0.0],
                ry * 0.52,
                rx * 0.48,
                rz * 0.14,
                dens.nu.max(1).min(2),
                dens.nv.max(1).min(2),
                WHITE,
            );
            // Plate arch (+Y) swings out to the shell normal.
            geom::rotate_x(&mut plate, std::f32::consts::FRAC_PI_2);
            geom::rotate_y(&mut plate, ang);
            geom::translate(&mut plate, [x, y, z]);
            geom::append(&mut mesh, &plate);
        }
    }
    mesh
}

fn spitter_head(length: f32, radius: f32, sides: u32) -> MeshData {
    let mut mesh = MeshData::new();
    geom::gem(
        &mut mesh,
        [0.0, 0.0, length * 0.18],
        [radius * 1.25, radius * 0.85, length * 0.52],
        sides.max(6).min(8),
        3,
        WHITE,
    );
    for side in [-1.0_f32, 1.0] {
        let mut jaw = segment_mesh(length * 0.78, radius * 0.36, radius * 0.14, 4);
        geom::rotate_x(&mut jaw, -1.05);
        geom::rotate_z(&mut jaw, side * 0.5);
        geom::translate(&mut jaw, [side * radius * 0.42, -radius * 0.08, length * 0.42]);
        geom::append(&mut mesh, &jaw);
    }
    mesh
}

fn tyrant_head(length: f32, radius: f32, _sides: u32) -> MeshData {
    let mut mesh = MeshData::new();
    // Wedge: wide at the back, a blunt edge at the front (+Z).
    let top = length * 0.28;
    let bot = -length * 0.22;
    let back = -radius * 0.45;
    let front = length * 0.78;
    let hw = radius * 1.2;
    let blt = [-hw, top, back];
    let brt = [hw, top, back];
    let ft = [0.0, top * 0.62, front];
    let blb = [-hw * 0.72, bot, back];
    let brb = [hw * 0.72, bot, back];
    let fb = [0.0, bot * 0.35, front * 0.9];
    geom::add_face(&mut mesh, blt, brt, ft, WHITE);
    geom::add_face(&mut mesh, blb, fb, brb, WHITE);
    geom::add_quad(&mut mesh, blt, ft, fb, blb, WHITE);
    geom::add_quad(&mut mesh, brt, brb, fb, ft, WHITE);
    geom::add_quad(&mut mesh, blt, blb, brb, brt, WHITE);
    for side in [-1.0_f32, 1.0] {
        let mut jaw = segment_mesh(length * 0.92, radius * 0.28, radius * 0.08, 4);
        geom::rotate_x(&mut jaw, -0.85);
        geom::rotate_z(&mut jaw, side * 0.4);
        geom::translate(&mut jaw, [side * radius * 0.55, bot * 0.2, front * 0.42]);
        geom::append(&mut mesh, &jaw);
    }
    mesh
}

/// Sharp crown spike. Local +Y, tip much narrower than the base.
fn spike_up(length: f32, radius: f32) -> MeshData {
    let mut mesh = segment_mesh(length, radius, radius * 0.08, 4);
    geom::rotate_x(&mut mesh, std::f32::consts::PI);
    mesh
}

/// Upper shell only, so a small carapace stays a dome instead of a faceted ball.
fn low_dome(mesh: &mut MeshData, radii: [f32; 3], slices: u32, stacks: u32) {
    let slices = slices.max(3);
    let stacks = stacks.max(2);
    let phi_max = 1.2_f32;
    let mut rings: Vec<Vec<[f32; 3]>> = Vec::new();
    for stack in 0..=stacks {
        let v = stack as f32 / stacks as f32;
        let phi = v * phi_max;
        let y = phi.cos();
        let r = phi.sin();
        let mut ring = Vec::new();
        for slice in 0..slices {
            let theta = slice as f32 / slices as f32 * std::f32::consts::TAU;
            ring.push([
                theta.cos() * r * radii[0],
                y * radii[1],
                theta.sin() * r * radii[2],
            ]);
        }
        rings.push(ring);
    }
    for s in 0..stacks as usize {
        for i in 0..slices as usize {
            let j = (i + 1) % slices as usize;
            geom::add_quad(mesh, rings[s][i], rings[s][j], rings[s + 1][j], rings[s + 1][i], WHITE);
        }
    }
}

fn horn_mesh(length: f32, radius: f32, sides: u32) -> MeshData {
    point_z(length, radius * 0.85, radius * 0.28, sides.max(5))
}

fn shell_radii(size: &[f32; 3], rear_open: bool) -> [f32; 3] {
    let mut radii = [size[0] * 0.47, size[1] * 0.47, size[2] * 0.47];
    if rear_open {
        radii[2] *= 0.84;
    }
    radii
}

fn plate_gap(radius: f32) -> f32 {
    (radius * 0.045).max(0.012)
}

fn plate_arch(radius: f32) -> f32 {
    (radius * 0.12).max(0.016)
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

fn glow_radius(id: &str, spec: &VPart) -> Result<f32, String> {
    if spec.visual_radius <= 0.0 {
        return Err(format!("{id} {} glow is missing visual_radius", spec.name));
    }
    if spec.part == "cannon" {
        let cap = gemini_sim::content::Content::embedded().cast.tail_glow_radius;
        if !(cap > 0.0) {
            return Err(format!("{id} tail glow cap is not positive"));
        }
        // The authored visual radius is the hit-zone glow. The tail tip is thinner.
        return Ok(spec.visual_radius.min(cap));
    }
    Ok(spec.visual_radius)
}

fn add_glow_orb(parts: &mut Vec<Part>, id: &str, spec: &VPart) -> Result<(), String> {
    let r = glow_radius(id, spec)?;
    parts.push(Part {
        name: spec.name.clone(),
        parent: None,
        rest_pos: spec.offset,
        rest_euler: [0.0, 0.0, 0.0],
        mesh: Arc::new(gem_mesh([r, r, r], &Dens::small())),
        palette: spec.color.clone(),
        emissive: true,
        joint: Joint::Static,
        mesh_key: format!("{id}:{}", spec.name),
        gate: spec.gate(),
        foot: None,
        grows: false,
        grow_slot: None,
        pod_slot: None,
        sway_rate: 0.0,
        sway_spread: 0.0,
        sway_amp: 0.0,
        swing_hz: 0.0,
        swing_amp: 0.0,
        grown_scale: 0.0,
        grown_glow: 0.0,
    });
    Ok(())
}

fn gem_mesh(radii: [f32; 3], dens: &Dens) -> MeshData {
    let mut mesh = MeshData::new();
    geom::gem(&mut mesh, [0.0, 0.0, 0.0], radii, dens.slices, dens.stacks, WHITE);
    mesh
}

/// Faceted frustum along +Z. The tip stays blunt so it is not a cone.
fn point_z(length: f32, r0: f32, r1: f32, sides: u32) -> MeshData {
    let mut mesh = segment_mesh(length, r0, r1.max(r0 * 0.22), sides);
    geom::rotate_x(&mut mesh, -std::f32::consts::FRAC_PI_2);
    mesh
}

/// Wide crescent. Local -Y is the reach; +Z is the curve. Cross-section is a
/// tapering diamond, not a box or a stick.
fn curved_blade(length: f32, width: f32, thick: f32) -> MeshData {
    let steps = 8u32;
    let mut mesh = MeshData::new();
    let mut rings = Vec::with_capacity(steps as usize + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let ang = 0.15 + t * 2.15;
        let y = -length * ang.sin() / 2.15;
        let z = length * (1.0 - (ang * 0.48).cos()) * 0.72;
        let w = width * (1.0 - t * 0.7).max(0.28);
        let th = thick * (1.0 - t * 0.25).max(thick * 0.45);
        rings.push([
            [0.0, y, z + th],
            [w, y, z + th * 0.12],
            [0.0, y, z - th * 0.55],
            [-w, y, z + th * 0.12],
        ]);
    }
    for i in 0..steps as usize {
        let a = rings[i];
        let b = rings[i + 1];
        for k in 0..4 {
            let n = (k + 1) % 4;
            geom::add_quad(&mut mesh, a[k], a[n], b[n], b[k], WHITE);
        }
    }
    let last = rings.len() - 1;
    geom::add_quad(&mut mesh, rings[0][0], rings[0][1], rings[0][2], rings[0][3], WHITE);
    geom::add_quad(&mut mesh, rings[last][0], rings[last][3], rings[last][2], rings[last][1], WHITE);
    mesh
}

/// A thin line sitting in the closed plate seam, above the shell and under the open core.
/// Dark collar where a pod meets the sac. `toward` points from the pod center into the sac.
fn collar_mesh(toward: [f32; 3], along: f32, outer: f32, inner: f32) -> MeshData {
    let n = normalize(toward);
    let hint = if n[1].abs() < 0.85 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let side = normalize(cross(n, hint));
    let bin = cross(n, side);
    let c = [n[0] * along, n[1] * along, n[2] * along];
    let mut mesh = MeshData::new();
    let steps = 8u32;
    let ring = |ang: f32, rad: f32| {
        let (s, co) = ang.sin_cos();
        [
            c[0] + (side[0] * co + bin[0] * s) * rad,
            c[1] + (side[1] * co + bin[1] * s) * rad,
            c[2] + (side[2] * co + bin[2] * s) * rad,
        ]
    };
    for i in 0..steps {
        let a0 = i as f32 / steps as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / steps as f32 * std::f32::consts::TAU;
        geom::add_quad(&mut mesh, ring(a0, outer), ring(a1, outer), ring(a1, inner), ring(a0, inner), WHITE);
    }
    mesh
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn seam_line(mesh: &mut MeshData, radii: &[f32; 3], seam_z: f32) {
    let steps = 4;
    let mut pts = Vec::with_capacity(steps + 1);
    let lift = plate_gap(radii[1]) + plate_arch(radii[1]) + 0.045;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let zig = if i % 2 == 0 { -1.0 } else { 1.0 };
        let x = zig * radii[0] * 0.035;
        let z = seam_z + (t - 0.5) * radii[2] * 0.16;
        let y = shell_surface_y(radii, x, z) + lift;
        pts.push([x, y, z]);
    }
    for w in pts.windows(2) {
        ribbon(mesh, w[0], w[1], 0.028);
    }
}

fn ribbon(mesh: &mut MeshData, a: [f32; 3], b: [f32; 3], half_w: f32) {
    let dx = b[0] - a[0];
    let dz = b[2] - a[2];
    let len = (dx * dx + dz * dz).sqrt().max(1e-4);
    let px = -dz / len * half_w;
    let pz = dx / len * half_w;
    let lift = half_w * 0.35;
    geom::add_quad(
        mesh,
        [a[0] - px, a[1] + lift, a[2] - pz],
        [a[0] + px, a[1] + lift, a[2] + pz],
        [b[0] + px, b[1] + lift, b[2] + pz],
        [b[0] - px, b[1] + lift, b[2] - pz],
        WHITE,
    );
}

fn shell_surface_y(radii: &[f32; 3], x: f32, z: f32) -> f32 {
    let nx = x / radii[0];
    let nz = z / radii[2];
    let r2 = nx * nx + nz * nz;
    let (x, z) = if r2 > 0.8 {
        let s = (0.8_f32).sqrt() / r2.sqrt();
        (x * s, z * s)
    } else {
        (x, z)
    };
    ellipse_y(radii, x, z)
}

fn segment_mesh(length: f32, r0: f32, r1: f32, sides: u32) -> MeshData {
    let mut mesh = MeshData::new();
    geom::limb(&mut mesh, length.max(0.04), r0.max(0.01), r1.max(0.008), sides.max(3), WHITE);
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

fn dens_of(src: &VDens) -> Result<Dens, String> {
    Ok(Dens {
        slices: src.slices,
        stacks: src.stacks,
        plates: src.plates,
        nu: src.nu,
        nv: src.nv,
        sides: src.sides,
        segs: src.segs,
        spines: src.spines,
        extra_plates: 0,
    })
}

fn size3(id: &str, spec: &VPart) -> Result<[f32; 3], String> {
    match spec.size.as_slice() {
        [x, y, z, ..] => Ok([*x, *y, *z]),
        [r, h] => Ok([*r * 2.0, *h, *r * 2.0]),
        [r] => Ok([*r, *r, *r]),
        _ => Err(format!("{id} {} is missing size", spec.name)),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum GateKind {
    Always,
    Vulnerable,
    PhaseMin,
}

#[derive(Deserialize)]
struct Row {
    visual: VVisual,
}

#[derive(Deserialize)]
struct VCamera {
    dist_mul: f32,
    dist_add: f32,
    eye_x: f32,
    eye_y: f32,
    eye_y_add: f32,
    eye_z: f32,
    look_y: f32,
    look_z: f32,
    fov: f32,
}

#[derive(Deserialize)]
struct VCameraOpen {
    eye_x: f32,
    eye_y: f32,
    eye_z: f32,
    look_y: f32,
    look_z: f32,
    fov: f32,
}

#[derive(Deserialize)]
struct VDens {
    slices: u32,
    stacks: u32,
    plates: u32,
    nu: u32,
    nv: u32,
    sides: u32,
    segs: u32,
    spines: u32,
}

#[derive(Deserialize)]
struct VVisual {
    height: f32,
    lod_distance: f32,
    camera: VCamera,
    #[serde(default)]
    camera_open: Option<VCameraOpen>,
    density: VDens,
    lod: VDens,
    parts: Vec<VPart>,
    anim: VAnim,
}

#[derive(Deserialize)]
struct VPart {
    name: String,
    #[serde(default)]
    part: String,
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
    /// Sphere radius of a glow. Separate from the hitbox. Zero unless this part is a glow.
    #[serde(default)]
    visual_radius: f32,
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
    curl_deg: f32,
    #[serde(default)]
    base_pitch: f32,
    #[serde(default)]
    curl_share: f32,
    #[serde(default)]
    joint_span: f32,
    #[serde(default)]
    segment_scale: f32,
    #[serde(default)]
    tip_gap: f32,
    #[serde(default)]
    segment_cap: u32,
    #[serde(default)]
    sway_rate: f32,
    #[serde(default)]
    sway_spread: f32,
    #[serde(default)]
    sway_amp: f32,
    #[serde(default)]
    pulse_phase: Option<u32>,
    #[serde(default)]
    rear_open: Option<bool>,
    #[serde(default)]
    point: Option<String>,
    #[serde(default)]
    fit: Option<f32>,
    #[serde(default)]
    yaw: Option<f32>,
    #[serde(default)]
    pitch: Option<f32>,
    #[serde(default)]
    aim: Option<Vec<f32>>,
    #[serde(default)]
    seg: Option<f32>,
    #[serde(default)]
    rad: Option<f32>,
    #[serde(default)]
    taper: Option<f32>,
    #[serde(default)]
    blade: Option<f32>,
    #[serde(default)]
    blade_euler: Option<Vec<f32>>,
    #[serde(default)]
    blade_r: Option<Vec<f32>>,
    #[serde(default)]
    swing_hz: Option<f32>,
    #[serde(default)]
    swing_amp: Option<f32>,
    #[serde(default)]
    hip: Option<f32>,
    #[serde(default)]
    femur: Option<f32>,
    #[serde(default)]
    femur_lod: Option<f32>,
    #[serde(default)]
    tibia: Option<f32>,
    #[serde(default)]
    foot: Option<f32>,
    /// "paired" or "radial". Required on a leg; empty fails the sculpt.
    #[serde(default)]
    layout: String,
    /// Radial hip distance, as a fraction of ring_radius.
    #[serde(default)]
    hip_scale: Option<f32>,
    /// Radial foot distance, as a fraction of ring_radius.
    #[serde(default)]
    plant_scale: Option<f32>,
    #[serde(default)]
    ring_phase: Option<f32>,
    #[serde(default)]
    sink: Option<f32>,
    #[serde(default)]
    ny_floor: Option<f32>,
    #[serde(default)]
    ring_turn: Option<f32>,
    #[serde(default)]
    grown_scale: Option<f32>,
    #[serde(default)]
    grown_glow: Option<f32>,
    #[serde(default)]
    spread: Option<Vec<f32>>,
    #[serde(default)]
    crown_pitch: Option<f32>,
    /// Pod indices that sit on the cast. The ring still places the rest.
    #[serde(default)]
    seats: Vec<VSeat>,
}

#[derive(Clone, Copy, Deserialize)]
struct VSeat {
    index: u32,
    offset: [f32; 3],
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

#[cfg(test)]
mod chain_errors {
    use super::*;

    #[test]
    fn tail_missing_segment_cap_is_an_error() {
        let mut text = include_str!("../../../content/bugs.toml").to_string();
        text = text.replacen("segment_cap=12", "segment_cap=0", 1);
        let rows = rows_with_visual(&text).expect("mutated bugs.toml");
        let row = rows.iter().find(|(id, _)| id == "hive_tyrant").expect("hive");
        let err = sculpt("hive_tyrant", &row.1).expect_err("missing cap");
        assert!(err.contains("segment_cap"), "{err}");
    }

    #[test]
    fn cannon_missing_tail_length_is_an_error() {
        let mut text = include_str!("../../../content/bugs.toml").to_string();
        text = text.replacen(
            "{ name=\"tail\",   part=\"tail\",    shape=\"capsule\", size=[0.4,1.0]",
            "{ name=\"tail\",   part=\"tail\",    shape=\"capsule\", size=[0.4]",
            1,
        );
        let rows = rows_with_visual(&text).expect("mutated bugs.toml");
        let row = rows.iter().find(|(id, _)| id == "hive_tyrant").expect("hive");
        let err = sculpt("hive_tyrant", &row.1).expect_err("missing length");
        assert!(err.contains("length"), "{err}");
    }
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
