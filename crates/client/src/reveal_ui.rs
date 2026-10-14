//! Full-screen playback of a server crate result. Timings, sounds, and colors come from the tables.

use bevy::audio::Volume;
use bevy::prelude::*;

use gemini_sim::content::Content;
use gemini_sim::reveal::{look, GridRun, RefundRun, RevealRun};

use crate::AppState;

#[derive(Clone, Debug)]
pub enum RevealShow {
    Single(RevealRun),
    Grid(GridRun),
    Refund(RefundRun),
}

#[derive(Resource)]
pub struct RevealPlayback(pub RevealShow);

/// Where Enter returns after the card. The crates screen stays on its route.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RevealFrom {
    #[default]
    Menu,
    Crates,
}

#[derive(Component)]
pub(crate) struct RevealRoot;

#[derive(Component)]
pub(crate) struct RevealVoice {
    pub id: String,
}

pub fn tick(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    assets: Res<AssetServer>,
    mut playback: ResMut<RevealPlayback>,
    mut next: ResMut<NextState<AppState>>,
    old: Query<Entity, With<RevealRoot>>,
    voices: Query<Entity, With<RevealVoice>>,
    prefs: Option<Res<crate::settings::Prefs>>,
) {
    let content = Content::embedded();
    let skip = keys.just_pressed(KeyCode::Space) || mouse.just_pressed(MouseButton::Left);
    if skip {
        for entity in &voices {
            commands.entity(entity).despawn();
        }
    }
    let dt = time.delta_secs();
    let bus = prefs.as_ref().map(|prefs| crate::audio::bus_gain(prefs, false)).unwrap_or(1.0);
    let enter = keys.just_pressed(KeyCode::Enter);
    match &mut playback.0 {
        RevealShow::Single(run) => {
            let cues = if skip { run.skip() } else { run.advance(dt) };
            for id in cues {
                play_cue(&mut commands, &assets, &content, &id, bus);
            }
            if run.time >= run.plan.card_at && enter {
                next.set(AppState::Menu);
                return;
            }
            for entity in &old {
                commands.entity(entity).despawn();
            }
            paint_single(&mut commands, &content, run);
        }
        RevealShow::Grid(run) => {
            let cues = if skip { run.skip() } else { run.advance(dt) };
            for id in cues {
                play_cue(&mut commands, &assets, &content, &id, bus);
            }
            if run.finished() && enter {
                next.set(AppState::Menu);
                return;
            }
            for entity in &old {
                commands.entity(entity).despawn();
            }
            paint_grid(&mut commands, &content, run);
        }
        RevealShow::Refund(run) => {
            let cues = if skip { run.skip() } else { run.advance(dt) };
            for id in cues {
                play_cue(&mut commands, &assets, &content, &id, bus);
            }
            if enter {
                next.set(AppState::Menu);
                return;
            }
            for entity in &old {
                commands.entity(entity).despawn();
            }
            paint_refund(&mut commands, &content, run);
        }
    }
}

fn paint_single(commands: &mut Commands, content: &Content, run: &RevealRun) {
    let view = look(content, &run.plan, run.time);
    let layout = &content.crate_reveal.layout;
    let rgb = Color::srgb(view.rgb[0], view.rgb[1], view.rgb[2]);
    let luma = view.rgb[0] * 0.3 + view.rgb[1] * 0.59 + view.rgb[2] * 0.11;
    let ink_name = if luma > layout.ink_luma { &layout.ink_dark } else { &layout.ink_light };
    let ink_rgb = content.palette.key(ink_name);
    let ink = Color::srgb(ink_rgb[0], ink_rgb[1], ink_rgb[2]);
    let paper = content.palette.key(&layout.ink_light);
    let stage = content.palette.key(&layout.stage);
    let title = view.shown.to_uppercase();
    let hint = if view.card { "ENTER" } else { "SKIP" };
    let hint_style = &content.crate_reveal.hint;
    let title_px = layout.title_px;
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(layout.row_gap),
            ..default()
        },
        BackgroundColor(Color::srgb(stage[0], stage[1], stage[2])),
        RevealRoot,
        children![
            (
                Node {
                    width: Val::Px(layout.panel_width),
                    height: Val::Px(layout.panel_height),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(content.crate_reveal.card.title_pad)),
                    ..default()
                },
                BackgroundColor(rgb),
                children![(Text::new(title), TextFont::from_font_size(title_px), TextColor(ink))],
            ),
            (
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(hint_style.margin_x),
                    bottom: Val::Px(hint_style.margin_y),
                    ..default()
                },
                children![(
                    Text::new(hint),
                    TextFont::from_font_size(hint_style.text_px),
                    TextColor(Color::srgba(paper[0], paper[1], paper[2], hint_style.opacity)),
                )],
            ),
        ],
    ));
    if view.dim > 0.0 {
        commands.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, view.dim)),
            RevealRoot,
        ));
    }
}

