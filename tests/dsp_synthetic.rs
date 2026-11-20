//! Accuracy tests for onset/BPM/offset detection on synthetic audio.

use std::sync::atomic::AtomicBool;

use ferrostep::audio_dsp::{analyze, decode::decode_file, synth};

const SR: u32 = 44_100;

/// Distance between two beat phases, modulo the beat period.
fn phase_error(a: f64, b: f64, period: f64) -> f64 {
    let d = (a - b).rem_euclid(period);
    d.min(period - d)
}

fn check(name: &str, samples: &[f32], bpm: f64, first_beat: f64) {
    let result = analyze(samples, SR, &mut |_| {}).expect("analysis should succeed");
    let t = result.tempo;
    let period = 60.0 / bpm;
    let err_ms = phase_error(t.first_beat, first_beat, period) * 1000.0;
    println!(
        "{name}: detected {:.3} BPM (expected {bpm}), phase error {err_ms:.1} ms",
        t.bpm
    );
    assert!((t.bpm - bpm).abs() <= 0.1, "{name}: bpm {} vs {bpm}", t.bpm);
    assert!(err_ms <= 10.0, "{name}: phase error {err_ms:.1} ms");
    assert!(
        (t.offset + t.first_beat).abs() < 1e-12,
        "offset convention: #OFFSET = -first_beat"
    );
}

#[test]
fn click_track_128_bpm() {
    let mut buf = synth::click_track(SR, 60.0, 128.0, 0.3, 4);
    synth::add_noise(&mut buf, 0.03, 1);
    check("click 128", &buf, 128.0, 0.3);
}

#[test]
fn click_track_140_bpm() {
    let mut buf = synth::click_track(SR, 45.0, 140.0, 1.234, 4);
    synth::add_noise(&mut buf, 0.03, 2);
    check("click 140", &buf, 140.0, 1.234);
}

#[test]
fn click_track_95_5_bpm() {
    let mut buf = synth::click_track(SR, 60.0, 95.5, 0.75, 4);
    synth::add_noise(&mut buf, 0.03, 3);
    check("click 95.5", &buf, 95.5, 0.75);
}

#[test]
fn drum_pattern_120_bpm() {
    // kick on the beat, hi-hat on the off-beat: phase must lock to the kicks
    let mut buf = synth::drum_pattern(SR, 40.0, 120.0, 0.6, 7);
    synth::add_noise(&mut buf, 0.02, 4);
    check("drums 120", &buf, 120.0, 0.6);
}

#[test]
fn onsets_have_bands() {
    let buf = synth::drum_pattern(SR, 20.0, 120.0, 0.5, 9);
    let result = analyze(&buf, SR, &mut |_| {}).unwrap();
    use ferrostep::audio_dsp::onset::Band;
    let low = result.onsets.iter().filter(|o| o.band == Band::Low).count();
    let high = result
        .onsets
        .iter()
        .filter(|o| o.band == Band::High)
        .count();
    assert!(low > 10 && high > 10, "low {low}, high {high}");
}

#[test]
fn too_short_audio_is_rejected() {
    let buf = synth::click_track(SR, 5.0, 120.0, 0.5, 4);
    assert!(analyze(&buf, SR, &mut |_| {}).is_err());
}

#[test]
fn decode_wav_round_trip() {
    let dir = std::env::temp_dir().join(format!("ferrostep-decode-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("click.wav");
    let buf = synth::click_track(SR, 3.0, 120.0, 0.5, 4);
    ferrostep::library::test_song::write_wav(&path, &buf, SR).unwrap();
    let decoded = decode_file(&path, None, &AtomicBool::new(false), &mut |_| {}).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(decoded.sample_rate, SR);
    assert_eq!(decoded.samples.len(), buf.len());
    assert!((decoded.samples[22_050 + 30] - buf[22_050 + 30]).abs() < 1e-3);
}
