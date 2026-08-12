//! The dev panel. **F1.**
//!
//! Everything in here already existed as a keystroke and a line of `info!`, which is exactly
//! why it needed building: the speed keys shipped in v0.3.0 and announced themselves to a
//! terminal, and the game is launched from a launcher and a `.app` where there is no terminal
//! to announce anything to. Brett asked whether the feature had ever been added. It had. It was
//! invisible, which for a testing tool is the same as absent.
//!
//! **The keys do the acting; the panel only tells you.** No buttons, and that is a decision
//! rather than a shortcut. A non-modal panel with controls sitting over the tank has to work out
//! whether a click was meant for it or for the glass, and getting that wrong means a dev tool
//! that shakes the farm you are trying to observe. Keys have no such ambiguity, they are faster
//! to test with, and the panel is free to be a readout — which is the part that was missing.
//!
//! It ships in the release build, like F12 and the speed keys, for the reason given in
//! `devcapture::SPEEDS`: release is the only build this game can be tested in, so a
//! `debug_assertions` gate would put the tool where it cannot be used.

use bevy::prelude::*;
use ordo::prelude::*;

use crate::ants::{Ant, ColonyStats, Queen};
use crate::away::AwayReport;
use crate::brood::{BroodStats, FOUNDING_DEPTH};
use crate::devcapture::{Census, ColonySpeed, SPEEDS, excavated_volume, mound_volume};
use crate::grid::{GRID_W, SandGrid};
use crate::grains::Grain;
use crate::pheromones::{NavField, UNREACHABLE};

/// Whether the panel is up. A resource, like the Esc menu's, because the farm keeps running
/// underneath either way.
#[derive(Resource, Default)]
pub struct DevPanel {
    pub open: bool,
}

/// Everything spawned for the panel, so closing it is one despawn.
#[derive(Component)]
pub struct DevPanelUi;

/// The block of text rewritten every frame.
#[derive(Component)]
pub struct DevReadout;

/// The sand total when the panel was first opened, so drift can be shown against something.
///
/// Not the tank's starting mass: the panel can be opened onto a farm that has been played for
/// forty hours, and there is no honest baseline available then. Drift from when you started
/// looking is the answer that is actually true, and it is the one that catches a leak.
///
/// The total it holds counts all **three** places a grain can be — in the grid, in the air, and
/// in a pair of mandibles. Counting only the grid, which this did at first, reads a farm with
/// nineteen grains being carried as a farm that has *lost* nineteen grains, and reports a leak
/// in the one number this game promises never to leak. It said `drift -19` on a perfectly
/// healthy colony. The harness has always summed all three; so does this now.
#[derive(Resource, Default)]
pub struct SandBaseline(pub Option<usize>);

pub fn toggle_dev_panel(keys: Res<ButtonInput<KeyCode>>, mut dev: ResMut<DevPanel>) {
    if keys.just_pressed(KeyCode::F1) {
        dev.open = !dev.open;
    }
}

/// `P` freezes the world for a look.
///
/// The Esc menu deliberately does *not* pause — see `crate::pause`, the farm keeps running
/// because a colony that froze whenever you opened a menu would be lying about what it is. This
/// is allowed to break that rule because it is not a menu: holding one frame still is half of
/// what a testing tool is for, and nobody reaches F1 by accident.
pub fn pause_key(
    keys: Res<ButtonInput<KeyCode>>,
    dev: Res<DevPanel>,
    mut time: ResMut<Time<Virtual>>,
) {
    if !dev.open || !keys.just_pressed(KeyCode::KeyP) {
        return;
    }
    if time.is_paused() {
        time.unpause();
    } else {
        time.pause();
    }
}

pub fn sync_dev_panel(
    mut commands: Commands,
    dev: Res<DevPanel>,
    mut baseline: ResMut<SandBaseline>,
    mut time: ResMut<Time<Virtual>>,
    existing: Query<Entity, With<DevPanelUi>>,
) {
    match (dev.open, existing.iter().next()) {
        (true, None) => {
            let root = commands
                .spawn((
                    DevPanelUi,
                    panel(Anchor::TopLeft, Some(300.0)),
                    children![heading("Dev — F1")],
                ))
                .id();
            commands.spawn((rule(), ChildOf(root)));
            commands.spawn((body(""), DevReadout, ChildOf(root)));
            commands.spawn((rule(), ChildOf(root)));
            commands.spawn((
                dim("[ ] colony speed   P pause   F12 shot"),
                ChildOf(root),
            ));
        }
        (false, Some(entity)) => {
            commands.entity(entity).despawn();
            // Forgotten on close, so reopening measures drift from now rather than from a
            // session you have stopped thinking about.
            baseline.0 = None;
            // Closing the panel while paused would otherwise leave the farm frozen with the one
            // thing that said so gone from the screen, and `P` no longer listening.
            time.unpause();
        }
        _ => {}
    }
}

