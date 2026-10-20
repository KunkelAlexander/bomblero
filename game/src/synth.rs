// Sound made from nothing: oscillators and noise, summed into a list of samples.
//
// No file is read and no audio device is touched here. The output is plain
// numbers (`Samples`), which `wav()` wraps in a WAV header so that macroquad's
// loader (which only understands files) can play them. Keeping this module free
// of macroquad means the tests can check the sounds without a sound card.
//
// Pattern: Type Object. Like a weapon, a sound effect is a row of data (a list
// of `Layer`s in `SOUNDS`), not a function. One `render` turns any row into
// samples; a new sound means a new row.

use std::f32::consts::TAU;

/// Samples per second. Low on purpose: it keeps the sounds small and gritty,
/// and the desktop mixer upsamples to 44100 Hz by exactly doubling each sample.
pub const RATE: u32 = 22050;

pub type Samples = Vec<f32>;

#[derive(Clone, Copy)]
pub enum Wave {
    Sine,
    Square,
    Saw,
    /// White noise through a low-pass filter. For noise, `from`/`to` are the
    /// filter's cutoff in Hz: high is a hiss, low is a rumble.
    Noise,
}

/// One ingredient of a sound.
#[derive(Clone, Copy)]
pub struct Layer {
    pub wave: Wave,
    /// Pitch (or noise cutoff) in Hz at the start and at the end. It glides
    /// between them on a curve that sounds even to the ear (exponential).
    pub from: f32,
    pub to: f32,
    /// Seconds. The layer is silent after this.
    pub len: f32,
    /// How fast it fades: volume is (1 - t)^decay with t from 0 to 1. 1 is a
    /// straight fade out, 3 is a sharp "tick" with a short tail.
    pub decay: f32,
    pub gain: f32,
    /// Seconds before the layer starts, to stack notes into a little melody.
    pub delay: f32,
    /// Pitch wobble: (speed in Hz, depth as a fraction of the pitch).
    pub wobble: (f32, f32),
}

pub const fn layer(wave: Wave, from: f32, to: f32, len: f32, decay: f32, gain: f32) -> Layer {
    Layer { wave, from, to, len, decay, gain, delay: 0.0, wobble: (0.0, 0.0) }
}

impl Layer {
    const fn after(self, delay: f32) -> Layer {
        Layer { delay, ..self }
    }
    const fn wobble(self, hz: f32, depth: f32) -> Layer {
        Layer { wobble: (hz, depth), ..self }
    }
}

use Wave::{Noise, Saw, Sine, Square};

/// Every sound effect the game makes. The order here is the order of `SOUNDS`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sfx {
    Blaster,
    Rocket,
    Railgun,
    Build,
    Bouncer,
    Bomb,
    Cluster,
    Swarm,
    Teleport,
    Nova,
    Sheep,
    Confuser,
    Mirror,
    Oil,
    Laser,
    BlackHole,
    Flash,
    BoomSmall,
    BoomMedium,
    BoomBig,
    /// Somebody was hit.
    Hit,
    /// *You* were hit: lower and harsher, so it cuts through the fight.
    Hurt,
    Death,
    Respawn,
    BrickBreak,
    Dash,
    Reflect,
    Victory,
}

pub const SFX_COUNT: usize = 28;

// `id` and `name` are read by the tests (order check, file names for `dump`).
#[allow(dead_code)]
pub struct Sound {
    pub id: Sfx,
    pub name: &'static str,
    pub layers: &'static [Layer],
}

