//! Software frames of a crate reveal. Timing and colors come from the sim tables.

use gemini_sim::content::Content;
use gemini_sim::reveal::{look, RevealPlan};

/// A reveal still is flat when it is mostly one color: too few distinct
/// colors, a single color covering most of the frame, or almost no luminance spread.
pub fn reveal_still_is_flat(rgb: &[u8]) -> bool {
    let stats = still_color_stats(rgb);
    stats.unique < 32 || stats.dominant > 0.72 || stats.variance < 180.0
}

pub struct StillColorStats {
    pub unique: usize,
    pub variance: f32,
    pub dominant: f32,
}

pub fn still_color_stats(rgb: &[u8]) -> StillColorStats {
    let mut counts: std::collections::HashMap<(u8, u8, u8), u32> = std::collections::HashMap::new();
    let mut sum = 0.0f32;
    let mut sum_sq = 0.0f32;
    let mut n = 0u32;
    for px in rgb.chunks_exact(3) {
        *counts.entry((px[0], px[1], px[2])).or_insert(0) += 1;
        let luma = 0.3 * px[0] as f32 + 0.59 * px[1] as f32 + 0.11 * px[2] as f32;
        sum += luma;
        sum_sq += luma * luma;
        n += 1;
    }
    let pixels = n.max(1) as f32;
    let mean = sum / pixels;
    let dominant = counts.values().copied().max().unwrap_or(0) as f32 / pixels;
    StillColorStats {
        unique: counts.len(),
        variance: sum_sq / pixels - mean * mean,
        dominant,
    }
}

pub fn paint_reveal(content: &Content, plan: &RevealPlan, t: f32, width: u32, height: u32) -> Vec<u8> {
    let width = width.max(8);
    let height = height.max(8);
    let view = look(content, plan, t);
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    let layout = &content.crate_reveal.layout;
    paint_stage(&mut rgb, width, height, content, layout);
    let color = bytes(view.rgb);
    let ink = if luminance(view.rgb) > layout.ink_luma {
        palette_bytes(content, &layout.ink_dark)
    } else {
        palette_bytes(content, &layout.ink_light)
    };
    let light = palette_bytes(content, &layout.ink_light);
    if view.card {
        paint_card(
            &mut rgb,
            width,
            height,
            color,
            ink,
            light,
            &view.shown,
            &content.crate_reveal.card,
            layout,
        );
    } else {
        paint_crate(&mut rgb, width, height, &view, color, light, layout);
        if view.burst > 0.02 {
            paint_burst(&mut rgb, width, height, color, view.burst);
        }
    }
    if view.dim > 0.0 {
        let keep = 1.0 - view.dim;
        for px in rgb.chunks_exact_mut(3) {
            for channel in px.iter_mut() {
                *channel = (*channel as f32 * keep) as u8;
            }
        }
    }
    let hint = if view.card { "ENTER" } else { "SKIP" };
    paint_hint(&mut rgb, width, height, hint, light, &content.crate_reveal.hint, layout);
    rgb
}

pub(crate) fn paint_stage(rgb: &mut [u8], width: u32, height: u32, content: &Content, layout: &gemini_sim::content::RevealLayout) {
    let cx = (width as f32 - 1.0) * 0.5;
    let cy = (height as f32 - 1.0) * layout.stage_y;
    let max_d = (cx * cx + (height as f32) * (height as f32)).sqrt().max(1.0);
    let stage = palette_bytes(content, &layout.stage);
    let base = [
        stage[0] as f32 + layout.stage_base[0],
        stage[1] as f32 + layout.stage_base[1],
        stage[2] as f32 + layout.stage_base[2],
    ];
    let lift_c = layout.stage_lift;
    let vig = layout.stage_vignette;
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d = (dx * dx + dy * dy).sqrt() / max_d;
            let lift = (1.0 - d).clamp(0.0, 1.0).powf(layout.stage_lift_pow);
            let vignette = d.clamp(0.0, 1.0).powf(layout.stage_vignette_pow);
            // Center lifts toward a cool slate; the corners fall to near-black.
            let r = (base[0] + lift * lift_c[0] - vignette * vig[0]).clamp(0.0, 255.0) as u8;
            let g = (base[1] + lift * lift_c[1] - vignette * vig[1]).clamp(0.0, 255.0) as u8;
            let b = (base[2] + lift * lift_c[2] - vignette * vig[2]).clamp(0.0, 255.0) as u8;
            put(rgb, width, x, y, [r, g, b]);
        }
    }
}

fn paint_card(
    rgb: &mut [u8],
    width: u32,
    height: u32,
    color: [u8; 3],
    ink: [u8; 3],
    light: [u8; 3],
    shown: &str,
    card: &gemini_sim::content::CardTitle,
    layout: &gemini_sim::content::RevealLayout,
) {
    let (x, y, panel_w, panel_h) = card_panel(width, height, layout);
    let frame = blend(color, light, layout.border_mix);
    fill(rgb, width, height, x, y, panel_w, panel_h, frame);
    let border = layout.border_px.min(panel_w / 2).min(panel_h / 2);
    let face_x = x + border;
    let face_y = y + border;
    let face_w = panel_w.saturating_sub(border * 2);
    let face_h = panel_h.saturating_sub(border * 2);
    fill(rgb, width, height, face_x, face_y, face_w, face_h, color);

    let scale = shared_title_scale(layout, card);
    let label = shown.to_uppercase();
    let glyph = layout.title_glyph_px.max(1);
    let text_w = label.chars().count() as u32 * glyph * scale;
    let text_h = layout.glyph_rows.max(1) * scale;
    let gem_h = (layout.panel_height * layout.gem_height).round().max(1.0) as u32;
    let div_h = layout.divider_px.max(1);
    let stack = gem_h.saturating_add(div_h).saturating_add(text_h);
    let gap = face_h.saturating_sub(stack) as f32 / 4.0;
    let gem_top = face_y as f32 + gap;
    let div_top = gem_top + gem_h as f32 + gap;
    let text_top = div_top + div_h as f32 + gap;
    let cx = face_x + face_w / 2;
    paint_gem(
        rgb,
        width,
        height,
        cx,
        gem_top.round() as u32,
        gem_h,
        color,
        light,
        layout.gem_glow,
        (face_x, face_y, face_w, face_h),
        layout.gem_edge_blend,
    );
    let rule_w = face_w.saturating_mul(layout.rule_num) / layout.rule_den;
    fill(
        rgb,
        width,
        height,
        face_x + face_w.saturating_sub(rule_w) / 2,
        div_top.round() as u32,
        rule_w,
        div_h,
        blend(ink, color, layout.divider_blend),
    );
    let text_x = title_origin(face_x, face_w, text_w, card.title_pad.round().max(0.0) as u32);
    let text_y = text_top.round() as u32;
    blit_at(rgb, width, height, text_x + scale / 2, text_y + scale / 2, &label, blend(ink, [0, 0, 0], 0.65), scale);
    blit_at(rgb, width, height, text_x, text_y, &label, ink, scale);
}

fn paint_gem(
    rgb: &mut [u8],
    width: u32,
    height: u32,
    cx: u32,
    top: u32,
    gem_h: u32,
    color: [u8; 3],
    light: [u8; 3],
    glow: f32,
    face: (u32, u32, u32, u32),
    edge_blend: f32,
) {
    let gem_h = gem_h.max(1);
    let cy = top + gem_h / 2;
    add_glow(rgb, width, height, cx, cy, gem_h, blend(color, light, 0.7), glow, face);
    let mid = (gem_h as f32 - 1.0) * 0.5;
    for row in 0..gem_h {
        let t = 1.0 - (row as f32 - mid).abs() / mid.max(0.5);
        let half = ((gem_h as f32) * 0.5 * t.clamp(0.0, 1.0)).round().max(1.0) as u32;
        let y = top + row;
        let x = cx.saturating_sub(half);
        let w = half.saturating_mul(2).max(1);
        fill(rgb, width, height, x, y, w, 1, color);
        put(rgb, width, x, y, blend(color, light, edge_blend));
        if w > 1 {
            put(rgb, width, x + w - 1, y, blend(color, light, edge_blend));
        }
    }
}

