//! The colony.
//!
//! No ant here knows what a tunnel is. Each one reads a couple of numbers off the
//! fields in [`crate::pheromones`], keeps roughly heading the way it was already going,
//! and either bites out the grain in front of it or hauls one it's already holding
//! toward the sky. Nest architecture is what that adds up to — which is the whole
//! reason the farm's history is worth protecting, and the whole reason shaking hurts.
//!
//! Three things do most of the work:
//!
//! - **Work attracts work.** Excavating deposits `Dig` pheromone, and diggers climb it.
//!   Without this they scatter and the farm turns into gravel; with it, digging
//!   concentrates at a face and the face advances. This is stigmergy, and it's the
//!   single most important rule in the file.
//! - **Heading persistence.** A digger that re-randomised its direction each tick would
//!   hollow out spheres. Keeping its heading is what makes a *shaft*.
//! - **Labour by age.** Nothing assigns jobs. Young workers stay in with the queen,
//!   middle-aged ones dig, the oldest drift to the surface — from one `age` float. It's
//!   also why the ants nearest the glass at the top are the old ones.

use crate::grid::*;
use crate::pheromones::*;
use crate::radial::{KIT_WORKERS, PlacementQueue, Stock, StockItem};
use crate::tank::TankRoot;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Cells per second. Our cells are ~1.2mm, so this is a shade under 2cm/s — about
/// right for *Lasius*, and deliberately real-time even though biology is compressed.
const WALK_SPEED: f32 = 14.0;
/// Alarm makes them faster and far less orderly.
const ALARM_SPEED_BONUS: f32 = 12.0;

const GRAVITY: f32 = 26.0;

/// Colony-days. Real workers shift tasks with age rather than being assigned them.
const NURSE_UNTIL: f32 = 10.0;
const DIGGER_UNTIL: f32 = 26.0;

/// Seconds between excavation attempts — an ant gnaws, it doesn't vaporise sand.
const DIG_INTERVAL: f32 = 0.45;
const DIG_DEPOSIT: f32 = 1.0;
/// How far ahead an ant looks for a face to bite, in cells. Must be at least one cell:
/// a tick's walk is a fraction of a cell, so probing the step target would mostly
/// re-test the cell the ant is already standing in.
const DIG_REACH: f32 = 1.2;

/// Alarm released *per second* by an ant struggling out of a collapse. Rate, not a flat
/// amount: depositing per tick at 60 Hz pinned the whole nest permanently above the
/// panic threshold, and a panicking ant refuses to dig — so the colony silently stopped
/// working and the excavation counters flatlined.
const BURIAL_ALARM_RATE: f32 = 0.5;

/// How far above the terrain an ant will roam, in cells. Enough to get out of the hole
/// and stand on the spoil mound; not enough to wander up into the empty tank.
const SURFACE_ROAM: f32 = 3.0;

/// Absolute ceiling on roaming, measured from the *original* fill line rather than the
/// current terrain. Without a fixed reference the two feed each other: spoil raises the
/// terrain, the raised terrain lets ants climb higher, and from up there they stack more
/// spoil. The colony builds a tower into the void instead of a mound.
const MOUND_HEADROOM: f32 = 12.0;

/// Sand agitation that shakes an ant off its footing. Above a tap, below a real shake —
/// so tapping alarms the colony while shaking physically throws it around.
const DISLODGE_AGITATION: f32 = 0.45;
const DISLODGE_SECONDS: f32 = 0.9;

/// Seconds of falling given to a freshly placed ant, so it drops to the sand instead
/// of hanging in the air where the pointer left it. Generous: it is cancelled the
/// moment the ant lands, so this is only ever an upper bound on the fall.
const AIRBORNE_ON_PLACEMENT: f32 = 6.0;

/// How wide an ant kit scatters across the tank as it pours in.
const KIT_SPREAD: f32 = 16.0;

/// How far a grain must be carried from where it was dug before it can be put down.
///
/// Without this the colony digs and refills the same hole forever. On flat sand the
/// working face *is* a valid dump site — an ant standing on the surface bites downward,
/// is immediately "outside" again, and drops the grain back where it came from. 845
/// excavations produced a farm with no visible tunnel in it. Carrying spoil clear of the
/// face is what real ants do, and it is the whole difference between digging and
/// shuffling.
const MIN_HAUL_DISTANCE: f32 = 10.0;

/// Seconds an ant will carry a grain before giving up and putting it down wherever it
/// happens to be. The backstop against the colony deadlocking itself in.
const HAUL_PATIENCE: f32 = 14.0;

/// How strongly each influence pulls on a digger's heading.
const W_PERSIST: f32 = 1.0;
const W_DIG_GRADIENT: f32 = 2.2;
const W_DOWNWARD: f32 = 0.9;
const W_JITTER: f32 = 0.5;
const W_QUEEN: f32 = 1.4;
const W_HOMEWARD: f32 = 2.6;

