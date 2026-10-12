//! One mesh handle per bug part. Joints pose the instances. Distant bugs swap to the LOD mesh.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, Hdr};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::render::view::{ColorGrading, Tonemapping};

use gemini_meshes::{pose, AnimInput, MeshData, Rgba};
use gemini_sim::content::EnemyKind;
use gemini_sim::math::Vec3 as SimVec;
use gemini_sim::sim::{AiState, ShotKind};

use crate::audio;
use crate::session::{Away, FrameFx, Game};

#[derive(Component)]
pub struct PlayPiece;

#[derive(Component)]
pub(crate) struct BugRoot {
    #[allow(dead_code)]
    id: u32,
    #[allow(dead_code)]
    kind: EnemyKind,
}

#[derive(Component)]
pub(crate) struct BugPart {
    #[allow(dead_code)]
    index: usize,
    emissive: bool,
    /// Bud that takes `grown_color` when it regrows.
    grows: bool,
    pod_slot: Option<u32>,
    rest: Handle<StandardMaterial>,
    hot: Handle<StandardMaterial>,
    hot_on: bool,
}

#[derive(Component)]
pub(crate) struct BugLod;

#[derive(Component)]
pub(crate) struct Shot {
    #[allow(dead_code)]
    id: u32,
}

#[derive(Component)]
pub(crate) struct Pickup {
    #[allow(dead_code)]
    id: u32,
}

#[derive(Component)]
pub(crate) struct Flash {
    life: f32,
    /// Seconds the flash fades across. Zero keeps a hard cutoff (hit pings).
    fade: f32,
}

/// Overlay camera. Its children are the first-person arms, on their own layer.
#[derive(Component)]
pub(crate) struct ViewmodelCamera;

