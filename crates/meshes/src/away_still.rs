//! Software frames of the host-away banner, the solo pause, and the 3-2-1 countdown.

use gemini_sim::content::{hex_srgb, Content};

#[derive(Clone, Debug)]
pub struct AwayShot {
    pub banner: bool,
    pub title: String,
    pub show_timer: bool,
    pub timer_fill: f32,
    pub digit: Option<u8>,
    pub digit_u: f32,
    pub frozen: bool,
}

pub fn paint_away(content: &Content, shot: &AwayShot, width: u32, height: u32) -> Vec<u8> {
    let width = width.max(8);
    let height = height.max(8);
    let hud = &content.presentation.hud.away;
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    paint_bands(&mut rgb, width, height);
    if shot.frozen {
        let dim = hud.world_dim;
        let sat = hud.world_saturation;
        for px in rgb.chunks_exact_mut(3) {
            let graded = grade(px[0], px[1], px[2], dim, sat);
            px.copy_from_slice(&graded);
        }
    }
    if shot.banner {
        let slate = bytes(content, &hud.slate);
        let ink = bytes(content, &hud.text);
        let bar = bytes(content, &hud.bar);
        let panel_w = hud.panel_width.round() as i32;
        let panel_h = hud.panel_height.round() as i32;
        let panel_x = ((width as i32) - panel_w) / 2;
        let panel_y = hud.panel_top.round() as i32;
        fill_rect(&mut rgb, width, height, panel_x, panel_y, panel_w, panel_h, slate, hud.panel_opacity);
        let cell = (hud.title_px / 7.0).round().max(2.0) as i32;
        let text_w = measure(&shot.title, cell);
        let text_h = cell * 7;
        let bar_h = hud.bar_height.round() as i32;
        let gap = hud.row_gap.round() as i32;
        let stack = if shot.show_timer { text_h + gap + bar_h } else { text_h };
        let mut y = panel_y + (panel_h - stack) / 2;
        let text_x = panel_x + (panel_w - text_w) / 2;
        draw_text(&mut rgb, width, height, &shot.title, text_x, y, cell, ink, 1.0);
        if shot.show_timer {
            y += text_h + gap;
            let bar_w = hud.bar_width.round() as i32;
            let bar_x = panel_x + (panel_w - bar_w) / 2;
            let track = [((slate[0] as f32) * 0.45) as u8, ((slate[1] as f32) * 0.45) as u8, ((slate[2] as f32) * 0.45) as u8];
            fill_rect(&mut rgb, width, height, bar_x, y, bar_w, bar_h, track, 1.0);
            let fill = (bar_w as f32 * shot.timer_fill.clamp(0.0, 1.0)).round() as i32;
            fill_rect(&mut rgb, width, height, bar_x, y, fill, bar_h, bar, 1.0);
        }
    }
    if let Some(digit) = shot.digit {
        let ink = bytes(content, &hud.count);
        let u = shot.digit_u.clamp(0.0, 1.0);
        let scale = hud.count_scale_from + (hud.count_scale_to - hud.count_scale_from) * u;
        let cell = ((hud.count_px * scale) / 7.0).round().max(1.0) as i32;
        let label = digit.to_string();
        let text_w = measure(&label, cell);
        let text_h = cell * 7;
        let x = (width as i32 - text_w) / 2;
        let y = (height as i32 - text_h) / 2;
        draw_text(&mut rgb, width, height, &label, x, y, cell, ink, 1.0 - u);
    }
    rgb
}

fn paint_bands(rgb: &mut [u8], width: u32, height: u32) {
    let band = (width / 3).max(1);
    for y in 0..height {
        let shade = 170 + ((y * 80) / height.max(1)) as u8;
        for x in 0..width {
            let px = if x < band {
                [shade, 24, 28]
            } else if x < band * 2 {
                [24, shade, 36]
            } else {
                [28, 36, shade]
            };
            put(rgb, width, x, y, px);
        }
    }
}

