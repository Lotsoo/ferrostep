//! Menu and countdown sound effects, synthesized in code (no audio assets).
//!
//! Arcade-style but polished: FM bells for sparkle, soft square waves for the
//! classic "coin" feel, a sub "thump" for weight, filtered noise for swishes,
//! stereo panning, and a small Schroeder reverb so sounds feel like they are
//! in a room instead of inside the speaker. `render` is a pure function.

use std::f32::consts::TAU;

/// All sounds the app makes besides music.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sfx {
    /// Pointer moved onto something clickable (very quiet).
    Hover,
    /// Selection moved up.
    MoveUp,
    /// Selection moved down.
    MoveDown,
    /// Value changed to the left (panned left).
    ChangeLeft,
    /// Value changed to the right (panned right).
    ChangeRight,
    /// Something was chosen.
    Confirm,
    /// Going back.
    Back,
    /// A song is about to start.
    Start,
    /// Something finished well (import done).
    Success,
    /// Countdown tick (3, 2, 1).
    Count,
    /// Countdown end: go!
    Go,
}

impl Sfx {
    pub const ALL: [Sfx; 11] = [
        Sfx::Hover,
        Sfx::MoveUp,
        Sfx::MoveDown,
        Sfx::ChangeLeft,
        Sfx::ChangeRight,
        Sfx::Confirm,
        Sfx::Back,
        Sfx::Start,
        Sfx::Success,
        Sfx::Count,
        Sfx::Go,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// Loudest sample after mixing; quiet sounds stay quiet, big ones are big.
    fn peak(self) -> f32 {
        match self {
            Sfx::Hover => 0.4,
            Sfx::MoveUp | Sfx::MoveDown | Sfx::ChangeLeft | Sfx::ChangeRight => 0.85,
            _ => 0.98,
        }
    }

    /// Saturation drive: higher = denser and louder (arcade cabinets are loud).
    fn drive(self) -> f32 {
        match self {
            Sfx::Hover => 1.5,
            _ => 5.0,
        }
    }

    /// Reverb amount (0 = dry).
    fn wet(self) -> f32 {
        match self {
            Sfx::Hover => 0.05,
            Sfx::MoveUp | Sfx::MoveDown | Sfx::ChangeLeft | Sfx::ChangeRight => 0.18,
            Sfx::Back | Sfx::Confirm | Sfx::Count => 0.25,
            Sfx::Start | Sfx::Success | Sfx::Go => 0.35,
        }
    }
}

/// Oscillator shapes.
#[derive(Debug, Clone, Copy)]
enum Wave {
    Sine,
    /// Square with few harmonics: arcade flavor without harshness.
    Square,
    /// Bright saw-like tone for chord stabs.
    Saw,
    /// FM bell: modulator at `ratio` × frequency, brightness fades with the note.
    Bell {
        ratio: f32,
        index: f32,
    },
    /// Noise through a low-pass whose cutoff glides with the frequency.
    Noise,
}

/// One sound event.
#[derive(Debug, Clone, Copy)]
struct Voice {
    start: f32,
    length: f32,
    f0: f32,
    f1: f32,
    amp: f32,
    /// Exponential decay rate (1/s); 0 = sustained until the end.
    decay: f32,
    wave: Wave,
    /// −1 = left, 0 = center, 1 = right.
    pan: f32,
}

fn v(start: f32, length: f32, f0: f32, f1: f32, amp: f32, wave: Wave) -> Voice {
    Voice {
        start,
        length,
        f0,
        f1,
        amp,
        decay: 4.0 / length,
        wave,
        pan: 0.0,
    }
}

const BELL: Wave = Wave::Bell {
    ratio: 2.0,
    index: 2.2,
};
const GLASS: Wave = Wave::Bell {
    ratio: 3.5,
    index: 1.4,
};

/// Low "thump" that gives a press physical weight.
fn thump(start: f32, amp: f32) -> Voice {
    v(start, 0.12, 150.0, 48.0, amp, Wave::Sine)
}

/// Square note that holds its level (chiptune style) instead of fading fast.
fn sq(start: f32, length: f32, f0: f32, f1: f32, amp: f32) -> Voice {
    Voice {
        decay: 1.5 / length,
        ..v(start, length, f0, f1, amp, Wave::Square)
    }
}

/// Bright saw layer an octave below, for body.
fn body(start: f32, length: f32, f: f32, amp: f32) -> Voice {
    Voice {
        decay: 2.0 / length,
        ..v(start, length, f * 0.5, f * 0.5, amp, Wave::Saw)
    }
}

fn voices(sfx: Sfx) -> Vec<Voice> {
    match sfx {
        Sfx::Hover => vec![v(0.0, 0.035, 2400.0, 2600.0, 1.0, Wave::Square)],
        // Moves and changes use the same instrument as Confirm/Back (square
        // + saw body + thump), but as ONE short note, so scrolling sounds like
        // a soft "tik" and only Enter gets the full two-note coin.
        Sfx::MoveUp | Sfx::MoveDown => {
            let f = if sfx == Sfx::MoveUp { 1319.0 } else { 1175.0 }; // E6 / D6
            vec![
                Voice {
                    decay: 22.0,
                    ..sq(0.0, 0.07, f, f, 0.8)
                },
                Voice {
                    decay: 22.0,
                    ..body(0.0, 0.07, f, 0.5)
                },
                thump(0.0, 0.6),
            ]
        }
        Sfx::ChangeLeft | Sfx::ChangeRight => {
            // Same "tik", lower (B5 / A5) and leaning toward the side pressed.
            let left = sfx == Sfx::ChangeLeft;
            let pan = if left { -0.35 } else { 0.35 };
            let f = if left { 880.0 } else { 988.0 };
            vec![
                Voice {
                    pan,
                    decay: 18.0,
                    ..sq(0.0, 0.09, f, f, 0.8)
                },
                Voice {
                    pan,
                    decay: 18.0,
                    ..body(0.0, 0.09, f, 0.5)
                },
                thump(0.0, 0.7),
            ]
        }
        Sfx::Confirm => vec![
            // The arcade coin: B5 then a ringing E6, big and bright.
            sq(0.0, 0.06, 988.0, 988.0, 0.8),
            body(0.0, 0.06, 988.0, 0.5),
            sq(0.06, 0.4, 1319.0, 1319.0, 0.8),
            body(0.06, 0.4, 1319.0, 0.5),
            v(0.06, 0.5, 2637.0, 2637.0, 0.4, GLASS),
            v(0.0, 0.04, 4000.0, 9000.0, 0.4, Wave::Noise),
            thump(0.0, 1.0),
        ],
        Sfx::Back => vec![
            sq(0.0, 0.07, 784.0, 784.0, 0.8),
            body(0.0, 0.07, 784.0, 0.5),
            sq(0.07, 0.16, 523.0, 392.0, 0.8),
            body(0.07, 0.16, 523.0, 0.5),
            thump(0.0, 0.8),
        ],
        Sfx::Start => {
            let mut out = vec![
                // Rising whoosh into a big chord with a boom.
                Voice {
                    decay: 0.0,
                    ..v(0.0, 0.45, 300.0, 9000.0, 0.6, Wave::Noise)
                },
                sq(0.0, 0.45, 220.0, 880.0, 0.4),
                thump(0.42, 1.0),
                v(0.42, 0.35, 110.0, 40.0, 0.9, Wave::Sine),
            ];
            for (i, f) in [523.25, 659.25, 783.99, 1046.5].into_iter().enumerate() {
                out.push(Voice {
                    pan: [-0.6, 0.6, -0.3, 0.3][i],
                    decay: 2.5,
                    ..v(0.42, 0.9, f * 0.995, f * 1.005, 0.4, Wave::Saw)
                });
            }
            for (i, f) in [2093.0, 2637.0, 3136.0, 4186.0].into_iter().enumerate() {
                out.push(Voice {
                    pan: if i % 2 == 0 { -0.5 } else { 0.5 },
                    ..v(0.45 + i as f32 * 0.05, 0.35, f, f, 0.3, GLASS)
                });
            }
            out
        }
        Sfx::Success => [1047.0, 1319.0, 1568.0, 2093.0, 2637.0]
            .into_iter()
            .enumerate()
            .flat_map(|(i, f)| {
                let t = i as f32 * 0.07;
                let pan = (i as f32 - 2.0) * 0.3;
                [
                    Voice {
                        pan,
                        ..sq(t, 0.12, f, f, 0.6)
                    },
                    Voice {
                        pan,
                        ..v(t, 0.6, f, f, 0.5, BELL)
                    },
                ]
            })
            .collect(),
        Sfx::Count => vec![
            // Stadium-style beep: thick square, octave saw, big low hit.
            sq(0.0, 0.26, 880.0, 880.0, 0.9),
            body(0.0, 0.26, 880.0, 0.6),
            v(0.0, 0.3, 1760.0, 1760.0, 0.35, GLASS),
            v(0.0, 0.03, 3000.0, 8000.0, 0.5, Wave::Noise),
            thump(0.0, 1.0),
            v(0.0, 0.2, 90.0, 45.0, 0.8, Wave::Sine),
        ],
        Sfx::Go => {
            let mut out = vec![
                thump(0.0, 1.0),
                v(0.0, 0.45, 110.0, 38.0, 1.0, Wave::Sine),
                Voice {
                    decay: 3.0,
                    ..v(0.0, 0.5, 1500.0, 12000.0, 0.45, Wave::Noise)
                },
            ];
            // Octave-up "beep" first (like the last countdown beep, but higher)…
            out.push(sq(0.0, 0.5, 1760.0, 1760.0, 0.7));
            // …over a wide A-major chord.
            for (i, f) in [440.0, 554.4, 659.3, 880.0, 1108.7].into_iter().enumerate() {
                out.push(Voice {
                    pan: [-0.6, 0.6, -0.3, 0.3, 0.0][i],
                    decay: 2.0,
                    ..v(0.0, 1.0, f * 0.995, f * 1.005, 0.4, Wave::Saw)
                });
            }
            out.push(v(0.03, 0.7, 3520.0, 3520.0, 0.35, GLASS));
            out
        }
    }
}

/// Small deterministic noise source.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Renders one voice into a mono buffer.
fn render_voice(voice: &Voice, sr: f32, seed: u32) -> Vec<f32> {
    let count = (voice.length * sr) as usize;
    let mut out = Vec::with_capacity(count);
    let mut phase = 0.0f32;
    let mut noise = Noise(seed.max(1));
    let mut lp = 0.0f32;
    let attack = 0.0015;
    let release = 0.01_f32.min(voice.length * 0.3);
    for i in 0..count {
        let t = i as f32 / sr;
        let progress = t / voice.length;
        // Exponential glide sounds natural for pitch.
        let freq = voice.f0 * (voice.f1 / voice.f0).powf(progress);
        phase += TAU * freq / sr;
        let env = (t / attack).min(1.0)
            * ((voice.length - t) / release).clamp(0.0, 1.0)
            * (-voice.decay * t).exp();
        let s = match voice.wave {
            Wave::Sine => phase.sin(),
            Wave::Square => {
                (phase.sin() + (3.0 * phase).sin() * 0.3 + (5.0 * phase).sin() * 0.12) / 1.25
            }
            Wave::Saw => {
                (1..=6)
                    .map(|k| (k as f32 * phase).sin() / k as f32)
                    .sum::<f32>()
                    / 1.6
            }
            Wave::Bell { ratio, index } => {
                let bright = index * (-6.0 * progress).exp();
                (phase + bright * (ratio * phase).sin()).sin()
            }
            Wave::Noise => {
                // One-pole low-pass with the "frequency" as cutoff.
                let a = 1.0 - (-TAU * freq / sr).exp();
                lp += a * (noise.next() - lp);
                lp * 1.8
            }
        };
        out.push(s * voice.amp * env);
    }
    out
}

/// Feedback comb filter with damping (one building block of the reverb).
struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            pos: 0,
            store: 0.0,
        }
    }

    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.buf[self.pos];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.pos] = x + self.store * feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