/// Orange barrel and stripes. Overdrive swaps them to a hotter emissive. The teal strip stays teal.
#[derive(Component)]
pub(crate) struct BarrelGlow {
    idle: Handle<StandardMaterial>,
    hot: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
pub(crate) struct Bank {
    meshes: HashMap<String, Handle<Mesh>>,
    mats: HashMap<String, Handle<StandardMaterial>>,
    tracer: Handle<Mesh>,
    acid: Handle<Mesh>,
    pickup: Handle<Mesh>,
    flash: Handle<Mesh>,
    /// Round emissive muzzle glow. Hit pings keep `flash`.
    muzzle: Handle<Mesh>,
    muzzle_core: Handle<StandardMaterial>,
    muzzle_halo: Handle<StandardMaterial>,
    ring: Handle<Mesh>,
    barrel_hot: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
pub(crate) struct Links {
    bugs: HashMap<u32, BugLink>,
    shots: HashMap<u32, Entity>,
    pickups: HashMap<u32, Entity>,
    feet: HashMap<u32, Vec<f32>>,
}

struct BugLink {
    root: Entity,
    parts: Vec<Entity>,
    lod: Entity,
}

pub fn scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    game: Res<Game>,
) {
    let content = &game.content;
    let mut bank = Bank::default();
    for visual in gemini_meshes::catalog() {
        for part in &visual.parts {
            bank.meshes.entry(part.mesh_key.clone()).or_insert_with(|| upload(&mut meshes, &part.mesh));
            material(&mut bank, &mut materials, content, &part.palette, part.emissive, false);
            if part.emissive {
                material(&mut bank, &mut materials, content, &part.palette, true, true);
            }
        }
        bank.meshes.entry(visual.lod_key.clone()).or_insert_with(|| upload(&mut meshes, &visual.lod));
        let shell = visual
            .parts
            .iter()
            .find(|p| !p.emissive)
            .map(|p| p.palette.as_str())
            .unwrap_or("brute_amber");
        material(&mut bank, &mut materials, content, shell, false, false);
    }
    for (_, row) in content.bugs.rows() {
        let row = &row.visual;
        if !row.bud_color.is_empty() {
            material(&mut bank, &mut materials, content, &row.bud_color, false, false);
        }
        if !row.grown_color.is_empty() {
            material(&mut bank, &mut materials, content, &row.grown_color, true, true);
        }
    }
    let teal = rgba(content, "shots_player");
    let acid = rgba(content, "spitter_glow");
    let white = rgba(content, "hero_white");
    let accent = rgba(content, "hero_accent");
    bank.tracer = upload(&mut meshes, &gemini_meshes::tracer(teal));
    bank.acid = upload(&mut meshes, &gemini_meshes::tracer(acid));
    bank.pickup = upload(&mut meshes, &gemini_meshes::pickup_beam(accent));
    bank.flash = upload(&mut meshes, &gemini_meshes::gun_core(white));
    bank.muzzle = upload(&mut meshes, &gemini_meshes::muzzle_glow(1.0));
    let flash = content.hero.muzzle_flash_color;
    bank.muzzle_core = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.96, 0.9),
        emissive: LinearRgba::rgb(8.0, 7.4, 6.2),
        unlit: true,
        ..default()
    });
    bank.muzzle_halo = materials.add(StandardMaterial {
        base_color: Color::srgb(flash[0], flash[1], flash[2]),
        emissive: LinearRgba::rgb(flash[0] * 6.0, flash[1] * 6.0, flash[2] * 6.0),
        unlit: true,
        ..default()
    });
    let mut ring = MeshData::new();
    // A flat faceted ring: two discs would be solid. A thin torus-like prism reads as a ring.
    for i in 0..10 {
        let a = i as f32 / 10.0 * std::f32::consts::TAU;
        let x = a.cos() * 0.35;
        let z = a.sin() * 0.35;
        bevy_ring_box(&mut ring, [x, 0.0, z], [0.08, 0.02, 0.12]);
    }
    bank.ring = upload(&mut meshes, &ring);
    material(&mut bank, &mut materials, content, "shots_player", true, false);
    material(&mut bank, &mut materials, content, "spitter_glow", true, false);
    material(&mut bank, &mut materials, content, "hero_white", true, true);
    material(&mut bank, &mut materials, content, "hero_accent", true, false);
    material(&mut bank, &mut materials, content, "hero_cobalt", false, false);
    material(&mut bank, &mut materials, content, "hero_white", false, false);
    material(&mut bank, &mut materials, content, "hero_teal", false, false);
    let trim = gemini_sim::content::hex_srgb(&content.hero.accent);
    let trim_handle = materials.add(StandardMaterial {
        base_color: Color::srgb(trim[0], trim[1], trim[2]),
        emissive: LinearRgba::BLACK,
        unlit: true,
        ..default()
    });
    bank.mats.insert("hero_trim:false:false".into(), trim_handle.clone());
    material(&mut bank, &mut materials, content, "arena_dark", false, false);
    material(&mut bank, &mut materials, content, "arena_light", false, false);

    let arena = &content.arena;
    let ground = upload(
        &mut meshes,
        &box_at([0.0, -0.08, 0.0], [arena.size[0], 0.16, arena.size[1]]),
    );
    commands.spawn((
        Mesh3d(ground),
        MeshMaterial3d(mat_of(&bank, "arena_dark", false, false)),
        Transform::IDENTITY,
        PlayPiece,
    ));
    for piece in gemini_meshes::arena_pieces(content) {
        let mesh = upload(&mut meshes, &piece.mesh);
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat_of(&bank, &piece.palette, false, false)),
            Transform::IDENTITY,
            PlayPiece,
        ));
    }
    for ledge in &arena.ledges {
        let mesh = upload(&mut meshes, &box_at([ledge.pos.x, ledge.pos.y, ledge.pos.z], [2.4, 0.3, 2.4]));
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat_of(&bank, "arena_light", false, false)),
            Transform::IDENTITY,
            PlayPiece,
        ));
    }
    for vent in &arena.vents {
        let mesh = upload(&mut meshes, &box_at([vent.pos.x, 0.2, vent.pos.z], [1.4, 0.4, 1.4]));
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat_of(&bank, "arena_dark", false, false)),
            Transform::IDENTITY,
            PlayPiece,
        ));
    }

    let vm = gemini_meshes::viewmodel(&content.heroes_visual.glove);
    let offset = content.hero.viewmodel_offset;
    let hands = Transform::from_xyz(offset[0], offset[1], offset[2]);
    let barrel_hot = materials.add(StandardMaterial {
        base_color: Color::srgb(trim[0], trim[1], trim[2]),
        emissive: LinearRgba::rgb(trim[0] * 6.0, trim[1] * 6.0, trim[2] * 6.0),
        unlit: true,
        ..default()
    });
    let trim_idle = trim_handle.clone();
    let strip = mat_of(&bank, "hero_teal", true, false);
    let cobalt_idle = mat_of(&bank, "hero_cobalt", false, false);
    // The world camera draws layer 0. The overlay camera draws the arms on
    // layer 1 with its own depth buffer, so the rifle cannot clip into the arena.
    let world_cam = commands
        .spawn((
            Camera3d::default(),
            Camera {
                order: 0,
                ..default()
            },
            RenderLayers::layer(0),
            // Bloom needs this HDR camera. WebGL2 can present the view without the
            // bloom pass; weak points stay unlit in their palette color so they
            // still read when that happens.
            Hdr,
            Bloom::NATURAL,
            Tonemapping::TonyMcMapface,
            ColorGrading::default(),
            IsDefaultUiCamera,
            PlayPiece,
        ))
        .id();
    let vm_cam = commands
        .spawn((
            ViewmodelCamera,
            Camera3d::default(),
            Camera {
                order: 1,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Projection::from(PerspectiveProjection {
                fov: content.hero.viewmodel_fov.to_radians(),
                near: 0.05,
                ..default()
            }),
            RenderLayers::layer(1),
            // Same HDR bloom path as the world camera, so the muzzle flash and the
            // teal strip glow instead of sitting as flat unlit color.
            Hdr,
            Bloom::NATURAL,
            Tonemapping::TonyMcMapface,
            ChildOf(world_cam),
        ))
        .id();
    let layer = RenderLayers::layer(1);
    let pieces = [
        (upload(&mut meshes, &vm.teal), mat_of(&bank, "hero_teal", false, false), None),
        (upload(&mut meshes, &vm.cobalt), cobalt_idle.clone(), None),
        (upload(&mut meshes, &vm.white), mat_of(&bank, "hero_white", false, false), None),
        (
            upload(&mut meshes, &vm.barrel),
            cobalt_idle.clone(),
            Some(BarrelGlow {
                idle: cobalt_idle,
                hot: barrel_hot.clone(),
            }),
        ),
        (
            upload(&mut meshes, &vm.accent),
            trim_idle.clone(),
            Some(BarrelGlow {
                idle: trim_idle,
                hot: barrel_hot.clone(),
            }),
        ),
        (upload(&mut meshes, &vm.core), strip, None),
    ];
    for (mesh, mat, glow) in pieces {
        let mut entity = commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            hands,
            layer.clone(),
            ChildOf(vm_cam),
        ));
        if let Some(glow) = glow {
            entity.insert(glow);
        }
    }
    bank.barrel_hot = barrel_hot;
    commands.insert_resource(bank);
    commands.insert_resource(Links::default());
}

