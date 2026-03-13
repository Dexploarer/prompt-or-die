//! Keyframe system for animation tracks and clips

use crate::easing::EasingFunction;
use glam::Vec2;
use serde::{Deserialize, Serialize};

/// A keyframe at a specific time with a value and easing function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyframe<T: Clone> {
    pub time: f32,
    pub value: T,
    pub easing: EasingFunction,
}

impl<T: Clone> Keyframe<T> {
    pub fn new(time: f32, value: T, easing: EasingFunction) -> Self {
        Self {
            time,
            value,
            easing,
        }
    }
}

/// Values that can be animated
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum KeyframeValue {
    Float(f32),
    Int(u32),
    Bool(bool),
}

impl KeyframeValue {
    pub fn as_float(&self) -> f32 {
        match self {
            KeyframeValue::Float(v) => *v,
            KeyframeValue::Int(v) => *v as f32,
            KeyframeValue::Bool(v) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    pub fn interpolate(&self, other: &KeyframeValue, t: f32) -> KeyframeValue {
        match (self, other) {
            (KeyframeValue::Float(a), KeyframeValue::Float(b)) => {
                KeyframeValue::Float(a + (b - a) * t)
            }
            (KeyframeValue::Int(a), KeyframeValue::Int(b)) => {
                let result = (*a as f32 + (*b as f32 - *a as f32) * t) as u32;
                KeyframeValue::Int(result)
            }
            (KeyframeValue::Bool(a), KeyframeValue::Bool(b)) => {
                KeyframeValue::Bool(if t < 0.5 { *a } else { *b })
            }
            _ => *self,
        }
    }
}

/// An animation track holds ordered keyframes and interpolates between them
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationTrack<T: Clone> {
    keyframes: Vec<Keyframe<T>>,
}

impl<T: Clone> AnimationTrack<T> {
    pub fn new() -> Self {
        Self {
            keyframes: Vec::new(),
        }
    }

    pub fn add_keyframe(&mut self, keyframe: Keyframe<T>) {
        self.keyframes.push(keyframe);
        self.keyframes
            .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    }

    pub fn keyframes(&self) -> &[Keyframe<T>] {
        &self.keyframes
    }

    pub fn duration(&self) -> f32 {
        self.keyframes.last().map(|kf| kf.time).unwrap_or(0.0)
    }
}

/// Interpolate Vec2 values
impl AnimationTrack<Vec2> {
    pub fn sample(&self, time: f32) -> Option<Vec2> {
        if self.keyframes.is_empty() {
            return None;
        }

        let time = time.clamp(0.0, self.duration());

        // Find surrounding keyframes
        let mut prev_frame = None;
        let mut next_frame = None;

        for (i, kf) in self.keyframes.iter().enumerate() {
            if kf.time <= time {
                prev_frame = Some(i);
            }
            if kf.time >= time && next_frame.is_none() {
                next_frame = Some(i);
            }
        }

        match (prev_frame, next_frame) {
            (Some(prev_idx), Some(next_idx)) if prev_idx == next_idx => {
                Some(self.keyframes[prev_idx].value)
            }
            (Some(prev_idx), Some(next_idx)) => {
                let prev_kf = &self.keyframes[prev_idx];
                let next_kf = &self.keyframes[next_idx];

                let local_t = (time - prev_kf.time) / (next_kf.time - prev_kf.time);
                let eased_t = crate::ease(local_t, &prev_kf.easing);

                Some(prev_kf.value.lerp(next_kf.value, eased_t))
            }
            (Some(idx), None) => Some(self.keyframes[idx].value),
            (None, Some(idx)) => Some(self.keyframes[idx].value),
            (None, None) => None,
        }
    }
}

/// Interpolate float values
impl AnimationTrack<f32> {
    pub fn sample(&self, time: f32) -> Option<f32> {
        if self.keyframes.is_empty() {
            return None;
        }

        let time = time.clamp(0.0, self.duration());

        let mut prev_frame = None;
        let mut next_frame = None;

        for (i, kf) in self.keyframes.iter().enumerate() {
            if kf.time <= time {
                prev_frame = Some(i);
            }
            if kf.time >= time && next_frame.is_none() {
                next_frame = Some(i);
            }
        }

        match (prev_frame, next_frame) {
            (Some(prev_idx), Some(next_idx)) if prev_idx == next_idx => {
                Some(self.keyframes[prev_idx].value)
            }
            (Some(prev_idx), Some(next_idx)) => {
                let prev_kf = &self.keyframes[prev_idx];
                let next_kf = &self.keyframes[next_idx];

                let local_t = (time - prev_kf.time) / (next_kf.time - prev_kf.time);
                let eased_t = crate::ease(local_t, &prev_kf.easing);

                Some(prev_kf.value + (next_kf.value - prev_kf.value) * eased_t)
            }
            (Some(idx), None) => Some(self.keyframes[idx].value),
            (None, Some(idx)) => Some(self.keyframes[idx].value),
            (None, None) => None,
        }
    }
}

/// Interpolate u32 values (for sprite frames)
impl AnimationTrack<u32> {
    pub fn sample(&self, time: f32) -> Option<u32> {
        if self.keyframes.is_empty() {
            return None;
        }

        let time = time.clamp(0.0, self.duration());

        let mut prev_frame = None;
        let mut next_frame = None;

        for (i, kf) in self.keyframes.iter().enumerate() {
            if kf.time <= time {
                prev_frame = Some(i);
            }
            if kf.time >= time && next_frame.is_none() {
                next_frame = Some(i);
            }
        }

        match (prev_frame, next_frame) {
            (Some(prev_idx), Some(next_idx)) if prev_idx == next_idx => {
                Some(self.keyframes[prev_idx].value)
            }
            (Some(prev_idx), Some(next_idx)) => {
                let prev_kf = &self.keyframes[prev_idx];
                let next_kf = &self.keyframes[next_idx];

                let local_t = (time - prev_kf.time) / (next_kf.time - prev_kf.time);

                // Step interpolation for frame numbers (no easing)
                if local_t < 0.5 {
                    Some(prev_kf.value)
                } else {
                    Some(next_kf.value)
                }
            }
            (Some(idx), None) => Some(self.keyframes[idx].value),
            (None, Some(idx)) => Some(self.keyframes[idx].value),
            (None, None) => None,
        }
    }
}

/// Sampled values from an animation clip
#[derive(Debug, Clone, Default)]
pub struct SampledAnimationData {
    pub position: Option<Vec2>,
    pub rotation: Option<f32>,
    pub scale: Option<Vec2>,
    pub color: Option<[f32; 4]>,
    pub frame: Option<u32>,
}

/// Named collection of animation tracks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationClip {
    pub name: String,
    position_track: Option<AnimationTrack<Vec2>>,
    rotation_track: Option<AnimationTrack<f32>>,
    scale_track: Option<AnimationTrack<Vec2>>,
    color_track: Option<AnimationTrack<[f32; 4]>>,
    frame_track: Option<AnimationTrack<u32>>,
}

impl AnimationClip {
    pub fn new(name: String) -> Self {
        Self {
            name,
            position_track: None,
            rotation_track: None,
            scale_track: None,
            color_track: None,
            frame_track: None,
        }
    }

