//! Save a concept-sheet still of each bug: a 1024 color panel beside a 1024 silhouette.
//!
//! ```text
//! cargo run -p gemini-meshes --bin bugshots -- --out art/bugs
//! ```
//!
//! Exits with an error if a glow part contributes no pixels in its palette color.

use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use gemini_meshes::{catalog, missing_glow_keys, painted_pixels, raster_bug, still, still_open};

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
    let ids: Vec<String> = catalog().iter().map(|visual| visual.id.clone()).collect();
    let panel = 1024u32;
    let mut failed = false;
    for id in &ids {
        let (w, h, rgb) = still(id, panel);
        let color = raster_bug(id, panel, panel);
        let painted = painted_pixels(&color);
        let missing = missing_glow_keys(id, &color);
        if painted < 800 || !missing.is_empty() {
            eprintln!("{id} still failed ({painted} px, missing glow {missing:?})");
            failed = true;
        }
        let path = format!("{out}/{id}.png");
        write_png(&path, w, h, &rgb);
        println!("wrote {path} ({painted} px)");
    }
    for visual in catalog() {
        if visual.camera_open.is_none() {
            continue;
        }
        let (ow, oh, open) = still_open(&visual.id, panel);
        let open_painted = painted_pixels(&open);
        let open_missing = missing_glow_keys(&visual.id, &open);
        if open_painted < 800 || !open_missing.is_empty() {
            eprintln!(
                "{}_open still failed ({open_painted} px, missing glow {open_missing:?})",
                visual.id
            );
            failed = true;
        }
        let open_path = format!("{out}/{}_open.png", visual.id);
        write_png(&open_path, ow, oh, &open);
        println!("wrote {open_path} ({open_painted} px)");
    }
    if failed {
        std::process::exit(1);
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
