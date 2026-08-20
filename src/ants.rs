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
/// right for *Lasius*. Real-time, like everything else in the farm now is.
const WALK_SPEED: f32 = 14.0;
/// Alarm makes them faster and far less orderly.
const ALARM_SPEED_BONUS: f32 = 12.0;

const GRAVITY: f32 = 26.0;

/// Colony-days. Real workers shift tasks with age rather than being assigned them.
const NURSE_UNTIL: f64 = 10.0;
const DIGGER_UNTIL: f64 = 26.0;

/// Seconds a digger waits between bites, **at real time**.
///
/// Eight and a third hours, which looks absurd and is the number the clock demands. The arithmetic,
/// because a constant this strange has to show its working: our cells are 1.2mm, so 1.44mm² each,
/// and a mature *Lasius* nest in a formicarium is galleries on the order of 100cm² seen side-on —
/// about 7,000 cells, excavated over a couple of months. That is 115 cells a day for the whole
/// colony. Split across the ~40 diggers in a colony of a hundred, one digger accounts for under
/// three cells a day, and one bite every 30,000 seconds is what that means.
///
/// It replaces 0.45 seconds, which was set when a colony day took an hour and was 1,300× too fast
/// for a day that takes a day. That mismatch was not a detail: it meant the colony would relocate
/// the entire tank in an afternoon, and it is what made every measurement of the crowding brake a
/// lie — biology ran 86,400× while labour ran once, so the founding workers died of old age inside
/// thirty-five seconds while a bite still took half of one.
///
/// **The fast-forward scales this**, by exactly the factor it scales biology by — see
/// [`ColonyClock::labour_scale`]. At a colony day a second the effective interval is a third of a
/// second, which is roughly what the game did before; at real time a colony excavates about five
/// cells an hour, and the farm is a thing you notice over weeks. That is the trade Brett chose,
/// and the reason it is the right one is the shake: a rebuild measured in days is a cost you watch
/// being paid, which is what gives "please don't shake" its teeth.
pub(crate) const DIG_INTERVAL: f32 = 30_000.0;

/// Seconds between the bites of an ant clawing out of a collapse, and *not* scaled by labour.
///
/// Construction is colony business on a colony's timescale; getting out from under a cave-in is an
/// animal in trouble, and it happens at the speed an ant actually moves. Sharing one constant
/// would mean a shaken colony took eight hours a cell to dig itself free — every ant buried by a
/// shake would simply be dead, and the farm would never recover from the one verb the game is
/// about.
const ESCAPE_INTERVAL: f32 = 0.45;
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
pub const MOUND_HEADROOM: f32 = 12.0;

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

/// Crowding laid down per second by one ant, and by one brood item.
///
/// Brood counts for more than a worker and that is the point of the number: a pile of eggs is a
/// standing demand for room that cannot walk somewhere else, and a chamber is what a colony digs
/// around one. A worker's own demand is transient — it is about to walk off.
const CROWD_PER_ANT: f32 = 1.0;
pub const CROWD_PER_BROOD: f32 = 1.6;

/// Crowding an ant has to feel before it will bite sand.
///
/// This is the rule that makes excavation answer to the colony instead of running forever, and
/// three separate faults were all this one rule missing. Digging used to be unconditional — an
/// ant dug because it walked into a face — so spoil was produced at a rate set by how many
/// diggers existed rather than by whether the nest needed to be bigger. That gave us: 45% of
/// everything dug falling back down the hole, because a fixed-size dump target was being fought
/// over by a colony that had grown fivefold; a farm that would relocate its entire tank in about
/// two hours of real time; and a nest that was one long shaft, because nothing ever dug *where
/// the colony needed room* as opposed to wherever a digger's nose happened to be.
///
/// Eight, chosen from measured readings rather than taste. `Ph::Crowd` is bodies per unit of open
/// space — it cannot cross sand, so it piles up in a cramped place and spreads thin in a roomy
/// one — and on the capture run ants felt a median of 5.7, a 75th percentile of 9.4 and a 90th of
/// 20, with the queen's own chamber at 9–25 and a nest crushed by a shake reading 25 at her. So a
/// roomy gallery sits below this and a packed chamber sits above it, which is the line wanted.
///
/// The equilibrium matters more than the threshold: as a nest grows, crowding falls, so digging
/// stops on its own. Raising this number makes for a tighter colony; lowering it, a more
/// excavated one. It does not need a cap on top, and `MOUND_HEADROOM` can probably go once this
/// has been watched for a few hours.
const DIG_DEMAND: f32 = 8.0;

/// Alarm level above which an ant abandons what it was doing.
const ALARM_PANIC: f32 = 0.35;

