//! Concept-versus-in-game stills for every approved cast.
//!
//! ```text
//! cargo run -p gemini-meshes --bin castshot -- --out art/casts --review /path/to/meshy/review
//! ```
//!
//! `--review` points at the handoff `side_<id>.png` sheets. The left half of each
//! sheet is the concept. The right panel is this build's fitted, tinted cast.
//! Bugs use the concept-sheet camera. The other casts use one three-quarter orbit.

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
            let half = concept.width() / 2;
            let left = image::imageops::crop_imm(&concept, 0, 0, half, concept.height()).to_image();
            let left = image::imageops::resize(&left, panel, panel, image::imageops::FilterType::Triangle);
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
