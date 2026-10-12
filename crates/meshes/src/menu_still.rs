//! Software frames of the menu screens. Layout, type size, and colors come from `ui.toml`.
//! The pixel font is the crate-reveal glyph set.

use gemini_sim::content::{format_fixed, Content};

use crate::away_still::paint_arena;
use crate::glyph::{blit, measure};
use crate::preview::{raster_viewmodel, ViewmodelFrame};
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

pub fn paint_menu(content: &Content, shot: &MenuShot, width: u32, height: u32) -> (Vec<u8>, Vec<Hit>) {
    let width = width.max(8);
    let height = height.max(8);
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    match shot {
        MenuShot::Pause { .. } | MenuShot::ClickToPlay => {
            paint_arena(&mut rgb, width, height, content);
            let keep = content.ui.layout.pause_keep.clamp(0.0, 1.0);
            for px in rgb.chunks_exact_mut(3) {
                for channel in px.iter_mut() {
                    *channel = (*channel as f32 * keep).round() as u8;
                }
            }
        }
        _ => paint_stage(&mut rgb, width, height, content, &content.crate_reveal.layout),
    }
    let hits = match shot {
        MenuShot::Title { focus, notice, native_quit } => paint_title(&mut rgb, content, width, height, *focus, *notice, *native_quit),
        MenuShot::Heroes { focus } => paint_heroes(&mut rgb, content, width, height, *focus),
        MenuShot::Difficulty { focus, hero_id, notice } => paint_difficulty(&mut rgb, content, width, height, *focus, hero_id, *notice),
        MenuShot::Pause { focus, hero_id } => paint_pause(&mut rgb, content, width, height, *focus, hero_id),
        MenuShot::Settings { focus, hero_id, prefs, notice } => paint_settings(&mut rgb, content, width, height, *focus, hero_id, prefs, *notice),
        MenuShot::Results { focus, victory, waves, kills, seconds, currency } => {
            paint_results(&mut rgb, content, width, height, *focus, *victory, *waves, *kills, *seconds, *currency)
        }
        MenuShot::Crates { focus, rarity, failed, can_pull } => {
            paint_crates(&mut rgb, content, width, height, *focus, rarity.as_deref(), *failed, *can_pull)
        }
        MenuShot::ClickToPlay => paint_click(&mut rgb, content, width, height),
    };
    (rgb, hits)
}

fn paint_title(rgb: &mut [u8], content: &Content, width: u32, height: u32, focus: usize, notice: bool, native_quit: bool) -> Vec<Hit> {
    let copy = &content.ui.copy;
    let mut labels = vec![
        ("play", copy.play.as_str()),
        ("crates", copy.crates.as_str()),
        ("settings", copy.settings.as_str()),
    ];
    if native_quit {
        labels.push(("quit", copy.quit.as_str()));
    }
    let hits = column(rgb, content, width, height, &copy.title, &labels, focus, None, true);
    if notice {
        let glyph = content.crate_reveal.layout.title_glyph_px;
        let scale = content.ui.layout.hint_scale;
        let text = copy.reset_notice.as_str();
        let tw = measure(text, scale, glyph) as i32;
        let x = (width as i32 - tw) / 2;
        let y = content.ui.layout.margin_y as i32;
        blit(rgb, width, height, x, y, text, ink(content, &content.ui.colors.defeat), scale, glyph);
    }
    hits
}

