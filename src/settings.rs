//! Settings.
//!
//! A tabbed window — Video, Audio, Gameplay — reachable from the title screen and from the
//! Esc menu, and the same window either way. Ordo owns the tabs, the panes and which one is
//! open; this module owns what's *in* them, which is the part only the game can know.
//!
//! Every control here does something. There are fewer of them than a settings screen usually
//! has, and that's the trade: a switch that doesn't move anything is worse than a short list,
//! because the player can't tell which kind they're looking at. When there is more to change
//! this is where it goes.
//!
//! Values are cycled rather than dragged. A slider is the obvious control and it isn't in the
//! kit yet, and stepping through named choices is honest about a range that only has a few
//! useful positions in it anyway — nobody needs shake sensitivity to two decimal places.

use bevy::audio::Volume;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use ordo::prelude::*;
use serde::{Deserialize, Serialize};

use crate::audio::BackgroundMusic;

/// Everything the player can change, and everything that gets written down.
///
/// Kept apart from the farm's own save file. A farm is a thing you can lose or start over;
/// these are how the game behaves on this machine, and starting a new colony has no business
/// resetting the volume.
#[derive(Resource, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub fullscreen: bool,
    /// Music level, 0 to 4. Zero is off.
    pub music: u8,
    /// How hard a given hand movement shakes the tank, as an index into [`SHAKE_STEPS`].
    pub shake: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self { fullscreen: false, music: 3, shake: 1 }
    }
}

/// Music levels, and the names shown for them.
const MUSIC_STEPS: [(f32, &str); 5] =
    [(0.0, "Off"), (0.22, "Quiet"), (0.45, "Low"), (0.65, "Normal"), (0.9, "Loud")];

/// Shake sensitivity: a multiplier on what the input path does with hand speed, and the name
/// for it. Not a difficulty setting — it's a mouse-feel setting, and the middle one is what
/// the game was tuned against.
pub const SHAKE_STEPS: [(f32, &str); 3] = [(0.6, "Gentle"), (1.0, "Normal"), (1.5, "Heavy")];

impl Settings {
    pub fn music_volume(&self) -> f32 {
        MUSIC_STEPS[(self.music as usize).min(MUSIC_STEPS.len() - 1)].0
    }

    fn music_name(&self) -> &'static str {
        MUSIC_STEPS[(self.music as usize).min(MUSIC_STEPS.len() - 1)].1
    }

    /// What the shake verb multiplies its agitation by.
    pub fn shake_scale(&self) -> f32 {
        SHAKE_STEPS[(self.shake as usize).min(SHAKE_STEPS.len() - 1)].0
    }

    fn shake_name(&self) -> &'static str {
        SHAKE_STEPS[(self.shake as usize).min(SHAKE_STEPS.len() - 1)].1
    }
}

// ---------------------------------------------------------------------------
// The window
// ---------------------------------------------------------------------------

/// Whether the settings window is up. A resource for the same reason the Esc menu is one:
/// the farm keeps running behind it.
#[derive(Resource, Default)]
pub struct SettingsWindow {
    pub open: bool,
}

/// Everything spawned for the window, so closing it is one despawn.
#[derive(Component)]
pub struct SettingsUi;

/// A control, and what it changes. One click cycles it.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Fullscreen,
    Music,
    Shake,
    Close,
}

/// The window's frame, so its width can be set without a second `Node`.
#[derive(Component)]
pub struct SettingsFrame;

/// The value half of a labelled row, so it can be rewritten without rebuilding the window.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct Reading(pub Control);

const WINDOW_WIDTH: f32 = 420.0;
const VALUE_WIDTH: f32 = 130.0;

const TABS: [&str; 3] = ["Video", "Audio", "Gameplay"];

/// Build and tear down the window to follow the flag.
pub fn sync_settings_ui(
    mut commands: Commands,
    window: Res<SettingsWindow>,
    settings: Res<Settings>,
    existing: Query<Entity, With<SettingsUi>>,
) {
    let shown = existing.iter().next();
    match (window.open, shown) {
        (true, None) => build(&mut commands, &settings),
        (false, Some(entity)) => commands.entity(entity).despawn(),
        _ => {}
    }
}