fn paint_crate(
    rgb: &mut [u8],
    width: u32,
    height: u32,
    view: &gemini_sim::reveal::RevealLook,
    color: [u8; 3],
    light: [u8; 3],
    layout: &gemini_sim::content::RevealLayout,
) {
    let crate_w = (width / layout.crate_w_div).max(layout.crate_w_min);
    let crate_h = (height / 3).max(crate_w + 4);
    let landed = (height / 2) as i32;
    let risen = landed - (height as i32 / 4);
    let dropped = risen + ((landed - risen) as f32 * view.drop) as i32;
    let y = (dropped + ((risen - dropped) as f32 * view.rise) as i32).max(8) as u32;
    let top = y.saturating_sub(crate_h / 2);
    let cx = width / 2;
    let left = cx.saturating_sub(crate_w / 2);
    add_glow(
        rgb,
        width,
        height,
        cx,
        top + crate_h / 2,
        crate_w.saturating_mul(3).max(24),
        color,
        0.8,
        (0, 0, width, height),
    );
    let shadow_w = crate_w.saturating_mul(2);
    let shadow_h = (crate_h / 8).max(3);
    let shadow_x = cx.saturating_sub(shadow_w / 2);
    let shadow_y = top + crate_h + shadow_h / 2;
    for row in 0..shadow_h {
        let t = 1.0 - (row as f32 / shadow_h as f32);
        let tone = blend([8, 6, 12], [20, 16, 28], t);
        let inset = ((1.0 - t) * (shadow_w as f32) * 0.25) as u32;
        fill(rgb, width, height, shadow_x + inset, shadow_y + row, shadow_w.saturating_sub(inset * 2), 1, tone);
    }
    paint_capsule(rgb, width, height, left, top, crate_w, crate_h, color, light);
    if view.crack > 0.0 {
        paint_cracks(rgb, width, height, cx, top, crate_w, crate_h, color, light, view.crack, layout);
    }
}

/// Vertical pill: semicircular caps and a straight body, with a dark seam.
fn paint_capsule(rgb: &mut [u8], width: u32, height: u32, left: u32, top: u32, crate_w: u32, crate_h: u32, color: [u8; 3], light: [u8; 3]) {
    let radius = (crate_w as f32) * 0.5;
    let body = (crate_h as f32 - radius * 2.0).max(0.0);
    let cx = left as f32 + radius;
    let dark = [10u8, 8, 14];
    for row in 0..crate_h {
        let local = row as f32;
        let dy = if local < radius {
            radius - local
        } else if local > radius + body {
            local - (radius + body)
        } else {
            0.0
        };
        let half = (radius * radius - dy * dy).max(0.0).sqrt();
        if half < 0.5 {
            continue;
        }
        let x0 = (cx - half).ceil().max(0.0) as u32;
        let x1 = (cx + half).floor().max(0.0) as u32;
        let py = top + row;
        if py >= height {
            break;
        }
        for x in x0..=x1.min(width.saturating_sub(1)) {
            let nx = ((x as f32) - cx) / radius;
            let rim = (x as f32 - cx).abs() > half - 1.6;
            // The interior stays the exact rarity color so a swatch can read the tier.
            let tone = if rim {
                blend(color, light, 0.42)
            } else if nx.abs() > 0.62 {
                if nx < 0.0 {
                    blend(color, light, 0.28)
                } else {
                    blend(color, dark, 0.4)
                }
            } else {
                color
            };
            put(rgb, width, x, py, tone);
        }
    }
    let seam = blend(color, dark, 0.72);
    let seam_w = (crate_w / 28).max(2);
    let inset = (radius * 0.35) as u32;
    for row in inset..crate_h.saturating_sub(inset) {
        let py = top + row;
        if py >= height {
            break;
        }
        fill(rgb, width, height, (cx as u32).saturating_sub(seam_w / 2), py, seam_w, 1, seam);
    }
}

/// Glowing rarity-colored cracks leave the seam. `crack` is 0 on the drop and about 0.5 mid-crack.
fn paint_cracks(
    rgb: &mut [u8],
    width: u32,
    height: u32,
    cx: u32,
    top: u32,
    crate_w: u32,
    crate_h: u32,
    color: [u8; 3],
    light: [u8; 3],
    crack: f32,
    layout: &gemini_sim::content::RevealLayout,
) {
    let crack = crack.clamp(0.0, 1.0);
    let reach = (crate_w as f32) * layout.crack_reach * crack;
    let mid_y = top as f32 + crate_h as f32 * layout.crack_mid;
    let hot = blend(color, light, layout.crack_hot_blend);
    let glow = blend(color, light, layout.crack_glow_blend);
    for ray in &layout.crack_rays {
        let (dx, dy) = (ray[0], ray[1]);
        for sign in [-1.0f32, 1.0] {
            let x1 = cx as f32 + sign * dx * reach;
            let y1 = mid_y + dy * reach;
            stroke(rgb, width, height, cx as f32, mid_y, x1, y1, glow, 0.55, 3);
            stroke(rgb, width, height, cx as f32, mid_y, x1, y1, hot, 0.92, 1);
        }
    }
}

fn stroke(rgb: &mut [u8], width: u32, height: u32, x0: f32, y0: f32, x1: f32, y1: f32, color: [u8; 3], alpha: f32, thickness: i32) {
    let steps = ((x1 - x0).abs().max((y1 - y0).abs()) as i32).max(1);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let x = x0 + (x1 - x0) * t;
        let y = y0 + (y1 - y0) * t;
        for oy in -thickness..=thickness {
            for ox in -thickness..=thickness {
                if ox * ox + oy * oy > thickness * thickness {
                    continue;
                }
                let px = x.round() as i32 + ox;
                let py = y.round() as i32 + oy;
                if px < 0 || py < 0 {
                    continue;
                }
                blend_put(rgb, width, height, px as u32, py as u32, color, alpha);
            }
        }
    }
}

fn paint_burst(rgb: &mut [u8], width: u32, height: u32, color: [u8; 3], burst: f32) {
    let wash = blend(color, [255, 248, 230], 0.72);
    let mid_y = (height as f32) * 0.5;
    for y in 0..height {
        let band = ((y as f32 - mid_y).abs() / mid_y).clamp(0.0, 1.0);
        let t = burst * (1.0 - band).powf(1.35) * 0.9;
        if t < 0.02 {
            continue;
        }
        for x in 0..width {
            let i = ((y * width + x) * 3) as usize;
            let px = [rgb[i], rgb[i + 1], rgb[i + 2]];
            let next = blend(px, wash, t);
            rgb[i..i + 3].copy_from_slice(&next);
        }
    }
}

fn card_panel(width: u32, height: u32, layout: &gemini_sim::content::RevealLayout) -> (u32, u32, u32, u32) {
    let panel_w = (layout.panel_width.round() as u32).clamp(1, width.max(1));
    let panel_h = (layout.panel_height.round() as u32).clamp(1, height.max(1));
    let x = width.saturating_sub(panel_w) / 2;
    let y = height.saturating_sub(panel_h) / 2;
    (x, y, panel_w, panel_h)
}

/// Centers a title in the face. `pad` is `card.title_pad`. A centered title
/// sits in the same place inside a symmetric inset, and a word wider than
/// that inset stays centered on the full face.
fn title_origin(face_x: u32, face_w: u32, text_w: u32, pad: u32) -> u32 {
    let inner = face_w.saturating_sub(pad.saturating_mul(2));
    if text_w <= inner {
        face_x + pad + inner.saturating_sub(text_w) / 2
    } else {
        face_x + face_w.saturating_sub(text_w) / 2
    }
}

