//! Amber wave bar, teal crosshair, fuel and health arcs. Magenta only on a boss weak point.

use bevy::prelude::*;
use gemini_sim::content::{EnemyKind, StackRule};
use gemini_sim::sim::Phase;

use crate::session::{FrameFx, Game};
use crate::world::PlayPiece;

#[derive(Component)]
pub(crate) struct HudNode;

#[derive(Component)]
pub(crate) struct HudLabel;

#[derive(Component)]
pub(crate) struct HudFill;

#[derive(Component)]
pub(crate) struct HudAmmo;

#[derive(Default)]
pub(crate) struct HudShape {
    key: String,
}

#[derive(Component)]
pub(crate) struct Floater {
    world: Vec3,
    age: f32,
    #[allow(dead_code)]
    weak: bool,
}

pub fn draw(
    mut commands: Commands,
    game: Res<Game>,
    fx: Res<FrameFx>,
    time: Res<Time>,
    // Only the root. Despawning it already despawns `HudNode` children.
    old: Query<Entity, (With<HudNode>, Without<ChildOf>)>,
    camera: Query<(&Camera, &GlobalTransform)>,
    mut floaters: Query<(Entity, &mut Floater, &mut Node), Without<HudFill>>,
    mut shape: Local<HudShape>,
    mut labels: Query<&mut Text, With<HudLabel>>,
    mut fills: Query<&mut Node, (With<HudFill>, Without<Floater>)>,
    mut ammos: Query<&mut Text, (With<HudAmmo>, Without<HudLabel>)>,
) {
    let dt = time.delta_secs();
    for (entity, mut floater, mut node) in &mut floaters {
        floater.age += dt;
        if floater.age > 0.7 {
            commands.entity(entity).despawn();
            continue;
        }
        if let Ok((cam, xf)) = camera.single() {
            let risen = floater.world + Vec3::Y * floater.age * 0.8;
            if let Ok(screen) = cam.world_to_viewport(xf, risen) {
                node.left = Val::Px(screen.x);
                node.top = Val::Px(screen.y);
            }
        }
    }
    let Some(player) = game.sim.players.first() else { return };
    let teal = color(&game, "hero_teal");
    let white = color(&game, "hero_white");
    let accent = color(&game, "hero_accent");
    let amber = color(&game, "brute_amber");
    let magenta = color(&game, "spitter_glow");
    let boss = game.sim.enemies.iter().find(|e| {
        e.alive && matches!(e.kind, EnemyKind::BroodMother | EnemyKind::HiveTyrant)
    });
    let (label, fill, bar, notches) = if let Some(boss) = boss {
        let frac = if boss.max_hp > 0.0 { boss.hp / boss.max_hp } else { 0.0 };
        let exposed = match boss.kind {
            EnemyKind::HiveTyrant => boss.phase >= 1,
            _ => boss.weak_exposed,
        };
        let name = match boss.kind {
            EnemyKind::BroodMother => "BROOD MOTHER",
            _ => "HIVE TYRANT",
        };
        let thresholds = game
            .content
            .boss(boss.kind.name())
            .map(|b| b.phase_thresholds.clone())
            .unwrap_or_default();
        (name.to_string(), frac, if exposed { magenta } else { amber }, thresholds)
    } else if let Phase::Breather { left } = &game.sim.phase {
        (format!("{left:.0}"), 0.0, teal, Vec::new())
    } else {
        let spawned = game.sim.director.spawned_this_wave.max(1) as f32;
        let left = game.sim.enemies_left() as f32 / spawned;
        (
            format!("WAVE {}", game.sim.director.wave),
            left.clamp(0.0, 1.0),
            amber,
            Vec::new(),
        )
    };
    let hp = if player.max_hp > 0.0 { player.hp / player.max_hp } else { 0.0 };
    let fuel_max = game.content.controller.boost.fuel_max;
    let fuel = if fuel_max > 0.0 { player.fuel / fuel_max } else { 0.0 };
    let cd = player.ability_cd;
    let max_cd = game.content.controller.ability.default_cooldown.max(0.1);
    let ready = cd <= 0.0;
    let boss_id = boss.map(|enemy| enemy.kind.name()).unwrap_or("");
    let upgrade_sig: String = player.upgrades.keys().cloned().collect();
    let key = format!(
        "{:?}|{boss_id}|{}|{upgrade_sig}|{}|{}|{}|{}",
        std::mem::discriminant(&game.sim.phase),
        player.downed as u8,
        (hp * 14.0) as i32,
        (fuel * 14.0) as i32,
        if ready { 14 } else { ((1.0 - cd / max_cd) * 14.0) as i32 },
        notches.len(),
    );
    if !old.is_empty() && shape.key == key {
        if let Ok(mut text) = labels.single_mut() {
            text.0 = label;
        }
        if let Ok(mut node) = fills.single_mut() {
            node.width = Val::Px(280.0 * fill.clamp(0.0, 1.0));
        }
        if let Ok(mut text) = ammos.single_mut() {
            let gun = if player.downed { &player.sidearm } else { &player.primary };
            text.0 = format!("{}  /  {}", gun.ammo, gun.reserve);
        }
        return;
    }
    for entity in &old {
        commands.entity(entity).despawn();
    }
    shape.key = key;
    let root = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            HudNode,
            PlayPiece,
        ))
        .id();
    commands.spawn((
        Text::new(label),
        TextFont::from_font_size(22.0),
        TextColor(if matches!(game.sim.phase, Phase::Breather { .. }) { teal } else { white }),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(18.0),
            left: Val::Percent(50.0),
            margin: UiRect::new(Val::Px(-70.0), Val::Auto, Val::Auto, Val::Auto),
            ..default()
        },
        ChildOf(root),
        HudNode,
        HudLabel,
    ));
    bar_widget(&mut commands, root, 280.0, 10.0, fill, bar, &notches, white);

    // Squad: one accent helmet, solo.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(24.0),
            top: Val::Px(28.0),
            width: Val::Px(28.0),
            height: Val::Px(28.0),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(accent),
        BorderColor::all(white),
        ChildOf(root),
        HudNode,
    ));

    arc(&mut commands, root, 78.0, 78.0, 46.0, hp.clamp(0.0, 1.0), teal);
    arc(&mut commands, root, 168.0, 78.0, 36.0, fuel.clamp(0.0, 1.0), white);
    let ready_flash = ready && (game.sim.time * 3.0).sin() > 0.2;
    arc(
        &mut commands,
        root,
        250.0,
        70.0,
        22.0,
        if ready { 1.0 } else { 1.0 - (cd / max_cd).clamp(0.0, 1.0) },
        if ready_flash { teal } else { white },
    );

    let mut shown = 0usize;
    let total: u32 = player.upgrades.values().sum();
    for (id, stacks) in &player.upgrades {
        if shown >= 12 {
            break;
        }
        let def = game.content.upgrade(id);
        let unique = def.map(|d| d.stack == StackRule::Unique).unwrap_or(false);
        let border = def
            .map(|d| match d.rarity.as_str() {
                "legendary" => color(&game, "rarity_legendary"),
                "epic" => color(&game, "rarity_epic"),
                "rare" => color(&game, "rarity_rare"),
                _ => color(&game, "rarity_common"),
            })
            .unwrap_or(white);
        let x = 24.0 + shown as f32 * 28.0;
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(x),
                bottom: Val::Px(168.0),
                width: Val::Px(22.0),
                height: Val::Px(22.0),
                border: UiRect::all(Val::Px(if unique { 2.0 } else { 1.0 })),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
            BorderColor::all(if unique { border } else { Color::srgba(1.0, 1.0, 1.0, 0.25) }),
            ChildOf(root),
            HudNode,
        ));
        commands.spawn((
            Text::new(format!("{stacks}")),
            TextFont::from_font_size(12.0),
            TextColor(white),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(x + 4.0),
                bottom: Val::Px(170.0),
                ..default()
            },
            ChildOf(root),
            HudNode,
        ));
        shown += 1;
    }
    if player.upgrades.len() > 12 {
        let extra = total.saturating_sub(12);
        commands.spawn((
            Text::new(format!("+{extra}")),
            TextFont::from_font_size(14.0),
            TextColor(white),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(24.0 + 12.0 * 28.0),
                bottom: Val::Px(170.0),
                ..default()
            },
            ChildOf(root),
            HudNode,
        ));
    }

    let gun = if player.downed { &player.sidearm } else { &player.primary };
    commands.spawn((
        Text::new(format!("{}  /  {}", gun.ammo, gun.reserve)),
        TextFont::from_font_size(26.0),
        TextColor(white),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(28.0),
            bottom: Val::Px(28.0),
            ..default()
        },
        ChildOf(root),
        HudNode,
        HudAmmo,
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            width: Val::Px(4.0),
            height: Val::Px(4.0),
            margin: UiRect::all(Val::Px(-2.0)),
            ..default()
        },
        BackgroundColor(teal),
        ChildOf(root),
        HudNode,
    ));

    if player.downed {
        let cap = game.content.controller.downed.solo_self_revives_per_wave;
        let need = game.content.controller.downed.solo_self_revive_time.max(0.1);
        let text = if player.self_revives_used < cap {
            format!("HOLD E  {:.0}%", (player.self_revive_held / need * 100.0).min(100.0))
        } else {
            "DOWN".into()
        };
        commands.spawn((
            Text::new(text),
            TextFont::from_font_size(28.0),
            TextColor(white),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(62.0),
                margin: UiRect::new(Val::Px(-90.0), Val::Auto, Val::Auto, Val::Auto),
                ..default()
            },
            ChildOf(root),
            HudNode,
        ));
    }

    if let Phase::Upgrade { options } = &game.sim.phase {
        for (i, id) in options.iter().enumerate() {
            let def = game.content.upgrade(id);
            let rarity = def.map(|d| d.rarity.as_str()).unwrap_or("common");
            let border = match rarity {
                "legendary" => color(&game, "rarity_legendary"),
                "epic" => color(&game, "rarity_epic"),
                "rare" => color(&game, "rarity_rare"),
                _ => color(&game, "rarity_common"),
            };
            let stat = def.map(|d| d.stat.as_str()).unwrap_or(id);
            commands.spawn((
                Text::new(format!("{}  {id}\n{stat}", i + 1)),
                TextFont::from_font_size(18.0),
                TextColor(white),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Px(180.0 + i as f32 * 64.0),
                    width: Val::Px(280.0),
                    padding: UiRect::all(Val::Px(10.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    margin: UiRect::new(Val::Px(-140.0), Val::Auto, Val::Auto, Val::Auto),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.85)),
                BorderColor::all(border),
                ChildOf(root),
                HudNode,
            ));
        }
    }

    let end = match game.sim.phase {
        Phase::Victory => Some("VICTORY\nENTER"),
        Phase::Defeat => Some("DEFEAT\nENTER"),
        _ => None,
    };
    if let Some(text) = end {
        commands.spawn((
            Text::new(text),
            TextFont::from_font_size(42.0),
            TextColor(teal),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(40.0),
                margin: UiRect::new(Val::Px(-90.0), Val::Auto, Val::Auto, Val::Auto),
                ..default()
            },
            ChildOf(root),
            HudNode,
        ));
    }

    if let Ok((cam, xf)) = camera.single() {
        for ping in &fx.pings {
            let world = Vec3::new(ping.pos.x, ping.pos.y + 0.4, ping.pos.z);
            if let Ok(screen) = cam.world_to_viewport(xf, world) {
                commands.spawn((
                    Text::new(format!("{}", ping.amount.round() as i32)),
                    TextFont::from_font_size(if ping.weak { 28.0 } else { 16.0 }),
                    TextColor(if ping.weak { white } else { amber }),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(screen.x),
                        top: Val::Px(screen.y),
                        ..default()
                    },
                    Floater {
                        world,
                        age: 0.0,
                        weak: ping.weak,
                    },
                    PlayPiece,
                ));
            }
        }
    }
}

