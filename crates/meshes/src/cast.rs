//! Meshy casts. The sim crate lists them. This crate hashes the embedded glb,
//! checks budgets, fits each mesh to its hitbox, and tints masked texels from palette keys.
//! A hash mismatch, an unknown id, or a blown budget fails the load. There is no
//! box-mesh fallback.

use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::OnceLock;

use gemini_sim::bugs::hit_bounds;
use gemini_sim::content::{CastModel, Content};
use sha2::{Digest, Sha256};

pub struct Tex {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

pub struct CastMesh {
    pub id: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub base: Tex,
    pub mask: Tex,
    /// sRGB shade map. Empty on a bug cast.
    pub shade: Tex,
    /// `weapon`, `hero`, `hitbox`, or `shield`. Channel fits use the shade map.
    pub fit: String,
    pub base_key: String,
    pub primary_key: String,
    pub secondary_key: String,
    pub accent_key: String,
    pub fourth_key: String,
    pub glow_key: String,
    pub rim_key: String,
    pub tris: usize,
    pub file_bytes: usize,
}

pub struct Placement {
    /// Spins a weapon around its grip before the portrait turn.
    pub pre_yaw: f32,
    pub pre_shift: [f32; 3],
    pub yaw_deg: f32,
    pub lean: f32,
    /// Portrait turns orbit this point, in the pre-shifted mesh.
    pub pivot: [f32; 3],
    pub shift: [f32; 3],
}

#[derive(Clone, Copy)]
pub struct PaintCam {
    pub eye: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub forward: [f32; 3],
    pub focal: f32,
    pub aspect: f32,
    pub w: u32,
    pub h: u32,
    pub key: [f32; 3],
    pub fill: [f32; 3],
}

pub fn casts() -> &'static [CastMesh] {
    static CELL: OnceLock<Vec<CastMesh>> = OnceLock::new();
    CELL.get_or_init(|| load_catalog().unwrap_or_else(|err| panic!("cast load failed: {err}")))
}

pub fn cast(id: &str) -> &'static CastMesh {
    casts()
        .iter()
        .find(|mesh| mesh.id == id)
        .unwrap_or_else(|| panic!("unknown model {id}"))
}

pub fn load_catalog() -> Result<Vec<CastMesh>, String> {
    let content = Content::embedded();
    let mut out = Vec::new();
    for spec in &content.models {
        let bytes = checked_model_bytes(spec)?;
        out.push(fit_model(bytes, spec, &content)?);
    }
    Ok(out)
}

/// Disk read for the native test that compares the shipped file to the embedded
/// copy. The load path does not call this. Wasm has no assets directory.
pub fn read_model_file(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|_| format!("missing model file {}", path.display()))
}

/// Lowercase SHA-256. The mesh load compares this to `CastModel.sha256`.
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
fn assets_file(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets").join(rel)
}

/// Hash the embedded glb. A mismatch fails the load. This does not read the disk,
/// so the browser can load the casts.
fn checked_model_bytes(spec: &CastModel) -> Result<&'static [u8], String> {
    let bytes = model_bytes(&spec.id)?;
    let got = sha256_hex(bytes);
    if got != spec.sha256 {
        return Err(format!(
            "{} mesh sha256 {got} does not match content {}",
            spec.id, spec.sha256
        ));
    }
    Ok(bytes)
}

pub fn model_bytes(id: &str) -> Result<&'static [u8], String> {
    Ok(match id {
        "vex_body" => include_bytes!("../../../assets/models/vex_body.glb"),
        "vex_gun" => include_bytes!("../../../assets/models/vex_gun.glb"),
        "nyra_body" => include_bytes!("../../../assets/models/nyra_body.glb"),
        "nyra_gun" => include_bytes!("../../../assets/models/nyra_gun.glb"),
        "brakk_body" => include_bytes!("../../../assets/models/brakk_body.glb"),
        "brakk_gun" => include_bytes!("../../../assets/models/brakk_gun.glb"),
        "brakk_shield" => include_bytes!("../../../assets/models/brakk_shield.glb"),
        "kip_body" => include_bytes!("../../../assets/models/kip_body.glb"),
        "kip_gun" => include_bytes!("../../../assets/models/kip_gun.glb"),
        "swarmer" => include_bytes!("../../../assets/models/swarmer.glb"),
        "spitter" => include_bytes!("../../../assets/models/spitter.glb"),
        "brute" => include_bytes!("../../../assets/models/brute.glb"),
        "brood_mother" => include_bytes!("../../../assets/models/brood_mother.glb"),
        "hive_tyrant" => include_bytes!("../../../assets/models/hive_tyrant.glb"),
        other => return Err(format!("unknown model file \"{other}\"")),
    })
}

fn mask_png(id: &str) -> Result<&'static [u8], String> {
    Ok(match id {
        "vex_body" => include_bytes!("../../../assets/masks/vex_body_mask_rgba.png"),
        "vex_gun" => include_bytes!("../../../assets/masks/vex_gun_mask_rgba.png"),
        "nyra_body" => include_bytes!("../../../assets/masks/nyra_body_mask_rgba.png"),
        "nyra_gun" => include_bytes!("../../../assets/masks/nyra_gun_mask_rgba.png"),
        "brakk_body" => include_bytes!("../../../assets/masks/brakk_body_mask_rgba.png"),
        "brakk_gun" => include_bytes!("../../../assets/masks/brakk_gun_mask_rgba.png"),
        "brakk_shield" => include_bytes!("../../../assets/masks/brakk_shield_mask_rgba.png"),
        "kip_body" => include_bytes!("../../../assets/masks/kip_body_mask_rgba.png"),
        "kip_gun" => include_bytes!("../../../assets/masks/kip_gun_mask_rgba.png"),
        "swarmer" => include_bytes!("../../../assets/masks/swarmer_mask.png"),
        "spitter" => include_bytes!("../../../assets/masks/spitter_mask.png"),
        "brute" => include_bytes!("../../../assets/masks/brute_mask.png"),
        "brood_mother" => include_bytes!("../../../assets/masks/brood_mother_mask.png"),
        "hive_tyrant" => include_bytes!("../../../assets/masks/hive_tyrant_mask.png"),
        other => return Err(format!("unknown mask file \"{other}\"")),
    })
}

fn shade_png(id: &str) -> Result<&'static [u8], String> {
    Ok(match id {
        "vex_body" => include_bytes!("../../../assets/masks/vex_body_shade.png"),
        "vex_gun" => include_bytes!("../../../assets/masks/vex_gun_shade.png"),
        "nyra_body" => include_bytes!("../../../assets/masks/nyra_body_shade.png"),
        "nyra_gun" => include_bytes!("../../../assets/masks/nyra_gun_shade.png"),
        "brakk_body" => include_bytes!("../../../assets/masks/brakk_body_shade.png"),
        "brakk_gun" => include_bytes!("../../../assets/masks/brakk_gun_shade.png"),
        "brakk_shield" => include_bytes!("../../../assets/masks/brakk_shield_shade.png"),
        "kip_body" => include_bytes!("../../../assets/masks/kip_body_shade.png"),
        "kip_gun" => include_bytes!("../../../assets/masks/kip_gun_shade.png"),
        other => return Err(format!("unknown shade file \"{other}\"")),
    })
}

/// 252 is 0.99 × 255. Every texel of a channel mask must carry at least this much R+G+B+A.
pub const MASK_MIN_BYTE_SUM: u16 = 252;

pub fn ensure_mask_weight(mask: &Tex, id: &str) -> Result<(), String> {
    let pixels = (mask.w as usize).saturating_mul(mask.h as usize);
    if pixels == 0 || mask.rgba.len() != pixels * 4 {
        return Err(format!("{id} mask is empty"));
    }
    for (i, px) in mask.rgba.chunks_exact(4).enumerate() {
        let sum = px[0] as u16 + px[1] as u16 + px[2] as u16 + px[3] as u16;
        if sum < MASK_MIN_BYTE_SUM {
            let x = (i as u32) % mask.w;
            let y = (i as u32) / mask.w;
            return Err(format!(
                "{id} mask texel ({x},{y}) has R+G+B+A weight {sum}/255, below {MASK_MIN_BYTE_SUM}"
            ));
        }
    }
    Ok(())
}

fn ensure_shade_hue_free(shade: &Tex, id: &str) -> Result<(), String> {
    let pixels = (shade.w as usize).saturating_mul(shade.h as usize);
    if pixels == 0 || shade.rgba.len() != pixels * 4 {
        return Err(format!("{id} shade map is empty"));
    }
    for (i, px) in shade.rgba.chunks_exact(4).enumerate() {
        if px[0] != px[1] || px[1] != px[2] {
            let x = (i as u32) % shade.w;
            let y = (i as u32) / shade.w;
            return Err(format!("{id} shade texel ({x},{y}) is not hue-free"));
        }
    }
    Ok(())
}

fn ensure_weighted_channels(mask: &Tex, keys: [&str; 4], id: &str) -> Result<(), String> {
    let names = ["R", "G", "B", "A"];
    for channel in 0..4 {
        let used = mask.rgba.chunks_exact(4).any(|px| px[channel] > 0);
        if used && keys[channel].is_empty() {
            return Err(format!("{id} mask {} has weight but no palette key", names[channel]));
        }
    }
    Ok(())
}

