//! The falling-sand cellular automaton.
//!
//! Sand is in one of two states, and they behave nothing alike.
//!
//! **Packed** sand — the strata the tank was filled with, and anything that has come to
//! rest since — is held by cohesion. Each cell scores how well it's gripped
//! (`SandGrid::stability`, 0..=9) and lets go when that score drops below a threshold
//! that rises with local **agitation**:
//!
//! - **Calm.** The threshold is 1.2, so a grain needs either something under it or a
//!   neighbour on each side. Vertical walls stand, tunnel ceilings hang, and the
//!   architecture the ants dig persists indefinitely. This is the whole reason the
//!   farm can accumulate a history.
//! - **Shaken.** The threshold climbs past 6, so only solidly buried grains hold.
//!   Overhangs fail, piles slump, tunnels cave. The mass survives; the architecture
//!   does not. Which is exactly the point — shaking erases the record, it doesn't
//!   destroy the sand.
//!
//! **Loose** sand — poured sand, ant spoil, anything the shake just knocked off a wall —
//! has no cohesion whatsoever. It falls, and failing that rolls off whatever it's on,
//! until it reaches a spot with no downhill step to take. That single rule is where the
//! angle of repose comes from: a grain rests only when both of its diagonals are blocked,
//! which is exactly a slope of 45°. Piles come out as cones, and pouring onto a cone's
//! apex sends the grain rolling to the toe, so the heap widens instead of spiking.
//! The moment a loose grain runs out of moves it packs, and the cohesion model takes over
//! — so a spoil mound, once settled, can be tunnelled through like anything else.

use crate::grid::*;
use bevy::prelude::*;

/// Per-tick multiplier on agitation. At 60 Hz this leaves ~30% after one second, so
/// the tank visibly settles a beat or two after you stop.
const AGITATION_DECAY: f32 = 0.98;

/// Below this, treat agitation as zero so chunks can actually get to sleep.
const AGITATION_EPSILON: f32 = 0.004;

const STABILITY_BASE: f32 = 1.2;
const STABILITY_PER_AGITATION: f32 = 5.6;

/// Per-cell variation in how well a grain grips, in stability units.
///
/// Without this the model has a nasty cliff: `stability` is an integer, so every cell
/// scoring the same number fails at the exact same threshold, and a tap either does
/// nothing at all or drops an entire ceiling in one tick. Real sand isn't uniform —
/// grains differ in how they've packed — so each cell gets a fixed, deterministic
/// offset. Ceilings then shed a *fraction* of their grains as agitation climbs, and
/// the survivors settle into an arch, which is what sand actually does.
const COHESION_JITTER: f32 = 0.9;

/// Agitation above which grains start fluidising sideways rather than just slumping.
const FLUIDISE_THRESHOLD: f32 = 0.30;
/// Agitation above which surface grains can be thrown clear of the grid entirely.
const EJECT_THRESHOLD: f32 = 0.45;

const MAX_QUEUED_GRAINS: usize = 96;

/// Fraction of the tank's own velocity an ejected grain inherits, and a hard ceiling
/// on it. The tank is only ~13 units wide, so anything much above this throws grains
/// clean across the farm.
const SHAKE_CARRY: f32 = 0.10;
const MAX_CARRY_SPEED: f32 = 1.6;

/// A grain thrown clear of the grid, to be picked up by the particle system.
pub struct GrainSpawn {
    pub x: usize,
    pub y: usize,
    pub shade: u8,
    pub vel: Vec3,
}

#[derive(Resource, Default)]
pub struct GrainSpawnQueue(pub Vec<GrainSpawn>);

/// How the tank itself is currently moving. Grains get thrown along with it, so a
/// shake to the left sprays sand to the left.
#[derive(Resource, Default)]
pub struct TankMotion {
    pub vel: Vec3,
}

pub fn step_sand(
    mut grid: ResMut<SandGrid>,
    mut queue: ResMut<GrainSpawnQueue>,
    motion: Res<TankMotion>,
) {
    sweep(&mut grid, &mut queue, motion.vel);
}

