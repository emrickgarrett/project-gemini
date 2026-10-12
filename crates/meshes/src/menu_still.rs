//! Software frames of the menu screens. Layout, type size, and colors come from `ui.toml`.
//! The pixel font is the crate-reveal glyph set.

use gemini_sim::content::{format_fixed, Content};

use crate::away_still::paint_arena;
use crate::glyph::{blit, measure};
use crate::preview::raster_parts;
use crate::reveal_still::paint_stage;

#[derive(Clone, Debug)]
pub struct Hit {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Debug)]
pub struct PaintPrefs {
    pub sensitivity: f32,
    pub fov: f32,
    pub invert_y: bool,
    pub view_bob: bool,
    pub master: f32,
    pub music: f32,
    pub effects: f32,
}

impl PaintPrefs {
    pub fn defaults(content: &Content) -> Self {
        let settings = &content.ui.settings;
        Self {
            sensitivity: settings.sensitivity,
            fov: settings.fov,
            invert_y: settings.invert_y,
            view_bob: settings.view_bob,
            master: settings.master,
            music: settings.music,
            effects: settings.effects,
        }
    }
}

#[derive(Clone, Debug)]
pub enum MenuShot {
    Title { focus: usize, notice: bool, native_quit: bool },
    Heroes { focus: usize },
    Difficulty { focus: usize, hero_id: String, notice: bool },
    Pause { focus: usize, hero_id: String },
    Settings { focus: usize, hero_id: String, prefs: PaintPrefs, notice: bool },
    Results {
        focus: usize,
        victory: bool,
        waves: u32,
        kills: u32,
        seconds: f32,
        currency: f32,
    },
    Crates { focus: usize, rarity: Option<String>, failed: bool, can_pull: bool },
    ClickToPlay,
}

/// One scale for a frame. Layout numbers in `ui.toml` are the 720p sizes.
#[derive(Clone, Copy)]
struct Frame {
    width: u32,
    height: u32,
    scale: f32,
}

impl Frame {
    fn new(content: &Content, width: u32, height: u32) -> Self {
        let base = content.ui.layout.fit_height.max(1.0);
        Self { width, height, scale: height as f32 / base }
    }

    fn px(self, value: f32) -> i32 {
        (value * self.scale).round() as i32
    }

    fn text(self, value: u32) -> u32 {
        ((value as f32) * self.scale).round().max(1.0) as u32
    }
}

pub fn paint_menu(content: &Content, shot: &MenuShot, width: u32, height: u32) -> (Vec<u8>, Vec<Hit>) {
    let frame = Frame::new(content, width.max(8), height.max(8));
    let mut rgb = vec![0u8; (frame.width * frame.height * 3) as usize];
    match shot {
        MenuShot::Pause { .. } | MenuShot::ClickToPlay => {
            paint_arena(&mut rgb, frame.width, frame.height, content);
            let keep = content.ui.layout.pause_keep.clamp(0.0, 1.0);
            for px in rgb.chunks_exact_mut(3) {
                for channel in px.iter_mut() {
                    *channel = (*channel as f32 * keep).round() as u8;
                }
            }
        }
        _ => paint_stage(&mut rgb, frame.width, frame.height, content, &content.crate_reveal.layout),
    }
    let hits = match shot {
        MenuShot::Title { focus, notice, native_quit } => paint_title(&mut rgb, content, frame, *focus, *notice, *native_quit),
        MenuShot::Heroes { focus } => paint_heroes(&mut rgb, content, frame, *focus),
        MenuShot::Difficulty { focus, hero_id, notice } => paint_difficulty(&mut rgb, content, frame, *focus, hero_id, *notice),
        MenuShot::Pause { focus, hero_id } => paint_pause(&mut rgb, content, frame, *focus, hero_id),
        MenuShot::Settings { focus, hero_id, prefs, notice } => paint_settings(&mut rgb, content, frame, *focus, hero_id, prefs, *notice),
        MenuShot::Results { focus, victory, waves, kills, seconds, currency } => {
            paint_results(&mut rgb, content, frame, *focus, *victory, *waves, *kills, *seconds, *currency)
        }
        MenuShot::Crates { focus, rarity, failed, can_pull } => {
            paint_crates(&mut rgb, content, frame, *focus, rarity.as_deref(), *failed, *can_pull)
        }
        MenuShot::ClickToPlay => paint_click(&mut rgb, content, frame),
    };
    (rgb, hits)
}

fn paint_title(rgb: &mut [u8], content: &Content, frame: Frame, focus: usize, notice: bool, native_quit: bool) -> Vec<Hit> {
    let copy = &content.ui.copy;
    let mut labels = vec![
        ("play", copy.play.as_str()),
        ("crates", copy.crates.as_str()),
        ("settings", copy.settings.as_str()),
    ];
    if native_quit {
        labels.push(("quit", copy.quit.as_str()));
    }
    let hits = column(rgb, content, frame, &copy.title, &labels, focus, None, true);
    if notice {
        let glyph = content.crate_reveal.layout.title_glyph_px;
        let scale = frame.text(content.ui.layout.hint_scale);
        let text = copy.reset_notice.as_str();
        let tw = measure(text, scale, glyph) as i32;
        let x = (frame.width as i32 - tw) / 2;
        let y = frame.px(content.ui.layout.margin_y);
        blit(rgb, frame.width, frame.height, x, y, text, ink(content, &content.ui.colors.defeat), scale, glyph);
    }
    hits
}

