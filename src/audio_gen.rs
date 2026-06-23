//! Procedural sound-effect synthesis. Pure `std` — no external crates, no Bevy.
//!
//! At startup we synthesize a full Quake-flavoured SFX set as 16-bit PCM mono WAV
//! files into `assets/sounds/`, which the Bevy `AssetServer` then loads normally.
//! This keeps the repo free of binary art assets while still shipping real audio.

use std::f32::consts::TAU;
use std::fs;
use std::io::Write;
use std::path::Path;

const SR: u32 = 22_050; // sample rate (plenty for gritty retro SFX, smaller files)

/// Tiny deterministic xorshift RNG so noise is reproducible across runs.
struct Rng(u32);
impl Rng {
    fn new(seed: u32) -> Self {
        Rng(seed | 1)
    }
    fn next_f32(&mut self) -> f32 {
        // xorshift32 -> [-1, 1)
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// A mono buffer of f32 samples we mutate with synthesis primitives.
struct Buf {
    s: Vec<f32>,
}
impl Buf {
    fn secs(t: f32) -> Self {
        Buf {
            s: vec![0.0; (t * SR as f32) as usize],
        }
    }
    /// Add a sine tone with linear frequency sweep (f0->f1) and an exp decay env.
    fn sine_sweep(&mut self, f0: f32, f1: f32, amp: f32, decay: f32) {
        let n = self.s.len() as f32;
        let mut phase = 0.0f32;
        for (i, out) in self.s.iter_mut().enumerate() {
            let t = i as f32 / SR as f32;
            let frac = i as f32 / n;
            let f = f0 + (f1 - f0) * frac;
            phase += TAU * f / SR as f32;
            let env = (-t * decay).exp();
            *out += (phase).sin() * amp * env;
        }
    }

    /// Add band-ish noise (one-pole lowpass + optional highpass) with exp decay.
    fn noise(&mut self, amp: f32, decay: f32, lp: f32, seed: u32) {
        let mut rng = Rng::new(seed);
        let mut lp_state = 0.0f32;
        let a = lp.clamp(0.0, 1.0);
        for (i, out) in self.s.iter_mut().enumerate() {
            let t = i as f32 / SR as f32;
            let white = rng.next_f32();
            lp_state += a * (white - lp_state);
            let env = (-t * decay).exp();
            *out += lp_state * amp * env;
        }
    }

    /// Square-ish buzz (cheap monster vocal-cord texture) with vibrato.
    fn buzz(&mut self, f: f32, vib: f32, amp: f32, decay: f32) {
        let mut phase = 0.0f32;
        for (i, out) in self.s.iter_mut().enumerate() {
            let t = i as f32 / SR as f32;
            let fm = f + (TAU * vib * t).sin() * f * 0.06;
            phase += TAU * fm / SR as f32;
            let sq = if phase.sin() >= 0.0 { 1.0 } else { -1.0 };
            let env = (-t * decay).exp();
            *out += sq * amp * env;
        }
    }

    /// Apply an attack ramp (seconds) to avoid clicks at the start.
    fn attack(&mut self, secs: f32) {
        let n = (secs * SR as f32) as usize;
        for i in 0..n.min(self.s.len()) {
            self.s[i] *= i as f32 / n as f32;
        }
    }

    /// Normalize + soft clip to keep peaks musical, then return.
    fn finish(mut self, peak: f32) -> Vec<f32> {
        let mx = self.s.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        if mx > 1e-6 {
            let g = peak / mx;
            for v in &mut self.s {
                let x = *v * g;
                *v = x.tanh(); // soft clip
            }
        }
        self.s
    }
}

fn write_wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    let mut data = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        data.extend_from_slice(&v.to_le_bytes());
    }
    let n = data.len() as u32;
    let mut f = fs::File::create(path)?;
    // RIFF / WAVE header (PCM, mono, 16-bit)
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + n).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?; // fmt chunk size
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&SR.to_le_bytes())?;
    f.write_all(&(SR * 2).to_le_bytes())?; // byte rate
    f.write_all(&2u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits
    f.write_all(b"data")?;
    f.write_all(&n.to_le_bytes())?;
    f.write_all(&data)?;
    Ok(())
}

