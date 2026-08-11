//! The Esc menu.
//!
//! Deliberately not a *pause*. The farm keeps running behind it — sand keeps settling,
//! ants keep digging — because this is an ambient game and a colony that froze whenever
//! you opened a menu would be lying about what it is. The menu is something you put in
//! front of the tank, not a switch on the world.
//!
//! Chrome is Ordo's, so this states no colours and follows the theme file.

use crate::title::GameState;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use ordo::prelude::*;

/// Marks everything belonging to the Esc menu, so closing it is one despawn.
#[derive(Component)]
pub struct PauseUi;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum PauseAction {
    Resume,
    Settings,
    TitleScreen,
    Exit,
}

impl PauseAction {
    /// Settings has nothing behind it yet, same as on the title screen. Shown dimmed
    /// rather than hidden: a menu whose shape changes as features land is worse than one
    /// with a quiet entry in it.
    fn enabled(self) -> bool {
        !matches!(self, PauseAction::Settings)
    }
}

/// Whether the menu is currently up. A resource rather than a state, because the world
/// deliberately keeps simulating underneath.
#[derive(Resource, Default)]
pub struct PauseMenu {
    pub open: bool,
}

const MENU_WIDTH: f32 = 230.0;

/// Esc opens it, and closes it again.
pub fn toggle_pause(keys: Res<ButtonInput<KeyCode>>, mut menu: ResMut<PauseMenu>) {
    if keys.just_pressed(KeyCode::Escape) {
        menu.open = !menu.open;
    }
}

/// Build and tear down the menu to follow that flag.
pub fn sync_pause_ui(
    mut commands: Commands,
    menu: Res<PauseMenu>,
    theme: Res<Theme>,
    existing: Query<Entity, With<PauseUi>>,
) {
    let shown = existing.iter().next();

    match (menu.open, shown) {
        (false, Some(root)) => {
            commands.entity(root).despawn();
        }
        (true, None) => {
            commands.spawn((
                PauseUi,
                Node {
                    width: percent(100),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: px(6),
                    ..default()
                },
                // A scrim, so the menu reads as being in front of the farm rather than
                // painted onto it — and translucent, because hiding the tank entirely
                // would defeat the point of the tank.
                BackgroundColor(theme.color(Role::Scrim)),
                children![
                    (button("Resume"), PauseAction::Resume),
                    (button("Settings"), PauseAction::Settings),
                    (button("Title Screen"), PauseAction::TitleScreen),
                    (button("Exit"), PauseAction::Exit),
                ],
            ));
        }
        _ => {}
    }
}

/// One width for every entry, and grey for the ones with nothing behind them.
pub fn dress_pause_menu(
    theme: Res<Theme>,
    mut actions: Query<(&PauseAction, &Children, &mut Node), Added<PauseAction>>,
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

/// Listens for `Activate`, not `Interaction` — Ordo's buttons are
/// `bevy_ui_widgets::Button` and carry no `Interaction` at all, which once made a whole
/// menu look wired up while doing nothing.
pub fn on_pause_activate(
    activate: On<Activate>,
    actions: Query<&PauseAction>,
    mut menu: ResMut<PauseMenu>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(action) = actions.get(activate.entity) else {
        return;
    };
    if !action.enabled() {
        return;
    }

    match action {
        PauseAction::Resume => menu.open = false,
        PauseAction::TitleScreen => {
            menu.open = false;
            next.set(GameState::Title);
        }
        PauseAction::Exit => {
            exit.write(AppExit::Success);
        }
        PauseAction::Settings => {}
    }
}

/// Leaving play takes the menu with it, so returning to the title screen doesn't leave
/// a stray scrim and four buttons floating over it.
pub fn close_on_leave(
    mut commands: Commands,
    mut menu: ResMut<PauseMenu>,
    existing: Query<Entity, With<PauseUi>>,
) {
    menu.open = false;
    for entity in &existing {
        commands.entity(entity).despawn();
    }
}
