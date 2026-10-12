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
    /// 1 is the full away grade. 0 is the clear frame. Countdown stills use the eased weight.
    pub look: f32,
}

pub fn paint_away(content: &Content, shot: &AwayShot, width: u32, height: u32) -> Vec<u8> {
    let width = width.max(8);
    let height = height.max(8);
    let hud = &content.presentation.hud.away;
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    paint_arena(&mut rgb, width, height, content);
    let look = shot.look.clamp(0.0, 1.0);
    if look > 0.0 {
        let dim = hud.away_dim * look;
        let desat = hud.away_desaturation * look;
        let floor = hud.dim_floor;
        for px in rgb.chunks_exact_mut(3) {
            let graded = grade(px[0], px[1], px[2], dim, desat, floor);
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
            let pad = hud.panel_pad.round() as i32;
            let bar_w = panel_w - pad * 2;
            let bar_x = panel_x + pad;
            let track_mix = hud.bar_track.clamp(0.0, 1.0);
            let track = [
                (slate[0] as f32 * track_mix) as u8,
                (slate[1] as f32 * track_mix) as u8,
                (slate[2] as f32 * track_mix) as u8,
            ];
            fill_rect(&mut rgb, width, height, bar_x, y, bar_w, bar_h, track, 1.0);
            let fill = (bar_w as f32 * shot.timer_fill.clamp(0.0, 1.0)).round() as i32;
            fill_rect(&mut rgb, width, height, bar_x, y, fill, bar_h, bar, 1.0);
        }
    }
    if let Some(digit) = shot.digit {
        let ink = bytes(content, &hud.count);
        let outline = bytes(content, &hud.count_outline);
        let u = shot.digit_u.clamp(0.0, 1.0);
        let scale = hud.count_scale_from + (hud.count_scale_to - hud.count_scale_from) * u;
        let cell = ((hud.count_px * scale) / 7.0).round().max(1.0) as i32;
        let label = digit.to_string();
        let text_w = measure(&label, cell);
        let text_h = cell * 7;
        let x = (width as i32 - text_w) / 2;
        let y = (height as i32 - text_h) / 2;
        let alpha = 1.0 - u;
        let shadow = bytes(content, &hud.count_shadow);
        let marks: Vec<(i32, i32, f32)> = hud
            .count_shadow_samples
            .iter()
            .map(|sample| {
                let dx = (hud.count_shadow_px + sample.ox * hud.count_shadow_soft_px).round() as i32;
                let dy = (hud.count_shadow_px + sample.oy * hud.count_shadow_soft_px).round() as i32;
                (dx, dy, sample.weight)
            })
            .collect();
        for (dx, dy, weight) in marks {
            draw_text(
                &mut rgb,
                width,
                height,
                &label,
                x + dx,
                y + dy,
                cell,
                shadow,
                hud.count_shadow_alpha * weight * alpha,
            );
        }
        let px = hud.count_outline_px.round().max(1.0) as i32;
        for oy in -1..=1 {
            for ox in -1..=1 {
                if ox == 0 && oy == 0 {
                    continue;
                }
                draw_text(&mut rgb, width, height, &label, x + ox * px, y + oy * px, cell, outline, alpha);
            }
        }
        draw_text(&mut rgb, width, height, &label, x, y, cell, ink, alpha);
    }
    rgb
}

/// Arena floor plus swarmer, spitter, and brute. The bug raster's grey field is transparent.
pub(crate) fn paint_arena(rgb: &mut [u8], width: u32, height: u32, content: &Content) {
    let paint = &content.arena.paint;
    let wash = &content.arena.edge_wash;
    let dark = bytes(content, &paint.floor_dark);
    let light = bytes(content, &paint.floor_light);
    let sky = scaled_palette(content, &wash.color, wash.num, wash.den);
    let horizon = wash.horizon;
    for y in 0..height {
        let floor = y as f32 > height as f32 * horizon;
        for x in 0..width {
            let px = if floor {
                let band = ((x / paint.band_x) + (y / paint.band_y)) % 2 == 0;
                if band { light } else { blend(light, dark, paint.shade) }
            } else {
                let t = y as f32 / (height as f32 * horizon);
                blend(dark, sky, wash.mix + (1.0 - t) * wash.span)
            };
            put(rgb, width, x, y, px);
        }
    }
    blit_bug(rgb, width, height, "swarmer", width / 10, height / 2, width / 4, height / 2);
    blit_bug(rgb, width, height, "spitter", width * 4 / 10, height * 4 / 10, width / 4, height * 5 / 10);
    blit_bug(rgb, width, height, "brute", width * 6 / 10, height * 3 / 10, width * 3 / 10, height * 6 / 10);
}

