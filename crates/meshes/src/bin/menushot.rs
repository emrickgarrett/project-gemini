//! Menu stills. Every screen at 1280x720, plus the pause screen at 1920x1080.
//!
//! ```text
//! cargo run -p gemini-meshes --bin menushot -- --out /opt/cursor/artifacts/review/menu
//! ```

use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use gemini_meshes::{paint_menu, raster_hero_portrait, MenuShot, PaintPrefs};
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
        ("pause", MenuShot::Pause { focus: 0, hero_id: hero.clone() }),
        ("settings", MenuShot::Settings { focus: 0, hero_id: hero.clone(), prefs: prefs.clone(), notice: false }),
        (
            "results_victory",
            MenuShot::Results { focus: 0, victory: true, waves: 5, kills: 42, seconds: 754.0, currency: 120.0 },
        ),
        (
            "results_defeat",
            MenuShot::Results { focus: 0, victory: false, waves: 3, kills: 18, seconds: 201.0, currency: 40.0 },
        ),
        ("crates", MenuShot::Crates { focus: 0, rarity: None, failed: false, can_pull: true }),
        ("click_to_play", MenuShot::ClickToPlay),
    ];
    for (name, shot) in shots {
        let (rgb, _) = paint_menu(&content, &shot, 1280, 720);
        let path = format!("{out}/{name}_1280x720.png");
        write_png(&path, &rgb, 1280, 720);
        println!("wrote {path}");
    }
    let heroes = MenuShot::Heroes { focus: 0 };
    for (width, height) in [(1366, 768), (2560, 1440)] {
        let (rgb, _) = paint_menu(&content, &heroes, width, height);
        let path = format!("{out}/heroes_{width}x{height}.png");
        write_png(&path, &rgb, width, height);
        println!("wrote {path}");
    }
    let (rgb, _) = paint_menu(&content, &MenuShot::Pause { focus: 0, hero_id: hero }, 1920, 1080);
    let path = format!("{out}/pause_1920x1080.png");
    write_png(&path, &rgb, 1920, 1080);
    println!("wrote {path}");
    let raster = content.ui.portrait.raster;
    for hero in &content.heroes {
        let rgb = raster_hero_portrait(&content, &hero.id, raster);
        let path = format!("{out}/portrait_{}.png", hero.id);
        write_png(&path, &rgb, raster, raster);
        println!("wrote {path}");
    }
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
