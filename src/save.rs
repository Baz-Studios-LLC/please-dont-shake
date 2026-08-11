//! Persistence. One farm, always there.
//!
//! There is no save button and there are no slots, because there is nothing to choose
//! between: the farm in the tank is the only farm. Closing the app puts it down; opening
//! the app picks it up where it was left, and **Continue** walks back into it. The player
//! never learns that a file is involved, which is the point — an ambient game that asked
//! you to remember to save it would be asking the wrong thing.
//!
//! Three moments write the file. Leaving play for the title screen, quitting, and every
//! [`AUTOSAVE_SECONDS`] in between. The last is the one that matters: a force-quit, a
//! crash or a flat battery are not the player's fault, and none of them send an exit
//! message.
//!
//! ## What isn't saved, and why
//!
//! The pheromone fields aren't. They're a chemical state with a half-life measured in
//! seconds — `Queen` is re-laid continuously by the queen sitting there, `Alarm` is zero
//! in a farm at rest, and `Dig` is re-deposited by the first bite of the next excavation.
//! The durable record of the colony's work isn't the chemistry, it's the shape of the
//! tunnels, and that is in the sand. Saving 480KB of floats to restore something that
//! re-establishes itself in a few seconds of play would be paying a real cost for nothing.
//!
//! The navigation flood isn't saved either, for a firmer reason: it is derived. The grid's
//! revision counter changes on load, so it rebuilds on the first tick without being asked.
//!
//! Grains **in flight** are saved, and they have to be. Sand in this game is conserved
//! exactly, and a grain mid-air is one that has been lifted out of the grid — drop it on
//! the floor of the save format and the tank quietly loses mass every time the player
//! closes the app during a shake.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ants::{Ant, AntAssets, ColonyClock, KitPour, Queen, body_bundle};
use crate::away::AwaySpan;
use crate::brood::{Brood, BroodAssets, Stage, brood_bundle};
use crate::farm::GameInProgress;
use crate::grains::Grain;
use crate::grid::*;
use crate::radial::Stock;
use crate::tank::TankRoot;

/// How often a farm being played is written down.
///
/// Thirty seconds of a colony's day is nothing — on a real-time clock it is thirty seconds of
/// one — so the worst case is losing a few grains of digging.
pub const AUTOSAVE_SECONDS: u64 = 30;

/// Bumped when the layout changes in a way an older file can't be read as. A file from
/// the future, or from a format we no longer understand, is ignored rather than guessed
/// at: starting fresh is a disappointment, and loading half a farm is a bug report.
const FORMAT: u32 = 1;

// ---------------------------------------------------------------------------
// Where it lives
// ---------------------------------------------------------------------------

/// The per-platform place a game is allowed to keep things.
///
/// Same shape as Divus Factus's, so everything the studio ships is findable in the same
/// place — though nothing here is shared with it, since that game keeps numbered slots a
/// player picks between and this one keeps a single farm nobody has to think about.
pub(crate) fn save_dir() -> std::path::PathBuf {
    // An override, so a harness run can be pointed somewhere harmless. The capture modes
    // don't load or save at all, but a game that can only ever write to one absolute path
    // is a game you can't test without risking somebody's farm.
    if let Ok(dir) = std::env::var("PDS_SAVE_DIR") {
        return std::path::PathBuf::from(dir);
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let base = if cfg!(target_os = "macos") {
        format!("{home}/Library/Application Support/Please Don't Shake")
    } else if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(|a| format!("{a}/Please Don't Shake"))
            .unwrap_or_else(|_| ".".into())
    } else {
        format!("{home}/.local/share/please-dont-shake")
    };
    std::path::PathBuf::from(base)
}

fn save_path() -> std::path::PathBuf {
    save_dir().join("farm.json")
}

