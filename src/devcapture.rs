//! Development harness. Not part of the game.
//!
//! `--capture` runs a scripted sequence that carves a nest by hand, lets it sit, then
//! shakes the tank, taking screenshots at each stage. It exists to answer the one
//! question M1 has to get right: do tunnels hold their shape indefinitely when the
//! tank is left alone, and fall in when it isn't?
//!
//! F12 takes a screenshot at any time, capture mode or not.

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::grid::*;
use crate::tank::TankSpring;

/// Offscreen render target used by capture mode.
///
/// Capturing the window itself only works while the window is actually composited —
/// a locked or sleeping screen yields solid black frames, which looks exactly like a
/// broken renderer and cost an hour to tell apart. Rendering to our own texture makes
/// the harness independent of whatever the desktop is doing.
#[derive(Resource)]
pub struct CaptureTarget(pub Handle<Image>);

pub fn setup_offscreen_target(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    camera: Single<Entity, With<Camera3d>>,
) {
    let mut image = Image::new_target_texture(
        CAPTURE_W,
        CAPTURE_H,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    );
    // Screenshot readback copies out of this texture.
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;

    let handle = images.add(image);
    commands
        .entity(*camera)
        .insert(RenderTarget::Image(handle.clone().into()));
    commands.insert_resource(CaptureTarget(handle));
}

const CAPTURE_W: u32 = 1280;
const CAPTURE_H: u32 = 800;

pub fn capture_mode() -> bool {
    std::env::args().any(|a| a == "--capture")
}

/// `--sand-only` reruns the original M1 test with no colony in the tank, so the sand
/// numbers stay comparable to the ones recorded in DESIGN.md.
pub fn sand_only() -> bool {
    std::env::args().any(|a| a == "--sand-only")
}

pub fn colony_enabled() -> bool {
    !sand_only()
}

/// `--title-shot` grabs a single frame of the title screen and exits.
pub fn title_shot() -> bool {
    std::env::args().any(|a| a == "--title-shot")
}

pub fn run_title_shot(
    mut commands: Commands,
    time: Res<Time>,
    mut cap: ResMut<DevCapture>,
    target: Res<CaptureTarget>,
    mut exit: MessageWriter<AppExit>,
) {
    let prev = cap.t;
    cap.t += time.delta_secs();

    // The window, not the offscreen target: UI attaches to the camera drawing to the
    // window, so an offscreen grab shows the farm but never the menu.
    let _ = &target;
    if prev < 2.5 && cap.t >= 2.5 {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!("{}/title.png", cap.out_dir)));
    }
    if prev < 3.5 && cap.t >= 3.5 {
        exit.write(AppExit::Success);
    }
}

/// Air below the original fill line — everything the ants have hollowed out.
pub fn excavated_volume(grid: &SandGrid) -> usize {
    let mut n = 0;
    let surface = INITIAL_SURFACE;
    for y in 0..surface {
        for x in 0..GRID_W {
            if grid.get(x, y).mat == Substance::Air {
                n += 1;
            }
        }
    }
    n
}

#[derive(Resource)]
pub struct DevCapture {
    pub t: f32,
    pub out_dir: String,
    /// The farm as it stood right after digging, to measure later damage against.
    pub baseline: Option<Vec<bool>>,
    pub baseline_sand: usize,
}

impl DevCapture {
    pub fn new() -> Self {
        let out_dir = std::env::args()
            .skip_while(|a| a != "--out")
            .nth(1)
            .unwrap_or_else(|| ".".to_string());
        Self { t: 0.0, out_dir, baseline: None, baseline_sand: 0 }
    }
}