/// Fill the readout. Only runs while the panel is up, so a closed panel costs nothing —
/// `excavated_volume` and `mound_volume` are both full sweeps of the grid.
pub fn update_readout(
    mut readout: Query<&mut Text, With<DevReadout>>,
    mut baseline: ResMut<SandBaseline>,
    speed: Res<ColonySpeed>,
    time: Res<Time<Virtual>>,
    grid: Res<SandGrid>,
    nav: Res<NavField>,
    stats: Res<ColonyStats>,
    brood: Res<BroodStats>,
    census: Res<Census>,
    away: Res<AwayReport>,
    ants: Query<&Ant>,
    grains: Query<(), With<Grain>>,
    queen: Query<&Ant, With<Queen>>,
    queens: Query<(), With<Queen>>,
) {
    let Ok(mut text) = readout.single_mut() else {
        return;
    };

    let in_flight = grains.iter().count();
    let carried = ants.iter().filter(|ant| ant.carrying.is_some()).count();
    let sand = grid.sand_count() + in_flight + carried;
    let base = *baseline.0.get_or_insert(sand);

    // Measured against the *terrain envelope*, because that is the test `lay_eggs` uses to decide
    // whether she is founded — and a readout that answers a different question than the rule is
    // worse than no readout. It said "on the surface" beside seven eggs, which cannot both be
    // true, and the disagreement was only that this line compared against the original fill line
    // while the rule compared against the ground as it is now. Spoil heaped round a shaft mouth
    // is the gap between them.
    //
    // Whether she is laying is the thing you actually want to know, so it says that.
    let queen_line = match queen.single() {
        Ok(her) => {
            let column = (her.pos.x.max(0.0) as usize).min(GRID_W - 1);
            let ground = nav.surface_at(column) as f32;
            let (cx, cy) = (her.pos.x as isize, her.pos.y as isize);
            if nav.at(cx, cy) == UNREACHABLE {
                format!("sealed in, y {:.0}", her.pos.y)
            } else if her.pos.y + FOUNDING_DEPTH <= ground {
                format!("founded, {:.0} cells down, laying", ground - her.pos.y)
            } else {
                format!(
                    "{:.0} cells down, needs {FOUNDING_DEPTH:.0} to lay",
                    (ground - her.pos.y).max(0.0)
                )
            }
        }
        // `single` fails on none *and* on more than one, and the difference matters: two queens
        // is what silently broke the brood once. Say which.
        Err(_) => format!("{} of them (want 1)", queens.iter().count()),
    };

    let (_, speed_name) = SPEEDS[speed.0.min(SPEEDS.len() - 1)];
    let clock = if time.is_paused() { "PAUSED" } else { "running" };

    // Every ant accounted for. The job tally skips whoever is buried or in the air, so on its
    // own it silently fails to add up to the headcount — 28 + 35 + 23 against 101 ants reads as
    // a broken counter when it is really thirteen ants mid-fall and one under a collapse.
    let away_line = if away.days <= 0.0 {
        "nothing owed (or the setting is off)".to_string()
    } else {
        format!(
            "{:.1} days settled{} — {} laid, {} hatched, {} died",
            away.days,
            if away.capped { ", capped" } else { "" },
            away.laid,
            away.hatched,
            away.died,
        )
    };

    text.0 = format!(
        "speed    {speed_name}\n\
         clock    {clock}\n\
         away     {away_line}\n\
         \n\
         ants     {ants}  ({nurses} nurse, {diggers} dig, {surface} surf)\n\
         also     {falling} falling, {buried} buried, {queens} queen\n\
         brood    {brood_total}  ({eggs} egg, {larvae} larva, {pupae} pupa)\n\
         queen    {queen_line}\n\
         \n\
         dug      {excavated} out, {mound} in the heap\n\
         sand     {sand}  (drift {drift:+})  {in_flight} in air, {carried} carried\n\
         idle     {idle} now, stuck {stuck}\n\
         glass    {glass}",
        ants = ants.iter().count(),
        queens = queens.iter().count(),
        falling = stats.falling,
        nurses = stats.nurses,
        diggers = stats.diggers,
        surface = stats.surface,
        brood_total = brood.eggs + brood.larvae + brood.pupae,
        eggs = brood.eggs,
        larvae = brood.larvae,
        pupae = brood.pupae,
        excavated = excavated_volume(&grid),
        mound = mound_volume(&grid),
        drift = sand as i64 - base as i64,
        idle = census.stalled,
        stuck = census.stuck,
        buried = stats.buried,
        glass = stats.at_the_glass,
    );
}

pub fn panel_is_open(dev: Res<DevPanel>) -> bool {
    dev.open
}
