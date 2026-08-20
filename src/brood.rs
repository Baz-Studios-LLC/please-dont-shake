//! The brood, and what the nurses are for.
//!
//! Two thirds of the colony had no job. Diggers dug, haulers hauled, and nurses correctly
//! went to the queen and stood there — which without brood reads as ants milling in a hole.
//! This is the other half of "behaviour is the storytelling": the colony grows, the youngest
//! workers have work, and the ending becomes an ending *of* something.
//!
//! Brood are entities rather than a layer of the sand grid, because they are laid one at a
//! time, carried individually, and sit in the air of a chamber rather than in the substrate.
//! A grid layer fights all three.
//!
//! The nurse's whole behaviour is two rules:
//!
//! 1. If you are carrying brood and you are near the queen, put it down.
//! 2. If you are not carrying brood and there is some lying away from the queen, pick it up.
//!
//! Nothing tells a nurse to build a pile. The pile is what those two rules add up to, because
//! nurses already walk up the queen's pheromone — the movement code needed no changes at all.
//! It is the first thing in this game that makes the nest look *inhabited* rather than
//! excavated.
//!
//! See DESIGN.md for the one deliberate inaccuracy: the cycle is compressed to about six
//! colony days, because an honest seven weeks does not fit inside a 40–60 hour lifespan.

use bevy::prelude::*;

use crate::ants::{Ant, AntAssets, ColonyStep, Job, Queen, body_bundle};
use crate::grid::*;
use crate::pheromones::{NavField, Ph, Pheromones};
use crate::tank::TankRoot;

/// Colony-days in each stage. Six days from laying to a walking worker.
const EGG_DAYS: f64 = 2.0;
const LARVA_DAYS: f64 = 2.5;
const PUPA_DAYS: f64 = 1.5;

/// Colony-days between eggs, and how much brood a queen will keep going at once.
///
/// The cap scales with the workforce because a real queen's laying rate does: a founding
/// queen with ten workers cannot feed a hundred larvae, and a colony that tried would be
/// modelling nothing.
const LAY_INTERVAL: f64 = 0.35;
const BASE_CLUTCH: usize = 4;
const CLUTCH_PER_WORKER: f32 = 0.5;

/// How close to the queen counts as "on the pile", and how far a nurse will reach for a
/// stray. The reach is generous: a nurse that had to stand exactly on an egg would spend its
/// life missing.
const PILE_RADIUS: f32 = 4.0;
const PICKUP_REACH: f32 = 3.0;

/// Colony-days a worker lives.
///
/// Without a top end the age model is a ramp: eggs keep arriving, nobody leaves, and every
/// ant eventually ages past `DIGGER_UNTIL` into surface work. Run at a day a second it took
/// sixty seconds to reach a hundred ants of which none dug and eighty-five patrolled — a
/// colony that had stopped building its own nest because everybody in it was old.
///
/// Real *Lasius* workers outlive this by a lot. It is scaled to the colony's own locked
/// lifespan of 40–60 days instead, so a farm turns its workforce over several times before
/// the queen's decline ends it, and the population is a curve rather than a climb.
const WORKER_LIFESPAN: f64 = 35.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Egg,
    Larva,
    Pupa,
}

impl Stage {
    fn lasts(self) -> f64 {
        match self {
            Stage::Egg => EGG_DAYS,
            Stage::Larva => LARVA_DAYS,
            Stage::Pupa => PUPA_DAYS,
        }
    }

    /// What it turns into. `None` means it ecloses into a worker.
    fn next(self) -> Option<Stage> {
        match self {
            Stage::Egg => Some(Stage::Larva),
            Stage::Larva => Some(Stage::Pupa),
            Stage::Pupa => None,
        }
    }
}

#[derive(Component)]
pub struct Brood {
    pub stage: Stage,
    /// Colony-days spent in the *current* stage, not since laying. `f64`, for the reason
    /// spelled out on [`ColonyClock`].
    pub age_days: f64,
    /// Grid coordinates, like an ant's. Brood are an overlay on the sand too.
    pub pos: Vec2,
    /// The nurse carrying it, if any.
    pub held_by: Option<Entity>,
}

/// Meshes and materials for the three stages, made once.
#[derive(Resource)]
pub struct BroodAssets {
    mesh: Handle<Mesh>,
    egg: Handle<StandardMaterial>,
    larva: Handle<StandardMaterial>,
    pupa: Handle<StandardMaterial>,
}

