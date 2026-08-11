//! Please Don't Shake — a 3D ant farm that asks you not to shake it.
//!
//! Wiring only; the substance is in the modules. Roughly in dependency order:
//!
//! - [`grid`] — the sand, and the one authority on where it is
//! - [`sand`] — the falling-sand automaton, driven by a single agitation variable
//! - [`pheromones`] — the fields the colony coordinates through
//! - [`ants`] — the colony, and where nest architecture comes from
//! - [`meshing`], [`tank`], [`grains`] — turning all of that into something to look at
//! - [`interact`] — the verbs, kept deliberately thin
//! - [`title`], [`devcapture`] — the menu, and the scripted verification runs
//!
//! Controls
//!   click            tap the glass
//!   click and drag   shake the tank
//!   right-drag       dig by hand (debug — the ants' job)
//!   shift+right-drag fill sand back in
//!   F12              screenshot

mod ants;
mod audio;
mod devcapture;
mod farm;
mod grains;
mod grid;
mod interact;
mod meshing;
mod pheromones;
mod pause;
mod radial;
mod sand;
mod tank;
mod title;

use bevy::prelude::*;

use audio::setup_music;

use title::GameState;

use ants::{ColonyClock, setup_ant_assets, sync_ant_transforms, update_ants};
use grains::{setup_grain_assets, spawn_queued_grains, update_grains};
use grid::{SandGrid, SandPalette, fill_strata};
use interact::{PointerState, pointer_input};
use meshing::remesh_dirty_chunks;
use pheromones::{NavField, Pheromones, diffuse_pheromones, rebuild_nav_field};
use sand::{GrainSpawnQueue, TankMotion, step_sand};
use tank::{TankSpring, setup_tank, tank_spring};

/// The fields update at 15 Hz rather than the sand's 60 — chemistry and pathfinding
/// don't need per-tick resolution, and this keeps their cost off the hot loop.
fn every_fourth_tick(grid: Res<SandGrid>) -> bool {
    grid.tick % 4 == 0
}

/// The sand runs on a fixed step so the farm stays deterministic across saves.
const SIM_HZ: f64 = 60.0;

/// Where to find `assets/`.
///
/// Bevy resolves its default asset path relative to the executable, so running
/// `./target/release/please_dont_shake` — which is exactly what the .command script does
/// — looks in `target/release/assets/` and finds nothing. Checking next to the executable
/// first and falling back to the crate root covers both the shipping layout and running
/// from a build directory.
fn asset_root() -> String {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let beside_exe = dir.join("assets");
        if beside_exe.is_dir() {
            return beside_exe.to_string_lossy().into_owned();
        }
    }

    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    if crate_root.is_dir() {
        return crate_root.to_string_lossy().into_owned();
    }

    "assets".to_string()
}

fn main() {
    let mut grid = SandGrid::new();
    fill_strata(&mut grid, grid::INITIAL_SURFACE);

    let mut app = App::new();
    app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Please Don't Shake".into(),
                        resolution: (1280u32, 800u32).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_root(),
                    ..default()
                }),
        )
        .insert_resource(ClearColor(Color::srgb(0.045, 0.043, 0.050)))
        .insert_resource(grid)
        .insert_resource(SandPalette::build())
        .insert_resource(Time::<Fixed>::from_hz(SIM_HZ))
        .init_resource::<GrainSpawnQueue>()
        .init_resource::<TankMotion>()
        .init_resource::<TankSpring>()
        .init_resource::<PointerState>()
        .init_resource::<ColonyClock>()
        .init_resource::<ants::ColonyStats>()
        .insert_resource(Pheromones::new())
        .insert_resource(NavField::new())
        .init_resource::<radial::RadialMenu>()
        .init_resource::<radial::Stock>()
        .init_resource::<radial::PlacementQueue>()
        .init_resource::<pause::PauseMenu>()
        .init_resource::<ants::KitPour>()
        .add_systems(
            Startup,
            (setup_grain_assets, setup_ant_assets, setup_tank, setup_music).chain(),
        )
        .add_plugins(ordo::OrdoPlugin::with_theme("theme.ordo.toml"))
        .add_systems(
            OnEnter(GameState::Title),
            (title::enter_title, title::dress_menu).chain(),
        )
        .add_systems(OnExit(GameState::Title), title::exit_title)
        // Leaving play tears the Esc menu down, and throws the farm away: the title
        // screen shows an empty tank, and one full of somebody's tunnels is not empty.
        .add_systems(
            OnExit(GameState::Playing),
            (pause::close_on_leave, farm::reset_farm),
        )
        .add_systems(
            FixedUpdate,
            (
                // Fields first, so the ants act on a current view of the nest; then the
                // ants, who dig; then the sand, which reacts to what they dug.
                (rebuild_nav_field, diffuse_pheromones).run_if(every_fourth_tick),
                update_ants,
                step_sand,
                spawn_queued_grains,
                update_grains,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                // No shaking the menu — the tank still settles behind it, but the verbs
                // don't exist until the colony does.
                pointer_input.run_if(in_state(GameState::Playing)),
                tank_spring,
                ants::place_queued,
                ants::pour_kit,
                radial::sync_radial_ui,
                sync_ant_transforms,
                remesh_dirty_chunks,
            )
                .chain(),
        )
        .add_observer(title::on_menu_activate)
        .add_observer(pause::on_pause_activate)
        // The farm keeps running behind the Esc menu on purpose — an ambient game whose
        // colony froze whenever you opened a menu would be lying about what it is.
        .add_systems(
            Update,
            (pause::toggle_pause, pause::sync_pause_ui, pause::dress_pause_menu)
                .chain()
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(Update, devcapture::screenshot_hotkey);

    // State registration has to come *after* DefaultPlugins, which is what brings
    // StatesPlugin with it. Verification runs skip the menu and open straight into a
    // live colony; the title-screen shot obviously has to stay on the menu.
    if devcapture::capture_mode() && !devcapture::title_shot() {
        app.insert_state(GameState::Playing);
    } else {
        app.init_state::<GameState>();
    }

    // Scripted verification runs. Dev only.
    if devcapture::capture_mode() {
        app.insert_resource(devcapture::DevCapture::new())
            .add_systems(Startup, devcapture::setup_offscreen_target.after(setup_tank));

        if devcapture::title_shot() {
            app.add_systems(Update, devcapture::run_title_shot);
        } else if devcapture::sand_only() {
            // The original M1 cohesion test, with no colony to disturb the numbers.
            app.add_systems(Update, devcapture::run_capture.before(tank_spring));
        } else {
            app.add_systems(Update, devcapture::run_colony_capture.before(tank_spring));
        }
    }

    app.run();
}
