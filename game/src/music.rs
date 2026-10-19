// The tune: a 16-bar punk rock loop, played by code instead of a band.
//
// It is a score (chords, a lead melody, a drum pattern) turned into samples with
// the oscillators from synth.rs. Like a MIDI file it stores *which notes*, not
// sound, and the instruments below decide how they sound. The whole loop is
// rendered once at startup (a fraction of a second) and then simply looped.
//
//   bars 1-8   verse:  palm-muted eighth-note chugging on power chords
//   bars 9-16  chorus: open chords, and the lead hook on top
//
// 176 BPM, key of E. Two things to change if you want a different tune: the
// CHORDS and LEAD tables below. Run `cargo test -p game dump -- --ignored` to
// write the loop to target/audio/ and listen to it without starting the game.

use crate::synth::{layer, render, soft_clip, LowPass, Samples, Wave, RATE};

const BPM: f32 = 176.0;
/// One sixteenth note, in seconds: the grid everything is placed on.
const STEP: f32 = 60.0 / BPM / 4.0;
const BARS: usize = 16;

// MIDI note numbers of the chord roots (E2 = 40; every +1 is a semitone).
const E2: u8 = 40;
const A2: u8 = 45;
const B2: u8 = 47;
const C3: u8 = 48;
const D3: u8 = 50;

/// The root of the power chord (root + fifth + octave) in each bar.
const CHORDS: [u8; BARS] = [
    E2, E2, C3, D3, E2, E2, C3, B2, // verse
    A2, B2, E2, E2, A2, B2, E2, B2, // chorus: IV - V - I, the most punk move there is
];

/// The lead melody, eight tokens per bar (eighth notes): a note name, `.` for a
/// rest, `-` to keep holding the previous note. Silent in the verse so the
/// chorus hits harder when the hook arrives.
const LEAD: [&str; BARS] = [
    "", "", "", "", "", "", "", "", //
    "E5 - E5 E5 C#5 - B4 -",
    "B4 - B4 B4 C#5 - D#5 -",
    "E5 - - - B4 - G#4 -",
    "G#4 - B4 - E5 - - .",
    "E5 - E5 E5 C#5 - B4 -",
    "B4 - B4 B4 C#5 - D#5 -",
    "E5 - F#5 - G#5 - B5 -", // the climb
    "B5 - - - - - . .",
];

/// Where the drums hit within a bar of 16 steps.
const KICK: [usize; 3] = [0, 8, 10];
const SNARE: [usize; 2] = [4, 12];
/// The last bar of each phrase swaps the beat for a snare roll.
const FILL_SNARE: [usize; 6] = [8, 10, 12, 13, 14, 15];

fn hz(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

struct Note {
    /// Start, in 16th-note steps from the beginning of the loop.
    step: usize,
    midi: u8,
    /// Length in steps.
    len: usize,
}

/// "C#5" -> 73 (a MIDI note number).
fn note_number(name: &str) -> u8 {
    let mut chars = name.chars();
    let letter = chars.next().expect("empty note name");
    let mut semitone = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => panic!("bad note name {name}"),
    };
    let mut rest = chars.as_str();
    if let Some(r) = rest.strip_prefix('#') {
        semitone += 1;
        rest = r;
    }
    let octave: u8 = rest.parse().expect("bad octave");
    (octave + 1) * 12 + semitone
}

fn lead_notes() -> Vec<Note> {
    let mut notes: Vec<Note> = Vec::new();
    for (bar, tokens) in LEAD.iter().enumerate() {
        for (i, token) in tokens.split_whitespace().enumerate() {
            let step = bar * 16 + i * 2;
            match token {
                "." => {}
                "-" => notes.last_mut().expect("a hold needs a note before it").len += 2,
                name => notes.push(Note { step, midi: note_number(name), len: 2 }),
            }
        }
    }
    notes
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

fn guitar_track(n: usize) -> Samples {
    let mut track = vec![0.0; n];
    // Pattern: Flyweight, in spirit: each bar renders its three kinds of
    // strum once and stamps them on every eighth note.
    for (bar, &root) in CHORDS.iter().enumerate() {
        let (muted, accent, open) = (power_chord(root, 0.045), power_chord(root, 0.20), power_chord(root, 0.12));
        for eighth in 0..8 {
            let step = bar * 16 + eighth * 2;
            let strum = if bar >= 8 {
                &open
            } else if eighth == 0 {
                &accent
            } else {
                &muted
            };
            stamp(&mut track, at(step), strum, 1.0);
        }
    }
    distort(&mut track, 5.0, 3500.0);
    track
}

fn bass_track(n: usize) -> Samples {
    let mut track = vec![0.0; n];
    for (bar, &root) in CHORDS.iter().enumerate() {
        // A saw for bite, a sine for weight, both on the chord's root.
        let note = render(&[layer(Wave::Saw, hz(root as f32), hz(root as f32), 0.16, 1.5, 0.35), layer(Wave::Sine, hz(root as f32), hz(root as f32), 0.16, 1.5, 0.45)]);
        for eighth in 0..8 {
            stamp(&mut track, at(bar * 16 + eighth * 2), &note, 1.0);
        }
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
        let fill = bar % 8 == 7;
        let first = bar * 16;
        if bar % 8 == 0 {
            stamp(&mut track, at(first), &crash, 0.6);
        }
        if fill {
            // Same kick on 1 and 3, then a roll up to the next crash.
            for &s in &KICK[..2] {
                stamp(&mut track, at(first + s), &kick, 1.0);
            }
            for &s in &FILL_SNARE {
                stamp(&mut track, at(first + s), &snare, 0.8);
            }
            for s in [0, 2, 4, 6] {
                stamp(&mut track, at(first + s), &hat, 0.5);
            }
        } else {
            for &s in &KICK {
                stamp(&mut track, at(first + s), &kick, 1.0);
            }
            for &s in &SNARE {
                stamp(&mut track, at(first + s), &snare, 0.9);
            }
            for s in (0..16).step_by(2) {
                // Louder on the beat than between.
                stamp(&mut track, at(first + s), &hat, if s % 4 == 0 { 0.5 } else { 0.3 });
            }
        }
    }
    track
}

fn lead_track(n: usize) -> Samples {
    let mut track = vec![0.0; n];
    for note in lead_notes() {
        let len = (note.len as f32 * STEP * RATE as f32) as usize;
        let base = hz(note.midi as f32);
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
        stamp(&mut track, at(note.step), &out, 1.0);
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
    fn note_names_become_midi_numbers() {
        assert_eq!(note_number("A4"), 69);
        assert_eq!(note_number("E4"), 64);
        assert_eq!(note_number("C#5"), 73);
        assert_eq!(note_number("B5"), 83);
    }

    #[test]
    fn the_hook_has_the_notes_it_should() {
        let notes = lead_notes();
        // Nothing before the chorus.
        assert!(notes.iter().all(|n| n.step >= 8 * 16));
        // "E5 - E5 E5 ..." : a held note is one note, twice as long.
        assert_eq!((notes[0].midi, notes[0].len), (76, 4));
        assert_eq!(notes[1].midi, 76);
        // No note runs past the end of the loop.
        assert!(notes.iter().all(|n| n.step + n.len <= BARS * 16));
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
