//! The sand grid: ground truth for everything in the tank.
//!
//! The grid is a side-on slice of a formicarium — a thin slab of sand between glass.
//! It is deliberately the *only* authority on where sand is. Rendering, input and
//! (later) the ants all read and write this one structure.
//!
//! Two design constraints shape it:
//!
//! 1. **Determinism.** The farm has to serialize across days, so nothing here uses an
//!    RNG. Randomness is a positional hash of `(x, y, tick)`, which gives the same
//!    result on every machine and every replay.
//! 2. **Cheap idling.** The game lives in a window for hours. The grid is divided into
//!    chunks that sleep when nothing in them moved, so settled sand costs nothing.

use bevy::prelude::*;

pub const GRID_W: usize = 256;
pub const GRID_H: usize = 160;

pub const CHUNK: usize = 16;
pub const CHUNKS_X: usize = GRID_W / CHUNK;
pub const CHUNKS_Y: usize = GRID_H / CHUNK;
pub const N_CHUNKS: usize = CHUNKS_X * CHUNKS_Y;

/// World-space size of one cell. The whole tank is `GRID_W x GRID_H` of these.
pub const CELL: f32 = 0.05;
/// How deep the sand slab is, front to back. Thin, like a real ant farm.
pub const SLAB_DEPTH: f32 = 0.35;

pub const TANK_W: f32 = GRID_W as f32 * CELL;
pub const TANK_H: f32 = GRID_H as f32 * CELL;

/// Row of the original sand surface: the fill line the farm starts at, and the fixed
/// reference the ants measure "outside" and their roaming ceiling against.
pub const INITIAL_SURFACE: usize = GRID_H * 62 / 100;

/// M1 substances. `Food` and `Water` arrive with the colony in M2.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Substance {
    Air,
    Sand,
    /// Immovable. Nothing places it yet — it's for the pebbles and decor the ants will
    /// have to build around, which the sim already treats correctly as support.
    #[allow(dead_code)]
    Stone,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub mat: Substance,
    /// Palette index. This travels with the grain when it moves, which is what makes
    /// spoil piles show visibly mixed strata — the farm's history in colour.
    pub shade: u8,
}

impl Cell {
    pub const AIR: Cell = Cell { mat: Substance::Air, shade: 0 };
}

#[inline]
pub fn chunk_index(x: usize, y: usize) -> usize {
    (y / CHUNK) * CHUNKS_X + (x / CHUNK)
}

/// Deterministic hash. Stands in for an RNG so replays and saves stay identical.
#[inline]
pub fn hash3(x: u32, y: u32, z: u32) -> u32 {
    let mut h = x
        .wrapping_mul(0x9E37_79B1)
        ^ y.wrapping_mul(0x85EB_CA77)
        ^ z.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    h
}

/// Deterministic hash in `0.0..1.0`.
#[inline]
pub fn hash01(x: u32, y: u32, z: u32) -> f32 {
    (hash3(x, y, z) >> 8) as f32 / 16_777_216.0
}

#[derive(Resource)]
pub struct SandGrid {
    cells: Vec<Cell>,
    /// Per chunk. Rises when the player disturbs the tank, decays on its own.
    /// This is the single variable the whole chaos mechanic runs on.
    pub agitation: Vec<f32>,
    /// Per chunk. Geometry changed, needs a remesh.
    pub dirty: Vec<bool>,
    /// Per chunk. Simulate this tick.
    pub awake: Vec<bool>,
    /// Per chunk. Accumulated during a tick, becomes `awake` on the next one.
    pub next_awake: Vec<bool>,
    /// Per cell scratch, so a grain can't be stepped twice in one tick.
    moved: Vec<bool>,
    /// Per cell. Set on a grain that has moved and not yet come to rest.
    ///
    /// Loose sand has **no cohesion at all**; packed sand has all of it. That single bit
    /// is the difference between poured sand, which has to find its angle of repose, and
    /// packed strata, which have to hold a tunnel open for days.
    ///
    /// One stability threshold cannot serve both, and trying cost a lot of time. The top
    /// grain of a one-cell-wide spire scores 3 — something directly beneath it — while a
    /// tunnel ceiling hanging by its two sides scores 2. Any threshold low enough to keep
    /// ceilings up is also happy to hold a spire, so no amount of tuning gets cones out
    /// of a pour. The two cases aren't the same physics: one is loose sand, the other is
    /// compacted. So the model says so.
    loose: Vec<bool>,
    pub tick: u32,
    /// Bumped whenever any cell changes. Lets the navigation flood skip rebuilding for
    /// a farm that hasn't moved, which is most of the time.
    pub epoch: u64,
}

