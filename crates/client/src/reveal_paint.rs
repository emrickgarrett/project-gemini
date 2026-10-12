//! Software frames of a crate reveal. The pixels are rendered with the bugshot stills.

use gemini_sim::content::Content;
use gemini_sim::reveal::RevealPlan;

pub fn paint(content: &Content, plan: &RevealPlan, t: f32, width: u32, height: u32) -> Vec<u8> {
    gemini_meshes::paint_reveal(content, plan, t, width, height)
}

fn bytes(rgb: [f32; 4]) -> [u8; 3] {
    [
        (rgb[0] * 255.0).round() as u8,
        (rgb[1] * 255.0).round() as u8,
        (rgb[2] * 255.0).round() as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gemini_sim::reveal::plan;

    #[test]
    fn cards_use_the_palette_and_the_server_rarity() {
        let content = Content::embedded();
        for rarity in ["common", "rare", "epic", "legendary"] {
            let planned = plan(&content, rarity, None).unwrap();
            let rgb = paint(&content, &planned, planned.card_at, 320, 180);
            let expect = bytes(content.palette.key(&format!("rarity_{rarity}")));
            // Middle of the card face, clear of the frame, gem, and title.
            let swatch = pixel(&rgb, 320, 160, 90);
            assert_eq!(swatch, expect, "{rarity} card {swatch:?}");
        }
        let teased = plan(&content, "rare", Some("epic")).unwrap();
        let mid = paint(&content, &teased, 2.8, 320, 180);
        let epic = bytes(content.palette.key("rarity_epic"));
        let rare = bytes(content.palette.key("rarity_rare"));
        assert!(rgb_contains(&mid, epic), "fakeout window shows the teased tier");
        let card = paint(&content, &teased, teased.card_at, 320, 180);
        assert_eq!(pixel(&card, 320, 160, 90), rare, "the card settles on the server rarity");
    }

    #[test]
    fn legendary_freeze_dims_the_frame() {
        let content = Content::embedded();
        let planned = plan(&content, "legendary", None).unwrap();
        let open = paint(&content, &planned, 3.2, 160, 90);
        let frozen = paint(&content, &planned, 4.0, 160, 90);
        // Left half of the crate. The crack gap sits on the midline.
        let open_c = pixel(&open, 160, 72, 45);
        let frozen_c = pixel(&frozen, 160, 72, 45);
        let sum = |px: [u8; 3]| px[0] as u32 + px[1] as u32 + px[2] as u32;
        assert!(sum(frozen_c) < sum(open_c), "freeze {frozen_c:?} vs landed {open_c:?}");
    }

    fn pixel(rgb: &[u8], width: u32, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * width + x) * 3) as usize;
        [rgb[i], rgb[i + 1], rgb[i + 2]]
    }

    fn rgb_contains(rgb: &[u8], color: [u8; 3]) -> bool {
        rgb.chunks_exact(3).any(|px| px == color)
    }
}