fn grade(r: u8, g: u8, b: u8, dim: f32, sat: f32) -> [u8; 3] {
    let rf = r as f32;
    let gf = g as f32;
    let bf = b as f32;
    let luma = 0.2126 * rf + 0.7152 * gf + 0.0722 * bf;
    let sat = sat.clamp(0.0, 1.0);
    let keep = (1.0 - dim).clamp(0.05, 1.0);
    let mix = |channel: f32| ((luma + (channel - luma) * sat) * keep).clamp(0.0, 255.0) as u8;
    [mix(rf), mix(gf), mix(bf)]
}

fn bytes(content: &Content, name: &str) -> [u8; 3] {
    let hex = content.palette.hex(name).unwrap_or_else(|| panic!("unknown palette key {name}"));
    let c = hex_srgb(hex);
    [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8]
}

fn fill_rect(rgb: &mut [u8], width: u32, height: u32, x: i32, y: i32, w: i32, h: i32, color: [u8; 3], alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    for row in 0..h {
        for col in 0..w {
            let px = x + col;
            let py = y + row;
            if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                continue;
            }
            let i = ((py as u32 * width) + px as u32) as usize * 3;
            for channel in 0..3 {
                let dst = rgb[i + channel] as f32;
                let src = color[channel] as f32;
                rgb[i + channel] = (src * alpha + dst * (1.0 - alpha)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

fn put(rgb: &mut [u8], width: u32, x: u32, y: u32, color: [u8; 3]) {
    let i = ((y * width) + x) as usize * 3;
    rgb[i..i + 3].copy_from_slice(&color);
}

fn measure(text: &str, cell: i32) -> i32 {
    text.chars().map(|ch| if ch == ' ' { cell * 4 } else { cell * 6 }).sum()
}

fn draw_text(rgb: &mut [u8], width: u32, height: u32, text: &str, mut x: i32, y: i32, cell: i32, color: [u8; 3], alpha: f32) {
    for ch in text.chars() {
        if ch == ' ' {
            x += cell * 4;
            continue;
        }
        let Some(rows) = glyph(ch) else {
            x += cell * 6;
            continue;
        };
        for (ry, row) in rows.iter().enumerate() {
            for (rx, bit) in row.chars().enumerate() {
                if bit == '#' {
                    fill_rect(rgb, width, height, x + rx as i32 * cell, y + ry as i32 * cell, cell, cell, color, alpha);
                }
            }
        }
        x += cell * 6;
    }
}

fn glyph(ch: char) -> Option<[&'static str; 7]> {
    Some(match ch {
        'H' => ["#...#", "#...#", "#####", "#...#", "#...#", "#...#", "#...#"],
        'O' => [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
        'S' => [".####", "#....", "#....", ".###.", "....#", "....#", "####."],
        'T' => ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."],
        'A' => [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
        'W' => ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#"],
        'Y' => ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."],
        'P' => ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."],
        'U' => ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
        'D' => ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."],
        'E' => ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
        '1' => ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
        '2' => [".###.", "#...#", "....#", "..##.", ".#...", "#....", "#####"],
        '3' => [".###.", "#...#", "....#", "..##.", "....#", "#...#", ".###."],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reveal_still::reveal_still_is_flat;

    #[test]
    fn away_stills_are_not_flat() {
        let content = Content::embedded();
        let hud = &content.presentation.hud.away;
        let fill = 1.0 - content.coop.host_away.detect_after_seconds / content.coop.host_away.end_after_seconds;
        let banner = AwayShot {
            banner: true,
            title: hud.coop.text.clone(),
            show_timer: true,
            timer_fill: fill,
            digit: None,
            digit_u: 0.0,
            frozen: true,
        };
        let rgb = paint_away(&content, &banner, 960, 540);
        assert!(!reveal_still_is_flat(&rgb), "host away banner");
        let paused = AwayShot {
            banner: true,
            title: hud.solo.text.clone(),
            show_timer: false,
            timer_fill: 0.0,
            digit: None,
            digit_u: 0.0,
            frozen: true,
        };
        assert!(!reveal_still_is_flat(&paint_away(&content, &paused, 960, 540)));
        for digit in [3, 2, 1] {
            let shot = AwayShot {
                banner: false,
                title: String::new(),
                show_timer: false,
                timer_fill: 0.0,
                digit: Some(digit),
                digit_u: 0.0,
                frozen: true,
            };
            assert!(!reveal_still_is_flat(&paint_away(&content, &shot, 960, 540)), "{digit}");
        }
    }
}
