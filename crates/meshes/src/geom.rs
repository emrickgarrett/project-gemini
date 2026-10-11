//! Faceted primitives. Each face owns its vertices so normals stay flat.

use super::MeshData;

pub fn add_face(mesh: &mut MeshData, a: [f32; 3], b: [f32; 3], c: [f32; 3], color: [f32; 4]) {
    let n = face_normal(a, b, c);
    let base = mesh.positions.len() as u32;
    for p in [a, b, c] {
        mesh.positions.push(p);
        mesh.normals.push(n);
        mesh.colors.push(color);
    }
    mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
}

pub fn add_quad(mesh: &mut MeshData, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], color: [f32; 4]) {
    add_face(mesh, a, b, c, color);
    add_face(mesh, a, c, d, color);
}

pub fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ux = b[0] - a[0];
    let uy = b[1] - a[1];
    let uz = b[2] - a[2];
    let vx = c[0] - a[0];
    let vy = c[1] - a[1];
    let vz = c[2] - a[2];
    let nx = uy * vz - uz * vy;
    let ny = uz * vx - ux * vz;
    let nz = ux * vy - uy * vx;
    let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
    [nx / len, ny / len, nz / len]
}

pub fn box_mesh(mesh: &mut MeshData, center: [f32; 3], size: [f32; 3], color: [f32; 4]) {
    let hx = size[0] * 0.5;
    let hy = size[1] * 0.5;
    let hz = size[2] * 0.5;
    let (cx, cy, cz) = (center[0], center[1], center[2]);
    let p = |x: f32, y: f32, z: f32| [cx + x, cy + y, cz + z];
    add_quad(mesh, p(-hx, hy, -hz), p(hx, hy, -hz), p(hx, hy, hz), p(-hx, hy, hz), color);
    add_quad(mesh, p(-hx, -hy, hz), p(hx, -hy, hz), p(hx, -hy, -hz), p(-hx, -hy, -hz), color);
    add_quad(mesh, p(-hx, -hy, hz), p(-hx, hy, hz), p(hx, hy, hz), p(hx, -hy, hz), color);
    add_quad(mesh, p(hx, -hy, -hz), p(hx, hy, -hz), p(-hx, hy, -hz), p(-hx, -hy, -hz), color);
    add_quad(mesh, p(hx, -hy, hz), p(hx, hy, hz), p(hx, hy, -hz), p(hx, -hy, -hz), color);
    add_quad(mesh, p(-hx, -hy, -hz), p(-hx, hy, -hz), p(-hx, hy, hz), p(-hx, -hy, hz), color);
}

/// Tapered tube along local -Y. Origin is the upper joint.
pub fn limb(mesh: &mut MeshData, length: f32, r0: f32, r1: f32, sides: u32, color: [f32; 4]) {
    let sides = sides.max(3);
    let mut top = Vec::with_capacity(sides as usize);
    let mut bot = Vec::with_capacity(sides as usize);
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        top.push([c * r0, 0.0, s * r0]);
        bot.push([c * r1, -length, s * r1]);
    }
    for i in 0..sides as usize {
        let j = (i + 1) % sides as usize;
        add_quad(mesh, top[i], top[j], bot[j], bot[i], color);
    }
    fan(mesh, [0.0, 0.02, 0.0], &top, color, true);
    fan(mesh, [0.0, -length, 0.0], &bot, color, false);
}

pub fn cone(mesh: &mut MeshData, length: f32, radius: f32, sides: u32, color: [f32; 4]) {
    limb(mesh, length, radius, radius * 0.08, sides, color);
}

/// Faceted sphere. Flat faces, not a smooth icosphere.
pub fn gem(mesh: &mut MeshData, center: [f32; 3], radii: [f32; 3], slices: u32, stacks: u32, color: [f32; 4]) {
    let slices = slices.max(3);
    let stacks = stacks.max(2);
    let mut rings: Vec<Vec<[f32; 3]>> = Vec::new();
    for stack in 0..=stacks {
        let v = stack as f32 / stacks as f32;
        let phi = v * std::f32::consts::PI;
        let y = phi.cos();
        let r = phi.sin();
        let mut ring = Vec::new();
        for slice in 0..slices {
            let theta = slice as f32 / slices as f32 * std::f32::consts::TAU;
            ring.push([
                center[0] + theta.cos() * r * radii[0],
                center[1] + y * radii[1],
                center[2] + theta.sin() * r * radii[2],
            ]);
        }
        rings.push(ring);
    }
    for s in 0..stacks as usize {
        for i in 0..slices as usize {
            let j = (i + 1) % slices as usize;
            add_quad(mesh, rings[s][i], rings[s][j], rings[s + 1][j], rings[s + 1][i], color);
        }
    }
}

