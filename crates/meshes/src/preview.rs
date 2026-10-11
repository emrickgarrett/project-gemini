//! Software raster of a posed bug. Used by `bugshots` and the flat-preview test.
//! Front is the art +Z (heads, horns, glands). The camera sits on the +X/+Z quarter.

use crate::pose::{pose, AnimInput};
use crate::visual::{bug, EnemyVisual};

pub fn raster_bug(id: &str, width: u32, height: u32) -> Vec<u8> {
    let visual = bug(id);
    let palette = palette_of();
    raster(visual, &palette, width, height)
}

pub fn raster(visual: &EnemyVisual, palette: &dyn Fn(&str) -> [f32; 4], width: u32, height: u32) -> Vec<u8> {
    let w = width.max(8);
    let h = height.max(8);
    let mut color = vec![18u8, 16, 24].repeat((w * h) as usize);
    // A floor so the feet have something to stand on.
    for y in (h * 2 / 3)..h {
        for x in 0..w {
            let i = ((y * w + x) * 3) as usize;
            color[i] = 28;
            color[i + 1] = 26;
            color[i + 2] = 34;
        }
    }
    let mut depth = vec![f32::INFINITY; (w * h) as usize];
    let posed = pose(
        visual,
        AnimInput {
            time: 0.35,
            moving: false,
            charging: false,
            vulnerable: true,
            phase: 0,
            frenzy: false,
        },
        &[],
    );
    let dist = visual.height * 2.15 + 0.85;
    let eye = [dist * 0.72, visual.height * 0.55 + 0.35, dist * 0.96];
    let target = [0.0, visual.height * 0.42, 0.05];
    let forward = norm(sub(target, eye));
    let right = norm(cross(forward, [0.0, 1.0, 0.0]));
    let up = cross(right, forward);
    let light = norm([-0.35, 0.85, 0.4]);
    let fov = 32.0_f32.to_radians();
    let aspect = w as f32 / h as f32;
    let focal = 1.0 / (fov * 0.5).tan();

    for (part, xf) in visual.parts.iter().zip(posed.xforms.iter()) {
        if !xf.visible {
            continue;
        }
        let albedo = palette(part.palette.as_str());
        let mesh = &part.mesh;
        let mut verts = Vec::with_capacity(mesh.positions.len());
        for p in &mesh.positions {
            let scaled = [p[0] * xf.scale[0], p[1] * xf.scale[1], p[2] * xf.scale[2]];
            let r = crate::pose::rotate_vec(xf.rot, scaled);
            verts.push([r[0] + xf.pos[0], r[1] + xf.pos[1], r[2] + xf.pos[2]]);
        }
        for tri in mesh.indices.chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            let ia = tri[0] as usize;
            let ib = tri[1] as usize;
            let ic = tri[2] as usize;
            let n = mesh.normals[ia];
            let nw = crate::pose::rotate_vec(xf.rot, n);
            let ndot = (nw[0] * light[0] + nw[1] * light[1] + nw[2] * light[2]).clamp(0.0, 1.0);
            let shade = if part.emissive {
                0.85 + 0.15 * ndot
            } else {
                0.22 + 0.78 * ndot
            };
            let rgb = [
                (albedo[0] * shade * if part.emissive { xf.glow.max(1.0) } else { 1.0 }).clamp(0.0, 1.0),
                (albedo[1] * shade * if part.emissive { xf.glow.max(1.0) } else { 1.0 }).clamp(0.0, 1.0),
                (albedo[2] * shade * if part.emissive { xf.glow.max(1.0) } else { 1.0 }).clamp(0.0, 1.0),
            ];
            let Some(a) = project(verts[ia], eye, right, up, forward, focal, aspect, w, h) else {
                continue;
            };
            let Some(b) = project(verts[ib], eye, right, up, forward, focal, aspect, w, h) else {
                continue;
            };
            let Some(c) = project(verts[ic], eye, right, up, forward, focal, aspect, w, h) else {
                continue;
            };
            fill_tri(&mut color, &mut depth, w, h, a, b, c, rgb);
        }
    }
    color
}

pub fn palette_of() -> impl Fn(&str) -> [f32; 4] {
    let text = include_str!("../../../content/palette.toml");
    let keys: std::collections::HashMap<String, String> = {
        let mut map = std::collections::HashMap::new();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].to_string();
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let hex = v.trim().trim_matches('"').to_string();
            let key = if section == "keys" {
                k.trim().to_string()
            } else {
                format!("{}_{}", section, k.trim())
            };
            map.insert(key, hex);
        }
        map
    };
    move |name: &str| {
        let hex = keys.get(name).map(|s| s.as_str()).unwrap_or("#C67A2E");
        hex_rgb(hex)
    }
}

fn hex_rgb(hex: &str) -> [f32; 4] {
    let hex = hex.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(180) as f32 / 255.0;
    if hex.len() >= 6 {
        [byte(0), byte(2), byte(4), 1.0]
    } else {
        [0.7, 0.5, 0.2, 1.0]
    }
}

fn project(
    p: [f32; 3],
    eye: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    forward: [f32; 3],
    focal: f32,
    aspect: f32,
    w: u32,
    h: u32,
) -> Option<[f32; 3]> {
    let d = sub(p, eye);
    let z = dot(d, forward);
    if z < 0.05 {
        return None;
    }
    let x = dot(d, right) / z * focal / aspect;
    let y = dot(d, up) / z * focal;
    Some([
        (x * 0.5 + 0.5) * w as f32,
        (1.0 - (y * 0.5 + 0.5)) * h as f32,
        z,
    ])
}

fn fill_tri(
    color: &mut [u8],
    depth: &mut [f32],
    w: u32,
    h: u32,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    rgb: [f32; 3],
) {
    let min_x = a[0].min(b[0]).min(c[0]).floor().max(0.0) as i32;
    let max_x = a[0].max(b[0]).max(c[0]).ceil().min(w as f32 - 1.0) as i32;
    let min_y = a[1].min(b[1]).min(c[1]).floor().max(0.0) as i32;
    let max_y = a[1].max(b[1]).max(c[1]).ceil().min(h as f32 - 1.0) as i32;
    let area = edge(a, b, c);
    if area.abs() < 1e-4 {
        return;
    }
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = [x as f32 + 0.5, y as f32 + 0.5, 0.0];
            let w0 = edge(b, c, p) / area;
            let w1 = edge(c, a, p) / area;
            let w2 = edge(a, b, p) / area;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let z = a[2] * w0 + b[2] * w1 + c[2] * w2;
            let idx = (y as u32 * w + x as u32) as usize;
            if z < depth[idx] {
                depth[idx] = z;
                let o = idx * 3;
                color[o] = (rgb[0] * 255.0) as u8;
                color[o + 1] = (rgb[1] * 255.0) as u8;
                color[o + 2] = (rgb[2] * 255.0) as u8;
            }
        }
    }
}

fn edge(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

pub fn painted_pixels(rgb: &[u8]) -> usize {
    rgb.chunks(3)
        .filter(|p| p[0] > 40 || p[1] > 40 || p[2] > 40)
        .count()
}
