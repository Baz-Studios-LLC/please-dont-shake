//! The title screen.
//!
//! The farm is already there behind it — real sand, real glass, settling in real
//! time. On the first run there's no colony in it yet. On every run after that
//! there is, and it carries on digging while you look at the menu: the title
//! screen isn't a place the game stops, it's a place you can see it from.
//! Nothing here is a backdrop image.
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
use bevy::ui_widgets::Activate;
use ordo::prelude::*;

use crate::farm::GameInProgress;

#[derive(States, Default, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum GameState {
    /// The studio's mark over black. See [`crate::splash`].
    #[default]
    Splash,
    Title,
    Playing,
}

/// Everything spawned for the title screen, so leaving it is one despawn.
#[derive(Component)]
pub struct TitleUi;

/// The sticker. Fades by hand, because it's an image and no theme role owns it.
#[derive(Component)]
pub struct TitleLogo;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuAction {
    /// Walk back into the farm that's already running behind this menu.
    Continue,
    NewGame,
    Settings,
}

impl MenuAction {
    /// Nothing sits behind Settings yet — there's nothing to set. It's shown
    /// dimmed rather than hidden, because a menu that changes shape as features
    /// land is worse than one with a quiet entry in it.
    ///
    /// Continue is a different case and is *absent* rather than dimmed when there
    /// is nothing to continue. It isn't an unfinished feature, it's a statement
    /// about the farm: on a first run there is no game to go back to, and a
    /// greyed-out Continue would be claiming otherwise.
    fn enabled(self) -> bool {
        !matches!(self, MenuAction::Settings)
    }
}

/// One width for every menu entry, wide enough for the longest label at the
/// theme's body size. A shared width is what makes the column read as a stack.
const MENU_WIDTH: f32 = 210.0;

/// The sticker's own aspect ratio, so it scales without distorting.
const LOGO_ASPECT: f32 = 1536.0 / 1024.0;

/// Seconds the title takes to get out of the way once a choice is made.
///
/// Long enough to read as the menu stepping aside rather than being cut, short
/// enough that nobody who has pressed Continue is kept waiting for the farm.
const TITLE_FADE: f32 = 0.5;

pub fn enter_title(
    mut commands: Commands,
    assets: Res<AssetServer>,
    progress: Res<GameInProgress>,
) {
    // No `UiTargetCamera` and no `Single` camera lookup. `Single` silently *skips*
    // its system when the query doesn't match exactly one entity, and `OnEnter`
    // fires only once — so a camera that isn't ready yet means the menu never gets
    // built at all, with no error. Letting Bevy attach the UI to the window camera
    // works, and this cost an evening once already.
    let root = commands
        .spawn((
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
        ))
        .id();

    commands.spawn((
        TitleLogo,
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
        ChildOf(root),
    ));

    // Built one at a time rather than with `children!`, because the first entry
    // is conditional and a fixed tuple can't be.
    if progress.0 {
        commands.spawn((button("Continue"), MenuAction::Continue, ChildOf(root)));
    }
    commands.spawn((button("New Game"), MenuAction::NewGame, ChildOf(root)));
    commands.spawn((button("Settings"), MenuAction::Settings, ChildOf(root)));
}