fn build(commands: &mut Commands, settings: &Settings) {
    // A scrim, so the window reads as something in front of the farm rather than more
    // furniture in it. Ordo's backdrop is the game's dimmer, stated in the theme.
    // Ordo's `backdrop` already carries its own `Layer` — and its own `GlobalZIndex`
    // follows from that — so adding either here is a duplicate component, which Bevy treats
    // as a hard panic rather than a shrug. The radial menu taught this once already.
    let root = commands.spawn((SettingsUi, backdrop())).id();

    // A card rather than a panel. A panel anchors itself absolutely and carries its own
    // half-size pullback to apply at spawn; a card is a framed box that lets the backdrop's
    // own centring put it in the middle, which is where this belongs.
    // A card rather than a panel. A panel anchors itself absolutely and carries its own
    // half-size pullback to apply at spawn; a card is a framed box that lets the backdrop's
    // own centring put it in the middle, which is where this belongs.
    //
    // Its width is set by `size_settings_ui`, not by a `Node` in this bundle. Ordo's pieces
    // bring their own `Node` and two in one bundle is a hard panic — which this module has
    // now walked into three times: the backdrop's `Layer`, a button's `Node`, and this.
    let frame = commands
        .spawn((SettingsFrame, card(), ChildOf(root), children![heading("Settings")]))
        .id();

    let strip = commands.spawn((tab_strip(), ChildOf(frame))).id();
    for (index, name) in TABS.iter().enumerate() {
        commands.spawn((tab(name, index), ChildOf(strip)));
    }

    // One pane per tab. Ordo shows the open one and hides the rest, so nothing here has to
    // know which that is.
    let video = commands.spawn((pane(strip, 0), ChildOf(frame))).id();
    setting_row(commands, video, Control::Fullscreen, "Fullscreen", settings);

    let audio = commands.spawn((pane(strip, 1), ChildOf(frame))).id();
    setting_row(commands, audio, Control::Music, "Music", settings);

    let gameplay = commands.spawn((pane(strip, 2), ChildOf(frame))).id();
    setting_row(commands, gameplay, Control::Shake, "Shake", settings);
    commands.spawn((
        dim("How hard your hand moves the tank."),
        ChildOf(gameplay),
    ));

    commands.spawn(((button("Done"), Control::Close), ChildOf(frame)));
}

/// A labelled row whose value is a button: the label on the left, the current choice on the
/// right, and clicking the choice cycles it.
fn setting_row(
    commands: &mut Commands,
    parent: Entity,
    control: Control,
    name: &str,
    settings: &Settings,
) {
    let row_entity = commands.spawn((row(), ChildOf(parent))).id();
    commands.spawn((label(name), ChildOf(row_entity)));
    // Width comes from `size_readings` rather than a `Node` in this bundle: Ordo's button
    // brings its own, and two in one bundle is a hard panic.
    commands.spawn((
        button(&reading_for(control, settings)),
        Reading(control),
        control,
        ChildOf(row_entity),
    ));
}

fn reading_for(control: Control, settings: &Settings) -> String {
    match control {
        Control::Fullscreen => if settings.fullscreen { "On" } else { "Off" }.to_string(),
        Control::Music => settings.music_name().to_string(),
        Control::Shake => settings.shake_name().to_string(),
        Control::Close => "Done".to_string(),
    }
}

/// One click cycles a control, or closes the window.
pub fn on_control_activate(
    activate: On<Activate>,
    controls: Query<&Control>,
    mut settings: ResMut<Settings>,
    mut window: ResMut<SettingsWindow>,
) {
    let Ok(control) = controls.get(activate.entity) else {
        return;
    };
    match control {
        Control::Fullscreen => settings.fullscreen = !settings.fullscreen,
        Control::Music => settings.music = (settings.music + 1) % MUSIC_STEPS.len() as u8,
        Control::Shake => settings.shake = (settings.shake + 1) % SHAKE_STEPS.len() as u8,
        Control::Close => window.open = false,
    }
}

/// Sizes the frame and the value column.
///
/// A pass rather than `Node`s at spawn, for the reason above — and `Added`, so it costs
/// nothing after the frame the window is built.
pub fn size_settings_ui(
    mut frames: Query<&mut Node, (Added<SettingsFrame>, Without<Reading>)>,
    mut readings: Query<&mut Node, Added<Reading>>,
) {
    for mut node in &mut frames {
        node.min_width = px(WINDOW_WIDTH);
    }
    for mut node in &mut readings {
        node.width = px(VALUE_WIDTH);
    }
}