fn paint_heroes(rgb: &mut [u8], content: &Content, frame: Frame, focus: usize) -> Vec<Hit> {
    let ui = &content.ui;
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let rows = content.crate_reveal.layout.glyph_rows;
    let title = ui.copy.heroes.to_uppercase();
    draw_title(rgb, content, frame, &title);
    let n = content.heroes.len().max(1) as i32;
    let card_w = frame.px(ui.layout.card_width);
    let card_h = frame.px(ui.layout.card_height);
    let gap = frame.px(ui.layout.card_gap);
    let total = n * card_w + (n - 1) * gap;
    let mut x = (frame.width as i32 - total) / 2;
    let y = frame.px(ui.layout.margin_y) + (frame.text(ui.layout.title_scale) * rows) as i32 + frame.px(ui.layout.title_gap);
    let mut hits = Vec::new();
    let locked_ink = palette(content, &ui.colors.locked_text);
    for (index, hero) in content.heroes.iter().enumerate() {
        let id = format!("hero:{}", hero.id);
        let focused = index == focus;
        let base = if hero.owned {
            palette(content, &hero.accent_key)
        } else {
            gray(palette(content, &hero.accent_key))
        };
        let border = if focused { lift(base, palette(content, &ui.colors.text), ui.layout.focus_lift) } else { base };
        rect(rgb, frame.width, frame.height, x, y, card_w, card_h, border);
        let inset = frame.px(ui.layout.border_px).max(ui.layout.inset_min.round() as i32);
        rect(rgb, frame.width, frame.height, x + inset, y + inset, card_w - inset * 2, card_h - inset * 2, palette(content, &ui.colors.panel));
        let pad = frame.px(ui.layout.card_pad);
        paint_portrait(rgb, content, frame, hero, x, y, card_w, card_h, pad);
        let name_color = if hero.owned { palette(content, &hero.accent_key) } else { locked_ink };
        let mut ty = y + card_h / 2;
        let inner = (card_w - pad * 2).max(1) as u32;
        let name_scale = frame.text(ui.layout.button_scale);
        let hint_scale = frame.text(ui.layout.hint_scale);
        if !hero.owned {
            let lock = ui.copy.locked.as_str();
            let tw = measure(lock, hint_scale, glyph) as i32;
            blit(rgb, frame.width, frame.height, x + (card_w - tw) / 2, ty, lock, palette(content, &ui.colors.locked), hint_scale, glyph);
            ty += (hint_scale * rows) as i32 + frame.px(ui.layout.spacing);
        }
        ty = draw_wrapped(rgb, content, frame, &hero.name.to_uppercase(), x + pad, ty, inner, name_scale, name_color);
        let paper = palette(content, &ui.colors.text);
        let body = if hero.owned { paper } else { locked_ink };
        ty = draw_wrapped(rgb, content, frame, &hero.role.to_uppercase(), x + pad, ty + frame.px(ui.layout.spacing), inner, hint_scale, body);
        ty = draw_wrapped(rgb, content, frame, &hero.ability.to_uppercase(), x + pad, ty + frame.px(ui.layout.spacing_role), inner, hint_scale, body);
        let stats = format!(
            "{} {}  {} {}",
            ui.copy.hp,
            format_fixed(hero.max_hp, 0),
            ui.copy.speed,
            format_fixed(hero.move_speed_mult, ui.layout.stat_decimals)
        );
        draw_wrapped(rgb, content, frame, &stats, x + pad, ty + frame.px(ui.layout.spacing_block), inner, hint_scale, body);
        hits.push(Hit { id, x, y, w: card_w, h: card_h });
        x += card_w + gap;
    }
    let by = y + card_h + frame.px(ui.layout.row_gap);
    hits.push(Hit { id: "back".into(), ..button(rgb, content, frame, by, &ui.copy.back, focus == content.heroes.len(), None) });
    hits
}

fn paint_difficulty(rgb: &mut [u8], content: &Content, frame: Frame, focus: usize, hero_id: &str, notice: bool) -> Vec<Hit> {
    let ui = &content.ui;
    draw_title(rgb, content, frame, &ui.copy.difficulty);
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let rows = content.crate_reveal.layout.glyph_rows;
    let mut y = frame.px(ui.layout.margin_y) + (frame.text(ui.layout.title_scale) * rows) as i32 + frame.px(ui.layout.title_gap);
    let mut hits = Vec::new();
    let hero = if hero_id.is_empty() { None } else { Some(hero_id) };
    for (index, tier) in content.difficulty.iter().enumerate() {
        let label = tier.id.to_uppercase();
        let stats = format!(
            "{} X{}  {} X{}",
            ui.copy.hp,
            format_fixed(tier.enemy_hp, ui.layout.mult_decimals),
            ui.copy.dmg,
            format_fixed(tier.enemy_dmg, ui.layout.mult_decimals)
        );
        let hit = two_line(rgb, content, frame, y, &label, &stats, index == focus, hero);
        y = hit.y + hit.h + frame.px(ui.layout.row_gap);
        hits.push(Hit { id: format!("tier:{}", tier.id), ..hit });
    }
    let host = button(rgb, content, frame, y, &ui.copy.host, focus == hits.len(), hero);
    y = host.y + host.h + frame.px(ui.layout.row_gap);
    hits.push(Hit { id: "host".into(), ..host });
    let join = button(rgb, content, frame, y, &ui.copy.join, focus == hits.len(), hero);
    y = join.y + join.h + frame.px(ui.layout.row_gap);
    hits.push(Hit { id: "join".into(), ..join });
    hits.push(Hit { id: "back".into(), ..button(rgb, content, frame, y, &ui.copy.back, focus == hits.len(), hero) });
    if notice {
        let scale = frame.text(ui.layout.hint_scale);
        let text = ui.copy.coming_soon.as_str();
        let tw = measure(text, scale, glyph) as i32;
        let ny = y + frame.px(ui.layout.button_height) + frame.px(ui.layout.row_gap);
        blit(rgb, frame.width, frame.height, (frame.width as i32 - tw) / 2, ny, text, ink(content, &ui.colors.text), scale, glyph);
    }
    hits
}

fn paint_pause(rgb: &mut [u8], content: &Content, frame: Frame, focus: usize, hero_id: &str) -> Vec<Hit> {
    let ui = &content.ui;
    let pw = frame.px(ui.layout.pause_panel_width);
    let ph = frame.px(ui.layout.pause_panel_height);
    let x = (frame.width as i32 - pw) / 2;
    let y = (frame.height as i32 - ph) / 2;
    let hero = if hero_id.is_empty() { None } else { Some(hero_id) };
    rect(rgb, frame.width, frame.height, x, y, pw, ph, accent(content, hero));
    let inset = frame.px(ui.layout.border_px).max(ui.layout.inset_min.round() as i32);
    rect(rgb, frame.width, frame.height, x + inset, y + inset, pw - inset * 2, ph - inset * 2, palette(content, &ui.colors.panel));
    let labels = [("resume", ui.copy.resume.as_str()), ("settings", ui.copy.settings.as_str()), ("quit_to_menu", ui.copy.quit_to_menu.as_str())];
    let button_h = frame.px(ui.layout.button_height);
    let gap = frame.px(ui.layout.row_gap);
    let stack = labels.len() as i32 * button_h + (labels.len() as i32 - 1) * gap;
    let mut by = y + (ph - stack) / 2;
    let mut hits = Vec::new();
    for (index, (id, label)) in labels.iter().enumerate() {
        let hit = button_at(rgb, content, frame, x + (pw - frame.px(ui.layout.button_width)) / 2, by, label, index == focus, hero);
        by = hit.y + hit.h + gap;
        hits.push(Hit { id: (*id).into(), ..hit });
    }
    hits
}

