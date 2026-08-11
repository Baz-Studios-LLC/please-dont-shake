//! The studio's mark, before anything else.
//!
//! Black screen, the New City Entertainment logo fading up, a hold, then a fade down
//! into the title. Deliberately the same shape and the same timings as Divus Factus, so
//! the two games open identically — a studio mark that behaved differently per game
//! would stop reading as a studio mark.
//!
//! The farm is already being built behind this. Meshing 160 chunks, decoding a 1.3MB
//! backdrop and starting the music all land on the first frames, which is exactly the
//! situation [`SLOWEST_STEP`] exists for.

use bevy::prelude::*;
use bevy::text::{FontSize, LineHeight};

use crate::title::GameState;

/// Seconds each fade takes, and seconds the mark holds at full strength.
const SPLASH_FADE: f32 = 1.3;
const SPLASH_HOLD: f32 = 1.8;

/// The most one frame may spend of the mark's life.
///
/// Inherited from Divus Factus, where the mark once vanished entirely: world generation
/// on the first frames produced single frames several seconds long, so the fade ran its
/// whole course inside one frame that hadn't drawn anything yet. Time nobody saw is not
/// time the mark was on screen.
///
/// Capped rather than ignored — a machine slow enough that *every* frame stalls would
/// otherwise hold the splash forever. A bounded step makes the mark worth about
/// forty-four drawn frames whatever the machine is doing.
const SLOWEST_STEP: f32 = 0.1;

/// The year on the studio's line.
const STUDIO_YEAR: u32 = 2026;

/// The mark's own aspect ratio, so it scales without distorting.
const MARK_ASPECT: f32 = 2526.0 / 1420.0;

/// How wide the mark sits, as a fraction of the window. A shade narrower than the title
/// logo: this one is a wordmark on black and wants air around it.
const MARK_WIDTH_PERCENT: f32 = 46.0;

/// The studio's line is the one piece of text in the game that names a font, and it does
/// it for one glyph.
///
/// Bevy's embedded default is `FiraMono-subset.ttf` — Fira Mono cut down to U+0020..U+007E,
/// 95 glyphs — so `©` draws as a missing-glyph box. This is the *same typeface*
/// unsubsetted (1349 glyphs), so the line renders identically to the rest of the UI and
/// the symbol is simply there. SIL Open Font License 1.1; `assets/fonts/OFL.txt` ships
/// beside it because the licence requires it to.
///
/// Named as a path, never as a family: a generic family needs Bevy's
/// `system_font_discovery` feature and without it text silently renders as *nothing*.
const STUDIO_FONT: &str = "fonts/FiraMono-Medium.ttf";

/// Size of the studio's line, in pixels. Bigger than the fine print this usually is —
/// on a 1280-wide window a 13px line was a grey smudge at the bottom of the screen — but
/// still quiet enough to sit under the mark rather than compete with it.
///
/// Everything about the `©` below is derived from this, so it's the only number to change.
const STUDIO_LINE_SIZE: f32 = 22.0;

// ---------------------------------------------------------------------------
// Setting the © on the line
//
// Fira draws the copyright sign as a *superior* mark — its outline sits well above the
// baseline and reaches above cap height. That's deliberate, and it's how `©` and `®` are
// conventionally set beside a wordmark (SF Mono does the same), but in a line of running
// text it reads as a glyph floating above its neighbours. Bevy's UI text has no baseline
// shift, so the sign is its own node: sized so it stands as tall as the digits, and
// nudged down so its foot lands on their baseline.
//
// Everything below is read out of the font file's own tables rather than guessed, so the
// two ends line up exactly instead of approximately. Re-measure if the face changes.
// ---------------------------------------------------------------------------

