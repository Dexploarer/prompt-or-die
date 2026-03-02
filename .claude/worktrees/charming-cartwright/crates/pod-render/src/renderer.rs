//! Platform-agnostic render state extraction and management

use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Type of drawable entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DrawType {
    /// Colored rectangle
    Rect {
        width: f32,
        height: f32,
        color: [f32; 4],
    },
    /// Textured sprite
    Sprite {
        texture: String,
        frame: u32,
        tint: [f32; 4],
    },
}

/// Single renderable item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderItem {
    pub position: Vec2,
    pub rotation: f32,
    pub scale: Vec2,
    pub layer: i32,
    pub draw_type: DrawType,
    pub visible: bool,
}

/// Complete render state for a frame
/// Sorted by layer for correct draw order
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderState {
    pub items: Vec<RenderItem>,
}

impl RenderState {
    /// Create a new empty render state
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
        }
    }

    /// Add a render item
    pub fn add_item(&mut self, item: RenderItem) {
        self.items.push(item);
    }

    /// Clear all items
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Sort by layer for correct draw order
    pub fn sort_by_layer(&mut self) {
        self.items.sort_by(|a, b| {
            match a.layer.cmp(&b.layer) {
                Ordering::Equal => {
                    // Secondary sort: position.y (back to front)
                    a.position.y.partial_cmp(&b.position.y).unwrap_or(Ordering::Equal)
                }
                other => other,
            }
        });
    }
}

/// Extract render state from hecs ECS world
pub fn extract_render_state(world: &hecs::World) -> RenderState {
    use pod_core::{Transform, Sprite, ColorRect};

    let mut state = RenderState::new();

    // Query all entities with Transform + Sprite
    for (_, (transform, sprite)) in world
        .query::<(&Transform, &Sprite)>()
        .iter()
    {
        if !sprite.visible {
            continue;
        }

        let item = RenderItem {
            position: transform.position,
            rotation: transform.rotation,
            scale: transform.scale,
            layer: sprite.layer,
            draw_type: DrawType::Sprite {
                texture: sprite.texture.clone(),
                frame: sprite.frame,
                tint: sprite.color,
            },
            visible: sprite.visible,
        };
        state.add_item(item);
    }

    for (_, (transform, color_rect)) in world
        .query::<(&Transform, &ColorRect)>()
        .iter()
    {
        let item = RenderItem {
            position: transform.position,
            rotation: transform.rotation,
            scale: transform.scale,
            layer: color_rect.layer,
            draw_type: DrawType::Rect {
                width: color_rect.width,
                height: color_rect.height,
                color: color_rect.color,
            },
            visible: true,
        };
        state.add_item(item);
    }

    state.sort_by_layer();
    state
}