fn paint_settings(rgb: &mut [u8], content: &Content, frame: Frame, focus: usize, hero_id: &str, prefs: &PaintPrefs, notice: bool) -> Vec<Hit> {
    let ui = &content.ui;
    draw_title(rgb, content, frame, &ui.copy.settings);
    let rows_n = content.crate_reveal.layout.glyph_rows;
    let mut y = frame.px(ui.layout.margin_y) + (frame.text(ui.layout.title_scale) * rows_n) as i32 + frame.px(ui.layout.title_gap);
    let hero = if hero_id.is_empty() { None } else { Some(hero_id) };
    let settings = &ui.settings;
    let rows = [
        ("slider:sensitivity", ui.copy.sensitivity.as_str(), format_fixed(prefs.sensitivity, 2), norm(prefs.sensitivity, settings.sensitivity_min, settings.sensitivity_max)),
        ("slider:fov", ui.copy.fov.as_str(), format_fixed(prefs.fov, 0), norm(prefs.fov, settings.fov_min, settings.fov_max)),
        ("toggle:bob", ui.copy.view_bob.as_str(), on_off(content, prefs.view_bob), if prefs.view_bob { 1.0 } else { 0.0 }),
        ("slider:master", ui.copy.master.as_str(), format_fixed(prefs.master, 2), norm(prefs.master, settings.volume_min, settings.volume_max)),
        ("slider:music", ui.copy.music.as_str(), format_fixed(prefs.music, 2), norm(prefs.music, settings.volume_min, settings.volume_max)),
        ("slider:effects", ui.copy.effects.as_str(), format_fixed(prefs.effects, 2), norm(prefs.effects, settings.volume_min, settings.volume_max)),
        ("toggle:invert", ui.copy.invert_y.as_str(), on_off(content, prefs.invert_y), if prefs.invert_y { 1.0 } else { 0.0 }),
    ];
    let mut hits = Vec::new();
    let mut steppers = Vec::new();
    for (index, (id, label, value, fill)) in rows.iter().enumerate() {
        let mut parts = slider_row(rgb, content, frame, y, label, value, *fill, index == focus, hero, id.starts_with("slider:"));
        let main = parts.remove(0);
        y = main.y + main.h + frame.px(ui.layout.spacing_block);
        hits.push(Hit { id: (*id).into(), ..main });
        for (suffix, part) in [":-", ":+"].into_iter().zip(parts) {
            steppers.push(Hit { id: format!("{id}{suffix}"), ..part });
        }
    }
    hits.push(Hit { id: "back".into(), ..button(rgb, content, frame, y, &ui.copy.back, focus == hits.len(), hero) });
    hits.extend(steppers);
    if notice {
        let glyph = content.crate_reveal.layout.title_glyph_px;
        let scale = frame.text(ui.layout.hint_scale);
        let text = ui.copy.reset_notice.as_str();
        let tw = measure(text, scale, glyph) as i32;
        let ny = y + frame.px(ui.layout.button_height) + frame.px(ui.layout.row_gap);
        blit(rgb, frame.width, frame.height, (frame.width as i32 - tw) / 2, ny, text, ink(content, &ui.colors.defeat), scale, glyph);
    }
    hits
}

fn paint_results(rgb: &mut [u8], content: &Content, frame: Frame, focus: usize, victory: bool, waves: u32, kills: u32, seconds: f32, currency: f32) -> Vec<Hit> {
    let ui = &content.ui;
    let title = if victory { ui.copy.victory.as_str() } else { ui.copy.defeat.as_str() };
    let color = if victory { &ui.colors.victory } else { &ui.colors.defeat };
    draw_title_colored(rgb, content, frame, title, palette(content, color));
    let rows = content.crate_reveal.layout.glyph_rows;
    let scale = frame.text(ui.layout.button_scale);
    let mut y = frame.px(ui.layout.margin_y) + (frame.text(ui.layout.title_scale) * rows) as i32 + frame.px(ui.layout.title_gap);
    let mins = (seconds.max(0.0) as u32) / 60;
    let secs = (seconds.max(0.0) as u32) % 60;
    let lines = [
        format!("{} {}", ui.copy.waves, waves),
        format!("{} {}", ui.copy.kills, kills),
        format!("{} {}:{:02}", ui.copy.time, mins, secs),
        format!("{} {}", ui.copy.currency, format_fixed(currency, ui.layout.currency_decimals)),
    ];
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let paper = palette(content, &ui.colors.text);
    for line in &lines {
        let tw = measure(line, scale, glyph) as i32;
        blit(rgb, frame.width, frame.height, (frame.width as i32 - tw) / 2, y, line, paper, scale, glyph);
        y += (scale * rows) as i32 + frame.px(ui.layout.spacing_block);
    }
    y += frame.px(ui.layout.row_gap);
    let mut hits = Vec::new();
    let again = button(rgb, content, frame, y, &ui.copy.play_again, focus == 0, None);
    y = again.y + again.h + frame.px(ui.layout.row_gap);
    hits.push(Hit { id: "play_again".into(), ..again });
    hits.push(Hit { id: "main_menu".into(), ..button(rgb, content, frame, y, &ui.copy.main_menu, focus == 1, None) });
    hits
}

fn paint_crates(rgb: &mut [u8], content: &Content, frame: Frame, focus: usize, rarity: Option<&str>, failed: bool, can_pull: bool) -> Vec<Hit> {
    let ui = &content.ui;
    draw_title(rgb, content, frame, &ui.copy.crates_title);
    let rows = content.crate_reveal.layout.glyph_rows;
    let scale = frame.text(ui.layout.button_scale);
    let mut y = frame.px(ui.layout.margin_y) + (frame.text(ui.layout.title_scale) * rows) as i32 + frame.px(ui.layout.title_gap);
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let paper = palette(content, &ui.colors.text);
    let line = if let Some(rarity) = rarity {
        rarity.to_uppercase()
    } else if failed {
        ui.copy.pull_failed.clone()
    } else {
        ui.copy.crates_empty.clone()
    };
    let tw = measure(&line, scale, glyph) as i32;
    blit(rgb, frame.width, frame.height, (frame.width as i32 - tw) / 2, y, &line, paper, scale, glyph);
    y += (scale * rows) as i32 + frame.px(ui.layout.title_gap);
    let mut hits = Vec::new();
    let mut index = 0;
    if can_pull {
        let hit = button(rgb, content, frame, y, &ui.copy.crates_pull, focus == index, None);
        y = hit.y + hit.h + frame.px(ui.layout.row_gap);
        hits.push(Hit { id: "pull".into(), ..hit });
        index += 1;
    }
    if rarity.is_some() {
        let hit = button(rgb, content, frame, y, &ui.copy.crates_open, focus == index, None);
        y = hit.y + hit.h + frame.px(ui.layout.row_gap);
        hits.push(Hit { id: "open".into(), ..hit });
        index += 1;
    }
    let _ = index;
    hits.push(Hit { id: "back".into(), ..button(rgb, content, frame, y, &ui.copy.back, focus == hits.len(), None) });
    hits
}