/// Schroeder all-pass filter (diffuses the echoes).
struct AllPass {
    buf: Vec<f32>,
    pos: usize,
}

impl AllPass {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            pos: 0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let delayed = self.buf[self.pos];
        let y = -x + delayed;
        self.buf[self.pos] = x + delayed * 0.5;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

/// Small room reverb on one channel; `spread` offsets the delays for stereo.
fn reverb(input: &[f32], sr: f32, spread: f32) -> Vec<f32> {
    let ms = |m: f32| ((m + spread) * sr / 1000.0) as usize;
    let mut combs: Vec<Comb> = [29.7, 37.1, 41.1, 43.7]
        .into_iter()
        .map(|m| Comb::new(ms(m)))
        .collect();
    let mut allpasses = [AllPass::new(ms(5.0)), AllPass::new(ms(1.7))];
    input
        .iter()
        .map(|&x| {
            let mut y: f32 = combs.iter_mut().map(|c| c.process(x, 0.74, 0.3)).sum();
            y *= 0.25;
            for ap in &mut allpasses {
                y = ap.process(y);
            }
            y
        })
        .collect()
}

/// Renders `sfx` as stereo samples `[left, right]` at `sample_rate`.
pub fn render(sfx: Sfx, sample_rate: u32) -> Vec<[f32; 2]> {
    let sr = sample_rate as f32;
    let voices = voices(sfx);
    let dry_end = voices
        .iter()
        .map(|v| v.start + v.length)
        .fold(0.0, f32::max);
    let tail = 0.35 * sfx.wet() / 0.35;
    let len = ((dry_end + tail) * sr).ceil() as usize + 1;
    let mut left = vec![0.0f32; len];
    let mut right = vec![0.0f32; len];
    for (i, voice) in voices.iter().enumerate() {
        let samples = render_voice(voice, sr, 0x9E37_79B9 ^ ((i as u32 + 1) * 7919));
        // Equal-power panning.
        let angle = (voice.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        let (gl, gr) = (angle.cos(), angle.sin());
        let first = (voice.start * sr) as usize;
        for (j, s) in samples.into_iter().enumerate() {
            if let Some(l) = left.get_mut(first + j) {
                *l += s * gl;
            }
            if let Some(r) = right.get_mut(first + j) {
                *r += s * gr;
            }
        }
    }
    let wet = sfx.wet();
    if wet > 0.0 {
        let mono: Vec<f32> = left
            .iter()
            .zip(&right)
            .map(|(l, r)| (l + r) * 0.5)
            .collect();
        let (rl, rr) = (reverb(&mono, sr, 0.0), reverb(&mono, sr, 1.3));
        for i in 0..len {
            left[i] += rl[i] * wet;
            right[i] += rr[i] * wet;
        }
    }
    // Soft saturation: normalize, drive into tanh, so the sound is dense and
    // loud without hard clipping. Then normalize to the target peak.
    let drive = sfx.drive();
    if drive > 1.0 {
        let p = left
            .iter()
            .chain(&right)
            .fold(0.0f32, |m, s| m.max(s.abs()))
            .max(1e-6);
        for s in left.iter_mut().chain(right.iter_mut()) {
            *s = (*s / p * drive).tanh();
        }
    }
    // Normalize to the sound's target loudness, and fade the very end.
    let peak = left
        .iter()
        .chain(&right)
        .fold(0.0f32, |m, s| m.max(s.abs()))
        .max(1e-6);
    let gain = sfx.peak() / peak;
    let fade = (0.03 * sr) as usize;
    (0..len)
        .map(|i| {
            let f = ((len - i) as f32 / fade as f32).min(1.0);
            [left[i] * gain * f, right[i] * gain * f]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(s: &[[f32; 2]]) -> f32 {
        s.iter()
            .fold(0.0f32, |m, [l, r]| m.max(l.abs()).max(r.abs()))
    }

    #[test]
    fn sounds_are_loud_enough_short_and_clean() {
        for sfx in Sfx::ALL {
            let s = render(sfx, 44_100);
            let seconds = s.len() as f32 / 44_100.0;
            assert!(seconds < 1.8, "{sfx:?} too long: {seconds}");
            let p = peak(&s);
            assert!((p - sfx.peak()).abs() < 0.01, "{sfx:?} peak {p}");
            let [l0, r0] = s[0];
            let [l1, r1] = s[s.len() - 1];
            assert!(
                l0.abs() < 0.02 && r0.abs() < 0.02,
                "{sfx:?} starts with a click"
            );
            assert!(
                l1.abs() < 0.01 && r1.abs() < 0.01,
                "{sfx:?} ends with a click"
            );
        }
        assert!(sfx_peak(Sfx::Hover) < sfx_peak(Sfx::MoveUp) * 0.5);
        // Loud: menu moves carry real energy, not just a short spike.
        let rms = |sfx| {
            let s = render(sfx, 44_100);
            let n = (0.05 * 44_100.0) as usize;
            (s[..n].iter().map(|[l, r]| l * l + r * r).sum::<f32>() / (2 * n) as f32).sqrt()
        };
        for sfx in [Sfx::MoveUp, Sfx::Confirm, Sfx::Count, Sfx::Go] {
            assert!(rms(sfx) > 0.25, "{sfx:?} rms {}", rms(sfx));
        }
    }

    fn sfx_peak(sfx: Sfx) -> f32 {
        peak(&render(sfx, 44_100))
    }

    #[test]
    fn left_and_right_changes_are_panned() {
        let energy = |s: &[[f32; 2]], ch: usize| s.iter().map(|f| f[ch] * f[ch]).sum::<f32>();
        let left = render(Sfx::ChangeLeft, 44_100);
        let right = render(Sfx::ChangeRight, 44_100);
        assert!(energy(&left, 0) > energy(&left, 1) * 1.3);
        assert!(energy(&right, 1) > energy(&right, 0) * 1.3);
    }
}