fn shared_title_scale(layout: &gemini_sim::content::RevealLayout, card: &gemini_sim::content::CardTitle) -> u32 {
    let panel_w = layout.panel_width.round().max(0.0) as u32;
    let panel_h = layout.panel_height.round().max(0.0) as u32;
    let face_w = panel_w.saturating_sub(layout.border_px.saturating_mul(2));
    let face_h = panel_h.saturating_sub(layout.border_px.saturating_mul(2));
    title_scale(face_w, face_h, "LEGENDARY".chars().count() as u32, layout, card)
}

fn title_scale(
    face_w: u32,
    face_h: u32,
    cols: u32,
    layout: &gemini_sim::content::RevealLayout,
    card: &gemini_sim::content::CardTitle,
) -> u32 {
    let cols = cols.max(1);
    let glyph = layout.title_glyph_px.max(1);
    let rows = layout.title_height_div.max(1);
    let pad = still_title_pad(layout, card);
    let fit_w = face_w.saturating_sub(pad.saturating_mul(2)) / (cols * glyph);
    let fit_h = face_h / rows;
    fit_w.min(fit_h).clamp(layout.title_scale_min, layout.title_scale_max)
}

/// The still fit uses the layout pad. A tighter card inset (a smaller
/// `title_pad`) replaces it. The shipped inset is looser, so the scale stays.
fn still_title_pad(layout: &gemini_sim::content::RevealLayout, card: &gemini_sim::content::CardTitle) -> u32 {
    let inset = card.title_pad.round().max(0.0) as u32;
    layout.title_pad_px.min(inset)
}

fn paint_hint(
    rgb: &mut [u8],
    width: u32,
    height: u32,
    text: &str,
    ink: [u8; 3],
    hint: &gemini_sim::content::RevealHint,
    layout: &gemini_sim::content::RevealLayout,
) {
    let rows_n = layout.glyph_rows.max(1);
    let glyph = layout.title_glyph_px.max(1);
    let scale = (hint.text_px / rows_n as f32).round().max(1.0) as u32;
    let cols = text.chars().count() as u32 * glyph * scale;
    let rows = rows_n * scale;
    let margin_x = hint.margin_x.round().max(0.0) as u32;
    let margin_y = hint.margin_y.round().max(0.0) as u32;
    let origin_x = width.saturating_sub(margin_x + cols);
    let origin_y = height.saturating_sub(margin_y + rows);
    blit_alpha(rgb, width, height, origin_x, origin_y, text, ink, scale, hint.opacity.clamp(0.0, 1.0));
}

fn add_glow(
    rgb: &mut [u8],
    width: u32,
    height: u32,
    cx: u32,
    cy: u32,
    radius: u32,
    color: [u8; 3],
    strength: f32,
    clip: (u32, u32, u32, u32),
) {
    let radius = radius.max(4);
    let r2 = (radius as f32) * (radius as f32);
    let x0 = cx.saturating_sub(radius).max(clip.0);
    let y0 = cy.saturating_sub(radius).max(clip.1);
    let x1 = (cx + radius).min(width).min(clip.0.saturating_add(clip.2));
    let y1 = (cy + radius).min(height).min(clip.1.saturating_add(clip.3));
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 - cx as f32;
            let dy = y as f32 - cy as f32;
            let d2 = dx * dx + dy * dy;
            if d2 >= r2 {
                continue;
            }
            let falloff = (1.0 - (d2 / r2).sqrt()).powf(1.7) * strength;
            let i = ((y * width + x) * 3) as usize;
            let px = [rgb[i], rgb[i + 1], rgb[i + 2]];
            let next = blend(px, color, falloff);
            rgb[i..i + 3].copy_from_slice(&next);
        }
    }
}

fn put(rgb: &mut [u8], width: u32, x: u32, y: u32, color: [u8; 3]) {
    if x >= width {
        return;
    }
    let i = ((y * width + x) * 3) as usize;
    if i + 2 < rgb.len() {
        rgb[i..i + 3].copy_from_slice(&color);
    }
}

fn palette_bytes(content: &Content, name: &str) -> [u8; 3] {
    bytes(content.palette.key(name))
}

fn bytes(rgb: [f32; 4]) -> [u8; 3] {
    [
        (rgb[0] * 255.0).round() as u8,
        (rgb[1] * 255.0).round() as u8,
        (rgb[2] * 255.0).round() as u8,
    ]
}

fn luminance(rgb: [f32; 4]) -> f32 {
    0.3 * rgb[0] + 0.59 * rgb[1] + 0.11 * rgb[2]
}

fn blend(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 * (1.0 - t) + b[0] as f32 * t) as u8,
        (a[1] as f32 * (1.0 - t) + b[1] as f32 * t) as u8,
        (a[2] as f32 * (1.0 - t) + b[2] as f32 * t) as u8,
    ]
}

fn fill(rgb: &mut [u8], width: u32, height: u32, x: u32, y: u32, w: u32, h: u32, color: [u8; 3]) {
    for row in y..y.saturating_add(h).min(height) {
        for col in x..x.saturating_add(w).min(width) {
            let i = ((row * width + col) * 3) as usize;
            rgb[i..i + 3].copy_from_slice(&color);
        }
    }
}

fn blend_put(rgb: &mut [u8], width: u32, height: u32, x: u32, y: u32, color: [u8; 3], alpha: f32) {
    if x >= width || y >= height {
        return;
    }
    let i = ((y * width + x) * 3) as usize;
    if i + 2 >= rgb.len() {
        return;
    }
    let next = blend([rgb[i], rgb[i + 1], rgb[i + 2]], color, alpha);
    rgb[i..i + 3].copy_from_slice(&next);
}

fn blit_alpha(rgb: &mut [u8], width: u32, height: u32, origin_x: u32, origin_y: u32, text: &str, ink: [u8; 3], scale: u32, alpha: f32) {
    let scale = scale.max(1);
    for (index, ch) in text.chars().enumerate() {
        let glyph = glyph(ch);
        let gx = origin_x + index as u32 * 6 * scale;
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) == 0 {
                    continue;
                }
                for oy in 0..scale {
                    for ox in 0..scale {
                        blend_put(rgb, width, height, gx + col * scale + ox, origin_y + row as u32 * scale + oy, ink, alpha);
                    }
                }
            }
        }
    }
}

fn blit_at(rgb: &mut [u8], width: u32, height: u32, origin_x: u32, origin_y: u32, text: &str, ink: [u8; 3], scale: u32) {
    let scale = scale.max(1);
    for (index, ch) in text.chars().enumerate() {
        let glyph = glyph(ch);
        let gx = origin_x + index as u32 * 6 * scale;
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) == 0 {
                    continue;
                }
                fill(rgb, width, height, gx + col * scale, origin_y + row as u32 * scale, scale, scale, ink);
            }
        }
    }
}

fn glyph(ch: char) -> [u8; 7] {
    crate::glyph::bits(ch)
}

