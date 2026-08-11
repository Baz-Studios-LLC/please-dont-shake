//! The fields the colony coordinates through.
//!
//! Ants have almost no individual knowledge. Everything that looks like planning —
//! tunnels that extend instead of blobbing, spoil that goes outside instead of
//! everywhere, workers that cluster around the queen — comes out of local rules read
//! off these shared fields. This module is where the colony's "mind" lives.
//!
//! Two different kinds of field, for two different jobs:
//!
//! **Diffusing pheromones** ([`Pheromones`]) for local attraction. Real chemical
//! signals: deposited by ants, spreading and evaporating. Crucially they diffuse
//! *through air only* — sand is a no-flux boundary — so a signal respects the shape of
//! the nest instead of leaking through walls.
//!
//! **A navigation field** ([`NavField`]) for finding the way out. This deliberately
//! isn't a pheromone. A diffusion-decay field's useful range is `sqrt(D·f/evap)`
//! cells, and an explicit 4-neighbour stencil goes unstable above `D = 0.25`, which
//! caps the practical reach at roughly twenty cells. The tank is 256 wide, so a laden
//! ant forty cells down would sit on a field of numerical zero and never find the
//! surface. A breadth-first distance flood is exact at any range, costs about the same,
//! and re-routes for free when a shake collapses a tunnel.

use crate::grid::*;
use bevy::prelude::*;
use std::collections::VecDeque;

/// How often the fields update. Chemistry doesn't need the sand's 60 Hz.
pub const FIELD_HZ: f32 = 15.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ph {
    /// Deposited where sand is excavated. Diggers climb it, which is what makes a
    /// tunnel a tunnel: work attracts work, so digging extends an existing face
    /// instead of starting a new hole somewhere else. This is the whole of stigmergy.
    Dig = 0,
    /// Released on injury or disturbance. What a tap and a shake actually land on.
    Alarm = 1,
    /// Signals the queen is alive and where she is. Workers drift up it, which is why
    /// a colony visibly clusters around her. When she dies it fades, and they know.
    Queen = 2,
}

pub const PH_LAYERS: usize = 3;
const LAYER_LEN: usize = GRID_W * GRID_H;

/// Diffusion coefficient and evaporation per second, per layer.
///
/// `D` must stay below 0.25: an explicit 4-neighbour stencil diverges above that.
/// Together they set the reach of each signal, `sqrt(D · FIELD_HZ / evap)` in cells —
/// noted below because reach is the property that actually matters behaviourally.
const PH_PARAMS: [(f32, f32); PH_LAYERS] = [
    (0.22, 0.010), // Dig   — reach ~18 cells, a dig site stays attractive ~100s
    (0.22, 0.012), // Alarm — floods the nest, then calms over ~80s
    (0.15, 0.020), // Queen — reach ~11 cells, a tight cluster
];

/// Below this a layer is treated as empty so it can be skipped entirely, which is what
/// keeps a farm nobody is touching at effectively no CPU cost.
const PH_EPSILON: f32 = 1e-4;

#[derive(Resource)]
pub struct Pheromones {
    fields: Vec<f32>,
    scratch: Vec<f32>,
    /// Per layer: is there anything in here worth simulating?
    active: [bool; PH_LAYERS],
}

impl Pheromones {
    pub fn new() -> Self {
        Self {
            fields: vec![0.0; PH_LAYERS * LAYER_LEN],
            scratch: vec![0.0; LAYER_LEN],
            active: [false; PH_LAYERS],
        }
    }

    #[inline]
    fn base(layer: Ph) -> usize {
        layer as usize * LAYER_LEN
    }

    #[inline]
    pub fn get(&self, layer: Ph, x: usize, y: usize) -> f32 {
        self.fields[Self::base(layer) + y * GRID_W + x]
    }

    /// Sample with bounds handling, for gradient reads near the walls.
    #[inline]
    pub fn at(&self, layer: Ph, x: isize, y: isize) -> f32 {
        if !SandGrid::in_bounds(x, y) {
            return 0.0;
        }
        self.get(layer, x as usize, y as usize)
    }

    pub fn deposit(&mut self, layer: Ph, x: usize, y: usize, amount: f32) {
        let i = Self::base(layer) + y * GRID_W + x;
        self.fields[i] += amount;
        self.active[layer as usize] = true;
    }

