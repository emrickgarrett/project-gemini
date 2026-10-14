//! 1280x720 ten-pull stills. The same painter draws the live reveal screen.

use std::path::Path;

use gemini_meshes::{paint_grid, paint_menu, paint_refund, GridStill, MenuShot, RefundStill};
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
    let visor = content.crate_item("visor").expect("visor").id.clone();
    let mark = content.crate_item("mark").expect("mark").id.clone();
    let backs: Vec<Option<&str>> = vec![None; pods];
    write(out.join("all_backs.png"), &paint_grid(&content, &GridStill { items: &backs, ready: false, again: true, focus: 0, shortfall: "", failure: "" }, width, height).expect("backs").0, width, height);
    let common: Vec<Option<&str>> = vec![Some(visor.as_str()); pods];
    write(out.join("all_common.png"), &paint_grid(&content, &GridStill { items: &common, ready: true, again: true, focus: 0, shortfall: "", failure: "" }, width, height).expect("all common").0, width, height);

    let mut one = common.clone();
    one[pods / 2] = Some(mark.as_str());
    write(out.join("one_legendary.png"), &paint_grid(&content, &GridStill { items: &one, ready: true, again: true, focus: 0, shortfall: "", failure: "" }, width, height).expect("one legendary").0, width, height);
    write(
        out.join("pull_failed.png"),
        &paint_grid(
            &content,
            &GridStill { items: &one, ready: true, again: true, focus: 0, shortfall: "", failure: &content.ui.copy.pull_failed },
            width,
            height,
        )
        .expect("pull failed")
        .0,
        width,
        height,
    );

    let mut cards = vec![GridCard { rarity: "common".into(), fakeout: None, item: visor.clone() }; pods];
    cards[pods / 2].rarity = "legendary".into();
    cards[pods / 2].item = mark.clone();
    let mut run = GridRun::start(&content, &cards).expect("grid");
    let flip = content.crate_reveal.grid.fast_flip;
    for _ in 0..pods - 1 {
        run.advance(flip);
    }
    let shown: Vec<Option<&str>> = (0..pods).map(|index| run.shown_item(index)).collect();
    write(out.join("best_face_down.png"), &paint_grid(&content, &GridStill { items: &shown, ready: false, again: true, focus: 0, shortfall: "", failure: "" }, width, height).expect("held back").0, width, height);
    run.advance(run.plan.best_plan.card_at + flip);
    let flipped: Vec<Option<&str>> = (0..pods).map(|index| run.shown_item(index)).collect();
    write(out.join("best_flipped.png"), &paint_grid(&content, &GridStill { items: &flipped, ready: true, again: true, focus: 0, shortfall: "", failure: "" }, width, height).expect("flipped").0, width, height);
    write(out.join("refund.png"), &paint_refund(&content, &RefundStill { again: true, focus: 0, shortfall: "", failure: "" }, width, height).expect("refund").0, width, height);
    let (short, _) = paint_menu(
        &content,
        &MenuShot::Crates {
            focus: 0,
            rarity: None,
            failed: false,
            credits: Some(content.banners.cost.single as f32 / 2.0),
            reason: String::new(),
        },
        width,
        height,
    );
    write(out.join("no_credits.png"), &short, width, height);
}

fn write(path: std::path::PathBuf, rgb: &[u8], width: u32, height: u32) {
    let image: RgbImage = ImageBuffer::from_raw(width, height, rgb.to_vec()).expect("frame size");
    image.save(&path).expect("png");
    println!("wrote {}", path.display());
}
