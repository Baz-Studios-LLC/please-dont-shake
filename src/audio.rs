//! Music, and the two sounds the player can cause.
//!
//! The piano loops from the moment the app opens. The other two answer the only two verbs there
//! are: a tap on the glass, and a shake.
//!
//! **Named for the action, not the material.** These were `GlassTap*` because the first set of
//! sounds authored for them were glass taps, which got replaced with plastic ones that sound
//! better — a real formicarium is acrylic, and it turns out that reads as "tank" where a wine-glass
//! ping reads as "wine glass". The tank in the fiction is still glass, so the code claiming either
//! material was going to be wrong in one direction or the other; it says `tap` and lets the
//! filename describe the sound.

use bevy::audio::Volume;
use bevy::prelude::*;

/// Marker component for the ambient background piano music.
#[derive(Component)]
pub struct BackgroundMusic;

/// Marker component for the shake rumble sound.
#[derive(Component)]
pub struct ShakeRumble;

/// Event sent whenever the glass is tapped.
#[derive(Event)]
pub struct TapEvent;

/// The tap sounds, in the order they cycle.
///
/// Three of them, played round-robin rather than at random, because a random pick from three
/// repeats itself audibly — two of the same in a row happens a third of the time, and a tap that
/// double-strikes sounds like a bug in the input rather than variety in the sound.
#[derive(Resource, Default)]
pub struct TapSounds {
    pub handles: Vec<Handle<AudioSource>>,
}

/// Spawns the background music player and loads sound effects on startup.
pub fn setup_audio(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        BackgroundMusic,
        AudioPlayer::new(asset_server.load("music/cozy_piano.ogg")),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.65)),
    ));

    commands.spawn((
        ShakeRumble,
        AudioPlayer::new(asset_server.load("sfx/shake_rumble.ogg")),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
    ));

    let tap_sounds = vec![
        asset_server.load("sfx/plastic_tap_1.ogg"),
        asset_server.load("sfx/plastic_tap_2.ogg"),
        asset_server.load("sfx/plastic_tap_3.ogg"),
    ];
    commands.insert_resource(TapSounds {
        handles: tap_sounds,
    });
}

/// Plays a glass tap sound effect.
pub fn play_tap(commands: &mut Commands, sfx: &TapSounds, volume: f32, tap_count: usize) {
    if sfx.handles.is_empty() || volume <= 0.0 {
        return;
    }
    let handle = &sfx.handles[tap_count % sfx.handles.len()];
    commands.spawn((
        AudioPlayer::new(handle.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
    ));
}

/// Observer that plays the glass tap sound effect when a `TapEvent` is triggered.
pub fn on_tap(
    _event: On<TapEvent>,
    mut commands: Commands,
    sfx: Res<TapSounds>,
    settings: Res<crate::settings::Settings>,
    mut tap_count: Local<usize>,
) {
    play_tap(&mut commands, &sfx, settings.sfx_volume(), *tap_count);
    *tap_count = tap_count.wrapping_add(1);
}

/// How much rumble a unit of tank speed is worth, past the deadzone.
const RUMBLE_PER_SPEED: f32 = 0.025;

/// Dynamically adjusts the rumble volume based on how hard the farm is shaken.
pub fn update_shake_rumble(
    mut rumble_q: Query<&mut bevy::audio::AudioSink, With<ShakeRumble>>,
    spring: Res<crate::tank::TankSpring>,
    settings: Res<crate::settings::Settings>,
    mut smoothed_vol: Local<f32>,
    time: Res<Time>,
) {
    if let Ok(mut sink) = rumble_q.single_mut() {
        let speed = spring.vel.length();
        let sfx_vol = settings.sfx_volume();

        // The *same* threshold the shake verb uses, not a copy of its value. A second 2.0 with a
        // comment pointing at the first is a number that will be tuned in one place and stay wrong
        // in the other, and the symptom would be a rumble that starts before the sand moves.
        let mut target_volume = 0.0;
        if speed > crate::interact::SHAKE_DEADZONE && sfx_vol > 0.0 {
            let intensity = (speed - crate::interact::SHAKE_DEADZONE) * RUMBLE_PER_SPEED;
            target_volume = intensity.clamp(0.0, 1.0) * sfx_vol;
        }

        // Smooth volume changes so the audio doesn't clip/pop on sudden stops
        let dt = time.delta_secs();
        if target_volume > *smoothed_vol {
            *smoothed_vol = target_volume; // instant attack
        } else {
            *smoothed_vol += (target_volume - *smoothed_vol) * (dt * 15.0).min(1.0); // fast decay
        }

        sink.set_volume(Volume::Linear(*smoothed_vol));
    }
}
