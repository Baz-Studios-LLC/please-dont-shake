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
use crate::pheromones::{Ph, Pheromones};
use crate::tank::TankSpring;
use crate::title::GameState;

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
    camera: Single<Entity, With<crate::tank::TankCamera>>,
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

#[allow(dead_code)] // kept beside sand_only: the colony is placed by the harness now.
pub fn colony_enabled() -> bool {
    !sand_only()
}

/// `--title-shot` grabs a single frame of the title screen and exits.
pub fn title_shot() -> bool {
    std::env::args().any(|a| a == "--title-shot")
}

/// `--splash-shot` sits through the studio's mark and grabs three frames of it.
///
/// One frame can't verify a fade. Three — climbing, held, leaving — is the smallest set
/// that shows the mark is animating rather than just being present, which is the failure
/// mode Divus Factus actually had.
pub fn splash_shot() -> bool {
    std::env::args().any(|a| a == "--splash-shot")
}

/// `--wheel-shot` opens the radial menu over the farm and photographs it.
///
/// The wheel is the one piece of chrome that has to be judged *against the sand* — it's
/// translucent, it sits over the tank, and its lit rim is a colour question. The colony
/// run drives the menu too, but only as a regression guard against the spawn panic: its
/// frames go to the offscreen texture, where no UI exists at all.
pub fn wheel_shot() -> bool {
    std::env::args().any(|a| a == "--wheel-shot")
}

/// `--hand-shot` puts the hand on the glass and photographs each of its poses.
///
/// It has to fake the pointer. An unattended run has no cursor position at all — the window
/// is never focused — so `Touch` is written straight here, downstream of the system that
/// normally fills it in. Which is the same seam a touchscreen would use, so faking it is
/// honest rather than a special case.
pub fn hand_shot() -> bool {
    std::env::args().any(|a| a == "--hand-shot")
}

/// `--settings-shot` opens the settings window and photographs each tab.
///
/// Tabs are the one piece of chrome where "it built" and "it works" are different claims:
/// a strip that never switches looks identical in a single frame to one that does.
pub fn settings_shot() -> bool {
    std::env::args().any(|a| a == "--settings-shot")
}

/// `--panel-shot` opens the dev panel over a live farm and photographs it.
///
/// The panel is a block of text over the tank, which makes it the one piece of chrome where a
/// missing glyph is invisible to every test that isn't a picture: Bevy's embedded font stops at
/// U+007E, so an em dash renders as a box and nothing in the type system minds. That is exactly
/// how it shipped.
pub fn panel_shot() -> bool {
    std::env::args().any(|a| a == "--panel-shot")
}

pub fn run_panel_shot(
    mut commands: Commands,
    time: Res<Time>,
    mut cap: ResMut<DevCapture>,
    mut dev: ResMut<crate::devpanel::DevPanel>,
    mut placements: ResMut<crate::radial::PlacementQueue>,
    mut speed: ResMut<ColonySpeed>,
    mut clock: ResMut<crate::ants::ColonyClock>,
    mut exit: MessageWriter<AppExit>,
) {
    let prev = cap.t;
    cap.t += time.delta_secs();
    let crossed = |at: f32| prev < at && cap.t >= at;

    // Fast, so the readout has brood and diggings in it rather than an empty tank.
    if prev == 0.0 {
        clock.days_per_second = CAPTURE_DAYS_PER_SECOND;
        speed.0 = SPEEDS.len() - 1;
    }
    // A colony. On a *threshold*, not on `prev == 0.0`, and that distinction cost a debugging
    // detour: Bevy's first `Update` has a delta of zero, so `cap.t` is still 0.0 on the second
    // frame and `prev == 0.0` fires twice. Two kits, two queens — the panel opened and said
    // "queen 2 of them (want 1)", which is the tool catching the bug in the code that built it.
    if crossed(0.3) {
        let drop_at = Vec2::new(GRID_W as f32 * 0.5, (INITIAL_SURFACE + 2) as f32);
        placements.0.push((crate::radial::StockItem::AntKit, drop_at));
    }
    if crossed(1.0) {
        dev.open = true;
    }
    // Twice: once as it opens on a young colony, once after the farm has been dug into, so the
    // numbers in the second frame are ones that had to be computed rather than defaults.
    if crossed(1.6) {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!("{}/panel-1-open.png", cap.out_dir)));
    }
    if crossed(12.0) {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!("{}/panel-2-working.png", cap.out_dir)));
    }
    if crossed(13.0) {
        exit.write(AppExit::Success);
    }
}

const SETTINGS_SHOTS: [(f32, &str); 3] = [
    (1.4, "settings-1-video"),
    (2.4, "settings-2-audio"),
    (3.4, "settings-3-gameplay"),
];

pub fn run_settings_shot(
    mut commands: Commands,
    time: Res<Time>,
    mut cap: ResMut<DevCapture>,
    mut window: ResMut<crate::settings::SettingsWindow>,
    mut strips: Query<&mut ordo::tabs::Tabs>,
    settings: Res<crate::settings::Settings>,
    mut exit: MessageWriter<AppExit>,
) {
    let prev = cap.t;
    cap.t += time.delta_secs();
    let crossed = |at: f32| prev < at && cap.t >= at;

    if crossed(1.0) {
        window.open = true;
    }
    // Straight at the strip rather than through a synthetic click: what is being checked is
    // that a moved selection swaps the panes, not that Ordo's buttons report presses.
    if crossed(2.0) {
        for mut strip in &mut strips {
            strip.selected = 1;
        }
    }
    if crossed(3.0) {
        for mut strip in &mut strips {
            strip.selected = 2;
        }
        info!(
            "settings: fullscreen {} | music {} | shake {}",
            settings.fullscreen, settings.music, settings.shake
        );
    }

    for (at, name) in SETTINGS_SHOTS {
        if crossed(at) {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{}/{name}.png", cap.out_dir)));
        }
    }
    if crossed(4.2) {
        exit.write(AppExit::Success);
    }
}

