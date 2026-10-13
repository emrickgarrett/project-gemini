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
    pub role: &'static str,
}

pub fn pieces(content: &Content) -> Vec<ArenaPiece> {
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
    out.extend(railing(content));
    out.extend(stairs(content));
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
        out.push(tagged(
            rim_pile(content, cliff),
            mat(scene_materials(content), &cliff.material),
            "rim",
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
        let center = gemini_sim::scenery::nest_anchor(
            vent.pos.x,
            vent.pos.z,
            nest.outset,
            scene.nest_lift,
        );
        let yaw = vent.bearing_deg.to_radians();
        let mut shell = hive_pod(content, nest);
        geom::rotate_y(&mut shell, yaw);
        geom::translate(&mut shell, center);
        out.push(piece(shell, mat(scene_materials(content), &nest.material)));
        let mut goo = hive_goo(content, nest);
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
    tagged(mesh, material, "scenery")
}

fn tagged(mesh: MeshData, material: &SceneMaterial, role: &'static str) -> ArenaPiece {
    ArenaPiece {
        mesh,
        palette: material.color.clone(),
        glow: material.glow,
        lit: material.lit,
        roughness: material.roughness,
        metallic: material.metallic,
        bloom: material.bloom,
        role,
    }
}

fn deck_pieces(content: &Content) -> Vec<ArenaPiece> {
    let scene = &content.arena_scene;
    let platform = &content.arena.platform;
    let navy = mat(scene_materials(content), &scene.navy);
    let pale = mat(scene_materials(content), &scene.pale);
    let trim = mat(scene_materials(content), &scene.trim);
    let center = [platform.center.x, platform.center.z];
    let sides = scene.platform_sides;
    let outer_r = platform.radius;
    let inner_r = platform.radius - scene.deck_step_outset;
    let mut skirt = MeshData::new();
    let mut wall = MeshData::new();
    let mut edge = MeshData::new();
    let mut top = MeshData::new();
    let mut trim_mesh = MeshData::new();
    let top_y = platform.height;
    let step_y = scene.deck_step_height;
    for i in 0..sides {
        let outer_a = gon_vertex(scene, i, outer_r, step_y, center);
        let outer_b = gon_vertex(scene, i + 1, outer_r, step_y, center);
        let inner_top_a = gon_vertex(scene, i, inner_r, top_y, center);
        let inner_top_b = gon_vertex(scene, i + 1, inner_r, top_y, center);
        geom::add_face(&mut top, [center[0], top_y, center[1]], inner_top_a, inner_top_b, INK);
        if side_is_open(content, i) {
            continue;
        }
        let outer_0a = [outer_a[0], 0.0, outer_a[2]];
        let outer_0b = [outer_b[0], 0.0, outer_b[2]];
        geom::add_quad(&mut skirt, outer_a, outer_0a, outer_0b, outer_b, INK);
        let shelf_a = gon_vertex(scene, i, inner_r, step_y, center);
        let shelf_b = gon_vertex(scene, i + 1, inner_r, step_y, center);
        geom::add_quad(&mut edge, shelf_a, outer_a, outer_b, shelf_b, INK);
        let wall_a = gon_vertex(scene, i, inner_r, top_y, center);
        let wall_b = gon_vertex(scene, i + 1, inner_r, top_y, center);
        geom::add_quad(&mut wall, wall_a, shelf_a, shelf_b, wall_b, INK);
        geom::add_quad(&mut edge, wall_a, wall_b, outer_b, outer_a, INK);
        let mid = [
            (inner_top_a[0] + inner_top_b[0]) * 0.5,
            top_y + scene.trim_lift + scene.trim_height * 0.5,
            (inner_top_a[2] + inner_top_b[2]) * 0.5,
        ];
        let span = [
            inner_top_b[0] - inner_top_a[0],
            0.0,
            inner_top_b[2] - inner_top_a[2],
        ];
        let span_len = (span[0] * span[0] + span[2] * span[2]).sqrt().max(1e-4);
        let along = [
            span[0] / span_len * scene.panel_length * 0.5,
            span[2] / span_len * scene.panel_length * 0.5,
        ];
        let out_n = [span[2] / span_len, 0.0, -span[0] / span_len];
        let lift = scene.trim_outset * 0.5;
        let p0 = [mid[0] - along[0] + out_n[0] * lift, mid[1], mid[2] - along[1] + out_n[2] * lift];
        let p1 = [mid[0] + along[0] + out_n[0] * lift, mid[1], mid[2] + along[1] + out_n[2] * lift];
        let y0 = mid[1] - scene.trim_height * 0.5;
        let y1 = mid[1] + scene.trim_height * 0.5;
        geom::add_quad(
            &mut trim_mesh,
            [p0[0], y1, p0[2]],
            [p1[0], y1, p1[2]],
            [p1[0], y0, p1[2]],
            [p0[0], y0, p0[2]],
            INK,
        );
    }
    vec![
        tagged(skirt, navy, "deck"),
        tagged(wall, navy, "deck"),
        tagged(edge, navy, "deck"),
        tagged(top, pale, "deck"),
        tagged(trim_mesh, trim, "deck"),
    ]
}

/// Posts and a beam at `railing_height`. Ramp openings come from the same `solid_at` test as the collider.
fn railing(content: &Content) -> Vec<ArenaPiece> {
    let dress = &content.arena_dressing;
    let platform = &content.arena.platform;
    let navy = mat(scene_materials(content), &content.arena_scene.navy);
    let mut posts = MeshData::new();
    let mut beam = MeshData::new();
    let y = platform.height + dress.rail_height * 0.5;
    let radius = platform.radius;
    for i in 0..dress.rail_posts {
        let ang = i as f32 / dress.rail_posts as f32 * std::f32::consts::TAU;
        let (s, c) = ang.sin_cos();
        let x = platform.center.x + s * radius;
        let z = platform.center.z + c * radius;
        if !rail_blocked(content, x, z) {
            continue;
        }
        let mut post = MeshData::new();
        geom::box_mesh(
            &mut post,
            [0.0, 0.0, 0.0],
            [dress.rail_thickness, dress.rail_height, dress.rail_thickness],
            INK,
        );
        geom::rotate_y(&mut post, ang);
        geom::translate(&mut post, [x, y, z]);
        append_mesh(&mut posts, &post);
    }
    let step = dress.rail_thickness / radius;
    for i in 0..dress.deck_sides {
        let a0 = i as f32 / dress.deck_sides as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / dress.deck_sides as f32 * std::f32::consts::TAU;
        for (lo, hi) in rail_runs(content, a0, a1, step, radius) {
            let mid = (lo + hi) * 0.5;
            let span = radius * (hi - lo);
            if span < dress.rail_thickness {
                continue;
            }
            let (s, c) = mid.sin_cos();
            let x = platform.center.x + s * radius;
            let z = platform.center.z + c * radius;
            let mut bar = MeshData::new();
            geom::box_mesh(
                &mut bar,
                [0.0, dress.rail_height * 0.5 - dress.rail_thickness * 0.5, 0.0],
                [span, dress.rail_thickness, dress.rail_thickness],
                INK,
            );
            geom::rotate_y(&mut bar, mid);
            geom::translate(&mut bar, [x, platform.height, z]);
            append_mesh(&mut beam, &bar);
        }
    }
    vec![
        tagged(posts, navy, "rail"),
        tagged(beam, navy, "rail"),
    ]
}

fn rail_blocked(content: &Content, x: f32, z: f32) -> bool {
    let platform = &content.arena.platform;
    let y = platform.height + content.arena_dressing.rail_height * 0.5;
    content.arena.solid_at(gemini_sim::Vec3::new(x, y, z))
}

/// Solid stretches of one deck side, sampled one rail thickness apart, so a beam stops at a ramp gap.
fn rail_runs(content: &Content, a0: f32, a1: f32, step: f32, radius: f32) -> Vec<(f32, f32)> {
    let platform = &content.arena.platform;
    let mut runs = Vec::new();
    let mut run_lo: Option<f32> = None;
    let mut prev = a0;
    let mut ang = a0;
    loop {
        let (s, c) = ang.sin_cos();
        let x = platform.center.x + s * radius;
        let z = platform.center.z + c * radius;
        if rail_blocked(content, x, z) {
            if run_lo.is_none() {
                run_lo = Some(ang);
            }
            prev = ang;
        } else if let Some(lo) = run_lo.take() {
            if prev > lo {
                runs.push((lo, prev));
            }
        }
        if ang >= a1 - 1.0e-4 {
            break;
        }
        ang = (ang + step).min(a1);
    }
    if let Some(lo) = run_lo {
        if prev > lo {
            runs.push((lo, prev));
        }
    }
    runs
}

fn stairs(content: &Content) -> Vec<ArenaPiece> {
    let scene = &content.arena_scene;
    let platform = &content.arena.platform;
    let material = mat(scene_materials(content), &scene.navy);
    let mut out = Vec::new();
    for ramp in &platform.ramps {
        let dx = ramp.to.x - ramp.from.x;
        let dz = ramp.to.z - ramp.from.z;
        let len = (dx * dx + dz * dz).sqrt();
        if len <= scene.stair_tread {
            continue;
        }
        let yaw = (-dz).atan2(dx);
        for step in 0..scene.stair_count {
            let t = (step as f32 + 0.5) / scene.stair_count as f32;
            let x = ramp.from.x + dx * t;
            let z = ramp.from.z + dz * t;
            let y = platform.height * (1.0 - t) + scene.stair_thickness * 0.5;
            let mut mesh = MeshData::new();
            geom::box_mesh(
                &mut mesh,
                [0.0, 0.0, 0.0],
                [scene.stair_tread, scene.stair_thickness, ramp.width],
                INK,
            );
            geom::rotate_y(&mut mesh, yaw);
            geom::translate(&mut mesh, [x, y, z]);
            out.push(tagged(mesh, material, "stairs"));
        }
    }
    out
}

fn gon_vertex(scene: &gemini_sim::scenery::ArenaScene, index: u32, radius: f32, y: f32, center: [f32; 2]) -> [f32; 3] {
    let step = std::f32::consts::TAU / scene.platform_sides as f32;
    let ang = scene.deck_phase_deg.to_radians() + index as f32 * step;
    let (s, c) = ang.sin_cos();
    [center[0] + s * radius, y, center[1] + c * radius]
}

fn side_is_open(content: &Content, index: u32) -> bool {
    let scene = &content.arena_scene;
    let step = 360.0 / scene.platform_sides as f32;
    let mid = (scene.deck_phase_deg + (index as f32 + 0.5) * step).rem_euclid(360.0);
    let open = |bearing: f32| {
        let mut d = (mid - bearing).abs();
        if d > 180.0 {
            d = 360.0 - d;
        }
        d < scene.deck_open_deg
    };
    if content
        .arena
        .platform
        .ramps
        .iter()
        .any(|ramp| open(ramp_bearing(ramp)))
    {
        return true;
    }
    content.arena.vents.iter().any(|vent| open(vent.bearing_deg))
}

fn ramp_bearing(ramp: &Ramp) -> f32 {
    let dx = ramp.to.x - ramp.from.x;
    let dz = ramp.to.z - ramp.from.z;
    dx.atan2(dz).to_degrees().rem_euclid(360.0)
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

fn rim_pile(content: &Content, cliff: &gemini_sim::scenery::Cliff) -> MeshData {
    let mut mesh = MeshData::new();
    for (start, end) in clear_arcs(content, cliff) {
        pile_arc(&mut mesh, content, cliff, start, end);
    }
    mesh
}

/// Boulders along one clear arc. The inner row is shorter, so the pile slopes down into the arena.
fn pile_arc(
    mesh: &mut MeshData,
    content: &Content,
    cliff: &gemini_sim::scenery::Cliff,
    start: f32,
    end: f32,
) {
    let scene = &content.arena_scene;
    let span = end - start;
    if span < scene.rim_min_arc_deg {
        return;
    }
    let rows = scene.rim_rows;
    let count = scene.rim_boulders;
    let radial = cliff.outer - cliff.inner;
    let depth = radial / rows as f32 * scene.rim_fill;
    let slot = span / count as f32;
    for i in 0..count {
        let mid = start + slot * (i as f32 + 0.5);
        for row in 0..rows {
            let center_r = cliff.inner + radial * (row as f32) / rows as f32 + depth * 0.5;
            let crest = if row + 1 == rows {
                cliff.outer_height
            } else {
                cliff.inner_height
            };
            let height = crest * scene.rim_inner_scale;
            let stagger = if (row + i) % 2 == 0 {
                scene.rim_stagger_deg
            } else {
                -scene.rim_stagger_deg
            };
            let ang = (mid + stagger).to_radians();
            let width = center_r * slot.to_radians() * scene.rim_fill;
            let run = cliff.outer - cliff.inner;
            let rise = cliff.outer_height - cliff.inner_height;
            let tilt = -(rise / run).atan();
            let lift = row as f32 * cliff.inner_height * scene.rim_inner_scale / rows as f32;
            place_rock(mesh, ang, center_r, [width, height, depth], tilt, lift, scene);
        }
    }
    let chunk = scene.chunk_size;
    let mid = (start + end) * 0.5;
    place_rock(
        mesh,
        (mid + scene.rim_stagger_deg).to_radians(),
        cliff.inner + chunk[2] * 0.5,
        chunk,
        0.0,
        0.0,
        scene,
    );
}

/// Cliff arc with the nest windows cut out, so a nest can sit in the wall.
fn clear_arcs(content: &Content, cliff: &gemini_sim::scenery::Cliff) -> Vec<(f32, f32)> {
    let mut spans = vec![(cliff.angle_deg, cliff.angle_deg + cliff.span_deg)];
    let gap = content.arena_scene.nest_gap_deg;
    for vent in &content.arena.vents {
        let mut next = Vec::new();
        for (start, end) in spans {
            next.extend(cut_span(start, end, vent.bearing_deg, gap));
        }
        spans = next;
    }
    spans
}

fn cut_span(start: f32, end: f32, bearing: f32, gap: f32) -> Vec<(f32, f32)> {
    let mut spans = vec![(start, end)];
    for shift in [-360.0, 0.0, 360.0] {
        let lo = bearing + shift - gap;
        let hi = bearing + shift + gap;
        let mut next = Vec::new();
        for (a, b) in spans {
            if hi <= a || lo >= b {
                next.push((a, b));
                continue;
            }
            if lo > a {
                next.push((a, lo));
            }
            if hi < b {
                next.push((hi, b));
            }
        }
        spans = next;
    }
    spans
}

fn place_rock(
    mesh: &mut MeshData,
    ang: f32,
    radius: f32,
    size: [f32; 3],
    tilt: f32,
    lift: f32,
    scene: &gemini_sim::scenery::ArenaScene,
) {
    let mut rock = MeshData::new();
    let radii = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
    geom::gem(
        &mut rock,
        [0.0, 0.0, 0.0],
        radii,
        scene.boulder_slices,
        scene.boulder_stacks,
        INK,
    );
    geom::rotate_x(&mut rock, tilt);
    geom::rotate_y(&mut rock, ang);
    let (s, c) = ang.sin_cos();
    // The lean pulls the crest inward. Shift the mound out by that amount so it stays outside the floor.
    let shove = tilt.abs().sin() * size[1] * 0.5;
    let placed = radius + shove;
    geom::translate(&mut rock, [s * placed, lift + size[1] * 0.5, c * placed]);
    append_mesh(mesh, &rock);
}

fn append_mesh(mesh: &mut MeshData, extra: &MeshData) {
    let shift = mesh.positions.len() as u32;
    mesh.positions.extend(extra.positions.iter().copied());
    mesh.normals.extend(extra.normals.iter().copied());
    mesh.colors.extend(extra.colors.iter().copied());
    mesh.indices
        .extend(extra.indices.iter().map(|index| index + shift));
}

fn hive_pod(content: &Content, nest: &gemini_sim::scenery::Nest) -> MeshData {
    let scene = &content.arena_scene;
    let mut mesh = MeshData::new();
    let radii = [nest.size[0] * 0.5, nest.size[1] * 0.5, nest.size[2] * 0.5];
    geom::gem(&mut mesh, [0.0, 0.0, 0.0], radii, scene.nest_sides, scene.nest_ribs, INK);
    for rib in 0..scene.nest_ribs {
        let t = (rib as f32 + 1.0) / (scene.nest_ribs as f32 + 1.0);
        let y = (t - 0.5) * nest.size[1] * (1.0 - scene.nest_rib_scale);
        let mut band = MeshData::new();
        geom::box_mesh(
            &mut band,
            [0.0, y, 0.0],
            [
                nest.size[0] * (1.0 + scene.nest_rib_scale),
                nest.size[1] * scene.nest_rib_scale,
                nest.size[2] * (1.0 - scene.nest_rib_scale),
            ],
            INK,
        );
        append_mesh(&mut mesh, &band);
    }
    mesh
}

fn hive_goo(content: &Content, nest: &gemini_sim::scenery::Nest) -> MeshData {
    let scene = &content.arena_scene;
    let mut mesh = MeshData::new();
    let radii = [
        nest.size[0] * nest.goo_scale[0] * 0.5,
        nest.size[1] * nest.goo_scale[1] * 0.5,
        nest.size[2] * nest.goo_scale[2] * 0.5,
    ];
    let front = -nest.size[2] * 0.5;
    let local_z = front + radii[2] * (1.0 + nest.goo_scale[2]);
    geom::gem(
        &mut mesh,
        [0.0, 0.0, local_z],
        radii,
        scene.nest_sides,
        scene.nest_ribs,
        INK,
    );
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
    let mut y = ledge.pos.y - content.arena_scene.ledge_thickness;
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
