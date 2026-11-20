//! Easing curves and small animation helpers. Durations: 150–300 ms.

use std::time::Instant;

/// Standard transition length.
pub const TRANSITION: f32 = 0.25;

pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Overshoots slightly, like a light spring.
pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// Seconds since `start`.
pub fn since(start: Instant, now: Instant) -> f32 {
    now.saturating_duration_since(start).as_secs_f32()
}

/// Progress 0..=1 of an animation of `duration` seconds started at `start`.
///
/// With `reduce_motion`, animations finish instantly.
pub fn progress(start: Instant, now: Instant, duration: f32, reduce_motion: bool) -> f32 {
    if reduce_motion || duration <= 0.0 {
        return 1.0;
    }
    (since(start, now) / duration).clamp(0.0, 1.0)
}

/// Linear interpolation.
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_start_and_end_at_bounds() {
        for f in [ease_out_cubic, ease_in_out_cubic, ease_out_back] {
            assert!(f(0.0).abs() < 1e-6);
            assert!((f(1.0) - 1.0).abs() < 1e-6);
        }
    }
}