pub fn sync(
    mut commands: Commands,
    game: Res<Game>,
    fx: Res<FrameFx>,
    bank: Res<Bank>,
    mut links: ResMut<Links>,
    mut roots: Query<&mut Transform, (With<BugRoot>, Without<BugPart>, Without<BugLod>, Without<Shot>, Without<Flash>)>,
    mut parts: Query<(&mut BugPart, &mut Transform, &mut Visibility), (Without<BugLod>, Without<BugRoot>, Without<Shot>, Without<Flash>)>,
    mut lods: Query<(&mut Transform, &mut Visibility), (With<BugLod>, Without<BugPart>, Without<BugRoot>, Without<Shot>, Without<Flash>)>,
    mut shots: Query<&mut Transform, (With<Shot>, Without<BugRoot>, Without<BugPart>, Without<BugLod>, Without<Flash>)>,
    mut flashes: Query<
        (Entity, &mut Flash, &mut Transform, &MeshMaterial3d<StandardMaterial>),
        (Without<Shot>, Without<BugRoot>, Without<BugPart>, Without<BugLod>),
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    assets: Res<AssetServer>,
    prefs: Res<crate::settings::Prefs>,
) {
    let player = game.local().cloned();
    let eye = player.as_ref().map(|p| p.pos).unwrap_or(SimVec::ZERO);
    let alive: HashMap<u32, usize> = game
        .sim
        .enemies
        .iter()
        .enumerate()
        .filter(|(_, e)| e.alive)
        .map(|(i, e)| (e.id, i))
        .collect();

    let stale: Vec<u32> = links.bugs.keys().copied().filter(|id| !alive.contains_key(id)).collect();
    for id in stale {
        if let Some(link) = links.bugs.remove(&id) {
            commands.entity(link.root).despawn();
        }
        links.feet.remove(&id);
    }

    for enemy in game.sim.enemies.iter().filter(|e| e.alive) {
        if !links.bugs.contains_key(&enemy.id) {
            let link = spawn_bug(&mut commands, &bank, &game, &mut materials, enemy.kind, enemy.id);
            links.bugs.insert(enemy.id, link);
            links.feet.insert(enemy.id, Vec::new());
        }
        let (root, part_ids, lod) = {
            let link = links.bugs.get(&enemy.id).expect("bug");
            (link.root, link.parts.clone(), link.lod)
        };
        if let Ok(mut xf) = roots.get_mut(root) {
            xf.translation = bevy::math::Vec3::new(enemy.pos.x, enemy.pos.y, enemy.pos.z);
            xf.rotation = Quat::from_rotation_y(enemy.yaw);
        }
        let visual = gemini_meshes::bug(enemy.kind.name());
        let moving = enemy.vel.x * enemy.vel.x + enemy.vel.z * enemy.vel.z > 0.05;
        let charging = match enemy.kind {
            EnemyKind::Spitter => matches!(enemy.ai, AiState::Telegraph { .. }),
            EnemyKind::Brute => matches!(enemy.ai, AiState::Telegraph { .. } | AiState::Lunge { .. }),
            _ => false,
        };
        let prev = links.feet.get(&enemy.id).cloned().unwrap_or_default();
        let (hatch_pod, hatch_charge) = if enemy.kind == EnemyKind::BroodMother && enemy.warn_pod != 255 && enemy.warn_left > 0.0 {
            let tele = game
                .content
                .boss("brood_mother")
                .expect("brood_mother is missing")
                .hatch_telegraph;
            (enemy.warn_pod, hatch_fraction(tele, enemy.warn_left))
        } else {
            (255, 0.0)
        };
        let posed = pose(
            visual,
            AnimInput {
                time: game.sim.time + enemy.id as f32 * 0.17,
                moving,
                charging,
                vulnerable: enemy.weak_exposed,
                phase: enemy.phase as u32,
                frenzy: enemy.kind == EnemyKind::HiveTyrant && enemy.phase >= 1,
                preview_glow: false,
                regrow: if enemy.kind == EnemyKind::BroodMother {
                    enemy.pods.iter().filter(|hp| **hp <= 0.0).count() as u32
                } else {
                    0
                },
                hatch_pod,
                hatch_charge,
            },
            &prev,
        );
        if let Some(slot) = links.feet.get_mut(&enemy.id) {
            *slot = posed.foot_lift.clone();
        }
        if moving {
            let name = game.content.bug(enemy.kind).audio.footstep.clone();
            for _ in &posed.planted {
                audio::footstep(&mut commands, assets.as_ref(), game.as_ref(), &name, enemy.pos, prefs.as_ref());
            }
        }
        let dist = {
            let dx = enemy.pos.x - eye.x;
            let dz = enemy.pos.z - eye.z;
            (dx * dx + dz * dz).sqrt()
        };
        let use_lod = dist > visual.lod_distance;
        for (entity, xf) in part_ids.iter().zip(posed.xforms.iter()) {
            if let Ok((mut part, mut local, mut vis)) = parts.get_mut(*entity) {
                local.translation = bevy::math::Vec3::new(xf.pos[0], xf.pos[1], xf.pos[2]);
                local.rotation = Quat::from_xyzw(xf.rot[0], xf.rot[1], xf.rot[2], xf.rot[3]);
                local.scale = bevy::math::Vec3::new(xf.scale[0], xf.scale[1], xf.scale[2]);
                let pod_dead = part
                    .pod_slot
                    .and_then(|slot| enemy.pods.get(slot as usize).copied())
                    .is_some_and(|hp| hp <= 0.0);
                *vis = if use_lod || !xf.visible || pod_dead {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                };
                let want_hot = (part.emissive || part.grows) && xf.glow > 1.5;
                if part.emissive && part.pod_slot.is_some() {
                    let rgb = game.content.palette.key("swarmer_glow");
                    let mul = 2.8 + xf.glow * 1.7;
                    if let Some(mut material) = materials.get_mut(&part.hot) {
                        material.emissive = LinearRgba::rgb(rgb[0] * mul, rgb[1] * mul, rgb[2] * mul);
                    }
                }
                if want_hot != part.hot_on {
                    part.hot_on = want_hot;
                    let handle = if want_hot { part.hot.clone() } else { part.rest.clone() };
                    commands.entity(*entity).insert(MeshMaterial3d(handle));
                }
            }
        }
        if let Ok((mut local, mut vis)) = lods.get_mut(lod) {
            local.translation = bevy::math::Vec3::ZERO;
            *vis = if use_lod { Visibility::Inherited } else { Visibility::Hidden };
        }
    }

    // Material hot flag lives on BugPart; the insert above doesn't update the component.
    // A follow-up query writes hot_on. Done below via HotFlag removal — see sync_hot.
    let _ = parts;

    let live_shots: HashMap<u32, usize> = game.sim.projectiles.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
    let stale: Vec<u32> = links.shots.keys().copied().filter(|id| !live_shots.contains_key(id)).collect();
    for id in stale {
        if let Some(entity) = links.shots.remove(&id) {
            commands.entity(entity).despawn();
        }
    }
    for shot in &game.sim.projectiles {
        let mesh = if shot.kind == ShotKind::Acid { bank.acid.clone() } else { bank.tracer.clone() };
        let mat = if shot.kind == ShotKind::Acid {
            mat_of(&bank, "spitter_glow", true, false)
        } else {
            mat_of(&bank, "shots_player", true, false)
        };
        let entity = if let Some(entity) = links.shots.get(&shot.id).copied() {
            entity
        } else {
            let entity = commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::from_xyz(shot.pos.x, shot.pos.y, shot.pos.z),
                    Shot { id: shot.id },
                    PlayPiece,
                ))
                .id();
            links.shots.insert(shot.id, entity);
            entity
        };
        if let Ok(mut xf) = shots.get_mut(entity) {
            xf.translation = bevy::math::Vec3::new(shot.pos.x, shot.pos.y, shot.pos.z);
            if shot.vel.length_squared() > 1.0 {
                xf.rotation = Quat::from_rotation_arc(
                    bevy::math::Vec3::Z,
                    bevy::math::Vec3::new(shot.vel.x, shot.vel.y, shot.vel.z).normalize(),
                );
            }
        }
    }

    let live_pick: HashMap<u32, usize> = game.sim.pickups.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
    let stale: Vec<u32> = links.pickups.keys().copied().filter(|id| !live_pick.contains_key(id)).collect();
    for id in stale {
        if let Some(entity) = links.pickups.remove(&id) {
            commands.entity(entity).despawn();
        }
    }
    for pickup in &game.sim.pickups {
        if links.pickups.contains_key(&pickup.id) {
            continue;
        }
        let entity = commands
            .spawn((
                Mesh3d(bank.pickup.clone()),
                MeshMaterial3d(mat_of(&bank, "hero_accent", true, false)),
                Transform::from_xyz(pickup.pos.x, 0.0, pickup.pos.z),
                Pickup { id: pickup.id },
                PlayPiece,
            ))
            .id();
        links.pickups.insert(pickup.id, entity);
    }

    for ping in &fx.pings {
        let scale = if ping.weak { 1.35 } else { 0.7 };
        commands.spawn((
            Mesh3d(bank.flash.clone()),
            MeshMaterial3d(mat_of(&bank, "hero_white", true, true)),
            Transform::from_xyz(ping.pos.x, ping.pos.y, ping.pos.z).with_scale(bevy::math::Vec3::splat(scale)),
            Flash { life: if ping.weak { 0.18 } else { 0.1 }, fade: 0.0 },
            PlayPiece,
        ));
        if ping.weak {
            commands.spawn((
                Mesh3d(bank.ring.clone()),
                MeshMaterial3d(mat_of(&bank, "spitter_glow", true, false)),
                Transform::from_xyz(ping.pos.x, ping.pos.y, ping.pos.z).with_scale(bevy::math::Vec3::splat(1.8)),
                Flash { life: 0.22, fade: 0.0 },
                PlayPiece,
            ));
        }
    }
    let dt = time.delta_secs();
    let accent = game.content.palette.key("accent");
    for (entity, mut flash, mut xf, mat) in &mut flashes {
        flash.life -= dt;
        if flash.fade > 0.0 {
            let t = (flash.life / flash.fade).clamp(0.0, 1.0);
            if let Some(mut material) = materials.get_mut(&mat.0) {
                material.base_color = Color::srgba(accent[0], accent[1], accent[2], t);
                material.emissive = LinearRgba::rgb(accent[0] * 8.0 * t, accent[1] * 8.0 * t, accent[2] * 8.0 * t);
            }
        } else {
            xf.scale *= 1.0 + dt * 4.0;
        }
        if flash.life <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn spawn_bug(
    commands: &mut Commands,
    bank: &Bank,
    game: &Game,
    materials: &mut Assets<StandardMaterial>,
    kind: EnemyKind,
    id: u32,
) -> BugLink {
    let visual = gemini_meshes::bug(kind.name());
    let shell = visual
        .parts
        .iter()
        .find(|p| !p.emissive)
        .map(|p| p.palette.clone())
        .unwrap_or_else(|| "brute_amber".into());
    let root = commands
        .spawn((
            Transform::IDENTITY,
            Visibility::Inherited,
            BugRoot { id, kind },
            PlayPiece,
        ))
        .id();
    let colors = &game.content.bug(kind).visual;
    let mut parts = Vec::new();
    for (index, part) in visual.parts.iter().enumerate() {
        let mesh = bank.meshes.get(&part.mesh_key).cloned().unwrap_or_else(|| bank.tracer.clone());
        let bud = part.grows;
        let rest_key = if bud && !colors.bud_color.is_empty() {
            colors.bud_color.as_str()
        } else {
            part.palette.as_str()
        };
        let rest = mat_of(bank, rest_key, part.emissive && !bud, false);
        let mut hot = if bud && !colors.grown_color.is_empty() {
            mat_of(bank, &colors.grown_color, true, true)
        } else if part.emissive {
            mat_of(bank, &part.palette, true, true)
        } else {
            rest.clone()
        };
        if part.pod_slot.is_some() && part.emissive {
            if let Some(src) = materials.get(&hot).cloned() {
                hot = materials.add(src);
            }
        }
        let pod_slot = part.pod_slot;
        let entity = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(rest.clone()),
                Transform::from_xyz(part.rest_pos[0], part.rest_pos[1], part.rest_pos[2]),
                Visibility::Inherited,
                BugPart {
                    index,
                    emissive: part.emissive,
                    grows: part.grows,
                    pod_slot,
                    rest,
                    hot,
                    hot_on: false,
                },
                ChildOf(root),
            ))
            .id();
        parts.push(entity);
    }
    let lod_mesh = bank.meshes.get(&visual.lod_key).cloned().unwrap_or_else(|| bank.tracer.clone());
    let lod = commands
        .spawn((
            Mesh3d(lod_mesh),
            MeshMaterial3d(mat_of(bank, &shell, false, false)),
            Transform::IDENTITY,
            Visibility::Hidden,
            BugLod,
            ChildOf(root),
        ))
        .id();
    BugLink { root, parts, lod }
}