pub fn screenshot_hotkey(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut n: Local<u32>,
) {
    if keys.just_pressed(KeyCode::F12) {
        let path = format!("./screenshot-{}.png", *n);
        *n += 1;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
}

/// Timeline, in seconds. The point is the *gradient*: leave it alone, tap it, shake it
/// gently, then shake it properly, and check each step does proportionally more damage.
const T_DIG: f32 = 0.6;
const T_SHOT_DUG: f32 = 2.2;
const T_SHOT_HELD: f32 = 8.0;
const T_TAP: f32 = 8.5;
const T_SHOT_TAPPED: f32 = 11.0;
const T_MODERATE: (f32, f32) = (11.5, 12.5);
const T_SHOT_MODERATE: f32 = 15.0;
const T_VIOLENT: (f32, f32) = (15.5, 17.5);
const T_SHOT_VIOLENT: f32 = 16.7;
const T_SHOT_SETTLED: f32 = 22.0;
const T_QUIT: f32 = 22.7;

/// Amplitude and angular frequency of each scripted shake. Moderate peaks at ~8 world
/// units/s — a firm but unremarkable wobble; violent peaks around 22.
const MODERATE: (f32, f32) = (0.35, 24.0);
const VIOLENT: (f32, f32) = (0.85, 26.0);

pub fn run_capture(
    mut commands: Commands,
    time: Res<Time>,
    mut cap: ResMut<DevCapture>,
    mut grid: ResMut<SandGrid>,
    mut ph: ResMut<crate::pheromones::Pheromones>,
    mut spring: ResMut<TankSpring>,
    target: Res<CaptureTarget>,

    grains: Query<(), With<crate::grains::Grain>>,
    mut exit: MessageWriter<AppExit>,
) {
    let target = &target.0;
    let in_flight = grains.iter().count();
    let prev = cap.t;
    cap.t += time.delta_secs();
    let t = cap.t;

    let crossed = |mark: f32| prev < mark && t >= mark;

    if crossed(T_DIG) {
        carve_nest(&mut grid);
        cap.baseline = Some(grid.solid_mask());
        cap.baseline_sand = grid.sand_count();
        info!("dug test nest ({} sand cells)", cap.baseline_sand);
    }

    if crossed(T_SHOT_DUG) {
        shoot(&mut commands, &cap, &grid, in_flight, target, "01-dug");
    }
    if crossed(T_SHOT_HELD) {
        shoot(&mut commands, &cap, &grid, in_flight, target, "02-held-6s-later");
    }

    if crossed(T_TAP) {
        // Right on the roof of the big central chamber — the most fragile structure in
        // the nest, and so the honest place to check a tap does *something* but not much.
        crate::interact::tap(&mut grid, &mut ph, &mut spring, Vec2::new(128.0, 44.0));
        info!("tapped");
    }
    if crossed(T_SHOT_TAPPED) {
        shoot(&mut commands, &cap, &grid, in_flight, target, "03-after-tap");
    }

    for (window, (amp, freq)) in [(T_MODERATE, MODERATE), (T_VIOLENT, VIOLENT)] {
        if t >= window.0 && t < window.1 {
            let phase = (t - window.0) * freq;
            spring.offset.x = phase.sin() * amp;
            spring.vel.x = phase.cos() * amp * freq;
            spring.tilt_vel -= phase.cos() * amp * 4.0 * time.delta_secs() * 60.0;
            // Same path the pointer takes, so this tests the real tuning.
            crate::interact::apply_shake_agitation(&mut grid, &mut ph, spring.vel.length(), time.delta_secs());
        }
    }

    if crossed(T_SHOT_MODERATE) {
        shoot(&mut commands, &cap, &grid, in_flight, target, "04-after-moderate-shake");
    }
    if crossed(T_SHOT_VIOLENT) {
        shoot(&mut commands, &cap, &grid, in_flight, target, "05-mid-violent-shake");
    }
    if crossed(T_SHOT_SETTLED) {
        shoot(&mut commands, &cap, &grid, in_flight, target, "06-settled");
    }

    if crossed(T_QUIT) {
        exit.write(AppExit::Success);
    }
}

// ---------------------------------------------------------------------------
// Colony timeline
// ---------------------------------------------------------------------------

/// Let the colony dig for a hundred seconds, then find out what a tap and a shake do to
/// something the ants actually built. The digging stretch is long on purpose: coherent
/// architecture is a slow, cumulative result, and short runs just show scratches.
const C_SHOTS: [(f32, &str); 4] = [
    (2.0, "01-founded"),
    (25.0, "02-digging-25s"),
    (60.0, "03-digging-60s"),
    (100.0, "04-nest-100s"),
];
const C_TAP: f32 = 103.0;
const C_SHOT_TAP: f32 = 107.0;
const C_SHAKE: (f32, f32) = (110.0, 112.5);
const C_SHOT_SHAKE: f32 = 111.5;
const C_SHOT_SETTLED: f32 = 124.0;
const C_QUIT: f32 = 125.0;

pub fn run_colony_capture(
    mut commands: Commands,
    time: Res<Time>,
    mut cap: ResMut<DevCapture>,
    mut grid: ResMut<SandGrid>,
    mut ph: ResMut<crate::pheromones::Pheromones>,
    mut spring: ResMut<TankSpring>,
    target: Res<CaptureTarget>,
    grains: Query<(), With<crate::grains::Grain>>,
    ants: Query<&crate::ants::Ant>,
    stats: Res<crate::ants::ColonyStats>,
    mut exit: MessageWriter<AppExit>,
) {
    let target = &target.0;
    let in_flight = grains.iter().count();
    let alive = ants.iter().count();
    let carried = ants.iter().filter(|a| a.carrying.is_some()).count();

    let prev = cap.t;
    cap.t += time.delta_secs();
    let t = cap.t;
    let crossed = |mark: f32| prev < mark && t >= mark;

    // Baseline once the founding chamber exists but before the ants have done anything.
    if crossed(0.3) {
        cap.baseline_sand = grid.sand_count() + in_flight + carried;
        info!(
            "colony founded: {} ants, {} sand cells, {} cells pre-excavated",
            alive,
            cap.baseline_sand,
            excavated_volume(&grid)
        );
    }

    for (mark, name) in C_SHOTS {
        if crossed(mark) {
            shoot_colony(
                &mut commands, &cap, &grid, in_flight + carried, alive, carried, &stats, target, name,
            );
        }
    }

    if crossed(C_TAP) {
        // Right over the nest. Barely disturbs the sand; the colony should notice.
        crate::interact::tap(&mut grid, &mut ph, &mut spring, Vec2::new(128.0, 80.0));
        info!("tapped the glass");
    }
    if crossed(C_SHOT_TAP) {
        shoot_colony(
            &mut commands,
            &cap,
            &grid,
            in_flight + carried,
            alive,
            carried,

            &stats,
            target,
            "05-after-tap",
        );
    }

    if t >= C_SHAKE.0 && t < C_SHAKE.1 {
        let (amp, freq) = (0.85, 26.0);
        let phase = (t - C_SHAKE.0) * freq;
        spring.offset.x = phase.sin() * amp;
        spring.vel.x = phase.cos() * amp * freq;
        spring.tilt_vel -= phase.cos() * amp * 4.0 * time.delta_secs() * 60.0;
        crate::interact::apply_shake_agitation(
            &mut grid,
            &mut ph,
            spring.vel.length(),
            time.delta_secs(),
        );
    }

    if crossed(C_SHOT_SHAKE) {
        shoot_colony(
            &mut commands,
            &cap,
            &grid,
            in_flight + carried,
            alive,
            carried,

            &stats,
            target,
            "06-mid-shake",
        );
    }
    if crossed(C_SHOT_SETTLED) {
        shoot_colony(
            &mut commands,
            &cap,
            &grid,
            in_flight + carried,
            alive,
            carried,

            &stats,
            target,
            "07-settled",
        );
    }

    if crossed(C_QUIT) {
        exit.write(AppExit::Success);
    }
}

#[allow(clippy::too_many_arguments)]
fn shoot_colony(
    commands: &mut Commands,
    cap: &DevCapture,
    grid: &SandGrid,
    off_grid: usize,
    alive: usize,
    carried: usize,
    stats: &crate::ants::ColonyStats,
    target: &Handle<Image>,
    name: &str,
) {
    // Excavation is now a third place sand can hide: in the grid, mid-flight as a
    // particle, or in an ant's mandibles. All three have to be counted or the mass
    // check silently passes while the farm leaks.
    let sand = grid.sand_count() + off_grid;
    let drift = sand as i64 - cap.baseline_sand as i64;
    info!(
        "{name}: excavated {} cells | {alive} ants, {carried} hauling | sand {sand} (drift {drift:+})",
        excavated_volume(grid)
    );
    info!(
        "    dug {} | dropped out {} / while-buried {} | inside {} | failed {} | now: {} diggers, {} buried, {} falling, {} panicking",
        stats.dug,
        stats.dropped_outside,
        stats.dropped_while_buried,
        stats.dropped_inside,
        stats.drop_failed,
        stats.diggers,
        stats.buried,
        stats.falling,
        stats.panicking,
    );

    commands
        .spawn(Screenshot::image(target.clone()))
        .observe(save_to_disk(format!("{}/{}.png", cap.out_dir, name)));
}

fn shoot(
    commands: &mut Commands,
    cap: &DevCapture,
    grid: &SandGrid,
    in_flight: usize,
    target: &Handle<Image>,
    name: &str,
) {
    // Two numbers matter here. `changed` is how much of the farm this stage rearranged
    // — the damage gradient from tap to shake. `sand` is the mass check: it must come
    // back to the baseline every time, because a farm that leaks a few grains per shake
    // would quietly empty itself over the days this game is meant to run for.
    let sand = grid.sand_count() + in_flight;
    let changed = match &cap.baseline {
        Some(base) => {
            let now = grid.solid_mask();
            base.iter().zip(&now).filter(|(a, b)| a != b).count()
        }
        None => 0,
    };
    let drift = sand as i64 - cap.baseline_sand as i64;
    info!(
        "{name}: {changed} cells changed vs dug, sand {sand} (drift {drift:+}, {in_flight} in flight)"
    );

    commands
        .spawn(Screenshot::image(target.clone()))
        .observe(save_to_disk(format!("{}/{}.png", cap.out_dir, name)));
}

/// A hand-carved nest roughly the shape a real *Lasius* colony digs: entrance shafts
/// from the surface, widening chambers at depth, side galleries off the main runs.
/// The ants take this job in M2; here it's a test fixture for the cohesion model.
fn carve_nest(grid: &mut SandGrid) {
    let surface = (GRID_H as f32 * 0.62) as isize;

    // Three entrance shafts down from the surface.
    for (sx, depth) in [(52isize, 74isize), (128, 88), (196, 62)] {
        for d in 0..depth {
            let y = surface - 4 - d;
            // Shafts wander rather than dropping straight, like a real one.
            let wobble = ((d as f32) * 0.13).sin() * 6.0;
            let cx = sx + wobble as isize;
            carve_disc(grid, cx, y, 2.6);
        }
    }

    // Chambers at depth, connected by horizontal galleries.
    for (cx, cy, rx, ry) in [
        (52isize, surface - 44, 11.0f32, 5.0f32),
        (128, surface - 62, 15.0, 7.0),
        (196, surface - 40, 10.0, 4.5),
        (90, surface - 84, 13.0, 6.0),
    ] {
        carve_ellipse(grid, cx, cy, rx, ry);
    }

    // Galleries linking them.
    for (x0, y0, x1, y1) in [
        (52isize, surface - 44, 128isize, surface - 62),
        (128, surface - 62, 196, surface - 40),
        (128, surface - 62, 90, surface - 84),
    ] {
        carve_line(grid, x0, y0, x1, y1, 2.4);
    }
}

fn carve_disc(grid: &mut SandGrid, cx: isize, cy: isize, r: f32) {
    carve_ellipse(grid, cx, cy, r, r);
}

fn carve_ellipse(grid: &mut SandGrid, cx: isize, cy: isize, rx: f32, ry: f32) {
    let x0 = (cx - rx as isize - 1).max(0);
    let x1 = (cx + rx as isize + 1).min(GRID_W as isize - 1);
    let y0 = (cy - ry as isize - 1).max(0);
    let y1 = (cy + ry as isize + 1).min(GRID_H as isize - 1);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = (x - cx) as f32 / rx;
            let dy = (y - cy) as f32 / ry;
            if dx * dx + dy * dy <= 1.0 {
                grid.set(x as usize, y as usize, Cell::AIR);
            }
        }
    }
}

fn carve_line(grid: &mut SandGrid, x0: isize, y0: isize, x1: isize, y1: isize, r: f32) {
    let steps = ((x1 - x0).abs().max((y1 - y0).abs())) as f32;
    let n = steps.max(1.0) as isize;
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let x = x0 + ((x1 - x0) as f32 * f) as isize;
        let y = y0 + ((y1 - y0) as f32 * f) as isize;
        carve_disc(grid, x, y, r);
    }
}
