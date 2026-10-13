//! Arena dressing. Radii and heights come from the parsed arena. Tessellation,
//! materials, and props come from arena_visual.toml. Cover boxes are the only
//! new colliders, and those sizes live in arena_01.toml.

use crate::geom;
use crate::MeshData;
use gemini_sim::content::{Content, Ramp};
use gemini_sim::scenery::{Cover, SceneMaterial};

const INK: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

pub struct ArenaPiece {
    pub mesh: MeshData,
    pub palette: String,
    pub glow: bool,
    pub lit: bool,
    pub roughness: f32,
    pub metallic: f32,
    pub bloom: bool,
}

pub fn pieces(content: &Content) -> Vec<ArenaPiece> {
    let dress = &content.arena_dressing;
    let scene = &content.arena_scene;
    let platform = &content.arena.platform;
    let mut out = Vec::new();

    for floor in &scene.floors {
        let material = mat(scene_materials(content), &floor.material);
        out.push(piece(
            annulus(
                floor.inner,
                floor.outer,
                floor.y,
                scene.floor_sides,
                &platform.ramps,
                platform.center.x,
                platform.center.z,
                scene.ramp_skip_pad,
            ),
            material,
        ));
    }

    out.extend(deck_pieces(content));
    out.push(curb(
        dress.rail_posts,
        dress.rail_thickness,
        dress.rail_height,
        dress.ramp_gap_dot,
        [platform.center.x, platform.center.z],
        platform.radius,
        platform.height,
        &platform.ramps,
        mat(scene_materials(content), &scene.navy),
    ));
    out.push(trim_ring(content));
    for ramp in &platform.ramps {
        out.push(piece(
            ramp_wedge(
                [ramp.from.x, ramp.from.z],
                [ramp.to.x, ramp.to.z],
                ramp.width,
                platform.height,
            ),
            mat(scene_materials(content), &scene.pale),
        ));
    }
    out.extend(panels(content));
    for ledge in &content.arena.ledges {
        out.extend(ledge_pad(content, ledge.pos.x, ledge.pos.y, ledge.pos.z));
    }
    for vent in &content.arena.vents {
        let mut mesh = MeshData::new();
        geom::box_mesh(
            &mut mesh,
            [vent.pos.x, scene.vent_center_y, vent.pos.z],
            scene.vent_size,
            INK,
        );
        out.push(piece(mesh, mat(scene_materials(content), &scene.navy)));
    }
    for cliff in &scene.cliffs {
        out.push(piece(
            cliff_segment(cliff),
            mat(scene_materials(content), &cliff.material),
        ));
    }
    for cover in &content.arena.cover {
        out.push(piece(
            cover_visual(content, cover),
            mat(scene_materials(content), &scene.boulder),
        ));
    }
    for nest in &scene.nests {
        let vent = content
            .arena
            .vents
            .iter()
            .find(|vent| vent.id == nest.vent)
            .unwrap_or_else(|| panic!("nest {} vent", nest.id));
        let len = (vent.pos.x * vent.pos.x + vent.pos.z * vent.pos.z)
            .sqrt()
            .max(1e-4);
        let ox = vent.pos.x / len;
        let oz = vent.pos.z / len;
        let center = [
            vent.pos.x + ox * nest.outset,
            nest.size[1] * 0.5,
            vent.pos.z + oz * nest.outset,
        ];
        let yaw = vent.bearing_deg.to_radians();
        let mut shell = MeshData::new();
        geom::box_mesh(&mut shell, [0.0, 0.0, 0.0], nest.size, INK);
        geom::rotate_y(&mut shell, yaw);
        geom::translate(&mut shell, center);
        out.push(piece(shell, mat(scene_materials(content), &nest.material)));
        let goo_size = [
            nest.size[0] * nest.goo_scale[0],
            nest.size[1] * nest.goo_scale[1],
            nest.size[2] * nest.goo_scale[2],
        ];
        // Local -Z faces the arena. Sit the goo on that face.
        let local_z = -nest.size[2] * 0.5 + goo_size[2] * 0.5;
        let mut goo = MeshData::new();
        geom::box_mesh(&mut goo, [0.0, 0.0, local_z], goo_size, INK);
        geom::rotate_y(&mut goo, yaw);
        geom::translate(&mut goo, center);
        out.push(piece(goo, mat(scene_materials(content), &nest.goo)));
    }
    for tower in &scene.towers {
        out.extend(relay(content, tower));
    }
    for prop in &scene.props {
        out.push(piece(
            prop_mesh(content, prop),
            mat(scene_materials(content), &prop.material),
        ));
    }
    out
}