    /// Deposit in a falling-off disc — what a tap on the glass does to alarm.
    pub fn deposit_disc(&mut self, layer: Ph, cx: f32, cy: f32, radius: f32, amount: f32) {
        let x0 = ((cx - radius).floor().max(0.0)) as usize;
        let x1 = ((cx + radius).ceil().clamp(0.0, GRID_W as f32 - 1.0)) as usize;
        let y0 = ((cy - radius).floor().max(0.0)) as usize;
        let y1 = ((cy + radius).ceil().clamp(0.0, GRID_H as f32 - 1.0)) as usize;

        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = Vec2::new(x as f32 + 0.5 - cx, y as f32 + 0.5 - cy).length();
                if d > radius {
                    continue;
                }
                let falloff = 1.0 - d / radius;
                self.deposit(layer, x, y, amount * falloff * falloff);
            }
        }
    }

    /// Flood a layer everywhere at once — what a shake does to alarm.
    pub fn deposit_everywhere(&mut self, layer: Ph, amount: f32) {
        let base = Self::base(layer);
        for v in &mut self.fields[base..base + LAYER_LEN] {
            *v += amount;
        }
        self.active[layer as usize] = true;
    }

    /// Local uphill direction, sampled from the four neighbours. Returns zero where the
    /// field is flat, so callers can fall back on their own heading.
    pub fn gradient(&self, layer: Ph, x: usize, y: usize) -> Vec2 {
        let (xi, yi) = (x as isize, y as isize);
        Vec2::new(
            self.at(layer, xi + 1, yi) - self.at(layer, xi - 1, yi),
            self.at(layer, xi, yi + 1) - self.at(layer, xi, yi - 1),
        )
    }
}

impl Default for Pheromones {
    fn default() -> Self {
        Self::new()
    }
}

pub fn diffuse_pheromones(mut ph: ResMut<Pheromones>, grid: Res<SandGrid>) {
    let dt = 1.0 / FIELD_HZ;
    let Pheromones { fields, scratch, active } = &mut *ph;

    for layer in 0..PH_LAYERS {
        if !active[layer] {
            continue;
        }
        let (d, evap) = PH_PARAMS[layer];
        let keep = 1.0 - evap * dt;
        let base = layer * LAYER_LEN;
        let mut any = false;

        for y in 0..GRID_H {
            for x in 0..GRID_W {
                let i = y * GRID_W + x;

                // Pheromone can't sit inside sand. A cell that just got buried loses
                // whatever it held, which is what burial should do.
                if !grid.is_air(x as isize, y as isize) {
                    scratch[i] = 0.0;
                    continue;
                }

                let v = fields[base + i];
                let (xi, yi) = (x as isize, y as isize);

                // Solid neighbours are a no-flux boundary: no exchange across them, so
                // a signal spreads along the tunnel rather than through the wall.
                //
                // `in_bounds` has to be checked *before* `is_air`, which reports the
                // space above the tank as air because it genuinely is open sky — true
                // for the sand sim, but there's no array cell there to read.
                let mut sum = 0.0;
                let mut open = 0.0;
                for (dx, dy) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (xi + dx, yi + dy);
                    if SandGrid::in_bounds(nx, ny) && grid.is_air(nx, ny) {
                        sum += fields[base + ny as usize * GRID_W + nx as usize];
                        open += 1.0;
                    }
                }

                let next = (v + d * (sum - open * v)) * keep;
                scratch[i] = if next < PH_EPSILON { 0.0 } else { next };
                any |= scratch[i] > 0.0;
            }
        }

        fields[base..base + LAYER_LEN].copy_from_slice(scratch);
        active[layer] = any;
    }
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

pub const UNREACHABLE: u16 = u16::MAX;

/// Half-width, in columns, of the window the terrain envelope is taken over. Has to be
/// wider than any tunnel the ants dig, or a shaft reads as open ground.
const SURFACE_ENVELOPE: usize = 7;

/// How far every air cell is from open sky, in steps through air, plus where the sand
/// surface currently sits in each column.
///
/// A laden ant descends `dist` to get out; a digger climbs it to get deeper. Because
/// it's a flood through actual air, a tunnel that caves in stops being a route on the
/// very next rebuild, and ants reroute without knowing anything about it.
#[derive(Resource)]
pub struct NavField {
    pub dist: Vec<u16>,
    /// Terrain height per column — but taken as the *maximum* over a window of nearby
    /// columns, not the column itself. This is what tells a laden ant it has genuinely
    /// got outside and can dump its grain.
    ///
    /// The per-column reading is wrong in a way that quietly breaks the colony. A column
    /// containing a vertical shaft has its topmost solid cell *below the chamber*, so the
    /// whole shaft reads as open ground: ants haul a grain a few cells up the tunnel,
    /// conclude they're outdoors, and drop it straight back into the nest. Digging churns
    /// forever and the nest never grows. Taking the envelope over neighbouring columns
    /// means a narrow hole doesn't count as outside, so spoil has to actually be carried
    /// out onto the surface — which is also what builds the crater rim around a real
    /// nest entrance.
    pub surface: Vec<u16>,
    /// Per-column topmost solid cell, before the envelope filter.
    raw_surface: Vec<u16>,
    queue: VecDeque<u32>,
    /// The grid revision this was built from, so a settled farm rebuilds nothing.
    built_epoch: u64,
}