pub fn grade(
    game: Res<Game>,
    away: Res<Away>,
    mut cameras: Query<&mut ColorGrading, (With<Camera3d>, Without<ViewmodelCamera>)>,
) {
    let hud = &game.content.presentation.hud.away;
    let look = away.view.look.clamp(0.0, 1.0);
    for mut grading in &mut cameras {
        if look > 0.0 {
            let dim = hud.away_dim * look;
            let desat = hud.away_desaturation * look;
            let factor = (1.0 - dim).clamp(hud.dim_floor, 1.0);
            grading.global.exposure = factor.log2();
            grading.global.post_saturation = 1.0 - desat;
            grading.global.temperature = 0.0;
            grading.global.tint = 0.0;
        } else {
            *grading = ColorGrading::default();
        }
    }
}

pub fn camera(
    game: Res<Game>,
    mut cameras: Query<(&mut Transform, &mut Projection), (With<Camera3d>, Without<ViewmodelCamera>)>,
    time: Res<Time>,
) {
    let Some(player) = game.local() else { return };
    let eye_y = game.content.view_eye(player.crouching);
    let speed = (player.vel.x * player.vel.x + player.vel.z * player.vel.z).sqrt();
    let bob = if game.bob && player.grounded && speed > 0.4 {
        let hz = game.content.presentation.view_bob_hz;
        let amp = game.content.controller.camera.view_bob_amplitude;
        (time.elapsed_secs() * hz * std::f32::consts::TAU).sin() * amp
    } else {
        0.0
    };
    let dip = game.dip * game.content.controller.camera.landing_dip;
    let mut fov = game.fov;
    if player.sprinting {
        fov += game.content.controller.camera.fov_sprint_add;
    }
    fov = fov.clamp(game.content.presentation.fov_min, game.content.presentation.fov_max + game.content.controller.camera.fov_sprint_add);
    for (mut xf, mut proj) in &mut cameras {
        xf.translation = bevy::math::Vec3::new(player.pos.x, player.pos.y + eye_y + bob - dip, player.pos.z);
        xf.rotation = Quat::from_rotation_y(game.yaw) * Quat::from_rotation_x(game.pitch);
        if let Projection::Perspective(persp) = proj.as_mut() {
            persp.fov = fov.to_radians();
        }
    }
}