/// Card slots in roll order, centered in the fit frame.
pub fn grid_slots(content: &Content, width: u32, height: u32) -> Result<Vec<(u32, u32, u32, u32)>, String> {
    let grid = &content.crate_reveal.grid;
    let fit_w = content.ui.layout.fit_width;
    let fit_h = content.ui.layout.fit_height;
    if !(fit_w > 0.0) || !(fit_h > 0.0) || width == 0 || height == 0 {
        return Err("grid still needs a positive frame".into());
    }
    let sx = width as f32 / fit_w;
    let sy = height as f32 / fit_h;
    let card_w = (grid.card_w * sx).round().max(1.0) as u32;
    let card_h = (grid.card_h * sy).round().max(1.0) as u32;
    let gap_x = (grid.gap * sx).round() as u32;
    let gap_y = (grid.gap * sy).round() as u32;
    let cols = grid.columns;
    let rows = grid.rows;
    let grid_w = card_w * cols + gap_x * cols.saturating_sub(1);
    let grid_h = card_h * rows + gap_y * rows.saturating_sub(1);
    let origin_x = width.saturating_sub(grid_w) / 2;
    let origin_y = height.saturating_sub(grid_h) / 2;
    let mut slots = Vec::new();
    for index in 0..cols * rows {
        let col = index % cols;
        let row = index / cols;
        let x = origin_x + col * (card_w + gap_x);
        let y = origin_y + row * (card_h + gap_y);
        slots.push((x, y, card_w, card_h));
    }
    Ok(slots)
}

/// One ten-pull frame. `items` is roll order. `None` is the shared back.
/// `ready` is false until the best card's face is up. `again` is false when the wallet cannot pay.
pub struct GridStill<'a> {
    pub items: &'a [Option<&'a str>],
    pub ready: bool,
    pub again: bool,
    pub focus: usize,
    pub shortfall: &'a str,
    /// Shown under the buttons when a pull again failed or replayed. Empty draws nothing.
    pub failure: &'a str,
}

/// Refund column. `again` is false when a ten-pull does not fit in the wallet.
pub struct RefundStill<'a> {
    pub again: bool,
    pub focus: usize,
    pub shortfall: &'a str,
    /// Shown on the stage under the panel. Empty draws nothing.
    pub failure: &'a str,
}

/// The live reveal screen and the stills call this. `None` paints the shared back.
pub fn paint_grid(content: &Content, shot: &GridStill<'_>, width: u32, height: u32) -> Result<(Vec<u8>, Vec<crate::menu_still::Hit>), String> {
    let cells = (content.crate_reveal.grid.columns * content.crate_reveal.grid.rows) as usize;
    if shot.items.len() != cells {
        return Err(format!("grid has {} faces and content says {cells}", shot.items.len()));
    }
    let mut rgb = vec![0u8; (width as usize) * (height as usize) * 3];
    paint_stage(&mut rgb, width, height, content, &content.crate_reveal.layout);
    let slots = grid_slots(content, width, height)?;
    for (slot, face) in slots.iter().zip(shot.items) {
        let (x, y, w, h) = *slot;
        match face {
            Some(item_id) => {
                let Some(item) = content.crate_item(item_id) else {
                    return Err(format!("unknown crate item {item_id}"));
                };
                paint_grid_face(&mut rgb, width, height, content, x, y, w, h, item)?;
            }
            None => paint_grid_back(&mut rgb, width, height, content, x, y, w, h),
        }
    }
    let hits = paint_result_chrome(&mut rgb, width, height, content, shot.ready, shot.again, shot.focus, shot.shortfall, shot.failure)?;
    Ok((rgb, hits))
}

/// Refund column in the bank bevel, sized to the banner, the message, and the two buttons.
pub fn paint_refund(content: &Content, shot: &RefundStill<'_>, width: u32, height: u32) -> Result<(Vec<u8>, Vec<crate::menu_still::Hit>), String> {
    let grid = &content.crate_reveal.grid;
    let fit_w = content.ui.layout.fit_width;
    let fit_h = content.ui.layout.fit_height;
    if !(fit_w > 0.0) || !(fit_h > 0.0) || width == 0 || height == 0 {
        return Err("refund still needs a positive frame".into());
    }
    let mut rgb = vec![0u8; (width as usize) * (height as usize) * 3];
    paint_stage(&mut rgb, width, height, content, &content.crate_reveal.layout);
    let sy = height as f32 / fit_h;
    let ui = &content.ui.layout;
    let layout = &content.crate_reveal.layout;
    let title_h = ((ui.title_scale * layout.glyph_rows) as f32 * sy).round() as i32;
    let line_h = ((ui.button_scale * layout.glyph_rows) as f32 * sy).round() as i32;
    let (x, mut y, panel_w, panel_h, button_w, button_h, gap, margin_x, margin_y) = refund_column(content, width, height, !shot.shortfall.is_empty());
    if !shot.failure.is_empty() {
        let clear = layout.failure_clear.round() as i32;
        let overflow = y + panel_h + line_h + clear - height as i32;
        if overflow > 0 {
            y -= overflow;
        }
        if y < 0 || y + panel_h + line_h + clear > height as i32 {
            return Err("refund failure line does not fit under the panel".into());
        }
    }
    let face = palette_bytes(content, &grid.refund_face);
    let ink = palette_bytes(content, &grid.refund_ink);
    draw_bevel(&mut rgb, width, height, x, y, panel_w, panel_h, &chunk_bevel(content, panel_h, panel_h as f32, ink, face));
    let mut ty = y + margin_y;
    let banner = content.banners.id.to_uppercase();
    let title_scale = ((ui.title_scale as f32) * sy).round().max(1.0) as u32;
    blit_centered(&mut rgb, width, height, x, panel_w, ty, &banner, ink, title_scale, layout);
    ty += title_h + gap;
    let body = ((ui.button_scale as f32) * sy).round().max(1.0) as u32;
    blit_centered(&mut rgb, width, height, x, panel_w, ty, &grid.refund_message, ink, body, layout);
    ty += line_h + gap;
    if !shot.shortfall.is_empty() {
        blit_centered(&mut rgb, width, height, x, panel_w, ty, shot.shortfall, ink, body, layout);
        ty += line_h + gap;
    }
    let bx = x + margin_x;
    let mut hits = Vec::new();
    let continue_hit = paint_text_button(&mut rgb, width, height, content, bx, ty, button_w, button_h, &content.ui.copy.crates_continue, true, shot.focus == 0);
    hits.push(crate::menu_still::Hit { id: "continue".into(), ..continue_hit });
    ty += button_h + gap;
    let again_hit = paint_text_button(&mut rgb, width, height, content, bx, ty, button_w, button_h, &content.ui.copy.crates_pull_again, shot.again, shot.again && shot.focus == 1);
    if shot.again {
        hits.push(crate::menu_still::Hit { id: "pull_again".into(), ..again_hit });
    }
    if !shot.failure.is_empty() {
        let failure_y = y + panel_h;
        if failure_y < 0 || failure_y + line_h > height as i32 {
            return Err("refund failure line does not fit under the panel".into());
        }
        let failure_ink = palette_bytes(content, &grid.failure_ink);
        blit_centered(&mut rgb, width, height, 0, width as i32, failure_y, shot.failure, failure_ink, body, layout);
    }
    Ok((rgb, hits))
}

/// Column panel, then button width, button height, row gap, and the two margins.
fn refund_column(content: &Content, width: u32, height: u32, shortfall: bool) -> (i32, i32, i32, i32, i32, i32, i32, i32, i32) {
    let fit_w = content.ui.layout.fit_width.max(1.0);
    let fit_h = content.ui.layout.fit_height.max(1.0);
    let sx = width as f32 / fit_w;
    let sy = height as f32 / fit_h;
    let ui = &content.ui.layout;
    let layout = &content.crate_reveal.layout;
    let title_h = ((ui.title_scale * layout.glyph_rows) as f32 * sy).round() as i32;
    let line_h = ((ui.button_scale * layout.glyph_rows) as f32 * sy).round() as i32;
    let button_w = (ui.button_width * sx).round() as i32;
    let button_h = (ui.button_height * sy).round() as i32;
    let gap = (ui.row_gap * sy).round() as i32;
    let margin_x = (ui.margin_x * sx).round() as i32;
    let margin_y = (ui.margin_y * sy).round() as i32;
    let short_h = if shortfall { line_h + gap } else { 0 };
    let panel_w = button_w + margin_x * 2;
    let panel_h = margin_y + title_h + gap + line_h + gap + short_h + button_h + gap + button_h + margin_y;
    let x = (width as i32 - panel_w) / 2;
    let y = (height as i32 - panel_h) / 2;
    (x, y, panel_w, panel_h, button_w, button_h, gap, margin_x, margin_y)
}