// ---------------------------------------------------------------------------
// The file
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    format: u32,
    /// One byte per cell, row-major from `y = 0`: substance in the top two bits, palette
    /// shade in the low six. Hex.
    cells: String,
    /// One bit per cell, same order: whether that grain is still loose. Hex.
    loose: String,
    ants: Vec<SavedAnt>,
    /// Grains that were in the air. Put back into the grid on load — see the module note.
    grains: Vec<SavedGrain>,
    kits: u32,
    /// A kit that was still pouring in, so closing the app mid-tip doesn't eat the rest
    /// of the colony.
    pour: Option<SavedPour>,
    /// The pile. Added after the format was already in the wild, so it defaults to empty
    /// rather than making an existing farm unreadable — a colony that comes back without its
    /// eggs is a disappointment, and a colony that doesn't come back at all is a bug report.
    #[serde(default)]
    brood: Vec<SavedBrood>,
    /// Unix seconds when this was written, for [`crate::away`]. Zero means a file from before
    /// this existed, which is read as "no idea how long ago" and therefore no catch-up.
    #[serde(default)]
    saved_at: u64,
}

/// Written out field by field rather than by deriving on `Ant` itself.
///
/// `Ant` is free to change shape as the simulation grows; the file is not. Spelling the
/// fields out here means a change to the component is a compile error in this file rather
/// than a save that silently stops loading.
#[derive(Serialize, Deserialize)]
struct SavedAnt {
    queen: bool,
    pos: [f32; 2],
    heading: [f32; 2],
    vel: [f32; 2],
    age_days: f64,
    carrying: Option<u8>,
    dig_cooldown: f32,
    haul_time: f32,
    dug_at: [f32; 2],
    dislodged: f32,
    z: f32,
}

/// A brood item. Stage as a small integer rather than the enum's name, so renaming a variant
/// in the simulation can't quietly invalidate every save on disk.
///
/// What's missing is deliberate: `held_by` is an `Entity`, and an entity id means nothing in
/// the next process. Brood in a nurse's mandibles is set down where it was — the nurse, or
/// another one, picks it up again within a second of play.
#[derive(Serialize, Deserialize)]
struct SavedBrood {
    stage: u8,
    age_days: f64,
    pos: [f32; 2],
}

fn stage_code(stage: Stage) -> u8 {
    match stage {
        Stage::Egg => 0,
        Stage::Larva => 1,
        Stage::Pupa => 2,
    }
}

fn stage_of(code: u8) -> Stage {
    match code {
        1 => Stage::Larva,
        2 => Stage::Pupa,
        _ => Stage::Egg,
    }
}

/// Wall-clock seconds since the epoch, or zero if the machine won't say.
///
/// The one piece of state in this file that isn't the farm: it is what lets the colony have
/// lived while the app was shut. A clock that has been wound backwards since the save reads as
/// zero elapsed rather than as negative time — see `load_farm`.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize)]
struct SavedGrain {
    x: u32,
    y: u32,
    shade: u8,
}

#[derive(Serialize, Deserialize)]
struct SavedPour {
    remaining: u32,
    x: f32,
    next_in: f32,
    seed: u32,
}

// ---------------------------------------------------------------------------
// Hex
// ---------------------------------------------------------------------------
//
// Hex rather than base64, for two blobs totalling about 92KB against base64's 61KB. The
// difference doesn't matter for a file this size, and hex is short enough to be obviously
// correct — which base64, hand-rolled to avoid a dependency, would not be.

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0xF) as u32, 16).unwrap());
    }
    s
}

fn from_hex(s: &str, expect: usize) -> Option<Vec<u8>> {
    if s.len() != expect * 2 {
        return None;
    }
    let raw = s.as_bytes();
    let mut out = Vec::with_capacity(expect);
    for pair in raw.chunks(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
    }
    Some(out)
}

const CELLS: usize = GRID_W * GRID_H;
const LOOSE_BYTES: usize = CELLS.div_ceil(8);

fn substance_code(mat: Substance) -> u8 {
    match mat {
        Substance::Air => 0,
        Substance::Sand => 1,
        Substance::Stone => 2,
    }
}

fn substance_of(code: u8) -> Substance {
    match code {
        1 => Substance::Sand,
        2 => Substance::Stone,
        _ => Substance::Air,
    }
}