/// Bevelled cover. The box reaches the authored extents, so the mesh bounds match the collider.
pub fn cover_visual(content: &Content, cover: &Cover) -> MeshData {
    let scene = &content.arena_scene;
    let mut mesh = MeshData::new();
    geom::bevel_box(&mut mesh, cover.size, scene.cover_chamfer, INK);
    rock_dome(
        &mut mesh,
        cover.size,
        scene.cover_chamfer,
        scene.boulder_slices,
        scene.boulder_stacks,
        scene.dome_radius_scale,
        scene.dome_height_scale,
    );
    geom::translate(&mut mesh, [cover.pos.x, cover.pos.y, cover.pos.z]);
    mesh
}

pub fn scenery_fingerprint(pieces: &[ArenaPiece]) -> (String, u32, u32) {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut tris = 0u32;
    let mut bytes = 0u32;
    for piece in pieces {
        for p in &piece.mesh.positions {
            hasher.update(p[0].to_le_bytes());
            hasher.update(p[1].to_le_bytes());
            hasher.update(p[2].to_le_bytes());
        }
        for index in &piece.mesh.indices {
            hasher.update(index.to_le_bytes());
        }
        let verts = piece.mesh.positions.len() as u32;
        tris += (piece.mesh.indices.len() / 3) as u32;
        // Positions, normals, the zero UVs the client uploads, and indices.
        bytes += verts * 12 + verts * 12 + verts * 8 + piece.mesh.indices.len() as u32 * 4;
    }
    let hex = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    (hex, tris, bytes)
}

fn scene_materials(content: &Content) -> &[SceneMaterial] {
    &content.arena_scene.materials
}

fn mat<'a>(materials: &'a [SceneMaterial], id: &str) -> &'a SceneMaterial {
    materials
        .iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("missing scenery material {id}"))
}

fn piece(mesh: MeshData, material: &SceneMaterial) -> ArenaPiece {
    ArenaPiece {
        mesh,
        palette: material.color.clone(),
        glow: material.glow,
        lit: material.lit,
        roughness: material.roughness,
        metallic: material.metallic,
        bloom: material.bloom,
    }
}

fn deck_pieces(content: &Content) -> Vec<ArenaPiece> {
    let dress = &content.arena_dressing;
    let platform = &content.arena.platform;
    let mut side = MeshData::new();
    let mut top = MeshData::new();
    let center = [platform.center.x, platform.height * 0.5, platform.center.z];
    let y0 = center[1] - platform.height * 0.5;
    let y1 = center[1] + platform.height * 0.5;
    let sides = dress.deck_sides;
    let mut ring_top = Vec::new();
    let mut ring_bot = Vec::new();
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        ring_top.push([
            center[0] + c * platform.radius,
            y1,
            center[2] + s * platform.radius,
        ]);
        ring_bot.push([
            center[0] + c * platform.radius,
            y0,
            center[2] + s * platform.radius,
        ]);
    }
    for i in 0..sides as usize {
        let j = (i + 1) % sides as usize;
        geom::add_quad(
            &mut side,
            ring_top[i],
            ring_bot[i],
            ring_bot[j],
            ring_top[j],
            INK,
        );
        geom::add_face(
            &mut top,
            [center[0], y1, center[2]],
            ring_top[j],
            ring_top[i],
            INK,
        );
        geom::add_face(
            &mut side,
            [center[0], y0, center[2]],
            ring_bot[i],
            ring_bot[j],
            INK,
        );
    }
    vec![
        piece(side, mat(scene_materials(content), &content.arena_scene.navy)),
        piece(top, mat(scene_materials(content), &content.arena_scene.pale)),
    ]
}

