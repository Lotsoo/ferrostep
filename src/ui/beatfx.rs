//! Music-reactive decoration for menus: everything pulses on the beat of the
//! song that is playing, so the menus feel like part of a rhythm game.
//!
//! The beat comes from the preview's playback position mapped through the
//! song's timing (same math as gameplay), so it is in sync with what you hear.

use std::f32::consts::TAU;

use eframe::egui::{Color32, Painter, Pos2, Rect, Stroke, pos2, vec2};

use crate::timing::TimingMap;

/// Where we are in the music right now.
#[derive(Debug, Clone, Copy)]
pub struct Beat {
    /// Beat number (fractional). Keeps counting while idle.
    pub beat: f64,
    /// 1.0 right on a beat, decaying to 0 before the next one.
    pub pulse: f32,
    /// Stronger pulse on the first beat of each 4-beat bar.
    pub downbeat: f32,
    pub bpm: f32,
    /// True when real music drives it (false = idle breathing).
    pub live: bool,
}

impl Beat {
    /// Beat from music position (seconds) and the song's timing.
    pub fn from_music(map: &TimingMap, seconds: f64) -> Self {
        let beat = map.seconds_to_beat(seconds);
        Self::at(beat, map.bpm_at(beat) as f32, true)
    }

    /// Slow idle clock when nothing plays (keeps the scene alive, softly).
    pub fn idle(seconds: f32) -> Self {
        let mut b = Self::at(f64::from(seconds) * 80.0 / 60.0, 80.0, false);
        b.pulse *= 0.35;
        b.downbeat *= 0.35;
        b
    }

    /// Beat from an already-known beat number (gameplay has its own timing).
    pub fn at(beat: f64, bpm: f32, live: bool) -> Self {
        let phase = beat.rem_euclid(1.0) as f32;
        // Sharp attack, quick decay: reads as a "kick".
        let pulse = if beat < 0.0 {
            0.0
        } else {
            (1.0 - phase).powi(4)
        };
        let downbeat = if beat.rem_euclid(4.0) < 1.0 {
            pulse
        } else {
            0.0
        };
        Self {
            beat,
            pulse,
            downbeat,
            bpm,
            live,
        }
    }

    /// Calm version for "reduce motion": no flashing.
    pub fn calm(mut self) -> Self {
        self.pulse *= 0.25;
        self.downbeat = 0.0;
        self
    }
}

/// Neon floor in perspective, rushing toward the viewer one line per beat
/// (the classic rhythm-game "highway").
pub fn draw_highway(painter: &Painter, screen: Rect, tint: Color32, beat: &Beat) {
    let horizon = screen.top() + screen.height() * 0.58;
    let floor = Rect::from_min_max(pos2(screen.left(), horizon), screen.max);
    let vanish = pos2(screen.center().x, horizon - screen.height() * 0.08);
    let line = |a: f32| tint.lerp_to_gamma(Color32::WHITE, 0.2).gamma_multiply(a);
    let painter = painter.with_clip_rect(floor);

    // Horizon glow, brighter on the beat.
    super::notefield::radial_light(
        &painter,
        pos2(screen.center().x, horizon),
        screen.width() * 0.45,
        tint,
        0.22 + 0.25 * beat.pulse,
    );
    // Rails converging to the vanishing point.
    let rails = 14;
    for i in 0..=rails {
        let x =
            screen.left() - screen.width() * 0.6 + (screen.width() * 2.2) * i as f32 / rails as f32;
        let bottom = pos2(x, screen.bottom());
        let center_bias = 1.0 - ((i as f32 / rails as f32) - 0.5).abs() * 2.0;
        painter.line_segment(
            [vanish, bottom],
            Stroke::new(1.0, line(0.10 + 0.08 * center_bias)),
        );
    }
    // Cross lines: one per beat, moving toward the viewer.
    let depth = 8;
    let frac = beat.beat.rem_euclid(1.0) as f32;
    for k in 0..depth {
        // z: 0 = horizon, 1 = bottom of the screen.
        let z = ((k as f32 + frac) / depth as f32).powf(2.2);
        let y = horizon + (screen.bottom() - horizon) * z;
        let is_bar = (beat.beat.floor() as i64 - k as i64 + depth as i64).rem_euclid(4) == 0;
        let a = (0.05 + 0.25 * z) * if is_bar { 1.6 } else { 1.0 };
        painter.line_segment(
            [pos2(screen.left(), y), pos2(screen.right(), y)],
            Stroke::new(1.0 + z * 1.5, line(a.min(0.6))),
        );
    }
    // Fade the floor into the background at the horizon.
    let fade = Rect::from_min_max(floor.min, pos2(floor.right(), horizon + 40.0));
    super::widgets::vertical_gradient(
        &painter,
        fade,
        Color32::from_black_alpha(90),
        Color32::TRANSPARENT,
    );
}