impl BroodAssets {
    fn material_for(&self, stage: Stage) -> Handle<StandardMaterial> {
        match stage {
            Stage::Egg => self.egg.clone(),
            Stage::Larva => self.larva.clone(),
            Stage::Pupa => self.pupa.clone(),
        }
    }
}

/// Everything that makes one brood item exist. Laying uses it, and so does restoring a saved
/// farm — the pile that comes back off disk has to be built the same way as the pile that was
/// laid, or the two paths drift and only one of them gets tested.
///
/// The `Transform` is a placeholder; `sync_brood_transforms` overwrites it on the first frame
/// from the item's own grid position.
pub fn brood_bundle(assets: &BroodAssets, item: Brood) -> impl Bundle {
    let material = assets.material_for(item.stage);
    (
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(material),
        Transform::default(),
        item,
    )
}

/// How long since the queen last laid, in colony-days.
#[derive(Resource, Default)]
pub struct LayClock(f64);

/// What the harness reports. Population is the number that matters for brood the way mass is
/// for sand: it should climb, and later it should fall.
#[derive(Resource, Default)]
pub struct BroodStats {
    pub eggs: usize,
    pub larvae: usize,
    pub pupae: usize,
    pub carried: usize,
    pub laid: u64,
    pub eclosed: u64,
    pub died: u64,
}

pub fn setup_brood_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // One cube for all three, scaled per stage. At this size a shape would be a pixel of
    // shape; the colour and the size are what read.
    let mesh = meshes.add(Cuboid::new(CELL * 1.5, CELL * 1.1, CELL * 1.1));
    let mut pale = |r: f32, g: f32, b: f32| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness: 0.55,
            reflectance: 0.10,
            ..default()
        })
    };
    commands.insert_resource(BroodAssets {
        mesh,
        // Eggs are tiny and almost white; larvae are bigger, fatter and creamier; pupae go
        // browner as the adult cuticle forms under the skin, which is what actually happens.
        egg: pale(0.95, 0.94, 0.88),
        larva: pale(0.94, 0.90, 0.76),
        pupa: pale(0.72, 0.60, 0.44),
    });
}

/// How far below the ground the queen has to be before she will lay, in cells.
///
/// Not zero, and the panel is why: with "under the terrain" as the whole test she counted as
/// founded standing in a one-cell scrape and the readout said `founded, 0 cells down, laying`,
/// which is laying in the doorway rather than in a chamber — the same fault as laying on the
/// lawn, moved by a cell.
///
/// Six is a judgement, not a measurement. A real founding chamber is a few centimetres down,
/// which at 1.2mm a cell would be twenty-odd and would hold the colony's first egg behind a
/// long dig; six is deep enough to be a chamber and shallow enough that a young colony gets
/// there. Both `lay_eggs` and the dev panel read this constant, because when they each had
/// their own idea of "underground" they disagreed on screen.
pub const FOUNDING_DEPTH: f32 = 6.0;

/// Which column the queen is standing in, clamped to the tank.
fn queen_column(queen: &Ant) -> usize {
    (queen.pos.x.max(0.0) as usize).min(GRID_W - 1)
}

