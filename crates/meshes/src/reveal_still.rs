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
    paint_stage(&mut rgb, width, height);
    let color = bytes(view.rgb);
    let ink = if luminance(view.rgb) > 0.62 {
        [28u8, 24, 32]
    } else {
        [242u8, 242, 242]
    };
    let light = [242u8, 242, 242];
    if view.card {
        paint_card(&mut rgb, width, height, color, ink, light, &view.shown);
    } else {
        paint_crate(&mut rgb, width, height, &view, color, light);
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
    rgb
}

fn paint_stage(rgb: &mut [u8], width: u32, height: u32) {
    let cx = (width as f32 - 1.0) * 0.5;
    let cy = (height as f32 - 1.0) * 0.42;
    let max_d = (cx * cx + (height as f32) * (height as f32)).sqrt().max(1.0);
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d = (dx * dx + dy * dy).sqrt() / max_d;
            let lift = (1.0 - d).clamp(0.0, 1.0).powf(1.35);
            let vignette = d.clamp(0.0, 1.0).powf(1.6);
            // Center lifts toward a cool slate; the corners fall to near-black.
            let r = (16.0 + lift * 28.0 - vignette * 10.0).clamp(0.0, 255.0) as u8;
            let g = (12.0 + lift * 22.0 - vignette * 8.0).clamp(0.0, 255.0) as u8;
            let b = (22.0 + lift * 36.0 - vignette * 12.0).clamp(0.0, 255.0) as u8;
            put(rgb, width, x, y, [r, g, b]);
        }
    }
}

fn paint_card(rgb: &mut [u8], width: u32, height: u32, color: [u8; 3], ink: [u8; 3], light: [u8; 3], shown: &str) {
    let (x, y, panel_w, panel_h) = card_panel(width, height);
    let cx = x + panel_w / 2;
    let cy = y + panel_h / 2;
    add_glow(rgb, width, height, cx, cy, panel_w.max(panel_h), color, 0.72);
    let frame = blend(color, light, 0.42);
    let shade = blend(color, [8, 6, 12], 0.55);
    fill(rgb, width, height, x, y, panel_w, panel_h, shade);
    let border = (panel_w / 18).clamp(4, 16);
    // Top rail is lighter than the bottom rail, so the frame reads as a beveled card.
    for row in 0..border {
        let t = row as f32 / border.max(1) as f32;
        let rail = blend(light, frame, 0.35 + t * 0.5);
        fill(rgb, width, height, x, y + row, panel_w, 1, rail);
        fill(rgb, width, height, x, y + panel_h - 1 - row, panel_w, 1, blend(shade, frame, t));
    }
    for col in 0..border {
        fill(rgb, width, height, x + col, y, 1, panel_h, blend(frame, shade, col as f32 / border.max(1) as f32));
        fill(rgb, width, height, x + panel_w - 1 - col, y, 1, panel_h, frame);
    }
    let face_x = x + border;
    let face_y = y + border;
    let face_w = panel_w.saturating_sub(border * 2);
    let face_h = panel_h.saturating_sub(border * 2);
    fill(rgb, width, height, face_x, face_y, face_w, face_h, color);
    // Sheen across the upper face. The middle of the face stays the rarity color.
    let sheen_h = (face_h / 5).max(2);
    for row in 0..sheen_h {
        let t = 0.28 * (1.0 - row as f32 / sheen_h as f32);
        let tone = blend(color, light, t);
        fill(rgb, width, height, face_x + 2, face_y + 2 + row, face_w.saturating_sub(4), 1, tone);
    }
    paint_gem(rgb, width, height, face_x, face_y, face_w, face_h, color, light, ink);
    let label = shown.to_uppercase();
    let scale = title_scale(face_w, face_h, label.chars().count() as u32);
    let text_w = label.chars().count() as u32 * 6 * scale;
    let text_h = 7 * scale;
    let text_x = face_x + face_w.saturating_sub(text_w) / 2;
    let text_y = face_y + face_h.saturating_sub(text_h + scale);
    blit_at(rgb, width, height, text_x + scale / 2, text_y + scale / 2, &label, blend(ink, [0, 0, 0], 0.65), scale);
    blit_at(rgb, width, height, text_x, text_y, &label, ink, scale);
    let rule_y = text_y.saturating_sub(scale.max(2));
    fill(rgb, width, height, face_x + face_w / 6, rule_y, face_w * 2 / 3, (scale / 3).max(1), blend(ink, color, 0.45));
    let hint_scale = (scale / 2).max(2);
    blit_centered_at(rgb, width, height, width / 2, (y + panel_h + hint_scale * 4).min(height.saturating_sub(8)), "ENTER", light, hint_scale);
}