fn paint_click(rgb: &mut [u8], content: &Content, frame: Frame) -> Vec<Hit> {
    let y = (frame.height as i32 - frame.px(content.ui.layout.button_height)) / 2;
    vec![Hit { id: "click".into(), ..button(rgb, content, frame, y, &content.ui.copy.click_to_play, true, None) }]
}

fn column(rgb: &mut [u8], content: &Content, frame: Frame, title: &str, labels: &[(&str, &str)], focus: usize, hero: Option<&str>, center: bool) -> Vec<Hit> {
    let ui = &content.ui;
    let rows = content.crate_reveal.layout.glyph_rows;
    let title_h = (frame.text(ui.layout.title_scale) * rows) as i32;
    let count = labels.len() as i32;
    let button_h = frame.px(ui.layout.button_height);
    let gap = frame.px(ui.layout.row_gap);
    let stack = title_h + frame.px(ui.layout.title_gap) + count * button_h + (count - 1).max(0) * gap;
    let mut y = if center { (frame.height as i32 - stack) / 2 } else { frame.px(ui.layout.margin_y) };
    draw_title_at(rgb, content, frame, title, y, palette(content, &ui.colors.text));
    y += title_h + frame.px(ui.layout.title_gap);
    let mut hits = Vec::new();
    for (index, (id, label)) in labels.iter().enumerate() {
        let hit = button(rgb, content, frame, y, label, index == focus, hero);
        y = hit.y + hit.h + gap;
        hits.push(Hit { id: (*id).into(), ..hit });
    }
    hits
}

fn draw_title(rgb: &mut [u8], content: &Content, frame: Frame, text: &str) {
    let y = frame.px(content.ui.layout.margin_y);
    draw_title_at(rgb, content, frame, text, y, palette(content, &content.ui.colors.text));
}

fn draw_title_colored(rgb: &mut [u8], content: &Content, frame: Frame, text: &str, color: [u8; 3]) {
    draw_title_at(rgb, content, frame, text, frame.px(content.ui.layout.margin_y), color);
}

fn draw_title_at(rgb: &mut [u8], content: &Content, frame: Frame, text: &str, y: i32, color: [u8; 3]) {
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let scale = frame.text(content.ui.layout.title_scale);
    let tw = measure(text, scale, glyph) as i32;
    blit(rgb, frame.width, frame.height, (frame.width as i32 - tw) / 2, y, text, color, scale, glyph);
}

fn button(rgb: &mut [u8], content: &Content, frame: Frame, y: i32, label: &str, focused: bool, hero: Option<&str>) -> Hit {
    let x = (frame.width as i32 - frame.px(content.ui.layout.button_width)) / 2;
    button_at(rgb, content, frame, x, y, label, focused, hero)
}

fn button_at(rgb: &mut [u8], content: &Content, frame: Frame, x: i32, y: i32, label: &str, focused: bool, hero: Option<&str>) -> Hit {
    let w = frame.px(content.ui.layout.button_width);
    let h = frame.px(content.ui.layout.button_height);
    let border = if focused { accent(content, hero) } else { palette(content, &content.ui.colors.dim) };
    rect(rgb, frame.width, frame.height, x, y, w, h, border);
    let inset = frame.px(content.ui.layout.border_px).max(content.ui.layout.inset_min.round() as i32);
    rect(rgb, frame.width, frame.height, x + inset, y + inset, w - inset * 2, h - inset * 2, palette(content, &content.ui.colors.panel));
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let scale = frame.text(content.ui.layout.button_scale);
    let tw = measure(label, scale, glyph) as i32;
    let th = (content.crate_reveal.layout.glyph_rows * scale) as i32;
    blit(rgb, frame.width, frame.height, x + (w - tw) / 2, y + (h - th) / 2, label, palette(content, &content.ui.colors.text), scale, glyph);
    Hit { id: String::new(), x, y, w, h }
}

fn two_line(rgb: &mut [u8], content: &Content, frame: Frame, y: i32, title: &str, hint: &str, focused: bool, hero: Option<&str>) -> Hit {
    let hit = button(rgb, content, frame, y, "", focused, hero);
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let rows = content.crate_reveal.layout.glyph_rows;
    let title_scale = frame.text(content.ui.layout.button_scale);
    let hint_scale = frame.text(content.ui.layout.hint_scale);
    let title_h = (title_scale * rows) as i32;
    let hint_h = (hint_scale * rows) as i32;
    let gap = frame.px(content.ui.layout.spacing);
    let block = title_h + gap + hint_h;
    let mut ty = hit.y + (hit.h - block) / 2;
    let tw = measure(title, title_scale, glyph) as i32;
    blit(rgb, frame.width, frame.height, hit.x + (hit.w - tw) / 2, ty, title, palette(content, &content.ui.colors.text), title_scale, glyph);
    ty += title_h + gap;
    let hw = measure(hint, hint_scale, glyph) as i32;
    blit(rgb, frame.width, frame.height, hit.x + (hit.w - hw) / 2, ty, hint, palette(content, &content.ui.colors.text), hint_scale, glyph);
    hit
}

