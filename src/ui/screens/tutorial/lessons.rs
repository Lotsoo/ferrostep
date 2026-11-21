//! Tutorial content: the melody, the six lessons and the notes built from them.

use std::time::Instant;

/// Tutorial tempo (beats per minute).
pub(super) const BPM: f64 = 90.0;
/// A press this early (seconds) before an arrow arrives still counts.
pub(super) const EARLY_WINDOW: f64 = 0.25;
/// Pause between lessons (seconds).
pub(super) const LESSON_BREAK: f32 = 1.4;

// Melody notes (Hz), one octave above middle C.
pub(super) const C: f32 = 523.25;
pub(super) const D: f32 = 587.33;
pub(super) const E: f32 = 659.25;
pub(super) const F: f32 = 698.46;
pub(super) const G: f32 = 783.99;
pub(super) const A: f32 = 880.0;

pub(super) const PHRASE_1: [f32; 7] = [C, C, G, G, A, A, G];
pub(super) const PHRASE_2: [f32; 7] = [F, F, E, E, D, D, C];
pub(super) const PHRASE_3: [f32; 7] = [G, G, F, F, E, E, D];

/// One step of a lesson: the columns pressed together and an optional hold
/// length in beats.
pub(super) struct Step {
    pub(super) columns: &'static [u8],
    pub(super) hold: f64,
}

pub(super) const fn tap(columns: &'static [u8]) -> Step {
    Step { columns, hold: 0.0 }
}

pub(super) const fn hold(column: &'static [u8], beats: f64) -> Step {
    Step {
        columns: column,
        hold: beats,
    }
}

pub(super) struct LessonDef {
    pub(super) title: &'static str,
    pub(super) text: &'static str,
    /// Beats between steps.
    pub(super) spacing: f64,
    pub(super) melody: [f32; 7],
    pub(super) steps: [Step; 7],
}

pub(super) fn lesson_defs() -> [LessonDef; 6] {
    [
        LessonDef {
            title: "Tekan saat panah sampai di target",
            text: "Panah naik ke target di atas, lalu BERHENTI dan menunggu kamu. Tekan tombol (atau injak panah) yang arahnya sama.",
            spacing: 2.0,
            melody: PHRASE_1,
            steps: [
                tap(&[0]),
                tap(&[1]),
                tap(&[2]),
                tap(&[3]),
                tap(&[3]),
                tap(&[2]),
                tap(&[1]),
            ],
        },
        LessonDef {
            title: "Setiap arah punya warna",
            text: "Kiri merah muda, bawah biru, atas hijau, kanan kuning. Tidak perlu buru-buru: lagunya menunggu kamu.",
            spacing: 1.5,
            melody: PHRASE_2,
            steps: [
                tap(&[3]),
                tap(&[0]),
                tap(&[2]),
                tap(&[1]),
                tap(&[3]),
                tap(&[0]),
                tap(&[1]),
            ],
        },
        LessonDef {
            title: "Tekan tepat waktu",
            text: "Sekarang coba tekan TEPAT saat panah menutupi target, sebelum ia berhenti. Kalau pas, muncul Perfect!",
            spacing: 1.0,
            melody: PHRASE_3,
            steps: [
                tap(&[0]),
                tap(&[2]),
                tap(&[1]),
                tap(&[3]),
                tap(&[0]),
                tap(&[2]),
                tap(&[1]),
            ],
        },
        LessonDef {
            title: "Dua panah sekaligus",
            text: "Kalau dua panah datang bersamaan, tekan keduanya sekaligus. Di dance mat: injak dengan dua kaki (lompat).",
            spacing: 2.0,
            melody: PHRASE_3,
            steps: [
                tap(&[0, 3]),
                tap(&[1, 2]),
                tap(&[0, 3]),
                tap(&[1, 2]),
                tap(&[0, 2]),
                tap(&[1, 3]),
                tap(&[0, 3]),
            ],
        },
        LessonDef {
            title: "Panah panjang: tahan",
            text: "Panah dengan ekor panjang harus DITAHAN sampai ekornya habis. Kalau dilepas, lagunya menunggu sampai kamu menahan lagi.",
            spacing: 1.5,
            melody: PHRASE_1,
            steps: [
                tap(&[0]),
                tap(&[3]),
                hold(&[1], 2.0),
                tap(&[2]),
                tap(&[0]),
                tap(&[3]),
                hold(&[2], 3.0),
            ],
        },
        LessonDef {
            title: "Gabungkan semuanya",
            text: "Satu lagu kecil dengan semua yang sudah kamu pelajari. Kamu pasti bisa!",
            spacing: 1.0,
            melody: PHRASE_2,
            steps: [
                tap(&[0]),
                tap(&[3]),
                tap(&[1]),
                tap(&[2]),
                tap(&[0, 3]),
                tap(&[1]),
                hold(&[0, 3], 3.0),
            ],
        },
    ]
}

#[derive(Debug, Clone)]
pub(super) struct Note {
    pub(super) beat: f64,
    pub(super) column: u8,
    /// Hold length in beats (0 = tap).
    pub(super) hold: f64,
    pub(super) pitch: f32,
    /// Index of the step (notes of one step share the melody note).
    pub(super) step: usize,
    pub(super) hit: bool,
    /// Hold finished (or tap: same as `hit`).
    pub(super) done: bool,
    /// When the arrow reached the target and started waiting.
    pub(super) arrived: Option<Instant>,
}

pub(super) fn build_notes(def: &LessonDef) -> Vec<Note> {
    let mut notes = Vec::new();
    let mut beat = 0.0;
    for (i, step) in def.steps.iter().enumerate() {
        for &column in step.columns {
            notes.push(Note {
                beat,
                column,
                hold: step.hold,
                pitch: def.melody[i],
                step: i,
                hit: false,
                done: false,
                arrived: None,
            });
        }
        beat += def.spacing + step.hold;
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lessons_have_one_melody_note_per_step() {
        for def in lesson_defs() {
            let notes = build_notes(&def);
            let steps = def.steps.iter().map(|s| s.columns.len()).sum::<usize>();
            assert_eq!(notes.len(), steps);
            // Notes of one step share a beat; steps never overlap a hold.
            for w in notes.windows(2) {
                if w[0].step != w[1].step {
                    assert!(w[1].beat >= w[0].beat + w[0].hold + def.spacing - 1e-9);
                } else {
                    assert_eq!(w[0].beat, w[1].beat);
                }
            }
        }
    }
}
