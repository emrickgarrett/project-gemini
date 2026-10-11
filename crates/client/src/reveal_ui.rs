//! Full-screen playback of a server crate result. Timings, sounds, and colors come from the tables.

use bevy::audio::Volume;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use gemini_sim::content::Content;
use gemini_sim::reveal::{look, RevealRun};

use crate::AppState;

#[derive(Resource)]
pub struct RevealPlayback(pub RevealRun);

#[derive(Component)]
pub(crate) struct RevealRoot;

#[derive(Component)]
pub(crate) struct RevealVoice;

#[derive(Component)]
pub(crate) struct RevealButton(pub(crate) &'static str);

pub fn tick(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    assets: Res<AssetServer>,
    mut playback: ResMut<RevealPlayback>,
    mut next: ResMut<NextState<AppState>>,
    old: Query<Entity, With<RevealRoot>>,
) {
    let content = Content::embedded();
    let run = &mut playback.0;
    let skip = keys.just_pressed(KeyCode::Space) || mouse.just_pressed(MouseButton::Left);
    let cues = if skip { run.skip() } else { run.advance(time.delta_secs()) };
    for id in cues {
        play_cue(&mut commands, &assets, &content, &id);
    }
    if run.time >= run.plan.card_at && keys.just_pressed(KeyCode::Enter) {
        next.set(AppState::Menu);
        return;
    }
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let view = look(&content, &run.plan, run.time);
    let rgb = Color::srgb(view.rgb[0], view.rgb[1], view.rgb[2]);
    let ink = if view.rgb[0] * 0.3 + view.rgb[1] * 0.59 + view.rgb[2] * 0.11 > 0.62 {
        Color::srgb(0.12, 0.1, 0.14)
    } else {
        Color::srgb(0.95, 0.95, 0.95)
    };
    let title = view.shown.to_uppercase();
    let hint = if view.card { "ENTER" } else { "SKIP" };
    let hint_style = &content.crate_reveal.hint;
    let title_px = title_font_px(&title, view.shown.eq_ignore_ascii_case("legendary"), &content.crate_reveal.card);
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(16.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.07, 0.055, 0.09)),
        RevealRoot,
        children![
            (
                Node {
                    width: Val::Px(420.0),
                    height: Val::Px(240.0),
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
                    TextColor(Color::srgba(0.95, 0.95, 0.95, hint_style.opacity)),
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
}

pub fn start_button(
    activate: On<Activate>,
    buttons: Query<&RevealButton>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
) {
    let Ok(button) = buttons.get(activate.entity) else { return };
    let content = Content::embedded();
    let run = RevealRun::start(&content, button.0, None).expect("reveal rarity");
    commands.insert_resource(RevealPlayback(run));
    next.set(AppState::Reveal);
}

pub fn from_env(spec: &str) -> Option<RevealRun> {
    let content = Content::embedded();
    let (rarity, fakeout) = spec.split_once('+').map(|(rarity, tease)| (rarity, Some(tease))).unwrap_or((spec, None));
    RevealRun::start(&content, rarity.trim(), fakeout).ok()
}

fn title_font_px(label: &str, legendary: bool, card: &gemini_sim::content::CardTitle) -> f32 {
    const PANEL_W: f32 = 420.0;
    const PANEL_H: f32 = 240.0;
    let pad = card.title_pad;
    let avail_w = (PANEL_W - pad * 2.0).max(1.0);
    let avail_h = (PANEL_H - pad * 2.0).max(1.0);
    let cols = label.chars().count().max(1) as f32;
    let fit = (avail_w / (cols * 0.6)).min(avail_h * 0.45);
    let cap = if legendary {
        card.legendary_title_max_scale
    } else {
        card.title_max_scale
    };
    fit.min(cap * 7.0).max(8.0)
}

fn play_cue(commands: &mut Commands, assets: &AssetServer, content: &Content, id: &str) {
    let Some(path) = content.cue_path(id) else { return };
    let Some(volume) = content.cue_volume(id) else { return };
    commands.spawn((
        AudioPlayer::new(assets.load::<AudioSource>(path.to_string())),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
        RevealVoice,
    ));
}