// ---- individual sound designs -------------------------------------------------

fn shotgun() -> Vec<f32> {
    let mut b = Buf::secs(0.28);
    b.noise(1.0, 26.0, 0.55, 0x1111); // bright crack
    b.noise(0.7, 12.0, 0.12, 0x2222); // body
    b.sine_sweep(160.0, 60.0, 0.8, 18.0); // low thump
    b.attack(0.002);
    b.finish(0.95)
}

fn super_shotgun() -> Vec<f32> {
    let mut b = Buf::secs(0.42);
    b.noise(1.0, 16.0, 0.5, 0x3333);
    b.noise(0.8, 9.0, 0.1, 0x4444);
    b.sine_sweep(140.0, 45.0, 1.0, 11.0);
    b.sine_sweep(90.0, 40.0, 0.6, 9.0);
    b.attack(0.002);
    b.finish(0.97)
}

fn nailgun() -> Vec<f32> {
    let mut b = Buf::secs(0.09);
    b.noise(0.7, 60.0, 0.9, 0x5151);
    b.sine_sweep(900.0, 500.0, 0.6, 40.0);
    b.attack(0.001);
    b.finish(0.8)
}

fn rocket_fire() -> Vec<f32> {
    let mut b = Buf::secs(0.5);
    b.noise(1.0, 6.0, 0.35, 0x6262); // whoosh
    b.sine_sweep(220.0, 120.0, 0.5, 7.0);
    b.attack(0.01);
    b.finish(0.9)
}

fn grenade_fire() -> Vec<f32> {
    let mut b = Buf::secs(0.18);
    b.noise(0.8, 30.0, 0.4, 0x7373);
    b.sine_sweep(260.0, 110.0, 0.7, 22.0);
    b.attack(0.003);
    b.finish(0.85)
}

fn explosion() -> Vec<f32> {
    let mut b = Buf::secs(0.9);
    b.noise(1.0, 5.0, 0.25, 0x8484); // rumble
    b.noise(0.9, 12.0, 0.7, 0x8a8a); // initial crack
    b.sine_sweep(120.0, 30.0, 1.0, 4.5); // deep boom
    b.sine_sweep(70.0, 22.0, 0.7, 3.5);
    b.attack(0.002);
    b.finish(1.0)
}

fn grenade_bounce() -> Vec<f32> {
    let mut b = Buf::secs(0.12);
    b.sine_sweep(680.0, 520.0, 0.7, 35.0);
    b.noise(0.3, 50.0, 0.8, 0x9595);
    b.attack(0.001);
    b.finish(0.6)
}

fn impact() -> Vec<f32> {
    let mut b = Buf::secs(0.08);
    b.noise(0.8, 70.0, 0.6, 0xa6a6);
    b.attack(0.001);
    b.finish(0.55)
}

fn pickup_health() -> Vec<f32> {
    let mut b = Buf::secs(0.22);
    b.sine_sweep(520.0, 880.0, 0.7, 10.0);
    b.sine_sweep(780.0, 1320.0, 0.3, 12.0);
    b.attack(0.004);
    b.finish(0.7)
}

fn pickup_armor() -> Vec<f32> {
    let mut b = Buf::secs(0.26);
    b.sine_sweep(300.0, 300.0, 0.5, 8.0);
    b.sine_sweep(450.0, 600.0, 0.5, 9.0);
    b.attack(0.004);
    b.finish(0.7)
}

fn pickup_ammo() -> Vec<f32> {
    let mut b = Buf::secs(0.12);
    b.sine_sweep(420.0, 620.0, 0.6, 20.0);
    b.attack(0.003);
    b.finish(0.6)
}

