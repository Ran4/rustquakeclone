//! Damage, armor, splash explosions, death and gibbing.

use bevy::prelude::*;

use crate::common::*;
use crate::effects::spawn_gibs;

pub struct CombatPlugin;
impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (handle_explosions, apply_damage, check_deaths)
                .chain()
                .run_if(in_state(GameState::Playing)),
        );
    }
}

/// Resolve explosion messages into per-target damage + knockback.
/// Player explosions hurt everyone (so rocket-jumps cost self-damage); monster
/// explosions never hurt the firer or other monsters (no friendly fire).
fn handle_explosions(
    mut explosions: MessageReader<ExplosionEvent>,
    targets: Query<(Entity, &GlobalTransform, &Hurtbox, &Faction), With<Health>>,
    mut damage: MessageWriter<DamageEvent>,
) {
    for ex in explosions.read() {
        for (e, gt, hb, faction) in &targets {
            if ex.source == Some(e) {
                continue; // never self-damage the firer
            }
            if !ex.from_player && *faction == Faction::Monster {
                continue; // monster splash doesn't harm other monsters
            }
            let center = gt.translation();
            let to = center - ex.pos;
            let dist = to.length();
            if dist >= ex.radius {
                continue;
            }
            let falloff = 1.0 - dist / ex.radius;
            let dir = if dist > 0.01 { to / dist } else { Vec3::Y };
            let _ = hb;
            damage.write(DamageEvent {
                target: e,
                amount: ex.damage * falloff,
                source: ex.source,
                knockback: dir * ex.push * falloff + Vec3::Y * ex.push * 0.25 * falloff,
            });
        }
    }
}

#[allow(clippy::type_complexity)]
fn apply_damage(
    mut reader: MessageReader<DamageEvent>,
    mut q: Query<(
        &mut Health,
        Option<&mut Armor>,
        Option<&mut Knockback>,
        Option<&Faction>,
        &GlobalTransform,
    )>,
    mut enemies: Query<&mut crate::enemies::Enemy>,
    mut sfx: MessageWriter<Sfx>,
    mut flash: MessageWriter<ScreenFlash>,
) {
    for ev in reader.read() {
        let Ok((mut hp, armor, kb, faction, gt)) = q.get_mut(ev.target) else {
            continue;
        };
        if hp.dead {
            continue;
        }
        let mut dmg = ev.amount.max(0.0);
        if let Some(mut a) = armor {
            if a.points > 0.0 && a.absorb > 0.0 {
                let absorbed = (dmg * a.absorb).min(a.points);
                a.points -= absorbed;
                dmg -= absorbed;
            }
        }
        hp.current -= dmg;
        if let Some(mut k) = kb {
            k.0 += ev.knockback;
        }
        let pos = gt.translation();
        match faction {
            Some(Faction::Player) => {
                sfx.write(Sfx::global(Sound::PlayerPain));
                flash.write(ScreenFlash { color: rgb(0.8, 0.0, 0.0), strength: (dmg / 40.0).clamp(0.15, 0.7) });
            }
            _ => {
                // Hit-flash + pain flinch on the monster (the crunch of the hit).
                if let Ok(mut en) = enemies.get_mut(ev.target) {
                    en.flash = 1.0;
                    if en.pain_cd <= 0.0 && hp.current > 0.0 {
                        en.pain = 0.22;
                        en.pain_cd = 0.55;
                        en.windup = 0.0; // a hit interrupts a pending shot
                        let pitch = crate::enemies::kind_pitch(en.kind);
                        sfx.write(Sfx::pitched(Sound::EnemyPain, pos, pitch));
                    }
                }
            }
        }
    }
}

fn check_deaths(
    mut commands: Commands,
    mut q: Query<(Entity, &mut Health, &Faction, &GlobalTransform, &Transform)>,
    mut sfx: MessageWriter<Sfx>,
    mut next: ResMut<NextState<GameState>>,
    mut mission: ResMut<Mission>,
    gfx: Res<GfxAssets>,
) {
    for (e, mut hp, faction, gt, tf) in &mut q {
        if hp.current > 0.0 || hp.dead {
            continue;
        }
        hp.dead = true;
        let pos = gt.translation();
        match faction {
            Faction::Player => {
                sfx.write(Sfx::global(Sound::PlayerDeath));
                next.set(GameState::Dead);
            }
            Faction::Monster => {
                sfx.write(Sfx::at(Sound::EnemyDeath, pos));
                mission.kills += 1;
                let overkill = hp.current < -25.0; // rockets / big hits gib
                if overkill {
                    // Blown apart: the whole body explodes into gibs.
                    spawn_gibs(&mut commands, &gfx, pos, 16);
                    commands.entity(e).despawn();
                } else {
                    // Killed cleanly: the monster topples over and lingers as a corpse.
                    spawn_gibs(&mut commands, &gfx, pos, 4);
                    let yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
                    commands
                        .entity(e)
                        .insert(crate::monster_model::Dying { t: 0.0, yaw });
                }
            }
        }
    }
}