pub fn sync_viewmodel(
    mut commands: Commands,
    game: Res<Game>,
    fx: Res<FrameFx>,
    bank: Res<Bank>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut barrels: Query<(&BarrelGlow, &mut MeshMaterial3d<StandardMaterial>)>,
    cam: Query<Entity, With<ViewmodelCamera>>,
) {
    let Some(player) = game.local() else { return };
    let overdrive = player.overdrive_left > 0.0;
    for (glow, mut material) in &mut barrels {
        material.0 = if overdrive { glow.hot.clone() } else { glow.idle.clone() };
    }
    let fired = fx
        .sounds
        .iter()
        .any(|sound| sound.id == format!("{}_fire", game.content.hero.id));
    if !(overdrive && fired) {
        return;
    }
    let Ok(cam) = cam.single() else { return };
    let hero = &game.content.hero;
    let offset = hero.viewmodel_offset;
    let local = gemini_meshes::muzzle_local();
    let tip = [offset[0] + local[0], offset[1] + local[1], offset[2] + local[2]];
    let (width, height) = windows
        .iter()
        .next()
        .map(|window| (window.width(), window.height()))
        .unwrap_or((1280.0, 720.0));
    let at = gemini_meshes::muzzle_on_barrel(tip, hero.viewmodel_fov, width, height, hero.muzzle_flash_tip_px);
    let place = Transform::from_xyz(at[0], at[1], at[2]);
    commands.spawn((
        Mesh3d(bank.muzzle.clone()),
        MeshMaterial3d(bank.muzzle_halo.clone()),
        place.with_scale(Vec3::splat(hero.muzzle_flash_radius)),
        RenderLayers::layer(1),
        Flash { life: hero.muzzle_flash_secs, fade: 0.0 },
        PlayPiece,
        ChildOf(cam),
    ));
    commands.spawn((
        Mesh3d(bank.muzzle.clone()),
        MeshMaterial3d(bank.muzzle_core.clone()),
        place.with_scale(Vec3::splat(hero.muzzle_flash_radius * hero.muzzle_flash_core)),
        RenderLayers::layer(1),
        Flash { life: hero.muzzle_flash_secs, fade: 0.0 },
        PlayPiece,
        ChildOf(cam),
    ));
}