fn pickup_weapon() -> Vec<f32> {
    let mut b = Buf::secs(0.4);
    b.sine_sweep(330.0, 660.0, 0.6, 5.0);
    b.sine_sweep(440.0, 880.0, 0.4, 6.0);
    b.sine_sweep(550.0, 990.0, 0.3, 7.0);
    b.attack(0.005);
    b.finish(0.75)
}

fn key_pickup() -> Vec<f32> {
    let mut b = Buf::secs(0.5);
    b.sine_sweep(880.0, 1760.0, 0.5, 4.0);
    b.sine_sweep(1320.0, 2200.0, 0.3, 5.0);
    b.attack(0.005);
    b.finish(0.65)
}

fn jump() -> Vec<f32> {
    let mut b = Buf::secs(0.12);
    b.noise(0.4, 30.0, 0.25, 0xb7b7);
    b.sine_sweep(180.0, 320.0, 0.5, 20.0);
    b.attack(0.003);
    b.finish(0.5)
}

fn land() -> Vec<f32> {
    let mut b = Buf::secs(0.14);
    b.noise(0.5, 28.0, 0.18, 0xc8c8);
    b.sine_sweep(150.0, 70.0, 0.6, 24.0);
    b.attack(0.002);
    b.finish(0.55)
}

fn player_pain() -> Vec<f32> {
    let mut b = Buf::secs(0.22);
    b.buzz(220.0, 18.0, 0.6, 12.0);
    b.noise(0.3, 18.0, 0.4, 0xd9d9);
    b.attack(0.004);
    b.finish(0.7)
}

fn player_death() -> Vec<f32> {
    let mut b = Buf::secs(0.7);
    b.buzz(200.0, 10.0, 0.7, 4.0);
    b.sine_sweep(220.0, 60.0, 0.5, 4.5);
    b.attack(0.006);
    b.finish(0.8)
}

fn enemy_sight() -> Vec<f32> {
    let mut b = Buf::secs(0.4);
    b.buzz(140.0, 8.0, 0.8, 5.0);
    b.sine_sweep(120.0, 180.0, 0.4, 4.0);
    b.attack(0.006);
    b.finish(0.8)
}

fn enemy_pain() -> Vec<f32> {
    let mut b = Buf::secs(0.2);
    b.buzz(300.0, 22.0, 0.6, 14.0);
    b.attack(0.004);
    b.finish(0.65)
}

fn enemy_death() -> Vec<f32> {
    let mut b = Buf::secs(0.55);
    b.buzz(260.0, 14.0, 0.7, 5.0);
    b.sine_sweep(280.0, 70.0, 0.5, 5.0);
    b.noise(0.3, 8.0, 0.3, 0xeaea);
    b.attack(0.006);
    b.finish(0.75)
}

fn door() -> Vec<f32> {
    let mut b = Buf::secs(0.8);
    b.noise(0.6, 3.5, 0.06, 0xfbfb); // grinding rumble
    b.sine_sweep(80.0, 60.0, 0.5, 2.0);
    b.attack(0.02);
    b.finish(0.6)
}

fn victory() -> Vec<f32> {
    // simple ascending arpeggio
    let notes = [392.0, 523.0, 659.0, 784.0];
    let mut full: Vec<f32> = Vec::new();
    for (i, &f) in notes.iter().enumerate() {
        let mut b = Buf::secs(0.18);
        b.sine_sweep(f, f, 0.6, 5.0);
        b.sine_sweep(f * 2.0, f * 2.0, 0.2, 6.0);
        b.attack(0.005);
        let amp = 1.0 - i as f32 * 0.0;
        for s in b.finish(0.7 * amp) {
            full.push(s);
        }
    }
    full
}

fn lightning() -> Vec<f32> {
    // electric crackle: buzzy high tone + bright noise
    let mut b = Buf::secs(0.12);
    b.buzz(1200.0, 60.0, 0.5, 18.0);
    b.buzz(800.0, 40.0, 0.3, 16.0);
    b.noise(0.6, 30.0, 0.95, 0x5ec7);
    b.attack(0.001);
    b.finish(0.7)
}