fn bar_widget(commands: &mut Commands, root: Entity, width: f32, height: f32, fill: f32, color: Color, notches: &[f32], mark: Color) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(48.0),
            left: Val::Percent(50.0),
            width: Val::Px(width),
            height: Val::Px(height),
            margin: UiRect::new(Val::Px(-width * 0.5), Val::Auto, Val::Auto, Val::Auto),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.12)),
        ChildOf(root),
        HudNode,
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(48.0),
            left: Val::Percent(50.0),
            width: Val::Px(width * fill.clamp(0.0, 1.0)),
            height: Val::Px(height),
            margin: UiRect::new(Val::Px(-width * 0.5), Val::Auto, Val::Auto, Val::Auto),
            border_radius: BorderRadius::all(Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(color),
        ChildOf(root),
        HudNode,
        HudFill,
    ));
    for notch in notches {
        let x = -width * 0.5 + width * notch.clamp(0.0, 1.0);
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(44.0),
                left: Val::Percent(50.0),
                width: Val::Px(2.0),
                height: Val::Px(height + 8.0),
                margin: UiRect::new(Val::Px(x), Val::Auto, Val::Auto, Val::Auto),
                ..default()
            },
            BackgroundColor(mark),
            ChildOf(root),
            HudNode,
        ));
    }
}

fn arc(commands: &mut Commands, root: Entity, cx: f32, bottom: f32, radius: f32, frac: f32, color: Color) {
    let segments = 14;
    for i in 0..segments {
        let t = i as f32 / segments as f32;
        if t > frac {
            continue;
        }
        let ang = (200.0 + t * 140.0).to_radians();
        let x = cx + ang.cos() * radius;
        let y = bottom + ang.sin() * radius * 0.15 + (1.0 - t) * 8.0;
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(x),
                bottom: Val::Px(y),
                width: Val::Px(7.0),
                height: Val::Px(7.0),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(color),
            ChildOf(root),
            HudNode,
        ));
    }
}

fn color(game: &Game, key: &str) -> Color {
    let c = game.content.palette.key(key);
    Color::srgb(c[0], c[1], c[2])
}