/// The whole tank as two hex blobs. Split out from the save system so the format can be
/// tested against a real grid without a window or a filesystem — it is the one part of
/// persistence that is this game's own arithmetic rather than serde's.
fn pack_grid(grid: &SandGrid) -> (String, String) {
    let mut cells = vec![0u8; CELLS];
    let mut loose = vec![0u8; LOOSE_BYTES];
    for y in 0..GRID_H {
        for x in 0..GRID_W {
            let i = SandGrid::idx(x, y);
            let cell = grid.get(x, y);
            cells[i] = (substance_code(cell.mat) << 6) | (cell.shade & 0x3F);
            if grid.is_loose(x, y) {
                loose[i / 8] |= 1 << (i % 8);
            }
        }
    }
    (to_hex(&cells), to_hex(&loose))
}

/// The reverse. `false` means the blobs were the wrong length and the grid was left alone —
/// half-applying a truncated file would be worse than not loading it.
fn unpack_grid(cells: &str, loose: &str, grid: &mut SandGrid) -> bool {
    let (Some(cells), Some(loose)) = (from_hex(cells, CELLS), from_hex(loose, LOOSE_BYTES)) else {
        return false;
    };
    for y in 0..GRID_H {
        for x in 0..GRID_W {
            let i = SandGrid::idx(x, y);
            let byte = cells[i];
            let cell = Cell { mat: substance_of(byte >> 6), shade: byte & 0x3F };
            grid.set_raw_with_loose(x, y, cell, loose[i / 8] & (1 << (i % 8)) != 0);
        }
    }
    true
}

