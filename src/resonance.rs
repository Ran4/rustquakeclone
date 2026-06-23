//! Resonant brushwork — sonic demolition (feature 29).
//!
//! Every themed surface carries an [`AcousticProfile`](crate::level::AcousticProfile):
//! a fundamental ring note, a damping factor, and a *shatter pitch*. A subset of
//! brushes are authored **resonant** (a cracked-ice wall, a brass bulkhead…). The
//! Lightning Gun is the resonator: hold the beam on a resonant brush and its hum
//! climbs a continuous frequency sweep from the fundamental toward the shatter
//! note; reach the note and the brush detonates into gibs, its collider
//! degenerating and its mesh vanishing to open a shortcut.
//!
//! ## How it hooks into the engine
//!
//! - The Lightning path in [`crate::weapons`] already raycasts the world every
//!   beam pulse. We taught its `hitscan` to report the world slot the beam
//!   terminated on, and `fire_weapon` writes a [`BrushStrike`] message for it
//!   (`beam: true` for the Lightning beam, `false` for a one-shot pellet/nail).
//! - [`ResonantBrushes`](crate::level::ResonantBrushes) is the parallel index
//!   built at level setup: per resonant brush it stores the `solids` slot, the
//!   mesh entity, the acoustic profile and a live charge. (`WorldColliders.solids`
//!   is a flat untagged `Vec<Aabb>`, so this is how a slot maps back to a brush.)
//! - Shattering writes the same far-away [`degenerate`] sentinel a closed door /
//!   cut strand / parked truck slot uses — so the slot index stays stable (never
//!   `Vec::remove`, which would shift every later door/truck/corpse slot) while
//!   the collider goes inert and the route opens for free.
//!
//! The looping hum follows the truck-engine / grapple-rope pattern: spawn a
//! `LOOP` [`AudioPlayer`] with a marker the first frame the beam dwells on a
//! resonant brush, scrub its playback SPEED to sweep the pitch with the charge
//! and its VOLUME with the dwell, and despawn it the instant the beam leaves the
//! brush or the brush shatters.

use bevy::audio::Volume;
use bevy::prelude::*;

use crate::common::tune::*;
use crate::common::*;
use crate::effects::spawn_gibs;
use crate::level::{AcousticProfile, ResonantBrushes};

/// The reference tone (Hz) `resonant_hum.wav` is rendered at — playback speed is
/// scaled by `pitch_hz / HUM_REF_HZ` to sweep it. Must match `audio_gen.rs`.
const HUM_REF_HZ: f32 = 220.0;
/// 1/s the hum's pitch/volume chase their targets (mirrors `ENGINE_SMOOTH`), so
/// the sweep glides instead of stepping per frame.
const HUM_SMOOTH: f32 = 10.0;
/// Charge below which the hum is silenced + torn down (a hair above zero so a
/// brush decaying back to rest stops singing).
const HUM_MIN_CHARGE: f32 = 0.02;

/// Map a brush's charge (normalised 0..`RESONANT_SHATTER`) onto the playback
/// speed that sweeps `resonant_hum.wav` (rendered at `HUM_REF_HZ`) from the
/// profile's fundamental up to its shatter pitch — a continuous theremin sweep.
fn sweep_speed(profile: AcousticProfile, charge: f32) -> f32 {
    let frac = (charge / RESONANT_SHATTER).clamp(0.0, 1.0);
    let pitch = profile.fundamental_hz + (profile.shatter_pitch_hz - profile.fundamental_hz) * frac;
    pitch / HUM_REF_HZ
}

/// The single looping resonant-hum audio entity (present only while the beam
/// dwells on a resonant brush). Carries the live sweep target the audio scrub
/// reads — written by `resonance_charge`, consumed by `resonance_audio`.
#[derive(Component)]
struct ResonantHumSound {
    /// Target playback speed (`pitch_hz / HUM_REF_HZ`) for the current charge.
    target_speed: f32,
    /// Target volume, ramped with dwell so a fresh dwell fades in.
    target_vol: f32,
}

pub struct ResonancePlugin;
impl Plugin for ResonancePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (resonance_charge, resonance_audio)
                .chain()
                .run_if(in_state(GameState::Playing)),
        )
        // Tear the hum down on death/victory/level-change (the run_if(Playing)
        // path can't fire its normal "beam left the brush" release there).
        .add_systems(OnExit(GameState::Playing), cleanup_hum);
    }
}