fn slider_row(
    rgb: &mut [u8],
    content: &Content,
    frame: Frame,
    y: i32,
    label: &str,
    value: &str,
    fill: f32,
    focused: bool,
    hero: Option<&str>,
    steppers: bool,
) -> Vec<Hit> {
    let ui = &content.ui;
    let w = frame.px(ui.layout.button_width);
    let h = frame.px(ui.layout.settings_row_height);
    let x = (frame.width as i32 - w) / 2;
    let border = if focused { accent(content, hero) } else { palette(content, &ui.colors.dim) };
    rect(rgb, frame.width, frame.height, x, y, w, h, border);
    let inset = frame.px(ui.layout.border_px).max(ui.layout.inset_min.round() as i32);
    let panel = palette(content, &ui.colors.panel);
    rect(rgb, frame.width, frame.height, x + inset, y + inset, w - inset * 2, h - inset * 2, panel);
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let scale = frame.text(ui.layout.hint_scale);
    let ink = palette(content, &ui.colors.text);
    let text = format!("{label} {value}");
    if !steppers {
        blit(rgb, frame.width, frame.height, x + frame.px(ui.layout.inset_x), y + frame.px(ui.layout.inset_y), &text, ink, scale, glyph);
        let bar_w = frame.px(ui.layout.slider_width);
        let bar_h = frame.px(ui.layout.slider_height).max(1);
        let bx = x + (w - bar_w) / 2;
        let by = y + h - bar_h - frame.px(ui.layout.inset_bar);
        paint_bar(rgb, frame, content, hero, bx, by, bar_w, bar_h, fill);
        return vec![Hit { id: String::new(), x, y, w, h }];
    }
    let mut side = h - inset * 2;
    if side < 8 {
        side = 8;
    }
    if side > w / 5 {
        side = w / 5;
    }
    let top = y + (h - side) / 2;
    let minus_x = x + inset;
    let plus_x = x + w - inset - side;
    let dim = palette(content, &ui.colors.dim);
    paint_stepper(rgb, frame, content, minus_x, top, side, "-", dim, panel, ink, glyph, scale);
    paint_stepper(rgb, frame, content, plus_x, top, side, "+", dim, panel, ink, glyph, scale);
    let gap = frame.px(ui.layout.inset_x).max(2);
    let text_x = minus_x + side + gap;
    blit(rgb, frame.width, frame.height, text_x, y + frame.px(ui.layout.inset_y), &text, ink, scale, glyph);
    let bar_h = frame.px(ui.layout.slider_height).max(1);
    let bar_x = text_x;
    let bar_w = (plus_x - gap - bar_x).max(1);
    let by = y + h - bar_h - frame.px(ui.layout.inset_bar);
    paint_bar(rgb, frame, content, hero, bar_x, by, bar_w, bar_h, fill);
    vec![
        Hit { id: String::new(), x: minus_x + side, y, w: (plus_x - (minus_x + side)).max(1), h },
        Hit { id: String::new(), x: minus_x, y: top, w: side, h: side },
        Hit { id: String::new(), x: plus_x, y: top, w: side, h: side },
    ]
}

fn paint_bar(rgb: &mut [u8], frame: Frame, content: &Content, hero: Option<&str>, x: i32, y: i32, w: i32, h: i32, fill: f32) {
    rect(rgb, frame.width, frame.height, x, y, w, h, palette(content, &content.ui.colors.dim));
    let fw = ((w as f32) * fill.clamp(0.0, 1.0)).round() as i32;
    rect(rgb, frame.width, frame.height, x, y, fw.max(0), h, accent(content, hero));
}

fn paint_stepper(
    rgb: &mut [u8],
    frame: Frame,
    content: &Content,
    x: i32,
    y: i32,
    side: i32,
    label: &str,
    border: [u8; 3],
    fill: [u8; 3],
    ink: [u8; 3],
    glyph: u32,
    scale: u32,
) {
    rect(rgb, frame.width, frame.height, x, y, side, side, border);
    let inner = (side / 8).max(1);
    rect(rgb, frame.width, frame.height, x + inner, y + inner, side - inner * 2, side - inner * 2, fill);
    let rows = content.crate_reveal.layout.glyph_rows;
    let tw = measure(label, scale, glyph) as i32;
    let th = (rows * scale) as i32;
    blit(rgb, frame.width, frame.height, x + (side - tw) / 2, y + (side - th) / 2, label, ink, scale, glyph);
}

fn draw_wrapped(rgb: &mut [u8], content: &Content, frame: Frame, text: &str, x: i32, mut y: i32, inner: u32, scale: u32, color: [u8; 3]) -> i32 {
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let rows = content.crate_reveal.layout.glyph_rows;
    let leading = frame.px(content.ui.layout.leading);
    let mut line = String::new();
    for word in text.split_whitespace() {
        let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if measure(&next, scale, glyph) <= inner || line.is_empty() {
            line = next;
        } else {
            blit(rgb, frame.width, frame.height, x, y, &line, color, scale, glyph);
            y += (scale * rows) as i32 + leading;
            line = word.to_string();
        }
    }
    if !line.is_empty() {
        blit(rgb, frame.width, frame.height, x, y, &line, color, scale, glyph);
        y += (scale * rows) as i32;
    }
    y
}

/// Frames the full-body portrait, then scales that silhouette into the top half of the card.
fn paint_portrait(rgb: &mut [u8], content: &Content, frame: Frame, hero: &gemini_sim::content::HeroCard, card_x: i32, card_y: i32, card_w: i32, card_h: i32, pad: i32) {
    if card_w <= 4 || card_h <= 4 {
        return;
    }
    let raster = content.ui.portrait.raster;
    // The portrait raster rounds palette channels. Truncation misses those pixels.
    let accent_px = palette_round(content, &hero.accent_key);
    let src = portrait_raster(content, hero, raster);
    let Some((x0, y0, x1, y1)) = opaque_bounds(content, &src, raster) else {
        return;
    };
    let bw = (x1 - x0).max(1);
    let bh = (y1 - y0).max(1);
    let top_half = card_h / 2;
    let inner_w = (card_w - pad * 2).max(1);
    let dest_h = (card_h as f32 * content.ui.layout.portrait_fraction)
        .round()
        .clamp(1.0, (top_half - frame.px(content.ui.layout.inset_portrait)).max(1) as f32) as i32;
    // Keep the portrait at portrait_fraction of the card. Crop a wide silhouette from the center.
    let window_w = ((bh as f32) * (inner_w as f32 / dest_h as f32)).round().clamp(1.0, bw as f32) as i32;
    let win_x0 = x0 + (bw - window_w) / 2;
    let win_x1 = win_x0 + window_w;
    let (sy0, sy1) = vertical_span(content, &src, raster, win_x0, win_x1, y0, y1);
    let window_h = (sy1 - sy0).max(1);
    let dx = card_x + (card_w - inner_w) / 2;
    let dy = card_y + (top_half - dest_h) / 2;
    for row in 0..dest_h {
        for col in 0..inner_w {
            let sx = win_x0 + ((col as f32 + 0.5) * window_w as f32 / inner_w as f32).floor() as i32;
            let sy = sy0 + ((row as f32 + 0.5) * window_h as f32 / dest_h as f32).floor() as i32;
            let sx = sx.clamp(win_x0, win_x1 - 1);
            let sy = sy.clamp(sy0, sy1 - 1);
            let i = ((sy as u32 * raster + sx as u32) * 3) as usize;
            let mut px = [src[i], src[i + 1], src[i + 2]];
            if px == portrait_bg(content) {
                continue;
            }
            if !hero.owned {
                let away = &content.presentation.hud.away;
                px = locked_pixel(
                    px,
                    accent_px,
                    content.ui.layout.locked_accent_mix,
                    away.away_dim,
                    away.away_desaturation,
                    content.ui.layout.locked_body_desaturation,
                    away.dim_floor,
                );
            }
            put(rgb, frame.width, frame.height, dx + col, dy + row, px);
        }
    }
}

fn portrait_bg(content: &Content) -> [u8; 3] {
    palette(content, &content.ui.portrait.bg)
}