impl SandGrid {
    pub fn new() -> Self {
        Self {
            cells: vec![Cell::AIR; GRID_W * GRID_H],
            agitation: vec![0.0; N_CHUNKS],
            dirty: vec![true; N_CHUNKS],
            awake: vec![true; N_CHUNKS],
            next_awake: vec![true; N_CHUNKS],
            moved: vec![false; GRID_W * GRID_H],
            loose: vec![false; GRID_W * GRID_H],
            tick: 0,
            epoch: 0,
        }
    }

    #[inline]
    pub fn idx(x: usize, y: usize) -> usize {
        y * GRID_W + x
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Cell {
        self.cells[Self::idx(x, y)]
    }

    #[inline]
    pub fn in_bounds(x: isize, y: isize) -> bool {
        x >= 0 && y >= 0 && x < GRID_W as isize && y < GRID_H as isize
    }

    /// Air includes everything above the open top of the tank. The floor and the two
    /// side walls are not air — sand rests against them.
    #[inline]
    pub fn is_air(&self, x: isize, y: isize) -> bool {
        if !Self::in_bounds(x, y) {
            return y >= GRID_H as isize && x >= 0 && x < GRID_W as isize;
        }
        self.cells[Self::idx(x as usize, y as usize)].mat == Substance::Air
    }

    /// Does this location hold up sand resting on it? Out-of-bounds walls and floor do.
    #[inline]
    pub fn supports(&self, x: isize, y: isize) -> bool {
        !self.is_air(x, y)
    }

    /// How well held-in-place a cell is, `0..=9`, weighted so support from directly
    /// below counts for most. Compared against a threshold that rises with agitation.
    #[inline]
    pub fn stability(&self, x: usize, y: usize) -> u32 {
        let (xi, yi) = (x as isize, y as isize);
        let s = |dx: isize, dy: isize| u32::from(self.supports(xi + dx, yi + dy));
        3 * s(0, -1) + 2 * (s(-1, -1) + s(1, -1)) + (s(-1, 0) + s(1, 0))
    }

    /// Number of sand cells currently in the grid. Grains in flight are *not* counted,
    /// since they've been lifted out — add the live particle count for a true total.
    pub fn sand_count(&self) -> usize {
        self.cells.iter().filter(|c| c.mat == Substance::Sand).count()
    }

    /// Snapshot of which cells are solid, for comparing farm states over time.
    pub fn solid_mask(&self) -> Vec<bool> {
        self.cells.iter().map(|c| c.mat != Substance::Air).collect()
    }

    #[inline]
    pub fn agitation_at(&self, x: usize, y: usize) -> f32 {
        self.agitation[chunk_index(x, y)]
    }

    /// Has this grain still to find its rest? Loose grains have no cohesion.
    #[inline]
    pub fn is_loose(&self, x: usize, y: usize) -> bool {
        self.loose[Self::idx(x, y)]
    }

    /// This grain has come to rest. It gains cohesion and stops slumping — which is what
    /// lets a spoil mound, once settled, be tunnelled through like any other sand.
    #[inline]
    pub fn pack(&mut self, x: usize, y: usize) {
        self.loose[Self::idx(x, y)] = false;
    }

    /// Write a cell without waking anything. Only for bulk fills, which lay down packed
    /// strata — sand that has been sitting in the tank, not sand that just arrived.
    #[inline]
    pub fn set_raw(&mut self, x: usize, y: usize, cell: Cell) {
        self.cells[Self::idx(x, y)] = cell;
        self.loose[Self::idx(x, y)] = false;
    }

    /// Write a cell and its loose flag, waking nothing. For restoring a saved farm, which
    /// wakes and remeshes the whole grid once at the end rather than forty thousand times
    /// on the way through.
    #[inline]
    pub fn set_raw_with_loose(&mut self, x: usize, y: usize, cell: Cell, loose: bool) {
        let i = Self::idx(x, y);
        self.cells[i] = cell;
        self.loose[i] = loose;
    }

    /// Write a cell and wake the neighbourhood so the sim reacts to it.
    pub fn set(&mut self, x: usize, y: usize, cell: Cell) {
        self.cells[Self::idx(x, y)] = cell;
        self.loose[Self::idx(x, y)] = false;
        self.touch(x, y);
    }

    /// Write a grain that hasn't settled yet — poured sand, ant spoil, a grain dropping
    /// out of the air. It arrives with no cohesion and slumps until it finds a rest, so
    /// it heaps into a cone instead of standing wherever it happened to land.
    pub fn set_loose(&mut self, x: usize, y: usize, cell: Cell) {
        self.cells[Self::idx(x, y)] = cell;
        self.loose[Self::idx(x, y)] = true;
        self.touch(x, y);
    }

    /// Mark a cell's chunk (and its neighbours) as needing a remesh and a simulation
    /// pass. Neighbours matter because a grain leaving one chunk lets the grain above
    /// it — possibly in the chunk above — start falling.
    pub fn touch(&mut self, x: usize, y: usize) {
        self.epoch = self.epoch.wrapping_add(1);
        let (cx, cy) = (x / CHUNK, y / CHUNK);
        for dy in -1isize..=1 {
            for dx in -1isize..=1 {
                let (nx, ny) = (cx as isize + dx, cy as isize + dy);
                if nx < 0 || ny < 0 || nx >= CHUNKS_X as isize || ny >= CHUNKS_Y as isize {
                    continue;
                }
                let c = ny as usize * CHUNKS_X + nx as usize;
                self.next_awake[c] = true;
                self.awake[c] = true;
            }
        }
        self.dirty[chunk_index(x, y)] = true;
    }

    /// Add agitation in a world-space radius of cells, falling off with distance.
    /// A tap uses a small radius; a shake floods every chunk.
    pub fn agitate(&mut self, cx: f32, cy: f32, radius_cells: f32, amount: f32) {
        let r_chunks = (radius_cells / CHUNK as f32).max(0.5);
        let ccx = cx / CHUNK as f32;
        let ccy = cy / CHUNK as f32;
        for cyi in 0..CHUNKS_Y {
            for cxi in 0..CHUNKS_X {
                let dx = (cxi as f32 + 0.5) - ccx;
                let dy = (cyi as f32 + 0.5) - ccy;
                let d = (dx * dx + dy * dy).sqrt();
                if d > r_chunks {
                    continue;
                }
                let falloff = 1.0 - (d / r_chunks);
                let c = cyi * CHUNKS_X + cxi;
                self.agitation[c] = (self.agitation[c] + amount * falloff * falloff).min(1.0);
                self.awake[c] = true;
                self.next_awake[c] = true;
            }
        }
    }

    pub fn agitate_all(&mut self, amount: f32) {
        for c in 0..N_CHUNKS {
            self.agitation[c] = (self.agitation[c] + amount).min(1.0);
            self.awake[c] = true;
            self.next_awake[c] = true;
        }
    }

    #[inline]
    pub fn was_moved(&self, x: usize, y: usize) -> bool {
        self.moved[Self::idx(x, y)]
    }

    /// Move a grain, leaving air behind. Marks the destination so it can't step again
    /// this tick, and wakes both neighbourhoods.
    ///
    /// Anything that moves lands loose, whatever it was before. A grain the shake just
    /// knocked off a wall is no longer part of a packed mass — it's tumbling — so it
    /// keeps tumbling until it finds somewhere to sit.
    pub fn move_cell(&mut self, x: usize, y: usize, nx: usize, ny: usize) {
        let cell = self.get(x, y);
        self.cells[Self::idx(x, y)] = Cell::AIR;
        self.cells[Self::idx(nx, ny)] = cell;
        self.loose[Self::idx(x, y)] = false;
        self.loose[Self::idx(nx, ny)] = true;
        self.moved[Self::idx(nx, ny)] = true;
        self.touch(x, y);
        self.touch(nx, ny);
    }

    /// Lift a grain out of the grid entirely — it's becoming a loose particle.
    pub fn take(&mut self, x: usize, y: usize) -> Cell {
        let cell = self.get(x, y);
        self.cells[Self::idx(x, y)] = Cell::AIR;
        self.loose[Self::idx(x, y)] = false;
        self.touch(x, y);
        cell
    }

    pub fn begin_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        std::mem::swap(&mut self.awake, &mut self.next_awake);
        self.next_awake.fill(false);
        self.moved.fill(false);
    }

