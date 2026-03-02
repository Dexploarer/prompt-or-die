//! Animation blending — smooth transitions between animations

use serde::{Deserialize, Serialize};
use crate::keyframe::AnimationClip;

/// Blend two Transform values using linear interpolation
pub fn blend_transforms(
    a: &pod_core::Transform,
    b: &pod_core::Transform,
    weight: f32,
) -> pod_core::Transform {
    let weight = weight.clamp(0.0, 1.0);
    pod_core::Transform {
        position: a.position.lerp(b.position, weight),
        rotation: a.rotation + (b.rotation - a.rotation) * weight,
        scale: a.scale.lerp(b.scale, weight),
    }
}

/// Blend two color values (RGBA)
pub fn blend_colors(a: [f32; 4], b: [f32; 4], weight: f32) -> [f32; 4] {
    let weight = weight.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * weight,
        a[1] + (b[1] - a[1]) * weight,
        a[2] + (b[2] - a[2]) * weight,
        a[3] + (b[3] - a[3]) * weight,
    ]
}

/// Additive blending — layer animations on top of a base value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdditiveBlend {
    pub base_value: f32,
    pub additive_value: f32,
    pub strength: f32,
}

impl AdditiveBlend {
    pub fn new(base_value: f32, additive_value: f32, strength: f32) -> Self {
        Self {
            base_value,
            additive_value,
            strength: strength.clamp(0.0, 1.0),
        }
    }

    pub fn evaluate(&self) -> f32 {
        let delta = self.additive_value - self.base_value;
        self.base_value + delta * self.strength
    }
}

/// Crossfade blending — smooth transition between two clips
#[derive(Debug, Clone)]
pub struct CrossfadeBlend {
    pub from_clip: AnimationClip,
    pub to_clip: AnimationClip,
    pub duration: f32,
    pub elapsed: f32,
}