fn trim_ring(content: &Content) -> ArenaPiece {
    let dress = &content.arena_dressing;
    let scene = &content.arena_scene;
    let platform = &content.arena.platform;
    let y0 = platform.height + dress.rail_height + scene.trim_lift;
    let y1 = y0 + scene.trim_height;
    let inner = platform.radius;
    let outer = platform.radius + scene.trim_outset;
    let mut mesh = MeshData::new();
    let posts = content.arena_dressing.deck_sides;
    let point = |ang: f32, rad: f32, y: f32| {
        let (s, c) = ang.sin_cos();
        [platform.center.x + c * rad, y, platform.center.z + s * rad]
    };
    for i in 0..posts {
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
    }
    piece(mesh, mat(scene_materials(content), &scene.trim))
}

fn panels(content: &Content) -> Vec<ArenaPiece> {
    let scene = &content.arena_scene;
    let platform = &content.arena.platform;
    let material = mat(scene_materials(content), &scene.navy);
    let mut out = Vec::new();
    let radius = platform.radius - scene.panel_length * 0.5 - scene.trim_outset;
    let y = platform.height + scene.panel_lift + scene.panel_thickness * 0.5;
    for i in 0..scene.panel_count {
        let ang = i as f32 / scene.panel_count as f32 * std::f32::consts::TAU;
        let (s, c) = ang.sin_cos();
        let mut mesh = MeshData::new();
        geom::box_mesh(
            &mut mesh,
            [0.0, 0.0, 0.0],
            [
                scene.panel_length,
                scene.panel_thickness,
                scene.panel_thickness * scene.panel_depth_scale,
            ],
            INK,
        );
        geom::rotate_y(&mut mesh, ang);
        geom::translate(
            &mut mesh,
            [
                platform.center.x + c * radius,
                y,
                platform.center.z + s * radius,
            ],
        );
        out.push(piece(mesh, material));
    }
    out
}

fn ledge_pad(content: &Content, x: f32, y: f32, z: f32) -> Vec<ArenaPiece> {
    let scene = &content.arena_scene;
    let material = mat(scene_materials(content), &scene.pale);
    let top = y - scene.ledge_size[1] * 0.5;
    let mut pad = MeshData::new();
    geom::box_mesh(&mut pad, [x, top, z], scene.ledge_size, INK);
    let skirt_y = top - scene.ledge_size[1] * 0.5 - scene.ledge_thickness * 0.5;
    let mut skirt = MeshData::new();
    geom::box_mesh(
        &mut skirt,
        [x, skirt_y, z],
        [
            scene.ledge_size[0] * scene.ledge_skirt_scale,
            scene.ledge_thickness,
            scene.ledge_size[2] * scene.ledge_skirt_scale,
        ],
        INK,
    );
    vec![
        piece(pad, material),
        piece(skirt, mat(scene_materials(content), &scene.navy)),
    ]
}

fn cliff_segment(cliff: &gemini_sim::scenery::Cliff) -> MeshData {
    let mut mesh = MeshData::new();
    let a0 = cliff.angle_deg.to_radians();
    let a1 = (cliff.angle_deg + cliff.span_deg).to_radians();
    // Bearing 0 is +Z, matching the vents.
    let point = |ang: f32, rad: f32, y: f32| {
        let (s, c) = ang.sin_cos();
        [s * rad, y, c * rad]
    };
    let i0 = point(a0, cliff.inner, 0.0);
    let i1 = point(a1, cliff.inner, 0.0);
    let o0 = point(a0, cliff.outer, 0.0);
    let o1 = point(a1, cliff.outer, 0.0);
    let ih0 = point(a0, cliff.inner, cliff.inner_height);
    let ih1 = point(a1, cliff.inner, cliff.inner_height);
    let oh0 = point(a0, cliff.outer, cliff.outer_height);
    let oh1 = point(a1, cliff.outer, cliff.outer_height);
    geom::add_quad(&mut mesh, ih0, ih1, oh1, oh0, INK);
    geom::add_quad(&mut mesh, i0, ih0, ih1, i1, INK);
    geom::add_quad(&mut mesh, o1, oh1, oh0, o0, INK);
    geom::add_quad(&mut mesh, i0, o0, oh0, ih0, INK);
    geom::add_quad(&mut mesh, i1, ih1, oh1, o1, INK);
    mesh
}