    /// Cell centre in tank-local space. The grid is centred on the tank's origin.
    #[inline]
    pub fn cell_to_world(x: usize, y: usize) -> Vec3 {
        Vec3::new(
            (x as f32 - GRID_W as f32 * 0.5 + 0.5) * CELL,
            (y as f32 - GRID_H as f32 * 0.5 + 0.5) * CELL,
            0.0,
        )
    }

    /// Tank-local space back to (possibly out-of-range) grid coordinates.
    #[inline]
    pub fn world_to_cell(p: Vec3) -> (f32, f32) {
        (
            p.x / CELL + GRID_W as f32 * 0.5,
            p.y / CELL + GRID_H as f32 * 0.5,
        )
    }
}

impl Default for SandGrid {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

pub const STRATA: usize = 5;
pub const VARIANTS: usize = 8;
pub const PALETTE_LEN: usize = STRATA * VARIANTS;

/// Coloured sand layers, like the ones people actually put in formicaria. They do real
/// work: when the ants dig through a band the cut is legible, and the spoil they carry
/// out is visibly the colour of wherever it came from.
const STRATA_BASE: [[f32; 3]; STRATA] = [
    [0.78, 0.65, 0.44], // pale ochre
    [0.62, 0.38, 0.26], // rust
    [0.84, 0.76, 0.60], // cream
    [0.44, 0.32, 0.24], // umber
    [0.58, 0.51, 0.45], // grey-brown
];

#[derive(Resource)]
pub struct SandPalette(pub Vec<Color>);

impl SandPalette {
    pub fn build() -> Self {
        let mut v = Vec::with_capacity(PALETTE_LEN);
        for base in STRATA_BASE {
            for i in 0..VARIANTS {
                // A narrow brightness spread per stratum reads as grain, not noise.
                let k = 0.88 + (i as f32 / (VARIANTS - 1) as f32) * 0.24;
                v.push(Color::srgb(
                    (base[0] * k).clamp(0.0, 1.0),
                    (base[1] * k).clamp(0.0, 1.0),
                    (base[2] * k).clamp(0.0, 1.0),
                ));
            }
        }
        Self(v)
    }