pub static SOUNDS: [Sound; SFX_COUNT] = [
    Sound {
        id: Sfx::Blaster,
        name: "blaster",
        layers: &[layer(Square, 1500.0, 320.0, 0.09, 2.0, 0.30), layer(Noise, 6000.0, 2000.0, 0.03, 1.0, 0.25)],
    },
    Sound {
        id: Sfx::Rocket,
        name: "rocket",
        layers: &[layer(Noise, 1600.0, 300.0, 0.35, 1.5, 0.55), layer(Saw, 140.0, 70.0, 0.30, 1.5, 0.30)],
    },
    Sound {
        id: Sfx::Railgun,
        name: "railgun",
        layers: &[
            layer(Saw, 2400.0, 80.0, 0.35, 3.0, 0.40),
            layer(Noise, 9000.0, 1000.0, 0.08, 3.0, 0.50),
            layer(Sine, 70.0, 40.0, 0.25, 1.5, 0.50),
        ],
    },
    Sound {
        id: Sfx::Build,
        name: "build",
        layers: &[layer(Square, 220.0, 110.0, 0.06, 1.5, 0.30), layer(Noise, 800.0, 300.0, 0.09, 2.0, 0.60)],
    },
    Sound { id: Sfx::Bouncer, name: "bouncer", layers: &[layer(Sine, 300.0, 900.0, 0.14, 1.5, 0.50).wobble(30.0, 0.05)] },
    Sound {
        id: Sfx::Bomb,
        name: "bomb",
        layers: &[layer(Sine, 520.0, 520.0, 0.06, 1.0, 0.40), layer(Sine, 520.0, 520.0, 0.06, 1.0, 0.40).after(0.09)],
    },
    Sound {
        id: Sfx::Cluster,
        name: "cluster",
        layers: &[layer(Saw, 200.0, 60.0, 0.20, 1.5, 0.35), layer(Noise, 2000.0, 300.0, 0.15, 1.5, 0.45)],
    },
    Sound {
        id: Sfx::Swarm,
        name: "swarm",
        layers: &[layer(Noise, 4000.0, 1200.0, 0.30, 1.0, 0.30), layer(Saw, 300.0, 600.0, 0.25, 1.0, 0.25).wobble(40.0, 0.3)],
    },
    Sound {
        id: Sfx::Teleport,
        name: "teleport",
        layers: &[layer(Sine, 300.0, 1800.0, 0.30, 1.2, 0.45).wobble(20.0, 0.10), layer(Square, 600.0, 2400.0, 0.25, 1.5, 0.12)],
    },
    Sound {
        id: Sfx::Nova,
        name: "nova",
        layers: &[
            layer(Saw, 80.0, 40.0, 0.60, 1.2, 0.50),
            layer(Sine, 200.0, 1200.0, 0.40, 1.5, 0.35),
            layer(Noise, 3000.0, 400.0, 0.50, 1.5, 0.40),
        ],
    },
    Sound { id: Sfx::Sheep, name: "sheep", layers: &[layer(Saw, 400.0, 300.0, 0.40, 1.0, 0.30).wobble(14.0, 0.25)] },
    Sound { id: Sfx::Confuser, name: "confuser", layers: &[layer(Sine, 700.0, 200.0, 0.40, 1.0, 0.45).wobble(9.0, 0.40)] },
    Sound {
        id: Sfx::Mirror,
        name: "mirror",
        layers: &[layer(Sine, 2000.0, 2000.0, 0.25, 3.0, 0.40), layer(Sine, 3010.0, 3010.0, 0.25, 3.0, 0.20)],
    },
    Sound {
        id: Sfx::Oil,
        name: "oil",
        layers: &[layer(Sine, 150.0, 400.0, 0.12, 1.2, 0.50), layer(Sine, 200.0, 500.0, 0.10, 1.2, 0.45).after(0.10)],
    },
    // The laser fires 20 times a second: short, quiet, and high, so the pulses blur into one buzz.
    Sound { id: Sfx::Laser, name: "laser", layers: &[layer(Square, 2200.0, 1200.0, 0.05, 1.0, 0.12)] },
    Sound {
        id: Sfx::BlackHole,
        name: "blackhole",
        layers: &[layer(Sine, 400.0, 40.0, 0.80, 0.8, 0.50).wobble(6.0, 0.10), layer(Noise, 500.0, 100.0, 0.80, 1.0, 0.30)],
    },
    Sound {
        id: Sfx::Flash,
        name: "flash",
        layers: &[layer(Noise, 9000.0, 9000.0, 0.10, 3.0, 0.50), layer(Sine, 3500.0, 3500.0, 0.40, 2.0, 0.25)],
    },
    Sound {
        id: Sfx::BoomSmall,
        name: "boom-small",
        layers: &[layer(Noise, 3000.0, 200.0, 0.35, 2.0, 0.60), layer(Sine, 120.0, 40.0, 0.30, 1.5, 0.50)],
    },
    Sound {
        id: Sfx::BoomMedium,
        name: "boom-medium",
        layers: &[layer(Noise, 2500.0, 120.0, 0.60, 1.8, 0.70), layer(Sine, 100.0, 30.0, 0.50, 1.5, 0.60)],
    },
    Sound {
        id: Sfx::BoomBig,
        name: "boom-big",
        layers: &[
            layer(Noise, 2000.0, 80.0, 1.00, 1.5, 0.80),
            layer(Sine, 80.0, 25.0, 0.90, 1.2, 0.70),
            layer(Saw, 60.0, 30.0, 0.60, 1.5, 0.30),
        ],
    },
    Sound {
        id: Sfx::Hit,
        name: "hit",
        layers: &[layer(Square, 600.0, 200.0, 0.08, 2.0, 0.28), layer(Noise, 4000.0, 1500.0, 0.05, 2.0, 0.25)],
    },
    Sound { id: Sfx::Hurt, name: "hurt", layers: &[layer(Saw, 300.0, 100.0, 0.18, 1.5, 0.40), layer(Noise, 2500.0, 500.0, 0.10, 2.0, 0.30)] },
    Sound {
        id: Sfx::Death,
        name: "death",
        layers: &[layer(Noise, 3000.0, 200.0, 0.50, 1.5, 0.55), layer(Saw, 400.0, 40.0, 0.50, 1.2, 0.40)],
    },
    Sound { id: Sfx::Respawn, name: "respawn", layers: &[layer(Square, 300.0, 1200.0, 0.20, 1.2, 0.25).wobble(25.0, 0.04)] },
    Sound {
        id: Sfx::BrickBreak,
        name: "brick-break",
        layers: &[layer(Noise, 2500.0, 500.0, 0.25, 3.0, 0.55), layer(Sine, 90.0, 50.0, 0.12, 1.5, 0.40)],
    },
    Sound { id: Sfx::Dash, name: "dash", layers: &[layer(Noise, 800.0, 3000.0, 0.20, 1.5, 0.40)] },
    Sound {
        id: Sfx::Reflect,
        name: "reflect",
        layers: &[layer(Square, 1800.0, 3000.0, 0.08, 1.5, 0.25), layer(Sine, 2600.0, 2600.0, 0.20, 3.0, 0.30)],
    },
    // A quick E major arpeggio (E4 G#4 B4 E5): the "you won" sting.
    Sound {
        id: Sfx::Victory,
        name: "victory",
        layers: &[
            layer(Square, 329.6, 329.6, 0.14, 1.0, 0.25),
            layer(Square, 415.3, 415.3, 0.14, 1.0, 0.25).after(0.12),
            layer(Square, 493.9, 493.9, 0.14, 1.0, 0.25).after(0.24),
            layer(Square, 659.3, 659.3, 0.60, 1.2, 0.30).after(0.36),
            layer(Saw, 164.8, 164.8, 0.60, 1.2, 0.20).after(0.36),
        ],
    },
];

