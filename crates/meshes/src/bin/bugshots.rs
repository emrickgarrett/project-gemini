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

use gemini_meshes::{
    catalog, missing_glow_keys, missing_glow_keys_open, paint_reveal, painted_pixels, raster_bug, raster_top, still,
    still_open, still_top,
};
use gemini_sim::content::Content;
use gemini_sim::reveal::plan;

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
        if id == "brood_mother" {
            let (tw, th, top) = still_top(id, panel);
            let top_color = raster_top(id, panel, panel);
            let top_painted = painted_pixels(&top_color);
            let top_missing = missing_glow_keys(id, &top_color);
            if top_painted < 800 || !top_missing.is_empty() {
                eprintln!("{id} top still failed ({top_painted} px, missing glow {top_missing:?})");
                failed = true;
            }
            let top_path = format!("{out}/{id}_top.png");
            write_png(&top_path, tw, th, &top);
            println!("wrote {top_path} ({top_painted} px)");
        }
    }
    for visual in catalog() {
        if visual.camera_open.is_none() {
            continue;
        }
        let (ow, oh, open) = still_open(&visual.id, panel);
        let open_painted = painted_pixels(&open);
        let open_missing = missing_glow_keys_open(&visual.id, &open);
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
    write_reveals(&out);
    if failed {
        std::process::exit(1);
    }
}

fn write_reveals(out: &str) {
    const W: u32 = 960;
    const H: u32 = 540;
    let content = Content::embedded();
    let mid = |window: [f32; 2]| (window[0] + window[1]) * 0.5;
    for rarity in ["common", "rare", "epic", "legendary"] {
        let planned = plan(&content, rarity, None).expect(rarity);
        let rgb = paint_reveal(&content, &planned, planned.card_at, W, H);
        let path = format!("{out}/crate_card_{rarity}.png");
        write_png(&path, W, H, &rgb);
        println!("wrote {path}");
    }
    let legendary = plan(&content, "legendary", None).expect("legendary");
    let tier = content.crate_reveal.tier("legendary").expect("legendary tier");
    let frames = [
        ("drop", mid(tier.drop)),
        ("crack", mid(tier.crack)),
        ("freeze", mid(tier.freeze.expect("legendary freeze"))),
        ("burst", tier.burst),
        ("rise", mid(tier.rise)),
        ("card", tier.card),
    ];
    for (name, t) in frames {
        let rgb = paint_reveal(&content, &legendary, t, W, H);
        let path = format!("{out}/crate_legendary_{name}.png");
        write_png(&path, W, H, &rgb);
        println!("wrote {path} t={t:.2}");
    }
    let rare = content.crate_reveal.tier("rare").expect("rare tier");
    let teased = plan(&content, "rare", Some("epic")).expect("rare fakeout");
    let rgb = paint_reveal(&content, &teased, mid(rare.fakeout_window), W, H);
    let path = format!("{out}/crate_rare_fakeout.png");
    write_png(&path, W, H, &rgb);
    println!("wrote {path}");
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