fn paint_gem(rgb: &mut [u8], width: u32, height: u32, face_x: u32, face_y: u32, face_w: u32, face_h: u32, color: [u8; 3], light: [u8; 3], ink: [u8; 3]) {
    let gem_h = (face_h / 5).max(6);
    let gem_w = (face_w / 5).max(6);
    let cx = face_x + face_w / 2;
    let top = face_y + (face_h / 12).max(2);
    let stone = blend(color, light, 0.55);
    let edge = blend(ink, color, 0.35);
    for row in 0..gem_h {
        let t = if row < gem_h / 2 {
            row as f32 / (gem_h / 2).max(1) as f32
        } else {
            (gem_h - 1 - row) as f32 / (gem_h / 2).max(1) as f32
        };
        let half = ((gem_w as f32) * 0.5 * t.clamp(0.15, 1.0)) as u32;
        let y = top + row;
        fill(rgb, width, height, cx.saturating_sub(half), y, half.saturating_mul(2).max(1), 1, if row == 0 || row + 1 == gem_h { edge } else { stone });
        if half > 2 && row > 0 && row + 1 < gem_h {
            put(rgb, width, cx, y, light);
        }
    }
}

fn paint_crate(rgb: &mut [u8], width: u32, height: u32, view: &gemini_sim::reveal::RevealLook, color: [u8; 3], light: [u8; 3]) {
    let crate_w = width / 6;
    let crate_h = height / 5;
    let landed = (height / 2) as i32;
    let risen = landed - (height as i32 / 4);
    let dropped = risen + ((landed - risen) as f32 * view.drop) as i32;
    let y = (dropped + ((risen - dropped) as f32 * view.rise) as i32).max(8) as u32;
    let x = (width - crate_w) / 2;
    let gap = ((crate_w as f32) * 0.18 * view.crack) as u32;
    let half = crate_w.saturating_sub(gap) / 2;
    let top = y.saturating_sub(crate_h / 2);
    let cx = x + crate_w / 2;
    add_glow(rgb, width, height, cx, top + crate_h / 2, crate_w.saturating_mul(3).max(24), color, 0.8);
    // Soft contact shadow, wider than the crate.
    let shadow_w = crate_w.saturating_mul(2);
    let shadow_h = (crate_h / 5).max(3);
    let shadow_x = cx.saturating_sub(shadow_w / 2);
    let shadow_y = top + crate_h + shadow_h / 2;
    for row in 0..shadow_h {
        let t = 1.0 - (row as f32 / shadow_h as f32);
        let tone = blend([8, 6, 12], [20, 16, 28], t);
        let inset = ((1.0 - t) * (shadow_w as f32) * 0.25) as u32;
        fill(rgb, width, height, shadow_x + inset, shadow_y + row, shadow_w.saturating_sub(inset * 2), 1, tone);
    }
    let lip = blend(color, light, 0.4);
    let side = blend(color, [8, 6, 12], 0.4);
    for (ox, w) in [(x, half), (x + half + gap, half)] {
        fill(rgb, width, height, ox, top, w, crate_h, side);
        let inset = 2.min(w / 3);
        fill(rgb, width, height, ox + inset, top + inset, w.saturating_sub(inset * 2), crate_h.saturating_sub(inset + inset / 2), color);
        fill(rgb, width, height, ox + inset, top, w.saturating_sub(inset * 2), (crate_h / 6).max(2), lip);
    }
    blit_centered_at(rgb, width, height, width / 2, (top + crate_h + shadow_h * 3).min(height.saturating_sub(8)), "SKIP", light, (height / 48).max(2));
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

fn card_panel(width: u32, height: u32) -> (u32, u32, u32, u32) {
    let panel_w = (width * 46 / 100).max(32);
    let panel_h = (height * 64 / 100).max(32);
    let x = (width - panel_w) / 2;
    let y = (height - panel_h) / 2;
    (x, y, panel_w, panel_h)
}

fn title_scale(face_w: u32, face_h: u32, cols: u32) -> u32 {
    let cols = cols.max(1);
    let fit_w = face_w.saturating_sub(8) / (cols * 6);
    let fit_h = face_h / 16;
    fit_w.min(fit_h).clamp(2, 14)
}

fn add_glow(rgb: &mut [u8], width: u32, height: u32, cx: u32, cy: u32, radius: u32, color: [u8; 3], strength: f32) {
    let radius = radius.max(4);
    let r2 = (radius as f32) * (radius as f32);
    let x0 = cx.saturating_sub(radius);
    let y0 = cy.saturating_sub(radius);
    let x1 = (cx + radius).min(width);
    let y1 = (cy + radius).min(height);
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

fn blit_centered_at(rgb: &mut [u8], width: u32, height: u32, cx: u32, cy: u32, text: &str, ink: [u8; 3], scale: u32) {
    let scale = scale.max(1);
    let cols = text.chars().count() as u32 * 6 * scale;
    let rows = 7 * scale;
    let origin_x = cx.saturating_sub(cols / 2);
    let origin_y = cy.saturating_sub(rows / 2);
    blit_at(rgb, width, height, origin_x, origin_y, text, ink, scale);
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
    match ch {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gemini_sim::reveal::plan;

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
}
