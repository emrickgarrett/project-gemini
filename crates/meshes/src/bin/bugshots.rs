//! Save a front three-quarter still of each bug.
//!
//! ```text
//! cargo run -p gemini-meshes --bin bugshots -- --out art/bugs
//! ```

use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use gemini_meshes::{painted_pixels, raster_bug};

fn main() {
    let mut out = "art/bugs".to_string();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--out" {
            if let Some(dir) = args.next() {
                out = dir;
            }
        }
    }
    fs::create_dir_all(&out).expect("create out dir");
    let ids = ["swarmer", "spitter", "brute", "brood_mother", "hive_tyrant"];
    let (w, h) = (640u32, 480u32);
    for id in ids {
        let rgb = raster_bug(id, w, h);
        let painted = painted_pixels(&rgb);
        if painted < 200 {
            eprintln!("{id} preview is nearly blank ({painted} px)");
            std::process::exit(1);
        }
        let path = format!("{out}/{id}.png");
        write_png(&path, w, h, &rgb);
        println!("wrote {path} ({painted} px)");
    }
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
        .stderr(Stdio::piped())
        .spawn()
        .expect("ffmpeg");
    child.stdin.take().unwrap().write_all(rgb).expect("write frame");
    let status = child.wait().expect("ffmpeg wait");
    if !status.success() {
        eprintln!("ffmpeg failed for {path}");
        std::process::exit(1);
    }
}
