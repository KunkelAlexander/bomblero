// A 16-bar combat loop: low C gallops and a four-bar sequence of sustained answers.
// 176 BPM. Replace the original music module with this file.
// Uses the existing synth.rs API. To audition outside the game:
// cargo test -p game dump -- --ignored

use crate::synth::{layer, render, soft_clip, LowPass, Samples, Wave, RATE};

const BPM: f32 = 176.0;
/// One sixteenth note, in seconds: the grid everything is placed on.
const STEP: f32 = 60.0 / BPM / 4.0;
const BARS: usize = 16;

const C2: u8 = 36;
const G2: u8 = 43;
const GS2: u8 = 44;
const B2: u8 = 47;
const C3: u8 = 48;
const D4: u8 = 62;
const DS4: u8 = 63;

/// One shared score keeps guitar and bass locked together.
#[derive(Clone, Copy)]
struct RiffHit {
    step: usize,
    midi: u8,
    gain: f32,
    answer: bool,
    duration: f32,
}

fn riff_hits() -> Vec<RiffHit> {
    let mut hits = Vec::new();
    for bar in 0..BARS {
        // Every fourth bar leaves beat 3 empty before the answer.
        let beats = if bar % 4 == 3 { 2 } else { 3 };
        for beat in 0..beats {
            for (offset, gain) in [(0, 1.0), (2, 0.68), (3, 0.78)] {
                hits.push(RiffHit {
                    step: bar * 16 + beat * 4 + offset,
                    midi: C2,
                    gain,
                    answer: false,
                    duration: STEP * 0.72,
                });
            }
        }
        let answer = match bar % 4 {
            0 => [G2, GS2],
            1 => [C3, B2],
            2 => [DS4, D4],
            _ => [GS2, G2],
        };
        for (offset, midi) in [12, 14].iter().copied().zip(answer) {
            hits.push(RiffHit {
                step: bar * 16 + offset,
                midi,
                gain: if offset == 14 { 1.1 } else { 0.9 },
                answer: true,
                duration: STEP * if bar % 4 == 3 && offset == 14 { 0.85 } else { 1.95 },
            });
        }
    }
    hits
}

const SNARE: [usize; 2] = [4, 12];