fn paint_heroes(rgb: &mut [u8], content: &Content, width: u32, height: u32, focus: usize) -> Vec<Hit> {
    let ui = &content.ui;
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let rows = content.crate_reveal.layout.glyph_rows;
    let title = ui.copy.heroes.to_uppercase();
    draw_title(rgb, content, width, &title);
    let n = content.heroes.len().max(1) as i32;
    let card_w = ui.layout.card_width as i32;
    let card_h = ui.layout.card_height as i32;
    let gap = ui.layout.card_gap as i32;
    let total = n * card_w + (n - 1) * gap;
    let mut x = (width as i32 - total) / 2;
    let y = ui.layout.margin_y as i32 + (ui.layout.title_scale * rows) as i32 + ui.layout.title_gap as i32;
    let mut hits = Vec::new();
    for (index, hero) in content.heroes.iter().enumerate() {
        let id = format!("hero:{}", hero.id);
        let focused = index == focus;
        let base = if hero.owned {
            palette(content, &hero.accent_key)
        } else {
            gray(palette(content, &hero.accent_key))
        };
        let border = if focused { lift(base, palette(content, &ui.colors.text), ui.layout.focus_lift) } else { base };
        rect(rgb, width, height, x, y, card_w, card_h, border);
        let inset = ui.layout.border_px as i32;
        rect(rgb, width, height, x + inset, y + inset, card_w - inset * 2, card_h - inset * 2, palette(content, &ui.colors.panel));
        let pad = ui.layout.card_pad as i32;
        let portrait_h = ui.layout.portrait_height as i32;
        paint_portrait(rgb, content, hero, x + pad, y + pad, card_w - pad * 2, portrait_h);
        let name_color = if hero.owned { palette(content, &hero.accent_key) } else { palette(content, &ui.colors.locked) };
        let mut ty = y + pad + portrait_h + 8;
        let inner = (card_w - pad * 2) as u32;
        ty = draw_wrapped(rgb, content, &hero.name.to_uppercase(), x + pad, ty, inner, ui.layout.button_scale, name_color);
        let dim = palette(content, &ui.colors.dim);
        let paper = palette(content, &ui.colors.text);
        let body = if hero.owned { paper } else { dim };
        ty = draw_wrapped(rgb, content, &hero.role.to_uppercase(), x + pad, ty + 4, inner, ui.layout.hint_scale, body);
        ty = draw_wrapped(rgb, content, &hero.ability.to_uppercase(), x + pad, ty + 6, inner, ui.layout.hint_scale, body);
        let stats = format!(
            "{} {}  {} {}",
            ui.copy.hp,
            format_fixed(hero.max_hp, 0),
            ui.copy.speed,
            format_fixed(hero.move_speed_mult, ui.layout.stat_decimals)
        );
        draw_wrapped(rgb, content, &stats, x + pad, ty + 8, inner, ui.layout.hint_scale, body);
        if !hero.owned {
            let lock = ui.copy.locked.as_str();
            let tw = measure(lock, ui.layout.hint_scale, glyph) as i32;
            blit(rgb, width, height, x + (card_w - tw) / 2, y + pad + 8, lock, dim, ui.layout.hint_scale, glyph);
        }
        hits.push(Hit { id, x, y, w: card_w, h: card_h });
        x += card_w + gap;
    }
    let by = y + card_h + ui.layout.row_gap as i32;
    hits.push(Hit { id: "back".into(), ..button(rgb, content, width, by, &ui.copy.back, focus == content.heroes.len(), None) });
    hits
}

fn paint_difficulty(rgb: &mut [u8], content: &Content, width: u32, height: u32, focus: usize, hero_id: &str, notice: bool) -> Vec<Hit> {
    let ui = &content.ui;
    draw_title(rgb, content, width, &ui.copy.difficulty);
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let rows = content.crate_reveal.layout.glyph_rows;
    let mut y = ui.layout.margin_y as i32 + (ui.layout.title_scale * rows) as i32 + ui.layout.title_gap as i32;
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
        let hit = two_line(rgb, content, width, y, &label, &stats, index == focus, hero);
        y = hit.y + hit.h + ui.layout.row_gap as i32;
        hits.push(Hit { id: format!("tier:{}", tier.id), ..hit });
    }
    let host = button(rgb, content, width, y, &ui.copy.host, focus == hits.len(), hero);
    y = host.y + host.h + ui.layout.row_gap as i32;
    hits.push(Hit { id: "host".into(), ..host });
    let join = button(rgb, content, width, y, &ui.copy.join, focus == hits.len(), hero);
    y = join.y + join.h + ui.layout.row_gap as i32;
    hits.push(Hit { id: "join".into(), ..join });
    hits.push(Hit { id: "back".into(), ..button(rgb, content, width, y, &ui.copy.back, focus == hits.len(), hero) });
    if notice {
        let text = ui.copy.coming_soon.as_str();
        let tw = measure(text, ui.layout.hint_scale, glyph) as i32;
        let ny = y + ui.layout.button_height as i32 + ui.layout.row_gap as i32;
        blit(rgb, width, height, (width as i32 - tw) / 2, ny, text, ink(content, &ui.colors.text), ui.layout.hint_scale, glyph);
    }
    hits
}

