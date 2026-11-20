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

/// A spring described the way Apple designers describe it (WWDC 2018
/// *Designing Fluid Interfaces*): a **damping ratio** (1.0 = no overshoot,
/// < 1.0 = bouncy) and a **response** in seconds (how quickly it gets there).
/// It has no fixed duration.
///
/// Springs are interruptible: changing the target keeps the current value and
/// velocity, so motion never jumps or hits a "brick wall".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
    pub damping: f32,
    pub response: f32,
}

impl Spring {
    /// Default UI spring: critically damped, no overshoot.
    pub const SMOOTH: (f32, f32) = (1.0, 0.35);
    /// Only after a gesture carried momentum (a flick): a little bounce.
    pub const MOMENTUM: (f32, f32) = (0.8, 0.35);
    /// Quick follow for scrolling and highlights.
    pub const SNAPPY: (f32, f32) = (1.0, 0.22);

    pub const fn new(value: f32, (damping, response): (f32, f32)) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            damping,
            response,
        }
    }

    /// Changes where the spring is going, keeping its current motion.
    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    /// Changes the feel (damping, response) without interrupting the motion.
    pub fn set_params(&mut self, (damping, response): (f32, f32)) {
        self.damping = damping;
        self.response = response;
    }

    /// Jumps straight to `value` (reduced motion, first frame).
    pub fn snap(&mut self, value: f32) {
        self.value = value;
        self.target = value;
        self.velocity = 0.0;
    }

    /// Advances by `dt` seconds and returns the new value.
    pub fn step(&mut self, dt: f32) -> f32 {
        // Mass 1: stiffness k = (2π / response)², damping c = 4π·ζ / response.
        let response = self.response.max(0.01);
        let k = (std::f32::consts::TAU / response).powi(2);
        let c = 4.0 * std::f32::consts::PI * self.damping / response;
        // Sub-steps keep the integration stable after a slow frame.
        let mut remaining = dt.clamp(0.0, 0.1);
        while remaining > 0.0 {
            let h = remaining.min(1.0 / 240.0);
            let force = -k * (self.value - self.target) - c * self.velocity;
            self.velocity += force * h;
            self.value += self.velocity * h;
            remaining -= h;
        }
        self.value
    }

    /// True when the spring is at rest within `epsilon` (in the value's units).
    pub fn is_settled(&self, epsilon: f32) -> bool {
        (self.value - self.target).abs() < epsilon && self.velocity.abs() < epsilon * 10.0
    }
}

/// Where a flick would come to rest: Apple's momentum projection
/// (`decelerationRate` 0.998 = normal scroll feel). Velocity in units/second.
pub fn project(velocity: f32, deceleration_rate: f32) -> f32 {
    (velocity / 1000.0) * deceleration_rate / (1.0 - deceleration_rate)
}

/// Rubber-band resistance past a boundary: the further you pull, the less
/// the content follows (Apple's constant 0.55).
pub fn rubberband(overshoot: f32, dimension: f32) -> f32 {
    const C: f32 = 0.55;
    let d = dimension.max(1.0);
    overshoot * d * C / (d + C * overshoot.abs())
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

    fn run(mut s: Spring, seconds: f32) -> (Spring, f32) {
        let mut max: f32 = s.value;
        let mut t = 0.0;
        while t < seconds {
            s.step(1.0 / 120.0);
            max = max.max(s.value);
            t += 1.0 / 120.0;
        }
        (s, max)
    }

    #[test]
    fn critically_damped_spring_settles_without_overshoot() {
        let mut s = Spring::new(0.0, Spring::SMOOTH);
        s.set_target(100.0);
        let (s, max) = run(s, 1.5);
        assert!(s.is_settled(0.5), "{s:?}");
        assert!(max <= 100.0 + 1e-3, "overshoot {max}");
    }

    #[test]
    fn bouncy_spring_overshoots_a_little() {
        let mut s = Spring::new(0.0, Spring::MOMENTUM);
        s.set_target(100.0);
        let (s, max) = run(s, 2.0);
        assert!(max > 100.5 && max < 110.0, "{max}");
        assert!(s.is_settled(0.5));
    }

    #[test]
    fn retarget_keeps_value_and_velocity() {
        let mut s = Spring::new(0.0, Spring::SMOOTH);
        s.set_target(100.0);
        for _ in 0..10 {
            s.step(1.0 / 60.0);
        }
        let (value, velocity) = (s.value, s.velocity);
        s.set_target(-50.0);
        assert_eq!(
            (s.value, s.velocity),
            (value, velocity),
            "no jump on interrupt"
        );
    }

    #[test]
    fn momentum_and_rubberband() {
        // 1000 px/s flick with d = 0.998 travels 499 px.
        assert!((project(1000.0, 0.998) - 499.0).abs() < 0.5);
        assert!(project(-1000.0, 0.998) < 0.0);
        // Resistance grows: pulling 400 px past the edge of a 600 px view moves far less.
        let r = rubberband(400.0, 600.0);
        assert!(r > 0.0 && r < 200.0, "{r}");
        assert!(rubberband(10.0, 600.0) > 5.0);
        assert!((rubberband(-50.0, 600.0) + rubberband(50.0, 600.0)).abs() < 1e-4);
    }
}