fn blit_bug(rgb: &mut [u8], width: u32, height: u32, id: &str, origin_x: u32, origin_y: u32, bug_w: u32, bug_h: u32) {
    let bug_w = bug_w.max(8);
    let bug_h = bug_h.max(8);
    let src = crate::raster_bug(id, bug_w, bug_h);
    for y in 0..bug_h {
        for x in 0..bug_w {
            let i = ((y * bug_w + x) * 3) as usize;
            let px = [src[i], src[i + 1], src[i + 2]];
            if px == [0x8A, 0x8A, 0x8A] {
                continue;
            }
            let dx = origin_x + x;
            let dy = origin_y + y;
            if dx < width && dy < height {
                put(rgb, width, dx, dy, px);
            }
        }
    }
}

fn grade(r: u8, g: u8, b: u8, dim: f32, desat: f32, floor: f32) -> [u8; 3] {
    let rf = r as f32;
    let gf = g as f32;
    let bf = b as f32;
    let luma = 0.2126 * rf + 0.7152 * gf + 0.0722 * bf;
    let sat = (1.0 - desat).clamp(0.0, 1.0);
    let keep = (1.0 - dim).clamp(floor, 1.0);
    let mix = |channel: f32| ((luma + (channel - luma) * sat) * keep).round().clamp(0.0, 255.0) as u8;
    [mix(rf), mix(gf), mix(bf)]
}

fn blend(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 * (1.0 - t) + b[0] as f32 * t) as u8,
        (a[1] as f32 * (1.0 - t) + b[1] as f32 * t) as u8,
        (a[2] as f32 * (1.0 - t) + b[2] as f32 * t) as u8,
    ]
}

fn bytes(content: &Content, name: &str) -> [u8; 3] {
    let hex = content.palette.hex(name).unwrap_or_else(|| panic!("unknown palette key {name}"));
    let c = hex_srgb(hex);
    [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8]
}

fn scaled_palette(content: &Content, name: &str, num: [u32; 3], den: [u32; 3]) -> [u8; 3] {
    let src = bytes(content, name);
    [
        (src[0] as u32 * num[0] / den[0].max(1)) as u8,
        (src[1] as u32 * num[1] / den[1].max(1)) as u8,
        (src[2] as u32 * num[2] / den[2].max(1)) as u8,
    ]
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
            look: 1.0,
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
            look: 1.0,
        };
        assert!(!reveal_still_is_flat(&paint_away(&content, &paused, 960, 540)));
        let full = content.coop.host_away.resume_countdown_seconds.max(1.0);
        for digit in [3, 2, 1] {
            let shot = AwayShot {
                banner: false,
                title: String::new(),
                show_timer: false,
                timer_fill: 0.0,
                digit: Some(digit),
                digit_u: 0.0,
                look: (digit as f32 / full).clamp(0.0, 1.0),
            };
            assert!(!reveal_still_is_flat(&paint_away(&content, &shot, 960, 540)), "{digit}");
        }
    }

    #[test]
    fn away_grade_keeps_a_faint_glow() {
        let content = Content::embedded();
        let hud = &content.presentation.hud.away;
        assert!((hud.away_desaturation - 0.6).abs() < 1e-4);
        assert!((hud.away_dim - 0.3).abs() < 1e-4);
        assert!((hud.count_outline_px - 4.0).abs() < 1e-4);
        assert!(hud.count_shadow_px > hud.count_outline_px);
        let glow = grade(0x7C, 0xFF, 0x3A, hud.away_dim, hud.away_desaturation, hud.dim_floor);
        assert!(glow[1] > glow[0] + 20, "green glow stays green {glow:?}");
        let gray = grade(0x7C, 0xFF, 0x3A, hud.away_dim, 1.0, hud.dim_floor);
        assert!(glow[1] > gray[1], "60% desat is greener than grayscale");
        let clear = grade(0x7C, 0xFF, 0x3A, 0.0, 0.0, hud.dim_floor);
        assert_eq!(clear, [0x7C, 0xFF, 0x3A]);
    }

    #[test]
    fn arena_edge_wash_keeps_the_current_sky() {
        let content = Content::embedded();
        let wash = &content.arena.edge_wash;
        let sky = scaled_palette(&content, &wash.color, wash.num, wash.den);
        assert_eq!(sky, [8, 10, 16]);
        let mut rgb = vec![0u8; 32 * 32 * 3];
        paint_arena(&mut rgb, 32, 32, &content);
        let dark = bytes(&content, &content.arena.paint.floor_dark);
        let t = 0.0 / (32.0 * wash.horizon);
        let expect = blend(dark, sky, wash.mix + (1.0 - t) * wash.span);
        assert_eq!(&rgb[0..3], &expect);
    }
}