    pub fn with_position_track(mut self, track: AnimationTrack<Vec2>) -> Self {
        self.position_track = Some(track);
        self
    }

    pub fn with_rotation_track(mut self, track: AnimationTrack<f32>) -> Self {
        self.rotation_track = Some(track);
        self
    }

    pub fn with_scale_track(mut self, track: AnimationTrack<Vec2>) -> Self {
        self.scale_track = Some(track);
        self
    }

    pub fn with_color_track(mut self, track: AnimationTrack<[f32; 4]>) -> Self {
        self.color_track = Some(track);
        self
    }

    pub fn with_frame_track(mut self, track: AnimationTrack<u32>) -> Self {
        self.frame_track = Some(track);
        self
    }

    pub fn duration(&self) -> f32 {
        [
            self.position_track.as_ref().map(|t| t.duration()),
            self.rotation_track.as_ref().map(|t| t.duration()),
            self.scale_track.as_ref().map(|t| t.duration()),
            self.color_track.as_ref().map(|t| t.duration()),
            self.frame_track.as_ref().map(|t| t.duration()),
        ]
        .iter()
        .filter_map(|d| *d)
        .fold(0.0, |a, b| a.max(b))
    }

    pub fn sample(&self, time: f32) -> SampledAnimationData {
        SampledAnimationData {
            position: self.position_track.as_ref().and_then(|t| t.sample(time)),
            rotation: self.rotation_track.as_ref().and_then(|t| t.sample(time)),
            scale: self.scale_track.as_ref().and_then(|t| t.sample(time)),
            color: self.color_track.as_ref().and_then(|t| t.sample(time)),
            frame: self.frame_track.as_ref().and_then(|t| t.sample(time)),
        }
    }
}

/// How to handle RGBA color interpolation
impl AnimationTrack<[f32; 4]> {
    pub fn sample(&self, time: f32) -> Option<[f32; 4]> {
        if self.keyframes.is_empty() {
            return None;
        }

        let time = time.clamp(0.0, self.duration());

        let mut prev_frame = None;
        let mut next_frame = None;

        for (i, kf) in self.keyframes.iter().enumerate() {
            if kf.time <= time {
                prev_frame = Some(i);
            }
            if kf.time >= time && next_frame.is_none() {
                next_frame = Some(i);
            }
        }

        match (prev_frame, next_frame) {
            (Some(prev_idx), Some(next_idx)) if prev_idx == next_idx => {
                Some(self.keyframes[prev_idx].value)
            }
            (Some(prev_idx), Some(next_idx)) => {
                let prev_kf = &self.keyframes[prev_idx];
                let next_kf = &self.keyframes[next_idx];

                let local_t = (time - prev_kf.time) / (next_kf.time - prev_kf.time);
                let eased_t = crate::ease(local_t, &prev_kf.easing);

                let prev = prev_kf.value;
                let next = next_kf.value;

                Some([
                    prev[0] + (next[0] - prev[0]) * eased_t,
                    prev[1] + (next[1] - prev[1]) * eased_t,
                    prev[2] + (next[2] - prev[2]) * eased_t,
                    prev[3] + (next[3] - prev[3]) * eased_t,
                ])
            }
            (Some(idx), None) => Some(self.keyframes[idx].value),
            (None, Some(idx)) => Some(self.keyframes[idx].value),
            (None, None) => None,
        }
    }
}

/// Playback state for animation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimPlayback {
    Playing,
    Paused,
    Stopped,
}