/// Spectrum-style bars. Not a real FFT: heights follow the beat with a
/// different rhythm per band (kick low, hats high), which reads as "music".
pub fn draw_equalizer(painter: &Painter, rect: Rect, colors: (Color32, Color32), beat: &Beat) {
    let bars = ((rect.width() / 9.0) as usize).clamp(8, 96);
    let gap = 3.0;
    let w = (rect.width() - gap * (bars - 1) as f32) / bars as f32;
    let t = beat.beat as f32;
    for i in 0..bars {
        let x = i as f32 / (bars - 1) as f32; // 0 = bass, 1 = treble
        // Kick on every beat for bass, eighths for mids, sixteenths for highs.
        let sub = if x < 0.3 {
            1.0
        } else if x < 0.7 {
            2.0
        } else {
            4.0
        };
        let phase = (t * sub).rem_euclid(1.0);
        let hit = (1.0 - phase).powi(3);
        let wobble = 0.5 + 0.5 * ((t * 0.9 + i as f32 * 0.7).sin() * (i as f32 * 1.3).cos());
        let shape = 1.0 - x * 0.45; // more energy in the bass
        let level = if beat.live {
            (0.15 + 0.55 * hit * shape + 0.3 * wobble * shape).clamp(0.06, 1.0)
        } else {
            0.08 + 0.12 * wobble
        };
        let h = rect.height() * level;
        let bar = Rect::from_min_size(
            pos2(rect.left() + i as f32 * (w + gap), rect.bottom() - h),
            vec2(w, h),
        );
        let color = colors.0.lerp_to_gamma(colors.1, x);
        super::hud::gradient_tile(
            painter,
            bar,
            (w * 0.5).min(3.0) as u8,
            color.lerp_to_gamma(Color32::WHITE, 0.3),
            color.gamma_multiply(0.35),
        );
    }
}

/// Four dots showing the position inside the bar; the current one pops.
pub fn draw_metronome(painter: &Painter, left_center: Pos2, color: Color32, beat: &Beat) {
    let current = beat.beat.rem_euclid(4.0).floor() as usize;
    for i in 0..4 {
        let c = left_center + vec2(i as f32 * 18.0, 0.0);
        let on = beat.live && i == current;
        let r = if on { 5.0 + 2.5 * beat.pulse } else { 4.0 };
        if on {
            super::notefield::radial_light(painter, c, 16.0, color, 0.6 * beat.pulse);
        }
        painter.circle_filled(
            c,
            r,
            if on {
                color.lerp_to_gamma(Color32::WHITE, 0.3 * beat.pulse)
            } else {
                Color32::from_white_alpha(40)
            },
        );
    }
}

/// Speaker-cone ring that kicks outward on each beat.
pub fn draw_kick_ring(painter: &Painter, center: Pos2, radius: f32, color: Color32, beat: &Beat) {
    let phase = beat.beat.rem_euclid(1.0) as f32;
    if !beat.live || beat.beat < 0.0 {
        return;
    }
    let r = radius * (1.0 + 0.35 * phase);
    painter.circle_stroke(
        center,
        r,
        Stroke::new(
            3.0 * (1.0 - phase),
            color.gamma_multiply(0.7 * (1.0 - phase)),
        ),
    );
    // Small orbiting sparks at the beat subdivisions.
    for i in 0..4 {
        let a = (beat.beat as f32 * 0.25 + i as f32 / 4.0) * TAU;
        painter.circle_filled(
            center + vec2(a.cos(), a.sin()) * radius * 0.92,
            2.0 + 1.5 * beat.pulse,
            color.gamma_multiply(0.8),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_peaks_on_the_beat_and_downbeat_every_four() {
        let on = Beat::at(8.0, 120.0, true);
        let off = Beat::at(8.5, 120.0, true);
        assert!(on.pulse > 0.99 && off.pulse < 0.1);
        assert!(on.downbeat > 0.99, "beat 8 starts a bar");
        assert_eq!(Beat::at(9.0, 120.0, true).downbeat, 0.0);
        assert_eq!(Beat::at(-0.5, 120.0, true).pulse, 0.0);
    }
}
