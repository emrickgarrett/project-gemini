//! Arena review stills. Cameras come from arena_visual.toml.

use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use gemini_meshes::{raster_arena, raster_view_gun, ViewmodelFrame};
use gemini_sim::content::{hex_srgb, Content};

const FIELD: u8 = 0x8A;

fn main() {
    let mut out = "art/arenas".to_string();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--out" {
            if let Some(dir) = args.next() {
                out = dir;
            }
        }
    }
    fs::create_dir_all(&out).expect("create out dir");
    let content = Content::embedded();
    for camera in &content.arena_scene.cameras {
        let mut rgb = raster_arena(
            camera.eye,
            camera.look,
            camera.fov,
            camera.width,
            camera.height,
        );
        if camera.id == "vex" {
            let arms = raster_view_gun(camera.width, camera.height, &vex_viewmodel(&content));
            composite(&mut rgb, &arms);
        }
        let path = format!("{out}/{}.png", camera.id);
        write_png(&path, camera.width, camera.height, &rgb);
        println!("wrote {path}");
    }
}

fn vex_viewmodel(content: &Content) -> ViewmodelFrame {
    let hero = &content.hero;
    let card = content
        .heroes
        .iter()
        .find(|card| card.id == hero.id)
        .unwrap_or_else(|| panic!("starter {} has no card", hero.id));
    let accent = content.palette.key(&card.accent_key);
    let loaded = hex_srgb(&hero.accent);
    if accent != loaded {
        panic!(
            "starter accent_key {} did not match the accent the loader stored",
            card.accent_key
        );
    }
    ViewmodelFrame {
        accent,
        offset: hero.viewmodel_offset,
        fov: hero.viewmodel_fov,
        overdrive: false,
        glow_radius: hero.muzzle_flash_radius,
        glow_core: hero.muzzle_flash_core,
        glow_falloff: hero.muzzle_flash_falloff,
        glow_tip_px: hero.muzzle_flash_tip_px,
        glow_color: [
            hero.muzzle_flash_color[0],
            hero.muzzle_flash_color[1],
            hero.muzzle_flash_color[2],
            1.0,
        ],
    }
}

fn composite(world: &mut [u8], arms: &[u8]) {
    for (index, pixel) in arms.chunks(3).enumerate() {
        if pixel == [FIELD, FIELD, FIELD] {
            continue;
        }
        let o = index * 3;
        world[o] = pixel[0];
        world[o + 1] = pixel[1];
        world[o + 2] = pixel[2];
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
    child
        .stdin
        .take()
        .unwrap()
        .write_all(rgb)
        .expect("write frame");
    let status = child.wait().expect("ffmpeg wait");
    if !status.success() {
        eprintln!("ffmpeg failed for {path}");
        std::process::exit(1);
    }
}
