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
    /// How crowded it is here: laid down continuously by every ant and every brood item, and
    /// read by anybody deciding whether to dig.
    ///
    /// This is the field that tells excavation when to stop, and the reason it can is the
    /// no-flux boundary in [`diffuse_pheromones`]. A signal cannot cross sand, so in a cramped
    /// chamber it has nowhere to go and builds up, while in a roomy nest it spreads thin. The
    /// concentration therefore *is* bodies per unit of open space, which is what crowding means
    /// — and it needs no census, no radius search and no per-ant neighbour list.
    ///
    /// Real ants do this. Excavation in *Lasius* responds to worker density and to the CO₂ that
    /// accumulates where a colony is packed into too little room; a nest stops growing when it
    /// is big enough for the colony in it, which is the behaviour three separate faults in this
    /// game were missing.
    Crowd = 3,
}

pub const PH_LAYERS: usize = 4;
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
    // Crowd — reach ~8 cells, and it has to be short. A long reach would let one busy chamber
    // license digging across the whole nest, which is the unconditional digging this replaces.
    // Evaporation is fast enough that the field is "who is here now" rather than a history:
    // a chamber emptied by a cohort growing up stops reading as crowded within a minute.
    (0.12, 0.030),
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

    /// Is this cell at least as marked as everything touching it?
    ///
    /// The test that makes work attract work actually work. A digger standing on sand bites what
    /// it walks into, and it is *always* standing on sand — so with a downward bias it bit the
    /// ground under its feet the instant its wait elapsed, wherever it happened to be. Measured
    /// at 110 ants over 45 minutes: forty columns of shallow scrapes, no shaft, no entrance the
    /// nav flood could even recognise, and two thirds of everything dug dropped back inside
    /// because there was no outside to carry it to.
    ///
    /// Requiring a local maximum turns the gradient into somewhere to *go*. An ant with a
    /// stronger mark beside it walks there instead of biting; an ant at the face is at the peak
    /// and bites; and an ant on unmarked ground is trivially at a maximum, so a colony tipped
    /// onto flat sand can still start a hole. Nothing is routed and no layout is prescribed —
    /// it is the same gradient, read as a destination rather than as a mood.
    pub fn at_local_max(&self, layer: Ph, x: usize, y: usize) -> bool {
        let here = self.get(layer, x, y);
        !NEIGHBOURS_8.iter().any(|(dx, dy)| {
            self.at(layer, x as isize + dx, y as isize + dy) > here
        })
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

/// How much slower the `Dig` layer lives than the other three, as a factor on both its
/// diffusion and its evaporation.
///
/// `Dig` is not a chemical the way `Alarm` is. It is the colony's memory of its own labour, and
/// a memory of labour has to outlast the gap between the events it records or it cannot
/// accumulate. That gap used to be 0.45 seconds and is now 30,000 — so a field tuned to
/// remember for a hundred seconds went from holding two hundred of one ant's bites to holding
/// none of them, and "work attracts work" quietly stopped being true. Measured at 110 ants: the
/// colony cut *fourteen separate one-cell scrapes* across the surface, `heap 1x14`, with no
/// shaft anywhere and no cell of the nest wider than a corridor.
///
/// So it runs on the colony's clock like the labour it records: the factor is the clock's rate
/// over the rate this was tuned at. Both numbers scale together on purpose — reach is
/// `sqrt(D · FIELD_HZ / evap)`, so scaling only the evaporation would flatten the field across
/// the whole tank and destroy the gradient that makes it useful. Slowing both keeps the shape
/// and stretches the timescale, which is exactly the intent.
fn dig_memory_scale(clock: &crate::ants::ColonyClock) -> f32 {
    // `1/86400` is real time, where a colony day takes a day. At that rate a bite is 30,000s
    // apart and the memory needs to be about a colony-day long.
    const TUNED_FOR_EVAP: f32 = 0.010;
    (clock.days_per_second as f32 / TUNED_FOR_EVAP).clamp(1.0e-6, 1.0)
}

pub fn diffuse_pheromones(
    mut ph: ResMut<Pheromones>,
    grid: Res<SandGrid>,
    clock: Res<crate::ants::ColonyClock>,
) {
    let dt = 1.0 / FIELD_HZ;
    let dig_scale = dig_memory_scale(&clock);
    let Pheromones { fields, scratch, active } = &mut *ph;

    for layer in 0..PH_LAYERS {
        if !active[layer] {
            continue;
        }
        let (mut d, mut evap) = PH_PARAMS[layer];
        if layer == Ph::Dig as usize {
            d *= dig_scale;
            evap *= dig_scale;
        }
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

/// How far below the ground on *both* sides a column has to sit before it counts as a
/// nest entrance.
///
/// Both sides is the whole trick. A column beside a spoil mound is also far below the
/// neighbourhood maximum, and calling that an entrance would spread entrance status across
/// the terrain until there was nowhere left to dump. A hole has high ground either side of
/// it; the flank of a mound only has it on one.
const MOUTH_DIP: u16 = 4;

/// Columns of clearance a hauler keeps between the entrance and where it drops its grain.
///
/// Sets how tall the spoil apron can grow before it reaches the hole, since a heap at
/// repose spreads about one column per cell of height.
const MOUND_CLEARANCE: usize = 7;

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
    /// Column of the nearest nest entrance, or `GRID_W` where there is none.
    ///
    /// Spoil has to be put down *clear of the hole it came out of*, and that needs an
    /// actual answer to "where is the hole". Loose sand rolls downhill, and an open shaft
    /// in flat ground is the lowest point for a long way — so a grain dropped on the lip
    /// goes straight back down it. Measured: digging netted 32 of its first 33 cells and
    /// only 87 of 231 by the time the mound had grown back to the mouth.
    nearest_mouth: Vec<u32>,
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
            nearest_mouth: vec![GRID_W as u32; GRID_W],
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

    /// How many columns from here to the nearest nest entrance. `GRID_W` if the farm has
    /// no entrance at all, which is true of an untouched tank.
    #[inline]
    pub fn mouth_clearance(&self, x: usize) -> usize {
        let x = x.min(GRID_W - 1);
        let mouth = self.nearest_mouth[x];
        if mouth >= GRID_W as u32 {
            return GRID_W;
        }
        x.abs_diff(mouth as usize)
    }

    /// Which way is away from the nearest entrance, or `None` when standing on it or when
    /// there isn't one. Haulers follow this so the spoil apron spreads outward instead of
    /// heaping up against the hole and slumping back in.
    #[inline]
    pub fn away_from_mouth(&self, x: usize) -> Option<f32> {
        let x = x.min(GRID_W - 1);
        let mouth = self.nearest_mouth[x];
        if mouth >= GRID_W as u32 {
            return None;
        }
        match x.cmp(&(mouth as usize)) {
            std::cmp::Ordering::Greater => Some(1.0),
            std::cmp::Ordering::Less => Some(-1.0),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// Somewhere a hauled grain can actually be put down: standing on the ground, in a
    /// column whose ground is its own top surface rather than the roof of a tunnel, and
    /// far enough from the entrance that the grain stays where it's put.
    ///
    /// All three parts were learned the hard way.
    ///
    /// Without the lower bound, an ant on the lip of the entrance shaft counts as
    /// outside — it does have open sky above it — but the grain it drops falls straight
    /// back down the hole. A thousand hauling trips netted about a hundred cells of nest.
    ///
    /// Judging it against the *envelope* instead over-corrects: on a spoil mound only the
    /// apex column satisfies it, so every hauler funnels into one cell, jams, and digging
    /// stops dead. Asking each column about itself gives a whole surface to dump on, and
    /// still refuses the shaft.
    ///
    /// The clearance is what the loose-sand model made necessary. Spoil is loose, so it
    /// rolls to its angle of repose — and a shaft in flat ground is the lowest point for
    /// a long way, so anything dropped within rolling distance ends up down it. This is
    /// also what real ants do: the spoil goes in a ring set back from the hole, which is
    /// why a *Lasius* nest has a crater rather than a plug.
    #[inline]
    pub fn is_dump_site(&self, x: usize, y: usize) -> bool {
        let ground = self.raw_surface[x.min(GRID_W - 1)];
        let y = y as u16;
        y > ground && y <= ground + 2 && self.mouth_clearance(x) >= MOUND_CLEARANCE
    }

    /// Step *inward*, toward the innermost air the nest has — or `None` if nothing adjacent is
    /// deeper.
    ///
    /// The mirror of [`Self::descend`], and it costs nothing extra because of what the flood
    /// already means: `dist` is how far a walk from open sky is, so climbing it is walking into
    /// the burrow. The deepest reachable cell is the innermost chamber, and the colony gets a
    /// route to it without anybody storing where "the chamber" is — there is no such variable,
    /// and there shouldn't be. It's the ants' diggings that say.
    pub fn deepen(&self, x: usize, y: usize) -> Option<Vec2> {
        let here = self.at(x as isize, y as isize);
        if here == UNREACHABLE {
            return None;
        }
        let mut best = here;
        let mut dir = Vec2::ZERO;
        for (dx, dy) in NEIGHBOURS_8 {
            let d = self.at(x as isize + dx, y as isize + dy);
            // `UNREACHABLE` is solid sand, not somewhere deep. Reading it as the deepest place
            // available would send anything following this straight into a wall.
            if d != UNREACHABLE && d > best {
                best = d;
                dir = Vec2::new(dx as f32, dy as f32);
            }
        }
        (dir != Vec2::ZERO).then(|| dir.normalize())
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    /// A tank filled to `fill`, with a one-cell shaft cut down the middle to `floor`.
    fn tank_with_a_shaft(fill: usize, floor: usize) -> SandGrid {
        let mut grid = SandGrid::new();
        for x in 0..GRID_W {
            for y in 0..fill {
                grid.set_raw(x, y, Cell { mat: Substance::Sand, shade: 0 });
            }
        }
        for y in floor..fill {
            grid.set(GRID_W / 2, y, Cell::AIR);
        }
        grid
    }

    fn flood(grid: SandGrid) -> NavField {
        let mut world = World::new();
        world.insert_resource(grid);
        world.insert_resource(NavField::new());
        world.run_system_once(rebuild_nav_field).unwrap();
        world.remove_resource::<NavField>().unwrap()
    }

    /// The claim `dig_memory_scale` makes, checked as arithmetic because checking it as behaviour
    /// is a four-hour colony run.
    ///
    /// Stigmergy needs the mark to still be there when the next ant arrives. One digger bites
    /// every `DIG_INTERVAL`, so a memory shorter than that interval cannot accumulate one ant's
    /// own work at all — which is what "work attracts work" is made of. This asserts the memory
    /// outlasts the gap, and that scaling it did not wreck the *reach*, since a field spread
    /// evenly across the tank carries no gradient and a gradient is the entire point.
    ///
    /// It also ties the two constants together: change `DIG_INTERVAL` without revisiting the
    /// field and this fails, which is the coupling that quietly broke when labour moved onto the
    /// colony clock and the field did not.
    #[test]
    fn the_dig_memory_outlasts_the_gap_between_bites() {
        let real_time = crate::ants::ColonyClock::default();
        let scale = dig_memory_scale(&real_time);

        let (d0, evap0) = PH_PARAMS[Ph::Dig as usize];
        let (d, evap) = (d0 * scale, evap0 * scale);

        let memory_secs = 1.0 / evap;
        assert!(
            memory_secs > crate::ants::DIG_INTERVAL,
            "the dig field forgets in {memory_secs:.0}s and a digger bites every {}s, so no ant \
             can ever reinforce its own work",
            crate::ants::DIG_INTERVAL,
        );

        let reach = |d: f32, evap: f32| (d * FIELD_HZ / evap).sqrt();
        let (before, after) = (reach(d0, evap0), reach(d, evap));
        assert!(
            (after - before).abs() < 0.001,
            "scaling changed the reach from {before:.1} cells to {after:.1}; a flat field has no \
             gradient to climb",
        );
        assert!(d < 0.25, "an explicit 4-neighbour stencil diverges above D = 0.25");
    }

    /// The queen's whole route is this function, so it has to lead *in* and then stop.
    ///
    /// Down the shaft is deeper by definition — the flood measures walking distance from open
    /// sky — and the bottom of it is the deepest air in the tank, where there is nowhere further
    /// to go. If this ever inverted, the queen would climb out of the nest to found her colony
    /// in the open air and every symptom would look like a rendering problem.
    #[test]
    fn the_inward_step_leads_down_the_shaft_and_stops_at_the_bottom() {
        let fill = 60;
        let floor = 20;
        let nav = flood(tank_with_a_shaft(fill, floor));
        let x = GRID_W / 2;

        // Partway down: the way inward is downward.
        let step = nav.deepen(x, 40).expect("mid-shaft has somewhere deeper to go");
        assert!(step.y < 0.0, "the inward step from mid-shaft pointed up: {step:?}");

        // The bottom: nowhere deeper, which is what tells the queen she has arrived.
        assert_eq!(nav.deepen(x, floor), None, "the shaft floor is not a dead end");

        // And the flood agrees about which end is which.
        assert!(
            nav.at(x as isize, floor as isize) > nav.at(x as isize, (fill - 1) as isize),
            "the bottom of the shaft is not deeper than its mouth",
        );
    }

    /// Solid sand is not somewhere deep. `UNREACHABLE` is `u16::MAX`, so a version of this that
    /// compared distances without excluding it would call every wall the deepest place in the
    /// tank and walk the queen into one.
    #[test]
    fn the_inward_step_never_points_into_sand() {
        let nav = flood(tank_with_a_shaft(60, 20));
        let x = GRID_W / 2;
        for y in [21, 30, 45, 58] {
            if let Some(step) = nav.deepen(x, y) {
                let (tx, ty) = ((x as f32 + step.x) as isize, (y as f32 + step.y) as isize);
                assert_ne!(
                    nav.at(tx, ty),
                    UNREACHABLE,
                    "from ({x}, {y}) the inward step pointed into solid sand",
                );
            }
        }
    }
}

pub fn rebuild_nav_field(mut nav: ResMut<NavField>, grid: Res<SandGrid>) {
    // Nothing has moved since the last flood, so there is nothing to redo. This is what
    // lets an untouched farm cost essentially no CPU while it sits in a window.
    if nav.built_epoch == grid.epoch {
        return;
    }
    nav.built_epoch = grid.epoch;

    let NavField { dist, surface, raw_surface, nearest_mouth, queue, .. } = &mut *nav;
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

    // Entrances: a column sunk well below the ground on *both* sides. See `MOUTH_DIP`.
    const NONE: u32 = GRID_W as u32;
    let highest = |lo: usize, hi: usize| raw_surface[lo..=hi].iter().copied().max().unwrap_or(0);
    for x in 0..GRID_W {
        // The columns against the glass have only one side, so they can't be judged.
        nearest_mouth[x] = if x > 0 && x + 1 < GRID_W {
            let left = highest(x.saturating_sub(SURFACE_ENVELOPE), x - 1);
            let right = highest(x + 1, (x + SURFACE_ENVELOPE).min(GRID_W - 1));
            if left.min(right) >= raw_surface[x] + MOUTH_DIP { x as u32 } else { NONE }
        } else {
            NONE
        };
    }

    // Nearest entrance to each column, in two linear sweeps — haulers read this every
    // tick, so it can't be a search. Both sweeps compare by absolute distance: after the
    // forward pass a column's answer may well lie to its left, so neither direction can
    // assume the sign.
    let mut spread = |xs: &mut dyn Iterator<Item = usize>, from: fn(usize) -> usize| {
        for x in xs {
            let candidate = nearest_mouth[from(x)];
            if candidate == NONE {
                continue;
            }
            let better = nearest_mouth[x] == NONE
                || (candidate as usize).abs_diff(x) < (nearest_mouth[x] as usize).abs_diff(x);
            if better {
                nearest_mouth[x] = candidate;
            }
        }
    };
    spread(&mut (1..GRID_W), |x| x - 1);
    spread(&mut (0..GRID_W - 1).rev(), |x| x + 1);

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