fn relay(content: &Content, tower: &gemini_sim::scenery::Tower) -> Vec<ArenaPiece> {
    let ledge = content
        .arena
        .ledges
        .iter()
        .find(|ledge| ledge.id == tower.ledge)
        .unwrap_or_else(|| panic!("tower {} ledge", tower.id));
    let material = mat(scene_materials(content), &tower.material);
    let beacon = mat(scene_materials(content), &tower.beacon_material);
    let mut y = ledge.pos.y;
    let mut out = Vec::new();
    for (size, glow) in [
        (tower.base, false),
        (tower.mast, false),
        (tower.head, false),
        (tower.beacon, true),
    ] {
        let center_y = y + size[1] * 0.5;
        let mut mesh = MeshData::new();
        geom::box_mesh(&mut mesh, [ledge.pos.x, center_y, ledge.pos.z], size, INK);
        out.push(piece(mesh, if glow { beacon } else { material }));
        y += size[1];
    }
    out
}

fn prop_mesh(content: &Content, prop: &gemini_sim::scenery::Prop) -> MeshData {
    let scene = &content.arena_scene;
    let shape = scene
        .prop_meshes
        .iter()
        .find(|row| row.kind == prop.kind)
        .unwrap_or_else(|| panic!("prop {} kind \"{}\" has no mesh", prop.id, prop.kind));
    let mut mesh = MeshData::new();
    match shape.mesh.as_str() {
        "cylinder" => {
            cylinder_upright(&mut mesh, prop.size, scene.flora_sides);
        }
        "bulb" => {
            let chamfer = prop.size[0].min(prop.size[2]) * scene.bulb_chamfer_scale;
            rock_dome(
                &mut mesh,
                prop.size,
                chamfer,
                scene.flora_sides,
                scene.bulb_stacks,
                scene.dome_radius_scale,
                scene.dome_height_scale,
            );
            geom::bevel_box(&mut mesh, prop.size, chamfer, INK);
        }
        "bevel" => {
            geom::bevel_box(
                &mut mesh,
                prop.size,
                prop.size[0].min(prop.size[2]) * scene.clutter_chamfer_scale,
                INK,
            );
        }
        "box" => {
            geom::box_mesh(&mut mesh, [0.0, 0.0, 0.0], prop.size, INK);
        }
        other => panic!("prop {} mesh \"{other}\" is unknown", prop.id),
    }
    geom::rotate_y(&mut mesh, prop.yaw_deg.to_radians());
    geom::translate(&mut mesh, prop.pos);
    mesh
}

fn rock_dome(
    mesh: &mut MeshData,
    size: [f32; 3],
    chamfer: f32,
    slices: u32,
    stacks: u32,
    radius_scale: f32,
    height_scale: f32,
) {
    let hx = (size[0] * 0.5 - chamfer) * radius_scale;
    let hz = (size[2] * 0.5 - chamfer) * radius_scale;
    let hy = size[1] * 0.5 * height_scale;
    let mut rings: Vec<Vec<[f32; 3]>> = Vec::new();
    for stack in 0..=stacks {
        let v = stack as f32 / stacks as f32;
        let pitch = v * std::f32::consts::FRAC_PI_2;
        let (s, c) = pitch.sin_cos();
        let mut ring = Vec::new();
        for slice in 0..slices {
            let u = slice as f32 / slices as f32 * std::f32::consts::TAU;
            let (us, uc) = u.sin_cos();
            ring.push([uc * hx * c, hy * s, us * hz * c]);
        }
        rings.push(ring);
    }
    for stack in 0..stacks as usize {
        let a = &rings[stack];
        let b = &rings[stack + 1];
        for slice in 0..slices as usize {
            let next = (slice + 1) % slices as usize;
            geom::add_quad(mesh, a[slice], a[next], b[next], b[slice], INK);
        }
    }
}