fn upload(meshes: &mut Assets<Mesh>, data: &MeshData) -> Handle<Mesh> {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals.clone());
    let uvs = vec![[0.0, 0.0]; data.positions.len()];
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(data.indices.clone()));
    meshes.add(mesh)
}

fn material(
    bank: &mut Bank,
    materials: &mut Assets<StandardMaterial>,
    content: &gemini_sim::content::Content,
    key: &str,
    emissive: bool,
    hot: bool,
) -> Handle<StandardMaterial> {
    let id = format!("{key}:{emissive}:{hot}");
    if let Some(handle) = bank.mats.get(&id) {
        return handle.clone();
    }
    let rgb = content.palette.key(key);
    let mul = if hot { 6.0 } else if emissive { 2.4 } else { 0.0 };
    let handle = materials.add(StandardMaterial {
        base_color: Color::srgb(rgb[0], rgb[1], rgb[2]),
        emissive: LinearRgba::rgb(rgb[0] * mul, rgb[1] * mul, rgb[2] * mul),
        unlit: true,
        ..default()
    });
    bank.mats.insert(id, handle.clone());
    handle
}

fn mat_of(bank: &Bank, key: &str, emissive: bool, hot: bool) -> Handle<StandardMaterial> {
    let id = format!("{key}:{emissive}:{hot}");
    bank.mats.get(&id).cloned().unwrap_or_else(|| {
        bank.mats.values().next().cloned().unwrap_or_default()
    })
}

