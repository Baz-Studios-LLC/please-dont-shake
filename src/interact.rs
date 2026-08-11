//! The three verbs — well, two of them for now, since feeding arrives with the ants.
//!
//! Tap and shake are deliberately the *same gesture*. Click and release without moving
//! and you tapped the glass. Move while held and you're shaking the tank. There is no
//! mode, no button, no menu — just a continuous slope between the harmless thing and
//! the forbidden one. The game asks you not to shake. It says nothing about tapping.
//!
//! Right-drag is the M1 debug brush: carve tunnels by hand so we can watch whether
//! they hold when calm and collapse when shaken. The ants take this job in M2.

use crate::grid::*;
use crate::pheromones::{Ph, Pheromones};
use crate::radial::{PlacementQueue, RadialMenu, Stock, commit_selection, wedge_at};
use crate::tank::{CAM_DIST, TankRoot, TankSpring};
use bevy::prelude::*;

/// Pointer travel, in pixels, below which a click counts as a tap rather than a shake.
const TAP_SLOP_PX: f32 = 6.0;

/// How much a tap stirs up the sand locally. `agitate` falls off quadratically from
/// the centre, so the value that actually lands is roughly half this — enough to push
/// the stability threshold just under 2, which sheds grains that are hanging off a
/// wall by a single neighbour while leaving every real ceiling standing.
///
/// This has to stay *below* what a gentle shake does. A tap that caves in chambers
/// would collapse the whole design: the game asks you not to shake, so tapping has to
/// stay the innocent thing you do instead. Its real payload is the ants' alarm
/// response in M2, not damage to the sand.
const TAP_AGITATION: f32 = 0.42;
const TAP_RADIUS_CELLS: f32 = 26.0;
/// Peak alarm from a tap. Comfortably over the ants' panic threshold, so the colony
/// reacts even though the sand barely does — the tap's real payload is behavioural.
const TAP_ALARM: f32 = 1.4;
/// Peak alarm per second from a full-strength shake.
const SHAKE_ALARM_RATE: f32 = 2.4;

/// Shake response. Agitation accumulates while you're moving and decays when you stop.
///
/// Both figures are **per second**, not per frame. Agitation decays on the fixed
/// timestep but is added from the input path, which runs per rendered frame — so a
/// per-frame amount would make the same gesture roughly twice as destructive at 120fps
/// as at 60. That's not a subtle difference: it turned a moderate shake into a
/// near-total collapse when first run against a release build.
const SHAKE_DEADZONE: f32 = 2.0;
const SHAKE_TO_AGITATION: f32 = 0.132;
const SHAKE_AGITATION_MAX_RATE: f32 = 3.6;
const TILT_FROM_DRAG: f32 = 2.2;

const DIG_RADIUS_CELLS: f32 = 4.5;

/// How long the button must be held still before the radial menu opens. Short enough not
/// to feel like a wait, long enough that an ordinary tap never triggers it.
const HOLD_TO_OPEN: f32 = 0.28;

#[derive(Resource, Default)]
pub struct PointerState {
    last_cursor: Option<Vec2>,
    /// Where the press landed, in screen pixels — the radial menu's centre.
    press_cursor: Vec2,
    /// Grid coordinates where the press landed, if it landed on the tank at all.
    press_cell: Option<Vec2>,
    drag_px: f32,
    held: f32,
}

/// Vertical world units visible at the tank's depth, given Bevy's default 45° fov.
fn visible_height() -> f32 {
    2.0 * CAM_DIST * (std::f32::consts::FRAC_PI_4 * 0.5).tan()
}