/// Where the hand is put, in window pixels: over the sand, left of centre so the poses have
/// room to be seen against a plain stretch of strata.
const HAND_AT: Vec2 = Vec2::new(520.0, 470.0);
/// Half a second between setting a pose and taking its picture, so the easing has landed.
const HAND_SHOTS: [(f32, &str); 3] = [
    (1.2, "hand-1-open"),
    (2.2, "hand-2-pressing"),
    (3.4, "hand-3-grabbing"),
];

pub fn run_hand_shot(
    mut commands: Commands,
    time: Res<Time>,
    mut cap: ResMut<DevCapture>,
    mut touch: ResMut<crate::hand::Touch>,
    mut exit: MessageWriter<AppExit>,
) {
    let prev = cap.t;
    cap.t += time.delta_secs();
    let crossed = |at: f32| prev < at && cap.t >= at;

    // Drifting, then a fingertip, then a whole palm — the hand's entire vocabulary.
    touch.at = Some(HAND_AT);
    touch.pressing = cap.t >= 1.7 && cap.t < 2.9;
    touch.grabbing = cap.t >= 2.9;

    for (at, name) in HAND_SHOTS {
        if crossed(at) {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{}/{name}.png", cap.out_dir)));
        }
    }
    if crossed(4.2) {
        exit.write(AppExit::Success);
    }
}

/// Where the wheel is opened, in window pixels, and which cell that stands for.
const WHEEL_AT: Vec2 = Vec2::new(640.0, 430.0);
const WHEEL_OPEN: f32 = 1.5;
/// Two frames: nothing aimed at, then a wedge lit. The lit one is the colour test; the
/// unlit one is what says the rim is the only thing that changed.
const WHEEL_IDLE_SHOT: f32 = 2.2;
const WHEEL_AIM: f32 = 2.6;
const WHEEL_LIT_SHOT: f32 = 3.3;
const WHEEL_QUIT: f32 = 4.0;

pub fn run_wheel_shot(
    mut commands: Commands,
    time: Res<Time>,
    mut cap: ResMut<DevCapture>,
    mut menu: ResMut<crate::radial::RadialMenu>,
    mut exit: MessageWriter<AppExit>,
) {
    let prev = cap.t;
    cap.t += time.delta_secs();
    let crossed = |at: f32| prev < at && cap.t >= at;
    let mut shots: Vec<&str> = Vec::new();

    if crossed(WHEEL_OPEN) {
        menu.open = true;
        menu.origin = WHEEL_AT;
        menu.cell = Vec2::new(GRID_W as f32 * 0.5, INITIAL_SURFACE as f32 + 2.0);
        menu.selected = None;
    }
    if crossed(WHEEL_IDLE_SHOT) {
        shots.push("wheel-1-idle");
    }
    if crossed(WHEEL_AIM) {
        menu.selected = Some(0);
    }
    if crossed(WHEEL_LIT_SHOT) {
        shots.push("wheel-2-lit");
    }

    for name in shots {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!("{}/{name}.png", cap.out_dir)));
    }

    if crossed(WHEEL_QUIT) {
        exit.write(AppExit::Success);
    }
}

/// When the mark is grabbed, in seconds of splash time: mid fade-in, mid hold, mid
/// fade-out. The fade is 1.3s either side of a 1.8s hold.
const SPLASH_SHOTS: [(f32, &str); 3] = [
    (0.65, "splash-1-rising"),
    (2.2, "splash-2-held"),
    (3.75, "splash-3-leaving"),
];

pub fn run_splash_shot(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut cap: ResMut<DevCapture>,
    mut exit: MessageWriter<AppExit>,
) {
    let prev = cap.t;
    cap.t += time.delta_secs();

    // The window, not the offscreen target: the mark is UI, and UI draws to the camera
    // pointed at the window. An offscreen grab would show the farm the splash is hiding.
    for (at, name) in SPLASH_SHOTS {
        if prev < at && cap.t >= at {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{}/{name}.png", cap.out_dir)));
        }
    }

    if prev < 5.5 && cap.t >= 5.5 {
        exit.write(AppExit::Success);
    }
}

/// `--title-shot` walks the title screen through both of its states and out.
///
/// One frame of a fresh title screen used to be enough, and isn't any more: the menu now
/// says different things depending on whether a farm exists, keeps that farm running
/// behind itself, and fades rather than cuts. All three are things a single boot-state
/// screenshot cannot show.
const MENU_FRESH_SHOT: f32 = 1.6;
const MENU_START: f32 = 2.0;
/// Long enough for the kit to pour in and the colony to break ground, so the frame taken
/// at the title afterwards has something in it worth continuing.
const MENU_BACK: f32 = 34.0;
const MENU_CONTINUE_SHOT: f32 = 35.6;
const MENU_FADE: f32 = 36.0;
/// Mid-fade, at roughly half opacity. `TITLE_FADE` is half a second.
const MENU_FADING_SHOT: f32 = 36.25;
const MENU_PLAYING_SHOT: f32 = 37.5;
const MENU_QUIT: f32 = 38.5;

