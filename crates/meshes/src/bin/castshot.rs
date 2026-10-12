//! Concept-versus-in-game stills for every approved cast.
//!
//! ```text
//! cargo run -p gemini-meshes --bin castshot -- --out art/casts --review /path/to/meshy/review
//! ```
//!
//! `--review` points at the handoff `side_<id>.png` sheets. The left panel is the
//! concept. The right panel is this build's fitted cast at that sheet's orbit.

use std::env;
use std::fs;
use std::path::PathBuf;

use gemini_meshes::raster_model;

const IDS: &[&str] = &[
    "vex_body",
    "vex_gun",
    "nyra_body",
    "nyra_gun",
    "brakk_body",
    "brakk_gun",
    "brakk_shield",
    "kip_body",
    "kip_gun",
    "swarmer",
    "spitter",
    "brute",
    "brood_mother",
    "hive_tyrant",
];

fn main() {
    let mut out = PathBuf::from("art/casts");
    let mut review: Option<PathBuf> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(args.next().expect("--out path")),
            "--review" => review = Some(PathBuf::from(args.next().expect("--review path"))),
            other => panic!("unknown arg {other}"),
        }
    }
    fs::create_dir_all(&out).expect("create out dir");
    let panel = 512u32;
    for id in IDS {
        let game = raster_model(id, panel, panel);
        let game_img = image::RgbImage::from_raw(panel, panel, game).expect("rgb");
        let side = review.as_ref().map(|dir| dir.join(format!("side_{id}.png")));
        let sheet = if let Some(path) = side.as_ref().filter(|path| path.is_file()) {
            let concept = image::open(path).expect("review sheet").to_rgb8();
            let left = fit_panel(&first_panel(&concept), panel);
            let mut canvas = image::RgbImage::from_pixel(panel * 2 + 8, panel, image::Rgb([0x8A, 0x8A, 0x8A]));
            image::imageops::overlay(&mut canvas, &left, 0, 0);
            image::imageops::overlay(&mut canvas, &game_img, (panel + 8) as i64, 0);
            canvas
        } else {
            game_img
        };
        let path = out.join(format!("{id}.png"));
        sheet.save(&path).expect("write png");
        println!("wrote {}", path.display());
    }
}

/// First content column of a side sheet. Gutters are the flat review grey.
fn first_panel(img: &image::RgbImage) -> image::RgbImage {
    let gutter = |x: u32| -> bool {
        let h = img.height();
        let mut gray = 0u32;
        for y in 0..h {
            let p = img.get_pixel(x, y);
            if p[0].abs_diff(128) <= 4 && p[1].abs_diff(128) <= 4 && p[2].abs_diff(128) <= 4 {
                gray += 1;
            }
        }
        gray * 100 >= h * 96
    };
    let w = img.width();
    let mut x = 0u32;
    while x < w && gutter(x) {
        x += 1;
    }
    let x0 = x;
    while x < w && !gutter(x) {
        x += 1;
    }
    let x1 = x.max(x0 + 1);
    image::imageops::crop_imm(img, x0, 0, x1 - x0, img.height()).to_image()
}

fn fit_panel(src: &image::RgbImage, panel: u32) -> image::RgbImage {
    let mut canvas = image::RgbImage::from_pixel(panel, panel, image::Rgb([0x80, 0x80, 0x80]));
    let scale = (panel as f32 / src.width() as f32).min(panel as f32 / src.height() as f32);
    let nw = ((src.width() as f32) * scale).round().max(1.0) as u32;
    let nh = ((src.height() as f32) * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(src, nw, nh, image::imageops::FilterType::Triangle);
    image::imageops::overlay(
        &mut canvas,
        &resized,
        ((panel - nw) / 2) as i64,
        ((panel - nh) / 2) as i64,
    );
    canvas
}