/// Alarm level above which an ant abandons what it was doing.
const ALARM_PANIC: f32 = 0.35;

/// Candidate turns, in radians, tried in order when the way ahead is blocked. Small
/// deflections first, so an ant slides along a wall instead of bouncing off it — real
/// ants follow walls, and this is where that falls out.
const DEFLECTIONS: [f32; 8] = [0.42, -0.42, 0.9, -0.9, 1.5, -1.5, 2.3, -2.3];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Job {
    /// Young. Stays in with the queen.
    Nurse,
    /// Middle-aged. Extends the nest.
    Digger,
    /// Oldest. Drifts to the surface — the dangerous job, given to the ants who are
    /// nearly done anyway.
    Surface,
}

impl Job {
    fn for_age(age_days: f32) -> Job {
        if age_days < NURSE_UNTIL {
            Job::Nurse
        } else if age_days < DIGGER_UNTIL {
            Job::Digger
        } else {
            Job::Surface
        }
    }
}

#[derive(Component)]
pub struct Ant {
    /// Continuous grid coordinates. Ants are an overlay on the sand, not cells in it.
    pub pos: Vec2,
    pub heading: Vec2,
    /// Only used while falling; walking ants move by their heading, not ballistically.
    pub vel: Vec2,
    pub age_days: f32,
    /// The shade of the grain being hauled, if any. Carrying the *shade* is what makes
    /// spoil piles show the colour of the stratum they were dug out of.
    pub carrying: Option<u8>,
    pub dig_cooldown: f32,
    /// How long this ant has been carrying its current grain.
    pub haul_time: f32,
    /// Where the grain it is carrying came from. Spoil has to be taken *away* from the
    /// working face, not put back down beside it.
    pub dug_at: Vec2,
    /// Seconds left of having been shaken off the glass. While this is running the ant
    /// is falling rather than walking.
    pub dislodged: f32,
    /// Depth within the slab. Purely visual, and only for parallax.
    pub z: f32,
}

#[derive(Component)]
pub struct Queen;

/// Counters, purely so the verification runs can say *why* the colony is behaving the
/// way it is. Guessing at emergent behaviour from an output image doesn't work.
#[derive(Resource, Default)]
pub struct ColonyStats {
    pub dug: u64,
    pub dropped_outside: u64,
    pub dropped_while_buried: u64,
    pub buried: usize,
    pub falling: usize,
    pub panicking: usize,
    pub diggers: usize,
    pub walled_in: usize,
    pub drop_failed: u64,
    pub dropped_inside: u64,
}

#[derive(Resource)]
pub struct AntAssets {
    pub worker_mesh: Handle<Mesh>,
    pub worker_mat: Handle<StandardMaterial>,
    pub queen_mat: Handle<StandardMaterial>,
    pub laden_mat: Handle<StandardMaterial>,
}

// ---------------------------------------------------------------------------
// Body
// ---------------------------------------------------------------------------

