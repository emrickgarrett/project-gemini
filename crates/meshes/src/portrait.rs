//! Full-body hero cards. Geometry comes from the parsed heroes visual table.
//! Framing (yaw, pitch, fit) stays on the portrait row in characters.toml.

use crate::geom;
use crate::MeshData;
use gemini_sim::content::{HeroMesh, HeroesVisual};

pub struct PortraitPart {
    pub mesh: MeshData,
    pub palette: String,
    pub glow: bool,
}

const INK: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

pub fn build(visual: &HeroesVisual, mesh_id: &str) -> Option<Vec<PortraitPart>> {
    let body = visual.meshes.iter().find(|mesh| mesh.id == mesh_id)?;
    Some(assemble(body))
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

fn assemble(body: &HeroMesh) -> Vec<PortraitPart> {
    let mut out = Vec::new();
    for spec in &body.parts {
        let mut mesh = MeshData::new();
        match spec.shape.as_str() {
            "box" => outward_box(&mut mesh, spec.size),
            "bevel" => geom::bevel_box(&mut mesh, spec.size, spec.chamfer, INK),
            "gem" => geom::gem(&mut mesh, [0.0, 0.0, 0.0], spec.radii, spec.slices, spec.stacks, INK),
            "limb" => geom::limb(&mut mesh, spec.length, spec.r0, spec.r1, spec.sides, INK),
            "arc" => geom::arc_plate(&mut mesh, spec.width, spec.height, spec.thick, spec.bend_deg, spec.segments, INK),
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
