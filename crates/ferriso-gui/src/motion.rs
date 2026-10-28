//! Renderer-local easing and short transition timelines.
//!
//! These types keep animation state out of the core actor. An application or
//! adapter advances an [`Animation`] from its frame subscription and feeds the
//! eased progress to an art primitive or a widget style.

use std::time::Duration;

/// Easing curve applied to normalized progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    /// Constant speed.
    Linear,
    /// Fast start with a soft settle.
    #[default]
    EaseOutCubic,
    /// Soft acceleration and deceleration.
    EaseInOutCubic,
    /// Overshoots slightly before settling.
    EaseOutBack,
}

impl Easing {
    /// Ease normalized `progress`.
    ///
    /// Non-finite input is treated as zero. `EaseOutBack` intentionally
    /// overshoots past `1.0`; callers that need a strict bound can clamp the
    /// result.
    #[must_use]
    pub fn ease(self, progress: f32) -> f32 {
        if !progress.is_finite() {
            return 0.0;
        }
        let progress = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => progress,
            Self::EaseOutCubic => 1.0 - (1.0 - progress).powi(3),
            Self::EaseInOutCubic => {
                if progress < 0.5 {
                    4.0 * progress.powi(3)
                } else {
                    1.0 - (-2.0 * progress + 2.0).powi(3) * 0.5
                }
            }
            Self::EaseOutBack => {
                const OVERSHOOT: f32 = 1.701_58;
                let shifted = progress - 1.0;
                1.0 + (OVERSHOOT + 1.0) * shifted.powi(3) + OVERSHOOT * shifted.powi(2)
            }
        }
    }
}

/// A finite transition clock with clamped progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Animation {
    duration: Duration,
    elapsed: Duration,
}

impl Animation {
    /// Create a transition starting at zero.
    #[must_use]
    pub const fn new(duration: Duration) -> Self {
        Self {
            duration,
            elapsed: Duration::ZERO,
        }
    }

    /// Create a transition already at `elapsed`, clamped to its duration.
    #[must_use]
    pub fn with_elapsed(duration: Duration, elapsed: Duration) -> Self {
        Self {
            duration,
            elapsed: elapsed.min(duration),
        }
    }

    /// Total transition duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Elapsed transition time.
    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Normalized progress in `0.0..=1.0`.
    ///
    /// A zero-duration animation is immediately complete.
    #[must_use]
    pub fn progress(&self) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let progress = self.elapsed.as_secs_f64() / self.duration.as_secs_f64();
        progress.clamp(0.0, 1.0) as f32
    }

    /// Progress after applying `easing`.
    #[must_use]
    pub fn eased(&self, easing: Easing) -> f32 {
        easing.ease(self.progress())
    }

    /// Advance the clock and return whether the animation is complete.
    pub fn advance(&mut self, elapsed: Duration) -> bool {
        self.elapsed = self.elapsed.saturating_add(elapsed).min(self.duration);
        self.is_finished()
    }

    /// Restart from zero.
    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
    }

    /// Whether the transition has reached its duration.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.elapsed >= self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_has_stable_endpoints() {
        for easing in [
            Easing::Linear,
            Easing::EaseOutCubic,
            Easing::EaseInOutCubic,
            Easing::EaseOutBack,
        ] {
            assert_eq!(easing.ease(0.0), 0.0);
            assert_eq!(easing.ease(1.0), 1.0);
        }
    }

    #[test]
    fn easing_rejects_non_finite_progress() {
        assert_eq!(Easing::EaseOutCubic.ease(f32::NAN), 0.0);
        assert_eq!(Easing::EaseOutCubic.ease(f32::INFINITY), 0.0);
    }

    #[test]
    fn ease_out_back_overshoots() {
        assert!(Easing::EaseOutBack.ease(0.8) > 1.0);
    }

    #[test]
    fn animation_clamps_advance_and_resets() {
        let mut animation = Animation::new(Duration::from_millis(100));
        assert_eq!(animation.progress(), 0.0);
        assert!(!animation.advance(Duration::from_millis(40)));
        assert!((animation.progress() - 0.4).abs() < f32::EPSILON);
        assert!(animation.advance(Duration::from_secs(1)));
        assert_eq!(animation.progress(), 1.0);
        animation.reset();
        assert_eq!(animation.progress(), 0.0);
    }

    #[test]
    fn zero_duration_animation_is_finished() {
        let animation = Animation::new(Duration::ZERO);
        assert!(animation.is_finished());
        assert_eq!(animation.progress(), 1.0);
    }
}