fn whip() -> Vec<f32> {
    // A leather crack: a fast descending air-whistle that ends in a sharp snap.
    let mut b = Buf::secs(0.16);
    b.noise(0.5, 22.0, 0.5, 0x3c3c); // air whoosh
    b.sine_sweep(1800.0, 280.0, 0.5, 26.0); // descending whistle
    b.noise(1.0, 95.0, 0.95, 0x7a7a); // sharp crack transient
    b.attack(0.002);
    b.finish(0.8)
}

fn rope_taut() -> Vec<f32> {
    // Taut-cable tension: two close low tones beating slowly + a faint fiber
    // creak, no hard transient so the loop point is seamless (~1.5s).
    let mut b = Buf::secs(1.5);
    let n = b.s.len();
    let mut rng = Rng::new(0x2099);
    let mut lp = 0.0f32;
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let hum = (TAU * 90.0 * t).sin() * 0.5
            + (TAU * 91.0 * t).sin() * 0.3
            + (TAU * 270.0 * t).sin() * 0.08; // creak overtone
        let w = rng.next_f32();
        lp += 0.03 * (w - lp); // fiber rasp
        let wobble = 0.75 + 0.25 * (TAU * 0.8 * t).sin();
        b.s[i] = (hum + lp * 0.15) * wobble;
    }
    b.finish(0.45)
}

fn sever() -> Vec<f32> {
    // A limb torn off: a meaty low thud, a wet tearing rasp, and a sharp bone crack.
    let mut b = Buf::secs(0.35);
    b.sine_sweep(180.0, 40.0, 0.7, 18.0); // meaty thud
    b.noise(0.5, 9.0, 0.25, 0x5e11); // wet tear
    b.sine_sweep(900.0, 300.0, 0.25, 40.0); // bone-crack tick
    b.attack(0.002);
    b.finish(0.9)
}

fn corpse_settle() -> Vec<f32> {
    // A knocked corpse skids and flops to rest: a soft low meaty thud under a
    // brief gritty scrape, no sharp transient (it's a settle, not a hit). Quiet —
    // it fades in/out under combat rather than announcing itself.
    let mut b = Buf::secs(0.22);
    b.sine_sweep(120.0, 45.0, 0.5, 20.0); // dull body thud
    b.noise(0.35, 16.0, 0.2, 0x4c07); // gritty floor scrape
    b.attack(0.006);
    b.finish(0.4)
}

fn lodestone() -> Vec<f32> {
    // An energy well powering up: a low rising sweep under a vibrato'd buzz with a
    // touch of noise — an ominous warbling charge/hum, distinct from the boom (~0.7s).
    let mut b = Buf::secs(0.7);
    b.sine_sweep(70.0, 150.0, 0.8, 1.5); // low rising swell
    b.buzz(110.0, 9.0, 0.5, 2.0); // warbling well hum
    b.buzz(160.0, 6.0, 0.25, 2.5); // overtone shimmer
    b.noise(0.2, 3.0, 0.18, 0x10de); // faint energy fizz
    b.attack(0.02);
    b.finish(0.7)
}

fn resonant_hum() -> Vec<f32> {
    // Feature 29: the loop a resonant brush sings while the Lightning beam dwells
    // on it. The game scrubs playback SPEED to sweep the pitch up toward the
    // shatter note, so this is rendered at a fixed reference tone (220 Hz) — a
    // clean, near-pure sine with a touch of octave + fifth shimmer so it reads as
    // a struck-material overtone, NOT a buzzy kazoo. Seamlessly loopable (~1.2s),
    // no transient, no noise, a slow tremolo so a sustained dwell breathes.
    let mut b = Buf::secs(1.2);
    let n = b.s.len();
    let f = 220.0;
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let body = (TAU * f * t).sin() * 0.7         // fundamental
            + (TAU * f * 2.0 * t).sin() * 0.18       // octave
            + (TAU * f * 3.0 * t).sin() * 0.06;      // a touch of the fifth-ish 3rd harmonic
        let tremolo = 0.85 + 0.15 * (TAU * 5.5 * t).sin();
        b.s[i] = body * tremolo;
    }
    b.finish(0.6)
}

