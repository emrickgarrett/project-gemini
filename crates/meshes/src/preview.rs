//! Software raster of a posed bug. Used by `bugshots` and the flat-preview test.
//!
//! Each still matches its concept sheet: flat bright light, a soft fill, and a
//! mid-grey field. Glow parts are written as their exact palette color so a
//! missing weak point fails the shot even when bloom is off.

use crate::pose::{pose, AnimInput};
use crate::visual::{bug, EnemyVisual};

const BG: [u8; 3] = [0x8A, 0x8A, 0x8A];
const SILHOUETTE: [u8; 3] = [0x29, 0x29, 0x29];

pub fn still_anim() -> AnimInput {
    AnimInput {
        time: 0.15,
        moving: false,
        charging: false,
        vulnerable: true,
        phase: 0,
        frenzy: false,
        preview_glow: true,
        regrow: 0,
    }
}

pub fn raster_bug(id: &str, width: u32, height: u32) -> Vec<u8> {
    let visual = bug(id);
    let palette = palette_of();
    draw(visual, &palette, width, height, false, None)
}

/// Color still from a visual that authors `camera_open` (the brute's open shell).
pub fn raster_crown(id: &str, width: u32, height: u32) -> Vec<u8> {
    let visual = bug(id);
    let palette = palette_of();
    let cam = visual
        .camera_open
        .unwrap_or_else(|| panic!("{id} has no camera_open"));
    draw(visual, &palette, width, height, false, Some((cam.eye, cam.target, cam.fov)))
}

pub fn raster(visual: &EnemyVisual, palette: &dyn Fn(&str) -> [f32; 4], width: u32, height: u32) -> Vec<u8> {
    draw(visual, palette, width, height, false, None)
}

/// Color panel on the left, untextured silhouette on the right. Each panel is square.
pub fn still(id: &str, panel: u32) -> (u32, u32, Vec<u8>) {
    compose(id, panel, None)
}

/// Same paired still as `still`, using `camera_open` when the bug authors one.
/// The brute's open-shell sheet is a three-quarter view with the silhouette.
pub fn still_open(id: &str, panel: u32) -> (u32, u32, Vec<u8>) {
    let visual = bug(id);
    let cam = visual.camera_open.map(|c| (c.eye, c.target, c.fov));
    compose(id, panel, cam)
}

fn compose(id: &str, panel: u32, camera: Option<([f32; 3], [f32; 3], f32)>) -> (u32, u32, Vec<u8>) {
    let panel = panel.max(8);
    let visual = bug(id);
    let palette = palette_of();
    let color = draw(visual, &palette, panel, panel, false, camera);
    let sil = draw(visual, &palette, panel, panel, true, camera);
    let w = panel * 2;
    let mut out = vec![0u8; (w * panel * 3) as usize];
    for y in 0..panel {
        for x in 0..panel {
            let s = ((y * panel + x) * 3) as usize;
            let left = ((y * w + x) * 3) as usize;
            let right = ((y * w + panel + x) * 3) as usize;
            out[left..left + 3].copy_from_slice(&color[s..s + 3]);
            out[right..right + 3].copy_from_slice(&sil[s..s + 3]);
        }
    }
    (w, panel, out)
}

/// Palette keys of visible glow parts that contributed no exact-color pixels.
pub fn missing_glow_keys(id: &str, rgb: &[u8]) -> Vec<String> {
    let visual = bug(id);
    let palette = palette_of();
    let posed = pose(visual, still_anim(), &[]);
    let mut keys = Vec::new();
    for (part, xf) in visual.parts.iter().zip(posed.xforms.iter()) {
        if part.emissive && xf.visible && !keys.contains(&part.palette) {
            keys.push(part.palette.clone());
        }
    }
    let mut missing = Vec::new();
    for key in keys {
        let c = palette(&key);
        let want = [byte(c[0]), byte(c[1]), byte(c[2])];
        let found = rgb.chunks(3).any(|p| p[0] == want[0] && p[1] == want[1] && p[2] == want[2]);
        if !found {
            missing.push(format!("{key} (#{:02X}{:02X}{:02X})", want[0], want[1], want[2]));
        }
    }
    missing
}

fn draw(
    visual: &EnemyVisual,
    palette: &dyn Fn(&str) -> [f32; 4],
    width: u32,
    height: u32,
    silhouette: bool,
    camera: Option<([f32; 3], [f32; 3], f32)>,
) -> Vec<u8> {
    let w = width.max(8);
    let h = height.max(8);
    let mut color = Vec::with_capacity((w * h * 3) as usize);
    for _ in 0..(w * h) {
        color.extend_from_slice(&BG);
    }
    let mut depth = vec![f32::INFINITY; (w * h) as usize];
    let posed = pose(visual, still_anim(), &[]);
    let (eye, target, fov_deg) = camera.unwrap_or((visual.camera.eye, visual.camera.target, visual.camera.fov));
    let forward = norm(sub(target, eye));
    let right = norm(cross(forward, [0.0, 1.0, 0.0]));
    let up = cross(right, forward);
    let key = norm([-0.2, 0.92, 0.28]);
    let fill = norm([0.55, 0.35, -0.45]);
    let fov = fov_deg.to_radians();
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
            let nw = crate::pose::rotate_vec(xf.rot, mesh.normals[ia]);
            let ndot = (nw[0] * key[0] + nw[1] * key[1] + nw[2] * key[2]).clamp(0.0, 1.0);
            let ndot2 = (nw[0] * fill[0] + nw[1] * fill[1] + nw[2] * fill[2]).clamp(0.0, 1.0);
            let shade = (0.62 + 0.40 * ndot + 0.22 * ndot2).clamp(0.0, 1.0);
            let rgb = if silhouette {
                [SILHOUETTE[0] as f32 / 255.0, SILHOUETTE[1] as f32 / 255.0, SILHOUETTE[2] as f32 / 255.0]
            } else if part.emissive {
                // Exact palette RGB. Unlit, so the color survives with bloom disabled.
                [albedo[0], albedo[1], albedo[2]]
            } else {
                [albedo[0] * shade, albedo[1] * shade, albedo[2] * shade]
            };
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
            // Hex colors are quoted (`"#7CFF3A"`). A hash starts a comment only outside quotes.
            let line = strip_hash_comment(line).trim();
            if line.is_empty() {
                continue;
            }
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

fn strip_hash_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

fn byte(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
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
    let br = byte(rgb[0]);
    let bg = byte(rgb[1]);
    let bb = byte(rgb[2]);
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
                color[o] = br;
                color[o + 1] = bg;
                color[o + 2] = bb;
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
        .filter(|p| {
            let dr = (p[0] as i16 - BG[0] as i16).abs();
            let dg = (p[1] as i16 - BG[1] as i16).abs();
            let db = (p[2] as i16 - BG[2] as i16).abs();
            dr + dg + db > 18
        })
        .count()
}
