//! Farm lifecycle — starting one over, and knowing when not to.
//!
//! A farm is thrown away by **New Game** and by nothing else. Going back to the title
//! screen leaves it running: the colony keeps digging behind the menu, and Continue is
//! there to walk back into it. That is what makes this an ambient game rather than a
//! series of sessions — the farm is a thing you keep, and the only way to lose one is to
//! ask for a new one.
//!
//! Everything the farm consists of has to be reset together, and it is spread across
//! several resources — the sand, the two fields the colony coordinates through, the
//! spawn queues, and what's left in stock. Missing one leaves a farm that looks new and
//! isn't: pheromone from a colony that no longer exists, or a navigation flood still
//! describing tunnels that have been filled in.

use crate::ants::{Ant, KitPour};
use crate::grains::Grain;
use crate::grid::*;
use crate::pheromones::{NavField, Pheromones};
use crate::radial::{PlacementQueue, RadialMenu, Stock};
use crate::sand::GrainSpawnQueue;
use bevy::prelude::*;

/// Whether there is a farm worth going back to.
///
/// False at boot and true from the moment play begins. Nothing sets it back to false yet;
/// when the queen's decline lands in M3 there will be a real end to a farm, and this is
/// where it will be recorded.
///
/// It exists because "is a game in progress" cannot be read off the world. Counting ants
/// says no for the first minute of every farm, since a new one starts with the kit still
/// in stock and the player choosing where to tip it in.
#[derive(Resource, Default)]
pub struct GameInProgress(pub bool);

/// Play has begun, so there is now something for Continue to return to.
pub fn mark_in_progress(mut progress: ResMut<GameInProgress>) {
    progress.0 = true;
}

#[allow(clippy::too_many_arguments)]
pub fn reset_farm(
    mut commands: Commands,
    mut grid: ResMut<SandGrid>,
    mut pheromones: ResMut<Pheromones>,
    mut nav: ResMut<NavField>,
    mut stock: ResMut<Stock>,
    mut placements: ResMut<PlacementQueue>,
    mut grains_queue: ResMut<GrainSpawnQueue>,
    mut menu: ResMut<RadialMenu>,
    mut pour: ResMut<KitPour>,
    ants: Query<Entity, With<Ant>>,
    grains: Query<Entity, With<Grain>>,
) {
    for entity in ants.iter().chain(grains.iter()) {
        commands.entity(entity).despawn();
    }

    // Fresh strata. `fill_strata` bumps the grid's revision, so the navigation flood
    // rebuilds and every chunk remeshes on its own.
    fill_strata(&mut grid, INITIAL_SURFACE);
    grid.agitation.fill(0.0);

    *pheromones = Pheromones::new();
    *nav = NavField::new();
    *stock = Stock::default();
    placements.0.clear();
    grains_queue.0.clear();
    *menu = RadialMenu::default();
    *pour = KitPour::default();
}