/// One tick of the automaton. Split out from the system so the tests can drive it
/// directly — the behaviour here is emergent enough that eyeballing a screenshot has
/// repeatedly failed to tell a working model from a broken one.
pub fn sweep(grid: &mut SandGrid, queue: &mut GrainSpawnQueue, shake_dir: Vec3) {
    grid.begin_tick();
    let tick = grid.tick;

    // Decay agitation, and keep any chunk that still has some awake.
    for c in 0..N_CHUNKS {
        let a = grid.agitation[c] * AGITATION_DECAY;
        grid.agitation[c] = if a < AGITATION_EPSILON { 0.0 } else { a };
        if grid.agitation[c] > 0.0 {
            grid.awake[c] = true;
            grid.next_awake[c] = true;
        }
    }

    // Alternate scan direction each tick, otherwise piles lean.
    let l2r = tick % 2 == 0;

    // Bottom row first, so a grain moves at most one step per tick.
    for y in 0..GRID_H {
        let row_base = (y / CHUNK) * CHUNKS_X;
        for ci in 0..CHUNKS_X {
            let cx = if l2r { ci } else { CHUNKS_X - 1 - ci };
            if !grid.awake[row_base + cx] {
                continue;
            }
            for k in 0..CHUNK {
                let k = if l2r { k } else { CHUNK - 1 - k };
                let x = cx * CHUNK + k;

                if grid.was_moved(x, y) || grid.get(x, y).mat != Substance::Sand {
                    continue;
                }
                step_grain(grid, queue, x, y, tick, shake_dir);
            }
        }
    }
}

fn step_grain(
    grid: &mut SandGrid,
    queue: &mut GrainSpawnQueue,
    x: usize,
    y: usize,
    tick: u32,
    shake_dir: Vec3,
) {
    let agit = grid.agitation_at(x, y);
    let required = STABILITY_BASE + agit * STABILITY_PER_AGITATION;
    // Keyed on position only, never the tick — grip has to be stable over time or
    // cells would flicker loose at rest and the farm would never stop moving.
    let grip = hash01(x as u32, y as u32, 0x6_1_11_1D) * COHESION_JITTER;
    // Loose sand doesn't get to consult cohesion at all. Sand that has just arrived
    // hasn't packed against anything, so it rolls until it runs out of downhill.
    let loose = grid.is_loose(x, y);
    let stable = !loose && (grid.stability(x, y) as f32 + grip) >= required;

    let (xi, yi) = (x as isize, y as isize);

    // A hard shake throws loose surface grains clear of the grid altogether. Only
    // grains with air above are eligible — those are the ones you'd actually see fly.
    if agit > EJECT_THRESHOLD
        && !stable
        && grid.is_air(xi, yi + 1)
        && queue.0.len() < MAX_QUEUED_GRAINS
        && hash01(x as u32, y as u32, tick ^ 0x9E1B) < (agit - EJECT_THRESHOLD) * 0.35
    {
        let cell = grid.take(x, y);
        let jitter = |s: u32| hash01(x as u32, y as u32, tick ^ s) - 0.5;
        // Grains are carried along by the tank, but only a little. Handing them the
        // tank's raw velocity flings them the full width of the farm in a second,
        // which reads as a fountain rather than a shake.
        let carry = (shake_dir * SHAKE_CARRY).clamp_length_max(MAX_CARRY_SPEED);
        let vel = Vec3::new(
            carry.x + jitter(0x11) * 3.0 * agit,
            carry.y + (0.8 + jitter(0x22)) * 3.2 * agit,
            jitter(0x33) * 0.6,
        );
        queue.0.push(GrainSpawn { x, y, shade: cell.shade, vel });
        return;
    }

    if stable {
        // Held by cohesion — but a shaken mass behaves more like a liquid than a
        // solid, so let held grains creep sideways to flatten the pile.
        if agit > FLUIDISE_THRESHOLD
            && hash01(x as u32, y as u32, tick ^ 0x4F2D) < (agit - FLUIDISE_THRESHOLD) * 0.30
        {
            slide(grid, x, y, tick);
        }
        return;
    }

    // Straight down.
    if grid.is_air(xi, yi - 1) {
        grid.move_cell(x, y, x, y - 1);
        return;
    }

    // Then diagonally, requiring the cell beside it to be clear too — otherwise sand
    // squeezes through diagonal gaps it should not fit through.
    let left_first = hash3(x as u32, y as u32, tick ^ 0x00C0FFEE) & 1 == 0;
    let order = if left_first { [-1isize, 1] } else { [1, -1] };
    for dx in order {
        let (nx, ny) = (xi + dx, yi - 1);
        if grid.is_air(nx, ny) && grid.is_air(xi + dx, yi) {
            grid.move_cell(x, y, nx as usize, ny as usize);
            return;
        }
    }

    // No downhill left in any direction, so this grain has arrived. Packing here is what
    // makes the heap a heap: both diagonals blocked means the local slope is 45° or
    // shallower, so a pile stops growing exactly at its angle of repose.
    if loose {
        grid.pack(x, y);
    }
}

