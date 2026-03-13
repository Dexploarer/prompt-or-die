//! Easing functions for smooth animation interpolation
//!
//! All easing functions work with normalized time t in range [0, 1]
//! where 0 is start and 1 is end of the animation.

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Easing function types for animation interpolation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EasingFunction {
    Linear,
    Step,
    CubicBezier { p0: f32, p1: f32, p2: f32, p3: f32 },

    // Ease In (accelerating from zero velocity)
    EaseInQuad,
    EaseInCubic,
    EaseInQuart,
    EaseInQuint,
    EaseInSine,
    EaseInExpo,
    EaseInCirc,
    EaseInBack,
    EaseInElastic,
    EaseInBounce,

    // Ease Out (decelerating to zero velocity)
    EaseOutQuad,
    EaseOutCubic,
    EaseOutQuart,
    EaseOutQuint,
    EaseOutSine,
    EaseOutExpo,
    EaseOutCirc,
    EaseOutBack,
    EaseOutElastic,
    EaseOutBounce,

    // Ease In-Out (accelerating and decelerating)
    EaseInOutQuad,
    EaseInOutCubic,
    EaseInOutQuart,
    EaseInOutQuint,
    EaseInOutSine,
    EaseInOutExpo,
    EaseInOutCirc,
    EaseInOutBack,
    EaseInOutElastic,
    EaseInOutBounce,
}

impl Default for EasingFunction {
    fn default() -> Self {
        EasingFunction::Linear
    }
}

/// Evaluate an easing function at time t (0..1)
pub fn ease(t: f32, easing: &EasingFunction) -> f32 {
    let t = t.clamp(0.0, 1.0);

    match easing {
        EasingFunction::Linear => t,
        EasingFunction::Step => {
            if t < 0.5 {
                0.0
            } else {
                1.0
            }
        }
        EasingFunction::CubicBezier { p0, p1, p2, p3 } => cubic_bezier(t, *p0, *p1, *p2, *p3),

        // Quad
        EasingFunction::EaseInQuad => ease_in_quad(t),
        EasingFunction::EaseOutQuad => ease_out_quad(t),
        EasingFunction::EaseInOutQuad => ease_in_out_quad(t),

        // Cubic
        EasingFunction::EaseInCubic => ease_in_cubic(t),
        EasingFunction::EaseOutCubic => ease_out_cubic(t),
        EasingFunction::EaseInOutCubic => ease_in_out_cubic(t),

        // Quart
        EasingFunction::EaseInQuart => ease_in_quart(t),
        EasingFunction::EaseOutQuart => ease_out_quart(t),
        EasingFunction::EaseInOutQuart => ease_in_out_quart(t),

        // Quint
        EasingFunction::EaseInQuint => ease_in_quint(t),
        EasingFunction::EaseOutQuint => ease_out_quint(t),
        EasingFunction::EaseInOutQuint => ease_in_out_quint(t),

        // Sine
        EasingFunction::EaseInSine => ease_in_sine(t),
        EasingFunction::EaseOutSine => ease_out_sine(t),
        EasingFunction::EaseInOutSine => ease_in_out_sine(t),

        // Expo
        EasingFunction::EaseInExpo => ease_in_expo(t),
        EasingFunction::EaseOutExpo => ease_out_expo(t),
        EasingFunction::EaseInOutExpo => ease_in_out_expo(t),

        // Circ
        EasingFunction::EaseInCirc => ease_in_circ(t),
        EasingFunction::EaseOutCirc => ease_out_circ(t),
        EasingFunction::EaseInOutCirc => ease_in_out_circ(t),

        // Back
        EasingFunction::EaseInBack => ease_in_back(t),
        EasingFunction::EaseOutBack => ease_out_back(t),
        EasingFunction::EaseInOutBack => ease_in_out_back(t),

        // Elastic
        EasingFunction::EaseInElastic => ease_in_elastic(t),
        EasingFunction::EaseOutElastic => ease_out_elastic(t),
        EasingFunction::EaseInOutElastic => ease_in_out_elastic(t),

        // Bounce
        EasingFunction::EaseInBounce => ease_in_bounce(t),
        EasingFunction::EaseOutBounce => ease_out_bounce(t),
        EasingFunction::EaseInOutBounce => ease_in_out_bounce(t),
    }
}

// ============================================================
// Quad (power of 2)
// ============================================================

fn ease_in_quad(t: f32) -> f32 {
    t * t
}