fn paint_pause(rgb: &mut [u8], content: &Content, width: u32, height: u32, focus: usize, hero_id: &str) -> Vec<Hit> {
    let ui = &content.ui;
    let pw = ui.layout.pause_panel_width as i32;
    let ph = ui.layout.pause_panel_height as i32;
    let x = (width as i32 - pw) / 2;
    let y = (height as i32 - ph) / 2;
    let hero = if hero_id.is_empty() { None } else { Some(hero_id) };
    rect(rgb, width, height, x, y, pw, ph, accent(content, hero));
    let inset = ui.layout.border_px as i32;
    rect(rgb, width, height, x + inset, y + inset, pw - inset * 2, ph - inset * 2, palette(content, &ui.colors.panel));
    let labels = [("resume", ui.copy.resume.as_str()), ("settings", ui.copy.settings.as_str()), ("quit_to_menu", ui.copy.quit_to_menu.as_str())];
    let stack = labels.len() as i32 * ui.layout.button_height as i32 + (labels.len() as i32 - 1) * ui.layout.row_gap as i32;
    let mut by = y + (ph - stack) / 2;
    let mut hits = Vec::new();
    for (index, (id, label)) in labels.iter().enumerate() {
        let hit = button_at(rgb, content, x + (pw - ui.layout.button_width as i32) / 2, by, label, index == focus, hero);
        by = hit.y + hit.h + ui.layout.row_gap as i32;
        hits.push(Hit { id: (*id).into(), ..hit });
    }
    hits
}

fn paint_settings(rgb: &mut [u8], content: &Content, width: u32, height: u32, focus: usize, hero_id: &str, prefs: &PaintPrefs, notice: bool) -> Vec<Hit> {
    let ui = &content.ui;
    draw_title(rgb, content, width, &ui.copy.settings);
    let rows_n = content.crate_reveal.layout.glyph_rows;
    let mut y = ui.layout.margin_y as i32 + (ui.layout.title_scale * rows_n) as i32 + ui.layout.title_gap as i32;
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
    for (index, (id, label, value, fill)) in rows.iter().enumerate() {
        let hit = slider_row(rgb, content, width, y, label, value, *fill, index == focus, hero);
        y = hit.y + hit.h + 8;
        hits.push(Hit { id: (*id).into(), ..hit });
    }
    hits.push(Hit { id: "back".into(), ..button(rgb, content, width, y, &ui.copy.back, focus == hits.len(), hero) });
    if notice {
        let glyph = content.crate_reveal.layout.title_glyph_px;
        let text = ui.copy.reset_notice.as_str();
        let tw = measure(text, ui.layout.hint_scale, glyph) as i32;
        let ny = y + ui.layout.button_height as i32 + ui.layout.row_gap as i32;
        blit(rgb, width, height, (width as i32 - tw) / 2, ny, text, ink(content, &ui.colors.defeat), ui.layout.hint_scale, glyph);
    }
    hits
}

fn paint_results(rgb: &mut [u8], content: &Content, width: u32, height: u32, focus: usize, victory: bool, waves: u32, kills: u32, seconds: f32, currency: f32) -> Vec<Hit> {
    let ui = &content.ui;
    let title = if victory { ui.copy.victory.as_str() } else { ui.copy.defeat.as_str() };
    let color = if victory { &ui.colors.victory } else { &ui.colors.defeat };
    draw_title_colored(rgb, content, width, title, palette(content, color));
    let rows = content.crate_reveal.layout.glyph_rows;
    let mut y = ui.layout.margin_y as i32 + (ui.layout.title_scale * rows) as i32 + ui.layout.title_gap as i32;
    let mins = (seconds.max(0.0) as u32) / 60;
    let secs = (seconds.max(0.0) as u32) % 60;
    let lines = [
        format!("{} {}", ui.copy.waves, waves),
        format!("{} {}", ui.copy.kills, kills),
        format!("{} {}:{:02}", ui.copy.time, mins, secs),
        format!("{} {}", ui.copy.currency, format_fixed(currency, ui.layout.stat_decimals)),
    ];
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let paper = palette(content, &ui.colors.text);
    for line in &lines {
        let tw = measure(line, ui.layout.button_scale, glyph) as i32;
        blit(rgb, width, height, (width as i32 - tw) / 2, y, line, paper, ui.layout.button_scale, glyph);
        y += (ui.layout.button_scale * rows) as i32 + 8;
    }
    y += ui.layout.row_gap as i32;
    let mut hits = Vec::new();
    let again = button(rgb, content, width, y, &ui.copy.play_again, focus == 0, None);
    y = again.y + again.h + ui.layout.row_gap as i32;
    hits.push(Hit { id: "play_again".into(), ..again });
    hits.push(Hit { id: "main_menu".into(), ..button(rgb, content, width, y, &ui.copy.main_menu, focus == 1, None) });
    hits
}