/// Fira Mono's em, in font units. It's 1000, so every figure here is per-mille.
const EM: f32 = 1000.0;
/// The `©` outline: bottom and top, in units above the baseline. From `glyf`.
const MARK_FOOT: f32 = 112.0;
const MARK_HEAD: f32 = 751.0;
/// Cap height of the digits the sign stands beside. Same source.
const DIGIT_HEAD: f32 = 704.0;
/// How far the font hangs below the baseline, positive. From `hhea`.
const DESCENDER: f32 = 265.0;

/// Line height as a multiple of the font size.
///
/// 1.2 em is Bevy's default *and* exactly Fira Mono's ascender plus descender
/// (935 + 265 = 1200), so there is no leading to reason about: the baseline sits
/// `DESCENDER` above the bottom of the line box at any size. `MARK_NUDGE` depends on
/// that, so it's pinned here rather than left to whatever the default happens to be.
const LINE_HEIGHT_EM: f32 = 1.2;

/// Fira Mono's advance width, in ems. It's monospaced, so this is every glyph's — which
/// makes it the width of the space that would have sat between the sign and the year if
/// they were still one string.
const MONO_ADVANCE_EM: f32 = 0.6;

/// Size the sign is set at, so that it stands exactly as tall as the digits. Its outline
/// is `MARK_HEAD - MARK_FOOT` tall against their `DIGIT_HEAD`, so it needs scaling up.
const MARK_SIZE: f32 = STUDIO_LINE_SIZE * DIGIT_HEAD / (MARK_HEAD - MARK_FOOT);

/// How far down the sign moves for its foot to reach the words' baseline.
///
/// Two parts, and missing the second is what left the top misaligned. Its own outline
/// starts `MARK_FOOT` above its baseline — and because the row aligns the two line boxes
/// by their *bottoms* and the sign's box is now the taller of the two, its baseline also
/// starts a descender's worth of the size difference above the words'.
const MARK_NUDGE: f32 = (MARK_FOOT * MARK_SIZE + DESCENDER * (MARK_SIZE - STUDIO_LINE_SIZE)) / EM;

/// How far right the sign shifts, in ems, purely by eye.
///
/// Unlike everything above this one is a judgement, not a measurement. The sign is a
/// small ring in a wide monospaced cell, so the white space around it reads as a bigger
/// gap than the same distance does between two letters, and it drifts away from the year
/// it belongs to. Closing it up by a fifth of a character puts it back.
const MARK_SHIFT_EM: f32 = 0.2;

/// Everything spawned for the splash, so leaving it is one despawn.
#[derive(Component)]
pub struct SplashScreen;

#[derive(Component)]
pub struct SplashArt;

/// The studio's line at the foot of the dark, fading with the mark above it.
#[derive(Component)]
pub struct SplashMark;

/// How far through the mark's life we are, and what it really cost.
#[derive(Resource, Default)]
pub struct SplashClock {
    /// Seconds of *drawn* time, which is what the fade runs on.
    spent: f32,
    /// Real seconds and frames, for the line it logs on the way out. Had Divus Factus
    /// had this, the bug behind `SLOWEST_STEP` would have been a glance rather than an
    /// afternoon.
    real: f32,
    frames: u32,
}

/// The studio line's typeface at a given size, with the line height pinned alongside it —
/// see [`LINE_HEIGHT_EM`], which [`MARK_NUDGE`] is derived from. `LineHeight` is its own
/// component in Bevy 0.19 rather than a field of `TextFont`, hence the pair.
fn studio_face(assets: &AssetServer, size: f32) -> (TextFont, LineHeight) {
    (
        TextFont {
            font: FontSource::Handle(assets.load(STUDIO_FONT)),
            font_size: FontSize::Px(size),
            ..default()
        },
        LineHeight::RelativeToFont(LINE_HEIGHT_EM),
    )
}

