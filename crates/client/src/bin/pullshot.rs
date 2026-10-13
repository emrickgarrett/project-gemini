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
    let tier = &content.crate_reveal.legendary;
    let freeze = tier.freeze.expect("legendary freeze");
    let marks = [
        tier.drop[0],
        (tier.crack[0] + tier.crack[1]) * 0.5,
        (freeze[0] + freeze[1]) * 0.5,
        tier.burst,
        (tier.rise[0] + tier.rise[1]) * 0.5,
        tier.card,
    ];
    let mut strip = Vec::new();
    for (index, t) in marks.iter().enumerate() {
        let rgb = paint(&content, &legendary, *t, W, H);
        let path = format!("{out}/crate_legendary_{index:02}.png");
        write_png(&path, &rgb);
        println!("wrote {path} t={t:.2}");
        if strip.is_empty() {
            strip = rgb;
        } else {
            let mut next = Vec::with_capacity(strip.len() + rgb.len());
            for row in 0..H {
                let start = (row * W * 3) as usize;
                let end = start + (W * 3) as usize;
                let wide = W as usize * (index as usize) * 3;
                next.extend_from_slice(&strip[row as usize * wide..row as usize * wide + wide]);
                next.extend_from_slice(&rgb[start..end]);
            }
            strip = next;
        }
    }
    let strip_path = format!("{out}/crate_legendary_strip.png");
    write_png_size(&strip_path, &strip, W * marks.len() as u32, H);
    println!("wrote {strip_path}");
}

fn write_png(path: &str, rgb: &[u8]) {
    write_png_size(path, rgb, W, H);
}

fn write_png_size(path: &str, rgb: &[u8], width: u32, height: u32) {
    let mut child = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            &format!("{width}x{height}"),
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