fn paint_crates(rgb: &mut [u8], content: &Content, width: u32, height: u32, focus: usize, rarity: Option<&str>, failed: bool, can_pull: bool) -> Vec<Hit> {
    let ui = &content.ui;
    draw_title(rgb, content, width, &ui.copy.crates_title);
    let rows = content.crate_reveal.layout.glyph_rows;
    let mut y = ui.layout.margin_y as i32 + (ui.layout.title_scale * rows) as i32 + ui.layout.title_gap as i32;
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let paper = palette(content, &ui.colors.text);
    let line = if let Some(rarity) = rarity {
        rarity.to_uppercase()
    } else if failed {
        ui.copy.pull_failed.clone()
    } else {
        ui.copy.crates_empty.clone()
    };
    let tw = measure(&line, ui.layout.button_scale, glyph) as i32;
    blit(rgb, width, height, (width as i32 - tw) / 2, y, &line, paper, ui.layout.button_scale, glyph);
    y += (ui.layout.button_scale * rows) as i32 + ui.layout.title_gap as i32;
    let mut hits = Vec::new();
    let mut index = 0;
    if can_pull {
        let hit = button(rgb, content, width, y, &ui.copy.crates_pull, focus == index, None);
        y = hit.y + hit.h + ui.layout.row_gap as i32;
        hits.push(Hit { id: "pull".into(), ..hit });
        index += 1;
    }
    if rarity.is_some() {
        let hit = button(rgb, content, width, y, &ui.copy.crates_open, focus == index, None);
        y = hit.y + hit.h + ui.layout.row_gap as i32;
        hits.push(Hit { id: "open".into(), ..hit });
        index += 1;
    }
    let _ = index;
    hits.push(Hit { id: "back".into(), ..button(rgb, content, width, y, &ui.copy.back, focus == hits.len(), None) });
    hits
}

fn paint_click(rgb: &mut [u8], content: &Content, width: u32, height: u32) -> Vec<Hit> {
    let y = (height as i32 - content.ui.layout.button_height as i32) / 2;
    vec![Hit { id: "click".into(), ..button(rgb, content, width, y, &content.ui.copy.click_to_play, true, None) }]
}

fn column(rgb: &mut [u8], content: &Content, width: u32, height: u32, title: &str, labels: &[(&str, &str)], focus: usize, hero: Option<&str>, center: bool) -> Vec<Hit> {
    let ui = &content.ui;
    let rows = content.crate_reveal.layout.glyph_rows;
    let title_h = (ui.layout.title_scale * rows) as i32;
    let count = labels.len() as i32;
    let stack = title_h + ui.layout.title_gap as i32 + count * ui.layout.button_height as i32 + (count - 1).max(0) * ui.layout.row_gap as i32;
    let mut y = if center { (height as i32 - stack) / 2 } else { ui.layout.margin_y as i32 };
    draw_title_at(rgb, content, width, title, y, palette(content, &ui.colors.text));
    y += title_h + ui.layout.title_gap as i32;
    let mut hits = Vec::new();
    for (index, (id, label)) in labels.iter().enumerate() {
        let hit = button(rgb, content, width, y, label, index == focus, hero);
        y = hit.y + hit.h + ui.layout.row_gap as i32;
        hits.push(Hit { id: (*id).into(), ..hit });
    }
    hits
}

fn draw_title(rgb: &mut [u8], content: &Content, width: u32, text: &str) {
    let y = content.ui.layout.margin_y as i32;
    draw_title_at(rgb, content, width, text, y, palette(content, &content.ui.colors.text));
}

fn draw_title_colored(rgb: &mut [u8], content: &Content, width: u32, text: &str, color: [u8; 3]) {
    draw_title_at(rgb, content, width, text, content.ui.layout.margin_y as i32, color);
}