fn decode_mask(bytes: &[u8], id: &str) -> Result<Tex, String> {
    let image = image::load_from_memory(bytes).map_err(|err| format!("{id} mask: {err}"))?;
    let rgba = image.to_rgba8();
    Ok(Tex {
        w: rgba.width(),
        h: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

pub fn check_budget(id: &str, tris: usize, file_len: usize, tri_budget: u32, byte_budget: u32) -> Result<(), String> {
    if file_len > byte_budget as usize {
        return Err(format!("{id} is {file_len} bytes, budget {byte_budget}"));
    }
    if tris > tri_budget as usize {
        return Err(format!("{id} has {tris} triangles, budget {tri_budget}"));
    }
    Ok(())
}

fn fit_model(bytes: &[u8], spec: &CastModel, content: &Content) -> Result<CastMesh, String> {
    check_budget(&spec.id, 0, bytes.len(), spec.triangles, spec.bytes)?;
    let mut decoded = decode_glb(bytes, spec)?;
    // Island coverage uses every authored triangle. Needle slivers are dropped
    // from the draw afterwards; they still own the texels the mask was painted from.
    if matches!(spec.fit.as_str(), "weapon" | "shield") {
        decoded.mask = decode_mask(mask_png(&spec.id)?, &spec.id)?;
        require_island_coverage(&decoded, &spec.id, content.cast.min_island_coverage)?;
    }
    drop_needles(&mut decoded, content.cast.needle_extent, content.cast.needle_area);
    check_budget(&spec.id, decoded.tris, bytes.len(), spec.triangles, spec.bytes)?;
    match spec.fit.as_str() {
        "hitbox" => fit_hitbox(&mut decoded, spec, content)?,
        "hero" => fit_height(&mut decoded, spec.size)?,
        "weapon" => fit_weapon(&mut decoded, spec)?,
        "shield" => fit_shield(&mut decoded, spec)?,
        other => return Err(format!("{} fit \"{other}\" is unknown", spec.id)),
    }
    if spec.yaw_deg != 0.0 && spec.fit != "weapon" {
        yaw_about_center(&mut decoded.positions, &mut decoded.normals, spec.yaw_deg);
    }
    decoded.mask = decode_mask(mask_png(&spec.id)?, &spec.id)?;
    let shade = if spec.fit == "hitbox" {
        Tex { w: 0, h: 0, rgba: Vec::new() }
    } else {
        let shade = decode_mask(shade_png(&spec.id)?, &spec.id)?;
        ensure_shade_hue_free(&shade, &spec.id)?;
        ensure_mask_weight(&decoded.mask, &spec.id)?;
        ensure_weighted_channels(
            &decoded.mask,
            [&spec.primary, &spec.secondary, &spec.accent, &spec.fourth],
            &spec.id,
        )?;
        shade
    };
    Ok(CastMesh {
        id: spec.id.clone(),
        positions: decoded.positions,
        normals: decoded.normals,
        uvs: decoded.uvs,
        indices: decoded.indices,
        base: decoded.base,
        mask: decoded.mask,
        shade,
        fit: spec.fit.clone(),
        base_key: spec.base.clone(),
        primary_key: spec.primary.clone(),
        secondary_key: spec.secondary.clone(),
        accent_key: spec.accent.clone(),
        fourth_key: spec.fourth.clone(),
        glow_key: spec.glow.clone(),
        rim_key: spec.rim.clone(),
        tris: decoded.tris,
        file_bytes: bytes.len(),
    })
}

struct RawMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    base: Tex,
    mask: Tex,
    tris: usize,
    /// `final_rig` puts the grip at the origin. The primitive itself stays in Meshy's +X layout.
    gripped: bool,
}

/// A needle is a triangle whose longest edge is a large fraction of the model
/// and whose area is almost zero. Kip's left hip had a fan of these, and they
/// draw as stray straps. Real panels are not this thin.
fn drop_needles(mesh: &mut RawMesh, extent_frac: f32, area_ratio: f32) {
    if mesh.positions.is_empty() || mesh.indices.is_empty() {
        return;
    }
    let (mn, mx) = bounds(&mesh.positions);
    let extent = (mx[0] - mn[0]).max(mx[1] - mn[1]).max(mx[2] - mn[2]).max(1.0e-6);
    let mut keep = Vec::with_capacity(mesh.indices.len());
    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let a = mesh.positions[tri[0] as usize];
        let b = mesh.positions[tri[1] as usize];
        let c = mesh.positions[tri[2] as usize];
        let ab = sub(b, a);
        let ac = sub(c, a);
        let cross = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let area = 0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let longest = dist(a, b).max(dist(b, c)).max(dist(c, a));
        let ratio = area / (longest * longest).max(1.0e-12);
        if longest > extent_frac * extent && ratio < area_ratio {
            continue;
        }
        keep.extend_from_slice(tri);
    }
    mesh.indices = keep;
    mesh.tris = mesh.indices.len() / 3;
}

fn fit_hitbox(mesh: &mut RawMesh, spec: &CastModel, content: &Content) -> Result<(), String> {
    let row = content
        .bugs
        .rows()
        .into_iter()
        .find(|(id, _)| *id == spec.id)
        .ok_or_else(|| format!("{} has no hitbox", spec.id))?;
    let hit = hit_bounds(&row.1.hitbox.body);
    let (mn, mx) = bounds(&mesh.positions);
    let ms = sub(mx, mn);
    let hs = sub(hit.max, hit.min);
    let mut scale = [1.0_f32; 3];
    for i in 0..3 {
        if ms[i] <= 1e-6 || hs[i] <= 1e-6 {
            return Err(format!("{} has a zero extent on axis {i}", spec.id));
        }
        // Per axis, from the hitbox. A uniform min-scale leaves the short axis
        // about half full, which is how the tyrant shrank under main's figure.
        scale[i] = hs[i] / ms[i];
    }
    // Before any vertex moves. After the stretch, every axis fills the box.
    require_axis_scale(&spec.id, scale, content.cast.axis_scale_limit)?;
    let mc = mid(mn, mx);
    let hc = mid(hit.min, hit.max);
    for p in &mut mesh.positions {
        for i in 0..3 {
            p[i] = (p[i] - mc[i]) * scale[i] + hc[i];
        }
    }
    for n in &mut mesh.normals {
        let inv = [n[0] / scale[0], n[1] / scale[1], n[2] / scale[2]];
        *n = norm(inv);
    }
    let (mn, mx) = bounds(&mesh.positions);
    require_hitbox_fill(&spec.id, sub(mx, mn), hs, content.cast.hitbox_fill)?;
    Ok(())
}

/// `scale` is hitbox extent / unscaled mesh extent. Fail when max/min is over the limit.
fn require_axis_scale(id: &str, scale: [f32; 3], limit: f32) -> Result<(), String> {
    let mut max_s = scale[0];
    let mut min_s = scale[0];
    for axis in scale {
        if !(axis > 0.0) {
            return Err(format!("{id} axis scale {axis} is not positive"));
        }
        max_s = max_s.max(axis);
        min_s = min_s.min(axis);
    }
    let ratio = max_s / min_s;
    if ratio > limit {
        return Err(format!(
            "{id} axis scale max/min is {ratio:.3}, limit {limit:.3}"
        ));
    }
    Ok(())
}

/// Each UV island's texels must agree with the mask channel of the face that covers them.
/// The raster is Pillow's filled polygon at 4× (coordinates truncate toward zero, `v = 0`
/// is the top row, then the center of each 4×4 block). A point-in-triangle test does not
/// match the masks, so it cannot be the load check.
fn require_island_coverage(mesh: &RawMesh, id: &str, min_cov: f32) -> Result<(), String> {
    let nfaces = mesh.indices.len() / 3;
    if nfaces == 0 {
        return Err(format!("{id} has no triangles for island coverage"));
    }
    let mut parent: Vec<u32> = (0..nfaces as u32).collect();
    fn find(parent: &mut [u32], mut i: u32) -> u32 {
        while parent[i as usize] != i {
            parent[i as usize] = parent[parent[i as usize] as usize];
            i = parent[i as usize];
        }
        i
    }
    fn unite(parent: &mut [u32], a: u32, b: u32) {
        let a = find(parent, a);
        let b = find(parent, b);
        if a != b {
            parent[b as usize] = a;
        }
    }
    let mut owner = vec![u32::MAX; mesh.positions.len()];
    for (fi, tri) in mesh.indices.chunks(3).enumerate() {
        if tri.len() < 3 {
            continue;
        }
        for &vert in tri {
            let slot = vert as usize;
            if slot >= owner.len() || slot >= mesh.uvs.len() {
                return Err(format!("{id} index {vert} is outside the mesh"));
            }
            if owner[slot] == u32::MAX {
                owner[slot] = fi as u32;
            } else {
                unite(&mut parent, owner[slot], fi as u32);
            }
        }
    }
    let w = mesh.mask.w;
    let h = mesh.mask.h;
    if w < 2 || h < 2 {
        return Err(format!("{id} mask is too small for island coverage"));
    }
    const SS: i32 = 4;
    let Some(pw) = (w as i32).checked_mul(SS) else {
        return Err(format!("{id} mask is too wide for island coverage"));
    };
    let Some(ph) = (h as i32).checked_mul(SS) else {
        return Err(format!("{id} mask is too tall for island coverage"));
    };
    let mut face_id = vec![-1i32; pw as usize * ph as usize];
    for (fi, tri) in mesh.indices.chunks(3).enumerate() {
        if tri.len() < 3 {
            continue;
        }
        let mut pts = [[0i32; 2]; 3];
        for (k, &vert) in tri.iter().enumerate() {
            let uv = mesh.uvs[vert as usize];
            // Pillow casts the float coordinate with a truncating `(int)`.
            let x = (uv[0] as f64) * (w as f64) * (SS as f64);
            let y = (uv[1] as f64) * (h as f64) * (SS as f64);
            pts[k] = [x as i32, y as i32];
        }
        fill_triangle(&mut face_id, pw, ph, pts, fi as i32);
    }
    let mut face_counts = vec![[0u32; 4]; nfaces];
    let mut face_covered = vec![0u32; nfaces];
    for ty in 0..h as i32 {
        for tx in 0..w as i32 {
            let sx = tx * SS + SS / 2;
            let sy = ty * SS + SS / 2;
            let fi = face_id[(sy as usize) * (pw as usize) + sx as usize];
            if fi < 0 {
                continue;
            }
            let fi = fi as usize;
            face_covered[fi] += 1;
            if let Some(channel) = texel_channel(&mesh.mask, tx, ty) {
                face_counts[fi][channel] += 1;
            }
        }
    }
    let mut face_major = vec![0usize; nfaces];
    for fi in 0..nfaces {
        let mut best = 0u32;
        let mut which = 0usize;
        for (channel, count) in face_counts[fi].iter().enumerate() {
            if *count > best {
                best = *count;
                which = channel;
            }
        }
        face_major[fi] = which;
    }
    let mut island_hit = std::collections::HashMap::<u32, u32>::new();
    let mut island_covered = std::collections::HashMap::<u32, u32>::new();
    for ty in 0..h as i32 {
        for tx in 0..w as i32 {
            let sx = tx * SS + SS / 2;
            let sy = ty * SS + SS / 2;
            let fi = face_id[(sy as usize) * (pw as usize) + sx as usize];
            if fi < 0 {
                continue;
            }
            let fi = fi as usize;
            let root = find(&mut parent, fi as u32);
            *island_covered.entry(root).or_insert(0) += 1;
            let channel = texel_channel(&mesh.mask, tx, ty);
            let painted = face_counts[fi].iter().any(|count| *count > 0);
            if painted && channel == Some(face_major[fi]) {
                *island_hit.entry(root).or_insert(0) += 1;
            }
        }
    }
    if island_covered.is_empty() {
        return Err(format!("{id} island coverage rasterized no texels"));
    }
    let mut worst = 1.0_f32;
    let mut worst_id = 0u32;
    for (island, count) in &island_covered {
        if *count == 0 {
            continue;
        }
        let hit = island_hit.get(island).copied().unwrap_or(0);
        let frac = hit as f32 / *count as f32;
        if frac < worst {
            worst = frac;
            worst_id = *island;
        }
    }
    if worst < min_cov {
        let hit = island_hit.get(&worst_id).copied().unwrap_or(0);
        let count = island_covered.get(&worst_id).copied().unwrap_or(0);
        return Err(format!(
            "{id} island {worst_id} coverage is {worst:.3} ({hit}/{count}), need {min_cov:.2}"
        ));
    }
    Ok(())
}

fn texel_channel(tex: &Tex, x: i32, y: i32) -> Option<usize> {
    let i = ((y as u32 * tex.w + x as u32) * 4) as usize;
    let bytes = [
        tex.rgba[i],
        tex.rgba[i + 1],
        tex.rgba[i + 2],
        tex.rgba[i + 3],
    ];
    let mut best = 0u8;
    let mut index = 0usize;
    let mut ties = 0u8;
    for (channel, value) in bytes.iter().enumerate() {
        if *value > best {
            best = *value;
            index = channel;
            ties = 1;
        } else if *value == best {
            ties += 1;
        }
    }
    if best >= 180 && ties == 1 {
        Some(index)
    } else {
        None
    }
}

/// Pillow `ImagingDrawPolygon` fill for one triangle. Later faces overwrite earlier ones.
fn fill_triangle(buf: &mut [i32], width: i32, height: i32, pts: [[i32; 2]; 3], ink: i32) {
    let xy = [
        pts[0][0], pts[0][1], pts[1][0], pts[1][1], pts[2][0], pts[2][1],
    ];
    let count = 3usize;
    let mut edges: Vec<ScanEdge> = Vec::with_capacity(3);
    let mut i = 0usize;
    while i < count - 1 {
        let x0 = xy[i * 2];
        let y0 = xy[i * 2 + 1];
        let x1 = xy[i * 2 + 2];
        let y1 = xy[i * 2 + 3];
        if y0 == y1 && i != 0 && y0 == xy[i * 2 - 1] {
            let last = edges.last_mut().expect("a horizontal run follows an edge");
            if x1 > x0 && x0 > xy[i * 2 - 2] {
                last.xmax = x1;
                i += 1;
                continue;
            } else if x1 < x0 && x0 < xy[i * 2 - 2] {
                last.xmin = x1;
                i += 1;
                continue;
            }
        }
        edges.push(scan_edge(x0, y0, x1, y1));
        i += 1;
    }
    if xy[i * 2] != xy[0] || xy[i * 2 + 1] != xy[1] {
        edges.push(scan_edge(xy[i * 2], xy[i * 2 + 1], xy[0], xy[1]));
    }
    fill_polygon(buf, width, height, &edges, ink);
}

struct ScanEdge {
    x0: i32,
    y0: i32,
    xmin: i32,
    xmax: i32,
    ymin: i32,
    ymax: i32,
    dx: f32,
}

fn scan_edge(x0: i32, y0: i32, x1: i32, y1: i32) -> ScanEdge {
    let (xmin, xmax) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let (ymin, ymax) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    let dx = if y0 == y1 {
        0.0
    } else {
        (x1 - x0) as f32 / (y1 - y0) as f32
    };
    ScanEdge { x0, y0, xmin, xmax, ymin, ymax, dx }
}

fn fill_polygon(buf: &mut [i32], width: i32, height: i32, edges: &[ScanEdge], ink: i32) {
    if edges.is_empty() {
        return;
    }
    let mut ymin = height - 1;
    let mut ymax = 0;
    let mut active = Vec::new();
    for edge in edges {
        if ymin > edge.ymin {
            ymin = edge.ymin;
        }
        if ymax < edge.ymax {
            ymax = edge.ymax;
        }
        if edge.ymin == edge.ymax {
            hline(buf, width, height, edge.xmin, edge.ymin, edge.xmax, ink);
            continue;
        }
        active.push(edge);
    }
    if ymin < 0 {
        ymin = 0;
    }
    if ymax > height {
        ymax = height;
    }
    let mut y = ymin;
    while y <= ymax {
        let mut xx = Vec::new();
        for (i, current) in active.iter().enumerate() {
            if y < current.ymin || y > current.ymax {
                continue;
            }
            xx.push((y - current.y0) as f32 * current.dx + current.x0 as f32);
            if y == current.ymax && y < ymax {
                let dup = *xx.last().expect("intersection");
                xx.push(dup);
            } else if (y == current.ymin || y == current.ymax) && current.dx != 0.0 {
                let here = *xx.last().expect("intersection");
                for other in active.iter().take(i) {
                    if (y != other.ymin && y != other.ymax) || other.dx == 0.0 {
                        continue;
                    }
                    let other_x = (y - other.y0) as f32 * other.dx + other.x0 as f32;
                    if here.round() != other_x.round() {
                        continue;
                    }
                    let offset = if y == current.ymax { -1 } else { 1 };
                    let adjacent =
                        (y + offset - current.y0) as f32 * current.dx + current.x0 as f32;
                    if y + offset < other.ymin || y + offset > other.ymax {
                        continue;
                    }
                    let adjacent_other =
                        (y + offset - other.y0) as f32 * other.dx + other.x0 as f32;
                    let last = xx.last_mut().expect("intersection");
                    if *last > adjacent + 1.0 && *last > adjacent_other + 1.0 {
                        *last = adjacent.max(adjacent_other).round() + 1.0;
                    } else if *last < adjacent - 1.0 && *last < adjacent_other - 1.0 {
                        *last = adjacent.min(adjacent_other).round() - 1.0;
                    }
                    break;
                }
            }
        }
        xx.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut i = 1;
        while i < xx.len() {
            hline(
                buf,
                width,
                height,
                round_up_pix(xx[i - 1]),
                y,
                round_down_pix(xx[i]),
                ink,
            );
            i += 2;
        }
        y += 1;
    }
}

fn hline(buf: &mut [i32], width: i32, height: i32, mut x0: i32, y: i32, mut x1: i32, ink: i32) {
    if y < 0 || y >= height {
        return;
    }
    if x0 < 0 {
        x0 = 0;
    } else if x0 >= width {
        return;
    }
    if x1 < 0 {
        return;
    } else if x1 >= width {
        x1 = width - 1;
    }
    let row = (y as usize) * (width as usize);
    while x0 <= x1 {
        buf[row + x0 as usize] = ink;
        x0 += 1;
    }
}

/// Pillow `ROUND_UP` / `ROUND_DOWN`: half away from zero, via `floor(f + 0.5)` and `ceil(f - 0.5)`.
fn round_up_pix(f: f32) -> i32 {
    if f >= 0.0 {
        (f + 0.5).floor() as i32
    } else {
        -((f.abs() + 0.5).floor() as i32)
    }
}

fn round_down_pix(f: f32) -> i32 {
    if f >= 0.0 {
        (f - 0.5).ceil() as i32
    } else {
        -((f.abs() - 0.5).ceil() as i32)
    }
}

/// Each axis of the fitted mesh has to land on the body hitbox, within `tol`.
fn require_hitbox_fill(id: &str, mesh_ext: [f32; 3], hit_ext: [f32; 3], tol: f32) -> Result<(), String> {
    for i in 0..3 {
        if hit_ext[i] <= 1e-6 {
            return Err(format!("{id} hitbox extent on axis {i} is zero"));
        }
        let fill = mesh_ext[i] / hit_ext[i];
        if !(fill >= 1.0 - tol && fill <= 1.0 + tol) {
            return Err(format!(
                "{id} axis {i} fills {fill:.3} of the hitbox, need within {tol}"
            ));
        }
    }
    Ok(())
}

fn fit_height(mesh: &mut RawMesh, height: f32) -> Result<(), String> {
    let (mn, mx) = bounds(&mesh.positions);
    let h = mx[1] - mn[1];
    if h <= 1e-6 {
        return Err("hero height is zero".into());
    }
    let scale = height / h;
    let cx = (mn[0] + mx[0]) * 0.5;
    let cz = (mn[2] + mx[2]) * 0.5;
    for p in &mut mesh.positions {
        p[0] = (p[0] - cx) * scale;
        p[1] = (p[1] - mn[1]) * scale;
        p[2] = (p[2] - cz) * scale;
    }
    let (mn, mx) = bounds(&mesh.positions);
    if (mx[1] - mn[1] - height).abs() > 1.0e-2 || mn[1].abs() > 1.0e-2 {
        return Err(format!("hero scale check failed, height {} feet {}", mx[1] - mn[1], mn[1]));
    }
    Ok(())
}

fn fit_weapon(mesh: &mut RawMesh, spec: &CastModel) -> Result<(), String> {
    if !spec.grip_root || !mesh.gripped {
        return Err(format!("{} is a weapon without a grip root", spec.id));
    }
    // The rig root already parks the grip at the origin with the muzzle on +Z.
    // `yaw_deg` turns that muzzle onto the shot axis. Guns use 180 so it lands on -Z.
    yaw_positions(&mut mesh.positions, &mut mesh.normals, spec.yaw_deg);
    let (mn, mx) = bounds(&mesh.positions);
    let len = mx[2] - mn[2];
    if len <= 1e-6 {
        return Err(format!("{} has no length", spec.id));
    }
    let scale = spec.size / len;
    for p in &mut mesh.positions {
        for i in 0..3 {
            p[i] *= scale;
        }
    }
    Ok(())
}

fn fit_shield(mesh: &mut RawMesh, spec: &CastModel) -> Result<(), String> {
    if !spec.grip_root || !mesh.gripped {
        return Err(format!("{} is a shield without a grip root", spec.id));
    }
    let (mn, mx) = bounds(&mesh.positions);
    let h = mx[1] - mn[1];
    if h <= 1e-6 {
        return Err(format!("{} shield height is zero", spec.id));
    }
    let scale = spec.size / h;
    for p in &mut mesh.positions {
        for i in 0..3 {
            p[i] *= scale;
        }
    }
    Ok(())
}

fn yaw_about_center(positions: &mut [[f32; 3]], normals: &mut [[f32; 3]], yaw_deg: f32) {
    let (mn, mx) = bounds(positions);
    let c = mid(mn, mx);
    for p in positions.iter_mut() {
        p[0] -= c[0];
        p[1] -= c[1];
        p[2] -= c[2];
    }
    yaw_positions(positions, normals, yaw_deg);
    for p in positions.iter_mut() {
        p[0] += c[0];
        p[1] += c[1];
        p[2] += c[2];
    }
}

fn yaw_positions(positions: &mut [[f32; 3]], normals: &mut [[f32; 3]], yaw_deg: f32) {
    for p in positions.iter_mut() {
        *p = yaw_point(*p, yaw_deg);
    }
    for n in normals.iter_mut() {
        *n = yaw_point(*n, yaw_deg);
    }
}

fn yaw_point(p: [f32; 3], yaw_deg: f32) -> [f32; 3] {
    let (s, c) = yaw_deg.to_radians().sin_cos();
    [c * p[0] + s * p[2], p[1], -s * p[0] + c * p[2]]
}

fn bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for p in positions {
        for i in 0..3 {
            mn[i] = mn[i].min(p[i]);
            mx[i] = mx[i].max(p[i]);
        }
    }
    (mn, mx)
}