/// The per-frame core: read this frame's brush strikes, charge the dwelt-on
/// resonant brush (decay all others), ring a struck one, and shatter any that
/// reached the note. Stashes the live sweep target on the hum entity for the
/// audio system to scrub.
fn resonance_charge(
    time: Res<Time>,
    mut commands: Commands,
    mut strikes: MessageReader<BrushStrike>,
    mut resonant: ResMut<ResonantBrushes>,
    mut colliders: ResMut<WorldColliders>,
    gfx: Res<GfxAssets>,
    mut sfx: MessageWriter<Sfx>,
    mut notify: MessageWriter<Notify>,
    mut q_hum: Query<&mut ResonantHumSound>,
) {
    let dt = time.delta_secs();

    // Collect this frame's strikes by slot: the beam dwell (one) and one-shot
    // rings (de-duped to the loudest per slot so a shotgun blast rings once).
    let mut beam_slot: Option<(usize, Vec3)> = None;
    let mut ring_slots: Vec<(usize, Vec3)> = Vec::new();
    for s in strikes.read() {
        if s.beam {
            beam_slot = Some((s.slot, s.point));
        } else if !ring_slots.iter().any(|(sl, _)| *sl == s.slot) {
            ring_slots.push((s.slot, s.point));
        }
    }

    // Charge the dwelt-on brush ONE notch per beam pulse (the beam writes a strike
    // once per its 0.06s cooldown — several frames apart — so charging per-frame
    // would be outrun by per-frame decay; we charge per pulse and only decay after
    // a short grace so the inter-pulse gap frames don't bleed the charge off).
    // Track the live sweep target (pitch + volume) so the audio system can scrub.
    let mut hum_target: Option<(f32, f32)> = None; // (speed, volume)
    let mut shatter: Option<usize> = None; // index into resonant.brushes to detonate
    for (i, rb) in resonant.brushes.iter_mut().enumerate() {
        let dwelt = beam_slot.map(|(slot, _)| slot == rb.slot).unwrap_or(false);
        if dwelt {
            // A beam pulse landed on this brush this frame: deposit a notch and
            // reset the grace clock.
            rb.charge = (rb.charge + RESONANT_CHARGE_PER_PULSE).min(RESONANT_SHATTER);
            rb.since_struck = 0.0;
            if rb.charge >= RESONANT_SHATTER {
                shatter = Some(i);
            }
        } else {
            // No pulse this frame. Decay only once we're past the grace window,
            // so the (multi-frame) gap between a held beam's pulses doesn't sag it.
            rb.since_struck += dt;
            if rb.since_struck > RESONANT_DECAY_GRACE {
                rb.charge = (rb.charge - RESONANT_DECAY_RATE * dt).max(0.0);
            }
        }
        // Whichever brush is most charged drives the hum — whether it's being
        // actively beamed (rising) or lingering/decaying (sagging). Volume scales
        // with charge so the hum fades down with the decay instead of freezing at
        // the pre-release level, and swells up from quiet on a fresh dwell.
        if rb.charge > HUM_MIN_CHARGE {
            let frac = (rb.charge / RESONANT_SHATTER).clamp(0.0, 1.0);
            let vol = (0.25 + 0.55 * frac).min(0.85);
            let target = (sweep_speed(rb.profile, rb.charge), vol);
            if hum_target.map(|(_, v)| vol > v).unwrap_or(true) {
                hum_target = Some(target);
            }
        }
    }

    // Telegraph rings: a struck resonant brush briefly tinks at its fundamental so
    // the world reads as audibly material (only resonant brushes, to dodge spam).
    // Uses the short, decaying `ResonantRing` one-shot — NOT the sustained hum loop
    // — so a grazing pellet strikes-and-dies rather than emitting a held note.
    for (slot, point) in ring_slots {
        if let Some(rb) = resonant.brushes.iter().find(|rb| rb.slot == slot) {
            // Pitch the ring sample down/up to the brush's fundamental.
            sfx.write(Sfx {
                sound: Sound::ResonantRing,
                pos: Some(point),
                volume: RESONANT_RING_VOL,
                pitch: rb.profile.fundamental_hz / HUM_REF_HZ,
            });
        }
    }

    // Detonate: degenerate the collider slot (route opens), hide the mesh, gib it,
    // pop it, and drop it from the live set so it stops charging/singing.
    if let Some(i) = shatter {
        let rb = resonant.brushes.remove(i);
        if let Some(s) = colliders.solids.get_mut(rb.slot) {
            *s = degenerate();
        }
        // The shatter point is the beam's terminal hit on the brush this frame
        // (a shatter only happens on a dwell frame, so `beam_slot` is always set).
        let pos = beam_slot.map(|(_, p)| p).unwrap_or(Vec3::ZERO);
        // Hide rather than despawn so the `LevelEntity` teardown still owns it.
        commands.entity(rb.entity).insert(Visibility::Hidden);
        spawn_gibs(&mut commands, &gfx, pos, 14);
        sfx.write(Sfx::at(Sound::ResonantShatter, pos));
        notify.write(Notify::new("The wall shatters open!"));
    }

    // Push the sweep target to the hum entity (if one is live). This is set for any
    // charged brush — rising while beamed, sagging while it decays — so the hum
    // tracks the charge both up and down. Once no brush is charged at all, no target
    // is pushed and `resonance_audio` fades the hum out and despawns it.
    if let (Some((speed, vol)), Ok(mut hum)) = (hum_target, q_hum.single_mut()) {
        hum.target_speed = speed;
        hum.target_vol = vol;
    }
}