impl CrossfadeBlend {
    pub fn new(
        from_clip: AnimationClip,
        to_clip: AnimationClip,
        duration: f32,
    ) -> Self {
        Self {
            from_clip,
            to_clip,
            duration: duration.max(0.001),
            elapsed: 0.0,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.elapsed += dt;
    }

    pub fn weight(&self) -> f32 {
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    pub fn is_done(&self) -> bool {
        self.elapsed >= self.duration
    }

    pub fn sample(&self, time: f32) -> crate::keyframe::SampledAnimationData {
        let from_sample = self.from_clip.sample(time);
        let to_sample = self.to_clip.sample(time);
        let w = self.weight();

        let mut result = crate::keyframe::SampledAnimationData::default();

        if let (Some(from_pos), Some(to_pos)) = (from_sample.position, to_sample.position) {
            result.position = Some(from_pos.lerp(to_pos, w));
        } else if let Some(pos) = from_sample.position {
            result.position = Some(pos.lerp(to_sample.position.unwrap_or(pos), w));
        } else if let Some(pos) = to_sample.position {
            result.position = Some(pos);
        }

        if let (Some(from_rot), Some(to_rot)) = (from_sample.rotation, to_sample.rotation) {
            result.rotation = Some(from_rot + (to_rot - from_rot) * w);
        } else if let Some(rot) = from_sample.rotation {
            result.rotation = Some(rot);
        } else if let Some(rot) = to_sample.rotation {
            result.rotation = Some(rot);
        }

        if let (Some(from_scale), Some(to_scale)) = (from_sample.scale, to_sample.scale) {
            result.scale = Some(from_scale.lerp(to_scale, w));
        } else if let Some(scale) = from_sample.scale {
            result.scale = Some(scale);
        } else if let Some(scale) = to_sample.scale {
            result.scale = Some(scale);
        }

        if let (Some(from_color), Some(to_color)) = (from_sample.color, to_sample.color) {
            result.color = Some(blend_colors(from_color, to_color, w));
        } else if let Some(color) = from_sample.color {
            result.color = Some(color);
        } else if let Some(color) = to_sample.color {
            result.color = Some(color);
        }

        if let (Some(from_frame), Some(to_frame)) = (from_sample.frame, to_sample.frame) {
            result.frame = Some(if w < 0.5 { from_frame } else { to_frame });
        } else if let Some(frame) = from_sample.frame {
            result.frame = Some(frame);
        } else if let Some(frame) = to_sample.frame {
            result.frame = Some(frame);
        }

        result
    }
}

/// 1D blend space for smooth transitions between animations based on a single parameter
#[derive(Debug, Clone)]
pub struct Blend1D {
    pub parameter_name: String,
    pub values: Vec<(f32, AnimationClip)>,
}

impl Blend1D {
    pub fn new(parameter_name: String) -> Self {
        Self {
            parameter_name,
            values: Vec::new(),
        }
    }

    pub fn add_clip(mut self, threshold: f32, clip: AnimationClip) -> Self {
        self.values.push((threshold, clip));
        self.values.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        self
    }

    pub fn sample(&self, param_value: f32, time: f32) -> crate::keyframe::SampledAnimationData {
        if self.values.is_empty() {
            return crate::keyframe::SampledAnimationData::default();
        }

        if self.values.len() == 1 {
            return self.values[0].1.sample(time);
        }

        // Find surrounding clips
        let mut prev_idx = 0;
        let mut next_idx = 0;

        for (i, (threshold, _)) in self.values.iter().enumerate() {
            if *threshold <= param_value {
                prev_idx = i;
            }
            if *threshold >= param_value && i > 0 {
                next_idx = i;
                break;
            }
        }

        if prev_idx == next_idx {
            return self.values[prev_idx].1.sample(time);
        }

        let prev_threshold = self.values[prev_idx].0;
        let next_threshold = self.values[next_idx].0;
        let range = next_threshold - prev_threshold;

        let blend_weight = if range.abs() < 0.001 {
            0.0
        } else {
            (param_value - prev_threshold) / range
        };

        let prev_sample = self.values[prev_idx].1.sample(time);
        let next_sample = self.values[next_idx].1.sample(time);

        blend_samples(prev_sample, next_sample, blend_weight)
    }
}

/// 2D blend space for smooth transitions based on two parameters
#[derive(Debug, Clone)]
pub struct Blend2D {
    pub param_x_name: String,
    pub param_y_name: String,
    pub clips: Vec<(f32, f32, AnimationClip)>,
}

impl Blend2D {
    pub fn new(param_x_name: String, param_y_name: String) -> Self {
        Self {
            param_x_name,
            param_y_name,
            clips: Vec::new(),
        }
    }

    pub fn add_clip(mut self, x: f32, y: f32, clip: AnimationClip) -> Self {
        self.clips.push((x, y, clip));
        self
    }

    pub fn sample(&self, x: f32, y: f32, time: f32) -> crate::keyframe::SampledAnimationData {
        if self.clips.is_empty() {
            return crate::keyframe::SampledAnimationData::default();
        }

        if self.clips.len() == 1 {
            return self.clips[0].2.sample(time);
        }

        // Simple bilinear interpolation between 4 closest points
        let mut closest = vec![];

        for (cx, cy, clip) in &self.clips {
            let dist = (cx - x) * (cx - x) + (cy - y) * (cy - y);
            closest.push((dist, (*cx, *cy, clip.clone())));
        }

        closest.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        if closest.len() == 1 {
            return closest[0].1.2.sample(time);
        }

        let (_, (x0, y0, clip0)) = &closest[0];
        let (_, (x1, y1, clip1)) = &closest[1];

        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let total_dist = (dx * dx + dy * dy).sqrt();

        let blend_weight = if total_dist < 0.001 {
            0.5
        } else {
            let point_dist = ((x - x0) * (x - x0) + (y - y0) * (y - y0)).sqrt();
            (point_dist / total_dist).clamp(0.0, 1.0)
        };

        let sample0 = clip0.sample(time);
        let sample1 = clip1.sample(time);

        blend_samples(sample0, sample1, blend_weight)
    }
}

/// Blend tree nodes for complex animation blending
#[derive(Debug, Clone)]
pub enum BlendNode {
    Clip(AnimationClip),
    Blend1D(Blend1D),
    Blend2D(Blend2D),
}

impl BlendNode {
    pub fn sample(
        &self,
        time: f32,
        param_x: Option<f32>,
        param_y: Option<f32>,
    ) -> crate::keyframe::SampledAnimationData {
        match self {
            BlendNode::Clip(clip) => clip.sample(time),
            BlendNode::Blend1D(blend) => blend.sample(param_x.unwrap_or(0.0), time),
            BlendNode::Blend2D(blend) => {
                blend.sample(param_x.unwrap_or(0.0), param_y.unwrap_or(0.0), time)
            }
        }
    }
}

// ============================================================
// Helper functions
// ============================================================

fn blend_samples(
    a: crate::keyframe::SampledAnimationData,
    b: crate::keyframe::SampledAnimationData,
    weight: f32,
) -> crate::keyframe::SampledAnimationData {
    let mut result = crate::keyframe::SampledAnimationData::default();

    if let (Some(pos_a), Some(pos_b)) = (a.position, b.position) {
        result.position = Some(pos_a.lerp(pos_b, weight));
    } else if let Some(pos) = a.position {
        result.position = Some(pos);
    } else if let Some(pos) = b.position {
        result.position = Some(pos);
    }

    if let (Some(rot_a), Some(rot_b)) = (a.rotation, b.rotation) {
        result.rotation = Some(rot_a + (rot_b - rot_a) * weight);
    } else if let Some(rot) = a.rotation {
        result.rotation = Some(rot);
    } else if let Some(rot) = b.rotation {
        result.rotation = Some(rot);
    }

    if let (Some(scale_a), Some(scale_b)) = (a.scale, b.scale) {
        result.scale = Some(scale_a.lerp(scale_b, weight));
    } else if let Some(scale) = a.scale {
        result.scale = Some(scale);
    } else if let Some(scale) = b.scale {
        result.scale = Some(scale);
    }

    if let (Some(color_a), Some(color_b)) = (a.color, b.color) {
        result.color = Some(blend_colors(color_a, color_b, weight));
    } else if let Some(color) = a.color {
        result.color = Some(color);
    } else if let Some(color) = b.color {
        result.color = Some(color);
    }

    if let (Some(frame_a), Some(frame_b)) = (a.frame, b.frame) {
        result.frame = Some(if weight < 0.5 { frame_a } else { frame_b });
    } else if let Some(frame) = a.frame {
        result.frame = Some(frame);
    } else if let Some(frame) = b.frame {
        result.frame = Some(frame);
    }

    result
}
