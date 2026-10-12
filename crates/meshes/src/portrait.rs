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

pub fn yaw_mesh(mesh: &MeshData, yaw_deg: f32) -> MeshData {
    let (s, c) = yaw_deg.to_radians().sin_cos();
    let mut mesh = mesh.clone();
    for p in &mut mesh.positions {
        let x = p[0];
        let z = p[2];
        p[0] = c * x + s * z;
        p[2] = -s * x + c * z;
    }
    for n in &mut mesh.normals {
        let x = n[0];
        let z = n[2];
        n[0] = c * x + s * z;
        n[2] = -s * x + c * z;
    }
    mesh
}

pub fn yaw(parts: &[PortraitPart], yaw_deg: f32) -> Vec<PortraitPart> {
    parts
        .iter()
        .map(|part| PortraitPart {
            mesh: yaw_mesh(&part.mesh, yaw_deg),
            glow: part.glow,
        })
        .collect()
}

fn part(mesh: MeshData, glow: bool) -> PortraitPart {
    PortraitPart { mesh, glow }
}

fn gauntlet() -> Vec<PortraitPart> {
    let mut body = MeshData::new();
    geom::box_mesh(&mut body, [0.0, -0.02, -0.62], [0.16, 0.1, 0.22], INK);
    geom::gem(&mut body, [0.0, 0.02, -0.5], [0.11, 0.05, 0.1], 10, 6, INK);
    for i in 0..4 {
        let spread = (i as f32 - 1.5) * 0.035;
        geom::box_mesh(&mut body, [spread, -0.01, -0.42], [0.02, 0.018, 0.06], INK);
    }
    let mut glow = MeshData::new();
    geom::gem(
        &mut glow,
        [0.0, 0.045, -0.5],
        [0.055, 0.012, 0.055],
        12,
        4,
        INK,
    );
    ring(&mut glow, [0.0, 0.06, -0.5], 0.075, 0.012, 16);
    vec![part(body, false), part(glow, true)]
}

fn slab_shield() -> Vec<PortraitPart> {
    // The rim sits behind a smaller face, so the accent reads as the edge.
    // The face covers the left two thirds; a short barrel fills the right third.
    let mut body = MeshData::new();
    geom::box_mesh(&mut body, [-0.16, 0.0, -0.66], [0.40, 0.32, 0.04], INK);
    geom::box_mesh(&mut body, [0.22, -0.02, -0.64], [0.14, 0.045, 0.05], INK);
    let mut glow = MeshData::new();
    geom::box_mesh(&mut glow, [-0.16, 0.0, -0.72], [0.48, 0.40, 0.02], INK);
    vec![part(body, false), part(glow, true)]
}

fn marker_pistol() -> Vec<PortraitPart> {
    let mut body = MeshData::new();
    geom::box_mesh(&mut body, [0.02, -0.02, -0.58], [0.07, 0.09, 0.22], INK);
    geom::box_mesh(&mut body, [0.02, -0.1, -0.52], [0.04, 0.1, 0.05], INK);
    geom::box_mesh(&mut body, [0.02, 0.0, -0.44], [0.035, 0.035, 0.08], INK);
    let mut glow = MeshData::new();
    geom::gem(
        &mut glow,
        [0.02, 0.0, -0.39],
        [0.02, 0.02, 0.012],
        8,
        4,
        INK,
    );
    geom::box_mesh(&mut glow, [0.02, 0.08, -0.56], [0.012, 0.1, 0.16], INK);
    vec![part(body, false), part(glow, true)]
}

fn ring(mesh: &mut MeshData, center: [f32; 3], radius: f32, tube: f32, segments: u32) {
    for i in 0..segments {
        let a = i as f32 / segments as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        geom::box_mesh(
            mesh,
            [center[0] + c * radius, center[1], center[2] + s * radius],
            [tube, tube * 0.45, tube],
            INK,
        );
    }
}