fn vertical_span(content: &Content, src: &[u8], raster: u32, x0: i32, x1: i32, y0: i32, y1: i32) -> (i32, i32) {
    let bg = portrait_bg(content);
    let mut top = y1;
    let mut bottom = y0;
    for row in y0..y1 {
        for col in x0..x1 {
            let i = ((row as u32 * raster + col as u32) * 3) as usize;
            if [src[i], src[i + 1], src[i + 2]] != bg {
                top = top.min(row);
                bottom = bottom.max(row + 1);
                break;
            }
        }
    }
    if top >= bottom { (y0, y1) } else { (top, bottom) }
}

fn opaque_bounds(content: &Content, src: &[u8], raster: u32) -> Option<(i32, i32, i32, i32)> {
    let bg = portrait_bg(content);
    let mut x0 = raster as i32;
    let mut y0 = raster as i32;
    let mut x1 = 0i32;
    let mut y1 = 0i32;
    let mut any = false;
    for row in 0..raster as i32 {
        for col in 0..raster as i32 {
            let i = ((row as u32 * raster + col as u32) * 3) as usize;
            if [src[i], src[i + 1], src[i + 2]] == bg {
                continue;
            }
            any = true;
            x0 = x0.min(col);
            y0 = y0.min(row);
            x1 = x1.max(col + 1);
            y1 = y1.max(row + 1);
        }
    }
    if any { Some((x0, y0, x1, y1)) } else { None }
}

/// Full-body card raster for one hero. Uses that hero's portrait mesh, yaw, pitch, and fit.
pub fn raster_hero_portrait(content: &Content, hero_id: &str, raster: u32) -> Vec<u8> {
    let hero = content
        .heroes
        .iter()
        .find(|hero| hero.id == hero_id)
        .unwrap_or_else(|| panic!("unknown hero {hero_id}"));
    portrait_raster(content, hero, raster)
}

fn portrait_raster(content: &Content, hero: &gemini_sim::content::HeroCard, raster: u32) -> Vec<u8> {
    let built = crate::portrait::build(&content.heroes_visual, &hero.portrait.mesh)
        .unwrap_or_else(|| panic!("unknown portrait mesh {}", hero.portrait.mesh));
    let turned = crate::portrait::turn(&built, hero.portrait.yaw_deg, hero.portrait.pitch_deg);
    let positions: Vec<[f32; 3]> = turned.iter().flat_map(|part| part.mesh.positions.iter().copied()).collect();
    let fov = mesh_fov(content, &positions, hero.portrait.fit);
    let parts: Vec<(crate::MeshData, [f32; 4], bool)> = turned
        .iter()
        .map(|part| {
            let key = if part.palette == "accent" { hero.accent_key.as_str() } else { part.palette.as_str() };
            let c = content.palette.key(key);
            (part.mesh.clone(), [c[0], c[1], c[2], 1.0], part.glow)
        })
        .collect();
    raster_parts(raster, raster, &parts, fov)
}

fn mesh_fov(content: &Content, positions: &[[f32; 3]], fit: f32) -> f32 {
    let framing = &content.ui.portrait;
    let mut fov = framing.fov_floor;
    let limit = framing.fov_limit;
    while fov < limit {
        if positions_fit(positions, fov, framing.near, fit) {
            return fov;
        }
        fov += framing.fov_step;
    }
    limit
}

fn positions_fit(positions: &[[f32; 3]], fov: f32, near: f32, fit: f32) -> bool {
    let focal = 1.0 / (fov.to_radians() * 0.5).tan();
    let mut any = false;
    for p in positions {
        let z = -p[2];
        if z < near {
            return false;
        }
        any = true;
        if (p[0] / z * focal).abs() > fit || (p[1] / z * focal).abs() > fit {
            return false;
        }
    }
    any
}

fn rect(rgb: &mut [u8], width: u32, height: u32, x: i32, y: i32, w: i32, h: i32, color: [u8; 3]) {
    if w <= 0 || h <= 0 {
        return;
    }
    for row in 0..h {
        for col in 0..w {
            put(rgb, width, height, x + col, y + row, color);
        }
    }
}

fn put(rgb: &mut [u8], width: u32, height: u32, x: i32, y: i32, color: [u8; 3]) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 || width == 0 {
        return;
    }
    let i = ((y as u32 * width) + x as u32) as usize * 3;
    if i + 2 < rgb.len() {
        rgb[i..i + 3].copy_from_slice(&color);
    }
}

fn palette(content: &Content, key: &str) -> [u8; 3] {
    let c = content.palette.key(key);
    [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8]
}

