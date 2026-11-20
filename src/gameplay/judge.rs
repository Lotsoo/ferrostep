//! Timing windows and judgments.

/// How accurate a step was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Judgment {
    Marvelous,
    Perfect,
    Great,
    Good,
    Boo,
    Miss,
}

impl Judgment {
    pub const ALL: [Judgment; 6] = [
        Judgment::Marvelous,
        Judgment::Perfect,
        Judgment::Great,
        Judgment::Good,
        Judgment::Boo,
        Judgment::Miss,
    ];

    /// Index 0..6, handy for arrays.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Marvelous/Perfect/Great keep the combo going.
    pub fn keeps_combo(self) -> bool {
        matches!(self, Self::Marvelous | Self::Perfect | Self::Great)
    }

    /// Text shown to the player.
    pub fn label(self) -> &'static str {
        match self {
            Self::Marvelous => "Marvelous",
            Self::Perfect => "Perfect",
            Self::Great => "Great",
            Self::Good => "Good",
            Self::Boo => "Boo",
            Self::Miss => "Miss",
        }
    }
}

/// Window edges in seconds for Marvelous..Boo (StepMania defaults).
pub const WINDOWS: [f64; 5] = [0.0225, 0.045, 0.090, 0.135, 0.180];

/// Hold must not be released for longer than this (seconds).
pub const HOLD_LET_GO: f64 = 0.25;
/// Rolls must be tapped at least this often (seconds).
pub const ROLL_LET_GO: f64 = 0.5;

/// Largest distance (seconds) at which a press still counts. Beyond: Miss.
pub fn miss_window(scale: f64) -> f64 {
    WINDOWS[4] * scale
}

/// Judges a press `diff` seconds away from the note (sign ignored).
///
/// Returns `None` when the press is outside every window.
pub fn judge(diff: f64, scale: f64) -> Option<Judgment> {
    let d = diff.abs();
    WINDOWS
        .iter()
        .position(|w| d <= w * scale)
        .map(|i| Judgment::ALL[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_edges() {
        let cases = [
            (0.0, Some(Judgment::Marvelous)),
            (0.0225, Some(Judgment::Marvelous)),
            (-0.0226, Some(Judgment::Perfect)),
            (0.045, Some(Judgment::Perfect)),
            (0.0451, Some(Judgment::Great)),
            (-0.090, Some(Judgment::Great)),
            (0.1, Some(Judgment::Good)),
            (0.135, Some(Judgment::Good)),
            (-0.15, Some(Judgment::Boo)),
            (0.180, Some(Judgment::Boo)),
            (0.1801, None),
            (-0.2, None),
        ];
        for (diff, expected) in cases {
            assert_eq!(judge(diff, 1.0), expected, "diff {diff}");
        }
    }

    #[test]
    fn sweep_minus_200_to_plus_200_ms() {
        for ms in -200i32..=200 {
            let diff = f64::from(ms) / 1000.0;
            let j = judge(diff, 1.0);
            let a = ms.abs();
            let expected = match a {
                0..=22 => Some(Judgment::Marvelous),
                23..=45 => Some(Judgment::Perfect),
                46..=90 => Some(Judgment::Great),
                91..=135 => Some(Judgment::Good),
                136..=180 => Some(Judgment::Boo),
                _ => None,
            };
            assert_eq!(j, expected, "{ms} ms");
        }
    }

    #[test]
    fn scale_makes_windows_stricter() {
        assert_eq!(judge(0.04, 0.5), Some(Judgment::Great));
        assert_eq!(miss_window(0.5), 0.09);
    }
}