/// Sideways creep under agitation. Biased along the shake so the mass sloshes.
fn slide(grid: &mut SandGrid, x: usize, y: usize, tick: u32) {
    let (xi, yi) = (x as isize, y as isize);
    let left_first = hash3(x as u32, y as u32, tick ^ 0x51DE) & 1 == 0;
    let order = if left_first { [-1isize, 1] } else { [1, -1] };
    for dx in order {
        let nx = xi + dx;
        // Only creep into a gap that has something to rest on, so slides fill
        // hollows instead of smearing grains across open tunnels.
        if grid.is_air(nx, yi) && grid.supports(nx, yi - 1) {
            grid.move_cell(x, y, nx as usize, y);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the automaton to a standstill, or fail. Every one of these tests depends on
    /// the sim actually settling, so a run that never converges is itself a bug.
    fn settle(grid: &mut SandGrid, queue: &mut GrainSpawnQueue, max_ticks: u32) -> u32 {
        for tick in 1..=max_ticks {
            let before = grid.epoch;
            sweep(grid, queue, Vec3::ZERO);
            if grid.epoch == before {
                return tick;
            }
        }
        panic!("the sand never came to rest in {max_ticks} ticks");
    }

    fn sand(shade: u8) -> Cell {
        Cell { mat: Substance::Sand, shade }
    }

    /// Height of the sand in each column: one past the topmost grain, 0 for empty.
    fn profile(grid: &SandGrid) -> Vec<usize> {
        (0..GRID_W)
            .map(|x| {
                (0..GRID_H)
                    .rev()
                    .find(|&y| grid.get(x, y).mat == Substance::Sand)
                    .map_or(0, |y| y + 1)
            })
            .collect()
    }

    /// The bug in the screenshot, at its smallest: a stack of loose grains one cell wide.
    /// It used to stand there indefinitely, because a grain with something underneath it
    /// scores 3 against a threshold of 1.2.
    #[test]
    fn a_loose_column_topples() {
        let mut grid = SandGrid::new();
        let mut queue = GrainSpawnQueue::default();
        let x = GRID_W / 2;
        for y in 0..12 {
            grid.set_loose(x, y, sand(0));
        }

        settle(&mut grid, &mut queue, 500);

        assert_eq!(grid.sand_count(), 12, "grains went missing");
        let heights = profile(&grid);
        assert!(
            heights[x] <= 3,
            "the tower is still standing: {} cells tall",
            heights[x]
        );
    }

    /// Pour onto one spot and the heap has to come out as a cone. The test of that is the
    /// slope itself: at rest no column may stand more than one cell above its neighbour,
    /// which is 45° — the angle the diagonal rule produces.
    #[test]
    fn a_pour_heaps_into_a_cone() {
        let mut grid = SandGrid::new();
        let mut queue = GrainSpawnQueue::default();
        let x = GRID_W / 2;

        // 400 grains fed in one at a time, the way the pour actually delivers them.
        for _ in 0..400 {
            let top = GRID_H - 1;
            if grid.get(x, top).mat == Substance::Air {
                grid.set_loose(x, top, sand(0));
            }
            sweep(&mut grid, &mut queue, Vec3::ZERO);
        }
        settle(&mut grid, &mut queue, 2000);

        assert_eq!(grid.sand_count(), 400, "grains went missing");

        let heights = profile(&grid);
        for x in 0..GRID_W - 1 {
            let step = heights[x].abs_diff(heights[x + 1]);
            assert!(
                step <= 1,
                "the slope is steeper than the angle of repose at x={x}: {} then {}",
                heights[x],
                heights[x + 1],
            );
        }

        // And it is a heap, not a puddle: a cone of 400 grains is about 20 tall.
        let peak = *heights.iter().max().unwrap();
        assert!((10..=30).contains(&peak), "peak height {peak} isn't cone-shaped");
    }

    /// The other half of the model, and the one the game is built on: sand that has
    /// settled holds its shape. If loose sand ever leaked into the packed case, tunnels
    /// would silently fill in and the farm would stop accumulating a history.
    #[test]
    fn packed_sand_holds_a_tunnel_open() {
        let mut grid = SandGrid::new();
        let mut queue = GrainSpawnQueue::default();
        fill_strata(&mut grid, INITIAL_SURFACE);

        // A shaft down from the surface and a chamber off the bottom of it.
        let shaft = GRID_W / 2;
        for y in 20..INITIAL_SURFACE {
            grid.set(shaft, y, Cell::AIR);
        }
        for y in 18..24 {
            for x in shaft..shaft + 14 {
                grid.set(x, y, Cell::AIR);
            }
        }
        let carved = grid.sand_count();

        settle(&mut grid, &mut queue, 600);

        assert_eq!(grid.sand_count(), carved, "sand appeared or vanished at rest");
        for y in 30..INITIAL_SURFACE {
            assert!(grid.is_air(shaft as isize, y as isize), "the shaft filled in at y={y}");
        }
        assert!(grid.is_air((shaft + 13) as isize, 21), "the chamber filled in");
    }

    /// Spoil dropped on a settled mound must not be able to re-open it. Packing is what
    /// makes a mound solid ground, so a grain landing on one rolls off the outside
    /// instead of burrowing in.
    #[test]
    fn spoil_settles_on_a_mound_without_reopening_it() {
        let mut grid = SandGrid::new();
        let mut queue = GrainSpawnQueue::default();
        for x in 40..60 {
            for y in 0..6 {
                grid.set_loose(x, y, sand(0));
            }
        }
        settle(&mut grid, &mut queue, 500);
        let before = profile(&grid);

        for _ in 0..40 {
            grid.set_loose(50, GRID_H - 1, sand(0));
            settle(&mut grid, &mut queue, 500);
        }

        assert_eq!(grid.sand_count(), 20 * 6 + 40);
        let after = profile(&grid);
        // The mound got taller or wider, never hollow: no column lost sand.
        for x in 0..GRID_W {
            assert!(after[x] >= before[x], "column {x} sank from {} to {}", before[x], after[x]);
        }
    }

    /// A shake is allowed to flatten the farm; it is not allowed to change how much sand
    /// is in it. The one number that must never drift.
    #[test]
    fn a_shake_conserves_sand() {
        let mut grid = SandGrid::new();
        let mut queue = GrainSpawnQueue::default();
        fill_strata(&mut grid, INITIAL_SURFACE);
        let filled = grid.sand_count();

        for _ in 0..240 {
            grid.agitate_all(0.05);
            sweep(&mut grid, &mut queue, Vec3::new(3.0, 0.0, 0.0));
        }

        // Grains thrown clear are in the queue, not the grid — both count.
        assert_eq!(grid.sand_count() + queue.0.len(), filled, "the shake changed the mass");
    }
}