pub fn exit_title(mut commands: Commands, ui: Query<Entity, With<TitleUi>>) {
    for entity in &ui {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<TitleFade>();
}

/// The title on its way out. Present only while it's actually fading.
#[derive(Resource, Default)]
pub struct TitleFade {
    elapsed: f32,
}

/// Hover and press colours are Ordo's job. This only has to decide what a click
/// *means*, and refuse the entry that doesn't mean anything yet.
///
/// Listens for `Activate` rather than polling `Interaction`. Ordo's buttons are
/// `bevy_ui_widgets::Button`, which carries no `Interaction` component at all — so
/// a query for one matches nothing and the button appears to work (it lights up on
/// hover, because that's painted from `Hovered`) while doing nothing at all when
/// clicked. `Activate` is also the better signal: it fires for Enter and Space
/// while focused, not just for a pointer.
///
/// Neither choice changes state here. Both start the fade and let it decide, so
/// there is one path out of the title screen instead of two.
pub fn on_menu_activate(
    activate: On<Activate>,
    actions: Query<&MenuAction>,
    fade: Option<Res<TitleFade>>,
    mut commands: Commands,
) {
    // A second click during the fade would restart it, and on New Game would pour
    // a second fresh farm over the first.
    if fade.is_some() {
        return;
    }
    let Ok(action) = actions.get(activate.entity) else {
        return;
    };
    if !action.enabled() {
        return;
    }

    // New Game throws the old farm away *now*, so what the fade uncovers is a
    // fresh tank rather than the previous colony blinking out a moment later.
    // The file goes with it: without that, starting over and then closing the app
    // would reopen onto the farm that was just abandoned.
    if *action == MenuAction::NewGame {
        commands.run_system_cached(crate::farm::reset_farm);
        crate::save::forget_farm();
    }
    commands.init_resource::<TitleFade>();
}

/// Takes the menu away and hands over to the farm.
///
/// Opacity rather than colours: Ordo repaints from the theme, and two writers over
/// one `BackgroundColor` is how a fade ends up flickering back to full strength.
/// `Opacity` is the kit's own way in, and the buttons honour it — the sticker is
/// the game's own image with no theme role, so that one is faded by hand.
///
/// Runs on `Time<Real>`, like the splash: this is chrome getting out of the way,
/// and it shouldn't care what the simulation clock is doing.
pub fn fade_title(
    time: Res<Time<Real>>,
    fade: Option<ResMut<TitleFade>>,
    mut next: ResMut<NextState<GameState>>,
    mut commands: Commands,
    roots: Query<Entity, With<TitleUi>>,
    children: Query<&Children>,
    mut opacities: Query<&mut Opacity>,
    mut logos: Query<&mut ImageNode, With<TitleLogo>>,
) {
    let Some(mut fade) = fade else {
        return;
    };
    fade.elapsed += time.delta_secs();
    let alpha = (1.0 - fade.elapsed / TITLE_FADE).clamp(0.0, 1.0);

    for root in &roots {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            match opacities.get_mut(node) {
                Ok(mut opacity) => opacity.0 = alpha,
                // Ordo only mutates an `Opacity` that is already there, and the
                // widgets don't spawn with one. Nothing in the subtree is exempt,
                // so anything without one gets one.
                Err(_) => {
                    commands.entity(node).insert(Opacity(alpha));
                }
            }
            if let Ok(kids) = children.get(node) {
                stack.extend(kids.iter());
            }
        }
    }
    for mut logo in &mut logos {
        logo.color = logo.color.with_alpha(alpha);
    }

    if fade.elapsed >= TITLE_FADE {
        next.set(GameState::Playing);
    }
}

/// One width for every entry, and grey for the one with nothing behind it.
///
/// Runs once on entering the title screen rather than every frame. Ordo paints a
/// button's own chrome from the theme; what's left is the game's statement about
/// its own state — that Settings doesn't exist yet — plus a shared width, so the
/// column reads as a stack rather than as ragged labels.
///
/// The dimming changes the label's *role* rather than its `TextColor`, and this is
/// not a style preference. Writing the colour looks like it works and doesn't:
/// Ordo's repaint pass paints every `Ink` from the theme, so a colour written here
/// during `OnEnter` is overwritten in the same frame's `Update` and the entry comes
/// out at full strength. That is exactly the two-writers-over-one-colour trap
/// Ordo's own docs warn about, and it had been quietly losing since the menu was
/// first built — measured, not guessed: every label sat at the same 236.
pub fn dress_menu(
    mut commands: Commands,
    mut actions: Query<(&MenuAction, &Children, &mut Node)>,
) {
    for (action, children, mut node) in &mut actions {
        node.width = px(MENU_WIDTH);

        if action.enabled() {
            continue;
        }
        for child in children.iter() {
            commands.entity(child).insert(Ink(Role::InkDim));
        }
    }
}