fn hz(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

/// Samples in the whole loop.
fn loop_len() -> usize {
    (BARS as f32 * 16.0 * STEP * RATE as f32) as usize
}

fn at(step: usize) -> usize {
    (step as f32 * STEP * RATE as f32) as usize
}

/// Add `sound` into `track` starting at sample `start`. Whatever runs past the
/// end wraps round to the beginning, so the cymbal ringing out of bar 16 is
/// still sounding when bar 1 comes back: the loop has no seam.
fn stamp(track: &mut [f32], start: usize, sound: &[f32], gain: f32) {
    let n = track.len();
    for (i, s) in sound.iter().enumerate() {
        track[(start + i) % n] += s * gain;
    }
}

/// Distortion: push the signal into a wall (tanh) so peaks flatten and extra
/// harmonics appear. That, plus a low-pass to tame the harshest ones, is most
/// of what "electric guitar" sounds like.
fn distort(track: &mut [f32], drive: f32, cutoff: f32) {
    let mut lp = LowPass::new();
    for s in track {
        *s = lp.run((*s * drive).tanh(), cutoff);
    }
}

/// A power chord (root, fifth, octave), each note as two slightly detuned saws
/// (they beat against each other, which is what makes a guitar sound "wide").
/// `tau` is how fast it dies away: tiny for a palm mute, long for an open strum.
fn power_chord(root: u8, tau: f32) -> Samples {
    let len = (tau * 5.0 * RATE as f32) as usize;
    let mut out = vec![0.0; len];
    for ratio in [1.0, 1.4983, 2.0] {
        for detune in [0.997, 1.003] {
            let step = hz(root as f32) * ratio * detune / RATE as f32;
            let mut phase = 0.0f32;
            for (i, o) in out.iter_mut().enumerate() {
                phase = (phase + step).fract();
                let attack = (i as f32 / (0.001 * RATE as f32)).min(1.0);
                *o += (phase * 2.0 - 1.0) * (-(i as f32) / (tau * RATE as f32)).exp() * attack * 0.2;
            }
        }
    }
    out
}

/// Trim the exponential tail with a short release to separate fast attacks.
fn gate(sound: &mut Samples, seconds: f32) {
    let len = ((seconds * RATE as f32) as usize).min(sound.len());
    sound.truncate(len);
    let release = (0.004 * RATE as f32).max(1.0);
    for (i, sample) in sound.iter_mut().enumerate() {
        *sample *= ((len - i) as f32 / release).min(1.0);
    }
}

fn guitar_track(n: usize) -> Samples {
    let mut track = vec![0.0; n];
    for hit in riff_hits() {
        // Root and octave for low chugs; full power chords for answers.
        let mut sound = if hit.answer {
            power_chord(hit.midi, 0.14)
        } else {
            render(&[
                layer(Wave::Saw, hz(C2 as f32), hz(C2 as f32), 0.065, 3.0, 0.6),
                layer(Wave::Saw, hz((C2 + 12) as f32), hz((C2 + 12) as f32), 0.065, 3.0, 0.25),
            ])
        };
        gate(&mut sound, hit.duration);
        let lift = if hit.step / 16 >= 8 { 1.12 } else { 1.0 };
        stamp(&mut track, at(hit.step), &sound, hit.gain * lift);
    }
    distort(&mut track, 5.0, 3500.0);
    track
}

fn bass_track(n: usize) -> Samples {
    let mut track = vec![0.0; n];
    for hit in riff_hits() {
        // Keep bass under the upper answers instead of leaping with them.
        let mut midi = if hit.answer { hit.midi - 12 } else { C2 - 12 };
        while midi >= C2 {
            midi -= 12;
        }
        let frequency = hz(midi as f32);
        let mut sound = render(&[
            layer(Wave::Saw, frequency, frequency, 0.10, 2.0, 0.3),
            layer(Wave::Sine, frequency, frequency, 0.10, 2.0, 0.55),
        ]);
        gate(&mut sound, hit.duration);
        stamp(&mut track, at(hit.step), &sound, hit.gain);
    }
    let mut lp = LowPass::new();
    for s in &mut track {
        *s = lp.run(*s, 900.0);
    }
    track
}

/// Noise with the low end removed: what a cymbal is. (Subtracting each sample
/// from the next throws away slow movement and keeps the fast.)
fn highpass(mut s: Samples) -> Samples {
    let mut prev = 0.0;
    for x in &mut s {
        let now = *x;
        *x = now - prev;
        prev = now;
    }
    s
}

fn drum_track(n: usize) -> Samples {
    let mut track = vec![0.0; n];
    let kick = render(&[layer(Wave::Sine, 160.0, 45.0, 0.18, 2.0, 0.9), layer(Wave::Noise, 3000.0, 800.0, 0.02, 1.0, 0.3)]);
    let snare = render(&[layer(Wave::Noise, 7000.0, 3000.0, 0.16, 2.0, 0.55), layer(Wave::Sine, 220.0, 160.0, 0.10, 2.0, 0.5)]);
    let hat = highpass(render(&[layer(Wave::Noise, 12000.0, 12000.0, 0.04, 3.0, 0.5)]));
    let crash = highpass(render(&[layer(Wave::Noise, 12000.0, 9000.0, 1.2, 2.0, 0.5)]));
    for bar in 0..BARS {
        let first = bar * 16;
        if bar % 4 == 0 {
            stamp(&mut track, at(first), &crash, 0.45);
        }
        // Reinforce selected gallops; leave room for the snare and answers.
        let kicks: &[usize] = if bar % 4 == 3 { &[0, 2, 6, 14] } else { &[0, 2, 6, 8, 10] };
        for &step in kicks {
            stamp(&mut track, at(first + step), &kick, 1.0);
        }
        // Four bars of half-time near the end give the riff a heavier turn.
        let snares: &[usize] = if (12..15).contains(&bar) { &[8] } else { &SNARE };
        for &step in snares {
            stamp(&mut track, at(first + step), &snare, 0.9);
        }
        for step in (0..16).step_by(2) {
            stamp(&mut track, at(first + step), &hat,
                if step % 4 == 0 { 0.42 } else { 0.24 });
        }
        if bar == 7 || bar == 15 {
            for step in [13, 15] {
                stamp(&mut track, at(first + step), &snare, 0.55);
            }
        }
    }
    track
}

fn lead_track(n: usize) -> Samples {
    let mut track = vec![0.0; n];
    // Only two brief high answers per loop; the rhythm remains the hook.
    for (step, midi) in [(7 * 16 + 12, GS2 + 24), (7 * 16 + 14, G2 + 24),
                         (15 * 16 + 12, GS2 + 24), (15 * 16 + 14, G2 + 24)] {
        let duration = if step % 16 == 14 { 0.85 } else { 1.95 };
        let len = (STEP * duration * RATE as f32) as usize;
        let base = hz(midi as f32);
        let mut phase = 0.0f32;
        let mut out = vec![0.0; len];
        for (i, o) in out.iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            // Vibrato that fades in after a moment, like a singer's.
            let vibrato = 1.0 + 0.004 * (t - 0.12).max(0.0).min(0.1) * 10.0 * (std::f32::consts::TAU * 5.5 * t).sin();
            phase = (phase + base * vibrato / RATE as f32).fract();
            // A narrow pulse sounds reedier than a square: it cuts through the guitars.
            let x = if phase < 0.3 { 1.0 } else { -1.0 };
            let attack = (i as f32 / (0.005 * RATE as f32)).min(1.0);
            let release = ((len - i) as f32 / (0.04 * RATE as f32)).min(1.0);
            *o = x * attack * release * 0.22;
        }
        stamp(&mut track, at(step), &out, 1.0);
    }
    let mut lp = LowPass::new();
    for s in &mut track {
        *s = lp.run(*s, 4500.0);
    }
    track
}

