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
    catalog, missing_glow_keys, missing_glow_keys_open, paint_away, paint_reveal, painted_pixels, raster_bug, raster_in_arena,
    raster_rail_edge, raster_top, reveal_still_is_flat, still, still_color_stats, still_open, still_top, AwayShot,
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
    let distance = Content::embedded().arena_dressing.review_distance;
    let far_dir = format!("{out}/at_15m");
    fs::create_dir_all(&far_dir).expect("create distance dir");
    for id in &ids {
        let rgb = raster_in_arena(id, panel, panel);
        let painted = painted_pixels(&rgb);
        if painted < 40 {
            eprintln!("{id} arena still failed ({painted} px at {distance} m)");
            failed = true;
        }
        let path = format!("{far_dir}/{id}.png");
        write_png(&path, panel, panel, &rgb);
        println!("wrote {path} ({painted} px, {distance} m)");
    }
    let rail = raster_rail_edge(panel, panel);
    let rail_path = format!("{out}/rail_edge.png");
    write_png(&rail_path, panel, panel, &rail);
    println!("wrote {rail_path}");
    if write_reveals(&out) {
        failed = true;
    }
    if write_away(&out) {
        failed = true;
    }
    if failed {
        std::process::exit(1);
    }
}

fn write_reveals(out: &str) -> bool {
    let mut flat = false;
    let mut guard = |name: &str, rgb: &[u8]| {
        if reveal_still_is_flat(rgb) {
            let stats = still_color_stats(rgb);
            eprintln!(
                "{name} is mostly one color (unique {}, variance {:.1}, dominant {:.3})",
                stats.unique, stats.variance, stats.dominant
            );
            flat = true;
        }
    };
    const W: u32 = 960;
    const H: u32 = 540;
    let content = Content::embedded();
    let mid = |window: [f32; 2]| (window[0] + window[1]) * 0.5;
    for rarity in ["common", "rare", "epic", "legendary"] {
        let planned = plan(&content, rarity, None).expect(rarity);
        let rgb = paint_reveal(&content, &planned, planned.card_at, W, H);
        guard(&format!("crate_card_{rarity}"), &rgb);
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
        guard(&format!("crate_legendary_{name}"), &rgb);
        let path = format!("{out}/crate_legendary_{name}.png");
        write_png(&path, W, H, &rgb);
        println!("wrote {path} t={t:.2}");
    }
    let rare = content.crate_reveal.tier("rare").expect("rare tier");
    let teased = plan(&content, "rare", Some("epic")).expect("rare fakeout");
    let rgb = paint_reveal(&content, &teased, mid(rare.fakeout_window), W, H);
    guard("crate_rare_fakeout", &rgb);
    let path = format!("{out}/crate_rare_fakeout.png");
    write_png(&path, W, H, &rgb);
    println!("wrote {path}");
    flat
}