/// Everything in the grid changed at once, so wake and remesh the lot. Cheaper than
/// touching forty thousand cells one at a time on the way in, and the revision bump is
/// what makes the navigation flood rebuild without being told.
fn wake_everything(grid: &mut SandGrid) {
    grid.dirty.fill(true);
    grid.awake.fill(true);
    grid.next_awake.fill(true);
    grid.agitation.fill(0.0);
    grid.epoch = grid.epoch.wrapping_add(1);
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// One system, three triggers: leaving play, quitting, and a timer. See the module note.
pub fn save_farm(
    grid: Res<SandGrid>,
    ants: Query<(&Ant, Has<Queen>)>,
    brood: Query<&Brood>,
    grains: Query<&Transform, With<Grain>>,
    grain_shades: Query<&Grain>,
    stock: Res<Stock>,
    pour: Res<KitPour>,
    progress: Res<GameInProgress>,
) {
    // Nothing has been started, so there is nothing to keep. Writing here would replace a
    // real farm with an empty tank the first time somebody opened the app and closed it.
    if !progress.0 {
        return;
    }

    let (cells, loose) = pack_grid(&grid);

    let ants: Vec<SavedAnt> = ants
        .iter()
        .map(|(ant, queen)| SavedAnt {
            queen,
            pos: ant.pos.to_array(),
            heading: ant.heading.to_array(),
            vel: ant.vel.to_array(),
            age_days: ant.age_days,
            carrying: ant.carrying,
            dig_cooldown: ant.dig_cooldown,
            haul_time: ant.haul_time,
            dug_at: ant.dug_at.to_array(),
            dislodged: ant.dislodged,
            z: ant.z,
        })
        .collect();

    let brood: Vec<SavedBrood> = brood
        .iter()
        .map(|item| SavedBrood {
            stage: stage_code(item.stage),
            age_days: item.age_days,
            pos: item.pos.to_array(),
        })
        .collect();

    // Where a flying grain would have landed, near enough. It goes back into the grid on
    // load rather than back into the air: mass is what has to survive, not the arc.
    let in_flight: Vec<SavedGrain> = grains
        .iter()
        .zip(grain_shades.iter())
        .map(|(tf, grain)| {
            let (cx, cy) = SandGrid::world_to_cell(tf.translation);
            SavedGrain {
                x: cx.floor().clamp(0.0, (GRID_W - 1) as f32) as u32,
                y: cy.floor().clamp(0.0, (GRID_H - 1) as f32) as u32,
                shade: grain.shade,
            }
        })
        .collect();

    let snapshot = Snapshot {
        format: FORMAT,
        cells,
        loose,
        ants,
        brood,
        saved_at: now_secs(),
        grains: in_flight,
        kits: stock.kits,
        pour: (pour.remaining > 0).then(|| SavedPour {
            remaining: pour.remaining,
            x: pour.x,
            next_in: pour.next_in,
            seed: pour.seed,
        }),
    };

    if let Err(why) = write(&snapshot) {
        // Not fatal, and not worth interrupting an ambient game over. The next autosave
        // is thirty seconds away.
        warn!("could not write the farm: {why}");
    }
}

fn write(snapshot: &Snapshot) -> Result<(), String> {
    let dir = save_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(snapshot).map_err(|e| e.to_string())?;

    // Written beside the real file and moved into place, because the alternative is a
    // half-written farm. A crash partway through a plain `write` leaves a truncated file
    // that parses as nothing, and the player loses the farm at the exact moment they'd
    // least forgive it. `rename` within one directory is atomic on every platform we ship.
    let temp = dir.join("farm.json.new");
    std::fs::write(&temp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, save_path()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// Put the farm back at startup, if there is one.
///
/// Runs after the ant assets exist, because restoring a colony means spawning bodies.
/// A farm that fails to load leaves the fresh strata that `main` already poured and
/// `GameInProgress` false, so the title screen simply offers New Game — which is the
/// right behaviour for a first run and for a corrupt file alike.
pub fn load_farm(
    mut commands: Commands,
    mut grid: ResMut<SandGrid>,
    mut stock: ResMut<Stock>,
    mut pour: ResMut<KitPour>,
    mut progress: ResMut<GameInProgress>,
    mut away: ResMut<AwaySpan>,
    clock: Res<ColonyClock>,
    assets: Res<AntAssets>,
    brood_assets: Option<Res<BroodAssets>>,
    tank: Query<Entity, With<TankRoot>>,
) {
    let Some(snapshot) = read() else {
        return;
    };
    let Ok(tank) = tank.single() else {
        warn!("no tank to put a saved farm into; starting fresh");
        return;
    };

    if !unpack_grid(&snapshot.cells, &snapshot.loose, &mut grid) {
        warn!("the farm on disk is the wrong size for this tank; starting fresh");
        return;
    }
    wake_everything(&mut grid);

    // Grains that were in the air. Put them back through the same settling the particle
    // system uses, so one that was over a filled column still finds somewhere to sit
    // instead of overwriting what's there.
    for grain in &snapshot.grains {
        crate::grains::settle(&mut grid, grain.x as isize, grain.y as isize, grain.shade);
    }

    for saved in &snapshot.ants {
        let ant = Ant {
            pos: Vec2::from_array(saved.pos),
            heading: Vec2::from_array(saved.heading),
            vel: Vec2::from_array(saved.vel),
            age_days: saved.age_days,
            carrying: saved.carrying,
            dig_cooldown: saved.dig_cooldown,
            haul_time: saved.haul_time,
            dug_at: Vec2::from_array(saved.dug_at),
            dislodged: saved.dislodged,
            z: saved.z,
        };
        // Parented to the tank, like every other ant. Without this a restored colony sits
        // in world space and stops moving when the tank does — the farm would shake around
        // ants that hung in the air where the glass used to be.
        let entity = commands
            .spawn((body_bundle(&assets, ant, saved.queen), ChildOf(tank)))
            .id();
        if saved.queen {
            commands.entity(entity).insert(Queen);
        }
    }

    // The pile. Only if the assets exist — they are made in the same startup chain, and a
    // farm restored without them would be brood that is real to the simulation and invisible.
    if let Some(brood_assets) = brood_assets {
        for saved in &snapshot.brood {
            let item = Brood {
                stage: stage_of(saved.stage),
                age_days: saved.age_days,
                pos: Vec2::from_array(saved.pos),
                held_by: None,
            };
            commands.spawn((brood_bundle(&brood_assets, item), ChildOf(tank)));
        }
    } else if !snapshot.brood.is_empty() {
        warn!("the brood assets were not ready; {} eggs lost", snapshot.brood.len());
    }

    // How long the app was shut, in colony-days, for `crate::away` to settle up. Saturating,
    // so a clock that has been wound back since the save is no time at all rather than
    // negative time — and `saved_at` is zero on a file written before it was recorded, which
    // reads the same way.
    if snapshot.saved_at > 0 {
        let seconds = now_secs().saturating_sub(snapshot.saved_at);
        away.0 = seconds as f64 * clock.days_per_second;
    }

    stock.kits = snapshot.kits;
    if let Some(saved) = &snapshot.pour {
        *pour = KitPour {
            remaining: saved.remaining,
            x: saved.x,
            next_in: saved.next_in,
            seed: saved.seed,
        };
    }

    // There is a farm, so the title screen offers Continue.
    progress.0 = true;
    info!(
        "farm restored: {} ants, {} brood, {} sand cells",
        snapshot.ants.len(),
        snapshot.brood.len(),
        grid.sand_count()
    );
}

fn read() -> Option<Snapshot> {
    let text = std::fs::read_to_string(save_path()).ok()?;
    let snapshot: Snapshot = serde_json::from_str(&text)
        .inspect_err(|why| warn!("the farm on disk could not be read: {why}"))
        .ok()?;
    if snapshot.format != FORMAT {
        warn!(
            "the farm on disk is format {} and this build reads {FORMAT}; starting fresh",
            snapshot.format
        );
        return None;
    }
    Some(snapshot)
}

/// Throw the saved farm away. Called by New Game, so that starting over and then closing
/// the app doesn't reopen onto the farm that was just abandoned.
pub fn forget_farm() {
    match std::fs::remove_file(save_path()) {
        Ok(()) => info!("the old farm is gone"),
        // Nothing there to remove is the normal case on a first run.
        Err(why) if why.kind() == std::io::ErrorKind::NotFound => {}
        Err(why) => warn!("could not remove the old farm: {why}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_survives_a_round_trip() {
        let bytes: Vec<u8> = (0..=255u8).chain(0..=255u8).collect();
        let hex = to_hex(&bytes);
        assert_eq!(hex.len(), bytes.len() * 2);
        assert_eq!(from_hex(&hex, bytes.len()).as_deref(), Some(&bytes[..]));
    }

    /// A truncated or padded blob has to be refused rather than half-applied. Length is
    /// the only cheap check there is against a file that was cut off mid-write.
    #[test]
    fn hex_of_the_wrong_length_is_refused() {
        let hex = to_hex(&[1, 2, 3]);
        assert!(from_hex(&hex, 3).is_some());
        assert!(from_hex(&hex, 4).is_none(), "short blob accepted");
        assert!(from_hex(&hex, 2).is_none(), "long blob accepted");
        assert!(from_hex("zz", 1).is_none(), "non-hex accepted");
    }

    /// The whole point, on a farm that looks like a real one: strata, a carved nest, and
    /// some loose spoil sitting on top. Every cell and every loose bit has to come back
    /// exactly, because "close enough" in a sand grid means tunnels shifting a cell on
    /// every restart.
    #[test]
    fn a_farm_survives_the_round_trip() {
        let mut grid = SandGrid::new();
        fill_strata(&mut grid, INITIAL_SURFACE);

        // A shaft and a chamber, so there is air below the fill line as well as above it.
        let shaft = GRID_W / 2;
        for y in 30..INITIAL_SURFACE {
            grid.set(shaft, y, Cell::AIR);
        }
        for y in 28..34 {
            for x in shaft..shaft + 12 {
                grid.set(x, y, Cell::AIR);
            }
        }
        // Loose spoil, which is the state most easily lost: it is one bit outside the cell.
        for x in shaft + 14..shaft + 20 {
            grid.set_loose(x, INITIAL_SURFACE, Cell { mat: Substance::Sand, shade: 9 });
        }
        // And one stone, since it shares the two bits with air and sand.
        grid.set(4, 4, Cell { mat: Substance::Stone, shade: 3 });

        let (cells, loose) = pack_grid(&grid);
        let mut back = SandGrid::new();
        assert!(unpack_grid(&cells, &loose, &mut back), "the blobs were refused");

        let mut wrong = 0;
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                if back.get(x, y) != grid.get(x, y) || back.is_loose(x, y) != grid.is_loose(x, y) {
                    wrong += 1;
                }
            }
        }
        assert_eq!(wrong, 0, "{wrong} cells came back different");
        assert_eq!(back.sand_count(), grid.sand_count(), "the mass changed");
    }

    fn test_ant(pos: Vec2, age_days: f64, carrying: Option<u8>) -> Ant {
        Ant {
            pos,
            heading: Vec2::new(0.6, -0.8),
            vel: Vec2::ZERO,
            age_days,
            carrying,
            dig_cooldown: 0.25,
            haul_time: 1.5,
            dug_at: Vec2::new(pos.x, pos.y - 4.0),
            dislodged: 0.0,
            z: 0.1,
        }
    }

    /// The feature the player actually experiences: close the app on a farm, open it, and
    /// the farm is there. Driven through the real systems rather than the packing
    /// functions, because everything that can go wrong here is wiring — a resource not
    /// restored, a marker not re-attached, the flag that decides whether Continue appears.
    ///
    /// No renderer and no window: two `App`s with the resources the two systems ask for.
    #[test]
    fn a_farm_comes_back_after_closing_the_app() {
        let dir = std::env::temp_dir().join("pds-save-round-trip");
        let _ = std::fs::remove_dir_all(&dir);
        // Sound because no other test reads or writes this variable, and nothing else in
        // the process consults it. Rust 2024 makes the unsafety explicit rather than new.
        unsafe { std::env::set_var("PDS_SAVE_DIR", &dir) };

        // ---- the session that gets closed --------------------------------
        let mut grid = SandGrid::new();
        fill_strata(&mut grid, INITIAL_SURFACE);
        for y in 40..INITIAL_SURFACE {
            grid.set(GRID_W / 2, y, Cell::AIR);
        }
        grid.set_loose(GRID_W / 2, INITIAL_SURFACE + 1, Cell { mat: Substance::Sand, shade: 5 });
        let dug = grid.sand_count();

        let mut app = App::new();
        app.insert_resource(grid)
            .insert_resource(Stock { kits: 0 })
            .init_resource::<KitPour>()
            .insert_resource(GameInProgress(true))
            .insert_resource(crate::ants::stub_assets())
            .add_systems(Update, save_farm);
        let queen = test_ant(Vec2::new(128.0, 45.0), 402.5, None);
        let worker = test_ant(Vec2::new(130.0, 96.0), 12.25, Some(21));
        app.world_mut().spawn((queen, Queen));
        app.world_mut().spawn(worker);
        // One of each stage, so a mixed-up stage code shows as a wrong count rather than
        // being masked by everything being an egg.
        app.world_mut().spawn(Brood {
            stage: Stage::Larva,
            age_days: 1.75,
            pos: Vec2::new(126.0, 44.0),
            held_by: None,
        });
        app.world_mut().spawn(Brood {
            stage: Stage::Pupa,
            age_days: 0.5,
            pos: Vec2::new(127.0, 44.0),
            // Held. The nurse is an entity id that means nothing next time, so this has to
            // come back set down rather than come back pointing at whatever now owns 3v1.
            held_by: Some(Entity::from_raw_u32(3).unwrap()),
        });
        app.update();

        assert!(save_path().is_file(), "closing the app wrote no farm");

        // ---- the session that opens it -----------------------------------
        let mut next = App::new();
        next.insert_resource(SandGrid::new())
            .insert_resource(Stock { kits: 7 })
            .init_resource::<KitPour>()
            .insert_resource(GameInProgress(false))
            .insert_resource(crate::ants::stub_assets())
            .insert_resource(crate::brood::stub_assets())
            .init_resource::<ColonyClock>()
            .init_resource::<AwaySpan>()
            .add_systems(Update, load_farm);
        let tank = next.world_mut().spawn(TankRoot).id();
        next.update();

        let restored = next.world().resource::<SandGrid>();
        assert_eq!(restored.sand_count(), dug, "the tank came back with different mass");
        assert!(
            restored.is_air((GRID_W / 2) as isize, 60),
            "the shaft did not come back",
        );
        assert!(
            restored.is_loose(GRID_W / 2, INITIAL_SURFACE + 1),
            "the loose spoil came back packed",
        );
        assert!(
            next.world().resource::<GameInProgress>().0,
            "a restored farm must offer Continue",
        );
        assert_eq!(
            next.world().resource::<Stock>().kits,
            0,
            "stock was not restored — the kit would come back after being used",
        );

        // The pile, and the tank it belongs to. An ant or an egg that comes back unparented
        // renders in world space and stops moving when the tank shakes.
        let mut brood = next.world_mut().query::<(&Brood, &ChildOf)>();
        let mut pile: Vec<(Stage, f64, bool)> = brood
            .iter(next.world())
            .map(|(item, parent)| (item.stage, item.age_days, parent.parent() == tank))
            .collect();
        pile.sort_by(|a, b| a.1.total_cmp(&b.1));
        assert_eq!(
            pile,
            vec![(Stage::Pupa, 0.5, true), (Stage::Larva, 1.75, true)],
            "the brood did not come back as it was laid down",
        );
        assert!(
            brood.iter(next.world()).all(|(item, _)| item.held_by.is_none()),
            "brood came back still in the mandibles of an ant that no longer exists",
        );
        let mut bodies = next.world_mut().query::<(&Ant, &ChildOf)>();
        assert!(
            bodies.iter(next.world()).all(|(_, parent)| parent.parent() == tank),
            "a restored ant was not put back in the tank",
        );

        // ---- and the session that opens it a week later -------------------
        //
        // The same two systems main.rs chains at startup, against the same file with its
        // timestamp wound back. This is the join between the two features: `load_farm`
        // spawns the colony through `Commands`, and the catch-up is what has to see it — a
        // missing sync point here would settle up an empty tank and nobody would notice,
        // because a farm that comes back unaged looks exactly like a farm nobody left.
        let mut snapshot = read().expect("the farm just written cannot be read back");
        snapshot.saved_at -= 7 * 86_400;
        write(&snapshot).expect("the backdated farm could not be written");

        let mut later = App::new();
        later
            .insert_resource(SandGrid::new())
            .insert_resource(Stock { kits: 7 })
            .init_resource::<KitPour>()
            .insert_resource(GameInProgress(false))
            .insert_resource(crate::ants::stub_assets())
            .insert_resource(crate::brood::stub_assets())
            .init_resource::<ColonyClock>()
            .init_resource::<AwaySpan>()
            .init_resource::<crate::ants::ColonyStep>()
            .init_resource::<crate::brood::BroodStats>()
            .init_resource::<crate::brood::LayClock>()
            .init_resource::<crate::settings::Settings>()
            .add_systems(
                Startup,
                (load_farm, crate::away::catch_up_while_away).chain(),
            );
        later.world_mut().spawn(TankRoot);
        later.update();

        let mut queens = later.world_mut().query_filtered::<&Ant, With<Queen>>();
        let queen_age = queens.iter(later.world()).next().expect("no queen came back").age_days;
        assert!(
            (queen_age - 409.5).abs() < 0.01,
            "a week away aged the queen {} days",
            queen_age - 402.5,
        );
        assert!(
            later.world().resource::<crate::brood::BroodStats>().eclosed >= 2,
            "neither the larva nor the pupa hatched in a week away",
        );

        let mut ants = next.world_mut().query::<(&Ant, Has<Queen>)>();
        let mut found: Vec<(f64, bool, Option<u8>)> = ants
            .iter(next.world())
            .map(|(ant, queen)| (ant.age_days, queen, ant.carrying))
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(
            found,
            vec![(12.25, false, Some(21)), (402.5, true, None)],
            "the colony came back as different ants",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Substance and shade share a byte, so the packing has to be exactly reversible for
    /// every value the palette can produce.
    #[test]
    fn every_cell_packs_and_unpacks() {
        for mat in [Substance::Air, Substance::Sand, Substance::Stone] {
            for shade in 0..PALETTE_LEN as u8 {
                let byte = (substance_code(mat) << 6) | (shade & 0x3F);
                assert_eq!(substance_of(byte >> 6), mat);
                assert_eq!(byte & 0x3F, shade, "shade {shade} does not fit in six bits");
            }
        }
    }
}
