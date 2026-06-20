//! Sound playback: loads the procedurally generated WAVs and plays Sfx messages
//! with simple distance attenuation. Also runs the looping ambient drone.

use bevy::audio::Volume;
use bevy::prelude::*;

use crate::common::*;

pub struct AudioPlugin;
impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_sounds)
            .add_systems(OnEnter(GameState::Playing), start_ambient)
            .add_systems(Update, play_sfx);
    }
}

fn load_sounds(asset_server: Res<AssetServer>, mut sounds: ResMut<Sounds>) {
    for s in Sound::all() {
        sounds.map.insert(s, asset_server.load(s.file()));
    }
}

#[derive(Component)]
struct Ambient;

fn start_ambient(
    mut commands: Commands,
    sounds: Res<Sounds>,
    existing: Query<Entity, With<Ambient>>,
) {
    // Avoid stacking ambient loops across restarts.
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        AudioPlayer::new(sounds.get(Sound::Ambient)),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.35)),
        Ambient,
        Name::new("Ambient"),
    ));
}

fn play_sfx(
    mut commands: Commands,
    mut reader: MessageReader<Sfx>,
    sounds: Res<Sounds>,
    listener: Query<&GlobalTransform, With<Camera3d>>,
) {
    let ear = listener
        .iter()
        .next()
        .map(|g| g.translation())
        .unwrap_or(Vec3::ZERO);

    for ev in reader.read() {
        let mut vol = ev.volume;
        if let Some(p) = ev.pos {
            let d = p.distance(ear);
            vol *= (1.0 - d / 45.0).clamp(0.0, 1.0).powf(1.3);
        }
        if vol <= 0.002 {
            continue;
        }
        commands.spawn((
            AudioPlayer::new(sounds.get(ev.sound)),
            PlaybackSettings::DESPAWN
                .with_volume(Volume::Linear(vol.min(1.0)))
                .with_speed(ev.pitch),
            LevelEntity,
        ));
    }
}
