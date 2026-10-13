//! Full-screen playback of a server crate result. Timings, sounds, and colors come from the tables.

use bevy::audio::Volume;
use bevy::prelude::*;

use gemini_sim::content::Content;
use gemini_sim::reveal::{look, RevealRun};

use crate::AppState;

#[derive(Resource)]
pub struct RevealPlayback(pub RevealRun);

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
pub(crate) struct RevealVoice;

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
    let run = &mut playback.0;
    let skip = keys.just_pressed(KeyCode::Space) || mouse.just_pressed(MouseButton::Left);
    if skip {
        for entity in &voices {
            commands.entity(entity).despawn();
        }
    }
    let cues = if skip { run.skip() } else { run.advance(time.delta_secs()) };
    let bus = prefs.as_ref().map(|prefs| crate::audio::bus_gain(prefs, false)).unwrap_or(1.0);
    for id in cues {
        play_cue(&mut commands, &assets, &content, &id, bus);
    }
    if run.time >= run.plan.card_at && keys.just_pressed(KeyCode::Enter) {
        next.set(AppState::Menu);
        return;
    }
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let view = look(&content, &run.plan, run.time);
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
        RevealVoice,
    ));
}
