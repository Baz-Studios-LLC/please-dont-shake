//! While you were away.
//!
//! The colony's clock runs in real time — a day is a day — which is only a coherent thing to
//! say if it keeps running with the app shut. Six days from egg to worker means nobody would
//! ever see one hatch otherwise: an evening of play is 1.4% of a single stage, so a farm that
//! only lived while it was being watched would be a farm that never changed. This module is
//! what makes the real-time decision mean something.
//!
//! What it does **not** do is simulate the farm. Nothing walks, nothing digs, and not one
//! grain of sand moves while you're gone; the tunnels are exactly as you left them. Only
//! biology is settled up — the brood advances, pupae eclose, workers age and the oldest die,
//! and the queen goes on laying. That split is the honest one. Ageing is a number times a
//! duration and can be resolved in closed form; excavation is two hundred ants reading a
//! pheromone field sixty times a second, and claiming to know where they would have dug would
//! be inventing a farm rather than continuing one.
//!
//! ## Why it steps rather than adds
//!
//! The catch-up runs the game's own systems — [`crate::brood::lay_eggs`],
//! [`crate::ants::age_ants`], [`crate::brood::age_brood`], [`crate::brood::age_out`] — over
//! and over, with [`ColonyStep`] set to a slice of a day instead of a slice of a second. That
//! is the whole trick, and it is deliberate: the alternative is a second copy of the rules
//! that only runs at startup, which would be the copy nobody notices has drifted. Laying is
//! clutch-capped against a headcount that changes as workers die and pupae eclose, so the
//! order matters and the loop has to actually iterate.
//!
//! It is capped, at [`MAX_DAYS`]. Past a couple of months the workforce has turned over
//! several times and the exact number stops meaning anything, and an uncapped loop would let
//! a machine whose clock jumped a decade sit at a black window grinding out four million
//! steps.

use bevy::prelude::*;

use crate::ants::ColonyStep;
use crate::settings::Settings;

/// Colony-days owed to the farm: how long the app was shut, converted by the colony's own
/// clock rate. Written by `load_farm`, spent once by [`catch_up_while_away`].
#[derive(Resource, Default)]
pub struct AwaySpan(pub f64);

/// How much colony time one catch-up step covers.
///
/// Comfortably below the shortest life stage (1.5 days, the pupa) so nothing skips one, and
/// below the queen's laying interval (0.35) so she doesn't lay in bursts. Both of those carry
/// their remainder forward anyway, which is what makes the size a matter of resolution rather
/// than of correctness.
const STEP_DAYS: f64 = 0.1;

/// The most one catch-up will resolve — three months, which is past the end of a colony on
/// this design's own timescale.
const MAX_DAYS: f64 = 90.0;

/// Below this, there is nothing to settle up and the run is skipped.
///
/// A minute, not a step: the loop's last slice is whatever is left, so a gap of an hour is one
/// short step rather than a rounding error. Dropping everything under a step would quietly bin
/// two and a half hours of colony time every time the app was opened, which for anyone who
/// looks in on the farm a few times a day is most of the time they were away.
const MIN_DAYS: f64 = 1.0 / 1440.0;

