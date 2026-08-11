//! Farm lifecycle — starting one over.
//!
//! Leaving for the title screen throws the farm away and pours a fresh one. That is the
//! honest reading of going back to the menu: the title screen shows an empty tank, and a
//! tank still full of somebody's tunnels is not empty.
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
