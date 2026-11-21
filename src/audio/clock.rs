//! Smooth, monotonic song clock driven by the audio playback position.
//!
//! The audio backend updates its position once per buffer (≈10.7 ms for 512
//! frames at 48 kHz), while the screen refreshes at its own rate (e.g. every
//! 6.9 ms at 144 Hz). Following the raw position directly makes the song time
//! advance in uneven steps (3 ms, 10.7 ms, 6 ms, …) and the arrows shimmer.
//!
//! So the clock runs on the monotonic system timer and is only *steered*
//! towards the audio position: each frame it moves a small, time-based
//! fraction of the way to where the audio says it should be. Buffer-sized
//! jitter averages out, while real drift is corrected within a few hundred
//! milliseconds. Large differences (a stall, a seek) snap instead. It never
//! runs backwards, so judgments stay consistent.

use std::time::Instant;

/// Time constant of the correction towards the audio position (seconds).
const STEER_TAU: f64 = 0.15;
/// Differences larger than this are not smoothed (seek, stall, resume).
const SNAP: f64 = 0.06;

/// See module docs.
#[derive(Debug, Clone)]
pub struct AudioClock {
    /// Latest raw audio position and when it was first seen.
    last_raw: f64,
    last_raw_instant: Instant,
    /// Smoothed output and when it was computed.
    output: f64,
    output_instant: Instant,
    rate: f64,
    /// How far past the last raw position the estimate may run (audio stalled).
    max_ahead: f64,
}

impl AudioClock {
    /// Starts the clock at playback position `raw` (seconds).
    pub fn new(raw: f64, now: Instant) -> Self {
        Self {
            last_raw: raw,
            last_raw_instant: now,
            output: raw,
            output_instant: now,
            rate: 1.0,
            max_ahead: 0.05,
        }
    }

    /// Playback rate (1.0 = normal speed).
    pub fn set_rate(&mut self, rate: f64) {
        self.rate = rate;
    }

    /// Feeds the latest raw position and returns the smoothed time.
    pub fn update(&mut self, raw: f64, now: Instant) -> f64 {
        if raw != self.last_raw {
            self.last_raw = raw;
            self.last_raw_instant = now;
        }
        // Where the audio says we are (interpolated since its last update).
        let since_raw = now
            .saturating_duration_since(self.last_raw_instant)
            .as_secs_f64();
        let target = self.last_raw + (since_raw * self.rate).min(self.max_ahead);

        // Where a perfectly steady clock would be now.
        let dt = now
            .saturating_duration_since(self.output_instant)
            .as_secs_f64();
        let predicted = self.output + dt * self.rate;
        let error = target - predicted;

        let next = if error > SNAP {
            target // fell behind (e.g. the app stalled): jump forward
        } else if error < -SNAP {
            self.output // audio stalled: wait for it instead of running away
        } else {
            let gain = 1.0 - (-dt / STEER_TAU).exp();
            predicted + error * gain
        };
        self.output = next.max(self.output);
        self.output_instant = now;
        self.output
    }

    /// Restarts from `raw` (after pause/seek). May go backwards.
    pub fn reset(&mut self, raw: f64, now: Instant) {
        *self = Self {
            rate: self.rate,
            ..Self::new(raw, now)
        };
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn secs(s: f64) -> Duration {
        Duration::from_secs_f64(s)
    }

    #[test]
    fn runs_with_the_timer_between_updates() {
        let t0 = Instant::now();
        let mut c = AudioClock::new(1.0, t0);
        let v = c.update(1.0, t0 + secs(0.004));
        assert!((v - 1.004).abs() < 1e-6, "{v}");
    }

    /// Audio reports in 10.667 ms steps, frames come every 6.944 ms (144 Hz):
    /// the output must advance evenly and stay close to the true time.
    #[test]
    fn smooths_buffer_sized_steps() {
        let t0 = Instant::now();
        let buffer = 512.0 / 48_000.0;
        let frame = 1.0 / 144.0;
        let mut c = AudioClock::new(0.0, t0);
        let mut prev = 0.0;
        for i in 1..1000 {
            let t = i as f64 * frame;
            let raw = (t / buffer).floor() * buffer;
            let out = c.update(raw, t0 + secs(t));
            if i > 150 {
                let step = out - prev;
                assert!(
                    (step - frame).abs() < 0.0012,
                    "uneven step {step} at frame {i}"
                );
                assert!((out - t).abs() < 0.012, "off by {} at frame {i}", out - t);
            }
            prev = out;
        }
    }

    #[test]
    fn follows_drift() {
        // The audio runs 0.5% fast: the clock must follow it.
        let t0 = Instant::now();
        let mut c = AudioClock::new(0.0, t0);
        let mut out = 0.0;
        for i in 1..=600 {
            let t = i as f64 / 120.0;
            out = c.update(t * 1.005, t0 + secs(t));
        }
        assert!((out - 5.0 * 1.005).abs() < 0.005, "{out}");
    }

    #[test]
    fn never_goes_backwards_and_waits_for_a_stalled_stream() {
        let t0 = Instant::now();
        let mut c = AudioClock::new(1.0, t0);
        let mut prev = 0.0;
        for i in 1..=100 {
            let out = c.update(1.0, t0 + secs(i as f64 * 0.01));
            assert!(out >= prev);
            prev = out;
        }
        // Stalled for a second: the clock may only run a little past the audio.
        assert!(prev <= 1.0 + 0.05 + SNAP + 0.01, "{prev}");
    }

    #[test]
    fn snaps_forward_after_a_long_frame() {
        let t0 = Instant::now();
        let mut c = AudioClock::new(0.0, t0);
        c.update(0.0, t0 + secs(0.01));
        let out = c.update(0.5, t0 + secs(0.5));
        assert!((out - 0.5).abs() < 1e-6, "{out}");
    }
}
