//! The title screen.
//!
//! The farm is already there behind it — real sand, real glass, settling in real time —
//! just with no colony in it yet. Nothing here is a backdrop image.
//!
//! The logo does double duty, and that's the point. It's a hand-lettered label with a
//! peeling corner, so on the title screen it reads as a sticker somebody slapped on the
//! glass, which is precisely the object the design asked for: a sign inside the tank
//! asking you not to shake it. The game's only voice is a piece of tape.

use bevy::math::Rot2;
use bevy::prelude::*;
use bevy::ui::UiTransform;

#[derive(States, Default, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum GameState {
    #[default]
    Title,
    Playing,
}

/// Everything spawned for the title screen, so leaving it is one despawn.
#[derive(Component)]
pub struct TitleUi;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Begin,
    Load,
    Settings,
}

const INK: Color = Color::srgb(0.86, 0.84, 0.79);
const INK_HOVER: Color = Color::srgb(1.0, 0.97, 0.90);
const INK_DIM: Color = Color::srgb(0.38, 0.36, 0.34);

/// The sticker's own aspect ratio, so it scales without distorting.
const LOGO_ASPECT: f32 = 1536.0 / 1024.0;

pub fn enter_title(mut commands: Commands, assets: Res<AssetServer>) {
    // No `UiTargetCamera` here, and no `Single` camera lookup either. `Single` silently
    // *skips* its system when the query doesn't match exactly one entity, and `OnEnter`
    // fires only once — so a camera that isn't ready yet means the menu never gets built
    // at all, with no error. Letting Bevy attach the UI to the window camera works.
    commands.spawn((
        TitleUi,
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(8),
            ..default()
        },
        // The scene behind is the whole point, so nothing dims it.
        BackgroundColor(Color::NONE),
        children![
            (
                ImageNode::new(assets.load("title-logo.png")),
                Node {
                    width: percent(52),
                    aspect_ratio: Some(LOGO_ASPECT),
                    margin: UiRect::bottom(px(4)),
                    ..default()
                },
                // Stuck on by hand, so very slightly crooked.
                UiTransform {
                    rotation: Rot2::degrees(-1.8),
                    ..default()
                },
            ),
            menu_item(MenuAction::Begin, "Begin", true),
            menu_item(MenuAction::Load, "Load", false),
            menu_item(MenuAction::Settings, "Settings", false),
        ],
    ));
}

/// Deliberately plain: no panels, no borders, no fills. The tank is the thing you're
/// meant to be looking at, and a 99c ambient game shouldn't open with chrome.
fn menu_item(action: MenuAction, label: &str, enabled: bool) -> impl Bundle {
    (
        Button,
        action,
        Node {
            padding: UiRect::axes(px(18), px(7)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::NONE),
        children![(
            Text::new(label),
            TextFont { font_size: FontSize::Px(21.0), ..default() },
            TextColor(if enabled { INK } else { INK_DIM }),
        )],
    )
}

pub fn exit_title(mut commands: Commands, ui: Query<Entity, With<TitleUi>>) {
    for entity in &ui {
        commands.entity(entity).despawn();
    }
}

pub fn title_menu(
    mut interactions: Query<(&Interaction, &MenuAction, &Children), Changed<Interaction>>,
    mut colours: Query<&mut TextColor>,
    mut next: ResMut<NextState<GameState>>,
) {
    for (interaction, action, children) in &mut interactions {
        let enabled = *action == MenuAction::Begin;

        if let Some(&child) = children.first()
            && let Ok(mut colour) = colours.get_mut(child)
        {
            colour.0 = match (*interaction, enabled) {
                (_, false) => INK_DIM,
                (Interaction::Hovered | Interaction::Pressed, true) => INK_HOVER,
                (Interaction::None, true) => INK,
            };
        }

        if *interaction == Interaction::Pressed && *action == MenuAction::Begin {
            next.set(GameState::Playing);
        }
    }
}