/// A degenerate AABB parked a million metres away — the inert sentinel a closed
/// door / cut strand / parked truck / settled corpse slot all share. Disabling a
/// slot this way keeps every later index stable (no `Vec::remove` shift).
fn degenerate() -> crate::physics::Aabb {
    crate::physics::Aabb { min: Vec3::splat(1.0e6), max: Vec3::splat(1.0e6 + 0.01) }
}

/// Spawn / scrub / tear down the looping resonant hum, mirroring the truck's
/// `vehicle_audio` three-state `AudioSink` handling (the sink goes live a frame
/// or two after the entity spawns). The hum spawns while some resonant brush is
/// charged above `HUM_MIN_CHARGE` and scrubs its pitch/volume toward the live
/// target `resonance_charge` stashed (which sags as the charge decays). When no
/// brush is charged any more it ramps the volume to silence and despawns only
/// once faded out, so teardown doesn't click.
fn resonance_audio(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Res<Sounds>,
    resonant: Res<ResonantBrushes>,
    mut q_hum: Query<(Entity, Option<&mut AudioSink>, &ResonantHumSound)>,
) {
    let dt = time.delta_secs();
    // A hum should be alive iff some brush is mid-sweep (charged). The dwell
    // target itself is one-frame; the charge is the persistent gate so the hum
    // doesn't strobe on a tap-fired beam between pulses — it lingers until the
    // charge decays back to rest.
    let any_charged = resonant.brushes.iter().any(|rb| rb.charge > HUM_MIN_CHARGE);

    match q_hum.single_mut() {
        Ok((e, sink, hum)) => {
            // Glide pitch toward the live target always; for volume, chase the live
            // dwell/decay target while a brush is charged, else ramp toward silence.
            // Despawn only once actually faded out (never mid-tone) so teardown
            // doesn't click — mirrors how the truck engine fades on release.
            let target_vol = if any_charged { hum.target_vol } else { 0.0 };
            if let Some(mut sink) = sink {
                let k = 1.0 - (-HUM_SMOOTH * dt).exp();
                let p = sink.speed();
                sink.set_speed(p + (hum.target_speed - p) * k);
                let cv = sink.volume().to_linear();
                let nv = cv + (target_vol - cv) * k;
                sink.set_volume(Volume::Linear(nv));
                if !any_charged && nv < 0.01 {
                    commands.entity(e).despawn();
                }
            } else if !any_charged {
                // Sink not live yet (spawned the same frame the charge fell away):
                // nothing audible to fade, so just tear it down.
                commands.entity(e).despawn();
            }
        }
        Err(_) => {
            if any_charged {
                // Seed the entity at the highest-charged brush's current note so it
                // fades up from there rather than from the reference tone.
                let (speed, vol) = resonant
                    .brushes
                    .iter()
                    .filter(|rb| rb.charge > HUM_MIN_CHARGE)
                    .max_by(|a, b| a.charge.total_cmp(&b.charge))
                    .map(|rb| (sweep_speed(rb.profile, rb.charge), 0.25))
                    .unwrap_or((1.0, 0.25));
                commands.spawn((
                    AudioPlayer::new(sounds.get(Sound::ResonantHum)),
                    PlaybackSettings::LOOP
                        .with_volume(Volume::Linear(0.0))
                        .with_speed(speed),
                    ResonantHumSound { target_speed: speed, target_vol: vol },
                    Name::new("ResonantHumSound"),
                ));
            }
        }
    }
}