fn resonant_ring() -> Vec<f32> {
    // Feature 29: the brief percussive "tink" a resonant brush rings with when a
    // pellet/nail/body grazes it — the same clean harmonic stack as `resonant_hum`
    // (so it shares the brush's voice when pitch-scaled) but SHORT, with a hard
    // attack and an exponential decay so it strikes and dies like a struck pane,
    // NOT the sustained drone the held beam drives. Rendered at the same 220 Hz
    // reference so the game can pitch it to each brush's fundamental.
    let mut b = Buf::secs(0.25);
    let n = b.s.len();
    let f = 220.0;
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let body = (TAU * f * t).sin() * 0.7         // fundamental
            + (TAU * f * 2.0 * t).sin() * 0.18       // octave
            + (TAU * f * 3.0 * t).sin() * 0.06;      // a touch of the 3rd harmonic
        let decay = (-13.0 * t).exp(); // tinks and dies
        b.s[i] = body * decay;
    }
    b.attack(0.002);
    b.finish(0.6)
}

fn resonant_shatter() -> Vec<f32> {
    // Feature 29: a resonant brush hitting its breaking note and detonating — a
    // bright cracking ring (a quick descending chime, the sweep "snapping") under
    // a sharp shatter burst and a low collapse thud, so it lands as glass/stone
    // breaking rather than just an explosion.
    let mut b = Buf::secs(0.6);
    b.sine_sweep(1400.0, 300.0, 0.6, 16.0); // the ring snapping down as it cracks
    b.sine_sweep(900.0, 220.0, 0.35, 14.0); // overtone
    b.noise(0.9, 22.0, 0.9, 0x5ea7); // sharp shatter burst
    b.sine_sweep(150.0, 50.0, 0.5, 9.0); // low collapse thud
    b.attack(0.001);
    b.finish(0.9)
}

fn wallrun_scuff() -> Vec<f32> {
    // Feature 37: the loop boots make dragging along a wall while wall-running.
    // A gritty, low-mid filtered noise scrape — band-limited white noise with a
    // slow rhythmic "stride" amplitude wobble (so it reads as repeated footfall
    // scuffs, not a flat hiss) and a faint friction tone. NO transient and a
    // seamless head/tail so the LOOP point is inaudible (~0.5s). The game fades
    // its volume with wall-run speed and tears it down on peel-off.
    let mut b = Buf::secs(0.5);
    let n = b.s.len();
    let mut rng = Rng::new(0x37a1);
    let mut lp = 0.0f32; // one-pole lowpass -> rumble body
    let mut hp_prev = 0.0f32; // simple highpass state -> grit
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let white = rng.next_f32();
        lp += 0.18 * (white - lp); // ~low-mid band scrape body
        // highpass = signal - lowpassed signal: keeps the gritty hiss on top.
        let hp = lp - hp_prev;
        hp_prev += 0.05 * (lp - hp_prev);
        // Two overlapping stride wobbles (a slight phase mismatch) so the scuff
        // has an organic loping cadence rather than a single pure tremolo.
        let stride = 0.55
            + 0.30 * (TAU * 7.0 * t).sin().abs()
            + 0.15 * (TAU * 3.3 * t + 1.1).sin().abs();
        let friction = (TAU * 95.0 * t).sin() * 0.05; // faint friction tone
        b.s[i] = (lp * 0.7 + hp * 0.5 + friction) * stride;
    }
    b.finish(0.5)
}

