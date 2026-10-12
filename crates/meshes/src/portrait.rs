//! Hero-card meshes other than the pulse rifle. The rifle stays `viewmodel()`.

use crate::geom;
use crate::MeshData;

pub struct PortraitPart {
    pub mesh: MeshData,
    pub glow: bool,
}

const INK: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

pub fn build(mesh_id: &str) -> Option<Vec<PortraitPart>> {
    if !gemini_sim::content::PORTRAIT_MESHES.contains(&mesh_id) {
        return None;
    }
    match mesh_id {
        "pulse_rifle" => None,
        "gauntlet" => Some(gauntlet()),
        "slab_shield" => Some(slab_shield()),
        "marker_pistol" => Some(marker_pistol()),
        other => panic!("portrait mesh {other} is listed in PORTRAIT_MESHES but has no builder"),
    }
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

pub fn turn(parts: &[PortraitPart], yaw_deg: f32, pitch_deg: f32) -> Vec<PortraitPart> {
    parts
        .iter()
        .map(|part| PortraitPart {
            mesh: turn_mesh(&part.mesh, yaw_deg, pitch_deg),
            glow: part.glow,
        })
        .collect()
}

fn part(mesh: MeshData, glow: bool) -> PortraitPart {
    PortraitPart { mesh, glow }
}

/// `box_mesh` winds inward, so the camera-facing side is unlit. Flip those normals.
fn outward_box(mesh: &mut MeshData, center: [f32; 3], size: [f32; 3]) {
    let mut tmp = MeshData::new();
    geom::box_mesh(&mut tmp, center, size, INK);
    for n in &mut tmp.normals {
        n[0] = -n[0];
        n[1] = -n[1];
        n[2] = -n[2];
    }
    geom::append(mesh, &tmp);
}

fn gauntlet() -> Vec<PortraitPart> {
    // Palm faces the camera. The cuff is wider and sits below it, so the wrist reads.
    let mut body = MeshData::new();
    outward_box(&mut body, [0.0, -0.15, -0.64], [0.20, 0.08, 0.16]);
    outward_box(&mut body, [0.0, 0.03, -0.50], [0.13, 0.13, 0.05]);
    for i in 0..4 {
        let spread = (i as f32 - 1.5) * 0.032;
        outward_box(&mut body, [spread, 0.14, -0.50], [0.02, 0.08, 0.03]);
    }
    let mut glow = MeshData::new();
    // Disc in the palm plane, thin along Z, so it aims at the camera.
    geom::gem(&mut glow, [0.0, 0.03, -0.47], [0.04, 0.04, 0.008], 12, 4, INK);
    ring_xy(&mut glow, [0.0, 0.03, -0.465], 0.058, 0.01, 14);
    vec![part(body, false), part(glow, true)]
}

fn slab_shield() -> Vec<PortraitPart> {
    let mut body = MeshData::new();
    let mut glow = MeshData::new();
    // Shaded bevel around a solid face. The accent is an inlay, not a rim behind the slab.
    let cx = -0.06;
    let cy = 0.0;
    let hx = 0.18;
    let hy = 0.15;
    let zf = -0.55;
    let bv = 0.055;
    let bd = 0.045;
    let x0 = cx - hx + bv;
    let x1 = cx + hx - bv;
    let y0 = cy - hy + bv;
    let y1 = cy + hy - bv;
    geom::add_quad(&mut body, [x0, y0, zf], [x1, y0, zf], [x1, y1, zf], [x0, y1, zf], INK);
    geom::add_quad(
        &mut body,
        [x0, y1, zf],
        [x1, y1, zf],
        [cx + hx, cy + hy, zf - bd],
        [cx - hx, cy + hy, zf - bd],
        INK,
    );
    geom::add_quad(
        &mut body,
        [x1, y0, zf],
        [x0, y0, zf],
        [cx - hx, cy - hy, zf - bd],
        [cx + hx, cy - hy, zf - bd],
        INK,
    );
    geom::add_quad(
        &mut body,
        [x0, y0, zf],
        [x0, y1, zf],
        [cx - hx, cy + hy, zf - bd],
        [cx - hx, cy - hy, zf - bd],
        INK,
    );
    geom::add_quad(
        &mut body,
        [x1, y1, zf],
        [x1, y0, zf],
        [cx + hx, cy - hy, zf - bd],
        [cx + hx, cy + hy, zf - bd],
        INK,
    );
    outward_box(&mut body, [cx, cy, zf - bd - 0.02], [hx * 2.0, hy * 2.0, 0.03]);
    let right = cx + hx;
    outward_box(&mut body, [right + 0.045, -0.01, zf - 0.01], [0.13, 0.042, 0.05]);
    // Vertical inlay on the face, so the accent is part of the slab.
    outward_box(&mut glow, [cx, cy, zf + 0.012], [0.028, hy * 1.15, 0.012]);
    vec![part(body, false), part(glow, true)]
}

fn marker_pistol() -> Vec<PortraitPart> {
    let mut body = MeshData::new();
    outward_box(&mut body, [0.0, 0.0, -0.58], [0.055, 0.065, 0.14]);
    // Grip drops clear of the receiver. Barrel sticks out the front.
    outward_box(&mut body, [0.0, -0.10, -0.54], [0.04, 0.13, 0.048]);
    outward_box(&mut body, [0.0, 0.012, -0.38], [0.044, 0.04, 0.18]);
    let mut glow = MeshData::new();
    geom::gem(&mut glow, [0.0, 0.012, -0.28], [0.018, 0.018, 0.01], 8, 4, INK);
    // Half the old fin (was 0.10 tall and 0.16 long).
    outward_box(&mut glow, [0.0, 0.055, -0.60], [0.012, 0.05, 0.08]);
    vec![part(body, false), part(glow, true)]
}

/// Ring in the view plane, so it circles a palm disc aimed at the camera.
fn ring_xy(mesh: &mut MeshData, center: [f32; 3], radius: f32, tube: f32, segments: u32) {
    for i in 0..segments {
        let a = i as f32 / segments as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        outward_box(
            mesh,
            [center[0] + c * radius, center[1] + s * radius, center[2]],
            [tube, tube, tube * 0.45],
        );
    }
}
