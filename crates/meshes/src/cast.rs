//! Meshy casts. The sim crate lists them. This crate reads the files, checks
//! budgets, fits each mesh to its hitbox, and tints masked texels from palette keys.
//! A missing file, an unknown id, or a blown budget fails the load. There is no
//! box-mesh fallback.

use std::path::Path;
use std::sync::OnceLock;

use gemini_sim::bugs::{hit_bounds, PartBounds};
use gemini_sim::content::{CastModel, Content};

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
    /// `weapon`, `hero`, `hitbox`, or `shield`. A weapon uses the view-gun grade.
    pub fit: String,
    pub base_key: String,
    pub accent_key: String,
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
        let bytes = model_bytes(&spec.id)?;
        out.push(fit_model(bytes, spec, &content)?);
    }
    Ok(out)
}

/// Disk read used by the missing-file check. The embedded bytes are what the client draws.
pub fn read_model_file(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|_| format!("missing model file {}", path.display()))
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
        "vex_body" => include_bytes!("../../../assets/masks/vex_body_mask.png"),
        "vex_gun" => include_bytes!("../../../assets/masks/vex_gun_mask.png"),
        "nyra_body" => include_bytes!("../../../assets/masks/nyra_body_mask.png"),
        "nyra_gun" => include_bytes!("../../../assets/masks/nyra_gun_mask.png"),
        "brakk_body" => include_bytes!("../../../assets/masks/brakk_body_mask.png"),
        "brakk_gun" => include_bytes!("../../../assets/masks/brakk_gun_mask.png"),
        "brakk_shield" => include_bytes!("../../../assets/masks/brakk_shield_mask.png"),
        "kip_body" => include_bytes!("../../../assets/masks/kip_body_mask.png"),
        "kip_gun" => include_bytes!("../../../assets/masks/kip_gun_mask.png"),
        "swarmer" => include_bytes!("../../../assets/masks/swarmer_mask.png"),
        "spitter" => include_bytes!("../../../assets/masks/spitter_mask.png"),
        "brute" => include_bytes!("../../../assets/masks/brute_mask.png"),
        "brood_mother" => include_bytes!("../../../assets/masks/brood_mother_mask.png"),
        "hive_tyrant" => include_bytes!("../../../assets/masks/hive_tyrant_mask.png"),
        other => return Err(format!("unknown mask file \"{other}\"")),
    })
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
    let mut decoded = decode_glb(bytes, &spec.id)?;
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
    Ok(CastMesh {
        id: spec.id.clone(),
        positions: decoded.positions,
        normals: decoded.normals,
        uvs: decoded.uvs,
        indices: decoded.indices,
        base: decoded.base,
        mask: decoded.mask,
        fit: spec.fit.clone(),
        base_key: spec.base.clone(),
        accent_key: spec.accent.clone(),
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
    let mut scale = f32::MAX;
    for i in 0..3 {
        if ms[i] <= 1e-6 || hs[i] <= 1e-6 {
            return Err(format!("{} has a zero extent on axis {i}", spec.id));
        }
        scale = scale.min(hs[i] / ms[i]);
    }
    let mc = mid(mn, mx);
    let hc = mid(hit.min, hit.max);
    for p in &mut mesh.positions {
        for i in 0..3 {
            p[i] = (p[i] - mc[i]) * scale + hc[i];
        }
    }
    if let Some(err) = outside_hitbox(&mesh.positions, hit) {
        return Err(format!("{} scale check failed: {err}", spec.id));
    }
    Ok(())
}

fn outside_hitbox(positions: &[[f32; 3]], hit: PartBounds) -> Option<String> {
    const EPS: f32 = 1.0e-3;
    for p in positions {
        for i in 0..3 {
            if p[i] < hit.min[i] - EPS || p[i] > hit.max[i] + EPS {
                return Some(format!("vertex {p:?} is outside the hitbox"));
            }
        }
    }
    None
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
    let (mn, mx) = bounds(&mesh.positions);
    let len = mx[0] - mn[0];
    if len <= 1e-6 {
        return Err(format!("{} has no length", spec.id));
    }
    let grip = butt_grip(&mesh.positions, mx[0], len);
    if grip[0] <= 0.0 {
        return Err(format!("{} grip is not on the +x butt", spec.id));
    }
    let scale = spec.size / len;
    for p in &mut mesh.positions {
        for i in 0..3 {
            p[i] *= scale;
        }
    }
    let grip = [grip[0] * scale, grip[1] * scale, grip[2] * scale];
    // -90° about Y sends the +X butt to +Z and the muzzle (-X) down -Z.
    yaw_positions(&mut mesh.positions, &mut mesh.normals, -90.0);
    let grip = yaw_point(grip, -90.0);
    for p in &mut mesh.positions {
        p[0] -= grip[0];
        p[1] -= grip[1];
        p[2] -= grip[2];
    }
    Ok(())
}

fn fit_shield(mesh: &mut RawMesh, spec: &CastModel) -> Result<(), String> {
    let handle = spec.grip;
    let near = mesh.positions.iter().any(|p| dist(*p, handle) < 0.03);
    if !near {
        return Err(format!("{} shield handle {:?} is not on the mesh", spec.id, handle));
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
    let handle = [handle[0] * scale, handle[1] * scale, handle[2] * scale];
    if spec.yaw_deg != 0.0 {
        yaw_positions(&mut mesh.positions, &mut mesh.normals, spec.yaw_deg);
    }
    let handle = yaw_point(handle, spec.yaw_deg);
    for p in &mut mesh.positions {
        p[0] -= handle[0];
        p[1] -= handle[1];
        p[2] -= handle[2];
    }
    Ok(())
}

fn butt_grip(positions: &[[f32; 3]], max_x: f32, len: f32) -> [f32; 3] {
    let mut acc = [0.0; 3];
    let mut n = 0.0;
    let cut = max_x - len * 0.04;
    for p in positions {
        if p[0] >= cut {
            acc[0] += p[0];
            acc[1] += p[1];
            acc[2] += p[2];
            n += 1.0;
        }
    }
    if n < 1.0 {
        return [max_x, 0.0, 0.0];
    }
    [acc[0] / n, acc[1] / n, acc[2] / n]
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

fn decode_glb(bytes: &[u8], id: &str) -> Result<RawMesh, String> {
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
    Ok(RawMesh {
        positions,
        normals,
        uvs,
        indices,
        base,
        mask,
        tris,
    })
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
/// Green mask texels are the exact glow color and are also written into the emissive image.
pub fn bake_textures(mesh: &CastMesh, palette: &dyn Fn(&str) -> [f32; 4]) -> (u32, u32, Vec<u8>, Vec<u8>) {
    let w = mesh.base.w.max(1);
    let h = mesh.base.h.max(1);
    let base_c = palette(&mesh.base_key);
    let accent = palette(&mesh.accent_key);
    let glow = palette(&mesh.glow_key);
    let rim = if mesh.rim_key.is_empty() {
        [0.0, 0.0, 0.0, 0.0]
    } else {
        let c = palette(&mesh.rim_key);
        [c[0], c[1], c[2], 1.0]
    };
    let (base_light, base_gray, rim_strength) = tint_constants();
    let (view_threshold, view_rim) = view_gun_grade();
    // The baked albedo has no view. Rim is the fresnel in the still raster.
    let view_gun = mesh.fit == "weapon";
    let fresnel = 0.0;
    let mut albedo = vec![0u8; (w * h * 4) as usize];
    let mut emit = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let u = if w > 1 { x as f32 / (w as f32 - 1.0) } else { 0.0 };
            let v = if h > 1 { 1.0 - (y as f32 / (h as f32 - 1.0)) } else { 0.0 };
            let base = sample(&mesh.base, [u, v]);
            let mask = sample(&mesh.mask, [u, v]);
            let color = if view_gun {
                tint_view_gun(base, mask, base_c, accent, glow, rim, 1.0, fresnel, view_threshold, view_rim)
            } else {
                tint_texel(base, mask, base_c, accent, glow, rim, 1.0, fresnel, base_light, base_gray, rim_strength)
            };
            let i = ((y * w + x) * 4) as usize;
            albedo[i] = (color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
            albedo[i + 1] = (color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
            albedo[i + 2] = (color[2].clamp(0.0, 1.0) * 255.0).round() as u8;
            albedo[i + 3] = 255;
            if (!view_gun && mask[1] > 0.0) || (view_gun && mask[1] >= view_threshold) {
                emit[i] = (glow[0].clamp(0.0, 1.0) * 255.0).round() as u8;
                emit[i + 1] = (glow[1].clamp(0.0, 1.0) * 255.0).round() as u8;
                emit[i + 2] = (glow[2].clamp(0.0, 1.0) * 255.0).round() as u8;
                emit[i + 3] = 255;
            }
        }
    }
    (w, h, albedo, emit)
}

/// The cleaned mask is applied as stored. Green is the glow. Red is the accent.
/// An empty mask lifts the gray base from `base_gray` to `base_light`. There is no noise threshold.
pub fn tint_texel(
    base: [f32; 3],
    mask: [f32; 3],
    base_c: [f32; 4],
    accent: [f32; 4],
    glow: [f32; 4],
    rim: [f32; 4],
    shade: f32,
    fresnel: f32,
    base_light: f32,
    base_gray: f32,
    rim_strength: f32,
) -> [f32; 3] {
    if mask[1] > 0.0 {
        return [glow[0], glow[1], glow[2]];
    }
    if mask[0] > 0.0 {
        return [accent[0], accent[1], accent[2]];
    }
    let lift = base_light / base_gray.max(1.0e-4);
    let mut color = [
        base[0] * lift * base_c[0] * shade,
        base[1] * lift * base_c[1] * shade,
        base[2] * lift * base_c[2] * shade,
    ];
    if rim[3] > 0.0 || rim[0] + rim[1] + rim[2] > 0.0 {
        let k = fresnel * rim_strength;
        color[0] += rim[0] * k;
        color[1] += rim[1] * k;
        color[2] += rim[2] * k;
    }
    color
}

/// First-person gun. `threshold` and `rim_strength` come from cast.toml.
/// The mask is the cleaned PNG. On a 0-or-255 mask the threshold matches a solid texel.
fn tint_view_gun(
    base: [f32; 3],
    mask: [f32; 3],
    base_c: [f32; 4],
    accent: [f32; 4],
    glow: [f32; 4],
    rim: [f32; 4],
    shade: f32,
    fresnel: f32,
    threshold: f32,
    rim_strength: f32,
) -> [f32; 3] {
    let r = mask[0];
    let g = mask[1];
    if g >= threshold {
        return [glow[0], glow[1], glow[2]];
    }
    if r >= threshold {
        return [accent[0], accent[1], accent[2]];
    }
    let unmasked = (1.0 - r - g).clamp(0.0, 1.0);
    let mut color = [
        base[0] * base_c[0] * shade * unmasked + base[0] * accent[0] * r + glow[0] * g,
        base[1] * base_c[1] * shade * unmasked + base[1] * accent[1] * r + glow[1] * g,
        base[2] * base_c[2] * shade * unmasked + base[2] * accent[2] * r + glow[2] * g,
    ];
    if rim[3] > 0.0 || rim[0] + rim[1] + rim[2] > 0.0 {
        let k = fresnel * rim_strength * unmasked;
        color[0] += rim[0] * k;
        color[1] += rim[1] * k;
        color[2] += rim[2] * k;
    }
    color
}

fn sample(tex: &Tex, uv: [f32; 2]) -> [f32; 3] {
    if tex.w == 0 || tex.h == 0 {
        return [0.5, 0.5, 0.5];
    }
    let u = uv[0] - uv[0].floor();
    let v = uv[1] - uv[1].floor();
    let x = ((u * (tex.w as f32 - 1.0)).round() as u32).min(tex.w - 1);
    let y = (((1.0 - v) * (tex.h as f32 - 1.0)).round() as u32).min(tex.h - 1);
    let i = ((y * tex.w + x) * 4) as usize;
    [
        tex.rgba[i] as f32 / 255.0,
        tex.rgba[i + 1] as f32 / 255.0,
        tex.rgba[i + 2] as f32 / 255.0,
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
    let base_c = palette(&mesh.base_key);
    let accent = palette(&mesh.accent_key);
    let glow = palette(&mesh.glow_key);
    let rim = if mesh.rim_key.is_empty() {
        [0.0, 0.0, 0.0, 0.0]
    } else {
        let c = palette(&mesh.rim_key);
        [c[0], c[1], c[2], 1.0]
    };
    let (base_light, base_gray, rim_strength) = tint_constants();
    let xform = |p: [f32; 3]| place_point(p, &place);
    let rotate_n = |n: [f32; 3]| place_normal(n, &place);
    paint_tris(
        color,
        depth,
        None,
        mesh,
        &xform,
        &rotate_n,
        cam,
        base_c,
        accent,
        glow,
        rim,
        silhouette,
        base_light,
        base_gray,
        rim_strength,
        false,
    );
}

/// First-person gun. Uses the cleaned mask and the view-gun grade from cast.toml.
pub fn paint_approved_gun(
    color: &mut [u8],
    depth: &mut [f32],
    mesh: &CastMesh,
    place: Placement,
    palette: &dyn Fn(&str) -> [f32; 4],
    cam: &PaintCam,
) {
    let base_c = palette(&mesh.base_key);
    let accent = palette(&mesh.accent_key);
    let glow = palette(&mesh.glow_key);
    let rim = if mesh.rim_key.is_empty() {
        [0.0, 0.0, 0.0, 0.0]
    } else {
        let c = palette(&mesh.rim_key);
        [c[0], c[1], c[2], 1.0]
    };
    let (base_light, base_gray, rim_strength) = tint_constants();
    let xform = |p: [f32; 3]| place_point(p, &place);
    let rotate_n = |n: [f32; 3]| place_normal(n, &place);
    paint_tris(
        color,
        depth,
        None,
        mesh,
        &xform,
        &rotate_n,
        cam,
        base_c,
        accent,
        glow,
        rim,
        false,
        base_light,
        base_gray,
        rim_strength,
        mesh.fit == "weapon",
    );
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
    let base_c = palette(&mesh.base_key);
    let accent = palette(&mesh.accent_key);
    let glow = palette(&mesh.glow_key);
    let rim = if mesh.rim_key.is_empty() {
        [0.0, 0.0, 0.0, 0.0]
    } else {
        let c = palette(&mesh.rim_key);
        [c[0], c[1], c[2], 1.0]
    };
    let (base_light, base_gray, rim_strength) = tint_constants();
    let xform = |p: [f32; 3]| place_point(p, &place);
    let rotate_n = |n: [f32; 3]| place_normal(n, &place);
    paint_tris(
        color,
        depth,
        Some(tags),
        mesh,
        &xform,
        &rotate_n,
        cam,
        base_c,
        accent,
        glow,
        rim,
        false,
        base_light,
        base_gray,
        rim_strength,
        false,
    );
}

fn paint_tris(
    color: &mut [u8],
    depth: &mut [f32],
    mut tags: Option<&mut [u8]>,
    mesh: &CastMesh,
    xform: &dyn Fn([f32; 3]) -> [f32; 3],
    rotate_n: &dyn Fn([f32; 3]) -> [f32; 3],
    cam: &PaintCam,
    base_c: [f32; 4],
    accent: [f32; 4],
    glow: [f32; 4],
    rim: [f32; 4],
    silhouette: bool,
    base_light: f32,
    base_gray: f32,
    rim_strength: f32,
    view_gun: bool,
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
            base_c,
            accent,
            glow,
            rim,
            silhouette,
            base_light,
            base_gray,
            rim_strength,
            view_gun,
        );
    }
}

fn tint_constants() -> (f32, f32, f32) {
    static CELL: OnceLock<(f32, f32, f32)> = OnceLock::new();
    *CELL.get_or_init(|| {
        let look = &Content::embedded().cast;
        (look.base_light, look.base_gray, look.rim_strength)
    })
}

fn view_gun_grade() -> (f32, f32) {
    static CELL: OnceLock<(f32, f32)> = OnceLock::new();
    *CELL.get_or_init(|| {
        let look = &Content::embedded().cast;
        (look.view_gun_mask_threshold, look.view_gun_rim_strength)
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
    base_c: [f32; 4],
    accent: [f32; 4],
    glow: [f32; 4],
    rim: [f32; 4],
    silhouette: bool,
    base_light: f32,
    base_gray: f32,
    rim_strength: f32,
    view_gun: bool,
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
                let ndot = (n[0] * cam.key[0] + n[1] * cam.key[1] + n[2] * cam.key[2]).clamp(0.0, 1.0);
                let ndot2 = (n[0] * cam.fill[0] + n[1] * cam.fill[1] + n[2] * cam.fill[2]).clamp(0.0, 1.0);
                let shade = (0.62 + 0.40 * ndot + 0.22 * ndot2).clamp(0.0, 1.15);
                let wp = [
                    world[0][0] * w0 + world[1][0] * w1 + world[2][0] * w2,
                    world[0][1] * w0 + world[1][1] * w1 + world[2][1] * w2,
                    world[0][2] * w0 + world[1][2] * w1 + world[2][2] * w2,
                ];
                let view = norm(sub(cam.eye, wp));
                let nd = norm(n);
                let facing = (nd[0] * view[0] + nd[1] * view[1] + nd[2] * view[2]).clamp(0.0, 1.0);
                let fresnel = (1.0 - facing).powf(2.0);
                let sampled = sample(&mesh.mask, uv);
                let texel = if view_gun {
                    let (threshold, view_rim) = view_gun_grade();
                    tint_view_gun(sample(&mesh.base, uv), sampled, base_c, accent, glow, rim, shade, fresnel, threshold, view_rim)
                } else {
                    tint_texel(
                        sample(&mesh.base, uv),
                        sampled,
                        base_c,
                        accent,
                        glow,
                        rim,
                        shade,
                        fresnel,
                        base_light,
                        base_gray,
                        rim_strength,
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
            assert!(mx[2] > -0.02 && mx[2] < 0.2, "{id} butt stays on +z, max z {}", mx[2]);
            assert!(mn[2] < -0.2, "{id} muzzle points down -z, min z {}", mn[2]);
        }
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
            assert!(outside_hitbox(&mesh.positions, hit).is_none(), "{id} leaves the hitbox");
            let authored = content.models.iter().find(|model| model.id == id).unwrap();
            assert_eq!(authored.fit, "hitbox", "{id}");
            let visual = crate::bug(id);
            let posed = rest_pose(visual);
            let mut glow = Vec::new();
            for (slot, shape) in row.hitbox.anchored() {
                gaps.push((
                    id.to_string(),
                    slot.to_string(),
                    shape.offset,
                    surface_gap(&mesh.positions, &mesh.indices, shape.offset),
                ));
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
            if *gap > 0.05 {
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
        let name = shape.anchor.as_str();
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
        assert!(
            authored + 1.0e-3 >= 0.70 * fit_r,
            "{id} {slot} visual radius {authored:.3} is under 70% of the hit radius {fit_r:.3}"
        );
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
        let mut best = f32::MAX;
        for tri in indices.chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            let d = dist_point_triangle(
                point,
                positions[tri[0] as usize],
                positions[tri[1] as usize],
                positions[tri[2] as usize],
            );
            if d < best {
                best = d;
            }
        }
        best
    }

    fn dist_point_triangle(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
        let ab = sub(b, a);
        let ac = sub(c, a);
        let ap = sub(p, a);
        let d1 = dot3(ab, ap);
        let d2 = dot3(ac, ap);
        if d1 <= 0.0 && d2 <= 0.0 {
            return dist(p, a);
        }
        let bp = sub(p, b);
        let d3 = dot3(ab, bp);
        let d4 = dot3(ac, bp);
        if d3 >= 0.0 && d4 <= d3 {
            return dist(p, b);
        }
        let vc = d1 * d4 - d3 * d2;
        if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
            let v = d1 / (d1 - d3);
            return dist(p, [a[0] + ab[0] * v, a[1] + ab[1] * v, a[2] + ab[2] * v]);
        }
        let cp = sub(p, c);
        let d5 = dot3(ab, cp);
        let d6 = dot3(ac, cp);
        if d6 >= 0.0 && d5 <= d6 {
            return dist(p, c);
        }
        let vb = d5 * d2 - d1 * d6;
        if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            let w = d2 / (d2 - d6);
            return dist(p, [a[0] + ac[0] * w, a[1] + ac[1] * w, a[2] + ac[2] * w]);
        }
        let va = d3 * d6 - d5 * d4;
        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
            let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
            let bc = sub(c, b);
            return dist(p, [b[0] + bc[0] * w, b[1] + bc[1] * w, b[2] + bc[2] * w]);
        }
        let denom = 1.0 / (va + vb + vc);
        let v = vb * denom;
        let w = vc * denom;
        dist(p, [a[0] + ab[0] * v + ac[0] * w, a[1] + ab[1] * v + ac[1] * w, a[2] + ab[2] * v + ac[2] * w])
    }

    fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
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
        assert!((look.base_light - 0.5).abs() < 1.0e-6);
        assert!((look.base_gray - 0.25).abs() < 1.0e-6);
        assert!((look.rim_strength - 1.0).abs() < 1.0e-6);
        assert!((look.view_gun_mask_threshold - 0.45).abs() < 1.0e-6);
        assert!((look.view_gun_rim_strength - 0.35).abs() < 1.0e-6);
        let text = include_str!("../../../content/cast.toml");
        assert!(
            !text.lines().any(|line| line.trim_start().starts_with("mask_threshold")),
            "mask_threshold is not a cast key"
        );
        let base = [look.base_gray, look.base_gray, look.base_gray];
        let accent = [1.0, 0.0, 0.0, 1.0];
        let glow = [0.0, 1.0, 0.0, 1.0];
        let rim = [1.0, 1.0, 1.0, 1.0];
        let base_c = [0.2, 0.4, 0.6, 1.0];
        let painted = tint_texel(base, [1.0, 0.0, 0.0], base_c, accent, glow, rim, 1.0, 0.0, look.base_light, look.base_gray, look.rim_strength);
        assert_eq!(painted, [1.0, 0.0, 0.0]);
        let acid = tint_texel(base, [0.0, 1.0, 0.0], base_c, accent, glow, rim, 1.0, 0.0, look.base_light, look.base_gray, look.rim_strength);
        assert_eq!(acid, [0.0, 1.0, 0.0]);
        let body = tint_texel(base, [0.0, 0.0, 0.0], base_c, accent, glow, rim, 1.0, 0.0, look.base_light, look.base_gray, look.rim_strength);
        assert!((body[0] - look.base_light * base_c[0]).abs() < 1.0e-4, "mid-gray lift {body:?}");
        let edged = tint_texel(base, [0.0, 0.0, 0.0], base_c, accent, glow, rim, 1.0, 1.0, look.base_light, look.base_gray, look.rim_strength);
        assert!((edged[0] - (body[0] + look.rim_strength)).abs() < 1.0e-4, "rim {edged:?}");
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
                let base = sample(&mesh.base, [u, 1.0 - v]);
                for shade in [0.6_f32, 1.0] {
                    let color = tint_texel(
                        base,
                        mask,
                        base_c,
                        weak,
                        acid,
                        rim,
                        shade,
                        0.0,
                        look.base_light,
                        look.base_gray,
                        look.rim_strength,
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
