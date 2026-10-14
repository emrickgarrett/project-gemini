//! Pending-vent decal stills. Distance, height, and the eye camera come from content.

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use gemini_meshes::raster_vent_tells;
use gemini_sim::content::Content;

fn main() {
    let content = Content::embedded();
    let eye = content
        .arena_scene
        .cameras
        .iter()
        .find(|camera| camera.id == "eye")
        .unwrap_or_else(|| panic!("missing eye camera"));
    let out = "art/arenas/review";
    fs::create_dir_all(out).expect("create out dir");
    let (ground_eye, ground_look) = ground_camera(&content);
    let rgb = raster_vent_tells(&[0], ground_eye, ground_look, eye.fov, eye.width, eye.height);
    let ground_path = format!("{out}/vent_tell_ground.png");
    write_png(&ground_path, eye.width, eye.height, &rgb);
    println!("wrote {ground_path}");
    let pending: Vec<u8> = (0..4).map(|index| index as u8).collect();
    let (top_eye, top_look) = top_camera(&content, eye.fov);
    let rgb = raster_vent_tells(&pending, top_eye, top_look, eye.fov, eye.width, eye.height);
    let top_path = format!("{out}/vent_tell_top.png");
    write_png(&top_path, eye.width, eye.height, &rgb);
    println!("wrote {top_path}");
}

fn ground_camera(content: &Content) -> ([f32; 3], [f32; 3]) {
    let vent = content.arena.vents.first().expect("arena vent");
    let center = content.arena.platform.center;
    let dx = center.x - vent.pos.x;
    let dz = center.z - vent.pos.z;
    let len = (dx * dx + dz * dz).sqrt();
    if !(len > 0.0) {
        panic!("first vent sits on the platform center");
    }
    let dist = content.arena_dressing.review_distance;
    let eye = [
        vent.pos.x + dx / len * dist,
        content.controller.camera.eye_height,
        vent.pos.z + dz / len * dist,
    ];
    let look = [vent.pos.x, content.arena_scene.vent_tell_height * 0.5, vent.pos.z];
    (eye, look)
}

fn top_camera(content: &Content, fov_deg: f32) -> ([f32; 3], [f32; 3]) {
    let center = content.arena.platform.center;
    let half = (fov_deg.to_radians() * 0.5).tan();
    if !(half > 0.0) {
        panic!("eye camera fov must be > 0");
    }
    let eye = [center.x, content.arena.radius / half, center.z];
    let look = [center.x, 0.0, center.z];
    (eye, look)
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
