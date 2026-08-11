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
//! - [`splash`], [`title`], [`pause`] — the shell around the farm
//! - [`devcapture`] — the scripted verification runs
//!
//! Controls
//!   click            tap the glass
//!   click and drag   shake the tank
//!   right-drag       dig by hand (debug — the ants' job)
//!   shift+right-drag fill sand back in
//!   F12              screenshot

mod ants;
mod audio;
mod away;
mod brood;
mod devcapture;
mod farm;
mod grains;
mod grid;
mod hand;
mod interact;
mod meshing;
mod pheromones;
mod pause;
mod radial;
mod sand;
mod save;
mod settings;
mod splash;
mod tank;
mod title;

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::common_conditions::on_real_timer;

use audio::setup_music;

use title::GameState;

use ants::{
    ColonyClock, ColonyStep, advance_colony_clock, age_ants, setup_ant_assets,
    sync_ant_transforms, update_ants,
};
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
        .init_resource::<ColonyStep>()
        .init_resource::<away::AwaySpan>()
        .init_resource::<ants::ColonyStats>()
        .insert_resource(Pheromones::new())
        .insert_resource(NavField::new())
        .init_resource::<radial::RadialMenu>()
        .init_resource::<radial::Stock>()
        .init_resource::<radial::PlacementQueue>()
        .init_resource::<pause::PauseMenu>()
        .init_resource::<ants::KitPour>()
        .init_resource::<farm::GameInProgress>()
        .init_resource::<brood::LayClock>()
        .init_resource::<brood::BroodStats>()
        .init_resource::<hand::Touch>()
        .init_resource::<settings::Settings>()
        .init_resource::<settings::SettingsWindow>()
        .add_systems(
            Startup,
            (
                setup_grain_assets,
                setup_ant_assets,
                brood::setup_brood_assets,
                setup_tank,
                // After the tank, whose camera the overlay hangs off.
                hand::setup_hand,
                setup_music,
            )
                .chain(),
        )
        .add_plugins(ordo::OrdoPlugin::with_theme("theme.ordo.toml"))
        // The studio's mark, over black, while the farm builds itself behind it.
        .add_systems(OnEnter(GameState::Splash), splash::enter_splash)
        .add_systems(OnExit(GameState::Splash), splash::exit_splash)
        .add_systems(
            Update,
            splash::play_splash.run_if(in_state(GameState::Splash)),
        )
        .add_systems(
            OnEnter(GameState::Title),
            (title::enter_title, title::dress_menu).chain(),
        )
        // Before Ordo's paint, not after: the fade writes `Opacity` and Ordo is what
        // turns that into colours, so writing it afterwards would show up a frame late.
        .add_systems(
            Update,
            title::fade_title
                .before(ordo::OrdoSet)
                .run_if(in_state(GameState::Title)),
        )
        .add_systems(OnExit(GameState::Title), title::exit_title)
        // The farm is *not* thrown away here. Going back to the title leaves the colony
        // digging behind the menu, which is what Continue returns to; only New Game
        // pours a fresh tank. See `farm`.
        .add_systems(OnExit(GameState::Playing), pause::close_on_leave)
        .add_systems(OnEnter(GameState::Playing), farm::mark_in_progress)
        .add_systems(
            FixedUpdate,
            (
                // How much colony time this tick is worth, before anything spends it.
                advance_colony_clock,
                // Fields first, so the ants act on a current view of the nest; then the
                // ants, who dig; then the sand, which reacts to what they dug.
                (rebuild_nav_field, diffuse_pheromones).run_if(every_fourth_tick),
                update_ants,
                age_ants,
                // The colony's other half. After the ants, so a nurse that has just moved
                // carries its brood to where it now is rather than to where it was; before
                // the sand, so a collapse this tick is what `unbury_brood` answers.
                brood::lay_eggs,
                brood::tend_brood,
                brood::age_brood,
                brood::unbury_brood,
                brood::age_out,
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
                // The hand is the cursor everywhere, so it's told what it's doing in every
                // state; only the verbs are play-only.
                interact::track_touch,
                pointer_input
                    .run_if(in_state(GameState::Playing))
                    .run_if(interact::the_farm_is_reachable),
                hand::move_hand,
                hand::restyle_hand,
                hand::hide_the_pointer,
                tank_spring,
                ants::place_queued,
                ants::pour_kit,
                radial::sync_radial_ui,
                sync_ant_transforms,
                brood::sync_brood_transforms,
                remesh_dirty_chunks,
            )
                .chain(),
        )
        .add_observer(title::on_menu_activate)
        .add_observer(pause::on_pause_activate)
        .add_observer(settings::on_control_activate)
        // Settings are read before the first frame and written on every change. They are
        // applied by *watching* the resource rather than by being pushed from the click, so
        // a value restored from disk lands by exactly the same road as one just cycled.
        .add_systems(PreStartup, settings::load_settings)
        .add_systems(
            Update,
            (
                settings::sync_settings_ui,
                settings::size_settings_ui,
                settings::refresh_readings,
                settings::apply_settings,
                settings::save_settings,
            )
                .chain(),
        )
        // The farm keeps running behind the Esc menu on purpose — an ambient game whose
        // colony froze whenever you opened a menu would be lying about what it is.
        .add_systems(
            Update,
            (pause::toggle_pause, pause::sync_pause_ui, pause::dress_pause_menu)
                .chain()
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(Update, devcapture::screenshot_hotkey);

    // Persistence, and deliberately not in capture mode.
    //
    // A scripted run must not read or write the player's farm. Reading one would put a
    // colony into a measurement that is supposed to start from bare strata, and writing
    // one would replace forty hours of somebody's tunnels with a test fixture. This is
    // the whole reason the harness is silent about saving rather than pointed at a
    // scratch directory: not touching the file at all is the only version with no way to
    // get it wrong.
    if !devcapture::capture_mode() {
        app
            // After the ant assets, which putting a saved colony back needs. Nothing else
            // has to know it happened — the grid and the ants are simply already there,
            // and the title screen finds `GameInProgress` true and offers Continue.
            // After the tank, and therefore after the ant and brood assets that come before
            // it in the same chain: restoring a farm means spawning bodies and a pile, and
            // parenting both to the tank they live in.
            .add_systems(
                Startup,
                (save::load_farm, away::catch_up_while_away)
                    .chain()
                    .after(setup_tank),
            )
            .add_systems(OnExit(GameState::Playing), save::save_farm)
            // On the way out of play, on the way out of the app, and on a timer in
            // between. The timer is the one that matters: a force-quit, a crash or a flat
            // battery sends no exit message, and none of them are the player's fault.
            .add_systems(
                Update,
                save::save_farm
                    .run_if(in_state(GameState::Playing))
                    .run_if(on_real_timer(Duration::from_secs(save::AUTOSAVE_SECONDS))),
            )
            .add_systems(Last, save::save_farm.run_if(on_message::<AppExit>));
    }

    // State registration has to come *after* DefaultPlugins, which is what brings
    // StatesPlugin with it. Verification runs skip the splash and the menu and open
    // straight into a live colony; the title-screen shot starts at the splash like any
    // other run, and `enter_splash` sends it on to the menu immediately.
    // Two separate questions. Every run that photographs UI has to keep the camera on the
    // window (see the offscreen note below) — but only the two that photograph the shell
    // want to *start* there. The wheel is chrome over a live farm, so it opens in play.
    let ui_shot = devcapture::title_shot()
        || devcapture::splash_shot()
        || devcapture::wheel_shot()
        || devcapture::hand_shot()
        || devcapture::settings_shot();
    let shell_shot = devcapture::title_shot() || devcapture::splash_shot();
    if devcapture::capture_mode() && !shell_shot {
        app.insert_state(GameState::Playing);
    } else {
        app.init_state::<GameState>();
    }

    // Scripted verification runs. Dev only.
    if devcapture::capture_mode() {
        app.insert_resource(devcapture::DevCapture::new());

        // Offscreen rendering is what makes the sand runs immune to a sleeping display,
        // but it moves the *only* camera off the window — and Bevy UI draws to whichever
        // camera is on the window. So the two runs that exist to photograph UI have to
        // keep the camera where it is and grab the window instead. Set the target up for
        // them and the screenshot is a perfectly plausible sheet of black.
        if !ui_shot {
            app.add_systems(Startup, devcapture::setup_offscreen_target.after(setup_tank));
        }

        if devcapture::settings_shot() {
            app.add_systems(Update, devcapture::run_settings_shot);
        } else if devcapture::hand_shot() {
            // Between the system that fills `Touch` in and the one that reads it, so the
            // faked pointer is what the hand actually sees.
            app.add_systems(
                Update,
                devcapture::run_hand_shot
                    .after(interact::track_touch)
                    .before(hand::move_hand),
            );
        } else if devcapture::wheel_shot() {
            app.add_systems(Update, devcapture::run_wheel_shot);
        } else if devcapture::splash_shot() {
            app.add_systems(Update, devcapture::run_splash_shot);
        } else if devcapture::title_shot() {
            app.add_systems(Update, devcapture::run_title_shot);
        } else if devcapture::sand_only() {
            // The original M1 cohesion test, with no colony to disturb the numbers.
            app.add_systems(Update, devcapture::run_capture.before(tank_spring));
        } else {
            app.init_resource::<devcapture::Census>().add_systems(
                Update,
                (devcapture::take_census, devcapture::run_colony_capture)
                    .chain()
                    .before(tank_spring),
            );
        }
    }

    app.run();
}
