//! Menu stills. Every screen at 1280x720, plus the pause screen at 1920x1080.
//!
//! ```text
//! cargo run -p gemini-meshes --bin menushot -- --out /opt/cursor/artifacts/review/menu
//! ```

use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use gemini_meshes::{paint_coop_pause, paint_menu, raster_hero_portrait, MenuShot, PaintPrefs};
use gemini_sim::bank::BankShow;
use gemini_sim::content::{currency_reward, milestone_payout, win_payout, Content};

fn main() {
    let mut out = "art/menu".to_string();
    let mut bank_dir: Option<String> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--out" {
            if let Some(path) = args.next() {
                out = path;
            }
        } else if arg == "--bank-dir" {
            bank_dir = args.next();
        }
    }
    let content = Content::embedded();
    if let Some(dir) = bank_dir {
        fs::create_dir_all(&dir).expect("create bank dir");
        for (name, shot) in bank_shots(&content) {
            let path = format!("{dir}/{name}_1280x720.png");
            write_shot(&content, &shot, 1280, 720, &path);
            println!("wrote {path}");
        }
        return;
    }
    fs::create_dir_all(&out).expect("create out dir");
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
            MenuShot::Results { focus: 0, show: BankShow::Win, waves: 5, kills: 42, seconds: 754.0, earned: 120.0, bonus: 0.0, balance: Some(120.0) },
        ),
        (
            "results_defeat",
            MenuShot::Results { focus: 0, show: BankShow::End, waves: 3, kills: 18, seconds: 201.0, earned: 40.0, bonus: 0.0, balance: Some(40.0) },
        ),
        ("crates", MenuShot::Crates { focus: 0, rarity: None, failed: false, credits: None, reason: String::new(), show_last: false }),
        (
            "crates_short",
            MenuShot::Crates {
                focus: 0,
                rarity: None,
                failed: true,
                credits: None,
                reason: "NEED 100 CREDITS HAVE 0".into(),
                show_last: false,
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

fn bank_shots(content: &Content) -> Vec<(&'static str, MenuShot)> {
    let tier = content.tier(&content.ui.settings.default_difficulty).currency;
    let first_boss = content.waves.wave.iter().find(|wave| wave.boss.is_some()).map(|wave| wave.n).expect("a boss wave");
    let end_wave = first_boss.checked_sub(1).expect("a wave before the first boss");
    let end_earned = currency_reward(&content.currency, end_wave, tier, false, 0);
    let open_wave = content.waves.wave.iter().map(|wave| wave.n).min().expect("waves");
    let quit_earned = 0.0;
    let last = content.waves.wave.iter().map(|wave| wave.n).max().expect("waves");
    let bosses = content.waves.wave.iter().filter(|wave| wave.boss.is_some()).count() as u32;
    let win_earned = currency_reward(&content.currency, last, tier, true, bosses);
    let mut bonus = win_payout(&content.currency, tier);
    let mut wave = 1u32;
    while wave <= last {
        bonus += milestone_payout(&content.currency, wave, tier);
        wave += 1;
    }
    let have = content.banners.dev.starting_credits;
    let seconds = content.waves.budget.breather_seconds;
    let kills = content.waves.budget.base as u32;
    vec![
        (
            "end",
            MenuShot::Results {
                focus: 0,
                show: BankShow::End,
                waves: end_wave,
                kills: kills * end_wave,
                seconds: seconds * end_wave as f32,
                earned: end_earned,
                bonus: 0.0,
                balance: Some(have + end_earned),
            },
        ),
        (
            "quit",
            MenuShot::Results {
                focus: 0,
                show: BankShow::Quit,
                waves: open_wave,
                kills,
                seconds,
                earned: quit_earned,
                bonus: 0.0,
                balance: Some(have),
            },
        ),
        (
            "win",
            MenuShot::Results {
                focus: 0,
                show: BankShow::Win,
                waves: last,
                kills: kills * last,
                seconds: seconds * last as f32,
                earned: win_earned,
                bonus,
                balance: Some(have + win_earned),
            },
        ),
    ]
}

/// Co-op pause leaves the live frame clear. The still lays the veil over the dimmed solo backdrop.
fn write_shot(content: &gemini_sim::content::Content, shot: &MenuShot, width: u32, height: u32, path: &str) {
    let rgb = match shot {
        MenuShot::Pause { live: true, focus, hero_id } => paint_coop_pause(content, *focus, hero_id, width, height),
        _ => paint_menu(content, shot, width, height).0,
    };
    write_png(path, &rgb, width, height);
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