/// Deterministic white noise (xorshift). Not the `rand` crate: a sound must
/// come out the same every time, which also lets the tests be exact.
pub struct NoiseSource(u32);

impl NoiseSource {
    pub fn new(seed: u32) -> NoiseSource {
        NoiseSource(seed.max(1))
    }

    /// A number between -1 and 1.
    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// One-pole low-pass filter: each output moves a fraction of the way to the input.
pub struct LowPass {
    y: f32,
}

impl LowPass {
    pub fn new() -> LowPass {
        LowPass { y: 0.0 }
    }

    pub fn run(&mut self, x: f32, cutoff_hz: f32) -> f32 {
        let a = 1.0 - (-TAU * cutoff_hz / RATE as f32).exp();
        self.y += a * (x - self.y);
        self.y
    }
}

/// A waveform at `phase` (0 to 1).
pub fn wave(w: Wave, phase: f32) -> f32 {
    match w {
        Wave::Sine => (phase * TAU).sin(),
        Wave::Square => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Saw => phase * 2.0 - 1.0,
        Wave::Noise => 0.0, // made from `NoiseSource`, not from a phase
    }
}

pub fn render(layers: &[Layer]) -> Samples {
    let end = layers.iter().map(|l| l.delay + l.len).fold(0.0, f32::max);
    let mut out = vec![0.0; (end * RATE as f32) as usize + 1];
    // The seed is fixed, so a given sound always has the same "crackle".
    let mut noise = NoiseSource::new(0x9E37_79B9);
    for l in layers {
        let (start, n) = ((l.delay * RATE as f32) as usize, (l.len * RATE as f32) as usize);
        let mut phase = 0.0;
        let mut filter = LowPass::new();
        for i in 0..n {
            let t = i as f32 / n as f32;
            let mut freq = l.from * (l.to / l.from).powf(t);
            freq *= 1.0 + l.wobble.1 * (TAU * l.wobble.0 * i as f32 / RATE as f32).sin();
            let x = match l.wave {
                Wave::Noise => filter.run(noise.next(), freq) * 2.5,
                w => {
                    phase = (phase + freq / RATE as f32).fract();
                    wave(w, phase)
                }
            };
            // A 2 ms fade-in: starting a wave at full volume clicks.
            let attack = (i as f32 / (0.002 * RATE as f32)).min(1.0);
            out[start + i] += x * l.gain * (1.0 - t).powf(l.decay) * attack;
        }
    }
    soft_clip(&mut out);
    out
}

/// Squash anything past full scale instead of letting it wrap around (which
/// sounds like a crackle) when it becomes 16-bit.
pub fn soft_clip(samples: &mut [f32]) {
    for s in samples {
        *s = s.tanh();
    }
}

/// Wrap samples in the smallest WAV file there is: a 44-byte header, then
/// 16-bit mono PCM. macroquad's loader sniffs the format from these bytes.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); // size of the fmt block
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes per second
    b.extend_from_slice(&2u16.to_le_bytes()); // bytes per frame
    b.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(s: &[f32]) -> f32 {
        s.iter().fold(0.0, |m, x| f32::max(m, x.abs()))
    }