/// Settle up, once, at startup. After `load_farm`, which is what tells it how long.
///
/// Exclusive because it drives other systems, which is not something a normal system can do.
pub fn catch_up_while_away(world: &mut World) {
    let owed = std::mem::take(&mut world.resource_mut::<AwaySpan>().0);
    if owed < MIN_DAYS {
        return;
    }

    // The player's call, and the reason it is a setting: a farm that ages while you're gone is
    // the honest version of a real-time clock, and it also means a fortnight away turns the
    // whole workforce over without you. Off, and the tank waits.
    if !world.resource::<Settings>().away {
        info!("{owed:.1} colony-days passed, and the farm was left waiting");
        return;
    }

    let days = owed.min(MAX_DAYS);
    if days < owed {
        info!("away for {owed:.0} days; settling up the last {MAX_DAYS:.0}");
    }

    let before = {
        let stats = world.resource::<crate::brood::BroodStats>();
        (stats.laid, stats.eclosed, stats.died)
    };

    // Each system on its own, so a failure names itself instead of being swallowed. A system
    // that cannot run here is a wiring bug, and the farm is better off unaged than half-aged.
    macro_rules! run {
        ($system:path) => {
            if let Err(why) = world.run_system_cached($system) {
                warn!("the catch-up could not run {}: {why}", stringify!($system));
                world.resource_mut::<ColonyStep>().0 = 0.0;
                return;
            }
        };
    }

    let mut left = days;
    while left > 0.0 {
        world.resource_mut::<ColonyStep>().0 = left.min(STEP_DAYS);
        // Laying first, so the eggs of this step can be aged by it; then the colony gets
        // older; then the brood turns over, and the oldest workers are done. Same order as
        // the fixed schedule, minus everything that involves moving.
        run!(crate::brood::lay_eggs);
        run!(crate::ants::age_ants);
        run!(crate::brood::age_brood);
        run!(crate::brood::age_out);
        // Spawned brood and despawned workers have to be real before the next step counts
        // them — the clutch cap and the headcount both read the world as it is.
        world.flush();
        left -= STEP_DAYS;
    }
    world.resource_mut::<ColonyStep>().0 = 0.0;

    let stats = world.resource::<crate::brood::BroodStats>();
    info!(
        "while you were away: {days:.1} days — {} laid, {} hatched, {} died of old age",
        stats.laid - before.0,
        stats.eclosed - before.1,
        stats.died - before.2,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ants::{Ant, Queen};
    use crate::brood::{Brood, BroodStats, LayClock, Stage, brood_bundle};
    use crate::tank::TankRoot;

    /// The cap has to be a real cap, and the step has to be small enough that nothing it
    /// drives can skip a stage. Both are stated here because both are silent when wrong: a
    /// step longer than a stage loses time, and a missing cap is a hang.
    #[test]
    fn the_step_fits_inside_every_stage() {
        const {
            assert!(STEP_DAYS < 0.35, "a step longer than the laying interval batches eggs");
            assert!(MAX_DAYS / STEP_DAYS < 2000.0, "the catch-up loop is effectively unbounded");
            assert!(MIN_DAYS < STEP_DAYS, "a gap smaller than one step must still be settled");
        }
    }

    /// A queen, one worker near the end of its life, and a pupa nearly ready.
    fn farm(away_days: f64, growing: bool) -> App {
        let mut app = App::new();
        let settings = crate::settings::Settings { away: growing, ..default() };
        app.insert_resource(settings)
            .insert_resource(AwaySpan(away_days))
            .init_resource::<ColonyStep>()
            .init_resource::<BroodStats>()
            .init_resource::<LayClock>()
            .insert_resource(crate::ants::stub_assets())
            .insert_resource(crate::brood::stub_assets())
            .add_systems(Update, catch_up_while_away);
        app.world_mut().spawn(TankRoot);

        let body = |age_days: f64| Ant {
            pos: Vec2::new(128.0, 40.0),
            heading: Vec2::X,
            vel: Vec2::ZERO,
            age_days,
            carrying: None,
            dig_cooldown: 0.0,
            haul_time: 0.0,
            dug_at: Vec2::ZERO,
            dislodged: 0.0,
            z: 0.0,
        };
        app.world_mut().spawn((body(400.0), Queen));
        app.world_mut().spawn(body(30.0));
        // Through `brood_bundle`, like every real pile: `age_brood` reaches for the material
        // it repaints on a stage change, so a bare `Brood` is invisible to it. A test that
        // hand-rolled the components would silently be testing nothing.
        let pupa = Brood {
            stage: Stage::Pupa,
            age_days: 1.0,
            pos: Vec2::new(128.0, 40.0),
            held_by: None,
        };
        app.world_mut().spawn(brood_bundle(&crate::brood::stub_assets(), pupa));
        app
    }

    fn ages(app: &mut App) -> Vec<f64> {
        let mut query = app.world_mut().query_filtered::<&Ant, Without<Queen>>();
        let mut out: Vec<f64> = query.iter(app.world()).map(|ant| ant.age_days).collect();
        out.sort_by(f64::total_cmp);
        out
    }

    /// A week away, resolved: the pupa is a worker, the old worker is dead, and the queen has
    /// been laying the whole time. This is the feature — a farm you come back to has moved on.
    #[test]
    fn a_week_away_hatches_the_brood() {
        let mut app = farm(7.0, true);
        app.update();

        let stats = app.world().resource::<BroodStats>();
        assert!(stats.eclosed >= 1, "the pupa never hatched");
        assert!(stats.laid > 0, "the queen laid nothing in a week");
        assert_eq!(stats.died, 1, "the thirty-day-old worker should have aged out");

        let queen = {
            let mut query = app.world_mut().query_filtered::<&Ant, With<Queen>>();
            query.iter(app.world()).next().unwrap().age_days
        };
        assert!(
            (queen - 407.0).abs() < 0.01,
            "the queen aged {} days in a week away",
            queen - 400.0,
        );

        // The pupa had half a day of its stage left, so its worker is six and a half days
        // old — the catch-up hatched it partway through and then kept ageing it.
        let workers = ages(&mut app);
        assert!(
            workers.iter().any(|age| (age - 6.5).abs() < 0.15),
            "no worker came out of the pupa; ages were {workers:?}",
        );
        assert!(
            workers.iter().all(|age| *age <= 35.0),
            "a worker outlived its lifespan during the catch-up: {workers:?}",
        );
    }

    fn remaining(app: &mut App) -> usize {
        let mut query = app.world_mut().query::<&Brood>();
        query.iter(app.world()).count()
    }

    /// Turned off, the tank waits. Nothing about the colony may move — not the brood, not an
    /// age, not one egg — because the player asked for the farm to be exactly as they left it.
    #[test]
    fn a_farm_left_waiting_does_not_move() {
        let mut app = farm(7.0, false);
        app.update();

        let stats = app.world().resource::<BroodStats>();
        assert_eq!((stats.laid, stats.eclosed, stats.died), (0, 0, 0));
        assert_eq!(ages(&mut app), vec![30.0], "a worker aged while the farm was waiting");
        assert_eq!(remaining(&mut app), 1, "the brood changed while the farm was waiting");
    }

    /// An hour's gap is an hour, not nothing. Someone who looks in on the farm over breakfast
    /// and again at lunch must not have both gaps rounded away.
    #[test]
    fn an_hour_away_is_an_hour() {
        let mut app = farm(1.0 / 24.0, true);
        app.update();

        let queen = {
            let mut query = app.world_mut().query_filtered::<&Ant, With<Queen>>();
            query.iter(app.world()).next().unwrap().age_days
        };
        assert!(
            (queen - (400.0 + 1.0 / 24.0)).abs() < 1e-9,
            "an hour away aged the queen {} days",
            queen - 400.0,
        );
    }

    /// A machine whose clock jumped forward — or a farm genuinely abandoned for a year —
    /// settles up three months and stops. Without the cap this is a black window and a
    /// million steps.
    #[test]
    fn a_year_away_is_capped() {
        let mut app = farm(365.0, true);
        app.update();

        let queen = {
            let mut query = app.world_mut().query_filtered::<&Ant, With<Queen>>();
            query.iter(app.world()).next().unwrap().age_days
        };
        assert!(
            (queen - (400.0 + MAX_DAYS)).abs() < 0.01,
            "a year away aged the queen {} days",
            queen - 400.0,
        );
    }
}