fn paint_grid_back(rgb: &mut [u8], width: u32, height: u32, content: &Content, x: u32, y: u32, w: u32, h: u32) {
    let fill = palette_bytes(content, &content.crate_reveal.grid.back);
    let edge = palette_bytes(content, &content.crate_reveal.grid.frame);
    let reference = content.crate_reveal.layout.panel_height;
    draw_bevel(rgb, width, height, x as i32, y as i32, w as i32, h as i32, &chunk_bevel(content, h as i32, reference, edge, fill));
}

fn paint_grid_face(
    rgb: &mut [u8],
    width: u32,
    height: u32,
    content: &Content,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    item: &gemini_sim::content::CrateItem,
) -> Result<(), String> {
    let fill = palette_bytes(content, &content.crate_reveal.grid.face);
    let edge = palette_bytes(content, &format!("rarity_{}", item.rarity));
    let reference = content.crate_reveal.layout.panel_height;
    draw_bevel(rgb, width, height, x as i32, y as i32, w as i32, h as i32, &chunk_bevel(content, h as i32, reference, edge, fill));
    let face = inner_face(content, x as i32, y as i32, w as i32, h as i32);
    let layout = &content.crate_reveal.layout;
    let ink = face_ink(content, fill);
    let icon_ink = palette_bytes(content, &item.ink);
    let light = palette_bytes(content, &layout.ink_light);
    paint_item_icon(rgb, width, height, content, item, icon_ink, light, face)?;
    let cols = content.banners.items.iter().map(|row| row.name.chars().count()).max().unwrap_or(1) as u32;
    let scale = title_scale(face.2 as u32, face.3 as u32, cols, layout, &content.crate_reveal.card);
    let text_h = layout.glyph_rows.max(1) * scale;
    let text_w = item.name.chars().count() as u32 * layout.title_glyph_px.max(1) * scale;
    let text_x = title_origin(face.0 as u32, face.2 as u32, text_w, content.crate_reveal.card.title_pad.round().max(0.0) as u32);
    let text_y = (face.1 + face.3).saturating_sub(text_h as i32 + 2).max(face.1) as u32;
    blit_at(rgb, width, height, text_x, text_y, &item.name, ink, scale);
    Ok(())
}

fn paint_item_icon(
    rgb: &mut [u8],
    width: u32,
    height: u32,
    content: &Content,
    item: &gemini_sim::content::CrateItem,
    ink: [u8; 3],
    light: [u8; 3],
    face: (i32, i32, i32, i32),
) -> Result<(), String> {
    let icon = &content.crate_reveal.icon;
    let rows = item.icon.len() as i32;
    let cols = item.icon.first().map(|row| row.chars().count()).unwrap_or(0) as i32;
    if rows <= 0 || cols <= 0 {
        return Err(format!("crate item {} icon is empty", item.id));
    }
    if icon.width_den == 0 || icon.height_den == 0 || icon.highlight_div == 0 {
        return Err("crate_reveal.toml icon divisors must be > 0".into());
    }
    let avail_w = face.2 * icon.width_num as i32 / icon.width_den as i32;
    let avail_h = face.3 * icon.height_num as i32 / icon.height_den as i32;
    if avail_w < 1 || avail_h < 1 {
        return Err(format!("crate item {} icon box is empty", item.id));
    }
    let scale = (avail_w / cols).min(avail_h / rows);
    if scale < 1 {
        return Err(format!("crate item {} icon does not fit the card", item.id));
    }
    let icon_w = cols * scale;
    let icon_h = rows * scale;
    let origin_x = face.0 + (face.2 - icon_w) / 2;
    let origin_y = face.1 + (avail_h - icon_h) / 2;
    let highlight_at = (rows + cols) / icon.highlight_div as i32;
    for (row_index, row) in item.icon.iter().enumerate() {
        for (col_index, ch) in row.chars().enumerate() {
            if ch != '#' {
                continue;
            }
            let lit = (row_index as i32) + (col_index as i32) < highlight_at;
            let color = if lit { blend(ink, light, icon.highlight_blend) } else { ink };
            fill(
                rgb,
                width,
                height,
                (origin_x + col_index as i32 * scale) as u32,
                (origin_y + row_index as i32 * scale) as u32,
                scale as u32,
                scale as u32,
                color,
            );
        }
    }
    Ok(())
}

fn paint_result_chrome(
    rgb: &mut [u8],
    width: u32,
    height: u32,
    content: &Content,
    ready: bool,
    again: bool,
    focus: usize,
    shortfall: &str,
    failure: &str,
) -> Result<Vec<crate::menu_still::Hit>, String> {
    let layout = &content.crate_reveal.layout;
    let paper = palette_bytes(content, &content.ui.colors.text);
    let ui = &content.ui;
    let banner = content.banners.id.to_uppercase();
    blit_centered(rgb, width, height, 0, width as i32, ui.layout.margin_y.round() as i32, &banner, paper, ui.layout.title_scale, layout);
    let button_w = ui.layout.button_width.round() as i32;
    let button_h = ui.layout.button_height.round() as i32;
    let gap = ui.layout.row_gap.round() as i32;
    let total = button_w * 2 + gap;
    let x = (width as i32 - total) / 2;
    let line_h = (layout.glyph_rows.max(1) * ui.layout.button_scale) as i32;
    let clear = layout.failure_clear.round() as i32;
    let mut y = height as i32 - button_h - ui.layout.margin_y.round() as i32;
    if !failure.is_empty() {
        let overflow = y + button_h + line_h + clear - height as i32;
        if overflow > 0 {
            y -= overflow;
        }
    }
    if !shortfall.is_empty() {
        let short_y = y - line_h - gap;
        if short_y < 0 {
            return Err("crate reveal shortfall does not fit above the buttons".into());
        }
        blit_centered(rgb, width, height, 0, width as i32, short_y, shortfall, paper, ui.layout.button_scale, layout);
    }
    if y < 0 {
        return Err("crate reveal buttons do not fit".into());
    }
    let mut hits = Vec::new();
    let continue_on = ready;
    let again_on = ready && again;
    let continue_hit = paint_text_button(rgb, width, height, content, x, y, button_w, button_h, &ui.copy.crates_continue, continue_on, continue_on && focus == 0);
    if continue_on {
        hits.push(crate::menu_still::Hit { id: "continue".into(), ..continue_hit });
    }
    let again_hit = paint_text_button(rgb, width, height, content, x + button_w + gap, y, button_w, button_h, &ui.copy.crates_pull_again, again_on, again_on && focus == hits.len());
    if again_on {
        hits.push(crate::menu_still::Hit { id: "pull_again".into(), ..again_hit });
    }
    if !failure.is_empty() {
        let failure_y = y + button_h;
        if failure_y < 0 || failure_y + line_h + clear > height as i32 {
            return Err("crate reveal failure line does not fit under the buttons".into());
        }
        let ink = palette_bytes(content, &content.crate_reveal.grid.failure_ink);
        blit_centered(rgb, width, height, 0, width as i32, failure_y, failure, ink, ui.layout.button_scale, layout);
    }
    Ok(hits)
}

