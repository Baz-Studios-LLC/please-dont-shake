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
    /// The hand's skin, as an index into [`SKIN_TONES`].
    pub skin: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self { fullscreen: false, music: 3, shake: 1, skin: 1 }
    }
}

/// Music levels, and the names shown for them.
const MUSIC_STEPS: [(f32, &str); 5] =
    [(0.0, "Off"), (0.22, "Quiet"), (0.45, "Low"), (0.65, "Normal"), (0.9, "Loud")];

/// Shake sensitivity: a multiplier on what the input path does with hand speed, and the name
/// for it. Not a difficulty setting — it's a mouse-feel setting, and the middle one is what
/// the game was tuned against.
pub const SHAKE_STEPS: [(f32, &str); 3] = [(0.6, "Gentle"), (1.0, "Normal"), (1.5, "Heavy")];

/// The hand's skin, light to deep.
///
/// The hand is the player in this game — it is the only part of them that is ever on screen —
/// so its colour is theirs to pick. Divus Factus reached the same conclusion about its own
/// hand and for the same reason.
///
/// A real range rather than one colour darkened six times: skin does not vary along a single
/// axis. Lighter tones sit pinker and less saturated, the middle of the range is the most
/// saturated, and the deepest are darker without being any redder. Names describe lightness
/// and nothing else.
///
/// The default is `Fair`, only because it is the tone the hand was already drawn in and the
/// rest of the art was judged against it. It is a starting point, not a norm.
pub const SKIN_TONES: [([f32; 3], &str); 6] = [
    ([0.94, 0.81, 0.73], "Pale"),
    ([0.86, 0.67, 0.56], "Fair"),
    ([0.75, 0.57, 0.41], "Olive"),
    ([0.62, 0.44, 0.30], "Tan"),
    ([0.44, 0.30, 0.21], "Brown"),
    ([0.28, 0.19, 0.14], "Deep"),
];

/// How much darker a knuckle is than the skin around it. One number, so a new tone is one
/// line and the joints can never be forgotten.
const KNUCKLE_SHADE: f32 = 0.84;

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

    /// The hand's skin, and the darker shade its knuckles take.
    pub fn skin(&self) -> (Color, Color) {
        let [r, g, b] = SKIN_TONES[(self.skin as usize).min(SKIN_TONES.len() - 1)].0;
        let k = KNUCKLE_SHADE;
        (Color::srgb(r, g, b), Color::srgb(r * k, g * k, b * k))
    }

    fn skin_name(&self) -> &'static str {
        SKIN_TONES[(self.skin as usize).min(SKIN_TONES.len() - 1)].1
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

/// A control, and what it changes.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Fullscreen,
    Music,
    Shake,
    Skin,
}

/// A nudge on a control: which one, and which way.
#[derive(Component, Clone, Copy)]
pub struct Nudge {
    pub control: Control,
    pub up: bool,
}

/// Closes the window.
#[derive(Component)]
pub struct Done;

// The window's parts. Each one is spaced by `size_settings_ui` rather than by a `Node` at
// spawn, because every piece here comes out of Ordo already carrying one.
#[derive(Component)]
pub struct SettingsFrame;

#[derive(Component)]
pub struct SettingsPane;

#[derive(Component)]
pub struct SettingsDivider;

/// A setting's name-and-control line, together with its hint.
#[derive(Component)]
pub struct SettingsLine;

/// The value half of a labelled row, so it can be rewritten without rebuilding the window.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct Reading(pub Control);

/// The window's own size and spacing, in pixels.
///
/// Set here rather than by leaning on the theme's metrics, which are deliberately tight: they
/// have to suit a HUD panel and a menu button as well as this, and a dialog wants more air
/// than a button does. Ordo's pieces are sized by a pass either way — they bring their own
/// `Node` and a second one in the same bundle is a panic — so putting real numbers in that
/// pass costs nothing extra.
const WINDOW_WIDTH: f32 = 680.0;
/// Fixed, not a minimum, and so is the width: the window must be the *same box* on every tab
/// rather than growing and shrinking as you move between them. A dialog that resizes under
/// the pointer moves the thing you were about to click. The footer is pushed to the bottom of
/// it by a spring, so a tab with one setting in it still looks deliberate.
const WINDOW_HEIGHT: f32 = 400.0;
const WINDOW_PAD: f32 = 30.0;
/// Between the window's own bands: heading, tabs, content, footer.
const BAND_GAP: f32 = 22.0;
/// Between settings inside one pane.
const SETTING_GAP: f32 = 26.0;
/// Between a setting's name and the line explaining it.
const HINT_GAP: f32 = 8.0;
/// Air above and below a divider, so it separates rather than crowds.
const RULE_MARGIN: f32 = 4.0;

const TABS: [&str; 3] = ["Video", "Audio", "Gameplay"];