/// How far to the side the founding queen's spoil lands, in columns. Clear of a one-cell shaft
/// with room to spare, so it cannot roll back in.
const QUEEN_SPOIL_OFFSET: isize = 4;

/// Seconds between the founding queen's own bites. She is slow, and she only digs at all until
/// she is in — see `settle_the_queen`.
const QUEEN_DIG_INTERVAL: f32 = 28_800.0;

/// The queen's pace, walking in and then pottering about once she is in. She is the largest
/// thing in the tank and she is not going anywhere in a hurry.
const QUEEN_WALK_SPEED: f32 = 4.0;
const QUEEN_SHUFFLE_SPEED: f32 = 1.1;

/// Ticks between the queen choosing a new direction to shuffle in — about three seconds.
const QUEEN_TURN_TICKS: u32 = 180;

/// How far the queen will stray from the deepest point she can reach, in flood steps.
///
/// Zero would freeze her: at a local maximum every neighbour is strictly shallower, so a rule
/// that only permitted equal-or-deeper steps would permit nothing at all, and she would be a
/// statue again by a different route. A few steps of slack gives her a chamber to potter in
/// without letting her wander back out of it.
const QUEEN_LEEWAY: u16 = 3;

/// Ticks a wander direction is held for. At 60 Hz this is a bit under half a second.
///
/// The number is a compromise between two ways of being wrong, and both were live at once.
/// Re-rolled every tick, the noise becomes a random walk in heading: paths stop being
/// coherent, and a digger that re-randomises hollows out a sphere instead of driving a shaft.
/// Held forever — which is what keying it on *position alone* amounted to — the entire
/// movement rule becomes a time-invariant function of position, and a time-invariant rule has
/// fixed points. Ants found them: an ant at A wanted B, at B wanted A, and paced that
/// two-cycle for the rest of its life. It never counted as stuck, because it moved every
/// single tick.
///
/// Measured, at a hundred ants: 44 of them going nowhere over four seconds, and excavation
/// frozen at 98 cells with 46 diggers on the books — a digger only bites what it walks into,
/// and a pacing digger walks into nothing. A tap unfroze the colony for a few seconds because
/// alarm is the one input that changes with time.
const WANDER_TICKS: u32 = 24;

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
    pub fn for_age(age_days: f64) -> Job {
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
    /// Colony-days since eclosion. `f64`, and it has to be: a real-time clock adds about
    /// two ten-millionths of a day per tick, which is smaller than a single-precision step
    /// at any age past four days. See [`ColonyClock`].
    pub age_days: f64,
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
    /// The rest of the job mix. Without these, "the ants aren't digging" cannot be told apart
    /// from "those particular ants are nurses", and the two want opposite fixes.
    pub nurses: usize,
    pub surface: usize,
    /// Boxed in *this tick* — every direction refused. A handful is normal in a tight
    /// tunnel; a steady count is the signature of ants stuck on the spot.
    pub walled_in: usize,
    /// Standing within a cell of the side glass. Should be ~0: an ant cannot hold glass, so
    /// anything living there is being propped up by a bug.
    pub at_the_glass: usize,
    pub drop_failed: u64,
    pub dropped_inside: u64,
    /// Where outside drops land, bucketed by how many columns they are from the nest mouth:
    /// `0-6, 7-9, 10-14, 15-24, 25+`.
    ///
    /// This is here to test one hypothesis. `is_dump_site` accepts any column at least
    /// `MOUND_CLEARANCE` from the mouth, and a hauler walks *away* from the mouth to find one —
    /// so it qualifies the instant it crosses that radius and drops there. If that is what
    /// happens, every grain in the farm lands on the same ring, the ring grows into a ridge, and
    /// the inner face of that ridge slopes back down toward the shaft at the angle of repose,
    /// which is a machine for feeding spoil back into the hole it came out of. The histogram
    /// says whether drops pile on one radius or spread across the apron.
    pub drops_by_clearance: [u64; 5],
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
fn worker_bundle(assets: &AntAssets, pos: Vec2, age_days: f64, seed: u32) -> impl Bundle {
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

/// An ant put back exactly as it was found, for [`crate::save`].
///
/// Founding builds its ants from a seed; this one takes the whole `Ant` as given, because
/// a restored colony has to come back at the ages and positions it was left at rather than
/// at plausible ones. The `Queen` marker goes on separately — a bundle can't be two types.
pub fn body_bundle(assets: &AntAssets, ant: Ant, queen: bool) -> impl Bundle {
    let material = if queen {
        assets.queen_mat.clone()
    } else if ant.carrying.is_some() {
        assets.laden_mat.clone()
    } else {
        assets.worker_mat.clone()
    };
    (
        ant,
        Mesh3d(assets.worker_mesh.clone()),
        MeshMaterial3d(material),
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
                // A pour already running is not restarted. Assigning `remaining` over the top of
                // one resets it to eleven mid-tip, which both tips in more ants than a kit holds
                // and takes `remaining` back through the value that means "spawn the queen".
                if pour.remaining > 0 {
                    warn!("a kit was tipped in while one was still pouring; ignored");
                    continue;
                }
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
pub fn cell_of(pos: Vec2) -> (isize, isize) {
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
    // In bounds, deliberately. `is_air` answers *for the sand*, and for the sand the walls
    // and the floor are solid — that is how a heap rests against the glass instead of
    // pouring through it. For an ant they are glass, and glass is the one surface it cannot
    // hold.
    //
    // Without the bounds check an ant against the left wall has a neighbour at x = -1 that
    // reports "not air", so the glass counts as footing: ants walked to the edge of the tank
    // and stood there indefinitely, held up by the window. The module already claimed to
    // refuse glass; this is the hole it was refusing it through.
    NEIGHBOURS_8.iter().any(|(dx, dy)| {
        let (nx, ny) = (x + dx, y + dy);
        SandGrid::in_bounds(nx, ny) && !grid.is_air(nx, ny)
    })
}

/// Somewhere an ant can actually stand: open, against sand, and not off up into the
/// empty top of the tank.
fn can_stand(grid: &SandGrid, nav: &NavField, pos: Vec2, from_y: f32) -> bool {
    if !is_free(grid, pos) || !touching_sand(grid, pos) {
        return false;
    }
    let (x, _) = cell_of(pos);
    let x = x.clamp(0, GRID_W as isize - 1) as usize;
    let ceiling =
        (nav.surface_at(x) as f32 + SURFACE_ROAM).min(INITIAL_SURFACE as f32 + MOUND_HEADROOM);

    // Above the ceiling, anything that isn't a *climb* is allowed — level included, and the
    // "level" is the part that matters. The ceiling limits how high an ant will climb; read
    // as a limit on where it may be, it strands whatever is already up there.
    //
    // Strictly-lower was not enough, and this is why: a tick's walk is about a fifth of a
    // cell, so a step along flat ground leaves `pos.y` equal to where it started, not below
    // it. An ant standing on ground above the cap therefore had every sideways step refused
    // as well, and the only ones left pointed *into* the sand it was standing on. It could
    // not move at all.
    pos.y <= ceiling || pos.y <= from_y
}

pub fn update_ants(
    time: Res<Time>,
    clock: Res<ColonyClock>,
    mut grid: ResMut<SandGrid>,
    mut ph: ResMut<Pheromones>,
    nav: Res<NavField>,
    mut stats: ResMut<ColonyStats>,
    mut ants: Query<(&mut Ant, Has<Queen>)>,
) {
    let dt = time.delta_secs();
    let labour = clock.labour_scale();
    // Read once, before the grid is borrowed mutably for digging.
    let tick = grid.tick;
    stats.buried = 0;
    stats.falling = 0;
    stats.panicking = 0;
    stats.diggers = 0;
    stats.nurses = 0;
    stats.surface = 0;
    // Per tick, both of them, so they read as "how many right now" rather than a total that
    // only ever grows.
    stats.walled_in = 0;
    stats.at_the_glass = 0;

    for (mut ant, is_queen) in &mut ants {
        let (cx, cy) = cell_of(ant.pos);
        let (ux, uy) = (
            cx.clamp(0, GRID_W as isize - 1) as usize,
            cy.clamp(0, GRID_H as isize - 1) as usize,
        );
        let alarm = ph.get(Ph::Alarm, ux, uy);
        // Every body is a demand for room. Laid down before anything is decided, so the field an
        // ant reads includes itself — a lone ant in a wide gallery is not crowded, and one of
        // twenty in a pocket is.
        ph.deposit(Ph::Crowd, ux, uy, CROWD_PER_ANT * dt);
        if ant.pos.x < 1.5 || ant.pos.x > GRID_W as f32 - 1.5 {
            stats.at_the_glass += 1;
        }

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

        // The queen walks in, then stays in. She is also the only ant that never digs.
        if is_queen {
            ph.deposit(Ph::Queen, ux, uy, 2.0 * dt);
            settle_the_queen(&mut ant, &mut grid, &nav, tick, dt, labour);
            continue;
        }

        let job = Job::for_age(ant.age_days);
        let panicking = alarm > ALARM_PANIC;
        if panicking {
            stats.panicking += 1;
        }
        match job {
            Job::Digger => stats.diggers += 1,
            Job::Nurse => stats.nurses += 1,
            Job::Surface => stats.surface += 1,
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
                    let clearance = nav.mouth_clearance(ux);
                    let bucket = match clearance {
                        0..=6 => 0,
                        7..=9 => 1,
                        10..=14 => 2,
                        15..=24 => 3,
                        _ => 4,
                    };
                    stats.drops_by_clearance[bucket] += 1;
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
        let ready_to_dig = wants_to_dig(
            job,
            ph.get(Ph::Crowd, ux, uy),
            above_ground,
            ant.carrying.is_some(),
            panicking,
            digs_downward,
        );
        // The wait between bites runs for any ant whose job digs, not only for one currently
        // standing in a digging posture, and that distinction paralysed the colony.
        //
        // It used to decrement only while `ready_to_dig`, which reads as "time spent trying".
        // With a bite every 0.45s that was invisible. With a bite every 30,000 seconds it is a
        // trap: a digger turns to face the sand, cannot walk because every step is into solid
        // ground, cannot dig because the cooldown has not elapsed — and the cooldown only
        // elapses while it keeps facing the sand. So it stands there pressing its face into the
        // floor for ten real minutes. Measured at 110 ants: 92 of them stuck for 12s or more,
        // 66 of those diggers, and the count climbing exactly as fast as ants adopted the pose.
        //
        // It is elapsed time since the last bite, so it passes while the ant does something
        // else. Which is also what real ants do — a colony's diggers are not queued at the face.
        if job != Job::Surface && !panicking && ant.carrying.is_none() {
            // Labour runs on the colony's clock, not the wall's. See `DIG_INTERVAL`.
            ant.dig_cooldown -= dt * labour;
        }

        // Hungry to dig, rather than merely allowed to. Only an ant whose wait has elapsed
        // adopts the downward, face-seeking heading; the rest walk the nest. This is what keeps
        // the two rates coherent — the posture now costs a bite, not a shift.
        let hungry = ant.dig_cooldown <= 0.0;
        ant.heading = desired_heading(
            &ant, &ph, &nav, job, panicking, above_ground, hungry, ux, uy, tick,
        );

        let may_dig = ready_to_dig && ant.dig_cooldown <= 0.0;
        let speed = WALK_SPEED + if panicking { ALARM_SPEED_BONUS * alarm } else { 0.0 };
        step(&mut ant, &mut grid, &mut ph, &nav, &mut stats, speed * dt, may_dig);
    }
}

/// Whether this ant will bite sand at all.
///
/// Its own function so the rule can be tested without a world.
///
/// **The crowding brake is built and parked, one line from here.** `Ph::Crowd` exists, every ant
/// and every brood item lays it down, and it measures exactly what it should: the field cannot
/// cross sand, so it piles up in a cramped chamber and spreads thin in a roomy one. Swapping the
/// last line for `crowd >= DIG_DEMAND` underground is the whole change.
///
/// It is parked because it cannot be *measured* yet, and Brett found the reason: the capture
/// compresses biology 86,400 times and leaves labour at real time. A rule that needs work to
/// happen before biology advances is therefore guaranteed to fail there — the founding workers
/// die of old age in thirty-five seconds while digging still takes 0.45s a bite. Three runs said
/// the brake killed the colony; all three were the instrument, not the rule. Fixing that means
/// deciding how fast a colony should dig on a clock where a day is a day, which is a design
/// question and not mine to answer alone. See NOTES.md.
fn wants_to_dig(
    job: Job,
    _crowd: f32,
    _above_ground: bool,
    carrying: bool,
    panicking: bool,
    digs_downward: bool,
) -> bool {
    !carrying && !panicking && digs_downward && job == Job::Digger
}

/// The queen goes down the shaft and stays there.
///
/// She had no movement code at all. The comment beside the branch said "she sits deep" and
/// nothing ever got her deep: she was poured out of the tube, fell onto the sand, and spent the
/// colony's whole life on the surface. That is not only wrong to look at — she *is* the `Queen`
/// pheromone source, so the nurses gathered on top of the sand and `lay_eggs` put the brood pile
/// out in the open, where the first shake scatters it. The nest had its centre outside itself.
///
/// Two states and no plan. If anything next to her is deeper, walk that way, favouring down.
/// If nothing is, potter within a few steps of where she is. She never digs, so she can only
/// occupy what the workers have already opened — which means the queen going deep is a thing
/// the colony achieves for her rather than a scripted move, and on a farm nobody has dug yet
/// she simply waits on the surface, correctly.
fn settle_the_queen(
    ant: &mut Ant,
    grid: &mut SandGrid,
    nav: &NavField,
    tick: u32,
    dt: f32,
    labour: f32,
) {
    let (cx, cy) = cell_of(ant.pos);
    let (ux, uy) = (
        cx.clamp(0, GRID_W as isize - 1) as usize,
        cy.clamp(0, GRID_H as isize - 1) as usize,
    );
    let here = nav.at(cx, cy);

    // Founding: she cuts her own shaft, and pushes the spoil out of the top of it.
    //
    // Two earlier versions of this are worth keeping written down, because both looked right.
    //
    // Leaving it to the workers starved the colony. `lay_eggs` will not lay until she is
    // `FOUNDING_DEPTH` under the ground, and at the capture's day-a-second the founding workers
    // die of old age inside thirty-five *seconds* while digging stays real-time — so they get
    // thirty-five seconds of labour where the real game gives them thirty-five days. The farm
    // went 10 ants -> 3 -> 1 with a six-cell scrape and no eggs, twice.
    //
    // Letting her backfill behind her deadlocked it instead. The grain went into the cell she had
    // just left, the seal fell out for free rather than being scripted, and it was lovely — but a
    // sealed chamber has no route out for spoil, so every grain dug inside was dropped back where
    // it was found: 84 ants, a nest of two cells, 96% of everything dug going straight home.
    //
    // So the grain goes out and *beside*, through the same `settle` that puts a flying grain back
    // into the grid. It is an abstraction and worth naming as one — she is not carrying it up —
    // but it is the honest shape of the real behaviour: a founding queen pushes her diggings out
    // of the entrance, and the little ring of spoil around a fresh ant hole is exactly what that
    // looks like.
    //
    // Beside, and not up her own column, which is the version that shipped for one measurement.
    // `settle` drops the grain down the first air it finds, and the first air above a queen who is
    // digging a shaft is *the shaft*: every grain fell straight back down the hole, refilled it
    // behind her, and left the workers re-digging the same cells forever — `dug 1284 -> excavated
    // 53`. Alternating sides is what makes it a ring rather than a bank on one flank.
    ant.dig_cooldown -= dt * labour;
    let ground = nav.surface_at(ux) as f32;
    let founding = ant.pos.y + crate::brood::FOUNDING_DEPTH > ground;
    if founding && ant.dig_cooldown <= 0.0 {
        let below = cy - 1;
        if below > 0 && !grid.is_air(cx, below) {
            let side = if grid.tick % 2 == 0 { 1 } else { -1 };
            let out = (cx + side * QUEEN_SPOIL_OFFSET).clamp(1, GRID_W as isize - 2);
            // Onto the ground beside the hole, not the roof of the tank.
            //
            // This passed `GRID_H - 2` and that was a bug worth remembering the shape of:
            // `settle` searches *upward* from the row it is given, so the grain materialised in
            // the top two rows and fell the whole height of the tank — and if those rows were
            // ever full, which is exactly what happens to a farm whose spoil reaches the top,
            // `settle` found nowhere and the grain ceased to exist. A mass leak, in the one
            // invariant this game promises never to leak, reachable by playing well.
            //
            // One row above the terrain envelope is air by construction, since the envelope is
            // the highest ground within seven columns either side.
            let onto = nav.surface_at(out.clamp(0, GRID_W as isize - 1) as usize) as isize + 1;
            let grain = grid.take(ux, below as usize);
            if crate::grains::settle(grid, out, onto, grain.shade) {
                ant.pos.y = below as f32 + 0.5;
                ant.dig_cooldown = QUEEN_DIG_INTERVAL;
            } else {
                // Nowhere to put it, so she did not dig after all. Putting it straight back is
                // the only version of this that keeps the mass exact.
                grid.set_raw(ux, below as usize, grain);
                warn!("the queen had nowhere to put her diggings");
            }
            return;
        }
    }

    // Toward the entrance while she is still outside, because `deepen` cannot help her there.
    //
    // The flood is distance from open sky, so *every* cell of open air above the sand is zero and
    // nothing next to her is deeper: from the surface the inward step has no opinion at all. She
    // was left shuffling at random until she happened to tread on the hole, which she managed
    // once by luck — the kit had dropped her on the mouth — and the run after that she sat at
    // ground level for the whole colony while the panel read `0 cells down, needs 6 to lay` and
    // the brood stayed at nought. A queen who cannot find the door never founds.
    //
    // `nearest_mouth` already knows where the door is: haulers read it to walk *away* from it, so
    // she reads the same thing and walks the other way.
    let toward_mouth = || {
        nav.away_from_mouth(ux)
            .map(|away| Vec2::new(-away, -0.35).normalize())
    };

    let (speed, want) = match nav.deepen(ux, uy).or_else(toward_mouth) {
        // Downward as well as inward: a plain flood-climb will happily follow a side pocket
        // that happens to be a step deeper, and a founding *Lasius* queen goes down.
        Some(inward) => (
            QUEEN_WALK_SPEED,
            (inward + Vec2::NEG_Y * 0.6).normalize_or(inward),
        ),
        None => (
            QUEEN_SHUFFLE_SPEED,
            Vec2::from_angle(
                hash01(ux as u32, uy as u32, 0x9D11 ^ tick / QUEEN_TURN_TICKS)
                    * std::f32::consts::TAU,
            ),
        ),
    };

    ant.heading = want;
    let target = ant.pos + want * speed * dt;
    let (tx, ty) = cell_of(target);
    let there = nav.at(tx, ty);

    // Deeper is always allowed; shallower only within the leeway. `UNREACHABLE` means she is
    // standing in sand that just fell on her, and `can_stand` is what handles that.
    let stays_in = there == UNREACHABLE || there + QUEEN_LEEWAY >= here;
    if stays_in && can_stand(grid, nav, target, ant.pos.y) {
        ant.pos = target;
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
    hungry: bool,
    ux: usize,
    uy: usize,
    tick: u32,
) -> Vec2 {
    let mut want = ant.heading * W_PERSIST;

    // Where the ant is *and* roughly when. Position alone is a trap — see `WANDER_TICKS`.
    let jitter_seed = (ant.pos.x.abs() * 31.0) as u32 ^ (ant.pos.y.abs() * 17.0) as u32;
    let jitter = Vec2::from_angle(
        hash01(jitter_seed, uy as u32, 0x51D3 ^ tick / WANDER_TICKS) * std::f32::consts::TAU,
    );

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
        Job::Digger if hungry => {
            // Work attracts work. This is the rule the nest's shape comes from.
            let dig = ph.gradient(Ph::Dig, ux, uy).normalize_or_zero();
            want += dig * W_DIG_GRADIENT;
            // Lasius drive downward while founding.
            want += Vec2::NEG_Y * W_DOWNWARD;
            want += jitter * W_JITTER;
        }
        Job::Digger => {
            // Between bites, and that is nearly all of the time now: walk the nest instead of
            // leaning on it. No downward bias, because pointing into sand it cannot bite yet is
            // the whole of how a digger gets stuck. Persistence and jitter follow the tunnels on
            // their own — the deflection ladder in `step` makes an ant hug a wall rather than
            // bounce off it, so "wander" underground *is* "patrol the galleries".
            want += jitter * (W_JITTER * 1.3);
            if above_ground {
                want += follow_terrain(ant, nav, ux) * 1.1;
            }
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

    let from_y = ant.pos.y;
    if can_stand(grid, nav, target, from_y) {
        ant.pos = target;
        return;
    }

    for turn in DEFLECTIONS {
        let h = Vec2::from_angle(ant.heading.to_angle() + turn);
        let t = ant.pos + h * distance;
        if can_stand(grid, nav, t, from_y) {
            ant.heading = h;
            ant.pos = t;
            return;
        }
    }

    // Boxed in. Turn away and try again next tick.
    //
    // Keyed on the **tick**, and that is the whole subtlety. Reversing exactly flips an ant
    // whose reverse is also blocked straight back, and it vibrates on the spot. Reversing
    // plus an offset taken from its *position* looks like a fix and isn't: a stuck ant's
    // position doesn't change, so the offset doesn't either, and the heading precesses by
    // the same angle every tick — a stuck ant spins on the spot instead, which is worse
    // because it looks deliberate. Brett watched them do it.
    //
    // From the tick, the turn is different every frame, so a boxed-in ant sweeps for a way
    // out instead of orbiting.
    stats.walled_in += 1;
    let sweep = hash01(ant.pos.x.abs() as u32, ant.pos.y.abs() as u32, grid.tick)
        * std::f32::consts::TAU;
    ant.heading = Vec2::from_angle(sweep);
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

    // Claw upward — the shortest way to air. Rate-limited, or a buried ant would bore out sixty
    // cells a second — but at `ESCAPE_INTERVAL` and *unscaled*, because this is survival rather
    // than construction.
    ant.dig_cooldown -= dt;
    if ant.dig_cooldown > 0.0 {
        return;
    }

    let above = (uy + 1).min(GRID_H - 1);
    if grid.get(ux, above).mat == Substance::Sand {
        let cell = grid.take(ux, above);
        ant.carrying = Some(cell.shade);
        ant.dig_cooldown = ESCAPE_INTERVAL;
        stats.dug += 1;
    }
    if grid.is_air(ux as isize, above as isize) {
        ant.pos.y = above as f32 + 0.5;
        ant.heading = Vec2::Y;
    }
}

/// Push each ant's simulated position into its transform.
/// How fast a body turns to face where it is going, in radians per second of catching up.
///
/// The heading is a decision, remade every tick out of a blend of influences, and it is
/// perfectly reasonable for it to jump: an ant sliding along a slope tries one deflection then
/// another, and a boxed-in one sweeps for a way out. Drawing that raw put the body wherever
/// the last decision pointed, so an ant climbing a 45-degree pile appeared to flip over and
/// over. What it faces is a *rendering* of that decision, and it catches up.
const TURN_RATE: f32 = 11.0;

pub fn sync_ant_transforms(
    time: Res<Time>,
    assets: Res<AntAssets>,
    mut ants: Query<(&Ant, &mut Transform, &mut MeshMaterial3d<StandardMaterial>, Has<Queen>)>,
) {
    let catch_up = 1.0 - (-TURN_RATE * time.delta_secs().max(1.0 / 240.0)).exp();
    for (ant, mut tf, mut mat, is_queen) in &mut ants {
        let mut p = SandGrid::cell_to_world(0, 0);
        p.x = (ant.pos.x - GRID_W as f32 * 0.5) * CELL;
        p.y = (ant.pos.y - GRID_H as f32 * 0.5) * CELL;
        p.z = ant.z;

        tf.translation = p;
        // Toward the heading, never straight onto it. Slerp takes the short way round, so a
        // reversal turns through one side rather than spinning.
        let facing = Quat::from_rotation_z(ant.heading.to_angle());
        tf.rotation = tf.rotation.slerp(facing, catch_up);
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

/// How fast biology runs. Sand and ant motion have always been real-time; this is the rate
/// the colony's own clock — ageing, laying, eclosion, death — ticks at.
///
/// `f64`, and not as a matter of taste. At a day per day this adds `1.9e-7` days per tick,
/// and a single-precision float carrying an age of four days cannot represent a step that
/// small: `age += 1.9e-7` rounds straight back to `age` and the ant stops ageing, silently
/// and forever. Measured, before this was `f64`: ages ran 24% fast between one and four days
/// and then froze dead at four, so no worker ever reached `NURSE_UNTIL`, nothing ever dug,
/// and nobody ever died. The clock said real time and the colony was a still photograph.
///
/// Anything accumulating colony-days is `f64` for the same reason. A rate this slow is only
/// meaningful if the sum can hold it.
#[derive(Resource)]
pub struct ColonyClock {
    pub days_per_second: f64,
    /// Colony-days since this process started. Not saved: it is "how long have I been watching",
    /// which is the question a long run is asking, and a farm's own age would be a different
    /// number wanting a different home.
    pub elapsed_days: f64,
}

/// How much colony time this step covers, in days. Everything biological reads this and
/// nothing biological reads [`Time`] directly.
///
/// The indirection buys one thing, and it is the reason it exists: [`crate::away`] can put a
/// day in here and run the same laying, ageing and eclosion the live game runs, over and over,
/// to settle up the time the app was closed for. A system that took its step from `Time`
/// instead would be a system the catch-up could not drive, and the rules would have to exist
/// twice — once for playing and once for having been away. They would then drift.
#[derive(Resource, Default)]
pub struct ColonyStep(pub f64);

impl ColonyClock {
    /// How many times faster than real time the colony is running.
    ///
    /// One at real time, 86,400 at a colony day a second. Everything the colony *does* is
    /// multiplied by this, so a fast-forward compresses work and biology by the same factor
    /// instead of only biology — which is what made the harness a liar.
    ///
    /// Locomotion is the exception, and it cannot be otherwise: an ant walking 86,400× would cross
    /// the tank in a tick, and the sand automaton it walks on runs at 60 Hz. So a fast-forward is
    /// faithful about *how much* a colony digs and never about *where* — quantities can be trusted
    /// at any speed, nest shape only at low multipliers over long runs.
    pub fn labour_scale(&self) -> f32 {
        (self.days_per_second * 86_400.0) as f32
    }
}

/// One tick of colony time. First in the fixed schedule, so everything after it agrees.
pub fn advance_colony_clock(
    time: Res<Time>,
    mut clock: ResMut<ColonyClock>,
    mut step: ResMut<ColonyStep>,
) {
    step.0 = time.delta_secs() as f64 * clock.days_per_second;
    clock.elapsed_days += step.0;
}

/// Everybody gets older.
///
/// Its own system rather than a line inside `update_ants`, so a colony can be aged without
/// being moved — nobody walks or digs while the app is shut.
pub fn age_ants(step: Res<ColonyStep>, mut ants: Query<&mut Ant>) {
    for mut ant in &mut ants {
        ant.age_days += step.0;
    }
}

impl Default for ColonyClock {
    fn default() -> Self {
        // Real time. A colony day takes a day.
        //
        // This was an hour per day — biology compressed twenty-four times while the sand and
        // the ants stayed real. Brett asked for the clock to run in real time, so it does:
        // nothing in the farm now moves faster than it would in a tank on a shelf.
        //
        // What that costs, stated plainly because it changes the shape of the game. Egg to
        // worker is six *days* rather than six hours, so a first cohort arrives about a week
        // in and the queen's decline is a couple of months out. It is an ambient game you
        // leave running, and now it is one on the scale of an actual ant farm — you will not
        // see a life stage in a sitting, only that the pile is bigger than it was.
        //
        // The scripted runs override this; see `CAPTURE_DAYS_PER_SECOND`. It is the only way a
        // two-minute test can say anything about a six-day life stage.
        Self { days_per_second: 1.0 / 86_400.0, elapsed_days: 0.0 }
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
    existing_queens: Query<(), With<Queen>>,
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

    // The queen leads, so the first thing in is the one that matters — unless the farm already
    // has one, in which case this kit contributes a worker instead.
    //
    // One queen per farm is a locked design decision, and this is the only place a queen can be
    // made, so this is where it belongs. It is not a hypothetical: a second queen makes
    // `Query::single` fail, and `lay_eggs` and `tend_brood` both used to give up on that — the
    // colony stopped laying and stopped tending, silently, forever. That cost weeks of wrong
    // numbers. Those two now take the first queen instead of demanding exactly one, and this
    // stops the second from existing at all. Both halves, because the failure was silent.
    if pour.remaining == 1 + KIT_WORKERS && existing_queens.is_empty() {
        commands.spawn((queen_bundle(&assets, at), ChildOf(tank)));
    } else {
        // Spread ages so labour divides itself immediately: some nurses, some diggers,
        // some at the surface. Real demography is M3.
        let age = (2.0 + hash01(s, 17, 0xD73) * 28.0) as f64;
        commands.spawn((worker_bundle(&assets, at, age, s), ChildOf(tank)));
    }

    pour.remaining -= 1;
}

/// Dummy handles, for tests that need a colony but not a renderer. Nothing looks at what
/// they point to — which is the useful part: the save path and the away catch-up can spawn
/// real ants in a headless `App`.
#[cfg(test)]
pub(crate) fn stub_assets() -> AntAssets {
    AntAssets {
        worker_mesh: Handle::default(),
        worker_mat: Handle::default(),
        queen_mat: Handle::default(),
        laden_mat: Handle::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixed timestep, from `SIM_HZ` in main.rs. The colony is aged from `Time`'s delta
    /// inside `FixedUpdate`, so this is the size of the step the sum has to survive.
    const DT: f32 = 1.0 / 60.0;

    /// A day of real time has to move a worker's age by a day — at *any* age.
    ///
    /// This is the whole real-time clock in one test. The rate is `1/86400` days per second,
    /// which is `1.9e-7` days a tick, and the failure it guards is not an off-by-a-bit: with
    /// `age_days` as `f32` this loop moved a thirty-day-old ant by exactly zero, because that
    /// increment is below half a single-precision step at 30.0 and every add rounded home.
    /// Ants stopped ageing at four days, so none reached `NURSE_UNTIL`, none dug, and none
    /// died of old age. Starting the loop at 30.0 rather than 0.0 is the point of it.
    #[test]
    fn a_worker_ages_a_day_in_a_day_at_any_age() {
        let clock = ColonyClock::default();
        for start in [0.0, 4.0, 30.0, 400.0] {
            let mut age_days: f64 = start;
            for _ in 0..(60 * 60 * 60 * 24) {
                age_days += DT as f64 * clock.days_per_second;
            }
            let gained = age_days - start;
            assert!(
                (gained - 1.0).abs() < 1e-6,
                "at age {start} a day of real time advanced the colony by {gained} days",
            );
        }
    }

    /// And the ageing has to reach the thresholds it gates. A worker one real day short of
    /// digging becomes a digger; ten real days later it is on the surface.
    #[test]
    fn a_worker_changes_job_as_the_days_pass() {
        assert_eq!(Job::for_age(NURSE_UNTIL - 0.001), Job::Nurse);
        assert_eq!(Job::for_age(NURSE_UNTIL), Job::Digger);
        assert_eq!(Job::for_age(DIGGER_UNTIL), Job::Surface);

        let clock = ColonyClock::default();
        let mut age_days: f64 = NURSE_UNTIL - 0.5;
        assert_eq!(Job::for_age(age_days), Job::Nurse);
        for _ in 0..(60 * 60 * 60 * 24) {
            age_days += DT as f64 * clock.days_per_second;
        }
        assert_eq!(
            Job::for_age(age_days),
            Job::Digger,
            "a nurse half a day off digging must be digging tomorrow",
        );
    }
}
