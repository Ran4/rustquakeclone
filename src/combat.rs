//! Damage, armor, splash explosions, death and gibbing.

use bevy::prelude::*;

use crate::common::*;
use crate::effects::spawn_gibs;

pub struct CombatPlugin;
impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (handle_explosions, apply_damage, crate::monster_model::do_sever, check_deaths)
                .chain()
                .run_if(in_state(GameState::Playing)),
        );
    }
}

/// Resolve explosion messages into per-target damage + knockback.
/// Player explosions hit everyone in range — including the firer, who is hurled
/// by the blast (a rocket/grenade jump) for reduced self-damage. Monster
/// explosions never hurt the firer or other monsters (no friendly fire).
pub(crate) fn handle_explosions(
    mut explosions: MessageReader<ExplosionEvent>,
    targets: Query<(Entity, &GlobalTransform, &Hurtbox, &Faction), With<Health>>,
    limb_boxes: Res<LimbBoxes>,
    mut damage: MessageWriter<DamageEvent>,
) {
    for ex in explosions.read() {
        for (e, gt, hb, faction) in &targets {
            let self_blast = ex.source == Some(e);
            // Whip-parried (returned) blast: it's now player-owned, but the player
            // who batted it back must NEVER be hurt by its splash (feature 39, the
            // #1 correctness requirement). The player is the blast's `source`, so
            // skipping the source entirely makes that immunity airtight regardless
            // of where the returned grenade detonates. Monsters are untouched by
            // this guard and still take the full returned-grenade splash.
            if ex.returned && self_blast {
                continue;
            }
            if !ex.from_player && *faction == Faction::Monster {
                continue; // monster splash doesn't harm other monsters (incl. the firer)
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
            if ex.implode {
                // Gravity well: pull monsters inward, zero damage, never the firer (self-pull guard).
                // `ex.push` is the per-frame inward velocity impulse magnitude (PULL_ACCEL * dt upstream).
                if self_blast {
                    continue;
                }
                // Taper near the centre so monsters settle into a clump instead of slingshotting through.
                let near = (dist / 2.0).clamp(0.0, 1.0); // 0 within ~2m of the well
                let pull = ex.push * falloff * near;
                // Pull along the ground only: a lobbed well sits above grounded monsters
                // (and around a flyer) while it's still bouncing, so a 3D pull would yank
                // them up/down. Horizontal-only drag also makes the clump tighter.
                let mut kb = -dir * pull; // `dir` is OUTWARD (well -> target); negate => inward
                kb.y = 0.0;
                damage.write(DamageEvent::body(e, 0.0, ex.source, kb));
                continue; // skip the normal damage + limb-splash path
            }
            // The firer is launched by their own blast (rocket/grenade jump), but
            // takes only half self-damage so a single jump isn't lethal.
            let dmg_scale = if self_blast { 0.5 } else { 1.0 };
            let kb = dir * ex.push * falloff + Vec3::Y * ex.push * 0.25 * falloff;
            damage.write(DamageEvent::body(e, ex.damage * falloff * dmg_scale, ex.source, kb));

            // A PLAYER blast also chips one limb (half value) so a rocket can finish
            // a limb you'd already chewed — the limb it struck directly if any, else
            // the nearest live limb. Body-level only; no per-bone splash raycasts.
            if ex.from_player && *faction == Faction::Monster {
                let limb = match ex.direct_limb {
                    Some((de, g)) if de == e => Some(g),
                    _ => nearest_limb_of(e, ex.pos, &limb_boxes),
                };
                if let Some(g) = limb {
                    damage.write(DamageEvent::limb(e, ex.damage * falloff * 0.5, ex.source, kb, g));
                }
            }
        }
    }
}

/// The live limb group of `e` whose box centre is nearest `pos` (for splash routing).
fn nearest_limb_of(e: Entity, pos: Vec3, boxes: &LimbBoxes) -> Option<LimbGroup> {
    let mut best: Option<(f32, LimbGroup)> = None;
    for &(be, g, bb) in &boxes.boxes {
        if be != e {
            continue;
        }
        let d = bb.center().distance(pos);
        if best.map_or(true, |(bd, _)| d < bd) {
            best = Some((d, g));
        }
    }
    best.map(|(_, g)| g)
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
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
    mut limbq: Query<&mut crate::monster_model::Limbs>,
    mount: Res<crate::mount::ActiveMount>,
    q_player: Query<Entity, With<crate::player::Player>>,
    mut sever_w: MessageWriter<SeverEvent>,
    mut sfx: MessageWriter<Sfx>,
    mut flash: MessageWriter<ScreenFlash>,
) {
    // Tank rule: while mounted, damage aimed at the player is soaked by the Ogre
    // (you ride on its HP and it dies under you). Resolved once per call.
    let player_e = q_player.single().ok();
    let redirect_to = mount.0;
    for ev in reader.read() {
        // Redirect a genuine player-targeted hit to the mounted Ogre. If the Ogre
        // is gone (overkill despawn, eject still resolving) the `get_mut` below
        // fails and we fall back to the player so the hit isn't silently dropped.
        let target = match (redirect_to, player_e) {
            (Some(ogre), Some(pe)) if ev.target == pe => ogre,
            _ => ev.target,
        };
        let resolved = if q.contains(target) { target } else { ev.target };
        // True when a player-targeted hit was actually soaked by the mounted Ogre
        // (tank rule). The rider's HP/pain now live on the Ogre, so the player
        // arm below never fires — give the rider a damped red flash so the ride
        // being whittled down is still legible.
        let redirected = redirect_to.is_some() && Some(resolved) == redirect_to && resolved != ev.target;
        let Ok((mut hp, armor, kb, faction, gt)) = q.get_mut(resolved) else {
            continue;
        };
        if hp.dead {
            // A dead body still takes knockback (feature 17 ragdoll corpses):
            // a rocket splash / Whip fling / ram impulse must reach its `Knockback`
            // so `ragdoll_solve` can fling/tumble it. We accumulate the impulse only —
            // the damage/pain/sever arms below are for the living, so we stop here.
            if ev.knockback != Vec3::ZERO {
                if let Some(mut k) = kb {
                    k.0 += ev.knockback;
                }
            }
            continue;
        }
        let en_kind = enemies.get(resolved).ok().map(|e| e.kind);
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
            // A zero-damage event is a pure knockback (the Lodestone well's inward
            // pull) — drag the monster silently, no hit-flash or pain grunt.
            _ if ev.amount <= 0.0 => {}
            _ => {
                // Hit-flash + pain flinch on the monster (the crunch of the hit).
                if let Ok(mut en) = enemies.get_mut(resolved) {
                    en.flash = 1.0;
                    if en.pain_cd <= 0.0 && hp.current > 0.0 {
                        en.pain = 0.22;
                        en.pain_cd = 0.55;
                        en.windup = 0.0; // a hit interrupts a pending shot
                        let pitch = crate::enemies::kind_pitch(en.kind);
                        sfx.write(Sfx::pitched(Sound::EnemyPain, pos, pitch));
                    }
                }
                // Tank rule: the hit was redirected off the player onto the mounted
                // Ogre, so the player arm above didn't run. Flash the rider (damped)
                // so they can tell their mount is soaking damage for them.
                if redirected && dmg > 0.0 {
                    flash.write(ScreenFlash { color: rgb(0.8, 0.0, 0.0), strength: (dmg / 80.0).clamp(0.1, 0.4) });
                }
            }
        }

        // Limb-targeted hits also pour into the limb's sever pool. When it empties,
        // sever the limb (a Head sever kills a normal outright / chunks a boss).
        if let Some(g) = ev.limb {
            if let Ok(mut limbs_mut) = limbq.get_mut(resolved) {
                let limbs = &mut *limbs_mut;
                let gi = g.idx();
                let max = limbs.max[gi];
                if max > 0.0 && !limbs.severed[gi] {
                    let severed_now = crate::monster_model::accrue(
                        &mut limbs.taken[gi],
                        max,
                        &mut limbs.severed[gi],
                        ev.amount.max(0.0),
                    );
                    if severed_now {
                        if g == LimbGroup::Head {
                            use crate::level::MonsterKind::*;
                            let boss = matches!(en_kind, Some(Ogre) | Some(DeathKnight));
                            if boss {
                                hp.current -= 120.0; // big chunk, not an instant decap
                            } else {
                                hp.current = hp.current.min(-30.0); // force the overkill gib
                            }
                        }
                        sever_w.write(SeverEvent {
                            root: resolved,
                            limb: g,
                            at: pos,
                            dir: ev.knockback.normalize_or_zero(),
                        });
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