    #[test]
    fn table_order_matches_the_enum() {
        // SOUNDS is indexed by `Sfx as usize`; a swapped row would play the wrong sound.
        for (i, s) in SOUNDS.iter().enumerate() {
            assert_eq!(s.id as usize, i, "{} is in the wrong row", s.name);
        }
        assert_eq!(Sfx::Victory as usize, SFX_COUNT - 1);
    }

    #[test]
    fn every_sound_is_audible_and_not_clipped() {
        for s in &SOUNDS {
            let samples = render(s.layers);
            assert!(samples.iter().all(|x| x.is_finite()), "{}", s.name);
            let p = peak(&samples);
            assert!(p > 0.05, "{} is silent (peak {p})", s.name);
            assert!(p < 1.0, "{} clips (peak {p})", s.name);
            assert!(samples.len() < RATE as usize * 2, "{} is over two seconds", s.name);
        }
    }

    #[test]
    fn rendering_is_deterministic() {
        let a = render(SOUNDS[Sfx::BoomBig as usize].layers);
        let b = render(SOUNDS[Sfx::BoomBig as usize].layers);
        assert_eq!(a, b);
    }

    #[test]
    fn wav_header_is_well_formed() {
        let bytes = wav(&[0.0, 0.5, -0.5, 1.0]);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(bytes.len(), 44 + 8);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize, bytes.len() - 8);
        assert_eq!(i16::from_le_bytes(bytes[46..48].try_into().unwrap()), 16383);
    }
}
