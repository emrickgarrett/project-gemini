//! Arena dressing. Radii and heights come from the parsed arena. Tessellation
//! and the curb come from arena dressing. Nothing here is a collider.

use crate::geom;
use crate::MeshData;
use gemini_sim::content::Content;

const INK: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

pub struct ArenaPiece {
    pub mesh: MeshData,
    pub palette: String,
}

pub fn pieces(content: &Content) -> Vec<ArenaPiece> {
    let dress = &content.arena_dressing;
    let platform = &content.arena.platform;
    let mut out = Vec::new();
    let mut deck = MeshData::new();
    cylinder(
        &mut deck,
        [platform.center.x, platform.height * 0.5, platform.center.z],
        platform.radius,
        platform.height,
        dress.deck_sides,
    );
    out.push(ArenaPiece { mesh: deck, palette: "arena_light".into() });
    out.push(curb(
        dress.rail_posts,
        dress.rail_thickness,
        dress.rail_height,
        dress.ramp_gap_dot,
        [platform.center.x, platform.center.z],
        platform.radius,
        platform.height,
        &platform.ramps,
    ));
    for ramp in &platform.ramps {
        out.push(ArenaPiece {
            mesh: ramp_wedge([ramp.from.x, ramp.from.z], [ramp.to.x, ramp.to.z], ramp.width, platform.height),
            palette: "arena_light".into(),
        });
    }
    out
}

/// Ground disc used by the in-arena still. The client keeps its own ground box.
pub fn review_ground(content: &Content) -> MeshData {
    let mut mesh = MeshData::new();
    disc(
        &mut mesh,
        [0.0, content.arena_dressing.review_ground_y, 0.0],
        content.arena.radius,
        content.arena_dressing.deck_sides,
    );
    mesh
}

fn curb(
    posts: u32,
    thick: f32,
    rail_h: f32,
    gap_dot: f32,
    center: [f32; 2],
    radius: f32,
    deck_top: f32,
    ramps: &[gemini_sim::content::Ramp],
) -> ArenaPiece {
    let mut mesh = MeshData::new();
    let inner = radius - thick * 0.5;
    let outer = radius + thick * 0.5;
    let y0 = deck_top;
    let y1 = deck_top + rail_h;
    let skipped = |i: u32| {
        let ang = (i as f32 + 0.5) / posts as f32 * std::f32::consts::TAU;
        let (s, c) = ang.sin_cos();
        ramps.iter().any(|ramp| near_ramp(center, [c, s], ramp, gap_dot))
    };
    let point = |ang: f32, rad: f32, y: f32| {
        let (s, c) = ang.sin_cos();
        [center[0] + c * rad, y, center[1] + s * rad]
    };
    for i in 0..posts {
        if skipped(i) {
            continue;
        }
        let a0 = i as f32 / posts as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / posts as f32 * std::f32::consts::TAU;
        geom::add_quad(
            &mut mesh,
            point(a0, outer, y1),
            point(a0, outer, y0),
            point(a1, outer, y0),
            point(a1, outer, y1),
            INK,
        );
        geom::add_quad(
            &mut mesh,
            point(a1, inner, y1),
            point(a1, inner, y0),
            point(a0, inner, y0),
            point(a0, inner, y1),
            INK,
        );
        geom::add_quad(
            &mut mesh,
            point(a0, inner, y1),
            point(a1, inner, y1),
            point(a1, outer, y1),
            point(a0, outer, y1),
            INK,
        );
        let prev = if i == 0 { posts - 1 } else { i - 1 };
        let next = (i + 1) % posts;
        if skipped(prev) {
            geom::add_quad(
                &mut mesh,
                point(a0, outer, y0),
                point(a0, outer, y1),
                point(a0, inner, y1),
                point(a0, inner, y0),
                INK,
            );
        }
        if skipped(next) {
            geom::add_quad(
                &mut mesh,
                point(a1, inner, y0),
                point(a1, inner, y1),
                point(a1, outer, y1),
                point(a1, outer, y0),
                INK,
            );
        }
    }
    ArenaPiece { mesh, palette: "arena_light".into() }
}

fn near_ramp(center: [f32; 2], dir: [f32; 2], ramp: &gemini_sim::content::Ramp, gap_dot: f32) -> bool {
    let mouth = [ramp.from.x - center[0], ramp.from.z - center[1]];
    let len = (mouth[0] * mouth[0] + mouth[1] * mouth[1]).sqrt().max(1e-4);
    let dot = (dir[0] * mouth[0] + dir[1] * mouth[1]) / len;
    dot > gap_dot
}

fn ramp_wedge(from: [f32; 2], to: [f32; 2], width: f32, height: f32) -> MeshData {
    let mut mesh = MeshData::new();
    let dx = to[0] - from[0];
    let dz = to[1] - from[1];
    let len = (dx * dx + dz * dz).sqrt().max(1e-4);
    let side = [-dz / len * width * 0.5, dx / len * width * 0.5];
    let hi = |sign: f32, y: f32| [from[0] + side[0] * sign, y, from[1] + side[1] * sign];
    let lo = |sign: f32| [to[0] + side[0] * sign, 0.0, to[1] + side[1] * sign];
    let a0 = hi(1.0, height);
    let b0 = hi(-1.0, height);
    let a0g = hi(1.0, 0.0);
    let b0g = hi(-1.0, 0.0);
    let a1 = lo(1.0);
    let b1 = lo(-1.0);
    geom::add_quad(&mut mesh, a0, b0, b1, a1, INK);
    geom::add_quad(&mut mesh, a0, a0g, b0g, b0, INK);
    geom::add_quad(&mut mesh, a0g, a1, b1, b0g, INK);
    geom::add_face(&mut mesh, a0, a1, a0g, INK);
    geom::add_face(&mut mesh, b0, b0g, b1, INK);
    mesh
}

fn cylinder(mesh: &mut MeshData, center: [f32; 3], radius: f32, height: f32, sides: u32) {
    let y0 = center[1] - height * 0.5;
    let y1 = center[1] + height * 0.5;
    let mut top = Vec::new();
    let mut bot = Vec::new();
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        top.push([center[0] + c * radius, y1, center[2] + s * radius]);
        bot.push([center[0] + c * radius, y0, center[2] + s * radius]);
    }
    for i in 0..sides as usize {
        let j = (i + 1) % sides as usize;
        geom::add_quad(mesh, top[i], bot[i], bot[j], top[j], INK);
        geom::add_face(mesh, [center[0], y1, center[2]], top[j], top[i], INK);
        geom::add_face(mesh, [center[0], y0, center[2]], bot[i], bot[j], INK);
    }
}

fn disc(mesh: &mut MeshData, center: [f32; 3], radius: f32, sides: u32) {
    let mut ring = Vec::new();
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        ring.push([center[0] + c * radius, center[1], center[2] + s * radius]);
    }
    for i in 0..sides as usize {
        let j = (i + 1) % sides as usize;
        geom::add_face(mesh, center, ring[i], ring[j], INK);
    }
}