pub fn enter_splash(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut next: ResMut<NextState<GameState>>,
) {
    // Scripted runs have no business sitting through a studio mark. The title shot needs
    // the menu, so it lands there; the colony runs are put straight into play by main and
    // never enter this state at all. `--splash-shot` is the one run that wants the mark.
    if crate::devcapture::capture_mode() && !crate::devcapture::splash_shot() {
        next.set(GameState::Title);
        return;
    }

    commands.insert_resource(SplashClock::default());

    commands.spawn((
        SplashScreen,
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        // True black, not the theme's charcoal: a studio mark opens the way a theatre
        // goes dark, and the sunlit bedroom behind the title reads warmer for following
        // it. This is also the one screen in the game that hides the farm completely.
        BackgroundColor(Color::BLACK),
        GlobalZIndex(320),
        children![
            (
                SplashArt,
                ImageNode {
                    image: assets.load("NewCityEntertainment.png"),
                    color: Color::srgba(1.0, 1.0, 1.0, 0.0),
                    ..default()
                },
                Node {
                    width: percent(MARK_WIDTH_PERCENT),
                    aspect_ratio: Some(MARK_ASPECT),
                    ..default()
                },
            ),
            // The sign is a separate node from the words purely so it can be sized and
            // positioned by hand. The row aligns the two line boxes by their bottoms,
            // which `MARK_NUDGE` accounts for.
            (
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(34),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::End,
                    column_gap: px(STUDIO_LINE_SIZE * MONO_ADVANCE_EM),
                    ..default()
                },
                children![
                    (
                        SplashMark,
                        Text::new("\u{00a9}"),
                        studio_face(&assets, MARK_SIZE),
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
                        // Visual only — `UiTransform` doesn't disturb the layout, so the
                        // gap either side of the sign stays exactly one character wide
                        // however much the sign itself is scaled and shifted.
                        UiTransform {
                            translation: Val2::px(
                                STUDIO_LINE_SIZE * MARK_SHIFT_EM,
                                MARK_NUDGE,
                            ),
                            ..default()
                        },
                    ),
                    (
                        SplashMark,
                        Text::new(format!(
                            "{STUDIO_YEAR} Baz Studios, LLC. All rights reserved."
                        )),
                        studio_face(&assets, STUDIO_LINE_SIZE),
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
                    ),
                ],
            ),
        ],
    ));
}

/// Runs the fade in, the hold and the fade out — and lets any key or click skip ahead.
pub fn play_splash(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    clock: Option<ResMut<SplashClock>>,
    mut arts: Query<&mut ImageNode, With<SplashArt>>,
    mut marks: Query<&mut TextColor, With<SplashMark>>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Some(mut clock) = clock else {
        return;
    };

    // `Time<Real>` rather than `Time`, so the splash is immune to anything the sim does
    // to the fixed clock. Only what a frame could actually *show* spends the mark — see
    // `SLOWEST_STEP`.
    clock.real += time.delta_secs();
    clock.frames += 1;
    clock.spent += time.delta_secs().min(SLOWEST_STEP);

    let fade_out_at = SPLASH_FADE + SPLASH_HOLD;
    let alpha = if clock.spent < SPLASH_FADE {
        clock.spent / SPLASH_FADE
    } else if clock.spent < fade_out_at {
        1.0
    } else {
        1.0 - (clock.spent - fade_out_at) / SPLASH_FADE
    }
    .clamp(0.0, 1.0);

    // Skipping jumps to the fade-out rather than cutting: the mark still leaves the way
    // it always leaves, just now. Entering the out-fade at the alpha it already has keeps
    // the brightness continuous, so an early skip doesn't flash.
    let skipped =
        keys.get_just_pressed().next().is_some() || buttons.get_just_pressed().next().is_some();
    if skipped && clock.spent < fade_out_at {
        clock.spent = fade_out_at + (1.0 - alpha) * SPLASH_FADE;
    }

    for mut art in &mut arts {
        art.color = Color::srgba(1.0, 1.0, 1.0, alpha);
    }
    for mut mark in &mut marks {
        mark.0 = Color::srgba(1.0, 1.0, 1.0, alpha);
    }

    if clock.spent >= fade_out_at + SPLASH_FADE {
        // What the mark actually got, in the two units that matter. A frame count in
        // single figures here means it was swallowed by whatever was loading behind it.
        info!("the studio's mark showed for {:.1}s over {} frames", clock.real, clock.frames);
        next.set(GameState::Title);
    }
}