impl NavField {
    pub fn new() -> Self {
        Self {
            dist: vec![UNREACHABLE; GRID_W * GRID_H],
            surface: vec![0; GRID_W],
            raw_surface: vec![0; GRID_W],
            queue: VecDeque::with_capacity(GRID_W * GRID_H / 4),
            built_epoch: u64::MAX,
        }
    }

    #[inline]
    pub fn at(&self, x: isize, y: isize) -> u16 {
        if !SandGrid::in_bounds(x, y) {
            // Off the sides and bottom is wall; above the top is open sky.
            return if y >= GRID_H as isize { 0 } else { UNREACHABLE };
        }
        self.dist[y as usize * GRID_W + x as usize]
    }

    #[inline]
    pub fn surface_at(&self, x: usize) -> u16 {
        self.surface[x.min(GRID_W - 1)]
    }

    /// Somewhere a hauled grain can actually be put down: standing on the ground, in a
    /// column whose ground is its own top surface rather than the roof of a tunnel.
    ///
    /// This is deliberately judged from the single column and nothing else, and both
    /// halves of it were learned the hard way.
    ///
    /// Without the lower bound, an ant on the lip of the entrance shaft counts as
    /// outside — it does have open sky above it — but the grain it drops falls straight
    /// back down the hole. A thousand hauling trips netted about a hundred cells of nest.
    ///
    /// Judging it against the *envelope* instead over-corrects: on a spoil mound only the
    /// apex column satisfies it, so every hauler funnels into one cell, jams, and digging
    /// stops dead. Asking each column about itself gives a whole surface to dump on, and
    /// still refuses the shaft.
    #[inline]
    pub fn is_dump_site(&self, x: usize, y: usize) -> bool {
        let ground = self.raw_surface[x.min(GRID_W - 1)];
        let y = y as u16;
        y > ground && y <= ground + 2
    }

    /// Downhill step toward open sky, or `None` at a dead end.
    pub fn descend(&self, x: usize, y: usize) -> Option<Vec2> {
        let here = self.at(x as isize, y as isize);
        if here == UNREACHABLE {
            return None;
        }
        let mut best = here;
        let mut dir = Vec2::ZERO;
        for (dx, dy) in NEIGHBOURS_8 {
            let d = self.at(x as isize + dx, y as isize + dy);
            if d < best {
                best = d;
                dir = Vec2::new(dx as f32, dy as f32);
            }
        }
        (dir != Vec2::ZERO).then(|| dir.normalize())
    }
}

impl Default for NavField {
    fn default() -> Self {
        Self::new()
    }
}

pub const NEIGHBOURS_8: [(isize, isize); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

pub fn rebuild_nav_field(mut nav: ResMut<NavField>, grid: Res<SandGrid>) {
    // Nothing has moved since the last flood, so there is nothing to redo. This is what
    // lets an untouched farm cost essentially no CPU while it sits in a window.
    if nav.built_epoch == grid.epoch {
        return;
    }
    nav.built_epoch = grid.epoch;

    let NavField { dist, surface, raw_surface, queue, .. } = &mut *nav;
    dist.fill(UNREACHABLE);
    queue.clear();

    for x in 0..GRID_W {
        raw_surface[x] = 0;
        for y in (0..GRID_H).rev() {
            if !grid.is_air(x as isize, y as isize) {
                raw_surface[x] = y as u16;
                break;
            }
        }
        // Seed from the open top row. Everything above the sand is connected, so one
        // flood covers the sky, then descends into whatever tunnels reach it — no need
        // to track where the entrances are, or how many there are.
        let top = GRID_H - 1;
        if grid.is_air(x as isize, top as isize) {
            dist[top * GRID_W + x] = 0;
            queue.push_back((top * GRID_W + x) as u32);
        }
    }

    // Envelope: a shaft narrower than the window can't masquerade as open ground.
    for x in 0..GRID_W {
        let lo = x.saturating_sub(SURFACE_ENVELOPE);
        let hi = (x + SURFACE_ENVELOPE).min(GRID_W - 1);
        surface[x] = raw_surface[lo..=hi].iter().copied().max().unwrap_or(0);
    }

    while let Some(i) = queue.pop_front() {
        let i = i as usize;
        let (x, y) = ((i % GRID_W) as isize, (i / GRID_W) as isize);
        let next = dist[i] + 1;
        for (dx, dy) in NEIGHBOURS_8 {
            let (nx, ny) = (x + dx, y + dy);
            if !SandGrid::in_bounds(nx, ny) || !grid.is_air(nx, ny) {
                continue;
            }
            let ni = ny as usize * GRID_W + nx as usize;
            if next < dist[ni] {
                dist[ni] = next;
                queue.push_back(ni as u32);
            }
        }
    }
}
