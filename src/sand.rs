//! The falling-sand cellular automaton.
//!
//! One variable does the work. Each sand cell scores how well it's held in place
//! (`SandGrid::stability`, 0..=9) and falls when that score drops below a threshold
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
    let shake_dir = motion.vel;

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
                step_grain(&mut grid, &mut queue, x, y, tick, shake_dir);
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
    let stable = (grid.stability(x, y) as f32 + grip) >= required;

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