fn rgba(content: &gemini_sim::content::Content, key: &str) -> Rgba {
    Rgba(content.palette.key(key))
}

fn box_at(center: [f32; 3], size: [f32; 3]) -> MeshData {
    let mut mesh = MeshData::new();
    // Local box; caller places it with a transform when center is baked in.
    let hx = size[0] * 0.5;
    let hy = size[1] * 0.5;
    let hz = size[2] * 0.5;
    let c = [0.7, 0.7, 0.75, 1.0];
    let faces = [
        ([0.0, 1.0, 0.0], [[-hx, hy, -hz], [hx, hy, -hz], [hx, hy, hz], [-hx, hy, hz]]),
        ([0.0, -1.0, 0.0], [[-hx, -hy, hz], [hx, -hy, hz], [hx, -hy, -hz], [-hx, -hy, -hz]]),
        ([0.0, 0.0, 1.0], [[-hx, -hy, hz], [-hx, hy, hz], [hx, hy, hz], [hx, -hy, hz]]),
        ([0.0, 0.0, -1.0], [[hx, -hy, -hz], [hx, hy, -hz], [-hx, hy, -hz], [-hx, -hy, -hz]]),
        ([1.0, 0.0, 0.0], [[hx, -hy, hz], [hx, hy, hz], [hx, hy, -hz], [hx, -hy, -hz]]),
        ([-1.0, 0.0, 0.0], [[-hx, -hy, -hz], [-hx, hy, -hz], [-hx, hy, hz], [-hx, -hy, hz]]),
    ];
    for (n, corners) in faces {
        let base = mesh.positions.len() as u32;
        for corner in corners {
            mesh.positions.push([center[0] + corner[0], center[1] + corner[1], center[2] + corner[2]]);
            mesh.normals.push(n);
            mesh.colors.push(c);
        }
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    mesh
}

/// Charge 0 at the start of the telegraph, 1 as warn_left hits 0.
/// A non-positive duration panics.
fn hatch_fraction(telegraph: f32, warn_left: f32) -> f32 {
    if telegraph <= 0.0 {
        panic!("brood_mother hatch_telegraph must be > 0");
    }
    (1.0 - warn_left / telegraph).clamp(0.0, 1.0)
}

fn bevy_ring_box(mesh: &mut MeshData, center: [f32; 3], size: [f32; 3]) {
    let piece = box_at(center, size);
    let base = mesh.positions.len() as u32;
    mesh.positions.extend_from_slice(&piece.positions);
    mesh.normals.extend_from_slice(&piece.normals);
    mesh.colors.extend_from_slice(&piece.colors);
    mesh.indices.extend(piece.indices.iter().map(|i| i + base));
}

#[cfg(test)]
mod tests {
    use super::hatch_fraction;

    #[test]
    fn hatch_charge_follows_the_loaded_telegraph() {
        let mid = hatch_fraction(1.5, 0.75);
        assert!((mid - 0.5).abs() < 1e-4, "{mid}");
        assert!((hatch_fraction(1.5, 1.5) - 0.0).abs() < 1e-4);
        assert!((hatch_fraction(1.5, 0.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    #[should_panic(expected = "hatch_telegraph must be > 0")]
    fn zero_hatch_telegraph_is_a_hard_failure() {
        hatch_fraction(0.0, 0.4);
    }
}
