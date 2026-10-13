//! Amber wave bar, teal crosshair, fuel and health arcs. Magenta only on a boss weak point.

use bevy::picking::hover::Hovered;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};
use bevy::window::{CursorGrabMode, CursorOptions};
use gemini_sim::content::{EnemyKind, StackRule};
use gemini_sim::math::Vec3 as SimVec;
use gemini_sim::sim::Phase;

use crate::session::{Away, FrameFx, Game, PauseMenu};
use crate::world::PlayPiece;

#[derive(Component)]
pub(crate) struct HudNode;

#[derive(Component)]
pub(crate) struct HudLabel;

#[derive(Component)]
pub(crate) struct HudFill;

#[derive(Component)]
pub(crate) struct HudAmmo;

#[derive(Component)]
pub(crate) struct HudPrompt;

/// One offered upgrade. The number is the keyboard hint and the pick index.
#[derive(Component)]
pub(crate) struct UpgradeCard(pub u8);

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
    away: Res<Away>,
    fx: Res<FrameFx>,
    time: Res<Time>,
    // Only the root. Despawning it already despawns `HudNode` children.
    old: Query<Entity, (With<HudNode>, Without<ChildOf>)>,
    camera: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<crate::world::ViewmodelCamera>)>,
    mut floaters: Query<(Entity, &mut Floater, &mut Node), Without<HudFill>>,
    mut shape: Local<HudShape>,
    mut labels: Query<&mut Text, With<HudLabel>>,
    mut fills: Query<&mut Node, (With<HudFill>, Without<Floater>)>,
    mut ammos: Query<&mut Text, (With<HudAmmo>, Without<HudLabel>)>,
    mut prompts: Query<&mut Text, (With<HudPrompt>, Without<HudLabel>, Without<HudAmmo>)>,
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
    let Some(player) = game.local().cloned() else { return };
    let teal = color(&game, "hero_teal");
    let white = color(&game, "hero_white");
    let accent_rgb = gemini_sim::content::hex_srgb(&game.content.hero.accent);
    let accent = Color::srgb(accent_rgb[0], accent_rgb[1], accent_rgb[2]);
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
    let max_cd = game.content.hero.ability_cooldown.max(game.content.hero.cooldown_floor);
    let ready = cd <= 0.0;
    let boss_id = boss.map(|enemy| enemy.kind.name()).unwrap_or("");
    let upgrade_sig: String = player.upgrades.keys().cloned().collect();
    let overdrive = player.overdrive_left > 0.0;
    let prompt = revive_line(&game, &player);
    let kind = prompt_kind(prompt.as_deref());
    let key = format!(
        "{:?}|{boss_id}|{}|{upgrade_sig}|{}|{}|{}|{}|{}|{kind}",
        std::mem::discriminant(&game.sim.phase),
        player.downed as u8,
        (hp * 14.0) as i32,
        (fuel * 14.0) as i32,
        if ready { 14 } else { ((1.0 - cd / max_cd) * 14.0) as i32 },
        notches.len(),
        overdrive as u8,
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
        if let Some(line) = revive_line(&game, &player) {
            if let Ok(mut text) = prompts.single_mut() {
                text.0 = line;
            }
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
            // The root covers the screen. It must not eat pause-menu clicks.
            Pickable::IGNORE,
        ))
        .id();
    if overdrive {
        let wash = Color::srgba(accent_rgb[0], accent_rgb[1], accent_rgb[2], 0.22);
        let band = 64.0;
        for node in [
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                height: Val::Px(band),
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                height: Val::Px(band),
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Px(band),
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                right: Val::Px(0.0),
                width: Val::Px(band),
                ..default()
            },
        ] {
            commands.spawn((node, BackgroundColor(wash), ChildOf(root), HudNode));
        }
    }
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

    if let Some(line) = prompt {
        commands.spawn((
            Text::new(line),
            TextFont::from_font_size(28.0),
            TextColor(white),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(62.0),
                margin: UiRect::new(Val::Px(-180.0), Val::Auto, Val::Auto, Val::Auto),
                ..default()
            },
            ChildOf(root),
            HudNode,
            HudPrompt,
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
            let hint = i as u8 + 1;
            commands
                .spawn((
                    Button,
                    Hovered::default(),
                    UpgradeCard(i as u8),
                    Text::new(format!("PRESS {hint}\n{id}\n{stat}")),
                    TextFont::from_font_size(18.0),
                    TextColor(white),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(50.0),
                        top: Val::Px(180.0 + i as f32 * 72.0),
                        width: Val::Px(280.0),
                        min_height: Val::Px(64.0),
                        padding: UiRect::all(Val::Px(10.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        margin: UiRect::new(Val::Px(-140.0), Val::Auto, Val::Auto, Val::Auto),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(panel_fill(&game)),
                    BorderColor::all(border),
                    ChildOf(root),
                    HudNode,
                ))
                .observe(on_upgrade_card);
        }
    }

    paint_away(&mut commands, root, &game, &away);

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

/// Shown when a wasm pointer lock after an upgrade pick did not stick.
#[derive(Component)]
pub(crate) struct ResumePrompt;

pub fn resume_prompt(
    mut commands: Commands,
    hold: Res<PauseMenu>,
    cursor: Query<&CursorOptions>,
    existing: Query<Entity, With<ResumePrompt>>,
) {
    let locked = cursor.iter().any(|options| options.grab_mode == CursorGrabMode::Locked);
    let show = hold.resume_click && !locked;
    if show {
        if existing.is_empty() {
            commands.spawn((
                Text::new("CLICK TO RESUME"),
                TextFont::from_font_size(28.0),
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(50.0),
                    top: Val::Percent(58.0),
                    margin: UiRect::new(Val::Px(-120.0), Val::Auto, Val::Auto, Val::Auto),
                    ..default()
                },
                ResumePrompt,
                PlayPiece,
            ));
        }
    } else {
        for entity in &existing {
            commands.entity(entity).despawn();
        }
    }
}

fn on_upgrade_card(activate: On<Activate>, cards: Query<&UpgradeCard>, mut game: ResMut<Game>) {
    let Ok(card) = cards.get(activate.entity) else { return };
    game.pending_pick = Some(card.0);
}

/// Solo down is a self-revive on hold. A living teammate holding E revives the downed player.
pub(crate) fn revive_line(game: &Game, player: &gemini_sim::sim::Player) -> Option<String> {
    let rules = &game.content.controller.downed;
    let solo = game.sim.connected_count() <= 1;
    if player.downed {
        if solo && player.self_revives_used < rules.solo_self_revives_per_wave {
            let need = rules.solo_self_revive_time;
            let pct = (player.self_revive_held / need * 100.0).clamp(0.0, 100.0);
            return Some(format!("HOLD E TO REVIVE  {pct:.0}%"));
        }
        if !solo && player.revive_progress > 0.0 {
            let need = rules.revive_time;
            let pct = (player.revive_progress / need * 100.0).clamp(0.0, 100.0);
            return Some(format!("REVIVING  {pct:.0}%"));
        }
        return Some(if solo { "DOWN".into() } else { "WAITING FOR REVIVE".into() });
    }
    let range = game.content.combat.revive_range;
    let near = game.sim.players.iter().any(|other| {
        other.token != player.token
            && other.connected
            && other.alive
            && other.downed
            && flat_distance(player.pos, other.pos) <= range
    });
    if near { Some("HOLD E TO REVIVE TEAMMATE".into()) } else { None }
}

fn prompt_kind(line: Option<&str>) -> u8 {
    match line {
        Some(text) if text.starts_with("HOLD E TO REVIVE TEAMMATE") => 2,
        Some(text) if text.starts_with("HOLD E") => 1,
        Some(text) if text.starts_with("REVIVING") => 3,
        Some(_) => 4,
        None => 0,
    }
}

fn flat_distance(a: SimVec, b: SimVec) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    (dx * dx + dz * dz).sqrt()
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

fn paint_away(commands: &mut Commands, root: Entity, game: &Game, away: &Away) {
    let hud = &game.content.presentation.hud.away;
    let slate = game.content.palette.key(&hud.slate);
    let ink = game.content.palette.key(&hud.text);
    let bar = game.content.palette.key(&hud.bar);
    let count = game.content.palette.key(&hud.count);
    if away.view.banner {
        let copy = if away.view.solo { &hud.solo } else { &hud.coop };
        let panel = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(hud.panel_top),
                    left: Val::Percent(50.0),
                    margin: UiRect::new(Val::Px(-hud.panel_width * 0.5), Val::Auto, Val::Auto, Val::Auto),
                    width: Val::Px(hud.panel_width),
                    height: Val::Px(hud.panel_height),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: Val::Px(hud.row_gap),
                    padding: UiRect::horizontal(Val::Px(hud.panel_pad)),
                    ..default()
                },
                BackgroundColor(Color::srgba(slate[0], slate[1], slate[2], hud.panel_opacity)),
                ChildOf(root),
                HudNode,
            ))
            .id();
        commands.spawn((
            Text::new(copy.text.clone()),
            TextFont::from_font_size(hud.title_px),
            TextColor(Color::srgb(ink[0], ink[1], ink[2])),
            ChildOf(panel),
            HudNode,
        ));
        if copy.show_timer {
            let track = commands
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(hud.bar_height),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(
                        slate[0] * hud.bar_track,
                        slate[1] * hud.bar_track,
                        slate[2] * hud.bar_track,
                        1.0,
                    )),
                    ChildOf(panel),
                    HudNode,
                ))
                .id();
            let fill = away.view.timer_fill.clamp(0.0, 1.0);
            commands.spawn((
                Node {
                    width: Val::Percent(fill * 100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(bar[0], bar[1], bar[2])),
                ChildOf(track),
                HudNode,
            ));
        }
    }
    if let Some(digit) = away.view.digit {
        let u = away.view.digit_u.clamp(0.0, 1.0);
        let scale = hud.count_scale_from + (hud.count_scale_to - hud.count_scale_from) * u;
        let alpha = (1.0 - u).clamp(0.0, 1.0);
        let outline = game.content.palette.key(&hud.count_outline);
        let shadow = game.content.palette.key(&hud.count_shadow);
        let shift = hud.count_outline_px;
        let label = digit.to_string();
        let place = |dx: f32, dy: f32| Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            margin: UiRect::new(
                Val::Px(-hud.count_px * scale * 0.35 + dx),
                Val::Auto,
                Val::Px(-hud.count_px * scale * 0.55 + dy),
                Val::Auto,
            ),
            ..default()
        };
        let drop = hud.count_shadow_px;
        let soft = hud.count_shadow_soft_px;
        let shadow_marks: Vec<(f32, f32, f32)> = hud
            .count_shadow_samples
            .iter()
            .map(|sample| (drop + sample.ox * soft, drop + sample.oy * soft, sample.weight))
            .collect();
        for (dx, dy, weight) in shadow_marks {
            commands.spawn((
                Text::new(label.clone()),
                TextFont::from_font_size(hud.count_px * scale),
                TextColor(Color::srgba(shadow[0], shadow[1], shadow[2], hud.count_shadow_alpha * weight * alpha)),
                place(dx, dy),
                ChildOf(root),
                HudNode,
            ));
        }
        for (dx, dy) in [
            (-shift, 0.0),
            (shift, 0.0),
            (0.0, -shift),
            (0.0, shift),
            (-shift, -shift),
            (shift, -shift),
            (-shift, shift),
            (shift, shift),
        ] {
            commands.spawn((
                Text::new(label.clone()),
                TextFont::from_font_size(hud.count_px * scale),
                TextColor(Color::srgba(outline[0], outline[1], outline[2], alpha)),
                place(dx, dy),
                ChildOf(root),
                HudNode,
            ));
        }
        commands.spawn((
            Text::new(label),
            TextFont::from_font_size(hud.count_px * scale),
            TextColor(Color::srgba(count[0], count[1], count[2], alpha)),
            place(0.0, 0.0),
            ChildOf(root),
            HudNode,
        ));
    }
}

fn color(game: &Game, key: &str) -> Color {
    let c = game.content.palette.key(key);
    Color::srgb(c[0], c[1], c[2])
}

fn panel_fill(game: &Game) -> Color {
    let c = game.content.palette.key(&game.content.ui.colors.panel);
    Color::srgba(c[0], c[1], c[2], game.content.ui.layout.panel_alpha)
}