fn write_away(out: &str) -> bool {
    let mut flat = false;
    let mut guard = |name: &str, rgb: &[u8]| {
        if reveal_still_is_flat(rgb) {
            let stats = still_color_stats(rgb);
            eprintln!(
                "{name} is mostly one color (unique {}, variance {:.1}, dominant {:.3})",
                stats.unique, stats.variance, stats.dominant
            );
            flat = true;
        }
    };
    const W: u32 = 960;
    const H: u32 = 540;
    let content = Content::embedded();
    let hud = &content.presentation.hud.away;
    let fill = 1.0 - content.coop.host_away.detect_after_seconds / content.coop.host_away.end_after_seconds;
    let countdown = content.coop.host_away.resume_countdown_seconds.max(1.0);
    let shots = [
        (
            "host_away_banner",
            AwayShot {
                banner: true,
                title: hud.coop.text.clone(),
                show_timer: hud.coop.show_timer,
                timer_fill: fill,
                digit: None,
                digit_u: 0.0,
                look: 1.0,
            },
        ),
        (
            "host_away_paused",
            AwayShot {
                banner: true,
                title: hud.solo.text.clone(),
                show_timer: hud.solo.show_timer,
                timer_fill: 0.0,
                digit: None,
                digit_u: 0.0,
                look: 1.0,
            },
        ),
        (
            "host_away_count_3",
            AwayShot {
                banner: false,
                title: String::new(),
                show_timer: false,
                timer_fill: 0.0,
                digit: Some(3),
                digit_u: 0.0,
                look: (3.0 / countdown).clamp(0.0, 1.0),
            },
        ),
        (
            "host_away_count_2",
            AwayShot {
                banner: false,
                title: String::new(),
                show_timer: false,
                timer_fill: 0.0,
                digit: Some(2),
                digit_u: 0.0,
                look: (2.0 / countdown).clamp(0.0, 1.0),
            },
        ),
        (
            "host_away_count_1",
            AwayShot {
                banner: false,
                title: String::new(),
                show_timer: false,
                timer_fill: 0.0,
                digit: Some(1),
                digit_u: 0.0,
                look: (1.0 / countdown).clamp(0.0, 1.0),
            },
        ),
    ];
    for (name, shot) in shots {
        let rgb = paint_away(&content, &shot, W, H);
        guard(name, &rgb);
        let path = format!("{out}/{name}.png");
        write_png(&path, W, H, &rgb);
        println!("wrote {path}");
        let mut clear = shot;
        clear.look = 0.0;
        let rgb = paint_away(&content, &clear, W, H);
        let clear_name = format!("{name}_clear");
        guard(&clear_name, &rgb);
        let path = format!("{out}/{clear_name}.png");
        write_png(&path, W, H, &rgb);
        println!("wrote {path}");
    }
    flat
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

#[cfg(test)]
mod vex_reference {
    use std::process::Command;

    use gemini_meshes::{raster_viewmodel, ViewmodelFrame};
    use gemini_sim::content::{hex_srgb, Content};

    const W: u32 = 1280;
    const H: u32 = 720;
    /// Per-channel delta that still counts as the same pixel.
    const CHANNEL_TOL: u8 = 12;
    /// Idle is the #12 still. A missing glove or stripe moves tens of thousands of pixels.
    const IDLE_MAX: usize = 400;
    /// Overdrive keeps that body. The #15 bloom is wider than #12's sphere, so the tip may differ.
    const OVERDRIVE_MAX: usize = 16_000;

    fn decode_png(path: &str) -> Vec<u8> {
        let out = Command::new("ffmpeg")
            .args(["-v", "error", "-i", path, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .output()
            .unwrap_or_else(|err| panic!("ffmpeg: {err}"));
        assert!(out.status.success(), "ffmpeg failed on {path}");
        out.stdout
    }

    fn drifted(fresh: &[u8], reference: &[u8]) -> usize {
        assert_eq!(fresh.len(), reference.len(), "rgb size");
        fresh
            .chunks(3)
            .zip(reference.chunks(3))
            .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > CHANNEL_TOL))
            .count()
    }

    fn frame(overdrive: bool) -> ViewmodelFrame {
        let hero = &Content::embedded().hero;
        let color = hero.muzzle_flash_color;
        ViewmodelFrame {
            accent: hex_srgb(&hero.accent),
            offset: hero.viewmodel_offset,
            fov: hero.viewmodel_fov,
            overdrive,
            glow_radius: hero.muzzle_flash_radius,
            glow_core: hero.muzzle_flash_core,
            glow_falloff: hero.muzzle_flash_falloff,
            glow_tip_px: hero.muzzle_flash_tip_px,
            glow_color: [color[0], color[1], color[2], 1.0],
        }
    }

    #[test]
    fn vex_fp_stills_stay_near_the_polished_reference() {
        // Approved stills live in reference/. art/heroes is what heroshot writes.
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../art/heroes/reference");
        let idle_ref = decode_png(&format!("{root}/vex_fp_idle.png"));
        let drive_ref = decode_png(&format!("{root}/vex_fp_overdrive.png"));
        assert_eq!(idle_ref.len(), (W * H * 3) as usize);
        let idle = drifted(&raster_viewmodel(W, H, &frame(false)), &idle_ref);
        let drive = drifted(&raster_viewmodel(W, H, &frame(true)), &drive_ref);
        assert!(idle <= IDLE_MAX, "vex_fp_idle drifted {idle} px from the #12 reference");
        assert!(
            drive <= OVERDRIVE_MAX,
            "vex_fp_overdrive drifted {drive} px from the #12 reference"
        );
    }
}