/// The queen lays, if she has room for another.
pub fn lay_eggs(
    mut commands: Commands,
    step: Res<ColonyStep>,
    mut lay: ResMut<LayClock>,
    mut stats: ResMut<BroodStats>,
    assets: Option<Res<BroodAssets>>,
    nav: Res<NavField>,
    tank: Query<Entity, With<TankRoot>>,
    queen: Query<&Ant, With<Queen>>,
    workers: Query<(), (With<Ant>, Without<Queen>)>,
    brood: Query<(), With<Brood>>,
) {
    // The *first* queen, not the only one. `queen.single()` here and in `tend_brood` was a
    // silent catastrophe waiting: two queens in one tank made `single()` fail, so the colony
    // stopped laying and stopped tending its brood, forever, with nothing logged and nothing
    // to see except a farm that had quietly given up. A second ant kit is all it took, and the
    // capture run was doing exactly that for weeks. Design says one queen per farm; the code
    // should not detonate if it ever gets two.
    let (Some(assets), Some(queen), Ok(tank)) = (assets, queen.iter().next(), tank.single())
    else {
        return;
    };

    // Not on the lawn. A founding *Lasius* queen digs herself in, seals the chamber and lays in
    // there; she does not lay on open sand, and eggs on the surface are eggs the weather and the
    // first shake take. Brett asked whether that was normal and it is not — the code laid at
    // `queen.pos` whatever `queen.pos` happened to be, which before she could move was wherever
    // the tube dropped her.
    //
    // This makes the founding stage mean something: the colony has to open a hole and get her
    // into it before it can grow at all. Nothing has to *decide* she is in, either — she is in
    // when she is under the terrain, and her own descent is what puts her there.
    let underground =
        queen.pos.y + FOUNDING_DEPTH <= nav.surface_at(queen_column(&queen)) as f32;
    if !underground {
        // Held, not lost: the clock keeps running, so she lays as soon as she is in rather than
        // waiting out another whole interval underground.
        lay.0 = lay.0.min(LAY_INTERVAL);
        return;
    }

    lay.0 += step.0;
    if lay.0 < LAY_INTERVAL {
        return;
    }
    // Subtract rather than zero. Zeroing throws away the overshoot, which is nothing at a
    // sixtieth of a second and is most of the interval when `crate::away` steps the same
    // system forward a tenth of a day at a time — the queen would lay at whatever rate the
    // catch-up happened to step at instead of her own.
    lay.0 -= LAY_INTERVAL;

    let clutch = BASE_CLUTCH + (workers.iter().count() as f32 * CLUTCH_PER_WORKER) as usize;
    if brood.iter().count() >= clutch {
        return;
    }

    // Laid where she stands. Nurses take it from there — and where she stands is already the
    // deepest the colony has got, so the first eggs of a farm are on the pile by definition.
    let egg = Brood { stage: Stage::Egg, age_days: 0.0, pos: queen.pos, held_by: None };
    commands.spawn((brood_bundle(&assets, egg), ChildOf(tank)));
    stats.laid += 1;
}

