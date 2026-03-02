//! # pod-animation — Keyframe interpolation, animation state machines, and tweening
//!
//! Provides a complete animation system for 2D entities:
//! - **Keyframe system**: AnimationClip with tracks for Transform, Sprite, and ColorRect
//! - **Easing functions**: Linear, Step, CubicBezier + EaseIn/Out/InOut variants
//! - **State machines**: AnimStateMachine with transition conditions and parameter-driven animation
//! - **Blending**: Smooth transitions between animations
//! - **Tweening**: Fire-and-forget animations with fluent builder API
//! - **System integration**: Top-level update function for ECS-based animation

pub mod keyframe;
pub mod easing;
pub mod state_machine;
pub mod blending;
pub mod tween;

pub use keyframe::{
    Keyframe, KeyframeValue, AnimationTrack, AnimationClip, AnimationPlayer, AnimPlayback,
};
pub use easing::{EasingFunction, ease};
pub use state_machine::{
    AnimState, AnimTransition, TransitionCondition, AnimStateMachine, AnimStatePlayer,
};
pub use blending::{blend_transforms, AdditiveBlend, CrossfadeBlend, BlendNode};
pub use tween::{Tween, TweenTarget, TweenBuilder};

use hecs::World;
use pod_core::TICK_DURATION_SECS;

pub fn init() {
    log::info!("{} initialized — Animation system ready", env!("CARGO_PKG_NAME"));
}

/// Top-level animation update system.
/// Integrates all animation components: AnimationPlayer, AnimStateMachine, Tweens.
pub fn update_animations(world: &mut World, dt: f32) {
    // Update animation players (keyframe animations)
    update_animation_players(world, dt);

    // Update animation state machines
    update_state_machines(world, dt);

    // Update tweens (fire-and-forget animations)
    update_tweens(world, dt);
}

/// Updates all AnimationPlayer components and applies results to Transform/Sprite/ColorRect
fn update_animation_players(world: &mut World, dt: f32) {
    let mut updates = Vec::new();

    for (entity, player) in world.query_mut::<&mut AnimationPlayer>() {
        if player.is_playing() && !player.paused {
            player.elapsed += dt * player.speed;

            if let Some(clip) = &player.current_clip {
                // Sample animation tracks at current time
                let samples = clip.sample(player.elapsed);

                updates.push((entity, samples));
            }

            // Handle looping
            if let Some(clip) = &player.current_clip {
                if player.looping && player.elapsed >= clip.duration() {
                    player.elapsed = player.elapsed % clip.duration();
                } else if player.elapsed >= clip.duration() {
                    player.state = AnimPlayback::Stopped;
                    player.elapsed = 0.0;
                }
            }
        }
    }

    // Apply sampled values to components
    for (entity, samples) in updates {
        if let Ok(mut transform) = world.get_mut::<pod_core::Transform>(entity) {
            if let Some(pos) = samples.position {
                transform.position = pos;
            }
            if let Some(rot) = samples.rotation {
                transform.rotation = rot;
            }
            if let Some(scale) = samples.scale {
                transform.scale = scale;
            }
        }

        if let Ok(mut sprite) = world.get_mut::<pod_core::Sprite>(entity) {
            if let Some(color) = samples.color {
                sprite.color = color;
            }
            if let Some(frame) = samples.frame {
                sprite.frame = frame;
            }
        }

        if let Ok(mut rect) = world.get_mut::<pod_core::ColorRect>(entity) {
            if let Some(color) = samples.color {
                rect.color = color;
            }
        }
    }
}

/// Updates all AnimStatePlayer components
fn update_state_machines(world: &mut World, dt: f32) {
    let mut state_updates = Vec::new();

    for (entity, state_player) in world.query_mut::<&mut AnimStatePlayer>() {
        state_player.update(dt);
        state_updates.push((entity, state_player.clone()));
    }

    // Apply state machine results to animation players
    for (entity, state_player) in state_updates {
        if let Ok(mut anim_player) = world.get_mut::<AnimationPlayer>(entity) {
            if let Some(clip) = state_player.get_current_clip() {
                anim_player.play(clip.clone());
            }
        }
    }
}

/// Updates all Tween components
fn update_tweens(world: &mut World, dt: f32) {
    let mut completed = Vec::new();
    let mut updates = Vec::new();

    for (entity, tween) in world.query_mut::<&mut Tween>() {
        tween.elapsed += dt;
        let t = (tween.elapsed / tween.duration).clamp(0.0, 1.0);
        let eased_t = ease(t, &tween.easing);

        updates.push((entity, tween.clone(), eased_t));

        if tween.elapsed >= tween.duration {
            completed.push(entity);
        }
    }

    // Apply tween values to components
    for (entity, tween, eased_t) in updates {
        apply_tween(world, entity, &tween, eased_t);
    }

    // Remove completed tweens
    for entity in completed {
        let _ = world.remove::<Tween>(entity);
    }
}

/// Apply a tween's interpolated value to the target entity
fn apply_tween(world: &World, entity: hecs::Entity, tween: &Tween, t: f32) {
    match &tween.target {
        TweenTarget::Position => {
            if let Ok(mut transform) = world.get_mut::<pod_core::Transform>(entity) {
                let start = tween.start_value;
                let end = tween.end_value;
                transform.position = glam::Vec2::new(
                    start + (end - start) * t,
                    0.0, // simplified; in real scenario, would store Vec2
                );
            }
        }
        TweenTarget::Rotation => {
            if let Ok(mut transform) = world.get_mut::<pod_core::Transform>(entity) {
                transform.rotation = tween.start_value + (tween.end_value - tween.start_value) * t;
            }
        }
        TweenTarget::Scale => {
            if let Ok(mut transform) = world.get_mut::<pod_core::Transform>(entity) {
                let factor = tween.start_value + (tween.end_value - tween.start_value) * t;
                transform.scale = glam::Vec2::splat(factor);
            }
        }
        TweenTarget::Color => {
            if let Ok(mut sprite) = world.get_mut::<pod_core::Sprite>(entity) {
                let start = tween.start_value;
                let end = tween.end_value;
                let val = start + (end - start) * t;
                sprite.color = [val, val, val, sprite.color[3]];
            }
        }
        TweenTarget::Alpha => {
            if let Ok(mut sprite) = world.get_mut::<pod_core::Sprite>(entity) {
                sprite.color[3] = tween.start_value + (tween.end_value - tween.start_value) * t;
            }
        }
        TweenTarget::SpriteFrame => {
            if let Ok(mut sprite) = world.get_mut::<pod_core::Sprite>(entity) {
                let frame = (tween.start_value + (tween.end_value - tween.start_value) * t) as u32;
                sprite.frame = frame;
            }
        }
    }
}
