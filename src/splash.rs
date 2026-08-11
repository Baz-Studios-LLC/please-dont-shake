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
use bevy::text::FontSize;

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
/// on a 1280-wide window a 13px line was a grey smudge at the bottom of the screen.
const STUDIO_LINE_SIZE: f32 = 26.0;

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
            (
                SplashMark,
                Text::new(format!(
                    "\u{00a9} {STUDIO_YEAR} Baz Studios, LLC. All rights reserved."
                )),
                TextFont {
                    font: FontSource::Handle(assets.load(STUDIO_FONT)),
                    font_size: FontSize::Px(STUDIO_LINE_SIZE),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(34),
                    ..default()
                },
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