fn ease_out_quad(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

fn ease_in_out_quad(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

// ============================================================
// Cubic (power of 3)
// ============================================================

fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

// ============================================================
// Quart (power of 4)
// ============================================================

fn ease_in_quart(t: f32) -> f32 {
    t * t * t * t
}

fn ease_out_quart(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(4)
}

fn ease_in_out_quart(t: f32) -> f32 {
    if t < 0.5 {
        8.0 * t * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
    }
}

// ============================================================
// Quint (power of 5)
// ============================================================

fn ease_in_quint(t: f32) -> f32 {
    t * t * t * t * t
}

fn ease_out_quint(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(5)
}

fn ease_in_out_quint(t: f32) -> f32 {
    if t < 0.5 {
        16.0 * t * t * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(5) / 2.0
    }
}

// ============================================================
// Sine
// ============================================================

fn ease_in_sine(t: f32) -> f32 {
    1.0 - (t * PI / 2.0).cos()
}

fn ease_out_sine(t: f32) -> f32 {
    (t * PI / 2.0).sin()
}

fn ease_in_out_sine(t: f32) -> f32 {
    -(((PI * t).cos()) - 1.0) / 2.0
}

// ============================================================
// Expo (exponential)
// ============================================================

fn ease_in_expo(t: f32) -> f32 {
    if t == 0.0 {
        0.0
    } else {
        2.0_f32.powf(10.0 * t - 10.0)
    }
}

fn ease_out_expo(t: f32) -> f32 {
    if t == 1.0 {
        1.0
    } else {
        1.0 - 2.0_f32.powf(-10.0 * t)
    }
}

fn ease_in_out_expo(t: f32) -> f32 {
    if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else if t < 0.5 {
        2.0_f32.powf(20.0 * t - 10.0) / 2.0
    } else {
        (2.0 - 2.0_f32.powf(-20.0 * t + 10.0)) / 2.0
    }
}

// ============================================================
// Circ (circular)
// ============================================================

fn ease_in_circ(t: f32) -> f32 {
    1.0 - (1.0 - t * t).sqrt()
}

fn ease_out_circ(t: f32) -> f32 {
    (1.0 - (t - 1.0) * (t - 1.0)).sqrt()
}

fn ease_in_out_circ(t: f32) -> f32 {
    if t < 0.5 {
        (1.0 - (1.0 - (2.0 * t).powi(2)).sqrt()) / 2.0
    } else {
        ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0) / 2.0
    }
}

// ============================================================
// Back (slight overshoot)
// ============================================================

const C1: f32 = 1.70158;
const C2: f32 = C1 + 1.0;

fn ease_in_back(t: f32) -> f32 {
    C2 * t * t * t - C1 * t * t
}

fn ease_out_back(t: f32) -> f32 {
    1.0 + C2 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
}

fn ease_in_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c2 = c1 * 1.525;

    if t < 0.5 {
        ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0
    } else {
        ((2.0 * t - 2.0).powi(2) * ((c2 + 1.0) * (t * 2.0 - 2.0) + c2) + 2.0) / 2.0
    }
}

// ============================================================
// Elastic (spring-like oscillation)
// ============================================================

const C3: f32 = (2.0 * PI) / 3.0;
const C4: f32 = (2.0 * PI) / 4.5;

fn ease_in_elastic(t: f32) -> f32 {
    if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else {
        -2.0_f32.powf(10.0 * t - 10.0) * ((t * 10.0 - 10.75) * C3).sin()
    }
}

fn ease_out_elastic(t: f32) -> f32 {
    if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else {
        2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * C4).sin() + 1.0
    }
}

fn ease_in_out_elastic(t: f32) -> f32 {
    if t == 0.0 {
        0.0
    } else if t == 1.0 {
        1.0
    } else if t < 0.5 {
        -(2.0_f32.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * C4).sin()) / 2.0
    } else {
        (2.0_f32.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * C4).sin()) / 2.0 + 1.0
    }
}

// ============================================================
// Bounce (bouncy, spring-like)
// ============================================================

fn ease_out_bounce(t: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;

    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        N1 * (t - 1.5 / D1) * (t - 1.5 / D1) + 0.75
    } else if t < 2.5 / D1 {
        N1 * (t - 2.25 / D1) * (t - 2.25 / D1) + 0.9375
    } else {
        N1 * (t - 2.625 / D1) * (t - 2.625 / D1) + 0.984375
    }
}

fn ease_in_bounce(t: f32) -> f32 {
    1.0 - ease_out_bounce(1.0 - t)
}

fn ease_in_out_bounce(t: f32) -> f32 {
    if t < 0.5 {
        (1.0 - ease_out_bounce(1.0 - 2.0 * t)) / 2.0
    } else {
        (1.0 + ease_out_bounce(2.0 * t - 1.0)) / 2.0
    }
}

// ============================================================
// Cubic Bezier
// ============================================================

/// Evaluate a cubic Bezier curve at parameter t (0..1)
/// Uses iterative method (Newton-Raphson for finding t)
fn cubic_bezier(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    // De Casteljau's algorithm
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;
    let t2 = t * t;
    let t3 = t2 * t;

    mt3 * p0 + 3.0 * mt2 * t * p1 + 3.0 * mt * t2 * p2 + t3 * p3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_easing_boundaries() {
        let easings = [
            EasingFunction::Linear,
            EasingFunction::EaseInQuad,
            EasingFunction::EaseOutQuad,
            EasingFunction::EaseInOutQuad,
            EasingFunction::EaseInSine,
            EasingFunction::EaseOutSine,
            EasingFunction::EaseInCubic,
            EasingFunction::EaseOutCubic,
        ];

        for easing in &easings {
            let start = ease(0.0, easing);
            let end = ease(1.0, easing);
            assert!((start - 0.0).abs() < 0.001, "Start value should be ~0");
            assert!((end - 1.0).abs() < 0.001, "End value should be ~1");
        }
    }

    #[test]
    fn test_linear_easing() {
        assert_eq!(ease(0.0, &EasingFunction::Linear), 0.0);
        assert_eq!(ease(0.5, &EasingFunction::Linear), 0.5);
        assert_eq!(ease(1.0, &EasingFunction::Linear), 1.0);
    }
}