fn metallic_ting() -> Vec<f32> {
    // Feature 39: the sharp "ting" the Whip rings when it bats a projectile out of
    // the air — a struck-steel ping. A bright inharmonic chime (a high fundamental
    // with slightly-stretched, non-integer partials so it reads as struck METAL,
    // not a tuned bell) with a hard attack and a fast exponential decay, plus a
    // tiny noise tick for the contact. Short (~0.22s). A perfect parry pitches the
    // whole clip up at playback time (Sfx pitch), so it's rendered at a mid tone.
    let mut b = Buf::secs(0.22);
    let n = b.s.len();
    let f = 1500.0; // bright steel fundamental
    // Stretched (inharmonic) partials => "clang", not "bell".
    let partials = [(1.0f32, 0.7f32), (2.76, 0.35), (5.40, 0.18), (8.93, 0.08)];
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let mut s = 0.0f32;
        for &(mult, amp) in &partials {
            // higher partials die faster (metal's bright transient settling to a hum)
            let decay = (-(18.0 + mult * 6.0) * t).exp();
            s += (TAU * f * mult * t).sin() * amp * decay;
        }
        b.s[i] = s;
    }
    b.noise(0.5, 120.0, 0.95, 0x71a9); // crisp contact tick
    b.attack(0.0008);
    b.finish(0.75)
}

fn ambient() -> Vec<f32> {
    // low, slowly-beating drone, loopable (~3s)
    let mut b = Buf::secs(3.0);
    let n = b.s.len();
    let mut rng = Rng::new(0x2024);
    let mut lp = 0.0f32;
    for i in 0..n {
        let t = i as f32 / SR as f32;
        let drone = (TAU * 55.0 * t).sin() * 0.5
            + (TAU * 55.5 * t).sin() * 0.4
            + (TAU * 110.0 * t).sin() * 0.15;
        let w = rng.next_f32();
        lp += 0.02 * (w - lp);
        let swell = 0.6 + 0.4 * (TAU * 0.12 * t).sin();
        b.s[i] = (drone + lp * 0.3) * swell;
    }
    b.finish(0.35)
}

/// (filename, generator) table.
fn table() -> Vec<(&'static str, fn() -> Vec<f32>)> {
    vec![
        ("shotgun.wav", shotgun),
        ("super_shotgun.wav", super_shotgun),
        ("nailgun.wav", nailgun),
        ("rocket_fire.wav", rocket_fire),
        ("grenade_fire.wav", grenade_fire),
        ("explosion.wav", explosion),
        ("grenade_bounce.wav", grenade_bounce),
        ("impact.wav", impact),
        ("pickup_health.wav", pickup_health),
        ("pickup_armor.wav", pickup_armor),
        ("pickup_ammo.wav", pickup_ammo),
        ("pickup_weapon.wav", pickup_weapon),
        ("key_pickup.wav", key_pickup),
        ("jump.wav", jump),
        ("land.wav", land),
        ("player_pain.wav", player_pain),
        ("player_death.wav", player_death),
        ("enemy_sight.wav", enemy_sight),
        ("enemy_pain.wav", enemy_pain),
        ("enemy_death.wav", enemy_death),
        ("door.wav", door),
        ("victory.wav", victory),
        ("ambient.wav", ambient),
        ("lightning.wav", lightning),
        ("whip.wav", whip),
        ("rope_taut.wav", rope_taut),
        ("sever.wav", sever),
        ("lodestone.wav", lodestone),
        ("corpse_settle.wav", corpse_settle),
        ("resonant_hum.wav", resonant_hum),
        ("resonant_ring.wav", resonant_ring),
        ("resonant_shatter.wav", resonant_shatter),
        ("wallrun_scuff.wav", wallrun_scuff),
        ("metallic_ting.wav", metallic_ting),
    ]
}

/// Generate every SFX into `<assets_dir>/sounds/` (idempotent, fast: a few ms).
/// Call this before the Bevy App starts so the AssetServer can load the files.
pub fn generate(assets_dir: &str) {
    let dir = Path::new(assets_dir).join("sounds");
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("audio_gen: could not create {dir:?}: {e}");
        return;
    }
    for (name, f) in table() {
        let samples = f();
        let path = dir.join(name);
        if let Err(e) = write_wav(&path, &samples) {
            eprintln!("audio_gen: failed writing {path:?}: {e}");
        }
    }
}