/// Brood ages, changes stage, and eventually walks away.
pub fn age_brood(
    mut commands: Commands,
    step: Res<ColonyStep>,
    ants: Res<AntAssets>,
    assets: Option<Res<BroodAssets>>,
    mut stats: ResMut<BroodStats>,
    tank: Query<Entity, With<TankRoot>>,
    mut brood: Query<(Entity, &mut Brood, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let (Some(assets), Ok(tank)) = (assets, tank.single()) else {
        return;
    };
    let days = step.0;

    for (entity, mut item, mut material) in &mut brood {
        item.age_days += days;
        if item.age_days < item.stage.lasts() {
            continue;
        }

        match item.stage.next() {
            Some(stage) => {
                // Carry the overshoot into the new stage rather than dropping it, for the
                // reason spelled out in `lay_eggs`: `crate::away` steps this a tenth of a day
                // at a time, and a stage that started from zero each time would run long by
                // however much of the step was left over. Subtract before reassigning — the
                // debt belongs to the stage being left.
                item.age_days -= item.stage.lasts();
                item.stage = stage;
                material.0 = assets.material_for(stage);
            }
            None => {
                // Eclosion. A worker at the age it has already earned — which is nothing in
                // play and can be most of a step during a catch-up — and `Job::for_age` picks
                // that up as a nurse with no changes. The labour split has been waiting for
                // this since it was written.
                let ant = Ant {
                    pos: item.pos,
                    heading: Vec2::X,
                    vel: Vec2::ZERO,
                    age_days: item.age_days - item.stage.lasts(),
                    carrying: None,
                    dig_cooldown: 0.0,
                    haul_time: 0.0,
                    dug_at: Vec2::ZERO,
                    dislodged: 0.0,
                    z: SLAB_DEPTH * 0.5 - 0.03,
                };
                commands.spawn((body_bundle(&ants, ant, false), ChildOf(tank)));
                commands.entity(entity).despawn();
                stats.eclosed += 1;
            }
        }
    }
}

/// Nurses gather the brood. Two rules; the pile is emergent.
pub fn tend_brood(
    mut stats: ResMut<BroodStats>,
    queen: Query<&Ant, With<Queen>>,
    nurses: Query<(Entity, &Ant), Without<Queen>>,
    mut brood: Query<(Entity, &mut Brood)>,
) {
    // The first queen. See `lay_eggs` for why this is not `single()`.
    let Some(queen) = queen.iter().next() else {
        return;
    };

    stats.eggs = 0;
    stats.larvae = 0;
    stats.pupae = 0;
    stats.carried = 0;

    // Who is already carrying something, so nobody ends up with two.
    let mut laden: Vec<Entity> = Vec::new();
    for (_, item) in &brood {
        if let Some(nurse) = item.held_by {
            laden.push(nurse);
        }
    }

    for (_, mut item) in &mut brood {
        match item.stage {
            Stage::Egg => stats.eggs += 1,
            Stage::Larva => stats.larvae += 1,
            Stage::Pupa => stats.pupae += 1,
        }

        match item.held_by {
            Some(nurse) => {
                let Ok((_, ant)) = nurses.get(nurse) else {
                    // The nurse is gone. Put it down where it is rather than losing it.
                    item.held_by = None;
                    continue;
                };
                stats.carried += 1;
                item.pos = ant.pos;
                // Rule one: near the queen, this is the pile. Put it down.
                if item.pos.distance(queen.pos) <= PILE_RADIUS {
                    item.held_by = None;
                }
            }
            None => {
                // Rule two: anything lying away from the queen wants fetching. A nurse walks
                // up her pheromone already, so carrying needs no steering of its own.
                if item.pos.distance(queen.pos) <= PILE_RADIUS {
                    continue;
                }
                let free_nurse = nurses.iter().find(|(entity, ant)| {
                    Job::for_age(ant.age_days) == Job::Nurse
                        && !laden.contains(entity)
                        && ant.carrying.is_none()
                        && ant.pos.distance(item.pos) <= PICKUP_REACH
                });
                if let Some((entity, _)) = free_nurse {
                    item.held_by = Some(entity);
                    laden.push(entity);
                }
            }
        }
    }
}

/// Workers die of old age.
///
/// Quietly and with no ceremony, which is the design's whole line on grimness: it is just
/// what happens. The body is not left behind — a midden of corpses is a real behaviour and a
/// real feature, and it belongs with foraging rather than here.
///
/// The queen is exempt. Her ending is the colony's, and it is M3's own piece of work.
pub fn age_out(
    mut commands: Commands,
    mut grid: ResMut<SandGrid>,
    mut stats: ResMut<BroodStats>,
    workers: Query<(Entity, &Ant), Without<Queen>>,
) {
    for (entity, ant) in &workers {
        if ant.age_days > WORKER_LIFESPAN {
            // Whatever it was carrying goes back in the tank.
            //
            // Sand is conserved exactly in this game, and an ant that died holding a grain
            // used to take it out of the world. It measured as mass drift and nothing else:
            // 25344 grains down to 25330 over a hundred and twenty-five days and two hundred
            // and twenty-nine deaths, with about a third of the colony hauling at any moment.
            // The mass invariant is the one number the harness watches hardest, and this is
            // the kind of leak it exists to catch — a tenth of a percent, invisible by eye,
            // and permanent.
            //
            // `settle` is the same function the save file uses to put in-flight grains back:
            // it searches upward for air, so it can never overwrite what is already there.
            if let Some(shade) = ant.carrying {
                let (cx, cy) = crate::ants::cell_of(ant.pos);
                if !crate::grains::settle(&mut grid, cx, cy, shade) {
                    warn!("a worker died holding a grain and the tank had nowhere to put it");
                }
            }
            commands.entity(entity).despawn();
            stats.died += 1;
        }
    }
}

/// The pile asks for room.
///
/// Its own system because brood has no other reason to touch the fields, and because this is the
/// half of crowding that does not walk away: eggs sit where the nurses put them, so the demand
/// stays in one place long enough for the colony to answer it by digging. A chamber is what that
/// answer looks like.
pub fn brood_crowds(time: Res<Time>, mut ph: ResMut<Pheromones>, brood: Query<&Brood>) {
    let dt = time.delta_secs();
    for item in &brood {
        let (x, y) = (
            (item.pos.x.max(0.0) as usize).min(GRID_W - 1),
            (item.pos.y.max(0.0) as usize).min(GRID_H - 1),
        );
        ph.deposit(Ph::Crowd, x, y, crate::ants::CROWD_PER_BROOD * dt);
    }
}

/// Brood doesn't walk, so nothing else keeps it out of the sand.
///
/// A collapse fills the chamber it was lying in, and an egg drawn inside a solid cell reads
/// as a bug rather than as a burial. Lifting it to the nearest air above is the cheap honest
/// answer: the pile gets pushed up by a cave-in instead of vanishing into it.
pub fn unbury_brood(grid: Res<SandGrid>, mut brood: Query<&mut Brood>) {
    for mut item in &mut brood {
        if item.held_by.is_some() {
            continue;
        }
        let (x, y) = (item.pos.x.floor() as isize, item.pos.y.floor() as isize);
        if !SandGrid::in_bounds(x, y) || grid.is_air(x, y) {
            continue;
        }
        for up in 1..8 {
            if grid.is_air(x, y + up) {
                item.pos.y = (y + up) as f32 + 0.5;
                break;
            }
        }
    }
}

pub fn sync_brood_transforms(mut brood: Query<(&Brood, &mut Transform)>) {
    for (item, mut tf) in &mut brood {
        let mut p = SandGrid::cell_to_world(0, 0);
        p.x = (item.pos.x - GRID_W as f32 * 0.5) * CELL;
        p.y = (item.pos.y - GRID_H as f32 * 0.5) * CELL;
        // In front of the sand but behind the ants, so a nurse standing over the pile reads
        // as standing over it.
        p.z = SLAB_DEPTH * 0.5 - 0.06;
        tf.translation = p;
        // Larvae are fatter than eggs, and a pupa is nearly a worker.
        tf.scale = Vec3::splat(match item.stage {
            Stage::Egg => 0.7,
            Stage::Larva => 1.0,
            Stage::Pupa => 1.25,
        });
    }
}

/// Dummy handles. See [`crate::ants::stub_assets`].
#[cfg(test)]
pub(crate) fn stub_assets() -> BroodAssets {
    BroodAssets {
        mesh: Handle::default(),
        egg: Handle::default(),
        larva: Handle::default(),
        pupa: Handle::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cycle is six colony days, and every stage leads somewhere. Stated as a test
    /// because the six is a design decision — see DESIGN.md — and a change to any one stage
    /// should have to notice it is changing the whole.
    #[test]
    fn the_cycle_runs_egg_to_worker_in_six_days() {
        assert_eq!(Stage::Egg.next(), Some(Stage::Larva));
        assert_eq!(Stage::Larva.next(), Some(Stage::Pupa));
        assert_eq!(Stage::Pupa.next(), None, "a pupa ecloses; it does not become brood");

        let total = Stage::Egg.lasts() + Stage::Larva.lasts() + Stage::Pupa.lasts();
        assert_eq!(total, 6.0, "the cycle is six colony days");
    }

    /// Sand is conserved exactly, and death is not an exception.
    ///
    /// A worker that dies with a grain in its mandibles used to delete it. Over a
    /// hundred-and-twenty-five-day run that was fourteen grains gone for good, which is
    /// exactly the kind of slow leak the mass check exists to catch.
    #[test]
    fn a_worker_that_dies_hauling_puts_the_grain_back() {
        let mut grid = SandGrid::new();
        fill_strata(&mut grid, INITIAL_SURFACE);
        let before = grid.sand_count();

        let mut app = App::new();
        app.insert_resource(grid)
            .init_resource::<BroodStats>()
            .add_systems(Update, age_out);
        app.world_mut().spawn(Ant {
            pos: Vec2::new(128.0, (INITIAL_SURFACE + 12) as f32),
            heading: Vec2::X,
            vel: Vec2::ZERO,
            age_days: WORKER_LIFESPAN + 1.0,
            carrying: Some(9),
            dig_cooldown: 0.0,
            haul_time: 0.0,
            dug_at: Vec2::ZERO,
            dislodged: 0.0,
            z: 0.0,
        });
        app.update();

        assert_eq!(app.world().resource::<BroodStats>().died, 1, "the worker did not age out");
        let mut ants = app.world_mut().query::<&Ant>();
        assert_eq!(ants.iter(app.world()).count(), 0, "the body was left behind");
        assert_eq!(
            app.world().resource::<SandGrid>().sand_count(),
            before + 1,
            "the grain it was carrying was not put back",
        );
    }

    /// A queen with more workers keeps more brood going. The floor matters as much as the
    /// slope: a founding queen alone must still be able to lay.
    #[test]
    fn the_clutch_grows_with_the_workforce() {
        let clutch = |workers: usize| {
            BASE_CLUTCH + (workers as f32 * CLUTCH_PER_WORKER) as usize
        };
        assert_eq!(clutch(0), BASE_CLUTCH, "a queen alone must still lay");
        assert!(clutch(10) > clutch(0));
        assert!(clutch(100) > clutch(10));
    }
}