/// Every setting, in the pane it belongs to, with the line that explains it.
///
/// A table rather than three hand-built panes. The window's shape stops being something to
/// maintain and becomes something to read, and adding a setting is one line here.
const ROWS: [(usize, Control, &str, &str); 4] = [
    (
        0,
        Control::Fullscreen,
        "Fullscreen",
        "Fills the screen. The farm keeps its shape either way.",
    ),
    (1, Control::Music, "Music", "The piano. Off is a setting too."),
    (
        2,
        Control::Shake,
        "Shake",
        "How hard your hand moves the tank.",
    ),
    (2, Control::Skin, "Hand", "Your hand. It is the only part of you in the room."),
];

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
    // Ordo's `backdrop` already carries its own `Layer` — and its own `GlobalZIndex`
    // follows from that — so adding either here is a duplicate component, which Bevy treats
    // as a hard panic rather than a shrug. The radial menu taught this once already.
    let root = commands.spawn((SettingsUi, backdrop())).id();

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
    commands.spawn((rule(), SettingsDivider, ChildOf(frame)));

    let strip = commands.spawn((tab_strip(), ChildOf(frame))).id();
    for (index, name) in TABS.iter().enumerate() {
        commands.spawn((tab(name, index), ChildOf(strip)));
    }
    // Under the strip, so the tabs read as tabs on top of the content rather than as three
    // more buttons floating above it.
    commands.spawn((rule(), SettingsDivider, ChildOf(frame)));

    // One pane per tab. Ordo shows the open one and hides the rest, so nothing here has to
    // know which that is.
    let panes: Vec<Entity> = (0..TABS.len())
        .map(|index| {
            commands
                .spawn((pane(strip, index), SettingsPane, ChildOf(frame)))
                .id()
        })
        .collect();

    for (pane_index, control, name, hint) in ROWS {
        setting_row(commands, panes[pane_index], control, name, hint, settings);
    }

    // Eats the rest of the window's height, so the footer sits on the bottom edge however
    // few settings the open tab has. Without it a short pane leaves Done floating in the
    // middle of a tall window, which reads as a layout accident rather than a footer.
    commands.spawn((spring(), ChildOf(frame)));
    commands.spawn((rule(), SettingsDivider, ChildOf(frame)));

    // The footer. Done sits at the right on its own line, the way a dialog's dismissal does
    // — full width, it read as the window's main business rather than the way out of it.
    let footer = commands.spawn((row(), ChildOf(frame))).id();
    commands.spawn((spring(), ChildOf(footer)));
    commands.spawn((button("Done"), Done, ChildOf(footer)));
}

/// A settings line: the name at the left, a stepper at the right, and underneath it one
/// sentence saying what the thing is.
fn setting_row(
    commands: &mut Commands,
    parent: Entity,
    control: Control,
    name: &str,
    hint: &str,
    settings: &Settings,
) {
    // Carries its own `Node` — unlike everything else here it isn't an Ordo bundle, and a UI
    // child whose parent has no `Node` is a layout warning and a wrong answer.
    let group = commands
        .spawn((SettingsLine, Node::default(), ChildOf(parent)))
        .id();
    let line = commands.spawn((row(), ChildOf(group))).id();
    commands.spawn((body(name), ChildOf(line)));
    commands.spawn((spring(), ChildOf(line)));

    let parts = stepper(commands, line, &reading_for(control, settings));
    commands
        .entity(parts.value)
        .insert((Reading(control), control));
    commands
        .entity(parts.down)
        .insert(Nudge { control, up: false });
    commands.entity(parts.up).insert(Nudge { control, up: true });

    commands.spawn((dim(hint), ChildOf(group)));
}

fn reading_for(control: Control, settings: &Settings) -> String {
    match control {
        Control::Fullscreen => if settings.fullscreen { "On" } else { "Off" }.to_string(),
        Control::Music => settings.music_name().to_string(),
        Control::Shake => settings.shake_name().to_string(),
        Control::Skin => settings.skin_name().to_string(),
    }
}

/// Step an index within a table, stopping at each end.
///
/// Stopping rather than wrapping, now that there are two arrows. A wheel that wraps is fine
/// when one button is the only way round; with a `<` beside a `>` it means the left arrow
/// sometimes goes up, which is a small lie about what the control does.
fn step(value: u8, up: bool, len: usize) -> u8 {
    let last = (len - 1) as u8;
    if up { value.saturating_add(1).min(last) } else { value.saturating_sub(1) }
}

/// A nudge either way, or Done.
pub fn on_control_activate(
    activate: On<Activate>,
    nudges: Query<&Nudge>,
    done: Query<&Done>,
    mut settings: ResMut<Settings>,
    mut window: ResMut<SettingsWindow>,
) {
    if done.get(activate.entity).is_ok() {
        window.open = false;
        return;
    }
    let Ok(Nudge { control, up }) = nudges.get(activate.entity) else {
        return;
    };
    match control {
        // Two states, so either arrow flips it. Refusing one of them would be technically
        // consistent and would read as a broken button.
        Control::Fullscreen => settings.fullscreen = !settings.fullscreen,
        Control::Music => settings.music = step(settings.music, *up, MUSIC_STEPS.len()),
        Control::Shake => settings.shake = step(settings.shake, *up, SHAKE_STEPS.len()),
        Control::Skin => settings.skin = step(settings.skin, *up, SKIN_TONES.len()),
    }
}