fn mid(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub(a, b);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Closest point on the mesh, and the distance to it.
pub fn closest_on_mesh(positions: &[[f32; 3]], indices: &[u32], point: [f32; 3]) -> ([f32; 3], f32) {
    let mut best = [0.0; 3];
    let mut best_d = f32::MAX;
    for tri in indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let q = closest_on_triangle(
            point,
            positions[tri[0] as usize],
            positions[tri[1] as usize],
            positions[tri[2] as usize],
        );
        let d = dist(point, q);
        if d < best_d {
            best = q;
            best_d = d;
        }
    }
    (best, best_d)
}

fn closest_on_triangle(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot3(ab, ap);
    let d2 = dot3(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot3(ab, bp);
    let d4 = dot3(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return [a[0] + ab[0] * v, a[1] + ab[1] * v, a[2] + ab[2] * v];
    }
    let cp = sub(p, c);
    let d5 = dot3(ab, cp);
    let d6 = dot3(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return [a[0] + ac[0] * w, a[1] + ac[1] * w, a[2] + ac[2] * w];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let bc = sub(c, b);
        return [b[0] + bc[0] * w, b[1] + bc[1] * w, b[2] + bc[2] * w];
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    [
        a[0] + ab[0] * v + ac[0] * w,
        a[1] + ab[1] * v + ac[1] * w,
        a[2] + ab[2] * v + ac[2] * w,
    ]
}

fn decode_glb(bytes: &[u8], spec: &CastModel) -> Result<RawMesh, String> {
    let id = spec.id.as_str();
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return Err(format!("{id} is not a glb"));
    }
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let json_start = 20;
    let json_end = json_start + json_len;
    if json_end + 8 > bytes.len() {
        return Err(format!("{id} glb json is truncated"));
    }
    let doc: serde_json::Value = serde_json::from_slice(&bytes[json_start..json_end])
        .map_err(|err| format!("{id} glb json: {err}"))?;
    let bin_len = u32::from_le_bytes(bytes[json_end..json_end + 4].try_into().unwrap()) as usize;
    let bin_start = json_end + 8;
    let bin_end = bin_start + bin_len;
    if bin_end > bytes.len() {
        return Err(format!("{id} glb bin is truncated"));
    }
    let bin = &bytes[bin_start..bin_end];
    let prim = doc["meshes"][0]["primitives"][0]
        .as_object()
        .ok_or_else(|| format!("{id} has no primitive"))?;
    let attrs = prim["attributes"].as_object().ok_or_else(|| format!("{id} has no attributes"))?;
    let positions = read_vec3(&doc, bin, attrs["POSITION"].as_u64().ok_or("POSITION")? as usize, id)?;
    let normal_acc = attrs
        .get("NORMAL")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("{id} has no NORMAL"))? as usize;
    let normals = read_vec3(&doc, bin, normal_acc, id)?;
    let uvs = read_vec2(&doc, bin, attrs["TEXCOORD_0"].as_u64().ok_or("TEXCOORD_0")? as usize, id)?;
    let index_acc = prim["indices"].as_u64().ok_or_else(|| format!("{id} has no indices"))? as usize;
    let indices = read_indices(&doc, bin, index_acc, id)?;
    let tris = indices.len() / 3;
    let mat = &doc["materials"][0];
    let base_tex = mat["pbrMetallicRoughness"]["baseColorTexture"]["index"]
        .as_u64()
        .ok_or_else(|| format!("{id} has no base color"))? as usize;
    let mask_tex = mat["extras"]["accent_mask_texture"]
        .as_u64()
        .ok_or_else(|| format!("{id} has no accent mask"))? as usize;
    let base = read_image(&doc, bin, base_tex, id)?;
    let mask = read_image(&doc, bin, mask_tex, id)?;
    let mut positions = positions;
    let mut normals = normals;
    let gripped = if spec.grip_root {
        apply_grip_root(&doc, &mut positions, &mut normals, id)?;
        true
    } else if grip_node_present(&doc) {
        return Err(format!("{id} has a grip root but content does not declare grip_root"));
    } else {
        false
    };
    Ok(RawMesh {
        positions,
        normals,
        uvs,
        indices,
        base,
        mask,
        tris,
        gripped,
    })
}

fn grip_node_present(doc: &serde_json::Value) -> bool {
    doc["nodes"].as_array().is_some_and(|nodes| {
        nodes.iter().any(|node| {
            node["name"]
                .as_str()
                .is_some_and(|name| name.ends_with("grip_root"))
        })
    })
}

/// `final_rig` parents a grip-rooted mesh under `*_grip_root`: grip at the origin, muzzle on +Z.
/// The matrix is column-major, matching glTF. A missing or non-finite matrix fails the load.
fn apply_grip_root(
    doc: &serde_json::Value,
    positions: &mut [[f32; 3]],
    normals: &mut [[f32; 3]],
    id: &str,
) -> Result<(), String> {
    let nodes = doc["nodes"]
        .as_array()
        .ok_or_else(|| format!("{id} glb has no nodes"))?;
    let mut found = None;
    for node in nodes {
        let Some(name) = node["name"].as_str() else {
            continue;
        };
        if !name.ends_with("grip_root") {
            continue;
        }
        if found.is_some() {
            return Err(format!("{id} has more than one grip root"));
        }
        found = Some(node);
    }
    let node = found.ok_or_else(|| format!("{id} is missing a grip root"))?;
    let list = node["matrix"]
        .as_array()
        .ok_or_else(|| format!("{id} grip root has no matrix"))?;
    if list.len() != 16 {
        return Err(format!("{id} grip root matrix must be 16 numbers"));
    }
    let mut m = [0.0f32; 16];
    for (i, value) in list.iter().enumerate() {
        let n = value
            .as_f64()
            .ok_or_else(|| format!("{id} grip root matrix is not a number"))?;
        if !n.is_finite() {
            return Err(format!("{id} grip root matrix is not finite"));
        }
        m[i] = n as f32;
    }
    let column = |base: usize| (m[base] * m[base] + m[base + 1] * m[base + 1] + m[base + 2] * m[base + 2]).sqrt();
    if column(0) < 1.0e-4 || column(4) < 1.0e-4 || column(8) < 1.0e-4 {
        return Err(format!("{id} grip root matrix is degenerate"));
    }
    for p in positions.iter_mut() {
        let x = m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12];
        let y = m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13];
        let z = m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14];
        *p = [x, y, z];
    }
    for n in normals.iter_mut() {
        let x = m[0] * n[0] + m[4] * n[1] + m[8] * n[2];
        let y = m[1] * n[0] + m[5] * n[1] + m[9] * n[2];
        let z = m[2] * n[0] + m[6] * n[1] + m[10] * n[2];
        *n = norm([x, y, z]);
    }
    Ok(())
}

fn read_image(doc: &serde_json::Value, bin: &[u8], texture: usize, id: &str) -> Result<Tex, String> {
    let source = doc["textures"][texture]["source"]
        .as_u64()
        .ok_or_else(|| format!("{id} texture {texture} has no source"))? as usize;
    let view = doc["images"][source]["bufferView"]
        .as_u64()
        .ok_or_else(|| format!("{id} image has no buffer view"))? as usize;
    let bytes = view_bytes(doc, bin, view, id)?;
    let image = image::load_from_memory(bytes).map_err(|err| format!("{id} image: {err}"))?;
    let rgba = image.to_rgba8();
    Ok(Tex {
        w: rgba.width(),
        h: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

fn view_bytes<'a>(doc: &serde_json::Value, bin: &'a [u8], view: usize, id: &str) -> Result<&'a [u8], String> {
    let v = &doc["bufferViews"][view];
    let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
    let len = v["byteLength"].as_u64().ok_or_else(|| format!("{id} view length"))? as usize;
    if off + len > bin.len() {
        return Err(format!("{id} buffer view is out of range"));
    }
    Ok(&bin[off..off + len])
}

fn read_vec3(doc: &serde_json::Value, bin: &[u8], acc: usize, id: &str) -> Result<Vec<[f32; 3]>, String> {
    let floats = read_f32(doc, bin, acc, 3, id)?;
    Ok(floats.chunks(3).map(|c| [c[0], c[1], c[2]]).collect())
}

fn read_vec2(doc: &serde_json::Value, bin: &[u8], acc: usize, id: &str) -> Result<Vec<[f32; 2]>, String> {
    let floats = read_f32(doc, bin, acc, 2, id)?;
    Ok(floats.chunks(2).map(|c| [c[0], c[1]]).collect())
}

fn read_f32(doc: &serde_json::Value, bin: &[u8], acc: usize, comps: usize, id: &str) -> Result<Vec<f32>, String> {
    let a = &doc["accessors"][acc];
    let view = a["bufferView"].as_u64().ok_or("view")? as usize;
    let count = a["count"].as_u64().ok_or("count")? as usize;
    let off = a["byteOffset"].as_u64().unwrap_or(0) as usize;
    let bytes = view_bytes(doc, bin, view, id)?;
    if off + count * comps * 4 > bytes.len() {
        return Err(format!("{id} accessor is truncated"));
    }
    let mut out = Vec::with_capacity(count * comps);
    for i in 0..count * comps {
        let s = off + i * 4;
        out.push(f32::from_le_bytes(bytes[s..s + 4].try_into().unwrap()));
    }
    Ok(out)
}

fn read_indices(doc: &serde_json::Value, bin: &[u8], acc: usize, id: &str) -> Result<Vec<u32>, String> {
    let a = &doc["accessors"][acc];
    let view = a["bufferView"].as_u64().ok_or("view")? as usize;
    let count = a["count"].as_u64().ok_or("count")? as usize;
    let kind = a["componentType"].as_u64().unwrap_or(5125);
    let bytes = view_bytes(doc, bin, view, id)?;
    let mut out = Vec::with_capacity(count);
    if kind == 5123 {
        if count * 2 > bytes.len() {
            return Err(format!("{id} indices are truncated"));
        }
        for i in 0..count {
            out.push(u16::from_le_bytes(bytes[i * 2..i * 2 + 2].try_into().unwrap()) as u32);
        }
    } else if kind == 5125 {
        if count * 4 > bytes.len() {
            return Err(format!("{id} indices are truncated"));
        }
        for i in 0..count {
            out.push(u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()));
        }
    } else {
        return Err(format!("{id} index type {kind} is unknown"));
    }
    Ok(out)
}

/// Albedo and emissive RGBA for the client. Colors come from palette keys.
/// A channel mask bakes `(shade / shade_mid_lin) * sum(weight * color)` in linear space.
/// Emissive is written only for palette keys whose `emissive` field is true.
/// Bug masks still treat green as the glow color.
pub fn bake_textures(mesh: &CastMesh, palette: &dyn Fn(&str) -> [f32; 4]) -> (u32, u32, Vec<u8>, Vec<u8>) {
    let channeled = mesh.fit != "hitbox";
    let (w, h) = if channeled {
        (mesh.shade.w.max(1), mesh.shade.h.max(1))
    } else {
        (mesh.base.w.max(1), mesh.base.h.max(1))
    };
    let job = paint_tint(mesh, palette);
    let fresnel = 0.0;
    let mut albedo = vec![0u8; (w * h * 4) as usize];
    let mut emit = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            // Image row 0 is v = 0. glTF and the mask PNGs share that top-left origin.
            let u = if w > 1 { x as f32 / (w as f32 - 1.0) } else { 0.0 };
            let v = if h > 1 { y as f32 / (h as f32 - 1.0) } else { 0.0 };
            let i = ((y * w + x) * 4) as usize;
            if channeled {
                let shade = sample(&mesh.shade, [u, v])[0];
                let mask = sample4(&mesh.mask, [u, v]);
                let (color, glow) = tint_channels(shade, mask, job.channels, job.emissive, job.shade_mid_lin, false);
                write_rgb(&mut albedo, i, color);
                if glow[0] + glow[1] + glow[2] > 0.0 {
                    write_rgb(&mut emit, i, glow);
                }
            } else {
                // Same top-left read as the software still. The client image is a 1:1 copy.
                let base = sample(&mesh.base, [u, v]);
                let mask = sample(&mesh.mask, [u, v]);
                let color = tint_texel(
                    base[0],
                    mask,
                    job.base_c,
                    job.accent,
                    job.glow,
                    job.rim,
                    fresnel,
                    job.shade_mid_lin,
                    job.rim_strength,
                    job.rim_fill,
                );
                write_rgb(&mut albedo, i, color);
                if mask[1] > 0.0 {
                    write_rgb(&mut emit, i, [job.glow[0], job.glow[1], job.glow[2]]);
                }
            }
        }
    }
    (w, h, albedo, emit)
}

/// White where the named mask channel has weight. The client multiplies this by the HDR gain.
pub fn bake_overdrive_emissive(mesh: &CastMesh, channel: &str) -> (u32, u32, Vec<u8>) {
    let index = gemini_sim::content::channel_index(channel)
        .unwrap_or_else(|| panic!("overdrive channel \"{channel}\" is unknown"));
    let (w, h) = (mesh.mask.w.max(1), mesh.mask.h.max(1));
    let mut emit = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let u = if w > 1 { x as f32 / (w as f32 - 1.0) } else { 0.0 };
            let v = if h > 1 { y as f32 / (h as f32 - 1.0) } else { 0.0 };
            let weight = sample4(&mesh.mask, [u, v])[index];
            let byte = (weight.clamp(0.0, 1.0) * 255.0).round() as u8;
            let i = ((y * w + x) * 4) as usize;
            emit[i] = byte;
            emit[i + 1] = byte;
            emit[i + 2] = byte;
            emit[i + 3] = 255;
        }
    }
    (w, h, emit)
}