pub fn run_title_shot(
    mut commands: Commands,
    time: Res<Time>,
    mut cap: ResMut<DevCapture>,
    mut next: ResMut<NextState<GameState>>,
    state: Res<State<GameState>>,
    progress: Res<crate::farm::GameInProgress>,
    mut placements: ResMut<crate::radial::PlacementQueue>,
    ants: Query<(), With<crate::ants::Ant>>,
    buttons: Query<&crate::title::MenuAction>,
    mut exit: MessageWriter<AppExit>,
) {
    let prev = cap.t;
    cap.t += time.delta_secs();
    let crossed = |at: f32| prev < at && cap.t >= at;
    // Collected rather than spawned on the spot, so the closure doesn't hold `Commands`
    // borrowed for the whole body.
    let mut shots: Vec<&str> = Vec::new();

    if crossed(MENU_FRESH_SHOT) {
        let entries: Vec<String> = buttons.iter().map(|a| format!("{a:?}")).collect();
        info!("fresh title: in progress {} | buttons {:?}", progress.0, entries);
        shots.push("title-1-fresh");
    }

    if crossed(MENU_START) {
        next.set(GameState::Playing);
        // Tip a kit in, so there is a colony to come back to.
        let drop_at = Vec2::new(GRID_W as f32 * 0.5, (INITIAL_SURFACE + 2) as f32);
        placements.0.push((crate::radial::StockItem::AntKit, drop_at));
    }

    if crossed(MENU_BACK) {
        info!("leaving play with {} ants in the tank", ants.iter().count());
        next.set(GameState::Title);
    }

    if crossed(MENU_CONTINUE_SHOT) {
        let entries: Vec<String> = buttons.iter().map(|a| format!("{a:?}")).collect();
        info!(
            "title over a running farm: in progress {} | buttons {:?} | {} ants still digging",
            progress.0,
            entries,
            ants.iter().count(),
        );
        shots.push("title-2-continue");
    }

    // Straight at the resource rather than through a synthetic click: what's being
    // checked here is that the fade takes the menu away and hands over, not that
    // Ordo's buttons report presses — the observer does that.
    if crossed(MENU_FADE) {
        commands.init_resource::<crate::title::TitleFade>();
    }
    if crossed(MENU_FADING_SHOT) {
        shots.push("title-3-fading");
    }
    if crossed(MENU_PLAYING_SHOT) {
        info!(
            "after the fade: state {:?} | {} ants",
            state.get(),
            ants.iter().count()
        );
        shots.push("title-4-playing");
    }
    // The window, not the offscreen target — and see `shell_shot` in main for the other
    // half of that: these runs also have to *skip* setting the offscreen target up, or
    // the one camera renders into the texture and the window screenshot is solid black.
    for name in shots {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!("{}/{name}.png", cap.out_dir)));
    }

    if crossed(MENU_QUIT) {
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

/// Sand piled *above* the original fill line — the spoil heap.
///
/// Measured because "excavated" alone can't say where a missing cell went. Every grain the
/// colony digs out is either still in the mound, back in the hole, or in a mandible, and
/// only the first is progress. Mound plus excavated should track the dig count; when it
/// doesn't, spoil is coming back in and the difference is how much.
/// Open space, and how much of it is *room* rather than corridor.
///
/// A cell counts as room when it and its four neighbours are all air — the interior of something
/// at least three cells across. A one-cell tunnel scores zero however long it is, so "did the
/// colony build a chamber" becomes a number instead of my opinion of a screenshot. Which it has
/// been until now: every claim about nest shape in this project has been read off a picture.
pub fn open_and_room(grid: &SandGrid) -> (usize, usize) {
    let mut open = 0;
    let mut room = 0;
    // Below the original fill line only. Measured over the whole tank it reported `room 14986`
    // on a farm with no chambers in it whatsoever — the empty air above the sand is the roomiest
    // place in the box, and counting it drowns the nest by three orders of magnitude.
    for y in 1..INITIAL_SURFACE {
        for x in 1..GRID_W - 1 {
            if !grid.is_air(x as isize, y as isize) {
                continue;
            }
            open += 1;
            let walled = [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|(dx, dy)| !grid.is_air(x as isize + dx, y as isize + dy));
            if !walled {
                room += 1;
            }
        }
    }
    (open, room)
}

/// The crowding field, as the ants see it: the busiest cell, and what it reads where the queen is.
///
/// Printed so the threshold that gates digging can be chosen from measured numbers. Setting it by
/// eye would be picking the most load-bearing constant in the colony out of the air.
pub fn crowd_readings(ph: &Pheromones, queen: Option<Vec2>, ants: &[Vec2]) -> (f32, f32, f32) {
    let mut peak = 0.0f32;
    let mut total = 0.0f32;
    let mut cells = 0.0f32;
    for y in 0..GRID_H {
        for x in 0..GRID_W {
            let v = ph.get(Ph::Crowd, x, y);
            if v > 0.0 {
                peak = peak.max(v);
                total += v;
                cells += 1.0;
            }
        }
    }
    let read_at = |p: Vec2| {
        ph.get(
            Ph::Crowd,
            (p.x.max(0.0) as usize).min(GRID_W - 1),
            (p.y.max(0.0) as usize).min(GRID_H - 1),
        )
    };
    let at_queen = queen.map(read_at).unwrap_or(0.0);

    // What an ant actually feels, and how many would be let through by each candidate threshold.
    // The gate on digging is the most load-bearing constant in the colony; picking it by eye
    // would be picking it out of the air.
    let mut felt: Vec<f32> = ants.iter().map(|p| read_at(*p)).collect();
    felt.sort_by(f32::total_cmp);
    if !felt.is_empty() {
        let pick = |q: f32| felt[((felt.len() - 1) as f32 * q) as usize];
        let over = |t: f32| felt.iter().filter(|v| **v >= t).count() * 100 / felt.len();
        info!(
            "    felt by ants: median {:.2} | 75th {:.2} | 90th {:.2} | max {:.2} | share over 1/2/5/10: {}/{}/{}/{}%",
            pick(0.5),
            pick(0.75),
            pick(0.90),
            felt[felt.len() - 1],
            over(1.0),
            over(2.0),
            over(5.0),
            over(10.0),
        );
    }

    (peak, if cells > 0.0 { total / cells } else { 0.0 }, at_queen)
}

/// The spoil heap's shape: how tall the tallest column stands above the original fill line, and
/// how many columns carry any spoil at all.
///
/// Volume alone cannot tell a cone from a spire, and the spire is the failure `MOUND_HEADROOM`
/// exists to prevent. Tall over few columns is a tower; tall over many is a heap, which is what
/// a real farm looks like. Reported so that raising the cap is a measurement rather than a hope.
pub fn mound_profile(grid: &SandGrid) -> (usize, usize) {
    let mut tallest = 0;
    let mut wide = 0;
    for x in 0..GRID_W {
        let mut top = None;
        for y in (INITIAL_SURFACE..GRID_H).rev() {
            if grid.get(x, y).mat == Substance::Sand {
                top = Some(y);
                break;
            }
        }
        if let Some(top) = top {
            wide += 1;
            tallest = tallest.max(top + 1 - INITIAL_SURFACE);
        }
    }
    (tallest, wide)
}

pub fn mound_volume(grid: &SandGrid) -> usize {
    let mut n = 0;
    for y in INITIAL_SURFACE..GRID_H {
        for x in 0..GRID_W {
            if grid.get(x, y).mat == Substance::Sand {
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
        // Bevy's screenshot writer will not make the directory, and when it can't find one it
        // logs an IO error per frame and carries on — so a mistyped `--out` produces a run that
        // looks entirely successful and has no pictures in it. Cost me a whole run.
        if let Err(why) = std::fs::create_dir_all(&out_dir) {
            warn!("could not make {out_dir}: {why}");
        }
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

// ---------------------------------------------------------------------------
// Colony speed
// ---------------------------------------------------------------------------

/// Colony speeds, slowest first, with the words each one is announced in.
///
/// Real time is index zero and every run starts there, flag aside — a testing tool that could
/// be left switched on would eventually be left switched on, and the difference between this
/// game and a game is that its clock is honest.
///
/// **Two entries, and it used to be four.** Brett: "our dev speedup should never break the
/// simulation, so we can remove any speed that does." The two that were removed did.
///
/// Biology *and* labour are both scaled now — see [`crate::ants::ColonyClock::labour_scale`] — so
/// a fast-forward compresses the whole colony rather than only its calendar. What cannot be scaled
/// is walking: an ant at 86,400× would cross the tank inside a tick, on sand that steps at 60 Hz.
///
/// That sets a hard ceiling, and it is measurable rather than a matter of taste. The test is how
/// far an ant walks between bites, because that is what spreads its digging into galleries instead
/// of hollowing out wherever it happens to stand. A real *Lasius* digger walks on the order of
/// 28,000 cells per cell it excavates (100m a day at 1.2mm a cell, against three cells a day). At
/// real time this model gives 420,000 — sparse, erring the safe way. Scaled:
///
/// | speed | cells walked per cell dug | verdict |
/// |---|---|---|
/// | real time | 420,000 | faithful |
/// | a colony day an hour, 24× | 17,500 | faithful |
/// | a colony day a minute, 1,440× | 292 | 1/96 of real — blobs, not galleries |
/// | a colony day a second, 86,400× | 4.9 | 1/5,700 of real — hollows out where it stands |
///
/// So 24× is the ceiling, which is *exactly* the rate this game shipped at before the clock went
/// real. That rate was never arbitrary; it was the balance point, and two independent derivations
/// land on it — this one, and the fact that a bite cannot happen more than once per 60 Hz tick.
///
/// The cost is real and worth stating: a two-minute capture at 24× covers three hundredths of a
/// colony day, so the harness can no longer say anything about brood or founding. Those need long
/// runs at an honest speed. A fast number from a broken speed was worse than no number — three of
/// them sent me chasing a crowding brake that was working.
pub const SPEEDS: [(f64, &str); 2] = [
    (1.0 / 86_400.0, "real time — a colony day takes a day"),
    (1.0 / 3_600.0, "a colony day an hour"),
];

/// Where in [`SPEEDS`] the game is running. Deliberately not saved: see the table.
#[derive(Resource, Default)]
pub struct ColonySpeed(pub usize);

/// Step to the next speed up or down. Stops at both ends rather than wrapping, so `[` held
/// down always arrives at real time.
fn stepped(from: usize, faster: bool) -> usize {
    if faster { (from + 1).min(SPEEDS.len() - 1) } else { from.saturating_sub(1) }
}

/// `--speed <multiplier>`, where 1 is real time and 86400 is a colony day a second.
///
/// A multiplier rather than an index, because "a thousand times faster" is a thing you can
/// think, and `--speed 0.0000115` is not. Anything unparseable is ignored rather than fatal:
/// a mistyped flag on a test run should not be the reason the farm didn't open.
pub fn speed_flag() -> Option<f64> {
    std::env::args()
        .skip_while(|a| a != "--speed")
        .nth(1)?
        .parse::<f64>()
        .ok()
        .filter(|m| *m > 0.0)
}

/// Put the flag into effect, and leave the keys stepping from the nearest named speed.
pub fn apply_speed_flag(mut speed: ResMut<ColonySpeed>, mut clock: ResMut<crate::ants::ColonyClock>) {
    let Some(multiplier) = speed_flag() else {
        return;
    };
    let rate = multiplier / 86_400.0;
    clock.days_per_second = rate;
    speed.0 = SPEEDS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (a.0 - rate).abs().total_cmp(&(b.0 - rate).abs()))
        .map(|(index, _)| index)
        .unwrap_or(0);
    info!("colony speed: {multiplier}x real time");
}

/// `[` slower, `]` faster.
///
/// Ships in the release build, like F12 does, because release is the only build this game can
/// be tested in — a debug build of the sand automaton doesn't hold frame rate, so a
/// `debug_assertions` gate would put the tool where it can't be used. Undocumented in game,
/// announced in the terminal, and never remembered between runs.
pub fn speed_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut speed: ResMut<ColonySpeed>,
    mut clock: ResMut<crate::ants::ColonyClock>,
) {
    let faster = keys.just_pressed(KeyCode::BracketRight);
    let slower = keys.just_pressed(KeyCode::BracketLeft);
    if faster == slower {
        return;
    }

    let at = stepped(speed.0, faster);
    if at == speed.0 {
        return;
    }
    speed.0 = at;
    let (rate, name) = SPEEDS[at];
    clock.days_per_second = rate;
    info!("colony speed: {name}");
}

#[cfg(test)]
mod speed_tests {
    use super::*;

    /// Real time is the floor and the game starts there, the table only ever gets faster, and
    /// the top of it is the rate every capture run uses. All three are load-bearing: the first
    /// because a testing tool must not be capable of being left on, the second because `[` has
    /// to be "slower" at every step, and the third because it is the only rate with a hundred
    /// and twenty-five days of evidence behind it.
    #[test]
    fn the_table_starts_at_real_time_and_only_climbs() {
        // The ceiling is 24×, derived in the doc comment on `SPEEDS` from how far an ant walks
        // between bites. A faster entry is not a tuning choice; it is a broken instrument, and
        // three measurements were lost to one. Anything added here has to pass that arithmetic.
        assert!(
            SPEEDS[SPEEDS.len() - 1].0 <= 1.0 / 3_600.0,
            "a speed faster than a colony day an hour hollows the nest out instead of tunnelling it",
        );
        assert_eq!(ColonySpeed::default().0, 0, "a run must start at real time");
        assert_eq!(SPEEDS[0].0, 1.0 / 86_400.0, "index zero is not real time");
        assert_eq!(CAPTURE_DAYS_PER_SECOND, SPEEDS[SPEEDS.len() - 1].0);

        for pair in SPEEDS.windows(2) {
            assert!(pair[1].0 > pair[0].0, "the table is not sorted slowest first");
        }
    }

    /// Stepping stops at both ends. Wrapping would mean `]` at the top slams the colony back to
    /// real time, which reads as the key having done nothing at all.
    #[test]
    fn stepping_stops_at_both_ends() {
        let top = SPEEDS.len() - 1;
        assert_eq!(stepped(0, false), 0, "slower than real time");
        assert_eq!(stepped(0, true), 1);
        assert_eq!(stepped(top, true), top, "faster than the fastest");
        assert_eq!(stepped(top, false), top - 1);
    }

    /// The keys themselves, through the real system.
    ///
    /// `stepped` being right is not the same as the keys being wired to it: a mistyped `KeyCode`
    /// or a `pressed` where `just_pressed` belongs both look exactly like working code, and
    /// neither can be caught by pressing a key once and seeing the log line appear.
    #[test]
    fn the_keys_move_the_colony_clock() {
        let press = |key: KeyCode| {
            let mut app = App::new();
            let mut keys = ButtonInput::<KeyCode>::default();
            keys.press(key);
            app.insert_resource(keys)
                .init_resource::<ColonySpeed>()
                .init_resource::<crate::ants::ColonyClock>()
                .add_systems(Update, speed_keys);
            app.update();
            let world = app.world();
            (world.resource::<ColonySpeed>().0, world.resource::<crate::ants::ColonyClock>().days_per_second)
        };

        assert_eq!(press(KeyCode::BracketRight), (1, SPEEDS[1].0), "] did not speed the colony up");
        assert_eq!(press(KeyCode::BracketLeft), (0, SPEEDS[0].0), "[ moved below real time");
        assert_eq!(press(KeyCode::KeyP), (0, SPEEDS[0].0), "an unrelated key changed the clock");
    }

    /// The named speeds have to *be* what they are named. A colony day an hour is 24 days a day,
    /// and this is the check that a typed zero in the table can't quietly become a fortnight.
    #[test]
    fn each_speed_is_the_span_it_claims() {
        let days_in = |rate: f64, seconds: f64| rate * seconds;
        assert!((days_in(SPEEDS[0].0, 86_400.0) - 1.0).abs() < 1e-12, "a day a day");
        assert!((days_in(SPEEDS[1].0, 3_600.0) - 1.0).abs() < 1e-12, "a day an hour");
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
            // 1.0: the harness always shakes at the sensitivity every recorded number was
            // measured against, whatever the player's setting happens to be.
            crate::interact::apply_shake_agitation(
                &mut grid,
                &mut ph,
                spring.vel.length(),
                time.delta_secs(),
                1.0,
            );
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
// ---------------------------------------------------------------------------
// The congestion run
// ---------------------------------------------------------------------------

/// `--congestion` — a long run at an honest speed, for measuring spoil logistics near a
/// hundred ants.
///
/// It exists because the two-minute capture cannot reach the regime the question is about. The
/// speed table tops out at a colony day an hour, which is the fastest rate at which ants still
/// walk far enough between bites to tunnel rather than hollow (see [`SPEEDS`]), and at that rate
/// a 125-second run digs *one cell* with eleven ants in the tank. Every congestion figure this
/// project has quoted — 45% of everything dug falling back in, 83 drops inside the nest — came
/// off runs at a colony day a second, where biology ran 86,400× while ants dug at walking pace.
/// Those numbers describe a farm that never existed.
///
/// So: tip in kits until the tank holds the target headcount, then run at 24× and watch the
/// spoil ledger. Two honest limitations, stated because a fixture that hides them is worse than
/// none. The colony is *seeded* rather than grown, so it skips the demographic ramp and starts
/// on flat sand with a hundred ants that a real farm would have acquired over a month; and the
/// nest they dig is therefore younger than the colony working it. What it measures faithfully is
/// the thing asked about — of the grains this colony digs, where do they end up.
pub fn congestion_run() -> bool {
    std::env::args().any(|a| a == "--congestion")
}

/// How many ants to fill the tank with, and how long to run. Both flagged, because the useful
/// length depends on what is being asked: a ratio needs grains, and at 24× a hundred ants dig
/// about two cells a minute.
fn flag_value(name: &str, fallback: f32) -> f32 {
    std::env::args()
        .skip_while(|a| a != name)
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}

/// Seconds between ledger lines.
const CONGESTION_REPORT: f32 = 60.0;

pub fn run_congestion(
    mut commands: Commands,
    time: Res<Time>,
    mut cap: ResMut<DevCapture>,
    mut clock: ResMut<crate::ants::ColonyClock>,
    mut speed: ResMut<ColonySpeed>,
    mut placements: ResMut<crate::radial::PlacementQueue>,
    pour: Res<crate::ants::KitPour>,
    grid: Res<SandGrid>,
    stats: Res<crate::ants::ColonyStats>,
    brood: Res<crate::brood::BroodStats>,
    everyone: Query<&crate::ants::Ant>,
    grains: Query<(), With<crate::grains::Grain>>,
    mut exit: MessageWriter<AppExit>,
) {
    let target = flag_value("--ants", 100.0) as usize;
    let minutes = flag_value("--minutes", 60.0);

    let prev = cap.t;
    cap.t += time.delta_secs();
    let now = cap.t;
    let crossed = |at: f32| prev < at && now >= at;

    if prev == 0.0 {
        // The fastest speed that still tunnels. Never faster: the point of this run is that its
        // numbers are true, and the whole reason it has to be long is that honesty.
        clock.days_per_second = SPEEDS[SPEEDS.len() - 1].0;
        speed.0 = SPEEDS.len() - 1;
        info!("congestion run: filling to {target} ants, then {minutes:.0} minutes at 24x");
    }

    let alive = everyone.iter().count();
    let carried = everyone.iter().filter(|ant| ant.carrying.is_some()).count();

    // Fill the tank through the game's own stocking path — a kit at a time, only once the last
    // one has finished pouring. Nothing here spawns an ant directly, so the fixture cannot
    // disagree with what the radial menu does, and the one-queen guard in `pour_kit` turns every
    // kit after the first into eleven workers.
    if alive < target && pour.remaining == 0 && cap.t > 1.0 {
        let spread = (alive as f32 * 0.7) % 60.0 - 30.0;
        let at = Vec2::new(
            (GRID_W as f32 * 0.5 + spread).clamp(20.0, GRID_W as f32 - 20.0),
            (INITIAL_SURFACE + 2) as f32,
        );
        placements.0.push((crate::radial::StockItem::AntKit, at));
    }

    // The ledger. Every grain this colony has dug is in exactly one of these places, which is
    // what makes the line worth reading: `dug` is the denominator and the rest have to sum to it.
    let report = |at: f32, baseline: usize| {
        let in_flight = grains.iter().count();
        let sand = grid.sand_count() + in_flight + carried;
        let drift = sand as i64 - baseline as i64;
        let excavated = excavated_volume(&grid);
        let mound = mound_volume(&grid);
        let (open, room) = open_and_room(&grid);
        let (tallest, wide) = mound_profile(&grid);
        let dug = stats.dug.max(1) as i64;
        let pct = |n: u64| n as i64 * 100 / dug;
        info!(
            "{at:5.0}s | {alive} ants, {carried} hauling | dug {} = {} out ({}%), {} in ({}%), {} failed, {} while buried | excavated {excavated}, mound {mound} | heap {tallest}x{wide} | nest {open} open, {room} room | brood {} | drift {drift:+}",
            stats.dug,
            stats.dropped_outside,
            pct(stats.dropped_outside),
            stats.dropped_inside,
            pct(stats.dropped_inside),
            stats.drop_failed,
            stats.dropped_while_buried,
            brood.eggs + brood.larvae + brood.pupae,
        );
    };

    // Baseline once the tank is full, so drift is measured against a settled farm rather than
    // against one that is still having ants poured into it.
    if cap.baseline_sand == 0 && alive >= target {
        cap.baseline_sand = grid.sand_count() + grains.iter().count() + carried;
        info!("tank full at {alive} ants; baseline {} sand", cap.baseline_sand);
    }

    let step = (now / CONGESTION_REPORT).floor() * CONGESTION_REPORT;
    if step > 0.0 && crossed(step) {
        report(now, cap.baseline_sand);
    }

    if crossed(minutes * 60.0) {
        report(now, cap.baseline_sand);
        info!("congestion run complete");
        exit.write(AppExit::Success);
    }
    let _ = &mut commands;
}

const C_SHOTS: [(f32, &str); 4] = [
    (2.0, "01-founded"),
    (25.0, "02-digging-25s"),
    (60.0, "03-digging-60s"),
    (100.0, "04-nest-100s"),
];
/// Radial menu: open it, aim at the ant kit, release. This is how the farm gets stocked —
/// through the menu, the way a player does it, rather than by pushing a placement straight
/// into the queue.
///
/// It used to be both: a direct push at 0.2s *and* a menu commit at 30s to prove the menu
/// still worked. Wedge zero is the ant kit, so the second one tipped in a second colony —
/// with a second queen. `lay_eggs` and `tend_brood` ask for `queen.single()`, which fails on
/// two, so from 32s onward the run measured a colony that had silently stopped laying and
/// stopped tending its brood. Every "the colony collapses" reading came from here. One kit,
/// through the real path, once.
const C_MENU_OPEN: f32 = 0.1;
const C_MENU_PICK: f32 = 0.15;
const C_MENU_CLOSE: f32 = 0.2;

const C_TAP: f32 = 103.0;
const C_SHOT_TAP: f32 = 107.0;
const C_SHAKE: (f32, f32) = (110.0, 112.5);
const C_SHOT_SHAKE: f32 = 111.5;
const C_SHOT_SETTLED: f32 = 124.0;
const C_QUIT: f32 = 125.0;

/// Colony-days per real second for the scripted runs.
///
/// The fastest honest speed — see [`SPEEDS`] for why there is no faster one to reach for.
///
/// This used to be a colony day a *second*, and every colony number this project has published
/// came off runs at that rate. They measured a farm whose biology ran 86,400× while its ants dug
/// at walking pace: brood counts and populations from those runs are fiction, and the shapes are
/// worse than fiction because they were hollowed out rather than tunnelled.
///
/// At 24× a two-minute capture is three hundredths of a colony day. This run is therefore a sand
/// and locomotion test — mass conservation, stalling, spoil hauling, the collapse and the rebuild
/// — and it no longer pretends to be a demography test. Brood and founding need a long run.
const CAPTURE_DAYS_PER_SECOND: f64 = SPEEDS[SPEEDS.len() - 1].0;

pub fn run_colony_capture(
    mut commands: Commands,
    time: Res<Time>,
    mut cap: ResMut<DevCapture>,
    mut grid: ResMut<SandGrid>,
    mut ph: ResMut<crate::pheromones::Pheromones>,
    mut spring: ResMut<TankSpring>,
    target: Res<CaptureTarget>,
    live: Res<Census>,
    stats: Res<crate::ants::ColonyStats>,
    brood: Res<crate::brood::BroodStats>,
    mut clock: ResMut<crate::ants::ColonyClock>,
    mut menu: ResMut<crate::radial::RadialMenu>,
    mut stock: ResMut<crate::radial::Stock>,
    mut placements: ResMut<crate::radial::PlacementQueue>,
    everyone: Query<(&crate::ants::Ant, Has<crate::ants::Queen>)>,
    mut exit: MessageWriter<AppExit>,
) {
    let target = &target.0;
    let (in_flight, alive, carried) = (live.in_flight, live.ants, live.carrying);

    let prev = cap.t;
    cap.t += time.delta_secs();
    let ant_at: Vec<Vec2> = everyone.iter().map(|(ant, _)| ant.pos).collect();
    let queen_at = everyone
        .iter()
        .find_map(|(ant, is_queen)| is_queen.then_some(ant.pos));
    let t = cap.t;
    let crossed = |mark: f32| prev < mark && t >= mark;

    // Once, at the top, before anything is stocked.
    if prev == 0.0 {
        clock.days_per_second = CAPTURE_DAYS_PER_SECOND;
    }

    // Baseline once the founding chamber exists but before the ants have done anything.
    if crossed(0.5) {
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
                &mut commands, &cap, &grid, in_flight + carried, alive, carried, &stats, &brood,
                &live, &ph, queen_at, &ant_at, target,
                name,
            );
        }
    }

    // Stock the farm the way a player does: open the radial menu, point it at the ant kit,
    // release. Nothing is in the tank until someone places it — without this the whole run
    // measured an empty box and said so ("0 ants").
    //
    // Going through the menu rather than pushing into the placement queue means the run also
    // covers the path the player uses, including `commit_selection` spending the stock. The
    // offscreen target can't show UI, so this still proves nothing about how the menu *looks*
    // — but spawning it used to panic, and nothing else here would catch that.
    if crossed(C_MENU_OPEN) {
        menu.open = true;
        menu.origin = Vec2::new(640.0, 400.0);
        menu.cell = Vec2::new(GRID_W as f32 * 0.5, (INITIAL_SURFACE + 2) as f32);
        info!("opened the radial menu");
    }
    if crossed(C_MENU_PICK) {
        menu.selected = Some(0);
    }
    if crossed(C_MENU_CLOSE) {
        let placed = crate::radial::commit_selection(&menu, &mut stock, &mut placements);
        menu.open = false;
        menu.selected = None;
        info!("closed the radial menu, placed {placed:?}");
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
            &brood,
            &live,
            &ph,
            queen_at,
            &ant_at,
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
        // 1.0: the harness always shakes at the sensitivity every recorded number was
        // measured against, whatever the player's setting happens to be.
        crate::interact::apply_shake_agitation(
            &mut grid,
            &mut ph,
            spring.vel.length(),
            time.delta_secs(),
            1.0,
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
            &brood,
            &live,
            &ph,
            queen_at,
            &ant_at,
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
            &brood,
            &live,
            &ph,
            queen_at,
            &ant_at,
            target,
            "07-settled",
        );
    }

    if crossed(C_QUIT) {
        exit.write(AppExit::Success);
    }
}

/// What is actually in the tank right now, as opposed to what the counters last recorded.
///
/// A resource with its own pass rather than more queries on the report, for two reasons. The
/// colony capture was already at Bevy's sixteen-parameter ceiling — a seventeenth stops being a
/// system, and the error for that is `no method named 'before'` on the system itself, which
/// points nowhere near the cause. And a census is a thing the harness should be able to take
/// from anywhere.
#[derive(Resource, Default)]
pub struct Census {
    ants: usize,
    carrying: usize,
    in_flight: usize,
    queens: usize,
    brood: usize,
    held: usize,
    /// Workers that have not gone anywhere in [`STALL_WINDOW`] seconds, and how many of those
    /// are stranded above the roaming cap.
    ///
    /// `ColonyStats::walled_in` was supposed to be this and isn't: it counts an ant whose eight
    /// candidate steps were *all* refused on a single tick, which is a snapshot of a decision
    /// rather than a report on an outcome. It reads zero on runs where ants sit on a mound doing
    /// nothing for a minute, because each tick they believe they are about to move. "Has it
    /// moved?" is the only question that matches what you see through the glass.
    pub stalled: usize,
    pub stalled_high: usize,
    /// The stalled ones broken down by job, because "went nowhere" is not the same as "stuck".
    /// A nurse that has reached the brood is *supposed* to stay on it, and counting her as a
    /// fault would have me chasing the colony working correctly.
    pub stalled_by_job: [usize; 3],
    /// Workers that have gone nowhere for [`STUCK_WINDOWS`] samples running.
    ///
    /// This is the one that answers "are any ants actually stuck", and it exists because a single
    /// four-second sample cannot tell a pause from a prison. The instantaneous count swings
    /// between 1 and 32 on a healthy run, which is what an idle reserve looks like when you
    /// photograph it; a *sustained* count is a bug. Before the wander was given a clock this read
    /// nearly half the colony, every sample, for the whole run.
    pub stuck: usize,
}

/// Consecutive stalled samples before an ant is called stuck rather than idle. Three windows is
/// twelve seconds of having gone nowhere at all.
const STUCK_WINDOWS: u8 = 3;

/// How long an ant is given to get somewhere before it counts as stuck, and how far it has to
/// have gone. Four seconds of walking is fifty-odd cells; a cell and a half is nothing.
const STALL_WINDOW: f32 = 4.0;
const STALL_DISTANCE: f32 = 1.5;

/// Count what is in the tank. Before the report, every frame of a capture run.
pub fn take_census(
    mut census: ResMut<Census>,
    time: Res<Time>,
    mut watch: Local<std::collections::HashMap<Entity, (Vec2, u8)>>,
    mut since: Local<f32>,
    ants: Query<&crate::ants::Ant>,
    workers: Query<(Entity, &crate::ants::Ant), Without<crate::ants::Queen>>,
    queens: Query<(), With<crate::ants::Queen>>,
    pile: Query<&crate::brood::Brood>,
    grains: Query<(), With<crate::grains::Grain>>,
) {
    let (stalled, stalled_high, stalled_by_job, stuck) =
        (census.stalled, census.stalled_high, census.stalled_by_job, census.stuck);
    *census = Census {
        ants: ants.iter().count(),
        carrying: ants.iter().filter(|ant| ant.carrying.is_some()).count(),
        in_flight: grains.iter().count(),
        queens: queens.iter().count(),
        brood: pile.iter().count(),
        held: pile.iter().filter(|item| item.held_by.is_some()).count(),
        // Carried between samples, so the report always has the last full answer rather than a
        // zero on every frame that isn't a sampling frame.
        stalled,
        stalled_high,
        stalled_by_job,
        stuck,
    };

    *since += time.delta_secs();
    if *since < STALL_WINDOW {
        return;
    }
    *since = 0.0;

    let cap = INITIAL_SURFACE as f32 + crate::ants::MOUND_HEADROOM;
    let mut stalled = 0;
    let mut high = 0;
    let mut by_job = [0usize; 3];
    let mut stuck = 0;
    let mut next = std::collections::HashMap::with_capacity(watch.len());
    for (entity, ant) in &workers {
        // An ant nobody has seen before starts its streak at zero, which is right: it has not
        // yet had a window in which to fail to move.
        let (was, streak) = watch.get(&entity).copied().unwrap_or((ant.pos, 0));
        let went_nowhere = watch.contains_key(&entity) && ant.pos.distance(was) < STALL_DISTANCE;
        let streak = if went_nowhere { streak.saturating_add(1) } else { 0 };

        if went_nowhere {
            stalled += 1;
            if ant.pos.y > cap {
                high += 1;
            }
            by_job[match crate::ants::Job::for_age(ant.age_days) {
                crate::ants::Job::Nurse => 0,
                crate::ants::Job::Digger => 1,
                crate::ants::Job::Surface => 2,
            }] += 1;
        }
        if streak >= STUCK_WINDOWS {
            stuck += 1;
        }
        next.insert(entity, (ant.pos, streak));
    }
    census.stalled = stalled;
    census.stalled_high = high;
    census.stalled_by_job = by_job;
    census.stuck = stuck;
    *watch = next;
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
    brood: &crate::brood::BroodStats,
    live: &Census,
    ph: &Pheromones,
    queen_at: Option<Vec2>,
    ant_at: &[Vec2],
    target: &Handle<Image>,
    name: &str,
) {
    // Excavation is now a third place sand can hide: in the grid, mid-flight as a
    // particle, or in an ant's mandibles. All three have to be counted or the mass
    // check silently passes while the farm leaks.
    let sand = grid.sand_count() + off_grid;
    let drift = sand as i64 - cap.baseline_sand as i64;
    let excavated = excavated_volume(grid);
    let mound = mound_volume(grid);
    info!(
        "{name}: excavated {excavated} | mound {mound} | {alive} ants, {carried} hauling | sand {sand} (drift {drift:+})",
    );
    // Where the digging actually went. `dug` grains left the ground; each one is now in the
    // mound, back in the hole, or in a mandible. `returned` is the leak.
    let returned = stats.dug as i64 - excavated as i64 - carried as i64;
    info!(
        "    dug {} -> excavated {excavated}, returned {returned} ({}%) | mound holds {mound}",
        stats.dug,
        if stats.dug > 0 { returned * 100 / stats.dug as i64 } else { 0 },
    );
    // The two locomotion faults, measured rather than watched for. Both should read zero.
    info!(
        "    stuck now {} | at the glass {} | jobs: {} nurses, {} diggers, {} surface",
        stats.walled_in, stats.at_the_glass, stats.nurses, stats.diggers, stats.surface
    );
    // Population is to the brood what mass is to the sand: the one number that says whether
    // the thing is working. It should climb, and much later it should fall.
    info!(
        "    brood {} eggs, {} larvae, {} pupae ({} carried) | laid {} | eclosed {} | died {}",
        brood.eggs, brood.larvae, brood.pupae, brood.carried, brood.laid, brood.eclosed,
        brood.died
    );
    // From the world rather than from the counters above. A colony with no queen is the end of
    // the farm and has to be legible as that, not as a report that stopped changing.
    let (tallest, wide) = mound_profile(grid);
    let (open, room) = open_and_room(grid);
    let (peak, mean, at_queen) = crowd_readings(ph, queen_at, ant_at);
    info!(
        "    heap: {tallest} cells tall over {wide} columns | open {open}, of which room {room}",
    );
    info!("    crowd: peak {peak:.2} | mean {mean:.2} | at the queen {at_queen:.2}");
    info!(
        "    live: {} queens, {} brood ({} held) | went nowhere in {STALL_WINDOW}s: {} = {} nurses, {} diggers, {} surface ({} above the cap) | STUCK for {}s+: {}",
        live.queens,
        live.brood,
        live.held,
        live.stalled,
        live.stalled_by_job[0],
        live.stalled_by_job[1],
        live.stalled_by_job[2],
        live.stalled_high,
        STALL_WINDOW * STUCK_WINDOWS as f32,
        live.stuck,
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
