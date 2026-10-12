//! First-person stills of Vex. Idle and Overdrive, 1280x720.
//!
//! ```text
//! cargo run -p gemini-meshes --bin heroshot -- --out art/heroes
//! ```

use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use gemini_meshes::{raster_viewmodel, ViewmodelFrame};

const W: u32 = 1280;
const H: u32 = 720;
const BG: [u8; 3] = [0x8A, 0x8A, 0x8A];

fn main() {
    let mut out = "art/heroes".to_string();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--out" {
            if let Some(path) = args.next() {
                out = path;
            }
        }
    }
    fs::create_dir_all(&out).expect("create out dir");
    let vex = vex_viewmodel();
    let idle = raster_viewmodel(
        W,
        H,
        &ViewmodelFrame {
            overdrive: false,
            ..vex
        },
    );
    let drive = raster_viewmodel(
        W,
        H,
        &ViewmodelFrame {
            overdrive: true,
            ..vex
        },
    );
    assert_lower_right(&idle, "idle");
    assert_overdrive(&idle, &drive);
    let idle_path = format!("{out}/vex_fp_idle.png");
    let drive_path = format!("{out}/vex_fp_overdrive.png");
    write_png(&idle_path, &idle);
    write_png(&drive_path, &drive);
    println!("wrote {idle_path}");
    println!("wrote {drive_path}");
}

fn assert_lower_right(rgb: &[u8], label: &str) {
    let (left, right, top, bottom) = halves(rgb);
    if right < left * 3 / 2 || bottom < top {
        eprintln!("{label} viewmodel is not lower-right (L{left} R{right} T{top} B{bottom})");
        std::process::exit(1);
    }
}

fn assert_overdrive(idle: &[u8], drive: &[u8]) {
    let idle_orange = orange_pixels(idle);
    let drive_orange = orange_pixels(drive);
    if drive_orange < idle_orange + 400 {
        eprintln!("overdrive barrel glow too weak ({drive_orange} vs idle {idle_orange})");
        std::process::exit(1);
    }
    let corner = &drive[0..3];
    if corner == BG {
        eprintln!("overdrive still is missing the screen-edge tint");
        std::process::exit(1);
    }
}

fn halves(rgb: &[u8]) -> (u32, u32, u32, u32) {
    let mut left = 0u32;
    let mut right = 0u32;
    let mut top = 0u32;
    let mut bottom = 0u32;
    for y in 0..H {
        for x in 0..W {
            let i = ((y * W + x) * 3) as usize;
            if rgb[i..i + 3] == BG {
                continue;
            }
            if x < W / 2 {
                left += 1;
            } else {
                right += 1;
            }
            if y < H / 2 {
                top += 1;
            } else {
                bottom += 1;
            }
        }
    }
    (left, right, top, bottom)
}

fn orange_pixels(rgb: &[u8]) -> u32 {
    rgb.chunks(3)
        .filter(|p| p[0] > 160 && p[1] > 40 && p[1] < 190 && p[2] < 120)
        .count() as u32
}

fn vex_viewmodel() -> ViewmodelFrame {
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
    let accent = parse_hex(hex);
    let offset = row
        .get("viewmodel_offset")
        .and_then(|v| v.as_array())
        .filter(|v| v.len() == 3)
        .map(|v| [num(&v[0]), num(&v[1]), num(&v[2])])
        .expect("vex is missing viewmodel_offset");
    let fov = row
        .get("viewmodel_fov")
        .map(num)
        .expect("vex is missing viewmodel_fov");
    let color = row
        .get("muzzle_flash_color")
        .and_then(|v| v.as_array())
        .filter(|v| v.len() == 3)
        .map(|v| [num(&v[0]), num(&v[1]), num(&v[2]), 1.0])
        .expect("vex is missing muzzle_flash_color");
    ViewmodelFrame {
        accent,
        offset,
        fov,
        overdrive: false,
        glow_radius: num(row.get("muzzle_flash_radius").expect("muzzle_flash_radius")),
        glow_core: num(row.get("muzzle_flash_core").expect("muzzle_flash_core")),
        glow_falloff: num(row.get("muzzle_flash_falloff").expect("muzzle_flash_falloff")),
        glow_tip_px: num(row.get("muzzle_flash_tip_px").expect("muzzle_flash_tip_px")),
        glow_color: color,
    }
}

fn parse_hex(hex: &str) -> [f32; 4] {
    let hex = hex.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("accent hex") as f32 / 255.0;
    [byte(0), byte(2), byte(4), 1.0]
}

fn num(value: &toml::Value) -> f32 {
    value
        .as_float()
        .map(|n| n as f32)
        .or_else(|| value.as_integer().map(|n| n as f32))
        .expect("number")
}

fn write_png(path: &str, rgb: &[u8]) {
    let mut child = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            &format!("{W}x{H}"),
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