fn draw_title_at(rgb: &mut [u8], content: &Content, width: u32, text: &str, y: i32, color: [u8; 3]) {
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let scale = content.ui.layout.title_scale;
    let tw = measure(text, scale, glyph) as i32;
    blit(rgb, width, content_height(rgb, width), (width as i32 - tw) / 2, y, text, color, scale, glyph);
}

fn content_height(rgb: &[u8], width: u32) -> u32 {
    (rgb.len() / 3 / width as usize) as u32
}

fn button(rgb: &mut [u8], content: &Content, width: u32, y: i32, label: &str, focused: bool, hero: Option<&str>) -> Hit {
    let x = (width as i32 - content.ui.layout.button_width as i32) / 2;
    button_at(rgb, content, x, y, label, focused, hero)
}

fn button_at(rgb: &mut [u8], content: &Content, x: i32, y: i32, label: &str, focused: bool, hero: Option<&str>) -> Hit {
    let frame_w = frame_width(rgb);
    let frame_h = height_of(rgb, frame_w);
    let w = content.ui.layout.button_width as i32;
    let h = content.ui.layout.button_height as i32;
    let border = if focused { accent(content, hero) } else { palette(content, &content.ui.colors.dim) };
    rect(rgb, frame_w, frame_h, x, y, w, h, border);
    let inset = content.ui.layout.border_px as i32;
    rect(rgb, frame_w, frame_h, x + inset, y + inset, w - inset * 2, h - inset * 2, palette(content, &content.ui.colors.panel));
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let scale = content.ui.layout.button_scale;
    let tw = measure(label, scale, glyph) as i32;
    let th = (content.crate_reveal.layout.glyph_rows * scale) as i32;
    blit(rgb, frame_w, frame_h, x + (w - tw) / 2, y + (h - th) / 2, label, palette(content, &content.ui.colors.text), scale, glyph);
    Hit { id: String::new(), x, y, w, h }
}

fn two_line(rgb: &mut [u8], content: &Content, width: u32, y: i32, title: &str, hint: &str, focused: bool, hero: Option<&str>) -> Hit {
    let hit = button(rgb, content, width, y, "", focused, hero);
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let rows = content.crate_reveal.layout.glyph_rows;
    let title_h = (content.ui.layout.button_scale * rows) as i32;
    let hint_h = (content.ui.layout.hint_scale * rows) as i32;
    let gap = 4;
    let block = title_h + gap + hint_h;
    let mut ty = hit.y + (hit.h - block) / 2;
    let tw = measure(title, content.ui.layout.button_scale, glyph) as i32;
    blit(rgb, width, height_of(rgb, width), hit.x + (hit.w - tw) / 2, ty, title, palette(content, &content.ui.colors.text), content.ui.layout.button_scale, glyph);
    ty += title_h + gap;
    let hw = measure(hint, content.ui.layout.hint_scale, glyph) as i32;
    blit(rgb, width, height_of(rgb, width), hit.x + (hit.w - hw) / 2, ty, hint, palette(content, &content.ui.colors.text), content.ui.layout.hint_scale, glyph);
    hit
}

fn slider_row(rgb: &mut [u8], content: &Content, width: u32, y: i32, label: &str, value: &str, fill: f32, focused: bool, hero: Option<&str>) -> Hit {
    let ui = &content.ui;
    let w = ui.layout.button_width as i32;
    let h = ui.layout.settings_row_height as i32;
    let x = (width as i32 - w) / 2;
    let border = if focused { accent(content, hero) } else { palette(content, &ui.colors.dim) };
    let frame_h = height_of(rgb, width);
    rect(rgb, width, frame_h, x, y, w, h, border);
    let inset = ui.layout.border_px as i32;
    rect(rgb, width, frame_h, x + inset, y + inset, w - inset * 2, h - inset * 2, palette(content, &ui.colors.panel));
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let text = format!("{label} {value}");
    blit(rgb, width, frame_h, x + 12, y + 6, &text, palette(content, &ui.colors.text), ui.layout.hint_scale, glyph);
    let bar_w = ui.layout.slider_width as i32;
    let bar_h = ui.layout.slider_height as i32;
    let bx = x + (w - bar_w) / 2;
    let by = y + h - bar_h - 8;
    rect(rgb, width, frame_h, bx, by, bar_w, bar_h, palette(content, &ui.colors.dim));
    let fw = ((bar_w as f32) * fill.clamp(0.0, 1.0)).round() as i32;
    rect(rgb, width, frame_h, bx, by, fw.max(0), bar_h, accent(content, hero));
    Hit { id: String::new(), x, y, w, h }
}