/// Sizes the frame and the value column.
///
/// A pass rather than `Node`s at spawn, for the reason above — and `Added`, so it costs
/// nothing after the frame the window is built.
/// One query with the parts as flags, rather than four queries over `&mut Node`.
///
/// Four would have to be `Without` of *each other*, not just of the one before — Bevy refuses
/// overlapping mutable access and says so at runtime, which is a panic on the frame the window
/// first opens. Nothing here can be two parts at once, so a single query answers it.
pub fn size_settings_ui(
    mut parts: Query<
        (
            &mut Node,
            Has<SettingsFrame>,
            Has<SettingsPane>,
            Has<SettingsLine>,
        ),
        Or<(
            Added<SettingsFrame>,
            Added<SettingsPane>,
            Added<SettingsLine>,
            Added<SettingsDivider>,
        )>,
    >,
) {
    for (mut node, frame, pane, line) in &mut parts {
        match (frame, pane, line) {
            (true, ..) => {
                node.width = px(WINDOW_WIDTH);
                node.height = px(WINDOW_HEIGHT);
                node.padding = UiRect::all(px(WINDOW_PAD));
                node.row_gap = px(BAND_GAP);
            }
            (_, true, _) => node.row_gap = px(SETTING_GAP),
            (.., true) => {
                node.flex_direction = FlexDirection::Column;
                node.row_gap = px(HINT_GAP);
            }
            // A divider: the only part left, and all it wants is air either side.
            _ => node.margin = UiRect::vertical(px(RULE_MARGIN)),
        }
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

    /// Stepping stops at both ends rather than wrapping or running off the table. Both ends
    /// matter: `saturating_sub` on a `u8` guards the bottom and the `min` guards the top, and
    /// either one missing is a panic on a value the player can reach by holding an arrow.
    #[test]
    fn stepping_stops_at_both_ends() {
        assert_eq!(step(0, false, 5), 0, "stepping down from the first should stay");
        assert_eq!(step(4, true, 5), 4, "stepping up from the last should stay");
        assert_eq!(step(2, true, 5), 3);
        assert_eq!(step(2, false, 5), 1);

        // And walked the whole way in both directions, reading the value each time — an
        // index off the end of a table would panic rather than fail an assertion.
        let mut settings = Settings::default();
        for _ in 0..MUSIC_STEPS.len() + 2 {
            settings.music = step(settings.music, true, MUSIC_STEPS.len());
            let _ = settings.music_volume();
        }
        for _ in 0..MUSIC_STEPS.len() + 2 {
            settings.music = step(settings.music, false, MUSIC_STEPS.len());
            let _ = settings.music_volume();
        }
        for _ in 0..SHAKE_STEPS.len() + 2 {
            settings.shake = step(settings.shake, true, SHAKE_STEPS.len());
            let _ = settings.shake_scale();
        }
    }

    /// A file from an older build, or a hand-edited one, must not be able to index past the
    /// end of a table. `serde(default)` covers missing fields; this covers wrong ones.
    #[test]
    fn a_nonsense_value_is_clamped_rather_than_fatal() {
        let settings = Settings { fullscreen: false, music: 200, shake: 200, skin: 200 };
        assert_eq!(settings.music_volume(), MUSIC_STEPS[MUSIC_STEPS.len() - 1].0);
        assert_eq!(settings.shake_scale(), SHAKE_STEPS[SHAKE_STEPS.len() - 1].0);
        assert_eq!(settings.skin_name(), SKIN_TONES[SKIN_TONES.len() - 1].1);
    }

    /// The defaults are the tuning the game was built against: the middle shake, music at the
    /// level `setup_music` used before there was a setting for it, and the tone the hand was
    /// already drawn in.
    #[test]
    fn the_defaults_are_what_the_game_was_tuned_at() {
        let settings = Settings::default();
        assert_eq!(settings.shake_scale(), 1.0, "the default shake must not scale anything");
        assert_eq!(settings.music_volume(), 0.65);
        assert!(!settings.fullscreen);
        assert_eq!(settings.skin_name(), "Fair");
    }

    /// Skin runs light to deep with no reversals, and every tone's knuckles are darker than
    /// its skin. Both are properties of the table that a hand-edited entry could break, and
    /// neither would be obvious on the tone you happened to be looking at.
    #[test]
    fn the_skin_tones_run_light_to_deep() {
        let luma = |c: Color| {
            let s = c.to_srgba();
            0.2126 * s.red + 0.7152 * s.green + 0.0722 * s.blue
        };
        let mut previous = f32::MAX;
        for index in 0..SKIN_TONES.len() as u8 {
            let settings = Settings { skin: index, ..Default::default() };
            let (skin, knuckle) = settings.skin();
            assert!(
                luma(skin) < previous,
                "tone {index} ({}) is not darker than the one before it",
                settings.skin_name(),
            );
            assert!(
                luma(knuckle) < luma(skin),
                "tone {index} has knuckles no darker than its skin",
            );
            previous = luma(skin);
        }
    }
}
