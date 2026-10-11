//! Software frames of a crate reveal. Timing and colors come from the sim tables.

use gemini_sim::content::Content;
use gemini_sim::reveal::{look, RevealPlan};

pub fn paint_reveal(content: &Content, plan: &RevealPlan, t: f32, width: u32, height: u32) -> Vec<u8> {
    let width = width.max(8);
    let height = height.max(8);
    let view = look(content, plan, t);
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    let bg = [18u8, 14, 22];
    for px in rgb.chunks_exact_mut(3) {
        px.copy_from_slice(&bg);
    }
    let color = bytes(view.rgb);
    let ink = if luminance(view.rgb) > 0.62 {
        [28u8, 24, 32]
    } else {
        [242u8, 242, 242]
    };
    if view.card {
        let panel_w = width * 5 / 12;
        let panel_h = height * 4 / 10;
        let x = (width - panel_w) / 2;
        let y = (height - panel_h) / 2;
        fill(&mut rgb, width, height, x, y, panel_w, panel_h, color);
        let label = view.shown.to_uppercase();
        blit_centered(&mut rgb, width, height, &label, ink, 6);
    } else {
        let crate_w = width / 6;
        let crate_h = height / 5;
        let landed = (height / 2) as i32;
        let risen = landed - (height as i32 / 4);
        let dropped = risen + ((landed - risen) as f32 * view.drop) as i32;
        let y = (dropped + ((risen - dropped) as f32 * view.rise) as i32).max(8) as u32;
        let x = (width - crate_w) / 2;
        let gap = ((crate_w as f32) * 0.18 * view.crack) as u32;
        let half = crate_w.saturating_sub(gap) / 2;
        fill(&mut rgb, width, height, x, y.saturating_sub(crate_h / 2), half, crate_h, color);
        let right = x + half + gap;
        fill(&mut rgb, width, height, right, y.saturating_sub(crate_h / 2), half, crate_h, color);
        if view.burst > 0.05 {
            let wash = blend(color, [255, 248, 230], view.burst);
            let band = ((height as f32) * 0.08 * view.burst) as u32;
            fill(&mut rgb, width, height, 0, 0, width, band.max(2), wash);
            fill(&mut rgb, width, height, 0, height.saturating_sub(band.max(2)), width, band.max(2), wash);
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

fn blit_centered(rgb: &mut [u8], width: u32, height: u32, text: &str, ink: [u8; 3], scale: u32) {
    let scale = scale.max(1);
    let cols = text.chars().count() as u32 * 6 * scale;
    let rows = 7 * scale;
    let origin_x = width.saturating_sub(cols) / 2;
    let origin_y = height.saturating_sub(rows) / 2;
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
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        _ => [0; 7],
    }
}