pub fn exit_splash(mut commands: Commands, screens: Query<Entity, With<SplashScreen>>) {
    for screen in &screens {
        commands.entity(screen).despawn();
    }
    commands.remove_resource::<SplashClock>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alpha curve, lifted out of the system so the shape can be checked without a
    /// window. Kept beside the constants it's made of.
    fn alpha_at(spent: f32) -> f32 {
        let fade_out_at = SPLASH_FADE + SPLASH_HOLD;
        if spent < SPLASH_FADE {
            spent / SPLASH_FADE
        } else if spent < fade_out_at {
            1.0
        } else {
            1.0 - (spent - fade_out_at) / SPLASH_FADE
        }
        .clamp(0.0, 1.0)
    }

    #[test]
    fn the_mark_fades_up_holds_and_fades_down() {
        let life = SPLASH_FADE * 2.0 + SPLASH_HOLD;
        assert_eq!(alpha_at(0.0), 0.0, "it should open on black");
        assert!(alpha_at(SPLASH_FADE * 0.5) > 0.4, "the fade in should be underway");
        assert_eq!(alpha_at(SPLASH_FADE), 1.0, "it should reach full strength");
        assert_eq!(alpha_at(SPLASH_FADE + SPLASH_HOLD * 0.5), 1.0, "it should hold");
        assert!(alpha_at(life - 0.01) < 0.02, "it should end on black");
        assert_eq!(alpha_at(life + 5.0), 0.0, "and stay there");
    }

    /// The sign has to meet the digits at *both* ends. Getting the foot onto the baseline
    /// and leaving the top short was the first attempt, and it read as plainly wrong.
    ///
    /// This is pure arithmetic on the font's metrics, so it needs no window — and it is
    /// the whole reason those metrics are named constants rather than inlined numbers.
    #[test]
    fn the_copyright_sign_meets_the_digits_top_and_bottom() {
        // Heights above the *words'* baseline, in pixels. The row aligns the two line
        // boxes by their bottoms, and the baseline sits `DESCENDER` above the bottom of a
        // box, so the taller box carries its baseline higher by this much.
        let baseline_lift = DESCENDER * (MARK_SIZE - STUDIO_LINE_SIZE) / EM;
        let foot = baseline_lift + MARK_FOOT * MARK_SIZE / EM - MARK_NUDGE;
        let head = baseline_lift + MARK_HEAD * MARK_SIZE / EM - MARK_NUDGE;
        let digit_head = DIGIT_HEAD * STUDIO_LINE_SIZE / EM;

        assert!(foot.abs() < 0.001, "the sign's foot is {foot:.4}px off the baseline");
        assert!(
            (head - digit_head).abs() < 0.001,
            "the sign tops out at {head:.4}px against the digits' {digit_head:.4}px",
        );
    }

    /// A skip has to be continuous. Entering the out-fade at the brightness already on
    /// screen is what stops an early skip flashing to full white and back.
    #[test]
    fn skipping_early_never_brightens_the_mark() {
        let fade_out_at = SPLASH_FADE + SPLASH_HOLD;
        for tenths in 0..=12 {
            let spent = tenths as f32 * 0.1;
            let alpha = alpha_at(spent);
            let skipped_to = fade_out_at + (1.0 - alpha) * SPLASH_FADE;
            let after = alpha_at(skipped_to);
            assert!(
                (after - alpha).abs() < 0.001,
                "skipping at {spent:.1}s jumped the mark from {alpha:.3} to {after:.3}",
            );
            assert!(skipped_to >= spent, "skipping must not rewind the clock");
        }
    }
}