fn draw_wrapped(rgb: &mut [u8], content: &Content, text: &str, x: i32, mut y: i32, inner: u32, scale: u32, color: [u8; 3]) -> i32 {
    let glyph = content.crate_reveal.layout.title_glyph_px;
    let rows = content.crate_reveal.layout.glyph_rows;
    let width = frame_width(rgb);
    let height = height_of(rgb, width);
    let mut line = String::new();
    for word in text.split_whitespace() {
        let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if measure(&next, scale, glyph) <= inner || line.is_empty() {
            line = next;
        } else {
            blit(rgb, width, height, x, y, &line, color, scale, glyph);
            y += (scale * rows) as i32 + 2;
            line = word.to_string();
        }
    }
    if !line.is_empty() {
        blit(rgb, width, height, x, y, &line, color, scale, glyph);
        y += (scale * rows) as i32;
    }
    y
}

fn paint_portrait(rgb: &mut [u8], content: &Content, hero: &gemini_sim::content::HeroCard, x: i32, y: i32, w: i32, h: i32) {
    let width = frame_width(rgb);
    let height = height_of(rgb, width);
    if w <= 2 || h <= 2 {
        return;
    }
    let frame = view_frame(content, hero);
    let src = raster_viewmodel(w as u32, h as u32, &frame);
    for row in 0..h as u32 {
        for col in 0..w as u32 {
            let i = ((row * w as u32 + col) * 3) as usize;
            let mut px = [src[i], src[i + 1], src[i + 2]];
            if px == [0x8A, 0x8A, 0x8A] {
                continue;
            }
            if !hero.owned {
                px = gray(px);
            }
            put(rgb, width, height, x + col as i32, y + row as i32, px);
        }
    }
}

fn view_frame(content: &Content, hero: &gemini_sim::content::HeroCard) -> ViewmodelFrame {
    let starter = &content.hero;
    ViewmodelFrame {
        accent: content.palette.key(&hero.accent_key),
        offset: starter.viewmodel_offset,
        fov: starter.viewmodel_fov,
        overdrive: false,
        glow_radius: starter.muzzle_flash_radius,
        glow_core: starter.muzzle_flash_core,
        glow_falloff: starter.muzzle_flash_falloff,
        glow_tip_px: starter.muzzle_flash_tip_px,
        glow_color: [starter.muzzle_flash_color[0], starter.muzzle_flash_color[1], starter.muzzle_flash_color[2], 1.0],
    }
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

fn ink(content: &Content, key: &str) -> [u8; 3] {
    palette(content, key)
}

fn accent(content: &Content, hero: Option<&str>) -> [u8; 3] {
    if let Some(id) = hero {
        if let Some(card) = content.heroes.iter().find(|card| card.id == id && card.owned) {
            return palette(content, &card.accent_key);
        }
    }
    palette(content, &content.ui.colors.focus_before_hero)
}

fn gray(px: [u8; 3]) -> [u8; 3] {
    let y = (0.30 * px[0] as f32 + 0.59 * px[1] as f32 + 0.11 * px[2] as f32).round() as u8;
    [y, y, y]
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

fn height_of(rgb: &[u8], width: u32) -> u32 {
    if width == 0 { 0 } else { (rgb.len() / 3 / width as usize) as u32 }
}

/// Menu frames are 16:9, so the stride is the factor of `len/3` closest to that ratio.
fn frame_width(rgb: &[u8]) -> u32 {
    let pixels = (rgb.len() / 3) as u32;
    let mut best = 1280u32;
    let mut err = u32::MAX;
    for width in [960u32, 1280, 1920, 320, 640] {
        if width == 0 || pixels % width != 0 {
            continue;
        }
        let height = pixels / width;
        let diff = width.abs_diff(height * 16 / 9);
        if diff < err {
            err = diff;
            best = width;
        }
    }
    if pixels % best == 0 { best } else { 1280 }
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
    }
}