/// The whole loop, as samples at `RATE`.
pub fn tune() -> Samples {
    let n = loop_len();
    let (guitar, bass, drums, lead) = (guitar_track(n), bass_track(n), drum_track(n), lead_track(n));
    let mut mix: Samples = (0..n).map(|i| guitar[i] * 0.55 + bass[i] * 0.5 + drums[i] * 0.8 + lead[i] * 0.5).collect();
    // Scale so the loudest moment sits just under full volume.
    let peak = mix.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    for s in &mut mix {
        *s *= 0.9 / peak;
    }
    soft_clip(&mut mix);
    mix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riff_has_gallops_and_four_distinct_answers() {
        let hits = riff_hits();
        let steps: Vec<_> = hits.iter().take(3).map(|h| h.step).collect();
        assert_eq!(steps, [0, 2, 3]);
        for bar in 0..BARS {
            let answer: Vec<_> = hits.iter()
                .filter(|h| h.step / 16 == bar && h.answer)
                .map(|h| (h.step % 16, h.midi)).collect();
            let expected = [[G2, GS2], [C3, B2], [DS4, D4], [GS2, G2]][bar % 4];
            assert_eq!(answer, [(12, expected[0]), (14, expected[1])]);
        }
        assert!(hits.iter().all(|h| h.step < BARS * 16));
        assert!(!hits.iter().any(|h| h.step / 16 % 4 == 3 && (8..12).contains(&(h.step % 16))));
    }

    #[test]
    fn the_loop_is_sixteen_bars_and_well_behaved() {
        let tune = tune();
        let seconds = tune.len() as f32 / RATE as f32;
        assert!((seconds - 16.0 * 4.0 * 60.0 / BPM).abs() < 0.01, "{seconds} s");
        assert!(tune.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        let rms = (tune.iter().map(|x| x * x).sum::<f32>() / tune.len() as f32).sqrt();
        assert!((0.05..0.5).contains(&rms), "rms {rms}: too quiet or too loud");
    }

    /// Writes every sound to target/audio/ so they can be played in any media
    /// player. Ignored by default (it writes files): run it on purpose.
    #[test]
    #[ignore]
    fn dump() {
        use crate::synth::{wav, SOUNDS};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/audio");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("music.wav"), wav(&tune())).unwrap();
        for s in &SOUNDS {
            std::fs::write(dir.join(format!("{}.wav", s.name)), wav(&render(s.layers))).unwrap();
        }
    }
}