/// Cursor → tank-local space, by intersecting the tank's own front plane. Doing it in
/// the tank's frame rather than the world's keeps the dig brush accurate while the
/// tank is mid-lurch.
fn cursor_to_tank_local(
    camera: &Camera,
    cam_tf: &GlobalTransform,
    tank_tf: &GlobalTransform,
    cursor: Vec2,
) -> Option<Vec3> {
    let ray = camera.viewport_to_world(cam_tf, cursor).ok()?;
    let plane_origin = tank_tf.transform_point(Vec3::new(0.0, 0.0, SLAB_DEPTH * 0.5));
    let hit = ray.plane_intersection_point(plane_origin, InfinitePlane3d::new(tank_tf.back()))?;
    Some(tank_tf.to_matrix().inverse().transform_point3(hit))
}

fn cell_of(local: Vec3) -> Vec2 {
    let (cx, cy) = SandGrid::world_to_cell(local);
    Vec2::new(cx, cy)
}

pub fn pointer_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    tank: Single<&GlobalTransform, With<TankRoot>>,
    mut state: ResMut<PointerState>,
    mut spring: ResMut<TankSpring>,
    mut grid: ResMut<SandGrid>,
    mut ph: ResMut<Pheromones>,
    mut menu: ResMut<RadialMenu>,
    mut stock: ResMut<Stock>,
    mut placements: ResMut<PlacementQueue>,
) {
    let (camera, cam_tf) = *camera;
    let tank_tf = *tank;
    let dt = time.delta_secs().max(1.0 / 240.0);

    let Some(cursor) = window.cursor_position() else {
        state.last_cursor = None;
        return;
    };

    let px_to_world = visible_height() / window.height().max(1.0);

    // ---- right button: debug dig brush -------------------------------------
    if mouse.pressed(MouseButton::Right)
        && let Some(local) = cursor_to_tank_local(camera, cam_tf, tank_tf, cursor)
    {
        let adding = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        brush(&mut grid, cell_of(local), adding);
    }

    // ---- left button: tap, shake, and the radial menu ----------------------
    //
    // All three share this button, split by what the hand does: release quickly and it's
    // a tap, move and it's a shake, hold still and the menu opens. Once the menu is up the
    // gesture is committed to it — dragging picks a wedge instead of shaking, so you can't
    // accidentally wreck the farm while choosing what to put in it.
    if mouse.just_pressed(MouseButton::Left) {
        state.drag_px = 0.0;
        state.held = 0.0;
        state.press_cursor = cursor;
        state.press_cell = cursor_to_tank_local(camera, cam_tf, tank_tf, cursor).map(cell_of);
        state.last_cursor = Some(cursor);
    }

    if mouse.pressed(MouseButton::Left) {
        state.held += dt;

        if menu.open {
            menu.selected = wedge_at(cursor - menu.origin);
        } else if state.drag_px < TAP_SLOP_PX
            && state.held >= HOLD_TO_OPEN
            && let Some(cell) = state.press_cell
        {
            menu.open = true;
            menu.origin = state.press_cursor;
            menu.cell = cell;
            menu.selected = None;
        } else if let Some(last) = state.last_cursor {
            let d_px = cursor - last;
            state.drag_px += d_px.length();

            // Screen-space delta, not tank-local — using the tank's own frame here
            // would be circular, since dragging is what moves the tank.
            let d_world = Vec3::new(d_px.x * px_to_world, -d_px.y * px_to_world, 0.0);

            // The tank follows your hand 1:1; the spring is what fights back.
            spring.offset += d_world;
            spring.vel = d_world / dt;
            spring.tilt_vel -= d_world.x * TILT_FROM_DRAG;

            apply_shake_agitation(&mut grid, &mut ph, spring.vel.length(), dt);
        }
        state.last_cursor = Some(cursor);
    }

    if mouse.just_released(MouseButton::Left) {
        if menu.open {
            commit_selection(&menu, &mut stock, &mut placements);
            menu.open = false;
            menu.selected = None;
        } else if state.drag_px < TAP_SLOP_PX && state.held < HOLD_TO_OPEN {
            if let Some(cell) = state.press_cell {
                tap(&mut grid, &mut ph, &mut spring, cell);
            }
        }
        state.last_cursor = None;
        state.press_cell = None;
        state.held = 0.0;
    }
}

