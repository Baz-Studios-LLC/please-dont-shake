//! The title screen.
//!
//! The farm is already there behind it — real sand, real glass, settling in real
//! time — just with no colony in it yet. Nothing here is a backdrop image.
//!
//! The logo does double duty, and that's the point. It's a hand-lettered label
//! with a peeling corner, so on the title screen it reads as a sticker somebody
//! slapped on the glass, which is precisely the object the design asked for: a
//! sign inside the tank asking you not to shake it. The game's only voice is a
//! piece of tape.
//!
//! Chrome comes from Ordo, so the buttons and their colours follow the theme file
//! rather than being spelled out here. Two consequences worth knowing: the palette
//! is editable with the game running, and this module states no colours at all.

use bevy::math::Rot2;
use bevy::prelude::*;
use bevy::ui::UiTransform;
use ordo::prelude::*;

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

impl MenuAction {
    /// Nothing sits behind Load or Settings yet — there's no save format until
    /// M3 and no settings to speak of. They're shown dimmed rather than hidden,
    /// because a menu that changes shape as features land is worse than one with
    /// a couple of quiet entries.
    fn enabled(self) -> bool {
        matches!(self, MenuAction::Begin)
    }
}

/// One width for every menu entry, wide enough for the longest label at the
/// theme's body size. A shared width is what makes the column read as a stack.
const MENU_WIDTH: f32 = 210.0;

/// The sticker's own aspect ratio, so it scales without distorting.
const LOGO_ASPECT: f32 = 1536.0 / 1024.0;

pub fn enter_title(mut commands: Commands, assets: Res<AssetServer>) {
    // No `UiTargetCamera` and no `Single` camera lookup. `Single` silently *skips*
    // its system when the query doesn't match exactly one entity, and `OnEnter`
    // fires only once — so a camera that isn't ready yet means the menu never gets
    // built at all, with no error. Letting Bevy attach the UI to the window camera
    // works, and this cost an evening once already.
    commands.spawn((
        TitleUi,
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(6),
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
                    margin: UiRect::bottom(px(6)),
                    ..default()
                },
                // Stuck on by hand, so very slightly crooked.
                UiTransform {
                    rotation: Rot2::degrees(-1.8),
                    ..default()
                },
            ),
            (button("Begin"), MenuAction::Begin),
            (button("Load"), MenuAction::Load),
            (button("Settings"), MenuAction::Settings),
        ],
    ));
}

pub fn exit_title(mut commands: Commands, ui: Query<Entity, With<TitleUi>>) {
    for entity in &ui {
        commands.entity(entity).despawn();
    }
}

/// Hover and press colours are Ordo's job. This only has to decide what a click
/// *means*, and refuse the two entries that don't mean anything yet.
pub fn title_menu(
    actions: Query<(&Interaction, &MenuAction), Changed<Interaction>>,
    mut next: ResMut<NextState<GameState>>,
) {
    for (interaction, action) in &actions {
        if *interaction == Interaction::Pressed && action.enabled() {
            next.set(GameState::Playing);
        }
    }
}

/// One width for every entry, and grey for the ones with nothing behind them.
///
/// Runs once on entering the title screen rather than every frame. Ordo paints a
/// button's own chrome from the theme; what's left is the game's statement about
/// its own state — that two of these choices don't exist yet — plus a shared
/// width, so the column reads as a stack rather than as three ragged labels.
pub fn dress_menu(
    theme: Res<Theme>,
    mut actions: Query<(&MenuAction, &Children, &mut Node)>,
    mut colours: Query<&mut TextColor>,
) {
    for (action, children, mut node) in &mut actions {
        node.width = px(MENU_WIDTH);

        if action.enabled() {
            continue;
        }
        for &child in children {
            if let Ok(mut colour) = colours.get_mut(child) {
                colour.0 = theme.color(Role::InkDim);
            }
        }
    }
}