fn paint_text_button(rgb: &mut [u8], width: u32, height: u32, content: &Content, x: i32, y: i32, w: i32, h: i32, label: &str, lit: bool, focused: bool) -> crate::menu_still::Hit {
    let edge = if focused {
        palette_bytes(content, &content.ui.colors.focus_before_hero)
    } else {
        palette_bytes(content, &content.ui.colors.dim)
    };
    let text = if lit { palette_bytes(content, &content.ui.colors.text) } else { palette_bytes(content, &content.ui.colors.dim) };
    let face = palette_bytes(content, &content.ui.colors.panel);
    draw_bevel(rgb, width, height, x, y, w, h, &chunk_bevel(content, h, content.ui.layout.button_height, edge, face));
    let layout = &content.crate_reveal.layout;
    let scale = content.ui.layout.button_scale;
    let text_h = layout.glyph_rows.max(1) * scale;
    let text_w = label.chars().count() as u32 * layout.title_glyph_px.max(1) * scale;
    let tx = title_origin(x.max(0) as u32, w.max(0) as u32, text_w, 0);
    let ty = (y + (h - text_h as i32) / 2).max(0) as u32;
    blit_at(rgb, width, height, tx, ty, label, text, scale);
    crate::menu_still::Hit { id: String::new(), x, y, w, h }
}

fn blit_centered(rgb: &mut [u8], width: u32, height: u32, origin_x: i32, span: i32, y: i32, text: &str, ink: [u8; 3], scale: u32, layout: &gemini_sim::content::RevealLayout) {
    let text_w = text.chars().count() as u32 * layout.title_glyph_px.max(1) * scale;
    let x = title_origin(origin_x.max(0) as u32, span.max(0) as u32, text_w, 0);
    blit_at(rgb, width, height, x, y.max(0) as u32, text, ink, scale);
}

struct ChunkBevel {
    radius: i32,
    outline: i32,
    depth: i32,
    edge: [u8; 3],
    light: [u8; 3],
    shadow: [u8; 3],
    fill: [u8; 3],
}

fn chunk_bevel(content: &Content, panel_h: i32, reference_h: f32, edge: [u8; 3], fill: [u8; 3]) -> ChunkBevel {
    let ui = &content.ui.layout;
    let scale = panel_h as f32 / reference_h.max(1.0);
    ChunkBevel {
        radius: (ui.panel_radius * scale).round().max(1.0) as i32,
        outline: (ui.pause_outline_px * scale).round().max(1.0) as i32,
        depth: (ui.bevel_px * scale).round().max(1.0) as i32,
        edge,
        light: palette_bytes(content, &content.ui.colors.bevel_light),
        shadow: palette_bytes(content, &content.ui.colors.bevel_shadow),
        fill,
    }
}

fn inner_face(content: &Content, x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
    let bevel = chunk_bevel(content, h, content.crate_reveal.layout.panel_height, [0, 0, 0], [0, 0, 0]);
    let band = bevel.outline.max(1) + bevel.depth.max(1);
    let face_w = (w - band * 2).max(1);
    let face_h = (h - band * 2).max(1);
    (x + band, y + band, face_w, face_h)
}

fn draw_bevel(rgb: &mut [u8], width: u32, height: u32, x: i32, y: i32, w: i32, h: i32, face: &ChunkBevel) {
    if w <= 0 || h <= 0 {
        return;
    }
    let outline = face.outline.max(1);
    let band = outline + face.depth.max(1);
    for row in 0..h {
        for col in 0..w {
            if !inside_round(col, row, w, h, face.radius) {
                continue;
            }
            let color = if !inside_round(col - outline, row - outline, w - outline * 2, h - outline * 2, face.radius - outline) {
                face.edge
            } else if !inside_round(col - band, row - band, w - band * 2, h - band * 2, face.radius - band) {
                if (col as f32) + (row as f32) < (w as f32 + h as f32) * 0.5 { face.light } else { face.shadow }
            } else {
                face.fill
            };
            put_at(rgb, width, height, x + col, y + row, color);
        }
    }
}

fn inside_round(col: i32, row: i32, w: i32, h: i32, radius: i32) -> bool {
    if w <= 0 || h <= 0 || col < 0 || row < 0 || col >= w || row >= h {
        return false;
    }
    let r = radius.max(0).min(w / 2).min(h / 2);
    let px = col as f32 + 0.5;
    let py = row as f32 + 0.5;
    let rf = r as f32;
    let cx = px.clamp(rf, w as f32 - rf);
    let cy = py.clamp(rf, h as f32 - rf);
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= rf * rf + 1e-3
}

fn put_at(rgb: &mut [u8], width: u32, height: u32, x: i32, y: i32, color: [u8; 3]) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }
    put(rgb, width, x as u32, y as u32, color);
}