/// Rewrite the readings when a setting moves.
///
/// The labels live inside Ordo's buttons, so the text to change is the button's child. Only
/// on a change: this would otherwise rewrite every string every frame.
pub fn refresh_readings(
    settings: Res<Settings>,
    readings: Query<(&Reading, &Children)>,
    mut text: Query<&mut Text>,
) {
    if !settings.is_changed() {
        return;
    }
    for (Reading(control), children) in &readings {
        let want = reading_for(*control, &settings);
        for &child in children {
            if let Ok(mut label) = text.get_mut(child)
                && label.0 != want
            {
                label.0 = want.clone();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Making them true
// ---------------------------------------------------------------------------

/// Apply whatever changed to the thing it controls.
///
/// Runs on change rather than being pushed from the click, so a setting restored from disk at
/// startup takes effect by exactly the same path as one the player just cycled. One road in
/// means there is no second road to forget.
pub fn apply_settings(
    settings: Res<Settings>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut music: Query<&mut AudioSink, With<BackgroundMusic>>,
) {
    if !settings.is_changed() {
        return;
    }

    let want = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if window.mode != want {
        window.mode = want;
    }

    for mut sink in &mut music {
        sink.set_volume(Volume::Linear(settings.music_volume()));
    }
}

// ---------------------------------------------------------------------------
// Written down
// ---------------------------------------------------------------------------

fn settings_path() -> std::path::PathBuf {
    crate::save::save_dir().join("settings.json")
}

pub fn load_settings(mut commands: Commands) {
    let restored = std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok());
    if let Some(settings) = restored {
        commands.insert_resource(settings);
    }
}

/// Written on every change rather than on exit. There are three of them and they're a few
/// bytes; waiting for a clean quit to keep them would be the only way to lose them.
pub fn save_settings(settings: Res<Settings>) {
    if !settings.is_changed() {
        return;
    }
    let path = settings_path();
    let Some(dir) = path.parent() else {
        return;
    };
    if let Err(why) = std::fs::create_dir_all(dir)
        .map_err(|e| e.to_string())
        .and_then(|()| serde_json::to_string(&*settings).map_err(|e| e.to_string()))
        .and_then(|json| std::fs::write(&path, json).map_err(|e| e.to_string()))
    {
        warn!("could not write the settings: {why}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cycling has to come back round rather than run off the end of the table, which is the
    /// one thing that could panic here.
    #[test]
    fn every_control_cycles_and_wraps() {
        let mut settings = Settings::default();
        for _ in 0..MUSIC_STEPS.len() * 2 + 1 {
            settings.music = (settings.music + 1) % MUSIC_STEPS.len() as u8;
            // Reading it is the assertion: an index off the end would panic.
            let _ = settings.music_volume();
            let _ = settings.music_name();
        }
        for _ in 0..SHAKE_STEPS.len() * 2 + 1 {
            settings.shake = (settings.shake + 1) % SHAKE_STEPS.len() as u8;
            let _ = settings.shake_scale();
            let _ = settings.shake_name();
        }
    }

    /// A file from an older build, or a hand-edited one, must not be able to index past the
    /// end of a table. `serde(default)` covers missing fields; this covers wrong ones.
    #[test]
    fn a_nonsense_value_is_clamped_rather_than_fatal() {
        let settings = Settings { fullscreen: false, music: 200, shake: 200 };
        assert_eq!(settings.music_volume(), MUSIC_STEPS[MUSIC_STEPS.len() - 1].0);
        assert_eq!(settings.shake_scale(), SHAKE_STEPS[SHAKE_STEPS.len() - 1].0);
    }

    /// The defaults are the tuning the game was built against: the middle shake, and music
    /// at the level `setup_music` used before there was a setting for it.
    #[test]
    fn the_defaults_are_what_the_game_was_tuned_at() {
        let settings = Settings::default();
        assert_eq!(settings.shake_scale(), 1.0, "the default shake must not scale anything");
        assert_eq!(settings.music_volume(), 0.65);
        assert!(!settings.fullscreen);
    }
}
