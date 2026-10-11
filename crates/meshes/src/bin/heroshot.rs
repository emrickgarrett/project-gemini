//! First-person still of the starter hero's arms and pulse rifle.
//!
//! ```text
//! cargo run -p gemini-meshes --bin heroshot -- --out art/heroes/vex_fp.png
//! ```

use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use gemini_meshes::{painted_pixels, raster_viewmodel};

fn main() {
    let mut out = "art/heroes/vex_fp.png".to_string();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--out" {
            if let Some(path) = args.next() {
                out = path;
            }
        }
    }
    if let Some(dir) = std::path::Path::new(&out).parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir).expect("create out dir");
        }
    }
    let accent = starter_accent();
    let rgb = raster_viewmodel(1024, 1024, accent);
    let painted = painted_pixels(&rgb);
    let orange = rgb
        .chunks(3)
        .filter(|p| p[0] > 150 && p[1] > 40 && p[1] < 170 && p[2] < 90)
        .count();
    let core = rgb
        .chunks(3)
        .filter(|p| p[0] > 30 && p[0] < 80 && p[1] > 180 && p[2] > 160)
        .count();
    if painted < 8000 || orange < 200 || core < 40 {
        eprintln!("viewmodel still failed ({painted} px, orange {orange}, core {core})");
        std::process::exit(1);
    }
    write_png(&out, 1024, 1024, &rgb);
    println!("wrote {out} ({painted} px, orange {orange}, core {core})");
}

fn starter_accent() -> [f32; 4] {
    let text = include_str!("../../../../content/characters.toml");
    let root: toml::Value = toml::from_str(text).expect("characters.toml");
    let starter = root
        .get("unlock")
        .and_then(|v| v.get("starter"))
        .and_then(|v| v.as_str())
        .expect("starter");
    let heroes = root.get("hero").and_then(|v| v.as_array()).expect("[[hero]]");
    let row = heroes
        .iter()
        .find(|hero| hero.get("id").and_then(|v| v.as_str()) == Some(starter))
        .unwrap_or_else(|| panic!("starter {starter}"));
    let hex = row.get("accent").and_then(|v| v.as_str()).expect("accent");
    let hex = hex.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("accent hex") as f32 / 255.0;
    [byte(0), byte(2), byte(4), 1.0]
}

fn write_png(path: &str, w: u32, h: u32, rgb: &[u8]) {
    let mut child = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            &format!("{w}x{h}"),
            "-i",
            "pipe:0",
            path,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("ffmpeg");
    child.stdin.take().unwrap().write_all(rgb).expect("write frame");
    let status = child.wait().expect("ffmpeg wait");
    if !status.success() {
        panic!("ffmpeg failed");
    }
}
