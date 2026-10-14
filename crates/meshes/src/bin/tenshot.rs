//! 1280x720 ten-pull stills. Sizes and colors come from the content tables.

use std::path::Path;

use gemini_meshes::{paint_grid, paint_refund};
use gemini_sim::content::Content;
use gemini_sim::reveal::{GridCard, GridRun};
use image::{ImageBuffer, RgbImage};

fn main() {
    let content = Content::embedded();
    let width = content.ui.layout.fit_width.round() as u32;
    let height = content.ui.layout.fit_height.round() as u32;
    let out = Path::new("art/ten_pull");
    std::fs::create_dir_all(out).expect("art/ten_pull");
    let pods = content.banners.cost.pods as usize;
    let common: Vec<Option<&str>> = vec![Some("common"); pods];
    write(out.join("all_common.png"), &paint_grid(&content, &common, width, height).expect("all common"), width, height);

    let mut one = common.clone();
    one[pods / 2] = Some("legendary");
    write(out.join("one_legendary.png"), &paint_grid(&content, &one, width, height).expect("one legendary"), width, height);

    let mut cards = vec![GridCard { rarity: "common".into(), fakeout: None }; pods];
    cards[pods / 2].rarity = "legendary".into();
    let mut run = GridRun::start(&content, &cards).expect("grid");
    let flip = content.crate_reveal.grid.fast_flip;
    for _ in 0..pods - 1 {
        run.advance(flip);
    }
    let shown: Vec<Option<&str>> = (0..pods).map(|index| run.shown_face(index)).collect();
    write(out.join("best_face_down.png"), &paint_grid(&content, &shown, width, height).expect("held back"), width, height);
    write(out.join("refund.png"), &paint_refund(&content, width, height).expect("refund"), width, height);
}

fn write(path: std::path::PathBuf, rgb: &[u8], width: u32, height: u32) {
    let image: RgbImage = ImageBuffer::from_raw(width, height, rgb.to_vec()).expect("frame size");
    image.save(&path).expect("png");
    println!("wrote {}", path.display());
}