fn push_box(
    pos: &mut Vec<[f32; 3]>,
    nrm: &mut Vec<[f32; 3]>,
    idx: &mut Vec<u32>,
    centre: Vec3,
    half: Vec3,
) {
    let faces: [(Vec3, [Vec3; 4]); 6] = [
        // Each face's corners are given counter-clockwise as seen from outside.
        (
            Vec3::Z,
            [
                Vec3::new(-1.0, -1.0, 1.0),
                Vec3::new(1.0, -1.0, 1.0),
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(-1.0, 1.0, 1.0),
            ],
        ),
        (
            Vec3::NEG_Z,
            [
                Vec3::new(1.0, -1.0, -1.0),
                Vec3::new(-1.0, -1.0, -1.0),
                Vec3::new(-1.0, 1.0, -1.0),
                Vec3::new(1.0, 1.0, -1.0),
            ],
        ),
        (
            Vec3::X,
            [
                Vec3::new(1.0, -1.0, 1.0),
                Vec3::new(1.0, -1.0, -1.0),
                Vec3::new(1.0, 1.0, -1.0),
                Vec3::new(1.0, 1.0, 1.0),
            ],
        ),
        (
            Vec3::NEG_X,
            [
                Vec3::new(-1.0, -1.0, -1.0),
                Vec3::new(-1.0, -1.0, 1.0),
                Vec3::new(-1.0, 1.0, 1.0),
                Vec3::new(-1.0, 1.0, -1.0),
            ],
        ),
        (
            Vec3::Y,
            [
                Vec3::new(-1.0, 1.0, 1.0),
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(1.0, 1.0, -1.0),
                Vec3::new(-1.0, 1.0, -1.0),
            ],
        ),
        (
            Vec3::NEG_Y,
            [
                Vec3::new(-1.0, -1.0, -1.0),
                Vec3::new(1.0, -1.0, -1.0),
                Vec3::new(1.0, -1.0, 1.0),
                Vec3::new(-1.0, -1.0, 1.0),
            ],
        ),
    ];

    for (normal, corners) in faces {
        let base = pos.len() as u32;
        for c in corners {
            let v = centre + c * half;
            pos.push([v.x, v.y, v.z]);
            nrm.push([normal.x, normal.y, normal.z]);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Gaster, thorax, head along local +X. No legs — at roughly fourteen pixels long they
/// would be mush, and the silhouette is what has to read.
fn worker_mesh() -> Mesh {
    let mut pos = Vec::new();
    let mut nrm = Vec::new();
    let mut idx = Vec::new();

    push_box(&mut pos, &mut nrm, &mut idx, Vec3::new(-1.00, 0.0, 0.0), Vec3::new(0.55, 0.42, 0.40));
    push_box(&mut pos, &mut nrm, &mut idx, Vec3::new(0.05, 0.0, 0.0), Vec3::new(0.45, 0.31, 0.30));
    push_box(&mut pos, &mut nrm, &mut idx, Vec3::new(0.85, 0.0, 0.0), Vec3::new(0.35, 0.35, 0.32));

    // Normalise so the body spans exactly ANT_LENGTH along +X.
    const ANT_LENGTH_CELLS: f32 = 3.5;
    let scale = (ANT_LENGTH_CELLS * CELL) / 2.75;
    for p in &mut pos {
        p[0] *= scale;
        p[1] *= scale;
        p[2] *= scale;
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
    .with_inserted_indices(Indices::U32(idx))
}

pub fn setup_ant_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Lasius niger: not actually black up close, a very dark warm brown — and glossy.
    //
    // The gloss is doing the real work here. At roughly fifteen pixels long, a matte
    // near-black ant against the inside of a dark tank is simply not visible. Real ant
    // chitin has a hard specular sheen, so low roughness and high reflectance give each
    // one a bright highlight that reads at any size, without lying about the colour.
    let worker_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.105, 0.078, 0.065),
        perceptual_roughness: 0.20,
        reflectance: 0.58,
        ..default()
    });
    // Lifted a little, so you can pick out who's hauling.
    let laden_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.185, 0.135, 0.105),
        perceptual_roughness: 0.20,
        reflectance: 0.58,
        ..default()
    });
    // Warmer and redder, so the queen is findable at a glance.
    let queen_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.235, 0.115, 0.088),
        perceptual_roughness: 0.18,
        reflectance: 0.62,
        ..default()
    });

    commands.insert_resource(AntAssets {
        worker_mesh: meshes.add(worker_mesh()),
        worker_mat,
        queen_mat,
        laden_mat,
    });
}

// ---------------------------------------------------------------------------
// Founding
// ---------------------------------------------------------------------------


/// Ant bodies, shared by founding and by player placement.
fn worker_bundle(assets: &AntAssets, pos: Vec2, age_days: f32, seed: u32) -> impl Bundle {
    (
        Ant {
            pos,
            heading: Vec2::from_angle(hash01(seed, 13, 0xC61) * std::f32::consts::TAU),
            vel: Vec2::ZERO,
            age_days,
            carrying: None,
            dig_cooldown: hash01(seed, 19, 0xE81) * DIG_INTERVAL,
            haul_time: 0.0,
            dug_at: Vec2::ZERO,
            // Airborne until it lands. An ant grips a surface it has actually reached,
            // and one that has just been dropped in hasn't reached anything — without
            // this it hangs wherever the pointer left it.
            dislodged: AIRBORNE_ON_PLACEMENT,
            // A little depth variation between individuals, purely for parallax.
            z: SLAB_DEPTH * 0.5 - 0.012 - hash01(seed, 23, 0xF97) * 0.055,
        },
        Mesh3d(assets.worker_mesh.clone()),
        MeshMaterial3d(assets.worker_mat.clone()),
        Transform::default(),
    )
}

fn queen_bundle(assets: &AntAssets, pos: Vec2) -> impl Bundle {
    (
        Queen,
        Ant {
            pos,
            heading: Vec2::X,
            vel: Vec2::ZERO,
            age_days: 400.0,
            carrying: None,
            dig_cooldown: 0.0,
            haul_time: 0.0,
            dug_at: Vec2::ZERO,
            dislodged: AIRBORNE_ON_PLACEMENT,
            z: SLAB_DEPTH * 0.5 - 0.05,
        },
        Mesh3d(assets.worker_mesh.clone()),
        MeshMaterial3d(assets.queen_mat.clone()),
        Transform::default(),
    )
}

