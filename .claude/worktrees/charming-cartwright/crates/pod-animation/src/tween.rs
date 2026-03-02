//! Tween system — fire-and-forget animations with fluent builder API

use serde::{Deserialize, Serialize};
use crate::easing::EasingFunction;

/// Target property that can be tweened
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TweenTarget {
    /// Position X/Y (encoded as single float for builder API)
    Position,
    /// Rotation in radians
    Rotation,
    /// Scale uniform (single axis)
    Scale,
    /// Color brightness (single channel)
    Color,
    /// Alpha transparency
    Alpha,
    /// Sprite frame index
    SpriteFrame,
}

/// A tween animation component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tween {
    pub target: TweenTarget,
    pub start_value: f32,
    pub end_value: f32,
    pub duration: f32,
    pub elapsed: f32,
    pub easing: EasingFunction,
    pub looping: bool,
}

impl Tween {
    pub fn new(
        target: TweenTarget,
        start_value: f32,
        end_value: f32,
        duration: f32,
    ) -> Self {
        Self {
            target,
            start_value,
            end_value,
            duration: duration.max(0.001),
            elapsed: 0.0,
            easing: EasingFunction::Linear,
            looping: false,
        }
    }

    pub fn with_easing(mut self, easing: EasingFunction) -> Self {
        self.easing = easing;
        self
    }

    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    pub fn progress(&self) -> f32 {
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    pub fn is_complete(&self) -> bool {
        self.elapsed >= self.duration
    }
}

/// Fluent builder API for creating tweens
pub struct TweenBuilder {
    target: Option<TweenTarget>,
    start: Option<f32>,
    end: Option<f32>,
    duration: f32,
    easing: EasingFunction,
    looping: bool,
}

impl TweenBuilder {
    pub fn new() -> Self {
        Self {
            target: None,
            start: None,
            end: None,
            duration: 1.0,
            easing: EasingFunction::Linear,
            looping: false,
        }
    }

    // ============================================================
    // Builder setters
    // ============================================================

    pub fn position(mut self, from: f32, to: f32) -> Self {
        self.target = Some(TweenTarget::Position);
        self.start = Some(from);
        self.end = Some(to);
        self
    }

    pub fn rotation(mut self, from: f32, to: f32) -> Self {
        self.target = Some(TweenTarget::Rotation);
        self.start = Some(from);
        self.end = Some(to);
        self
    }

    pub fn scale(mut self, from: f32, to: f32) -> Self {
        self.target = Some(TweenTarget::Scale);
        self.start = Some(from);
        self.end = Some(to);
        self
    }

    pub fn color(mut self, from: f32, to: f32) -> Self {
        self.target = Some(TweenTarget::Color);
        self.start = Some(from);
        self.end = Some(to);
        self
    }

    pub fn alpha(mut self, from: f32, to: f32) -> Self {
        self.target = Some(TweenTarget::Alpha);
        self.start = Some(from);
        self.end = Some(to);
        self
    }

    pub fn sprite_frame(mut self, from: f32, to: f32) -> Self {
        self.target = Some(TweenTarget::SpriteFrame);
        self.start = Some(from);
        self.end = Some(to);
        self
    }

    pub fn duration(mut self, secs: f32) -> Self {
        self.duration = secs.max(0.001);
        self
    }

    pub fn easing(mut self, easing: EasingFunction) -> Self {
        self.easing = easing;
        self
    }

    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    pub fn build(self) -> Option<Tween> {
        match (self.target, self.start, self.end) {
            (Some(target), Some(start), Some(end)) => Some(Tween {
                target,
                start_value: start,
                end_value: end,
                duration: self.duration,
                elapsed: 0.0,
                easing: self.easing,
                looping: self.looping,
            }),
            _ => None,
        }
    }
}

impl Default for TweenBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// Convenience factory functions
// ============================================================

/// Create a tweened position animation
pub fn tween_position(from: f32, to: f32, duration: f32) -> TweenBuilder {
    TweenBuilder::new()
        .position(from, to)
        .duration(duration)
}

/// Create a tweened rotation animation
pub fn tween_rotation(from: f32, to: f32, duration: f32) -> TweenBuilder {
    TweenBuilder::new()
        .rotation(from, to)
        .duration(duration)
}

/// Create a tweened scale animation
pub fn tween_scale(from: f32, to: f32, duration: f32) -> TweenBuilder {
    TweenBuilder::new()
        .scale(from, to)
        .duration(duration)
}

/// Create a tweened alpha animation
pub fn tween_alpha(from: f32, to: f32, duration: f32) -> TweenBuilder {
    TweenBuilder::new()
        .alpha(from, to)
        .duration(duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tween_builder() {
        let tween = TweenBuilder::new()
            .scale(1.0, 2.0)
            .duration(0.5)
            .easing(EasingFunction::EaseOutQuad)
            .build();

        assert!(tween.is_some());
        let t = tween.unwrap();
        assert_eq!(t.target, TweenTarget::Scale);
        assert_eq!(t.start_value, 1.0);
        assert_eq!(t.end_value, 2.0);
        assert_eq!(t.duration, 0.5);
    }

    #[test]
    fn test_tween_progress() {
        let mut tween = Tween::new(TweenTarget::Alpha, 0.0, 1.0, 1.0);
        assert_eq!(tween.progress(), 0.0);

        tween.elapsed = 0.5;
        assert_eq!(tween.progress(), 0.5);

        tween.elapsed = 1.5;
        assert_eq!(tween.progress(), 1.0);
    }

    #[test]
    fn test_missing_target() {
        let tween = TweenBuilder::new()
            .duration(0.5)
            .build();

        assert!(tween.is_none());
    }
}