fn palette_round(content: &Content, key: &str) -> [u8; 3] {
    let c = content.palette.key(key);
    [(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8]
}

fn ink(content: &Content, key: &str) -> [u8; 3] {
    palette(content, key)
}

fn accent(content: &Content, hero: Option<&str>) -> [u8; 3] {
    if let Some(id) = hero {
        if let Some(card) = content.heroes.iter().find(|card| card.id == id) {
            return palette(content, &card.accent_key);
        }
    }
    palette(content, &content.ui.colors.focus_before_hero)
}

fn gray(px: [u8; 3]) -> [u8; 3] {
    let y = crate::luma(px[0] as f32, px[1] as f32, px[2] as f32).round().clamp(0.0, 255.0) as u8;
    [y, y, y]
}

fn locked_pixel(px: [u8; 3], accent: [u8; 3], mix: f32, dim: f32, accent_desat: f32, body_desat: f32, floor: f32) -> [u8; 3] {
    let accent_px = px == accent;
    let desat = if accent_px { accent_desat } else { body_desat };
    let body = crate::away_still::grade(px[0], px[1], px[2], dim, desat, floor);
    if accent_px { lift(body, accent, mix) } else { body }
}

fn lift(base: [u8; 3], toward: [u8; 3], amount: f32) -> [u8; 3] {
    let t = amount.clamp(0.0, 1.0);
    let mix = |from: u8, to: u8| (from as f32 + (to as f32 - from as f32) * t).round() as u8;
    [mix(base[0], toward[0]), mix(base[1], toward[1]), mix(base[2], toward[2])]
}

fn norm(value: f32, min: f32, max: f32) -> f32 {
    if max <= min { 0.0 } else { (value - min) / (max - min) }
}

fn on_off(content: &Content, on: bool) -> String {
    if on { content.ui.copy.on.clone() } else { content.ui.copy.off.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_and_reset_notice_change_the_title_frame() {
        let content = Content::embedded();
        let (play, hits) = paint_menu(&content, &MenuShot::Title { focus: 0, notice: false, native_quit: true }, 1280, 720);
        let (crates, _) = paint_menu(&content, &MenuShot::Title { focus: 1, notice: false, native_quit: true }, 1280, 720);
        let (notice, _) = paint_menu(&content, &MenuShot::Title { focus: 0, notice: true, native_quit: true }, 1280, 720);
        assert_ne!(play, crates, "focus must be visible without a pointer");
        assert_ne!(play, notice, "a rejected settings file must show the reset notice");
        assert!(hits.iter().any(|hit| hit.id == "play"));
        assert!(hits.iter().any(|hit| hit.id == "quit"));
    }

    #[test]
    fn results_title_uses_the_outcome_color() {
        let content = Content::embedded();
        let victory = MenuShot::Results { focus: 0, victory: true, waves: 4, kills: 12, seconds: 90.0, currency: 40.0 };
        let defeat = MenuShot::Results { focus: 0, victory: false, waves: 2, kills: 6, seconds: 40.0, currency: 10.0 };
        let (won, hits) = paint_menu(&content, &victory, 1280, 720);
        let (lost, _) = paint_menu(&content, &defeat, 1280, 720);
        assert_ne!(won, lost);
        assert_eq!(hits[0].id, "play_again");
        assert_eq!(hits[1].id, "main_menu");
        let defeat_color = bytes(&content, &content.ui.colors.defeat);
        assert_eq!(defeat_color, bytes(&content, "spitter_glow"));
        assert!(lost.chunks(3).any(|px| px == defeat_color), "defeat title uses spitter_glow");
        let whole = paint_menu(&content, &MenuShot::Results { focus: 0, victory: false, waves: 2, kills: 6, seconds: 40.0, currency: 40.0 }, 1280, 720).0;
        let rounded = paint_menu(&content, &MenuShot::Results { focus: 0, victory: false, waves: 2, kills: 6, seconds: 40.0, currency: 40.4 }, 1280, 720).0;
        let next = paint_menu(&content, &MenuShot::Results { focus: 0, victory: false, waves: 2, kills: 6, seconds: 40.0, currency: 41.0 }, 1280, 720).0;
        assert_eq!(whole, rounded, "currency_decimals 0 drops the fraction");
        assert_ne!(whole, next);
    }

    #[test]
    fn click_areas_match_the_drawn_buttons_at_odd_sizes() {
        for (width, height) in [(1366, 768), (2560, 1440)] {
            buttons_line_up(width, height);
        }
    }

    fn buttons_line_up(width: u32, height: u32) {
        let content = Content::embedded();
        let (rgb, hits) = paint_menu(&content, &MenuShot::Title { focus: 0, notice: false, native_quit: true }, width, height);
        assert!(hits.len() >= 4, "{width}x{height}");
        let scale = height as f32 / content.ui.layout.fit_height;
        let expect_w = (content.ui.layout.button_width * scale).round() as i32;
        let expect_h = (content.ui.layout.button_height * scale).round() as i32;
        let inset = ((content.ui.layout.border_px * scale).round() as i32).max(content.ui.layout.inset_min.round() as i32);
        let common = bytes(&content, &content.ui.colors.focus_before_hero);
        let dim = bytes(&content, &content.ui.colors.dim);
        let panel = bytes(&content, &content.ui.colors.panel);
        for (index, hit) in hits.iter().enumerate() {
            assert_eq!((hit.w, hit.h), (expect_w, expect_h), "{} at {width}x{height}", hit.id);
            let border = if index == 0 { common } else { dim };
            for x in hit.x..hit.x + hit.w {
                assert_eq!(sample(&rgb, width, x, hit.y), border, "{} top {x} at {width}x{height}", hit.id);
                assert_eq!(sample(&rgb, width, x, hit.y + hit.h - 1), border, "{} bottom {x} at {width}x{height}", hit.id);
            }
            for y in hit.y..hit.y + hit.h {
                assert_eq!(sample(&rgb, width, hit.x, y), border, "{} left {y} at {width}x{height}", hit.id);
                assert_eq!(sample(&rgb, width, hit.x + hit.w - 1, y), border, "{} right {y} at {width}x{height}", hit.id);
            }
            assert_eq!(sample(&rgb, width, hit.x + inset, hit.y + hit.h / 2), panel, "{} at {width}x{height}", hit.id);
            let mid_y = hit.y + hit.h / 2;
            let mid_x = hit.x + hit.w / 2;
            if hit.x > 0 {
                assert_ne!(sample(&rgb, width, hit.x - 1, mid_y), border, "{} at {width}x{height}", hit.id);
            }
            if hit.y > 0 {
                assert_ne!(sample(&rgb, width, mid_x, hit.y - 1), border, "{} at {width}x{height}", hit.id);
            }
            if hit.x + hit.w < width as i32 {
                assert_ne!(sample(&rgb, width, hit.x + hit.w, mid_y), border, "{} at {width}x{height}", hit.id);
            }
            if hit.y + hit.h < height as i32 {
                assert_ne!(sample(&rgb, width, mid_x, hit.y + hit.h), border, "{} at {width}x{height}", hit.id);
            }
        }
    }

    #[test]
    fn portraits_are_centered_in_the_top_half_and_locked_stats_use_the_text_key() {
        let content = Content::embedded();
        let locked_ink = bytes(&content, &content.ui.colors.locked_text);
        assert_eq!(content.ui.colors.locked_text, "rarity_common");
        let panel = bytes(&content, &content.ui.colors.panel);
        for (width, height) in [(1280, 720), (2560, 1440)] {
            let (rgb, hits) = paint_menu(&content, &MenuShot::Heroes { focus: 0 }, width, height);
            let cards: Vec<_> = hits.iter().filter(|hit| hit.id.starts_with("hero:")).collect();
            assert_eq!(cards.len(), content.heroes.len());
            for hit in cards {
                let id = hit.id.trim_start_matches("hero:");
                let hero = content.heroes.iter().find(|hero| hero.id == id).unwrap();
                let inset = ((content.ui.layout.border_px * (height as f32 / content.ui.layout.fit_height)).round() as i32)
                    .max(content.ui.layout.inset_min.round() as i32);
                let top = hit.y + inset;
                let bottom = hit.y + hit.h / 2;
                let mut x0 = hit.x + hit.w;
                let mut y0 = bottom;
                let mut x1 = hit.x;
                let mut y1 = top;
                let mut count = 0u32;
                for y in top..bottom {
                    for x in (hit.x + inset)..(hit.x + hit.w - inset) {
                        let px = sample(&rgb, width, x, y);
                        if px == panel {
                            continue;
                        }
                        count += 1;
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x + 1);
                        y1 = y1.max(y + 1);
                    }
                }
                assert!(count > 40, "{id} portrait pixels at {width}x{height}");
                let target = (hit.h as f32 * content.ui.layout.portrait_fraction).round() as i32;
                let painted_h = y1 - y0;
                assert!((painted_h - target).abs() <= 3, "{id} portrait height {painted_h} vs {target}");
                let center = (x0 + x1) / 2;
                let card_center = hit.x + hit.w / 2;
                assert!((center - card_center).abs() <= 2, "{id} portrait center {center} vs {card_center}");
                assert!(y1 <= hit.y + hit.h / 2 + 1, "{id} portrait stays in the top half");
                let mut lowest = panel;
                let mut found = false;
                for y in ((hit.y + hit.h / 2)..(hit.y + hit.h - inset)).rev() {
                    for x in (hit.x + inset)..(hit.x + hit.w - inset) {
                        let px = sample(&rgb, width, x, y);
                        if px != panel {
                            lowest = px;
                            found = true;
                            break;
                        }
                    }
                    if found {
                        break;
                    }
                }
                assert!(found, "{id} name sits below the portrait");
                if !hero.owned {
                    assert_eq!(lowest, locked_ink, "{id} HP/SPD line uses locked_text");
                }
            }
        }
        let content = Content::embedded();
        let panel = bytes(&content, &content.ui.colors.panel);
        let (rgb, hits) = paint_menu(&content, &MenuShot::Heroes { focus: 0 }, 1280, 720);
        for hit in hits.iter().filter(|hit| hit.id.starts_with("hero:") && hit.id != "hero:vex") {
            let id = hit.id.trim_start_matches("hero:");
            let hero = content.heroes.iter().find(|hero| hero.id == id).unwrap();
            let accent = round_bytes(&content, &hero.accent_key);
            let away = &content.presentation.hud.away;
            let graded = crate::away_still::grade(accent[0], accent[1], accent[2], away.away_dim, away.away_desaturation, away.dim_floor);
            let mixed = lift(graded, accent, content.ui.layout.locked_accent_mix);
            let mut glow = 0u32;
            let mut colored = 0u32;
            let inset = ((content.ui.layout.border_px * (720.0 / content.ui.layout.fit_height)).round() as i32).max(1);
            let top = hit.y + inset;
            let bottom = hit.y + hit.h / 2;
            for y in top..bottom {
                for x in (hit.x + inset)..(hit.x + hit.w - inset) {
                    let px = sample(&rgb, 1280, x, y);
                    if px == panel {
                        continue;
                    }
                    if px == mixed {
                        glow += 1;
                    } else if px[0] != px[1] || px[1] != px[2] {
                        colored += 1;
                    }
                }
            }
            assert!(glow > 8, "{id} keeps a mixed accent glow");
            assert_eq!(colored, 0, "{id} locked portrait is grayscale aside from the accent mix");
        }
    }

    #[test]
    fn settings_rows_have_minus_and_plus_buttons() {
        let content = Content::embedded();
        let prefs = PaintPrefs::defaults(&content);
        let (rgb, hits) = paint_menu(
            &content,
            &MenuShot::Settings { focus: 0, hero_id: String::new(), prefs, notice: false },
            1280,
            720,
        );
        let row = hits.iter().find(|hit| hit.id == "slider:sensitivity").unwrap();
        let dec = hits.iter().find(|hit| hit.id == "slider:sensitivity:-").unwrap();
        let inc = hits.iter().find(|hit| hit.id == "slider:sensitivity:+").unwrap();
        assert!(dec.w > 8 && dec.h > 8 && inc.w > 8 && inc.h > 8);
        assert!(dec.x + dec.w <= row.x, "minus overlaps the value");
        assert!(inc.x >= row.x + row.w, "plus overlaps the value");
        let ink = bytes(&content, &content.ui.colors.text);
        assert!(hit_has_ink(&rgb, 1280, dec, ink), "minus glyph is not drawn");
        assert!(hit_has_ink(&rgb, 1280, inc, ink), "plus glyph is not drawn");
        assert!(hits.iter().any(|hit| hit.id == "slider:fov:-"));
        assert!(!hits.iter().any(|hit| hit.id == "toggle:bob:-"), "toggles are not sliders");
    }

    fn hit_has_ink(rgb: &[u8], width: u32, hit: &Hit, ink: [u8; 3]) -> bool {
        for y in hit.y..hit.y + hit.h {
            for x in hit.x..hit.x + hit.w {
                if sample(rgb, width, x, y) == ink {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn portraits_are_full_bodies_and_the_content_turn_changes_them() {
        let content = Content::embedded();
        let raster = content.ui.portrait.raster;
        let listed: Vec<&str> = content.heroes_visual.meshes.iter().map(|mesh| mesh.id.as_str()).collect();
        let portraits: Vec<&str> = content.heroes.iter().map(|hero| hero.portrait.mesh.as_str()).collect();
        assert_eq!(listed.len(), portraits.len());
        for id in &listed {
            assert!(portraits.contains(id), "{id} is not a portrait mesh");
        }
        for id in &portraits {
            assert!(listed.contains(id), "{id} has no visual mesh");
        }
        for hero in &content.heroes {
            let (x0, y0, x1, y1) = opaque_bounds(&content, &portrait_raster(&content, hero, raster), raster).unwrap();
            let w = x1 - x0;
            let h = y1 - y0;
            assert!(h > w, "{} portrait is {w}x{h}, wider than a body", hero.id);
            let turned = portrait_raster(&content, hero, raster);
            let mut flat_hero = hero.clone();
            flat_hero.portrait.yaw_deg = 0.0;
            flat_hero.portrait.pitch_deg = 0.0;
            let flat = portrait_raster(&content, &flat_hero, raster);
            if hero.portrait.yaw_deg == 0.0 && hero.portrait.pitch_deg == 0.0 {
                assert_eq!(turned, flat, "{}", hero.id);
            } else {
                assert_ne!(turned, flat, "{}", hero.id);
            }
        }
    }

    #[test]
    fn portrait_builders_follow_the_content_list() {
        let content = gemini_sim::content::Content::embedded();
        for mesh in &content.heroes_visual.meshes {
            let built = crate::portrait::build(&content.heroes_visual, &mesh.id);
            assert!(built.is_some_and(|parts| !parts.is_empty()), "{}", mesh.id);
        }
        assert!(crate::portrait::build(&content.heroes_visual, "laser_knife").is_none());
        assert!(crate::portrait::build(&content.heroes_visual, "pulse_rifle").is_none());
    }

    fn round_bytes(content: &Content, key: &str) -> [u8; 3] {
        let c = content.palette.key(key);
        [(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8]
    }

    fn bytes(content: &Content, key: &str) -> [u8; 3] {
        let c = content.palette.key(key);
        [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8]
    }

    fn sample(rgb: &[u8], width: u32, x: i32, y: i32) -> [u8; 3] {
        let i = ((y as u32 * width) + x as u32) as usize * 3;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    }
}