/// Turn queued placements into ants.
///
/// An ant can't be dropped inside packed sand, so a placement over solid ground searches
/// outward for somewhere to stand. If there's genuinely nowhere close the stock is handed
/// back rather than silently swallowed — holding over the middle of the sand shouldn't
/// cost you one of ten workers.
pub fn place_queued(
    mut queue: ResMut<PlacementQueue>,
    mut stock: ResMut<Stock>,
    mut pour: ResMut<KitPour>,
    grid: Res<SandGrid>,
    mut placed: Local<u32>,
) {
    for (item, at) in queue.0.drain(..) {
        let Some(pos) = nearest_free(&grid, at) else {
            stock.give(item);
            continue;
        };

        *placed += 1;
        let seed = *placed;

        match item {
            StockItem::AntKit => {
                // Start a pour rather than spawning eleven ants at once. Tipping a tube
                // into a farm is a stream, not a block arriving — see `pour_kit`.
                pour.remaining = 1 + KIT_WORKERS;
                pour.x = pos.x;
                pour.next_in = 0.0;
                pour.seed = seed;
            }
            // Not simulated yet — the wedges are dimmed, so this shouldn't be reachable.
            StockItem::Food | StockItem::Water => stock.give(item),
        }
    }
}

/// Nearest cell an ant can actually stand in, spiralling out from the requested spot.
fn nearest_free(grid: &SandGrid, at: Vec2) -> Option<Vec2> {
    let (ax, ay) = (at.x.floor() as isize, at.y.floor() as isize);

    for r in 0..12isize {
        for dy in -r..=r {
            for dx in -r..=r {
                // Only the ring at this radius; inner rings were covered already.
                if r > 0 && dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let (x, y) = (ax + dx, ay + dy);
                if SandGrid::in_bounds(x, y) && grid.is_air(x, y) {
                    return Some(Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Behaviour
// ---------------------------------------------------------------------------

#[inline]
fn cell_of(pos: Vec2) -> (isize, isize) {
    (pos.x.floor() as isize, pos.y.floor() as isize)
}

fn is_free(grid: &SandGrid, pos: Vec2) -> bool {
    let (x, y) = cell_of(pos);
    grid.is_air(x, y)
}

/// Is there sand within reach to hold on to?
///
/// Ants walk on the sand — any face of it, floor, wall or the roof of a tunnel — and
/// not on the glass. So a position with no solid neighbour is thin air, and an ant
/// there falls.
///
/// This did briefly allow glass-walking, because a hand-carved 18-cell-wide test
/// chamber turned into a pit that laden ants couldn't cross and digging stalled. That
/// chamber doesn't exist any more: the colony digs its own nest now, and self-dug
/// tunnels are a couple of cells wide, so every point in one is against sand anyway.
/// The pit was an artifact of the fixture, not of the rule.
fn touching_sand(grid: &SandGrid, pos: Vec2) -> bool {
    let (x, y) = cell_of(pos);
    NEIGHBOURS_8
        .iter()
        .any(|(dx, dy)| !grid.is_air(x + dx, y + dy))
}

/// Somewhere an ant can actually stand: open, against sand, and not off up into the
/// empty top of the tank.
fn can_stand(grid: &SandGrid, nav: &NavField, pos: Vec2) -> bool {
    if !is_free(grid, pos) || !touching_sand(grid, pos) {
        return false;
    }
    let (x, _) = cell_of(pos);
    let x = x.clamp(0, GRID_W as isize - 1) as usize;
    let ceiling =
        (nav.surface_at(x) as f32 + SURFACE_ROAM).min(INITIAL_SURFACE as f32 + MOUND_HEADROOM);
    pos.y <= ceiling
}

pub fn update_ants(
    time: Res<Time>,
    mut grid: ResMut<SandGrid>,
    mut ph: ResMut<Pheromones>,
    nav: Res<NavField>,
    colony_day_per_sec: Res<ColonyClock>,
    mut stats: ResMut<ColonyStats>,
    mut ants: Query<(&mut Ant, Has<Queen>)>,
) {
    let dt = time.delta_secs();
    stats.buried = 0;
    stats.falling = 0;
    stats.panicking = 0;
    stats.diggers = 0;

    for (mut ant, is_queen) in &mut ants {
        ant.age_days += dt * colony_day_per_sec.days_per_second;

        let (cx, cy) = cell_of(ant.pos);
        let (ux, uy) = (
            cx.clamp(0, GRID_W as isize - 1) as usize,
            cy.clamp(0, GRID_H as isize - 1) as usize,
        );
        let alarm = ph.get(Ph::Alarm, ux, uy);

        // --- buried ------------------------------------------------------
        // Sand fell on it. Design calls for stunned-and-digs-itself-out rather than
        // killed, so being shaken makes you a vandal, not a murderer.
        if !grid.is_air(cx, cy) {
            stats.buried += 1;
            escape_burial(&mut ant, &mut grid, &mut ph, &mut stats, ux, uy, dt);
            continue;
        }

        // --- falling ------------------------------------------------------
        // Two ways to be in the air: shaken off a surface, or simply not touching one —
        // which covers an ant that was just dropped into the tank by the player and has
        // nothing to grip yet. One rule handles placement gravity, shake-throwing and
        // the refusal to walk on glass.
        if grid.agitation_at(ux, uy) > DISLODGE_AGITATION {
            ant.dislodged = DISLODGE_SECONDS;
        }
        if ant.dislodged > 0.0 || !touching_sand(&grid, ant.pos) {
            stats.falling += 1;
            ant.dislodged -= dt;
            ant.vel.y -= GRAVITY * dt;
            let next = ant.pos + ant.vel * dt;
            if is_free(&grid, next) {
                ant.pos = next;
            } else {
                // Landed.
                ant.vel = Vec2::ZERO;
                ant.dislodged = 0.0;
                ant.pos.y = ant.pos.y.floor() + 0.5;
            }
            continue;
        }
        ant.vel = Vec2::ZERO;

        // The queen doesn't wander. She sits deep and signals that she's alive.
        if is_queen {
            ph.deposit(Ph::Queen, ux, uy, 2.0 * dt);
            continue;
        }

        let job = Job::for_age(ant.age_days);
        let panicking = alarm > ALARM_PANIC;
        if panicking {
            stats.panicking += 1;
        }
        if job == Job::Digger {
            stats.diggers += 1;
        }

        let above_ground = ant.pos.y > nav.surface_at(ux) as f32 + 0.5;
        let sealed_in = !above_ground && nav.at(cx, cy) == UNREACHABLE;

        // --- what to do next ---------------------------------------------
        // Hauling: get out, dump it, come back. Descending the navigation flood is what
        // makes spoil end up outside rather than redistributed around the nest.
        if ant.carrying.is_some() {
            ant.haul_time += dt;

            let carried_far_enough = ant.pos.distance(ant.dug_at) >= MIN_HAUL_DISTANCE;
            if carried_far_enough && nav.is_dump_site(ux, uy) {
                if drop_spoil(&mut ant, &mut grid, ux, uy) {
                    stats.dropped_outside += 1;
                } else {
                    stats.drop_failed += 1;
                }
            } else if sealed_in || ant.haul_time > HAUL_PATIENCE {
                // Give up and put it down where it stands.
                //
                // This exists because the colony can wall itself in: spoil dropped near
                // the shaft mouth plugs the entrance, the navigation flood then reports
                // no route out, and every digger is stuck holding a grain it can't
                // deliver — while nobody can dig, because digging needs empty mandibles.
                // Nothing changes the grid, so nothing ever recovers. A hard deadlock.
                //
                // Putting the grain down frees an ant to dig its way out, and it's honest
                // besides: real colonies shift sand around inside the nest constantly.
                if drop_spoil(&mut ant, &mut grid, ux, uy) {
                    stats.dropped_inside += 1;
                }
            }
        }

        // Above ground a digger may only bite *downward*.
        //
        // Banning it outright was wrong in both directions. Allowed freely, an ant that
        // has just put its grain down stands on the surface, unladen, and bites the
        // topsoil at its feet, carries it one cell and puts it back — and the Dig
        // pheromone it leaves draws others up to join in, so the colony churns the front
        // lawn in a self-reinforcing loop while the nest never grows. Banned outright, and
        // a colony tipped onto flat sand can never get underground at all, because the
        // only way down is to dig: the farm stayed pristine and every counter read zero.
        //
        // Downward-only threads between the two. A shaft can be started from the surface;
        // the lawn cannot be shuffled sideways.
        let digs_downward = !above_ground || ant.heading.y < -0.3;
        let ready_to_dig =
            ant.carrying.is_none() && job == Job::Digger && !panicking && digs_downward;
        if ready_to_dig {
            ant.dig_cooldown -= dt;
        }

        ant.heading = desired_heading(&ant, &ph, &nav, job, panicking, above_ground, ux, uy);

        let may_dig = ready_to_dig && ant.dig_cooldown <= 0.0;
        let speed = WALK_SPEED + if panicking { ALARM_SPEED_BONUS * alarm } else { 0.0 };
        step(&mut ant, &mut grid, &mut ph, &nav, &mut stats, speed * dt, may_dig);
    }
}

/// Blend of everything pulling on this ant, resolved once per tick. Persistence is
/// always in the mix, which is what keeps paths coherent rather than jittery.
fn desired_heading(
    ant: &Ant,
    ph: &Pheromones,
    nav: &NavField,
    job: Job,
    panicking: bool,
    above_ground: bool,
    ux: usize,
    uy: usize,
) -> Vec2 {
    let mut want = ant.heading * W_PERSIST;

    let jitter_seed = (ant.pos.x.abs() * 31.0) as u32 ^ (ant.pos.y.abs() * 17.0) as u32;
    let jitter = Vec2::from_angle(hash01(jitter_seed, uy as u32, 0x51D3) * std::f32::consts::TAU);

    if panicking {
        // Alarm: run, mostly away from the disturbance, and stop being useful.
        let away = -ph.gradient(Ph::Alarm, ux, uy).normalize_or_zero();
        return (ant.heading * 0.7 + away * 1.6 + jitter * 1.1).normalize_or(ant.heading);
    }

    if ant.carrying.is_some() {
        if above_ground {
            // Out with a load: walk away from the hole until there's ground to drop it on.
            //
            // Heading away rather than simply onward is what keeps the spoil apron
            // spreading outward. Committed to its own direction, an ant that surfaced
            // facing the nest walks back over the mouth and dumps on the near side, and
            // since spoil is loose it slumps in. The apron then never gets past the
            // clearance and every extra grain leaks back down the shaft.
            want += outbound(ant, nav, ux) * 2.0
                + follow_terrain(ant, nav, ux) * 1.2
                + jitter * 0.3;
        } else if let Some(out) = nav.descend(ux, uy) {
            want += out * W_HOMEWARD;
            want += jitter * (W_JITTER * 0.4);
        }
        return want.normalize_or(ant.heading);
    }

    match job {
        Job::Digger => {
            // Work attracts work. This is the rule the nest's shape comes from.
            let dig = ph.gradient(Ph::Dig, ux, uy).normalize_or_zero();
            want += dig * W_DIG_GRADIENT;
            // Lasius drive downward while founding.
            want += Vec2::NEG_Y * W_DOWNWARD;
            want += jitter * W_JITTER;
        }
        Job::Nurse => {
            // Drift up the queen's signal, which is why the colony visibly gathers
            // around her instead of being told to.
            let q = ph.gradient(Ph::Queen, ux, uy).normalize_or_zero();
            want += q * W_QUEEN;
            want += jitter * (W_JITTER * 1.5);
        }
        Job::Surface => {
            // The oldest workers head for open air, then patrol along the surface
            // rather than continuing to climb into the empty tank.
            if above_ground {
                want += lateral(ant) * 1.0 + follow_terrain(ant, nav, ux) * 0.9;
            } else if let Some(out) = nav.descend(ux, uy) {
                want += out * 1.2;
            }
            want += jitter * (W_JITTER * 1.4);
        }
    }

    want.normalize_or(ant.heading)
}

/// Whichever way along the surface the ant was already going. Keeps patrolling and
/// spoil-carrying committed to a direction instead of dithering on the spot.
#[inline]
fn lateral(ant: &Ant) -> Vec2 {
    if ant.heading.x >= 0.0 { Vec2::X } else { Vec2::NEG_X }
}

/// Away from the nest entrance, for an ant with a grain to get rid of. Standing directly
/// over the mouth there's no away, so it keeps the direction it already had.
#[inline]
fn outbound(ant: &Ant, nav: &NavField, ux: usize) -> Vec2 {
    match nav.away_from_mouth(ux) {
        Some(dir) => Vec2::new(dir, 0.0),
        None => lateral(ant),
    }
}

/// Pull toward walking *on* the terrain — upward as readily as downward.
///
/// A plain downward bias looks equivalent and isn't: haulers emerging from the shaft
/// walked into the base of their own spoil mound and couldn't climb it, because the bias
/// fought the slope. Dumping stopped dead the moment the pile got tall enough, and the
/// colony sat there holding its grains. Aiming at the terrain height instead means an ant
/// walks up over its own spoil without being told anything about slopes.
///
/// The height used is the envelope, not the column's own surface: over the mouth of a
/// shaft the column's surface is metres down, and seeking that would post haulers
/// straight back into the hole they just climbed out of.
#[inline]
fn follow_terrain(ant: &Ant, nav: &NavField, ux: usize) -> Vec2 {
    let target = nav.surface_at(ux) as f32 + 1.5;
    Vec2::new(0.0, (target - ant.pos.y).clamp(-1.0, 1.0))
}

/// Move along the heading, and when that's blocked either bite through it or slide
/// around it. Small deflections are tried before large ones, so an ant hugs a tunnel
/// wall rather than ricocheting off it.
///
/// Excavation *has* to live here, at the moment of collision. Deciding to dig earlier,
/// from the ant's heading, doesn't work: the wall-sliding above spends its time turning
/// the ant to be tangent to the sand, so by the time it looks ahead it is looking down
/// the tunnel and not at the face. Diggers slide along the work forever and the nest
/// never grows. An ant digs because it walked into something.
fn step(
    ant: &mut Ant,
    grid: &mut SandGrid,
    ph: &mut Pheromones,
    nav: &NavField,
    stats: &mut ColonyStats,
    distance: f32,
    may_dig: bool,
) {
    // A single tick's walk is a fraction of a cell, so look a whole cell ahead when
    // deciding whether there's a face to bite. Testing the sub-cell step position means
    // an ant usually just shuffles around inside the cell it's already in and only
    // notices sand on the tick it happens to cross a boundary.
    let probe = ant.pos + ant.heading * DIG_REACH;
    let target = ant.pos + ant.heading * distance;

    if may_dig {
        let (tx, ty) = cell_of(probe);
        // Never undercut the tank floor, or sand drains out of the world.
        if SandGrid::in_bounds(tx, ty)
            && ty > 0
            && grid.get(tx as usize, ty as usize).mat == Substance::Sand
        {
            let cell = grid.take(tx as usize, ty as usize);
            ant.carrying = Some(cell.shade);
            ant.haul_time = 0.0;
            ant.dug_at = ant.pos;
            ant.dig_cooldown = DIG_INTERVAL;
            stats.dug += 1;
            // Work attracts work: this is the deposit the whole nest shape grows from.
            ph.deposit(Ph::Dig, tx as usize, ty as usize, DIG_DEPOSIT);
            return;
        }
    }

    if can_stand(grid, nav, target) {
        ant.pos = target;
        return;
    }

    for turn in DEFLECTIONS {
        let h = Vec2::from_angle(ant.heading.to_angle() + turn);
        let t = ant.pos + h * distance;
        if can_stand(grid, nav, t) {
            ant.heading = h;
            ant.pos = t;
            return;
        }
    }

    // Boxed in. Turn around and try again next tick.
    stats.walled_in += 1;
    ant.heading = -ant.heading;
}

/// Put the grain down outside. The sand simulation takes it from here, which is how the
/// spoil mound around the entrance builds itself.
fn drop_spoil(ant: &mut Ant, grid: &mut SandGrid, ux: usize, uy: usize) -> bool {
    let Some(shade) = ant.carrying else {
        return false;
    };
    if grid.get(ux, uy).mat != Substance::Air {
        return false;
    }
    // Loose, because tipped-out spoil is the loosest sand in the farm. It rolls to the
    // angle of repose and the mound comes out as a cone; left packed, a stream of drops
    // on one spot builds a chimney and the colony climbs it, which is what it used to do.
    grid.set_loose(ux, uy, Cell { mat: Substance::Sand, shade });

    ant.carrying = None;
    ant.haul_time = 0.0;
    // Step off the grain it just dropped rather than standing in it.
    ant.pos.y += 1.3;
    ant.heading = Vec2::new(ant.heading.x, -0.4).normalize_or(Vec2::NEG_Y);
    true
}

/// Dig out from under a collapse. Mass has to stay conserved, so an ant only excavates
/// when its hands are free; if it's buried while already carrying it puts that grain
/// down first, and if there's nowhere to put it, it waits for the sand to shift.
fn escape_burial(
    ant: &mut Ant,
    grid: &mut SandGrid,
    ph: &mut Pheromones,
    stats: &mut ColonyStats,
    ux: usize,
    uy: usize,
    dt: f32,
) {
    // Scrambling into adjacent air is always the first move, and it keeps hold of the
    // grain. Dropping the load the instant it gets buried is what made a third of all
    // excavated sand end up back inside the nest: an ant at the working face gets
    // half-buried constantly, because taking a grain out is exactly what destabilises
    // the grains around it.
    for (dx, dy) in NEIGHBOURS_8 {
        let (nx, ny) = (ux as isize + dx, uy as isize + dy);
        if SandGrid::in_bounds(nx, ny) && grid.is_air(nx, ny) {
            ant.pos = Vec2::new(nx as f32 + 0.5, ny as f32 + 0.5);
            ant.heading = Vec2::new(dx as f32, dy as f32).normalize_or(Vec2::Y);
            return;
        }
    }

    // Properly entombed. Now the mandibles have to be freed to dig, so the grain goes
    // down in the nearest air within a few cells — never destroyed, never duplicated.
    if let Some(shade) = ant.carrying {
        let mut placed = false;
        'search: for r in 2..=4isize {
            for dy in -r..=r {
                for dx in -r..=r {
                    let (nx, ny) = (ux as isize + dx, uy as isize + dy);
                    if SandGrid::in_bounds(nx, ny) && grid.is_air(nx, ny) {
                        grid.set_loose(
                            nx as usize,
                            ny as usize,
                            Cell { mat: Substance::Sand, shade },
                        );
                        ant.carrying = None;
                        stats.dropped_while_buried += 1;
                        placed = true;
                        break 'search;
                    }
                }
            }
        }
        // No air anywhere near. Wait for the sand to shift; mass stays exact.
        if !placed {
            return;
        }
    }

    // Struggling counts as a disturbance, which is how a burial recruits help.
    ph.deposit(Ph::Alarm, ux, uy, BURIAL_ALARM_RATE * dt);

    // Claw upward — the shortest way to air. Rate-limited like any other digging, or a
    // buried ant would bore out sixty cells a second.
    ant.dig_cooldown -= dt;
    if ant.dig_cooldown > 0.0 {
        return;
    }

    let above = (uy + 1).min(GRID_H - 1);
    if grid.get(ux, above).mat == Substance::Sand {
        let cell = grid.take(ux, above);
        ant.carrying = Some(cell.shade);
        ant.dig_cooldown = DIG_INTERVAL;
        stats.dug += 1;
    }
    if grid.is_air(ux as isize, above as isize) {
        ant.pos.y = above as f32 + 0.5;
        ant.heading = Vec2::Y;
    }
}

/// Push each ant's simulated position into its transform.
pub fn sync_ant_transforms(
    assets: Res<AntAssets>,
    mut ants: Query<(&Ant, &mut Transform, &mut MeshMaterial3d<StandardMaterial>, Has<Queen>)>,
) {
    for (ant, mut tf, mut mat, is_queen) in &mut ants {
        let mut p = SandGrid::cell_to_world(0, 0);
        p.x = (ant.pos.x - GRID_W as f32 * 0.5) * CELL;
        p.y = (ant.pos.y - GRID_H as f32 * 0.5) * CELL;
        p.z = ant.z;

        tf.translation = p;
        tf.rotation = Quat::from_rotation_z(ant.heading.to_angle());
        tf.scale = if is_queen { Vec3::splat(2.3) } else { Vec3::ONE };

        if !is_queen {
            let want = if ant.carrying.is_some() {
                &assets.laden_mat
            } else {
                &assets.worker_mat
            };
            if mat.id() != want.id() {
                mat.0 = want.clone();
            }
        }
    }
}

/// How fast biology runs. Sand and ant motion stay real-time; only the colony's own
/// clock is compressed, at roughly one colony day per real hour.
#[derive(Resource)]
pub struct ColonyClock {
    pub days_per_second: f32,
}

impl Default for ColonyClock {
    fn default() -> Self {
        Self { days_per_second: 1.0 / 3600.0 }
    }
}

/// An ant kit mid-pour: how many are still to come, and where they're going in.
///
/// A kit arrives as a stream, the way tipping a tube actually looks, rather than eleven
/// ants materialising in formation. Same reasoning as pouring sand — the arrival is part
/// of what makes it read as *you* putting them in.
#[derive(Resource, Default)]
pub struct KitPour {
    pub remaining: u32,
    pub x: f32,
    pub next_in: f32,
    pub seed: u32,
}

/// Seconds between ants while a kit pours in. Eleven of them takes about two seconds.
const KIT_DROP_INTERVAL: f32 = 0.18;

/// Drip the kit in, one ant at a time, from the top of the tank.
///
/// The queen comes first and the workers follow. They fall from the open top and land
/// wherever the sand happens to be — there is no placement logic here at all, because
/// gravity and `touching_sand` already do it.
pub fn pour_kit(
    mut commands: Commands,
    time: Res<Time>,
    mut pour: ResMut<KitPour>,
    assets: Res<AntAssets>,
    tank: Query<Entity, With<TankRoot>>,
) {
    if pour.remaining == 0 {
        return;
    }
    let Ok(tank) = tank.single() else {
        return;
    };

    pour.next_in -= time.delta_secs();
    if pour.next_in > 0.0 {
        return;
    }
    pour.next_in = KIT_DROP_INTERVAL;

    let index = pour.remaining;
    let s = pour.seed.wrapping_mul(31).wrapping_add(index);
    let spread = (hash01(s, 7, 0xA47) - 0.5) * KIT_SPREAD;
    let at = Vec2::new(
        (pour.x + spread).clamp(1.0, GRID_W as f32 - 2.0),
        (GRID_H - 2) as f32,
    );

    // The queen leads, so the first thing in is the one that matters.
    if pour.remaining == 1 + KIT_WORKERS {
        commands.spawn((queen_bundle(&assets, at), ChildOf(tank)));
    } else {
        // Spread ages so labour divides itself immediately: some nurses, some diggers,
        // some at the surface. Real demography is M3.
        let age = 2.0 + hash01(s, 17, 0xD73) * 28.0;
        commands.spawn((worker_bundle(&assets, at, age, s), ChildOf(tank)));
    }

    pour.remaining -= 1;
}