fn write_rgb(buf: &mut [u8], i: usize, color: [f32; 3]) {
    buf[i] = (color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    buf[i + 1] = (color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    buf[i + 2] = (color[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    buf[i + 3] = 255;
}

struct PaintTint {
    base_c: [f32; 4],
    accent: [f32; 4],
    glow: [f32; 4],
    rim: [f32; 4],
    channels: [[f32; 3]; 4],
    emissive: [bool; 4],
    channeled: bool,
    shade_mid_lin: f32,
    rim_strength: f32,
    rim_fill: f32,
    overdrive_channel: Option<usize>,
    overdrive_toward: [f32; 3],
    overdrive_gain: [f32; 3],
    /// Sample the glb baseColorTexture through `sample4` and write those texels.
    /// The mask palette is not applied. This is the mesh-versus-reader check.
    embedded_base: bool,
}

fn paint_tint(mesh: &CastMesh, palette: &dyn Fn(&str) -> [f32; 4]) -> PaintTint {
    let (rim_strength, shade_mid_lin, rim_fill) = tint_look();
    let rgb = |key: &str| -> [f32; 3] {
        if key.is_empty() {
            [0.0, 0.0, 0.0]
        } else {
            let c = palette(key);
            [c[0], c[1], c[2]]
        }
    };
    let rgba = |key: &str| -> [f32; 4] {
        if key.is_empty() {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            palette(key)
        }
    };
    let keys = [
        mesh.primary_key.as_str(),
        mesh.secondary_key.as_str(),
        mesh.accent_key.as_str(),
        mesh.fourth_key.as_str(),
    ];
    PaintTint {
        base_c: rgba(&mesh.base_key),
        accent: rgba(&mesh.accent_key),
        glow: rgba(&mesh.glow_key),
        rim: if mesh.rim_key.is_empty() {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            let c = palette(&mesh.rim_key);
            [c[0], c[1], c[2], 1.0]
        },
        channels: [rgb(keys[0]), rgb(keys[1]), rgb(keys[2]), rgb(keys[3])],
        emissive: [
            channel_emissive(keys[0]),
            channel_emissive(keys[1]),
            channel_emissive(keys[2]),
            channel_emissive(keys[3]),
        ],
        channeled: mesh.fit != "hitbox",
        shade_mid_lin,
        rim_strength,
        rim_fill,
        overdrive_channel: None,
        overdrive_toward: [0.0, 0.0, 0.0],
        overdrive_gain: Content::embedded().cast.overdrive_emissive_gain,
        embedded_base: false,
    }
}

/// The glb's own baseColorTexture, through the same triangle walk and `sample4` as `paint`.
pub fn paint_embedded_base(
    color: &mut [u8],
    depth: &mut [f32],
    mesh: &CastMesh,
    place: Placement,
    palette: &dyn Fn(&str) -> [f32; 4],
    cam: &PaintCam,
) {
    let mut job = paint_tint(mesh, palette);
    job.embedded_base = true;
    let xform = |p: [f32; 3]| place_point(p, &place);
    let rotate_n = |n: [f32; 3]| place_normal(n, &place);
    paint_tris(color, depth, None, mesh, &xform, &rotate_n, cam, &job, false);
}

fn channel_emissive(key: &str) -> bool {
    if key.is_empty() {
        false
    } else {
        Content::embedded().palette.emissive(key)
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Linear albedo is `(shade / shade_mid_lin) * sum(weight * palette)`.
/// Palette colors are sRGB. `shade` is an sRGB texel unless `linear_shade` is set.
/// Hero shade PNGs are sRGB. A bug's embedded base is already the linear factor;
/// gamma-decoding it again sends a mid texel (~0.24) to ~0.05 and the body goes black.
/// The second color is the emissive contribution, and it is black unless a key's
/// `emissive` field is true.
pub fn tint_channels(
    shade: f32,
    weights: [f32; 4],
    colors_srgb: [[f32; 3]; 4],
    emissive: [bool; 4],
    shade_mid_lin: f32,
    linear_shade: bool,
) -> ([f32; 3], [f32; 3]) {
    let shade_lin = if linear_shade { shade } else { srgb_to_linear(shade) };
    let gain = shade_lin / shade_mid_lin;
    let mut sum = [0.0_f32; 3];
    let mut emit = [0.0_f32; 3];
    for i in 0..4 {
        let w = weights[i].clamp(0.0, 1.0);
        if w == 0.0 {
            continue;
        }
        let lin = [
            srgb_to_linear(colors_srgb[i][0]),
            srgb_to_linear(colors_srgb[i][1]),
            srgb_to_linear(colors_srgb[i][2]),
        ];
        for k in 0..3 {
            sum[k] += w * lin[k];
            if emissive[i] {
                emit[k] += w * lin[k];
            }
        }
    }
    let albedo = [
        linear_to_srgb(sum[0] * gain),
        linear_to_srgb(sum[1] * gain),
        linear_to_srgb(sum[2] * gain),
    ];
    let glow = [linear_to_srgb(emit[0]), linear_to_srgb(emit[1]), linear_to_srgb(emit[2])];
    (albedo, glow)
}

/// Bug mask. Red weights the accent. Green weights the glow.
/// An empty mask is the base palette key through the shared shade gain.
/// `shade_linear` is the glb base map (already linear), not an sRGB texel.
pub fn tint_texel(
    shade_linear: f32,
    mask: [f32; 3],
    base_c: [f32; 4],
    accent: [f32; 4],
    glow: [f32; 4],
    rim: [f32; 4],
    fresnel: f32,
    shade_mid_lin: f32,
    rim_strength: f32,
    rim_fill: f32,
) -> [f32; 3] {
    let accent_w = mask[0].clamp(0.0, 1.0);
    let glow_w = mask[1].clamp(0.0, 1.0);
    let body_w = (1.0 - accent_w - glow_w).clamp(0.0, 1.0);
    let (body, _) = tint_channels(
        shade_linear,
        [1.0, 0.0, 0.0, 0.0],
        [[base_c[0], base_c[1], base_c[2]], [0.0; 3], [0.0; 3], [0.0; 3]],
        [false; 4],
        shade_mid_lin,
        true,
    );
    let mut color = [
        body[0] * body_w + accent[0] * accent_w + glow[0] * glow_w,
        body[1] * body_w + accent[1] * accent_w + glow[1] * glow_w,
        body[2] * body_w + accent[2] * accent_w + glow[2] * glow_w,
    ];
    if rim[3] > 0.0 || rim[0] + rim[1] + rim[2] > 0.0 {
        let k = (rim_fill + (1.0 - rim_fill) * fresnel) * rim_strength * body_w;
        color[0] += rim[0] * k;
        color[1] += rim[1] * k;
        color[2] += rim[2] * k;
    }
    color
}

fn sample(tex: &Tex, uv: [f32; 2]) -> [f32; 3] {
    let px = sample4(tex, uv);
    [px[0], px[1], px[2]]
}

/// One mask read for every cast. `v = 0` is the top row, matching glTF and the PNGs.
fn sample4(tex: &Tex, uv: [f32; 2]) -> [f32; 4] {
    if tex.w == 0 || tex.h == 0 {
        return [0.5, 0.5, 0.5, 1.0];
    }
    let u = uv[0] - uv[0].floor();
    let v = uv[1] - uv[1].floor();
    let x = ((u * (tex.w as f32 - 1.0)).round() as u32).min(tex.w - 1);
    let y = ((v * (tex.h as f32 - 1.0)).round() as u32).min(tex.h - 1);
    let i = ((y * tex.w + x) * 4) as usize;
    [
        tex.rgba[i] as f32 / 255.0,
        tex.rgba[i + 1] as f32 / 255.0,
        tex.rgba[i + 2] as f32 / 255.0,
        tex.rgba[i + 3] as f32 / 255.0,
    ]
}

pub fn paint(
    color: &mut [u8],
    depth: &mut [f32],
    mesh: &CastMesh,
    place: Placement,
    palette: &dyn Fn(&str) -> [f32; 4],
    cam: &PaintCam,
    silhouette: bool,
) {
    let job = paint_tint(mesh, palette);
    let xform = |p: [f32; 3]| place_point(p, &place);
    let rotate_n = |n: [f32; 3]| place_normal(n, &place);
    paint_tris(color, depth, None, mesh, &xform, &rotate_n, cam, &job, silhouette);
}

/// First-person gun. Overdrive brightens `overdrive_emissive_channel` toward `hero_<id>`.
pub fn paint_approved_gun(
    color: &mut [u8],
    depth: &mut [f32],
    mesh: &CastMesh,
    place: Placement,
    palette: &dyn Fn(&str) -> [f32; 4],
    cam: &PaintCam,
    overdrive: bool,
) {
    let mut job = paint_tint(mesh, palette);
    if overdrive {
        let content = Content::embedded();
        let look = &content.cast;
        let hero = content
            .models
            .iter()
            .find(|model| model.id == mesh.id)
            .map(|model| model.hero.as_str())
            .filter(|hero| !hero.is_empty())
            .unwrap_or_else(|| panic!("{} has no hero id", mesh.id));
        let toward = palette(&format!("hero_{hero}"));
        job.overdrive_channel = Some(look.overdrive_channel_index());
        job.overdrive_toward = [toward[0], toward[1], toward[2]];
        job.overdrive_gain = look.overdrive_emissive_gain;
    }
    let xform = |p: [f32; 3]| place_point(p, &place);
    let rotate_n = |n: [f32; 3]| place_normal(n, &place);
    paint_tris(color, depth, None, mesh, &xform, &rotate_n, cam, &job, false);
}

/// Same raster as `paint`, plus a per-pixel tag. 2 is mask green, 1 is mask red.
#[cfg(test)]
fn paint_tagged(
    color: &mut [u8],
    depth: &mut [f32],
    tags: &mut [u8],
    mesh: &CastMesh,
    place: Placement,
    palette: &dyn Fn(&str) -> [f32; 4],
    cam: &PaintCam,
) {
    let job = paint_tint(mesh, palette);
    let xform = |p: [f32; 3]| place_point(p, &place);
    let rotate_n = |n: [f32; 3]| place_normal(n, &place);
    paint_tris(color, depth, Some(tags), mesh, &xform, &rotate_n, cam, &job, false);
}

fn paint_tris(
    color: &mut [u8],
    depth: &mut [f32],
    mut tags: Option<&mut [u8]>,
    mesh: &CastMesh,
    xform: &dyn Fn([f32; 3]) -> [f32; 3],
    rotate_n: &dyn Fn([f32; 3]) -> [f32; 3],
    cam: &PaintCam,
    job: &PaintTint,
    silhouette: bool,
) {
    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let ids = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let world = [xform(mesh.positions[ids[0]]), xform(mesh.positions[ids[1]]), xform(mesh.positions[ids[2]])];
        let Some(a) = project(world[0], cam) else { continue };
        let Some(b) = project(world[1], cam) else { continue };
        let Some(c) = project(world[2], cam) else { continue };
        fill_textured(
            color,
            depth,
            tags.as_deref_mut(),
            cam,
            mesh,
            ids,
            a,
            b,
            c,
            world,
            rotate_n,
            job,
            silhouette,
        );
    }
}

fn tint_look() -> (f32, f32, f32) {
    static CELL: OnceLock<(f32, f32, f32)> = OnceLock::new();
    *CELL.get_or_init(|| {
        let look = &Content::embedded().cast;
        (look.rim_strength, look.shade_mid_lin, look.rim_fill)
    })
}

fn fill_textured(
    color: &mut [u8],
    depth: &mut [f32],
    mut tags: Option<&mut [u8]>,
    cam: &PaintCam,
    mesh: &CastMesh,
    ids: [usize; 3],
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    world: [[f32; 3]; 3],
    rotate_n: &dyn Fn([f32; 3]) -> [f32; 3],
    job: &PaintTint,
    silhouette: bool,
) {
    let min_x = a[0].min(b[0]).min(c[0]).floor().max(0.0) as i32;
    let max_x = a[0].max(b[0]).max(c[0]).ceil().min(cam.w as f32 - 1.0) as i32;
    let min_y = a[1].min(b[1]).min(c[1]).floor().max(0.0) as i32;
    let max_y = a[1].max(b[1]).max(c[1]).ceil().min(cam.h as f32 - 1.0) as i32;
    let area = edge(a, b, c);
    if area.abs() < 1e-4 {
        return;
    }
    let uvs = [mesh.uvs[ids[0]], mesh.uvs[ids[1]], mesh.uvs[ids[2]]];
    let normals = [rotate_n(mesh.normals[ids[0]]), rotate_n(mesh.normals[ids[1]]), rotate_n(mesh.normals[ids[2]])];
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
            let idx = (y as u32 * cam.w + x as u32) as usize;
            if z >= depth[idx] {
                continue;
            }
            depth[idx] = z;
            let uv = [
                uvs[0][0] * w0 + uvs[1][0] * w1 + uvs[2][0] * w2,
                uvs[0][1] * w0 + uvs[1][1] * w1 + uvs[2][1] * w2,
            ];
            let mask = sample(&mesh.mask, uv);
            if let Some(tags) = tags.as_deref_mut() {
                tags[idx] = if mask[1] > 0.0 { 2 } else if mask[0] > 0.0 { 1 } else { 0 };
            }
            let rgb = if silhouette {
                [0x29, 0x29, 0x29]
            } else {
                let n = [
                    normals[0][0] * w0 + normals[1][0] * w1 + normals[2][0] * w2,
                    normals[0][1] * w0 + normals[1][1] * w1 + normals[2][1] * w2,
                    normals[0][2] * w0 + normals[1][2] * w1 + normals[2][2] * w2,
                ];
                let wp = [
                    world[0][0] * w0 + world[1][0] * w1 + world[2][0] * w2,
                    world[0][1] * w0 + world[1][1] * w1 + world[2][1] * w2,
                    world[0][2] * w0 + world[1][2] * w1 + world[2][2] * w2,
                ];
                let view = norm(sub(cam.eye, wp));
                let nd = norm(n);
                let facing = (nd[0] * view[0] + nd[1] * view[1] + nd[2] * view[2]).clamp(0.0, 1.0);
                let fresnel = (1.0 - facing).powf(2.0);
                let texel = if job.embedded_base {
                    let px = sample4(&mesh.base, uv);
                    [px[0], px[1], px[2]]
                } else if job.channeled {
                    // The shade map is the lighting. Emissive stays in the client texture.
                    let shade_srgb = sample(&mesh.shade, uv)[0];
                    let weights = sample4(&mesh.mask, uv);
                    let albedo = tint_channels(shade_srgb, weights, job.channels, job.emissive, job.shade_mid_lin, false).0;
                    if let Some(channel) = job.overdrive_channel {
                        brighten_channel(albedo, weights, channel, job.overdrive_toward, job.overdrive_gain)
                    } else {
                        albedo
                    }
                } else {
                    // The glb base is the shade map. Lambert on top of it double-darkens.
                    let shade_linear = sample(&mesh.base, uv)[0];
                    tint_texel(
                        shade_linear,
                        sample(&mesh.mask, uv),
                        job.base_c,
                        job.accent,
                        job.glow,
                        job.rim,
                        fresnel,
                        job.shade_mid_lin,
                        job.rim_strength,
                        job.rim_fill,
                    )
                };
                [
                    (texel[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (texel[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (texel[2].clamp(0.0, 1.0) * 255.0).round() as u8,
                ]
            };
            let o = idx * 3;
            color[o] = rgb[0];
            color[o + 1] = rgb[1];
            color[o + 2] = rgb[2];
        }
    }
}

fn project(p: [f32; 3], cam: &PaintCam) -> Option<[f32; 3]> {
    let d = sub(p, cam.eye);
    let z = d[0] * cam.forward[0] + d[1] * cam.forward[1] + d[2] * cam.forward[2];
    if z < 0.05 {
        return None;
    }
    let x = (d[0] * cam.right[0] + d[1] * cam.right[1] + d[2] * cam.right[2]) / z * cam.focal / cam.aspect;
    let y = (d[0] * cam.up[0] + d[1] * cam.up[1] + d[2] * cam.up[2]) / z * cam.focal;
    Some([(x * 0.5 + 0.5) * cam.w as f32, (1.0 - (y * 0.5 + 0.5)) * cam.h as f32, z])
}

fn edge(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])
}

/// Replace the named channel with `toward * gain`, the same product the client
/// puts on the emissive color. The still has no bloom, so the product clamps.
fn brighten_channel(albedo: [f32; 3], weights: [f32; 4], channel: usize, toward: [f32; 3], gain: [f32; 3]) -> [f32; 3] {
    let w = weights[channel].clamp(0.0, 1.0);
    if w <= 0.0 {
        return albedo;
    }
    let hot = [
        (toward[0] * gain[0]).min(1.0),
        (toward[1] * gain[1]).min(1.0),
        (toward[2] * gain[2]).min(1.0),
    ];
    [
        albedo[0] + (hot[0] - albedo[0]) * w,
        albedo[1] + (hot[1] - albedo[1]) * w,
        albedo[2] + (hot[2] - albedo[2]) * w,
    ]
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

/// Hero card pieces: body, then that hero's gun, then Brakk's shield.
pub fn hero_pieces(hero_id: &str) -> Vec<&'static CastMesh> {
    let mut pieces = vec![cast(&format!("{hero_id}_body")), cast(&format!("{hero_id}_gun"))];
    if hero_id == "brakk" {
        pieces.push(cast("brakk_shield"));
    }
    pieces
}

pub fn hero_placement(content: &Content, mesh: &CastMesh, body_at: [f32; 3], pivot: [f32; 3], yaw_deg: f32, pitch_deg: f32) -> Placement {
    let spec = content
        .models
        .iter()
        .find(|model| model.id == mesh.id)
        .unwrap_or_else(|| panic!("{} has no cast record", mesh.id));
    match spec.fit.as_str() {
        "weapon" | "hero" | "shield" => {}
        other => panic!("{} fit {other} is not a hero piece", mesh.id),
    }
    if spec.fit != "hero" && spec.at == [0.0, 0.0, 0.0] {
        panic!("{} at is zero", mesh.id);
    }
    Placement {
        pre_yaw: if spec.fit == "weapon" { spec.yaw_deg } else { 0.0 },
        pre_shift: if spec.fit == "hero" { [0.0, 0.0, 0.0] } else { spec.at },
        yaw_deg,
        lean: pitch_deg.to_radians(),
        pivot,
        shift: body_at,
    }
}

/// Same turn `paint` applies. Portraits use it to pick a FOV.
pub fn place_point(p: [f32; 3], place: &Placement) -> [f32; 3] {
    let (ps, pc) = place.pre_yaw.to_radians().sin_cos();
    let (ys, yc) = place.yaw_deg.to_radians().sin_cos();
    let (ls, lc) = place.lean.sin_cos();
    let p = spin_y(p, ps, pc);
    let p = [p[0] + place.pre_shift[0], p[1] + place.pre_shift[1], p[2] + place.pre_shift[2]];
    let p = [p[0] - place.pivot[0], p[1] - place.pivot[1], p[2] - place.pivot[2]];
    let p = spin_y(p, ys, yc);
    let y2 = lc * p[1] - ls * p[2];
    let z2 = ls * p[1] + lc * p[2];
    [
        p[0] + place.pivot[0] + place.shift[0],
        y2 + place.pivot[1] + place.shift[1],
        z2 + place.pivot[2] + place.shift[2],
    ]
}

fn place_normal(n: [f32; 3], place: &Placement) -> [f32; 3] {
    let (ps, pc) = place.pre_yaw.to_radians().sin_cos();
    let (ys, yc) = place.yaw_deg.to_radians().sin_cos();
    let (ls, lc) = place.lean.sin_cos();
    let n = spin_y(n, ps, pc);
    let n = spin_y(n, ys, yc);
    let y2 = lc * n[1] - ls * n[2];
    let z2 = ls * n[1] + lc * n[2];
    [n[0], y2, z2]
}

fn spin_y(p: [f32; 3], s: f32, c: f32) -> [f32; 3] {
    [c * p[0] + s * p[2], p[1], -s * p[0] + c * p[2]]
}

/// Vertex nearest the muzzle. Fitted guns point down -Z, grip at the origin.
pub fn muzzle_tip(mesh: &CastMesh) -> [f32; 3] {
    mesh.positions
        .iter()
        .copied()
        .min_by(|a, b| a[2].partial_cmp(&b[2]).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or_else(|| panic!("{} has no vertices", mesh.id))
}

pub fn body_pivot(content: &Content, hero_id: &str) -> [f32; 3] {
    let spec = content
        .models
        .iter()
        .find(|model| model.id == format!("{hero_id}_body"))
        .expect("hero body");
    [0.0, spec.size * 0.5, 0.0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gemini_sim::bugs::PartBounds;

    /// 5 cm. The fit test's bar for a hit center off the cast.
    const HIT_SURFACE_GAP_M: f32 = 0.05;

    #[test]
    fn budgets_grips_and_hitboxes_hold() {
        let loaded = load_catalog().expect("casts");
        assert_eq!(loaded.len(), 14, "every approved cast");
        let content = Content::embedded();
        for mesh in &loaded {
            let spec = content.models.iter().find(|model| model.id == mesh.id).unwrap();
            assert!(mesh.file_bytes <= spec.bytes as usize, "{}", mesh.id);
            assert!(mesh.tris <= spec.triangles as usize, "{} tris {}", mesh.id, mesh.tris);
            assert!(mesh.file_bytes > 0 && mesh.tris > 0, "{}", mesh.id);
        }
        let err = check_budget("swarmer", 9000, 10, 7200, 1100000).expect_err("over triangles");
        assert!(err.contains("triangles"), "{err}");
        let err = check_budget("swarmer", 10, 2_000_000, 7200, 1100000).expect_err("over bytes");
        assert!(err.contains("bytes"), "{err}");
        let err = model_bytes("laser_knife").expect_err("unknown");
        assert!(err.contains("unknown"), "{err}");
        let err = read_model_file(Path::new("assets/models/not_a_model.glb")).expect_err("missing");
        assert!(err.contains("missing"), "{err}");
        let on_disk = read_model_file(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/vex_body.glb")).expect("vex on disk");
        assert_eq!(on_disk.len(), model_bytes("vex_body").unwrap().len());
    }

    #[test]
    fn loaded_mask_bytes_match_the_committed_png() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/masks");
        for id in [
            "vex_body",
            "vex_gun",
            "nyra_body",
            "nyra_gun",
            "brakk_body",
            "brakk_gun",
            "brakk_shield",
            "kip_body",
            "kip_gun",
            "swarmer",
            "spitter",
            "brute",
            "brood_mother",
            "hive_tyrant",
        ] {
            let channeled = !matches!(id, "swarmer" | "spitter" | "brute" | "brood_mother" | "hive_tyrant");
            let mask_name = if channeled { format!("{id}_mask_rgba.png") } else { format!("{id}_mask.png") };
            let loaded = mask_png(id).expect(id);
            let disk = std::fs::read(root.join(&mask_name)).unwrap_or_else(|err| panic!("{id} mask on disk: {err}"));
            assert_eq!(loaded, disk.as_slice(), "{id} bytes");
            let decoded = decode_mask(&disk, id).expect(id);
            let mesh = cast(id);
            assert_eq!(mesh.mask.rgba, decoded.rgba, "{id} renderer uses the committed mask");
            if channeled {
                let shade_bytes = shade_png(id).expect(id);
                let shade_disk = std::fs::read(root.join(format!("{id}_shade.png"))).unwrap_or_else(|err| panic!("{id} shade on disk: {err}"));
                assert_eq!(shade_bytes, shade_disk.as_slice(), "{id} shade bytes");
                assert_eq!(mesh.shade.rgba, decode_mask(&shade_disk, id).expect(id).rgba, "{id} shade");
                ensure_mask_weight(&mesh.mask, id).unwrap_or_else(|err| panic!("{err}"));
            }
        }
    }

    #[test]
    fn a_channel_mask_fails_when_any_texel_is_under_full_weight() {
        let covered = Tex { w: 1, h: 1, rgba: vec![252, 0, 0, 0] };
        ensure_mask_weight(&covered, "full").expect("252 passes");
        let mut rgba = vec![255u8, 0, 0, 0, 251, 0, 0, 0];
        let hole = Tex { w: 1, h: 2, rgba: rgba.clone() };
        let err = ensure_mask_weight(&hole, "synthetic").expect_err("251 fails");
        assert!(err.contains("(0,1)"), "{err}");
        assert!(err.contains("251"), "{err}");
        rgba[4] = 84;
        rgba[5] = 84;
        rgba[6] = 83;
        let spread = Tex { w: 2, h: 1, rgba };
        let err = ensure_mask_weight(&spread, "synthetic").expect_err("a late hole fails");
        assert!(err.contains("(1,0)"), "{err}");
        let mut img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 0]));
        img.put_pixel(1, 1, image::Rgba([250, 0, 0, 0]));
        let mut bytes = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        let decoded = decode_mask(&bytes, "synthetic").unwrap();
        let err = ensure_mask_weight(&decoded, "synthetic").expect_err("decoded hole fails");
        assert!(err.contains("(1,1)"), "{err}");
    }

    #[test]
    fn shade_times_a_channel_matches_the_palette_and_is_not_black() {
        let content = Content::embedded();
        let mid = content.cast.shade_mid_lin;
        let primary = content.palette.key("hero_vex_primary");
        let shade = 0.55_f32;
        let (color, emit) = tint_channels(
            shade,
            [1.0, 0.0, 0.0, 0.0],
            [[primary[0], primary[1], primary[2]], [0.0; 3], [0.0; 3], [0.0; 3]],
            [false, false, false, false],
            mid,
            false,
        );
        for c in 0..3 {
            assert!((color[c] - primary[c]).abs() < 0.02, "channel {c} {color:?} vs {primary:?}");
        }
        assert!(color[0] > 0.05 && color[1] > 0.2, "mid shade is not black {color:?}");
        assert_eq!(emit, [0.0, 0.0, 0.0], "a body channel is not emissive");
        let glow = content.palette.key("gear_nyra_gun_glow");
        let (_, emit) = tint_channels(
            shade,
            [0.0, 0.0, 0.0, 1.0],
            [[0.0; 3], [0.0; 3], [0.0; 3], [glow[0], glow[1], glow[2]]],
            [false, false, false, true],
            mid,
            false,
        );
        assert!(emit[1] > 0.5, "nyra coil emits {emit:?}");
        let body = content.palette.key("hero_vex_glow");
        let (_, quiet) = tint_channels(
            shade,
            [0.0, 0.0, 0.0, 1.0],
            [[0.0; 3], [0.0; 3], [0.0; 3], [body[0], body[1], body[2]]],
            [false, false, false, content.palette.emissive("hero_vex_glow")],
            mid,
            false,
        );
        assert_eq!(quiet, [0.0, 0.0, 0.0], "hero_vex_glow is a fourth color");
        const UI: &[&str] = &["hero_vex", "hero_nyra", "hero_brakk", "hero_kip"];
        for model in &content.models {
            if model.fit == "hitbox" {
                continue;
            }
            for key in [&model.primary, &model.secondary, &model.accent, &model.fourth, &model.base, &model.glow] {
                assert!(!UI.contains(&key.as_str()), "{} tints with UI key {key}", model.id);
            }
        }
    }

    #[test]
    fn shield_handle_and_gun_grip_survive_the_fit() {
        let content = Content::embedded();
        let shield = content.models.iter().find(|model| model.id == "brakk_shield").unwrap();
        assert!((shield.grip[0]).abs() < 1e-6 && shield.grip[1].abs() < 1e-6);
        assert!((shield.grip[2] + 0.058).abs() < 1e-4, "handle {}", shield.grip[2]);
        let mesh = cast("brakk_shield");
        let near = mesh.positions.iter().any(|p| dist(*p, [0.0, 0.0, 0.0]) < 0.08);
        assert!(near, "fitted shield keeps the handle at the origin");
        for id in ["vex_gun", "nyra_gun", "brakk_gun", "kip_gun"] {
            let gun = cast(id);
            let (mn, mx) = bounds(&gun.positions);
            let gripped = gun.positions.iter().any(|p| dist(*p, [0.0, 0.0, 0.0]) < 0.04);
            assert!(gripped, "{id} grip is at the origin");
            assert!(mx[2] > 0.05, "{id} stock extends behind the grip, max z {}", mx[2]);
            assert!(mn[2] < -0.2, "{id} muzzle points down -z, min z {}", mn[2]);
            assert!(mn[2].abs() > mx[2], "{id} muzzle is the long end");
            let spec = content.models.iter().find(|model| model.id == id).unwrap();
            assert!(spec.grip_root, "{id} is declared grip-rooted");
        }
    }

    #[test]
    fn a_missing_or_undeclared_grip_root_fails_the_load() {
        let gun = model_bytes("vex_gun").unwrap();
        let err = must_err(decode_glb(gun, &model_spec("swarmer")), "bug bytes are not a gun");
        assert!(err.contains("swarmer") || err.contains("grip") || err.contains("primitive"), "{err}");
        let spec = model_spec("vex_gun");
        let mut bare = spec.clone();
        bare.grip_root = false;
        let err = must_err(decode_glb(gun, &bare), "undeclared grip");
        assert!(err.contains("grip_root"), "{err}");
        let mut body = model_spec("vex_body");
        body.grip_root = true;
        let err = must_err(decode_glb(model_bytes("vex_body").unwrap(), &body), "body has no grip");
        assert!(err.contains("missing a grip root"), "{err}");
    }

    #[test]
    fn a_wrong_mesh_sha256_fails_the_load() {
        let bytes = model_bytes("vex_gun").unwrap();
        let spec = model_spec("vex_gun");
        assert_eq!(sha256_hex(bytes), spec.sha256);
        let mut bad = spec.clone();
        bad.sha256 = "0".repeat(64);
        let err = match checked_model_bytes(&bad) {
            Err(err) => err,
            Ok(_) => panic!("bad sha"),
        };
        assert!(err.contains("vex_gun") && err.contains("sha256"), "{err}");
        // The browser hashes the embedded bytes only. The disk match stays here.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let disk = read_model_file(&assets_file(&spec.file)).expect("shipped file");
            assert_eq!(disk, bytes, "disk bytes differ from the embedded glb");
            let missing = read_model_file(&assets_file("models/not_a_model.glb")).expect_err("missing file");
            assert!(missing.contains("missing"), "{missing}");
        }
    }

    #[test]
    fn weapon_yaw_comes_from_content() {
        let spec = model_spec("vex_gun");
        assert!((spec.yaw_deg - 180.0).abs() < 1.0e-3, "vex_gun yaw_deg {}", spec.yaw_deg);
        let bytes = model_bytes("vex_gun").unwrap();
        let mut turned = decode_glb(bytes, &spec).unwrap_or_else(|err| panic!("{err}"));
        fit_weapon(&mut turned, &spec).unwrap_or_else(|err| panic!("{err}"));
        let (mn, _) = bounds(&turned.positions);
        assert!(mn[2] < -0.2, "content yaw points the muzzle down -z, min {}", mn[2]);
        let mut flat = spec;
        flat.yaw_deg = 0.0;
        let mut straight = decode_glb(bytes, &flat).unwrap_or_else(|err| panic!("{err}"));
        fit_weapon(&mut straight, &flat).unwrap_or_else(|err| panic!("{err}"));
        let (_, mx) = bounds(&straight.positions);
        assert!(mx[2] > 0.2, "yaw 0 leaves the rig muzzle on +z, max {}", mx[2]);
    }

    /// TEXCOORD_0 and POSITION bytes from the final_rig body, before skin and clips were removed.
    #[test]
    fn static_body_uvs_match_the_final_rig_source() {
        let expect = [
            (
                "vex_body",
                "b178d21f181e12217df3a8890bc40eca5e8c0327ee391ef600e2337db1ce70dd",
                "d3d498b5afb41ca2581a5958d8d1d2cde163d0691151185c07c7ed324e136703",
            ),
            (
                "nyra_body",
                "51be9f77045125c8f4d875e440267870dd44c10a6f32eb838696e927b339bb47",
                "fd20c3811ecb37e2fa5b5b2d572bbfeae01506d26f8b68db9e7ab123bc542237",
            ),
            (
                "brakk_body",
                "12c2b676936831a8b852c557414f0519c6be95554dd24c257566b9685442ce77",
                "df594989f83292dbbb20a9c762432e5c611feb262a1bb2752c67090d8dd6422d",
            ),
            (
                "kip_body",
                "205e16ecb4d826d10d4f6d8ef03587eeb346f056adfaa91770ee01a52f53d256",
                "64de8d43e34d0e453cf2f41623e3d4b5f3f6b689310caac8fff9c6415b2618f8",
            ),
        ];
        for (id, uv_hash, pos_hash) in expect {
            let spec = model_spec(id);
            assert!(!spec.grip_root, "{id} is static");
            assert_eq!(spec.bytes, 1_100_000, "{id} budget");
            let bytes = model_bytes(id).unwrap();
            assert!(bytes.len() <= spec.bytes as usize, "{id} file {}", bytes.len());
            let (doc, bin) = glb_parts(bytes, id);
            assert!(doc.get("skins").is_none(), "{id} still has a skin");
            assert!(doc.get("animations").is_none(), "{id} still has clips");
            let prim = &doc["meshes"][0]["primitives"][0];
            let attrs = &prim["attributes"];
            assert!(attrs.get("JOINTS_0").is_none() && attrs.get("WEIGHTS_0").is_none(), "{id}");
            let uv = attr_bytes(&doc, bin, attrs["TEXCOORD_0"].as_u64().unwrap() as usize);
            let pos = attr_bytes(&doc, bin, attrs["POSITION"].as_u64().unwrap() as usize);
            assert_eq!(sha256_hex(&uv), uv_hash, "{id} uvs moved off the final_rig source");
            assert_eq!(sha256_hex(&pos), pos_hash, "{id} positions moved off the final_rig source");
        }
    }

    fn model_spec(id: &str) -> CastModel {
        Content::embedded()
            .models
            .into_iter()
            .find(|model| model.id == id)
            .unwrap_or_else(|| panic!("{id}"))
    }

    fn must_err(result: Result<RawMesh, String>, what: &str) -> String {
        match result {
            Err(err) => err,
            Ok(_) => panic!("{what}"),
        }
    }

    fn glb_parts<'a>(bytes: &'a [u8], id: &str) -> (serde_json::Value, &'a [u8]) {
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json_end = 20 + json_len;
        let doc: serde_json::Value = serde_json::from_slice(&bytes[20..json_end]).unwrap_or_else(|err| panic!("{id} {err}"));
        let bin_len = u32::from_le_bytes(bytes[json_end..json_end + 4].try_into().unwrap()) as usize;
        let bin_start = json_end + 8;
        (doc, &bytes[bin_start..bin_start + bin_len])
    }

    fn attr_bytes(doc: &serde_json::Value, bin: &[u8], accessor: usize) -> Vec<u8> {
        let view = doc["accessors"][accessor]["bufferView"].as_u64().unwrap() as usize;
        let v = &doc["bufferViews"][view];
        let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        let len = v["byteLength"].as_u64().unwrap() as usize;
        bin[off..off + len].to_vec()
    }

    #[test]
    fn cast_mesh_bounds_fit_inside_the_existing_hitbox() {
        let content = Content::embedded();
        let mut table = vec![
            "id | mesh min | mesh max | hitbox | inside | weak glow".to_string(),
        ];
        let mut gaps: Vec<(String, String, [f32; 3], f32)> = Vec::new();
        for (id, row) in content.bugs.rows() {
            let mesh = cast(id);
            let hit = hit_bounds(&row.hitbox.body);
            let (mn, mx) = bounds(&mesh.positions);
            let mesh_ext = sub(mx, mn);
            let hit_ext = sub(hit.max, hit.min);
            require_hitbox_fill(id, mesh_ext, hit_ext, content.cast.hitbox_fill)
                .unwrap_or_else(|err| panic!("{err}"));
            let authored = content.models.iter().find(|model| model.id == id).unwrap();
            assert_eq!(authored.fit, "hitbox", "{id}");
            let visual = crate::bug(id);
            let posed = rest_pose(visual);
            let mut glow = Vec::new();
            for (slot, shape) in row.hitbox.anchored() {
                let gap = if shape.anchor == gemini_sim::bugs::PROCEDURAL_TAIL_ANCHOR {
                    let seated = gemini_sim::bugs::visual_anchor(&row.visual, &shape.anchor).unwrap_or_else(|| {
                        panic!("{id} {slot} anchor {} did not resolve", shape.anchor)
                    });
                    let chain = dist(shape.offset, seated);
                    let mesh_gap = tail_surface_gap(visual, &posed, shape.offset);
                    let tail = row.visual.parts.iter().find(|part| part.name == "tail").expect("tail");
                    let gem = row
                        .visual
                        .parts
                        .iter()
                        .find(|part| part.at.as_deref() == Some("tail_end"))
                        .expect("tail end");
                    let place_r = gem
                        .size
                        .first()
                        .copied()
                        .expect("tail end is missing a size; refusing to fall back to a zero radius")
                        * 0.5;
                    let seat = place_r + tail.tip_gap;
                    if mesh_gap > seat + HIT_SURFACE_GAP_M {
                        gaps.push((
                            id.to_string(),
                            slot.to_string(),
                            shape.offset,
                            mesh_gap,
                        ));
                    }
                    chain
                } else {
                    surface_gap(&mesh.positions, &mesh.indices, shape.offset)
                };
                gaps.push((id.to_string(), slot.to_string(), shape.offset, gap));
                glow.push(glow_sits_on_hit(id, visual, &posed, slot, shape));
            }
            if let Some(radius) = row.hitbox.pod_radius() {
                for (index, place) in row.hitbox.pods.iter().enumerate() {
                    let name = format!("pod{index}");
                    let part = visual
                        .parts
                        .iter()
                        .find(|part| part.name == name)
                        .unwrap_or_else(|| panic!("{id} missing {name}"));
                    let center = part.rest_pos;
                    let gap = dist(center, place.offset);
                    assert!(gap < 1.0e-3, "{id} {name} center is {gap} off the pod hit");
                    let mesh_r = local_radius(&part.mesh.positions);
                    let authored = content_visual_radius(id, "pod");
                    assert!(authored > 0.0, "{id} pod glow is missing visual_radius");
                    assert!(
                        (mesh_r - authored).abs() < 1.0e-2,
                        "{id} {name} mesh radius {mesh_r:.3} is not the content visual radius {authored:.3}"
                    );
                    assert!(
                        authored + 1.0e-3 >= 0.70 * radius,
                        "{id} {name} visual radius {authored:.3} is under 70% of the hit radius {radius:.3}"
                    );
                    assert!(
                        mesh_r <= radius + 1.0e-3,
                        "{id} {name} glow radius {mesh_r:.3} is outside the pod hit {radius:.3}"
                    );
                    gaps.push((
                        id.to_string(),
                        name.clone(),
                        place.offset,
                        surface_gap(&mesh.positions, &mesh.indices, place.offset),
                    ));
                    glow.push(format!(
                        "{name} center ({:.2},{:.2},{:.2}) r {:.2} inside hit r {radius:.2}",
                        center[0], center[1], center[2], mesh_r
                    ));
                }
            }
            let glow = if glow.is_empty() { "none".into() } else { glow.join("; ") };
            table.push(format!(
                "{id} | ({:.2},{:.2},{:.2}) | ({:.2},{:.2},{:.2}) | {} | yes | {glow}",
                mn[0], mn[1], mn[2], mx[0], mx[1], mx[2], shape_label(&row.hitbox.body)
            ));
        }
        let mut missed = Vec::new();
        for (id, slot, center, gap) in &gaps {
            eprintln!("{id} {slot} center {center:?} surface gap {gap:.3}");
            if *gap > HIT_SURFACE_GAP_M {
                missed.push(format!(
                    "{id} {slot} center {center:?} is {gap:.3} off the cast surface"
                ));
            }
        }
        assert!(missed.is_empty(), "hit centers off the cast:\n{}", missed.join("\n"));
        let player_r = content.combat.hit_radius.player;
        let eye = content.controller.camera.eye_height;
        let sphere_y = eye * 0.5;
        for hero in ["vex", "nyra", "brakk", "kip"] {
            let id = format!("{hero}_body");
            let mesh = cast(&id);
            let spec = content.models.iter().find(|model| model.id == id).unwrap();
            assert_eq!(spec.fit, "hero", "{id}");
            let (mn, mx) = bounds(&mesh.positions);
            let horiz = mesh
                .positions
                .iter()
                .map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt())
                .fold(0.0_f32, f32::max);
            assert!(mn[1] >= -1.0e-3 && (mx[1] - spec.size).abs() < 1.0e-2, "{id} height");
            assert!(
                (mn[0] + mx[0]).abs() < 1.0e-2 && (mn[2] + mx[2]).abs() < 1.0e-2,
                "{id} is not centered on the player"
            );
            // Shots use a sphere of `hit_radius.player` around half eye height.
            // Movement uses that same radius as a wall pad. Neither volume is a
            // body box, and the cast stays at the authored height.
            let outside_sphere = mesh.positions.iter().any(|p| {
                let dx = p[0];
                let dy = p[1] - sphere_y;
                let dz = p[2];
                (dx * dx + dy * dy + dz * dz).sqrt() > player_r + 1.0e-3
            });
            let sphere = if horiz <= player_r + 1.0e-3 && !outside_sphere {
                "yes"
            } else {
                "no"
            };
            table.push(format!(
                "{id} | ({:.2},{:.2},{:.2}) | ({:.2},{:.2},{:.2}) | authored height {:.2} feet 0; player sphere r {player_r:.2} at y {sphere_y:.2}, mesh r {horiz:.2} | height yes, sphere {sphere} | none",
                mn[0], mn[1], mn[2], mx[0], mx[1], mx[2], spec.size
            ));
        }
        eprintln!("{}", table.join("\n"));
    }

    fn rest_pose(visual: &crate::EnemyVisual) -> crate::pose::Pose {
        crate::pose::pose(
            visual,
            crate::pose::AnimInput {
                time: 0.0,
                moving: false,
                charging: false,
                vulnerable: true,
                phase: 0,
                frenzy: false,
                preview_glow: true,
                regrow: 0,
                hatch_pod: 255,
                hatch_charge: 0.0,
            },
            &[],
        )
    }

    fn glow_sits_on_hit(
        id: &str,
        visual: &crate::EnemyVisual,
        posed: &crate::pose::Pose,
        slot: &str,
        shape: &gemini_sim::bugs::ShapeDef,
    ) -> String {
        let name = if shape.name.is_empty() {
            shape.anchor.as_str()
        } else {
            shape.name.as_str()
        };
        let index = visual
            .parts
            .iter()
            .position(|part| part.name == name)
            .unwrap_or_else(|| panic!("{id} {slot} has no part {name}"));
        let part = &visual.parts[index];
        assert!(part.emissive, "{id} {name} is the weak glow and it is not emissive");
        let xf = &posed.xforms[index];
        let hit = hit_bounds(shape);
        let mut glow_min = [f32::MAX; 3];
        let mut glow_max = [f32::MIN; 3];
        let mut centroid = [0.0f32; 3];
        let count = part.mesh.positions.len().max(1) as f32;
        for p in &part.mesh.positions {
            let p = [p[0] * xf.scale[0], p[1] * xf.scale[1], p[2] * xf.scale[2]];
            let p = crate::pose::rotate_vec(xf.rot, p);
            let world = [p[0] + xf.pos[0], p[1] + xf.pos[1], p[2] + xf.pos[2]];
            centroid[0] += world[0] / count;
            centroid[1] += world[1] / count;
            centroid[2] += world[2] / count;
            for axis in 0..3 {
                glow_min[axis] = glow_min[axis].min(world[axis]);
                glow_max[axis] = glow_max[axis].max(world[axis]);
            }
        }
        let glow = PartBounds { min: glow_min, max: glow_max };
        assert!(hit.overlaps(glow), "{id} {slot} glow {glow:?} misses the hit {hit:?}");
        let authored = content_visual_radius(id, name);
        assert!(authored > 0.0, "{id} {name} glow is missing visual_radius");
        let fit_r = glow_fit_radius(shape);
        if shape.anchor == gemini_sim::bugs::PROCEDURAL_TAIL_ANCHOR {
            // The tail glow authors `tail_glow_radius`. That cap is smaller than
            // 70% of the cannon hit sphere, and the other glows still fill 70%.
            let cap = Content::embedded().cast.tail_glow_radius;
            assert!(
                (authored - cap).abs() < 1.0e-4,
                "{id} {slot} visual radius {authored:.3} is not tail_glow_radius {cap:.3}"
            );
        } else {
            assert!(
                authored + 1.0e-3 >= 0.70 * fit_r,
                "{id} {slot} visual radius {authored:.3} is under 70% of the hit radius {fit_r:.3}"
            );
        }
        assert!(
            authored <= fit_r + 1.0e-3,
            "{id} {slot} visual radius {authored:.3} is larger than the hit radius {fit_r:.3}"
        );
        for p in &part.mesh.positions {
            let p = [p[0] * xf.scale[0], p[1] * xf.scale[1], p[2] * xf.scale[2]];
            let p = crate::pose::rotate_vec(xf.rot, p);
            let world = [p[0] + xf.pos[0], p[1] + xf.pos[1], p[2] + xf.pos[2]];
            assert!(
                point_in_shape(shape, world),
                "{id} {slot} glow vertex {world:?} is outside the hit at {:?}",
                shape.offset
            );
        }
        assert!(
            point_in_shape(shape, centroid),
            "{id} {slot} glow center {centroid:?} is outside the hit at {:?} r {}",
            shape.offset,
            shape.bounding_radius()
        );
        format!(
            "{slot} {name} center ({:.2},{:.2},{:.2}) on hit ({:.2},{:.2},{:.2})",
            centroid[0], centroid[1], centroid[2], shape.offset[0], shape.offset[1], shape.offset[2]
        )
    }

    fn point_in_shape(shape: &gemini_sim::bugs::ShapeDef, p: [f32; 3]) -> bool {
        let d = [p[0] - shape.offset[0], p[1] - shape.offset[1], p[2] - shape.offset[2]];
        match shape.shape.as_str() {
            "box" => d.iter().zip(shape.half_extents).all(|(n, h)| n.abs() <= h + 1.0e-3),
            "capsule" => {
                let half = shape.height * 0.5;
                let y = d[1].clamp(-half, half);
                let dx = d[0];
                let dy = d[1] - y;
                let dz = d[2];
                (dx * dx + dy * dy + dz * dz).sqrt() <= shape.radius + 1.0e-3
            }
            _ => (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= shape.radius + 1.0e-3,
        }
    }

    /// Largest sphere centered on the hit that stays inside the zone.
    fn glow_fit_radius(shape: &gemini_sim::bugs::ShapeDef) -> f32 {
        match shape.shape.as_str() {
            "box" => shape.half_extents[0].min(shape.half_extents[1]).min(shape.half_extents[2]),
            _ => shape.radius,
        }
    }

    fn surface_gap(positions: &[[f32; 3]], indices: &[u32], point: [f32; 3]) -> f32 {
        closest_on_mesh(positions, indices, point).1
    }

    fn tail_surface_gap(visual: &crate::EnemyVisual, posed: &crate::pose::Pose, point: [f32; 3]) -> f32 {
        let mut best = f32::MAX;
        for (part, xf) in visual.parts.iter().zip(posed.xforms.iter()) {
            if !part.name.starts_with("tail") {
                continue;
            }
            let positions: Vec<[f32; 3]> = part
                .mesh
                .positions
                .iter()
                .map(|p| {
                    let p = [p[0] * xf.scale[0], p[1] * xf.scale[1], p[2] * xf.scale[2]];
                    let p = crate::pose::rotate_vec(xf.rot, p);
                    [p[0] + xf.pos[0], p[1] + xf.pos[1], p[2] + xf.pos[2]]
                })
                .collect();
            best = best.min(surface_gap(&positions, &part.mesh.indices, point));
        }
        best
    }

    /// Independent of `sample4`. `flip_v` is the bottom-left row this reader must not use.
    fn mask_row(tex: &Tex, uv: [f32; 2], flip_v: bool) -> [u8; 4] {
        let u = uv[0] - uv[0].floor();
        let mut v = uv[1] - uv[1].floor();
        if flip_v {
            v = 1.0 - v;
        }
        let x = ((u * (tex.w as f32 - 1.0)).round() as u32).min(tex.w - 1);
        let y = ((v * (tex.h as f32 - 1.0)).round() as u32).min(tex.h - 1);
        let i = ((y * tex.w + x) * 4) as usize;
        [tex.rgba[i], tex.rgba[i + 1], tex.rgba[i + 2], tex.rgba[i + 3]]
    }

    fn strict_channel(px: &[u8], floor: u8) -> Option<usize> {
        let mut best = 0u8;
        let mut index = 0usize;
        let mut ties = 0u8;
        for (i, channel) in px.iter().take(4).enumerate() {
            if *channel > best {
                best = *channel;
                index = i;
                ties = 1;
            } else if *channel == best {
                ties += 1;
            }
        }
        if best >= floor && ties == 1 { Some(index) } else { None }
    }

    fn content_visual_radius(id: &str, name: &str) -> f32 {
        let content = Content::embedded();
        let (_, row) = content.bugs.rows().into_iter().find(|(bug, _)| *bug == id).unwrap();
        row.visual
            .parts
            .iter()
            .find(|part| part.name == name)
            .map(|part| part.visual_radius)
            .unwrap_or(0.0)
    }

    fn local_radius(positions: &[[f32; 3]]) -> f32 {
        positions
            .iter()
            .map(|p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt())
            .fold(0.0_f32, f32::max)
    }

    fn shape_label(shape: &gemini_sim::bugs::ShapeDef) -> String {
        match shape.shape.as_str() {
            "capsule" => format!(
                "capsule r {:.2} h {:.2} at ({:.2},{:.2},{:.2})",
                shape.radius, shape.height, shape.offset[0], shape.offset[1], shape.offset[2]
            ),
            "box" => format!(
                "box half ({:.2},{:.2},{:.2}) at ({:.2},{:.2},{:.2})",
                shape.half_extents[0],
                shape.half_extents[1],
                shape.half_extents[2],
                shape.offset[0],
                shape.offset[1],
                shape.offset[2]
            ),
            _ => format!(
                "sphere r {:.2} at ({:.2},{:.2},{:.2})",
                shape.radius, shape.offset[0], shape.offset[1], shape.offset[2]
            ),
        }
    }

    #[test]
    fn tint_honors_the_content_mask_and_rim() {
        let look = &Content::embedded().cast;
        let text = include_str!("../../../content/cast.toml");
        let shade_line = text
            .lines()
            .find(|line| line.starts_with("shade_mid_lin"))
            .expect("shade_mid_lin");
        let shade_mid: f32 = shade_line.split('=').nth(1).unwrap().trim().parse().unwrap();
        assert!(!text.contains("base_light"), "base_light is not a cast key");
        assert!((look.shade_mid_lin - shade_mid).abs() < 1.0e-6);
        assert!(!text.contains("base_gray"), "base_gray is not a cast key");
        assert!((look.rim_strength - 1.0).abs() < 1.0e-6);
        assert!((look.rim_fill - 0.30).abs() < 1.0e-6, "rim_fill comes from cast.toml");
        assert_eq!(look.overdrive_emissive_channel, "accent");
        assert!(!text.contains("view_gun_"), "the view-gun grade is not a cast key");
        assert!((look.glow_falloff - 2.0).abs() < 1.0e-6);
        assert!((look.glow_show_through - 0.45).abs() < 1.0e-6);
        assert!(
            !text.lines().any(|line| line.trim_start().starts_with("mask_threshold")),
            "mask_threshold is not a cast key"
        );
        let accent = [1.0, 0.0, 0.0, 1.0];
        let glow = [0.0, 1.0, 0.0, 1.0];
        let rim = [1.0, 1.0, 1.0, 1.0];
        let base_c = [0.2, 0.4, 0.6, 1.0];
        let fill = look.rim_fill;
        let mid = look.shade_mid_lin;
        let painted = tint_texel(mid, [1.0, 0.0, 0.0], base_c, accent, glow, rim, 0.0, mid, look.rim_strength, fill);
        assert_eq!(painted, [1.0, 0.0, 0.0]);
        let acid = tint_texel(mid, [0.0, 1.0, 0.0], base_c, accent, glow, rim, 0.0, mid, look.rim_strength, fill);
        assert_eq!(acid, [0.0, 1.0, 0.0]);
        let bare = [0.0, 0.0, 0.0, 0.0];
        let body = tint_texel(mid, [0.0, 0.0, 0.0], base_c, accent, glow, bare, 0.0, mid, look.rim_strength, fill);
        assert!((body[0] - base_c[0]).abs() < 1.0e-3, "mid shade keeps the palette {body:?}");
        // A bug base of ~0.24 is already linear. Treating it as sRGB crushes the body.
        let (kept, _) = tint_channels(0.24, [1.0, 0.0, 0.0, 0.0], [[base_c[0], base_c[1], base_c[2]], [0.0; 3], [0.0; 3], [0.0; 3]], [false; 4], mid, true);
        let (crushed, _) = tint_channels(0.24, [1.0, 0.0, 0.0, 0.0], [[base_c[0], base_c[1], base_c[2]], [0.0; 3], [0.0; 3], [0.0; 3]], [false; 4], mid, false);
        assert!(kept[0] > crushed[0] * 2.0, "gamma-decoding a linear bug shade crushes it {kept:?} vs {crushed:?}");
        assert!(kept[0] > 0.15, "a linear mid shade stays tan {kept:?}");
        let facing = tint_texel(mid, [0.0, 0.0, 0.0], base_c, accent, glow, rim, 0.0, mid, look.rim_strength, fill);
        assert!(
            (facing[0] - (body[0] + look.rim_strength * fill)).abs() < 1.0e-4,
            "facing rim is rim_fill {facing:?}"
        );
        let edged = tint_texel(mid, [0.0, 0.0, 0.0], base_c, accent, glow, rim, 1.0, mid, look.rim_strength, fill);
        assert!((edged[0] - (body[0] + look.rim_strength)).abs() < 1.0e-4, "rim {edged:?}");
    }

    #[test]
    fn every_cast_mask_is_read_from_the_top_left() {
        let content = Content::embedded();
        assert!(content.models.len() > 4, "the uv test walks every content mask");
        for spec in &content.models {
            assert_eq!(spec.file, format!("models/{}.glb", spec.id));
            let mesh = cast(&spec.id);
            let tex = &mesh.mask;
            assert!(tex.w > 1 && tex.h > 1, "{} mask", spec.id);
            let mut flipped_differs = false;
            let mut hit = [false; 4];
            for uv in &mesh.uvs {
                let got = sample4(tex, *uv);
                let top = mask_row(tex, *uv, false);
                let bytes = [
                    (got[0] * 255.0).round() as u8,
                    (got[1] * 255.0).round() as u8,
                    (got[2] * 255.0).round() as u8,
                    (got[3] * 255.0).round() as u8,
                ];
                assert_eq!(bytes, top, "{} sample4 is not the top-left row", spec.id);
                if top != mask_row(tex, *uv, true) {
                    flipped_differs = true;
                }
                if let Some(channel) = strict_channel(&bytes, 200) {
                    hit[channel] = true;
                }
            }
            let mut image_flips = false;
            let row = (tex.w * 4) as usize;
            for y in 0..tex.h / 2 {
                let top = (y * tex.w * 4) as usize;
                let bot = ((tex.h - 1 - y) * tex.w * 4) as usize;
                if tex.rgba[top..top + row] != tex.rgba[bot..bot + row] {
                    image_flips = true;
                    break;
                }
            }
            if image_flips {
                assert!(
                    flipped_differs,
                    "{} mask changes under a v flip and no mesh uv shows it",
                    spec.id
                );
            }
            let mut strong = [false; 4];
            for px in tex.rgba.chunks_exact(4) {
                if let Some(channel) = strict_channel(px, 200) {
                    strong[channel] = true;
                }
            }
            for channel in 0..4 {
                assert!(
                    !strong[channel] || hit[channel],
                    "{} channel {channel} is in the mask and no mesh uv reads it",
                    spec.id
                );
            }
        }
    }

    /// Stock and rail UVs live on the weapon rows. The sample is an interior texel
    /// of the painted face, so the face that covers it is the panel. The stock face
    /// is the rear quarter. The rail face is the top of the barrel. The shield has neither.
    #[test]
    fn gun_rail_and_stock_sample_the_painted_panels() {
        let content = Content::embedded();
        let guns: Vec<_> = content.models.iter().filter(|model| model.fit == "weapon").collect();
        assert_eq!(guns.len(), 4, "every gun has a stock and a rail");
        for spec in guns {
            let id = spec.id.as_str();
            let stock_uv = spec.stock_uv.expect("weapon stock uv");
            let rail_uv = spec.rail_uv.expect("weapon rail uv");
            let stock_channel = gemini_sim::content::channel_index(&spec.stock_channel)
                .unwrap_or_else(|| panic!("{id} stock channel"));
            let rail_channel = gemini_sim::content::channel_index(&spec.rail_channel)
                .unwrap_or_else(|| panic!("{id} rail channel"));
            let mesh = cast(id);
            let (mn, mx) = bounds(&mesh.positions);
            let len = mx[2] - mn[2];
            let height = mx[1] - mn[1];
            assert!(len > 0.2 && height > 0.05, "{id} has no barrel");
            assert_channel(&mesh.mask, stock_uv, stock_channel, id, "stock");
            let (stock_point, stock_hits) = painted_face(mesh, stock_uv)
                .unwrap_or_else(|| panic!("{id} stock uv is not on a face"));
            let rear = stock_hits.iter().filter(|p| (p[2] - mn[2]) / len >= 0.75).count();
            let share = rear * 100 / stock_hits.len();
            assert!(
                share >= 55,
                "{id} stock face sits on {share}% of {} vertices, need the rear quarter",
                stock_hits.len()
            );
            assert!(
                (stock_point[2] - mn[2]) / len >= 0.75,
                "{id} stock texel is not in the rear quarter"
            );
            assert_channel(&mesh.mask, rail_uv, rail_channel, id, "rail");
            let (rail_point, rail_hits) = painted_face(mesh, rail_uv)
                .unwrap_or_else(|| panic!("{id} rail uv is not on a face"));
            let top = rail_hits.iter().filter(|p| (p[1] - mn[1]) / height >= 0.72).count();
            let share = top * 100 / rail_hits.len();
            assert!(
                share >= 55,
                "{id} rail face sits on {share}% of {} vertices, need the top of the barrel",
                rail_hits.len()
            );
            assert!(
                (rail_point[1] - mn[1]) / height >= 0.72,
                "{id} rail texel is not on top of the barrel"
            );
        }
        let shield = content.models.iter().find(|model| model.id == "brakk_shield").expect("shield");
        assert!(shield.stock_uv.is_none() && shield.rail_uv.is_none(), "the shield has no stock or rail");
    }

    fn assert_channel(mask: &Tex, uv: [f32; 2], channel: usize, id: &str, panel: &str) {
        let sample = sample4(mask, uv);
        let bytes = [
            (sample[0] * 255.0).round() as u8,
            (sample[1] * 255.0).round() as u8,
            (sample[2] * 255.0).round() as u8,
            (sample[3] * 255.0).round() as u8,
        ];
        assert_eq!(
            strict_channel(&bytes, 180),
            Some(channel),
            "{id} {panel} uv {uv:?} is not channel {channel} ({bytes:?})"
        );
    }

    /// The face that covers `uv`, its interpolated position, and that face's vertices.
    fn painted_face(mesh: &CastMesh, uv: [f32; 2]) -> Option<([f32; 3], Vec<[f32; 3]>)> {
        for tri in mesh.indices.chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            let ids = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let a = mesh.uvs[ids[0]];
            let b = mesh.uvs[ids[1]];
            let c = mesh.uvs[ids[2]];
            let v0 = [c[0] - a[0], c[1] - a[1]];
            let v1 = [b[0] - a[0], b[1] - a[1]];
            let v2 = [uv[0] - a[0], uv[1] - a[1]];
            let den = v0[0] * v1[1] - v1[0] * v0[1];
            if den.abs() < 1.0e-14 {
                continue;
            }
            let w = (v2[0] * v1[1] - v1[0] * v2[1]) / den;
            let v = (v0[0] * v2[1] - v2[0] * v0[1]) / den;
            let u = 1.0 - v - w;
            if u < -1.0e-4 || v < -1.0e-4 || w < -1.0e-4 {
                continue;
            }
            let u = u.max(0.0);
            let v = v.max(0.0);
            let w = w.max(0.0);
            let sum = u + v + w;
            let (u, v, w) = (u / sum, v / sum, w / sum);
            let pa = mesh.positions[ids[0]];
            let pb = mesh.positions[ids[1]];
            let pc = mesh.positions[ids[2]];
            let point = [
                pa[0] * u + pb[0] * v + pc[0] * w,
                pa[1] * u + pb[1] * v + pc[1] * w,
                pa[2] * u + pb[2] * v + pc[2] * w,
            ];
            return Some((point, vec![pa, pb, pc]));
        }
        None
    }

    #[test]
    fn stray_needles_are_dropped() {
        let look = &Content::embedded().cast;
        let text = include_str!("../../../content/cast.toml");
        let read = |key: &str| -> f32 {
            text.lines()
                .find(|line| line.starts_with(key))
                .unwrap_or_else(|| panic!("{key}"))
                .split('=')
                .nth(1)
                .unwrap()
                .trim()
                .parse()
                .unwrap()
        };
        assert!((look.needle_extent - read("needle_extent")).abs() < 1.0e-6);
        assert!((look.needle_area - read("needle_area")).abs() < 1.0e-6);
        let extent_frac = look.needle_extent;
        let area_ratio = look.needle_area;
        for mesh in casts() {
            let (mn, mx) = bounds(&mesh.positions);
            let extent = (mx[0] - mn[0]).max(mx[1] - mn[1]).max(mx[2] - mn[2]).max(1.0e-6);
            for tri in mesh.indices.chunks(3) {
                let a = mesh.positions[tri[0] as usize];
                let b = mesh.positions[tri[1] as usize];
                let c = mesh.positions[tri[2] as usize];
                let ab = sub(b, a);
                let ac = sub(c, a);
                let cross = [
                    ab[1] * ac[2] - ab[2] * ac[1],
                    ab[2] * ac[0] - ab[0] * ac[2],
                    ab[0] * ac[1] - ab[1] * ac[0],
                ];
                let area = 0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
                let longest = dist(a, b).max(dist(b, c)).max(dist(c, a));
                let ratio = area / (longest * longest).max(1.0e-12);
                assert!(
                    !(longest > extent_frac * extent && ratio < area_ratio),
                    "{} still has a needle",
                    mesh.id
                );
            }
        }
    }

    #[test]
    fn the_cannon_glow_stays_on_its_hit_center() {
        let content = Content::embedded();
        let (_, row) = content.bugs.rows().into_iter().find(|(id, _)| *id == "hive_tyrant").unwrap();
        let hit = row.hitbox.cannon.as_ref().expect("cannon").offset;
        let visual = crate::bug("hive_tyrant");
        let posed = rest_pose(visual);
        let index = visual.parts.iter().position(|part| part.name == "cannon").expect("cannon");
        let center = posed.xforms[index].pos;
        assert!(dist(center, hit) < 1.0e-3, "cannon glow {center:?} is not the hit center {hit:?}");
        let cap = content.cast.tail_glow_radius;
        let authored = content_visual_radius("hive_tyrant", "cannon");
        let radius = local_radius(&visual.parts[index].mesh.positions);
        assert!((authored - cap).abs() < 1.0e-4, "cannon visual radius {authored} is not the content cap {cap}");
        assert!((radius - authored).abs() < 1.0e-2, "cannon mesh {radius:.3} is not the authored radius {authored:.3}");
        let mesh = cast("hive_tyrant");
        let gap = closest_on_mesh(&mesh.positions, &mesh.indices, center).1;
        assert!(gap > radius, "hive cannon gap {gap:.3} stays on the tail, not the body cast");
    }

    #[test]
    fn an_overstretched_axis_scale_fails_before_the_fit() {
        let content = Content::embedded();
        let limit = content.cast.axis_scale_limit;
        let text = include_str!("../../../content/cast.toml");
        let authored: f32 = text
            .lines()
            .find(|line| line.starts_with("axis_scale_limit"))
            .expect("axis_scale_limit")
            .split('=')
            .nth(1)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!((limit - authored).abs() < 1.0e-6);
        let err = require_axis_scale("synthetic", [limit + 1.0, 1.0, 1.0], limit).expect_err("over the limit");
        assert!(err.contains("axis scale"), "{err}");
        require_axis_scale("synthetic", [1.0, 1.0, 1.0], limit).expect("a uniform scale loads");
        let spec = model_spec("hive_tyrant");
        let mut thin = RawMesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.1, 0.0], [0.0, 0.0, 1.0]],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0]; 4],
            indices: vec![0, 1, 2],
            base: Tex { w: 0, h: 0, rgba: Vec::new() },
            mask: Tex { w: 0, h: 0, rgba: Vec::new() },
            tris: 1,
            gripped: false,
        };
        let before = thin.positions.clone();
        let err = fit_hitbox(&mut thin, &spec, &content).expect_err("a thin cast");
        assert!(err.contains("axis scale"), "{err}");
        assert_eq!(thin.positions, before, "the stretch check runs before any vertex moves");
        let mut matched = RawMesh {
            positions: vec![[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 4.0]],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0]; 4],
            indices: vec![0, 1, 2],
            base: Tex { w: 0, h: 0, rgba: Vec::new() },
            mask: Tex { w: 0, h: 0, rgba: Vec::new() },
            tris: 1,
            gripped: false,
        };
        fit_hitbox(&mut matched, &spec, &content).expect("a matched box loads");
    }

    #[test]
    fn an_island_under_the_coverage_bar_fails_the_load() {
        let min_cov = Content::embedded().cast.min_island_coverage;
        assert!((min_cov - 0.98).abs() < 1.0e-6);
        let mut rgba = vec![0u8; 8 * 8 * 4];
        for y in 0..8 {
            for x in 0..8 {
                let i = (y * 8 + x) * 4;
                if x < 4 {
                    rgba[i] = 255;
                } else {
                    rgba[i + 1] = 255;
                }
            }
        }
        let mesh = RawMesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            indices: vec![0, 1, 2, 1, 3, 2],
            base: Tex { w: 0, h: 0, rgba: Vec::new() },
            mask: Tex { w: 8, h: 8, rgba },
            tris: 2,
            gripped: false,
        };
        let err = require_island_coverage(&mesh, "synthetic", min_cov).expect_err("a split island");
        assert!(err.contains("coverage"), "{err}");
        let mut solid = mesh;
        for px in solid.mask.rgba.chunks_exact_mut(4) {
            px[0] = 255;
            px[1] = 0;
            px[2] = 0;
            px[3] = 0;
        }
        require_island_coverage(&solid, "synthetic", min_cov).expect("one channel covers the island");
    }

    #[test]
    fn a_cast_that_misses_its_hitbox_fails_the_load() {
        let err = require_hitbox_fill("hive_tyrant", [2.0, 2.5, 2.0], [4.0, 5.0, 4.0], 0.02).expect_err("half a hitbox");
        assert!(err.contains("fills"), "{err}");
        require_hitbox_fill("hive_tyrant", [4.0, 5.0, 4.0], [4.0, 5.0, 4.0], 0.02).expect("an exact fill loads");
        let content = Content::embedded();
        for (id, row) in content.bugs.rows() {
            let mesh = cast(id);
            let hit = hit_bounds(&row.hitbox.body);
            let (mn, mx) = bounds(&mesh.positions);
            require_hitbox_fill(id, sub(mx, mn), sub(hit.max, hit.min), content.cast.hitbox_fill)
                .unwrap_or_else(|err| panic!("{err}"));
        }
    }

    #[test]
    fn the_tail_glow_is_capped_at_the_tip_and_visible_at_15m() {
        let content = Content::embedded();
        let cap = content.cast.tail_glow_radius;
        let text = include_str!("../../../content/cast.toml");
        let authored: f32 = text
            .lines()
            .find(|line| line.starts_with("tail_glow_radius"))
            .expect("tail_glow_radius")
            .split('=')
            .nth(1)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!((cap - authored).abs() < 1.0e-6);
        let (_, row) = content.bugs.rows().into_iter().find(|(id, _)| *id == "hive_tyrant").unwrap();
        let cannon_hit = row.hitbox.cannon.as_ref().expect("cannon");
        let hit_r = cannon_hit.radius;
        let visual_r = row
            .visual
            .parts
            .iter()
            .find(|part| part.name == "cannon")
            .expect("cannon")
            .visual_radius;
        assert!(visual_r <= cap + 1.0e-4, "authored glow {visual_r} exceeds tail_glow_radius {cap}");
        assert!((visual_r - cap).abs() < 1.0e-4, "the cannon authors the content cap");
        let tail_r = row
            .visual
            .parts
            .iter()
            .find(|part| part.name == "tail")
            .expect("tail")
            .size
            .first()
            .copied()
            .expect("tail radius");
        assert!(cap <= tail_r + 1.0e-3, "glow {cap} is wider than the tail tip {tail_r}");
        assert!(cap <= hit_r + 1.0e-3, "glow {cap} is wider than the hit radius {hit_r}");
        let visual = crate::bug("hive_tyrant");
        let cannon = visual.parts.iter().find(|part| part.name == "cannon").expect("cannon");
        let mesh_r = local_radius(&cannon.mesh.positions);
        assert!((mesh_r - visual_r).abs() < 1.0e-2, "cannon mesh {mesh_r} is not the authored radius {visual_r}");
        let px = crate::radius_px(cap, content.arena_dressing.review_distance, visual.camera.fov, 720.0);
        assert!(px >= 12.0, "tail glow is {px:.1}px at 15 m");
    }

    #[test]
    fn overdrive_brightens_only_the_named_channel() {
        let toward = [0.976, 0.451, 0.086];
        let gain = Content::embedded().cast.overdrive_emissive_gain;
        let albedo = [0.15, 0.45, 0.50];
        let hot = brighten_channel(albedo, [0.0, 0.0, 1.0, 0.0], 2, toward, gain);
        let expect = [
            (toward[0] * gain[0]).min(1.0),
            (toward[1] * gain[1]).min(1.0),
            (toward[2] * gain[2]).min(1.0),
        ];
        assert!((hot[0] - expect[0]).abs() < 1.0e-4 && (hot[1] - expect[1]).abs() < 1.0e-4 && (hot[2] - expect[2]).abs() < 1.0e-4, "still uses toward * gain {hot:?}");
        let other = brighten_channel(albedo, [1.0, 0.0, 0.0, 0.0], 2, toward, gain);
        assert_eq!(other, albedo);
        let (w, h, emit) = bake_overdrive_emissive(cast("vex_gun"), "accent");
        assert!(w > 1 && h > 1);
        assert!(emit.chunks(4).any(|px| px[0] > 200), "vex gun accent channel has weight");
        assert!(emit.chunks(4).any(|px| px[0] == 0), "the emissive mask is not the whole gun");
    }

    #[test]
    fn an_unmasked_body_texel_is_not_the_weak_color() {
        let content = Content::embedded();
        let look = &content.cast;
        for id in ["swarmer", "spitter", "brute", "brood_mother", "hive_tyrant"] {
            let mesh = cast(id);
            let weak = content.palette.key(&mesh.accent_key);
            let acid = content.palette.key(&mesh.glow_key);
            let base_c = content.palette.key(&mesh.base_key);
            let rim = if mesh.rim_key.is_empty() {
                [0.0, 0.0, 0.0, 0.0]
            } else {
                let c = content.palette.key(&mesh.rim_key);
                [c[0], c[1], c[2], 1.0]
            };
            let mut off = 0usize;
            for i in (0..mesh.mask.rgba.len()).step_by(4) {
                let mask = [
                    mesh.mask.rgba[i] as f32 / 255.0,
                    mesh.mask.rgba[i + 1] as f32 / 255.0,
                    mesh.mask.rgba[i + 2] as f32 / 255.0,
                ];
                if mask[0] > 0.0 || mask[1] > 0.0 {
                    continue;
                }
                off += 1;
                let u = ((i / 4) % mesh.mask.w as usize) as f32 / (mesh.mask.w.max(2) - 1) as f32;
                let v = ((i / 4) / mesh.mask.w as usize) as f32 / (mesh.mask.h.max(2) - 1) as f32;
                let base = sample(&mesh.base, [u, v]);
                for shade in [base[0], look.shade_mid_lin] {
                    let color = tint_texel(
                        shade,
                        mask,
                        base_c,
                        weak,
                        acid,
                        rim,
                        0.0,
                        look.shade_mid_lin,
                        look.rim_strength,
                        look.rim_fill,
                    );
                    assert!(
                        away(color, weak) && away(color, acid),
                        "{id} unmasked texel {color:?} is a weak-point color"
                    );
                }
            }
            assert!(off > 0, "{id} mask has no empty texels");
            for px in mesh.mask.rgba.chunks_exact(4) {
                for channel in &px[..3] {
                    assert!(
                        *channel == 0 || *channel == 255,
                        "{id} mask is not the cleaned binary PNG"
                    );
                }
            }
        }
    }

    #[test]
    fn rendered_body_pixels_away_from_the_glow_are_not_the_weak_color() {
        let content = Content::embedded();
        let w = 96u32;
        let h = 96u32;
        let bg = [0x11u8, 0x13, 0x18];
        for id in ["swarmer", "spitter", "brute", "brood_mother", "hive_tyrant"] {
            let mesh = cast(id);
            let (mn, mx) = bounds(&mesh.positions);
            let center = [(mn[0] + mx[0]) * 0.5, (mn[1] + mx[1]) * 0.5, (mn[2] + mx[2]) * 0.5];
            let ext = [mx[0] - mn[0], mx[1] - mn[1], mx[2] - mn[2]];
            let radius = (ext[0] * ext[0] + ext[1] * ext[1] + ext[2] * ext[2]).sqrt() * 0.5;
            let eye = [
                center[0] + radius * 1.7,
                center[1] + radius * 0.45,
                center[2] + radius * 1.9,
            ];
            let cam = look_cam(eye, center, 32.0, w, h);
            let mut color = Vec::with_capacity((w * h * 3) as usize);
            for _ in 0..(w * h) {
                color.extend_from_slice(&bg);
            }
            let mut depth = vec![f32::INFINITY; (w * h) as usize];
            let mut tags = vec![0u8; (w * h) as usize];
            let palette = |name: &str| content.palette.key(name);
            paint_tagged(
                &mut color,
                &mut depth,
                &mut tags,
                mesh,
                Placement {
                    pre_yaw: 0.0,
                    pre_shift: [0.0, 0.0, 0.0],
                    yaw_deg: 0.0,
                    lean: 0.0,
                    pivot: [0.0, 0.0, 0.0],
                    shift: [0.0, 0.0, 0.0],
                },
                &palette,
                &cam,
            );
            let weak = [
                content.palette.key(&mesh.accent_key),
                content.palette.key(&mesh.glow_key),
            ];
            let (_, row) = content.bugs.rows().into_iter().find(|(bug, _)| *bug == id).unwrap();
            let mut discs = Vec::new();
            for (_, shape) in row.hitbox.anchored() {
                if let Some(disc) = glow_disc(shape, &cam) {
                    discs.push(disc);
                }
            }
            for place in &row.hitbox.pods {
                if let Some(disc) = glow_disc_at(place.offset, row.hitbox.pod_radius().unwrap_or(0.0), &cam) {
                    discs.push(disc);
                }
            }
            let mut body = 0u32;
            let mut bad = 0u32;
            for i in 0..(w * h) as usize {
                let px = [color[i * 3], color[i * 3 + 1], color[i * 3 + 2]];
                if px == bg {
                    continue;
                }
                let x = (i as u32 % w) as f32 + 0.5;
                let y = (i as u32 / w) as f32 + 0.5;
                let on_glow = discs.iter().any(|(cx, cy, r)| {
                    let dx = x - cx;
                    let dy = y - cy;
                    dx * dx + dy * dy <= r * r
                });
                // Green is the acid glow on the mesh. Red on the brute is a speck, not trim.
                let mask_exempt = tags[i] == 2 || (id != "brute" && tags[i] == 1);
                if on_glow || mask_exempt {
                    continue;
                }
                body += 1;
                if weak.iter().any(|key| !byte_away(px, *key)) {
                    bad += 1;
                }
            }
            assert!(body > 30, "{id} rendered almost no body away from the glow ({body} px)");
            assert_eq!(bad, 0, "{id} has {bad} body pixels away from the glow in a weak-point color");
        }
    }
}

