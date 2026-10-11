//! Rarity cards and a legendary reveal sequence.
//!
//! ```text
//! cargo run -p gemini-client --features pullshot --bin pullshot -- --out /opt/cursor/artifacts/review
//! ```

use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use gemini_client::reveal_paint::paint;
use gemini_sim::content::Content;
use gemini_sim::reveal::plan;

const W: u32 = 960;
const H: u32 = 540;

fn main() {
    let mut out = "/opt/cursor/artifacts/review".to_string();
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
    for rarity in ["common", "rare", "epic", "legendary"] {
        let planned = plan(&content, rarity, None).expect(rarity);
        let rgb = paint(&content, &planned, planned.card_at, W, H);
        let path = format!("{out}/crate_card_{rarity}.png");
        write_png(&path, &rgb);
        println!("wrote {path}");
    }
    let legendary = plan(&content, "legendary", None).expect("legendary");
    let mut t = 0.0f32;
    let mut index = 0u32;
    while t <= legendary.card_at + 0.001 {
        let rgb = paint(&content, &legendary, t, W, H);
        let path = format!("{out}/crate_legendary_{index:02}.png");
        write_png(&path, &rgb);
        println!("wrote {path} t={t:.2}");
        t += 0.25;
        index += 1;
    }
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
        panic!("ffmpeg failed for {path}");
    }
}
