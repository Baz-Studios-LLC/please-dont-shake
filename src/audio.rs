//! Music system for "Please Don't Shake".
//!
//! Plays slow, peaceful classical piano background music on a continuous seamless loop.

use bevy::audio::Volume;
use bevy::prelude::*;

/// Marker component for the ambient background piano music.
#[derive(Component)]
pub struct BackgroundMusic;

/// Spawns the background music player on startup.
pub fn setup_music(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        BackgroundMusic,
        AudioPlayer::new(asset_server.load("music/cozy_piano.wav")),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.65)),
    ));
}