/// Overlapping carapace plate: a curved patch with a beveled rim.
pub fn plate(
    mesh: &mut MeshData,
    origin: [f32; 3],
    length: f32,
    width: f32,
    arch: f32,
    nu: u32,
    nv: u32,
    color: [f32; 4],
) {
    let nu = nu.max(1);
    let nv = nv.max(1);
    let mut grid = Vec::new();
    for iu in 0..=nu {
        let u = iu as f32 / nu as f32;
        let mut row = Vec::new();
        for iv in 0..=nv {
            let v = iv as f32 / nv as f32;
            let along = (u - 0.5) * length;
            let across = (v - 0.5) * width;
            let dome = (1.0 - (u * 2.0 - 1.0).powi(2)) * (1.0 - (v * 2.0 - 1.0).powi(2));
            let bevel = if iu == 0 || iv == 0 || iu == nu || iv == nv {
                arch * 0.35
            } else {
                arch
            };
            row.push([
                origin[0] + across,
                origin[1] + bevel * dome.max(0.0),
                origin[2] + along,
            ]);
        }
        grid.push(row);
    }
    for iu in 0..nu as usize {
        for iv in 0..nv as usize {
            add_quad(
                mesh,
                grid[iu][iv],
                grid[iu][iv + 1],
                grid[iu + 1][iv + 1],
                grid[iu + 1][iv],
                color,
            );
        }
    }
}

pub fn spine(mesh: &mut MeshData, base: [f32; 3], height: f32, radius: f32, color: [f32; 4]) {
    let tip = [base[0], base[1] + height, base[2]];
    let sides = 4;
    let mut ring = Vec::new();
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        ring.push([base[0] + a.cos() * radius, base[1], base[2] + a.sin() * radius]);
    }
    for i in 0..sides {
        let j = (i + 1) % sides;
        add_face(mesh, ring[i], ring[j], tip, color);
    }
}

fn fan(mesh: &mut MeshData, center: [f32; 3], ring: &[[f32; 3]], color: [f32; 4], flip: bool) {
    for i in 0..ring.len() {
        let j = (i + 1) % ring.len();
        if flip {
            add_face(mesh, center, ring[j], ring[i], color);
        } else {
            add_face(mesh, center, ring[i], ring[j], color);
        }
    }
}

pub fn translate(mesh: &mut MeshData, by: [f32; 3]) {
    for p in &mut mesh.positions {
        p[0] += by[0];
        p[1] += by[1];
        p[2] += by[2];
    }
}

pub fn rotate_x(mesh: &mut MeshData, angle: f32) {
    let (s, c) = angle.sin_cos();
    spin(mesh, |x, y, z| [x, y * c - z * s, y * s + z * c]);
}

#[allow(dead_code)]
pub fn rotate_y(mesh: &mut MeshData, angle: f32) {
    let (s, c) = angle.sin_cos();
    spin(mesh, |x, y, z| [x * c + z * s, y, -x * s + z * c]);
}

#[allow(dead_code)]
pub fn rotate_z(mesh: &mut MeshData, angle: f32) {
    let (s, c) = angle.sin_cos();
    spin(mesh, |x, y, z| [x * c - y * s, x * s + y * c, z]);
}

fn spin(mesh: &mut MeshData, f: impl Fn(f32, f32, f32) -> [f32; 3]) {
    for p in &mut mesh.positions {
        *p = f(p[0], p[1], p[2]);
    }
    for n in &mut mesh.normals {
        *n = f(n[0], n[1], n[2]);
    }
}

pub fn append(dst: &mut MeshData, src: &MeshData) {
    let base = dst.positions.len() as u32;
    dst.positions.extend_from_slice(&src.positions);
    dst.normals.extend_from_slice(&src.normals);
    dst.colors.extend_from_slice(&src.colors);
    dst.indices.extend(src.indices.iter().map(|i| i + base));
}