/// Tear the hum down on death/victory/level-change.
fn cleanup_hum(mut commands: Commands, q: Query<Entity, With<ResonantHumSound>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ice voice: fundamental 440 Hz, shatter 1320 Hz, sample rendered at 220 Hz.
    const ICE: AcousticProfile = AcousticProfile::new(440.0, 16.0, 1320.0);

    /// At zero charge the sweep sits on the fundamental; the speed is
    /// fundamental / reference (440/220 = 2.0).
    #[test]
    fn sweep_starts_at_the_fundamental() {
        assert!((sweep_speed(ICE, 0.0) - 2.0).abs() < 1e-4);
    }

    /// At full charge the sweep has climbed to the shatter pitch (1320/220 = 6.0).
    #[test]
    fn sweep_ends_at_the_shatter_pitch() {
        assert!((sweep_speed(ICE, RESONANT_SHATTER) - 6.0).abs() < 1e-4);
    }

    /// The sweep is monotonic and clamps past full charge (so an over-charged
    /// frame can't overshoot the shatter note).
    #[test]
    fn sweep_is_monotonic_and_clamped() {
        let mid = sweep_speed(ICE, RESONANT_SHATTER * 0.5);
        assert!(sweep_speed(ICE, 0.0) < mid && mid < sweep_speed(ICE, RESONANT_SHATTER));
        assert!((sweep_speed(ICE, RESONANT_SHATTER * 2.0) - 6.0).abs() < 1e-4);
    }

    /// Replicate the per-brush charge update from `resonance_charge` (per-pulse
    /// charge, grace-gated decay) and return seconds of held beam until shatter, or
    /// `None` if it plateaus (never shatters) within `cap` seconds. `fps` is the
    /// render rate; the beam deposits one pulse every Lightning cooldown (0.06s).
    fn time_to_shatter(fps: f32, cap: f32) -> Option<f32> {
        let dt = 1.0 / fps;
        let cooldown = 0.06; // Lightning Stats::cooldown
        let mut charge = 0.0f32;
        let mut since_struck = f32::INFINITY;
        let mut cd = 0.0f32; // beam cooldown clock
        let mut t = 0.0f32;
        while t < cap {
            // The beam fires a pulse whenever its cooldown reaches zero (held fire).
            cd -= dt;
            let pulse = cd <= 0.0;
            if pulse {
                cd = cooldown;
                charge = (charge + RESONANT_CHARGE_PER_PULSE).min(RESONANT_SHATTER);
                since_struck = 0.0;
                if charge >= RESONANT_SHATTER {
                    return Some(t);
                }
            } else {
                since_struck += dt;
                if since_struck > RESONANT_DECAY_GRACE {
                    charge = (charge - RESONANT_DECAY_RATE * dt).max(0.0);
                }
            }
            t += dt;
        }
        None
    }

    /// A continuously-held beam must shatter the brush at every plausible frame
    /// rate — and in roughly the documented ~2.2s, frame-rate-independent (the old
    /// per-frame charge was outrun by per-frame decay and NEVER shattered).
    #[test]
    fn held_beam_shatters_across_frame_rates() {
        for &fps in &[30.0f32, 60.0, 75.0, 120.0, 144.0, 240.0] {
            let t = time_to_shatter(fps, 10.0)
                .unwrap_or_else(|| panic!("never shattered at {fps} fps"));
            assert!(
                (1.8..3.0).contains(&t),
                "shatter at {fps} fps took {t:.2}s, expected ~2.2s",
            );
        }
    }

    /// Releasing the beam past the grace window must bleed the charge back to rest
    /// (you can't park a half-charged brush forever).
    #[test]
    fn charge_decays_once_grace_elapses() {
        let dt = 1.0 / 60.0;
        let mut charge = 0.5f32;
        let mut since_struck = 0.0f32;
        // No pulses ever again; advance a couple of seconds.
        let mut t = 0.0;
        while t < 2.0 {
            since_struck += dt;
            if since_struck > RESONANT_DECAY_GRACE {
                charge = (charge - RESONANT_DECAY_RATE * dt).max(0.0);
            }
            t += dt;
        }
        assert!(charge <= HUM_MIN_CHARGE, "charge should decay to rest, got {charge}");
    }
}