fn face_ink(content: &Content, color: [u8; 3]) -> [u8; 3] {
    let layout = &content.crate_reveal.layout;
    let luma = (color[0] as f32 * 0.3 + color[1] as f32 * 0.59 + color[2] as f32 * 0.11) / 255.0;
    let name = if luma > layout.ink_luma { &layout.ink_dark } else { &layout.ink_light };
    palette_bytes(content, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gemini_sim::reveal::plan;
    use sha2::Digest;

    #[test]
    fn a_card_still_is_not_a_flat_color() {
        let content = Content::embedded();
        let planned = plan(&content, "legendary", None).unwrap();
        let rgb = paint_reveal(&content, &planned, planned.card_at, 320, 180);
        let stats = still_color_stats(&rgb);
        assert!(
            !reveal_still_is_flat(&rgb),
            "unique {} variance {:.1} dominant {:.3}",
            stats.unique, stats.variance, stats.dominant
        );
        let flat = vec![18u8; (320 * 180 * 3) as usize];
        assert!(reveal_still_is_flat(&flat));
    }

    #[test]
    fn every_rarity_word_fits_at_the_shared_title_size() {
        let content = Content::embedded();
        let layout = &content.crate_reveal.layout;
        let card = &content.crate_reveal.card;
        let scale = shared_title_scale(layout, card);
        let panel_w = layout.panel_width.round() as u32;
        let panel_h = layout.panel_height.round() as u32;
        let pad = still_title_pad(layout, card);
        let inner_w = panel_w - layout.border_px * 2 - pad * 2;
        let inner_h = panel_h - layout.border_px * 2 - pad * 2;
        let face_w = panel_w - layout.border_px * 2;
        let face_h = panel_h - layout.border_px * 2;
        for word in ["COMMON", "RARE", "EPIC", "LEGENDARY"] {
            let box_w = word.chars().count() as u32 * layout.title_glyph_px * scale;
            let box_h = layout.glyph_rows * scale;
            assert!(box_w <= inner_w, "{word} is {box_w}px wide, padding leaves {inner_w}");
            assert!(box_h <= inner_h, "{word} is {box_h}px tall, padding leaves {inner_h}");
            let origin = title_origin(0, face_w, box_w, card.title_pad.round() as u32);
            assert_eq!(origin, face_w.saturating_sub(box_w) / 2, "{word} stays centered");
        }
        assert_eq!(scale, title_scale(face_w, face_h, "LEGENDARY".chars().count() as u32, layout, card));
        let common = title_scale(face_w, face_h, "COMMON".chars().count() as u32, layout, card);
        assert!(common > scale, "a shorter word would be larger if each card sized itself");
    }

    #[test]
    fn card_title_pad_is_the_still_inset() {
        let content = Content::embedded();
        let layout = &content.crate_reveal.layout;
        let card = &content.crate_reveal.card;
        assert_eq!(still_title_pad(layout, card), layout.title_pad_px);
        let mut tight = card.clone();
        tight.title_pad = 0.0;
        assert_eq!(still_title_pad(layout, &tight), 0);
        let face_w = layout.panel_width.round() as u32 - layout.border_px * 2;
        let legend = "LEGENDARY".chars().count() as u32 * layout.title_glyph_px * shared_title_scale(layout, card);
        assert_eq!(
            title_origin(8, face_w, legend, card.title_pad.round() as u32),
            8 + face_w.saturating_sub(legend) / 2
        );
    }

    #[test]
    fn reveal_backdrop_follows_the_stage_palette_key() {
        let content = Content::embedded();
        let layout = &content.crate_reveal.layout;
        let mut rgb = vec![0u8; 64 * 64 * 3];
        paint_stage(&mut rgb, 64, 64, &content, layout);
        let stage = palette_bytes(&content, &layout.stage);
        let base = [
            stage[0] as f32 + layout.stage_base[0],
            stage[1] as f32 + layout.stage_base[1],
            stage[2] as f32 + layout.stage_base[2],
        ];
        assert_eq!(base, [16.0, 12.0, 22.0]);
        let cx = (64.0f32 - 1.0) * 0.5;
        let cy = (64.0f32 - 1.0) * layout.stage_y;
        let max_d = (cx * cx + 64.0 * 64.0).sqrt().max(1.0);
        let dx = 0.0 - cx;
        let dy = 0.0 - cy;
        let d = (dx * dx + dy * dy).sqrt() / max_d;
        let lift = (1.0 - d).clamp(0.0, 1.0).powf(layout.stage_lift_pow);
        let vignette = d.clamp(0.0, 1.0).powf(layout.stage_vignette_pow);
        let expect = [
            (base[0] + lift * layout.stage_lift[0] - vignette * layout.stage_vignette[0]).clamp(0.0, 255.0) as u8,
            (base[1] + lift * layout.stage_lift[1] - vignette * layout.stage_vignette[1]).clamp(0.0, 255.0) as u8,
            (base[2] + lift * layout.stage_lift[2] - vignette * layout.stage_vignette[2]).clamp(0.0, 255.0) as u8,
        ];
        assert_eq!(&rgb[0..3], &expect);
    }

    #[test]
    fn a_crack_frame_is_not_the_landed_drop() {
        let content = Content::embedded();
        let planned = plan(&content, "legendary", None).unwrap();
        let tier = content.crate_reveal.tier("legendary").unwrap();
        let landed = tier.drop[1];
        let crack_mid = (tier.crack[0] + tier.crack[1]) * 0.5;
        let drop_look = gemini_sim::reveal::look(&content, &planned, landed);
        let crack_look = gemini_sim::reveal::look(&content, &planned, crack_mid);
        assert_eq!(drop_look.crack, 0.0, "drop ends with the pod still closed");
        assert!((crack_look.crack - 0.5).abs() < 0.02, "mid crack is {}", crack_look.crack);
        let drop = paint_reveal(&content, &planned, landed, 320, 180);
        let crack = paint_reveal(&content, &planned, crack_mid, 320, 180);
        assert_ne!(drop, crack, "cracks must change the landed pod");
    }

    #[test]
    fn card_panel_size_follows_the_layout_keys() {
        let content = Content::embedded();
        let mut layout = content.crate_reveal.layout.clone();
        let (_, _, width, height) = card_panel(960, 540, &layout);
        assert_eq!(width, layout.panel_width.round() as u32);
        assert_eq!(height, layout.panel_height.round() as u32);
        layout.panel_width += 80.0;
        layout.panel_height += 40.0;
        let (_, _, wider, taller) = card_panel(960, 540, &layout);
        assert_eq!(wider, width + 80);
        assert_eq!(taller, height + 40);
    }

    #[test]
    fn ten_pull_backs_match_and_gold_stays_on_legendary_faces() {
        let content = Content::embedded();
        let width = content.ui.layout.fit_width.round() as u32;
        let height = content.ui.layout.fit_height.round() as u32;
        let pods = (content.crate_reveal.grid.columns * content.crate_reveal.grid.rows) as usize;
        let slots = grid_slots(&content, width, height).unwrap();
        let gold = palette_bytes(&content, "rarity_legendary");
        let common = palette_bytes(&content, "rarity_common");
        let back = palette_bytes(&content, &content.crate_reveal.grid.back);
        let backs = paint_grid(&content, &GridStill { items: &vec![None; pods], ready: false, again: true, focus: 0, shortfall: "", failure: "" }, width, height).unwrap().0;
        let first = rounded_card(&backs, width, slots[0], &content);
        let second = rounded_card(&backs, width, slots[1], &content);
        assert_eq!(first, second, "every back is the same picture");
        assert_eq!(face_swatch(&backs, width, slots[0], &content), back);
        assert!(!first.chunks_exact(3).any(|px| px == gold), "a back has no legendary gold");

        let paper = palette_bytes(&content, &content.crate_reveal.grid.face);
        let visor = content.crate_item("visor").expect("visor");
        let grip = content.crate_item("grip").expect("grip");
        let mark = content.crate_item("mark").expect("mark");
        let perk = content.crate_item("perk").expect("perk");
        assert_eq!(visor.rarity, grip.rarity);
        assert_ne!(visor.id, grip.id);
        let mut faces = vec![Some(visor.id.as_str()); pods];
        let legendary_at = pods / 2;
        faces[legendary_at] = Some(mark.id.as_str());
        let mixed = paint_grid(&content, &GridStill { items: &faces, ready: true, again: true, focus: 0, shortfall: "", failure: "" }, width, height).unwrap().0;
        assert_eq!(face_swatch(&mixed, width, slots[legendary_at], &content), paper, "the face is paper, not a rarity flood");
        assert_eq!(face_swatch(&mixed, width, slots[0], &content), paper);
        assert_ne!(paper, gold);
        assert_ne!(paper, common);
        for (index, slot) in slots.iter().enumerate() {
            if index == legendary_at {
                assert!(card_pixels(&mixed, width, *slot).chunks_exact(3).any(|px| px == gold));
            } else {
                assert!(
                    card_pixels(&mixed, width, *slot).chunks_exact(3).all(|px| px != gold),
                    "card {index} picked up legendary gold"
                );
            }
        }

        let mut cards = vec![gemini_sim::reveal::GridCard { rarity: "common".into(), fakeout: None, item: visor.id.clone() }; pods];
        cards[legendary_at].rarity = "legendary".into();
        cards[legendary_at].item = mark.id.clone();
        let mut run = gemini_sim::reveal::GridRun::start(&content, &cards).unwrap();
        let flip = content.crate_reveal.grid.fast_flip;
        for _ in 0..pods - 1 {
            run.advance(flip);
        }
        assert!(run.shown_face(legendary_at).is_none());
        let shown: Vec<Option<&str>> = (0..pods).map(|index| run.shown_item(index)).collect();
        assert_eq!(shown[legendary_at], None);
        assert_eq!(shown[0], Some(visor.id.as_str()));
        let held = paint_grid(&content, &GridStill { items: &shown, ready: false, again: true, focus: 0, shortfall: "", failure: "" }, width, height).unwrap().0;
        assert_eq!(card_pixels(&held, width, slots[legendary_at]), card_pixels(&backs, width, slots[legendary_at]));
        assert!(card_pixels(&held, width, slots[legendary_at]).chunks_exact(3).all(|px| px != gold));

        let (refund, refund_hits) = paint_refund(&content, &RefundStill { again: true, focus: 0, shortfall: "", failure: "" }, width, height).unwrap();
        assert!(refund_hits.iter().any(|hit| hit.id == "continue"));
        assert!(refund_hits.iter().any(|hit| hit.id == "pull_again"));
        assert!(refund.chunks_exact(3).all(|px| px != gold), "the refund panel has no rarity gold");
        let refund_paper = palette_bytes(&content, &content.crate_reveal.grid.refund_face);
        let refund_slot = refund_rect(&content, width, height);
        assert_eq!(face_swatch(&refund, width, refund_slot, &content), refund_paper);

        let failed_line = content.ui.copy.pull_failed.as_str();
        let lost_line = content.ui.copy.connection_lost.as_str();
        assert_ne!(failed_line, lost_line);
        let (failed_paint, failed_hits) = paint_grid(
            &content,
            &GridStill { items: &faces, ready: true, again: true, focus: 0, shortfall: "", failure: failed_line },
            width,
            height,
        )
        .unwrap();
        let (lost_paint, lost_hits) = paint_grid(
            &content,
            &GridStill { items: &faces, ready: true, again: true, focus: 0, shortfall: "", failure: lost_line },
            width,
            height,
        )
        .unwrap();
        assert!(failed_hits.iter().any(|hit| hit.id == "continue") && failed_hits.iter().any(|hit| hit.id == "pull_again"));
        assert!(lost_hits.iter().any(|hit| hit.id == "continue") && lost_hits.iter().any(|hit| hit.id == "pull_again"));
        let failure_ink = palette_bytes(&content, &content.crate_reveal.grid.failure_ink);
        assert_eq!(failure_ink, palette_bytes(&content, &content.crate_reveal.layout.ink_light));
        assert_ne!(failure_ink, palette_bytes(&content, &content.ui.colors.defeat));
        let clear = content.crate_reveal.layout.failure_clear.round() as u32;
        for (name, paint, hits) in [("pull failed", &failed_paint, &failed_hits), ("connection lost", &lost_paint, &lost_hits)] {
            let button_bottom = hits.iter().map(|hit| hit.y + hit.h).max().unwrap() as u32;
            let mut saw_failure = false;
            let mut lowest = 0u32;
            for (index, px) in paint.chunks_exact(3).enumerate() {
                if px != failure_ink.as_slice() {
                    continue;
                }
                let y = index as u32 / width;
                if y < button_bottom {
                    continue;
                }
                saw_failure = true;
                lowest = lowest.max(y);
            }
            assert!(saw_failure, "{name} draws the paper line under the buttons");
            assert!(lowest + 1 + clear <= height, "{name} line ends at {lowest} and the frame is {height}");
            for (index, slot) in slots.iter().enumerate() {
                assert_eq!(card_pixels(&mixed, width, *slot), card_pixels(paint, width, *slot), "{name} card {index} changed");
            }
        }
        assert_ne!(failed_paint, lost_paint, "the two failure lines are different pictures");

        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
        let flip = std::fs::read(root.join("assets/audio/crate_flip.ogg")).unwrap();
        assert_eq!(flip.len(), 4957);
        let digest = sha2::Sha256::digest(&flip);
        assert_eq!(
            format!("{digest:x}"),
            "50177d7e89c48e7df8438c67dc01aa8f2672d0f02e1df17ec020c837a4b907aa"
        );
        let failed_cue = std::fs::read(root.join("assets/audio/ui_pull_failed.ogg")).unwrap();
        assert_eq!(failed_cue.len(), 5195);
        let failed_digest = sha2::Sha256::digest(&failed_cue);
        assert_eq!(
            format!("{failed_digest:x}"),
            "e54d1f7224f22008eb60762a8943cbc2da735ebac79a91e5ac5d76db3dcfa24a"
        );
        let broke = crate::paint_menu(
            &content,
            &crate::MenuShot::Crates {
                focus: 0,
                rarity: None,
                failed: false,
                credits: Some(content.banners.cost.single as f32 / 2.0),
                reason: String::new(),
            },
            width,
            height,
        );
        assert!(broke.1.iter().all(|hit| hit.id != "pull" && hit.id != "ten"), "a short wallet sends no pull");
        assert!(broke.1.iter().any(|hit| hit.id == "back"));
        let grip_faces = vec![Some(grip.id.as_str()); pods];
        let grip_paint = paint_grid(&content, &GridStill { items: &grip_faces, ready: true, again: true, focus: 0, shortfall: "", failure: "" }, width, height).unwrap().0;
        let visor_ink = palette_bytes(&content, &visor.ink);
        let grip_ink = palette_bytes(&content, &grip.ink);
        assert!(card_pixels(&grip_paint, width, slots[0]).chunks_exact(3).any(|px| px == grip_ink), "the card draws the server item");
        assert!(card_pixels(&grip_paint, width, slots[0]).chunks_exact(3).all(|px| px != visor_ink), "a grip card does not draw visor");
        assert_ne!(grip_ink, gold);
        let flipped_items: Vec<Option<&str>> = cards.iter().map(|card| Some(card.item.as_str())).collect();
        let (flipped, flipped_hits) = paint_grid(&content, &GridStill { items: &flipped_items, ready: true, again: true, focus: 0, shortfall: "", failure: "" }, width, height).unwrap();
        assert!(flipped_hits.iter().any(|hit| hit.id == "continue") && flipped_hits.iter().any(|hit| hit.id == "pull_again"));
        let (held_paint, held_hits) = paint_grid(&content, &GridStill { items: &shown, ready: false, again: true, focus: 0, shortfall: "", failure: "" }, width, height).unwrap();
        assert!(held_hits.is_empty(), "buttons stay dead until the best face is up");
        assert_eq!(held_paint, held);
        let _ = perk;
        let stills = [
            ("all_backs.png", backs),
            ("all_common.png", paint_grid(&content, &GridStill { items: &vec![Some(visor.id.as_str()); pods], ready: true, again: true, focus: 0, shortfall: "", failure: "" }, width, height).unwrap().0),
            ("one_legendary.png", mixed),
            ("best_face_down.png", held),
            ("best_flipped.png", flipped),
            ("refund.png", refund),
            ("no_credits.png", broke.0),
            ("pull_failed.png", failed_paint),
            ("connection_lost.png", lost_paint),
        ];
        for (name, rgb) in stills {
            let bytes = std::fs::read(root.join("art/ten_pull").join(name)).unwrap();
            let image = image::load_from_memory(&bytes).unwrap().to_rgb8();
            assert_eq!(image.dimensions(), (width, height), "{name}");
            assert_eq!(image.as_raw(), &rgb, "{name} does not match the painter");
        }
    }

    fn rounded_card(rgb: &[u8], width: u32, slot: (u32, u32, u32, u32), content: &Content) -> Vec<u8> {
        let (x, y, w, h) = slot;
        let radius = chunk_bevel(content, h as i32, content.crate_reveal.layout.panel_height, [0, 0, 0], [0, 0, 0]).radius;
        let mut out = Vec::new();
        for row in 0..h {
            for col in 0..w {
                if !inside_round(col as i32, row as i32, w as i32, h as i32, radius) {
                    continue;
                }
                let px = x + col;
                let py = y + row;
                let i = ((py * width + px) * 3) as usize;
                out.extend_from_slice(&rgb[i..i + 3]);
            }
        }
        out
    }

    fn card_pixels(rgb: &[u8], width: u32, slot: (u32, u32, u32, u32)) -> Vec<u8> {
        let (x, y, w, h) = slot;
        let mut out = Vec::with_capacity((w * h * 3) as usize);
        for row in y..y + h {
            let start = ((row * width + x) * 3) as usize;
            out.extend_from_slice(&rgb[start..start + (w * 3) as usize]);
        }
        out
    }

    fn face_swatch(rgb: &[u8], width: u32, slot: (u32, u32, u32, u32), content: &Content) -> [u8; 3] {
        let (x, y, w, h) = slot;
        let face = inner_face(content, x as i32, y as i32, w as i32, h as i32);
        let px = (face.0 + 4).max(0) as u32;
        let py = (face.1 + face.3 / 2).max(0) as u32;
        let i = ((py * width + px) * 3) as usize;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    }

    fn refund_rect(content: &Content, width: u32, height: u32) -> (u32, u32, u32, u32) {
        let (x, y, panel_w, panel_h, _, _, _, _, _) = refund_column(content, width, height, false);
        (x.max(0) as u32, y.max(0) as u32, panel_w.max(0) as u32, panel_h.max(0) as u32)
    }
}