#[cfg(test)]
fn byte_away(px: [u8; 3], key: [f32; 4]) -> bool {
    let want = [
        (key[0].clamp(0.0, 1.0) * 255.0).round() as i32,
        (key[1].clamp(0.0, 1.0) * 255.0).round() as i32,
        (key[2].clamp(0.0, 1.0) * 255.0).round() as i32,
    ];
    (px[0] as i32 - want[0]).abs() > 20 || (px[1] as i32 - want[1]).abs() > 20 || (px[2] as i32 - want[2]).abs() > 20
}

#[cfg(test)]
fn look_cam(eye: [f32; 3], target: [f32; 3], fov_deg: f32, w: u32, h: u32) -> PaintCam {
    let forward = norm(sub(target, eye));
    let hint = if forward[1].abs() > 0.92 { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] };
    let right = norm(cross(forward, hint));
    let up = cross(right, forward);
    let fov = fov_deg.to_radians();
    PaintCam {
        eye,
        right,
        up,
        forward,
        focal: 1.0 / (fov * 0.5).tan(),
        aspect: w as f32 / h as f32,
        w,
        h,
        key: norm([-0.2, 0.92, 0.28]),
        fill: norm([0.55, 0.35, -0.45]),
    }
}

#[cfg(test)]
fn glow_disc(shape: &gemini_sim::bugs::ShapeDef, cam: &PaintCam) -> Option<(f32, f32, f32)> {
    glow_disc_at(shape.offset, shape.bounding_radius(), cam)
}

#[cfg(test)]
fn glow_disc_at(center: [f32; 3], radius: f32, cam: &PaintCam) -> Option<(f32, f32, f32)> {
    let screen = project(center, cam)?;
    let edge = project(
        [
            center[0] + cam.right[0] * radius,
            center[1] + cam.right[1] * radius,
            center[2] + cam.right[2] * radius,
        ],
        cam,
    )?;
    let dx = screen[0] - edge[0];
    let dy = screen[1] - edge[1];
    Some((screen[0], screen[1], (dx * dx + dy * dy).sqrt() + 2.0))
}

#[cfg(test)]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[cfg(test)]
fn away(color: [f32; 3], key: [f32; 4]) -> bool {
    (color[0] - key[0]).abs() > 0.08 || (color[1] - key[1]).abs() > 0.08 || (color[2] - key[2]).abs() > 0.08
}