fn paint_grid(commands: &mut Commands, content: &Content, run: &GridRun) {
    let layout = &content.crate_reveal.layout;
    let grid = &content.crate_reveal.grid;
    let stage = content.palette.key(&layout.stage);
    let back = content.palette.key(&grid.back);
    let frame = content.palette.key(&grid.frame);
    let fit_w = content.ui.layout.fit_width;
    let fit_h = content.ui.layout.fit_height;
    let grid_w = grid.columns as f32 * grid.card_w + (grid.columns - 1) as f32 * grid.gap;
    let grid_h = grid.rows as f32 * grid.card_h + (grid.rows - 1) as f32 * grid.gap;
    let origin_x = (fit_w - grid_w) * 0.5;
    let origin_y = (fit_h - grid_h) * 0.5;
    let border = (layout.border_px as f32 * grid.card_h / layout.panel_height).round().max(1.0);
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..default()
        },
        BackgroundColor(Color::srgb(stage[0], stage[1], stage[2])),
        RevealRoot,
    ));
    for (index, card) in run.plan.cards.iter().enumerate() {
        let col = index as u32 % grid.columns;
        let row = index as u32 / grid.columns;
        let x = origin_x + col as f32 * (grid.card_w + grid.gap);
        let y = origin_y + row as f32 * (grid.card_h + grid.gap);
        let shown = run.shown_face(index);
        let (fill, edge, label) = match shown {
            Some(rarity) => {
                let color = content.palette.key(&format!("rarity_{rarity}"));
                let edge = mix(color, frame, layout.border_mix);
                (Color::srgb(color[0], color[1], color[2]), edge, Some(rarity.to_uppercase()))
            }
            None => (Color::srgb(back[0], back[1], back[2]), Color::srgb(frame[0], frame[1], frame[2]), None),
        };
        let mut card_node = commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(x / fit_w * 100.0),
                top: Val::Percent(y / fit_h * 100.0),
                width: Val::Percent(grid.card_w / fit_w * 100.0),
                height: Val::Percent(grid.card_h / fit_h * 100.0),
                border: UiRect::all(Val::Px(border)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(fill),
            BorderColor::all(edge),
            RevealRoot,
        ));
        if let Some(label) = label {
            let rgb = content.palette.key(&format!("rarity_{}", card.rarity));
            let luma = rgb[0] * 0.3 + rgb[1] * 0.59 + rgb[2] * 0.11;
            let ink_name = if luma > layout.ink_luma { &layout.ink_dark } else { &layout.ink_light };
            let ink = content.palette.key(ink_name);
            card_node.with_child((
                Text::new(label),
                TextFont::from_font_size(layout.title_px * grid.card_h / layout.panel_height),
                TextColor(Color::srgb(ink[0], ink[1], ink[2])),
            ));
        }
    }
    let hint = if run.finished() { "ENTER" } else { "SKIP" };
    paint_hint(commands, content, hint);
}

fn paint_refund(commands: &mut Commands, content: &Content, run: &RefundRun) {
    let layout = &content.crate_reveal.layout;
    let grid = &content.crate_reveal.grid;
    let stage = content.palette.key(&layout.stage);
    let face = content.palette.key(&grid.refund_face);
    let ink = content.palette.key(&grid.refund_ink);
    let fit_w = content.ui.layout.fit_width;
    let fit_h = content.ui.layout.fit_height;
    let border = (layout.border_px as f32 * grid.refund_h / layout.panel_height).round().max(1.0);
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..default()
        },
        BackgroundColor(Color::srgb(stage[0], stage[1], stage[2])),
        RevealRoot,
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent((fit_w - grid.refund_w) / 2.0 / fit_w * 100.0),
            top: Val::Percent((fit_h - grid.refund_h) / 2.0 / fit_h * 100.0),
            width: Val::Percent(grid.refund_w / fit_w * 100.0),
            height: Val::Percent(grid.refund_h / fit_h * 100.0),
            border: UiRect::all(Val::Px(border)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgb(face[0], face[1], face[2])),
        BorderColor::all(mix(face, ink, layout.border_mix)),
        RevealRoot,
        children![(
            Text::new(run.message.clone()),
            TextFont::from_font_size(layout.title_px * grid.refund_h / layout.panel_height),
            TextColor(Color::srgb(ink[0], ink[1], ink[2])),
        )],
    ));
    paint_hint(commands, content, "ENTER");
}

fn paint_hint(commands: &mut Commands, content: &Content, hint: &str) {
    let paper = content.palette.key(&content.crate_reveal.layout.ink_light);
    let hint_style = &content.crate_reveal.hint;
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(hint_style.margin_x),
            bottom: Val::Px(hint_style.margin_y),
            ..default()
        },
        RevealRoot,
        children![(
            Text::new(hint),
            TextFont::from_font_size(hint_style.text_px),
            TextColor(Color::srgba(paper[0], paper[1], paper[2], hint_style.opacity)),
        )],
    ));
}

fn mix(face: [f32; 4], edge: [f32; 4], t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::srgb(
        face[0] * (1.0 - t) + edge[0] * t,
        face[1] * (1.0 - t) + edge[1] * t,
        face[2] * (1.0 - t) + edge[2] * t,
    )
}

pub fn close(mut commands: Commands, roots: Query<Entity, Or<(With<RevealRoot>, With<RevealVoice>)>>) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<RevealPlayback>();
    commands.remove_resource::<RevealFrom>();
}

pub fn from_env(spec: &str) -> Option<RevealRun> {
    let content = Content::embedded();
    let (rarity, fakeout) = spec.split_once('+').map(|(rarity, tease)| (rarity, Some(tease))).unwrap_or((spec, None));
    RevealRun::start(&content, rarity.trim(), fakeout).ok()
}

fn play_cue(commands: &mut Commands, assets: &AssetServer, content: &Content, id: &str, bus: f32) {
    let Some(path) = content.cue_path(id) else { return };
    let Some(volume) = content.cue_volume(id) else { return };
    commands.spawn((
        AudioPlayer::new(assets.load::<AudioSource>(path.to_string())),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume * bus)),
        RevealVoice { id: id.to_string() },
    ));
}