fn cylinder_upright(mesh: &mut MeshData, size: [f32; 3], sides: u32) {
    let hx = size[0] * 0.5;
    let hy = size[1] * 0.5;
    let hz = size[2] * 0.5;
    let mut top = Vec::new();
    let mut bot = Vec::new();
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        top.push([c * hx, hy, s * hz]);
        bot.push([c * hx, -hy, s * hz]);
    }
    for i in 0..sides as usize {
        let j = (i + 1) % sides as usize;
        geom::add_quad(mesh, top[i], bot[i], bot[j], top[j], INK);
        geom::add_face(mesh, [0.0, hy, 0.0], top[j], top[i], INK);
        geom::add_face(mesh, [0.0, -hy, 0.0], bot[i], bot[j], INK);
    }
}

fn annulus(
    inner: f32,
    outer: f32,
    y: f32,
    sides: u32,
    ramps: &[Ramp],
    cx: f32,
    cz: f32,
    ramp_skip_pad: f32,
) -> MeshData {
    let mut mesh = MeshData::new();
    for i in 0..sides {
        let a0 = i as f32 / sides as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / sides as f32 * std::f32::consts::TAU;
        let mid = (a0 + a1) * 0.5;
        let (s, c) = mid.sin_cos();
        let rad = (inner + outer) * 0.5;
        let x = cx + c * rad;
        let z = cz + s * rad;
        if on_ramp(x, z, ramps, ramp_skip_pad) {
            continue;
        }
        let point = |ang: f32, rad: f32| {
            let (s, c) = ang.sin_cos();
            [cx + c * rad, y, cz + s * rad]
        };
        geom::add_face(
            &mut mesh,
            point(a0, inner),
            point(a0, outer),
            point(a1, outer),
            INK,
        );
        geom::add_face(
            &mut mesh,
            point(a0, inner),
            point(a1, outer),
            point(a1, inner),
            INK,
        );
    }
    mesh
}

fn on_ramp(x: f32, z: f32, ramps: &[Ramp], pad: f32) -> bool {
    for ramp in ramps {
        let abx = ramp.to.x - ramp.from.x;
        let abz = ramp.to.z - ramp.from.z;
        let len2 = abx * abx + abz * abz;
        if len2 < 1e-4 {
            continue;
        }
        let t = ((x - ramp.from.x) * abx + (z - ramp.from.z) * abz) / len2;
        if t < -pad || t > 1.0 + pad {
            continue;
        }
        let clamped = t.clamp(0.0, 1.0);
        let px = ramp.from.x + abx * clamped;
        let pz = ramp.from.z + abz * clamped;
        let lateral = ((x - px).powi(2) + (z - pz).powi(2)).sqrt();
        if lateral <= ramp.width * 0.5 {
            return true;
        }
    }
    false
}

fn curb(
    posts: u32,
    thick: f32,
    rail_h: f32,
    gap_dot: f32,
    center: [f32; 2],
    radius: f32,
    deck_top: f32,
    ramps: &[Ramp],
    material: &SceneMaterial,
) -> ArenaPiece {
    let mut mesh = MeshData::new();
    let inner = radius - thick * 0.5;
    let outer = radius + thick * 0.5;
    let y0 = deck_top;
    let y1 = deck_top + rail_h;
    let skipped = |i: u32| {
        let ang = (i as f32 + 0.5) / posts as f32 * std::f32::consts::TAU;
        let (s, c) = ang.sin_cos();
        ramps
            .iter()
            .any(|ramp| near_ramp(center, [c, s], ramp, gap_dot))
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
    piece(mesh, material)
}

fn near_ramp(center: [f32; 2], dir: [f32; 2], ramp: &Ramp, gap_dot: f32) -> bool {
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