    #[inline]
    pub fn linear(&self, shade: u8) -> [f32; 4] {
        let c = self.0[(shade as usize).min(PALETTE_LEN - 1)].to_linear();
        [c.red, c.green, c.blue, 1.0]
    }
}

/// Fill the tank with layered sand up to `fill_frac` of its height.
///
/// Stratum boundaries get a couple of superimposed sine waves so the layers aren't
/// suspiciously flat — the same trick geology uses.
pub fn fill_strata(grid: &mut SandGrid, fill_h: usize) {
    // Empty the tank first. This only ever wrote *below* the fill line, which is fine for
    // a fresh grid and wrong for a reset: spoil mounds, poured sand and anything else
    // above the line survived, so going back to the title screen left the previous farm's
    // heaps sitting on top of brand new strata.
    for y in 0..GRID_H {
        for x in 0..GRID_W {
            grid.set_raw(x, y, Cell::AIR);
        }
    }

    for x in 0..GRID_W {
        let fx = x as f32;
        let wobble = (fx * 0.021).sin() * 5.0 + (fx * 0.052 + 1.7).sin() * 2.5;

        for y in 0..fill_h {
            let depth_frac = (y as f32 + wobble) / fill_h as f32;
            let stratum = ((1.0 - depth_frac) * STRATA as f32)
                .clamp(0.0, (STRATA - 1) as f32) as usize;
            let variant = (hash01(x as u32, y as u32, 0x5EED) * VARIANTS as f32) as usize;
            let shade = (stratum * VARIANTS + variant.min(VARIANTS - 1)) as u8;
            grid.set_raw(x, y, Cell { mat: Substance::Sand, shade });
        }
    }

    grid.dirty.fill(true);
    grid.awake.fill(true);
    grid.next_awake.fill(true);
    // A wholesale change, so bump the revision: the navigation flood skips rebuilding
    // when the grid looks untouched, and a refilled tank must not look untouched.
    grid.epoch = grid.epoch.wrapping_add(1);
}
