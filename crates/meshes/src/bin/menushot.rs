//! Menu stills. Every screen at 1280x720, plus the pause screen at 1920x1080.
//!
//! ```text
//! cargo run -p gemini-meshes --bin menushot -- --out /opt/cursor/artifacts/review/menu
//! ```

use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use gemini_meshes::{paint_menu, paint_menu_masked, raster_hero_portrait, MenuShot, PaintPrefs};
use gemini_sim::content::Content;

fn main() {
    let mut out = "art/menu".to_string();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--out" {
            if let Some(path) = args.next() {
                out = path;
            }
        }
    }
    fs::create_dir_all(&out).expect("create out dir");
    let content = Content::embedded();
    let hero = content.hero.id.clone();
    let prefs = PaintPrefs::defaults(&content);
    let shots = [
        ("title", MenuShot::Title { focus: 0, notice: false, native_quit: true }),
        ("title_reset", MenuShot::Title { focus: 0, notice: true, native_quit: true }),
        ("heroes", MenuShot::Heroes { focus: 0 }),
        ("difficulty", MenuShot::Difficulty { focus: 0, hero_id: hero.clone(), notice: false }),
        ("pause_solo", MenuShot::Pause { focus: 0, hero_id: hero.clone(), live: false }),
        ("pause_coop", MenuShot::Pause { focus: 0, hero_id: hero.clone(), live: true }),
        ("settings", MenuShot::Settings { focus: 0, hero_id: hero.clone(), prefs: prefs.clone(), notice: false }),
        (
            "results_victory",
            MenuShot::Results { focus: 0, victory: true, waves: 5, kills: 42, seconds: 754.0, currency: 120.0 },
        ),
        (
            "results_defeat",
            MenuShot::Results { focus: 0, victory: false, waves: 3, kills: 18, seconds: 201.0, currency: 40.0 },
        ),
        ("crates", MenuShot::Crates { focus: 0, rarity: None, failed: false, can_pull: true, reason: String::new() }),
        (
            "crates_short",
            MenuShot::Crates {
                focus: 0,
                rarity: None,
                failed: true,
                can_pull: true,
                reason: "NEED 100 CREDITS HAVE 0".into(),
            },
        ),
        ("click_to_play", MenuShot::ClickToPlay),
    ];
    for (name, shot) in shots {
        let path = format!("{out}/{name}_1280x720.png");
        write_shot(&content, &shot, 1280, 720, &path);
        println!("wrote {path}");
    }
    let mut unlocked = content.clone();
    for hero in &mut unlocked.heroes {
        hero.owned = true;
    }
    let (rgb, _) = paint_menu(&unlocked, &MenuShot::Heroes { focus: 0 }, 1280, 720);
    let path = format!("{out}/heroes_unlocked_1280x720.png");
    write_png(&path, &rgb, 1280, 720);
    println!("wrote {path}");
    let heroes = MenuShot::Heroes { focus: 0 };
    for (width, height) in [(1366, 768), (2560, 1440)] {
        let (rgb, _) = paint_menu(&content, &heroes, width, height);
        let path = format!("{out}/heroes_{width}x{height}.png");
        write_png(&path, &rgb, width, height);
        println!("wrote {path}");
    }
    for (name, live) in [("pause_solo_1920x1080", false), ("pause_coop_1920x1080", true)] {
        let path = format!("{out}/{name}.png");
        write_shot(&content, &MenuShot::Pause { focus: 0, hero_id: hero.clone(), live }, 1920, 1080, &path);
        println!("wrote {path}");
    }
    let raster = content.ui.portrait.raster;
    for hero in &content.heroes {
        let rgb = raster_hero_portrait(&content, &hero.id, raster);
        let path = format!("{out}/portrait_{}.png", hero.id);
        write_png(&path, &rgb, raster, raster);
        println!("wrote {path}");
        let big = 640u32;
        let rgb = raster_hero_portrait(&content, &hero.id, big);
        let path = format!("{out}/portrait_{}_{big}.png", hero.id);
        write_png(&path, &rgb, big, big);
        println!("wrote {path}");
    }
}

/// Co-op pause leaves the live frame clear. The still bakes the veil the client draws over it.
fn write_shot(content: &gemini_sim::content::Content, shot: &MenuShot, width: u32, height: u32, path: &str) {
    let rgb = match shot {
        MenuShot::Pause { live: true, focus, hero_id } => coop_backdrop(content, *focus, hero_id, width, height),
        _ => paint_menu(content, shot, width, height).0,
    };
    write_png(path, &rgb, width, height);
}

fn coop_backdrop(content: &gemini_sim::content::Content, focus: usize, hero_id: &str, width: u32, height: u32) -> Vec<u8> {
    let solo = paint_menu_masked(
        content,
        &MenuShot::Pause { focus, hero_id: hero_id.to_string(), live: false },
        width,
        height,
    );
    let coop = paint_menu_masked(
        content,
        &MenuShot::Pause { focus, hero_id: hero_id.to_string(), live: true },
        width,
        height,
    );
    let keep = content.ui.layout.pause_keep.max(1e-3);
    let dim = content.ui.layout.coop_pause_dim.clamp(0.0, 1.0);
    let veil = content.palette.key(&content.ui.colors.veil);
    let mut rgb = Vec::with_capacity(coop.rgb.len());
    for (index, cover) in coop.alpha.iter().copied().enumerate() {
        if cover == 0 {
            for channel in 0..3 {
                let stored = solo.rgb[index * 3 + channel] as f32;
                let arena = (stored / keep).round().clamp(0.0, 255.0);
                let mixed = arena * (1.0 - dim) + veil[channel] * 255.0 * dim;
                rgb.push(mixed.round().clamp(0.0, 255.0) as u8);
            }
        } else {
            rgb.extend_from_slice(&coop.rgb[index * 3..index * 3 + 3]);
        }
    }
    rgb
}

fn write_png(path: &str, rgb: &[u8], width: u32, height: u32) {
    let mut child = Command::new("ffmpeg")
        .args(["-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{width}x{height}"), "-i", "pipe:0", path])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("ffmpeg");
    child.stdin.take().unwrap().write_all(rgb).expect("write frame");
    let status = child.wait().expect("ffmpeg wait");
    if !status.success() {
        panic!("ffmpeg failed for {path}");
    }
}