/// Component that plays animation clips on an entity
#[derive(Debug, Clone)]
pub struct AnimationPlayer {
    pub current_clip: Option<AnimationClip>,
    pub state: AnimPlayback,
    pub paused: bool,
    pub elapsed: f32,
    pub speed: f32,
    pub looping: bool,
}

impl AnimationPlayer {
    pub fn new() -> Self {
        Self {
            current_clip: None,
            state: AnimPlayback::Stopped,
            paused: false,
            elapsed: 0.0,
            speed: 1.0,
            looping: false,
        }
    }

    pub fn play(&mut self, clip: AnimationClip) {
        self.current_clip = Some(clip);
        self.state = AnimPlayback::Playing;
        self.paused = false;
        self.elapsed = 0.0;
    }

    pub fn pause(&mut self) {
        if self.state == AnimPlayback::Playing {
            self.paused = true;
        }
    }

    pub fn resume(&mut self) {
        if self.state == AnimPlayback::Playing {
            self.paused = false;
        }
    }

    pub fn stop(&mut self) {
        self.state = AnimPlayback::Stopped;
        self.elapsed = 0.0;
    }

    pub fn is_playing(&self) -> bool {
        self.state == AnimPlayback::Playing
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }

    pub fn get_duration(&self) -> f32 {
        self.current_clip
            .as_ref()
            .map(|c| c.duration())
            .unwrap_or(0.0)
    }
}

impl Default for AnimationPlayer {
    fn default() -> Self {
        Self::new()
    }
}
