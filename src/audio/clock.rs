//! Smooth, monotonic song clock driven by the audio playback position.
//!
//! The audio backend updates its position once per buffer (every few ms),
//! but the screen refreshes faster. We interpolate between updates with a
//! monotonic timer, never run more than `max_ahead` past the last real
//! position, and never go backwards.

use std::time::Instant;

/// See module docs.
#[derive(Debug, Clone)]
pub struct AudioClock {
    last_raw: f64,
    last_instant: Instant,
    last_output: f64,
    rate: f64,
    max_ahead: f64,
}

impl AudioClock {
    /// Starts the clock at playback position `raw` (seconds).
    pub fn new(raw: f64, now: Instant) -> Self {
        Self {
            last_raw: raw,
            last_instant: now,
            last_output: raw,
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
            self.last_instant = now;
        }
        let elapsed = now
            .saturating_duration_since(self.last_instant)
            .as_secs_f64()
            * self.rate;
        let smooth = self.last_raw + elapsed.min(self.max_ahead);
        self.last_output = smooth.max(self.last_output);
        self.last_output
    }

    /// Restarts interpolation from `raw` (after pause/seek). May go backwards.
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

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn interpolates_between_updates() {
        let t0 = Instant::now();
        let mut c = AudioClock::new(1.0, t0);
        let v = c.update(1.0, t0 + ms(4));
        assert!((v - 1.004).abs() < 1e-9);
    }

    #[test]
    fn never_runs_too_far_ahead() {
        let t0 = Instant::now();
        let mut c = AudioClock::new(1.0, t0);
        assert!((c.update(1.0, t0 + ms(500)) - 1.05).abs() < 1e-9);
    }

    #[test]
    fn never_goes_backwards() {
        let t0 = Instant::now();
        let mut c = AudioClock::new(1.0, t0);
        let a = c.update(1.0, t0 + ms(10)); // 1.010
        let b = c.update(1.005, t0 + ms(11)); // raw is behind the interpolated value
        assert!(b >= a);
        let later = c.update(1.005, t0 + ms(20)); // continues from 1.005 + 9 ms
        assert!((later - 1.014).abs() < 1e-9);
    }
}