/// How hard the tank is being shaken, expressed as agitation. Shared with the dev
/// capture harness so the scripted runs exercise the same tuning the player feels.
pub fn apply_shake_agitation(
    grid: &mut SandGrid,
    ph: &mut Pheromones,
    speed: f32,
    dt: f32,
) {
    if speed > SHAKE_DEADZONE {
        let rate = ((speed - SHAKE_DEADZONE) * SHAKE_TO_AGITATION).min(SHAKE_AGITATION_MAX_RATE);
        grid.agitate_all(rate * dt);
        // The shake lands on the colony's nervous system, not just its architecture.
        // Alarm floods every cell at once and then calms chemically over a minute or so.
        ph.deposit_everywhere(Ph::Alarm, (rate / SHAKE_AGITATION_MAX_RATE) * SHAKE_ALARM_RATE * dt);
    }
}

/// A knuckle on the glass. Local, brief, and structurally harmless — but the ants will
/// have opinions about it later.
pub fn tap(grid: &mut SandGrid, ph: &mut Pheromones, spring: &mut TankSpring, cell: Vec2) {
    grid.agitate(cell.x, cell.y, TAP_RADIUS_CELLS, TAP_AGITATION);

    // The part that matters. Ants sense substrate vibration through subgenual organs in
    // their legs, so a knuckle on the glass is a signal in a channel they genuinely
    // have — and they read it, correctly, as a predator. Barely touches the sand;
    // everyone nearby stops what they were doing.
    ph.deposit_disc(Ph::Alarm, cell.x, cell.y, TAP_RADIUS_CELLS * 0.8, TAP_ALARM);

    // The tank flinches away from the finger and rocks a little, biased by where you
    // hit it — a tap near the edge torques it more than one in the middle.
    let off_centre = (cell.x / GRID_W as f32) - 0.5;
    spring.vel += Vec3::new(off_centre * 1.1, 0.0, -1.3);
    spring.tilt_vel -= off_centre * 1.6;
}

/// M1 debug brush. `adding` refills with sand of the stratum that belongs at that
/// height, so hand-carved test tunnels stay consistent with the layering.
fn brush(grid: &mut SandGrid, cell: Vec2, adding: bool) {
    let r = DIG_RADIUS_CELLS;
    let min_x = ((cell.x - r).floor().max(0.0)) as usize;
    let max_x = ((cell.x + r).ceil().min(GRID_W as f32 - 1.0)).max(0.0) as usize;
    let min_y = ((cell.y - r).floor().max(0.0)) as usize;
    let max_y = ((cell.y + r).ceil().min(GRID_H as f32 - 1.0)).max(0.0) as usize;

    if cell.x < -r || cell.y < -r || cell.x > GRID_W as f32 + r || cell.y > GRID_H as f32 + r {
        return;
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(cell);
            if d > r {
                continue;
            }
            if adding {
                if grid.get(x, y).mat == Substance::Air {
                    grid.set(x, y, Cell { mat: Substance::Sand, shade: shade_for(x, y) });
                }
            } else if grid.get(x, y).mat == Substance::Sand {
                grid.set(x, y, Cell::AIR);
            }
        }
    }
}

/// Which palette entry a grain at this height ought to be. Mirrors `fill_strata`.
fn shade_for(x: usize, y: usize) -> u8 {
    let depth_frac = (y as f32 / INITIAL_SURFACE as f32).clamp(0.0, 1.0);
    let stratum = ((1.0 - depth_frac) * STRATA as f32).clamp(0.0, (STRATA - 1) as f32) as usize;
    let variant = (hash01(x as u32, y as u32, 0x5EED) * VARIANTS as f32) as usize;
    (stratum * VARIANTS + variant.min(VARIANTS - 1)) as u8
}
