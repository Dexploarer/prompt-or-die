//! Platform-agnostic render state extraction and management

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    /// 2.5D world-space sprite in the depth-aware 3D pass
    Sprite3D {
        texture: String,
        frame: u32,
        tint: [f32; 4],
        transform: RenderTransform3D,
        billboard: bool,
    },
    Mesh3D {
        mesh: String,
        material: String,
        transform: RenderTransform3D,
        cast_shadows: bool,
        receive_shadows: bool,
    },
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct RenderTransform3D {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl From<&pod_core::Transform3D> for RenderTransform3D {
    fn from(transform: &pod_core::Transform3D) -> Self {
        Self {
            position: transform.position.to_array(),
            rotation: transform.rotation,
            scale: transform.scale.to_array(),
        }
    }
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
                    // Secondary sort:
                    // 2D items use y as depth proxy,
                    // 3D items use world z for depth ordering.
                    a.sort_key().partial_cmp(&b.sort_key()).unwrap_or(Ordering::Equal)
                }
                other => other,
            }
        });
    }
}

impl RenderItem {
    fn sort_key(&self) -> f32 {
        match &self.draw_type {
            DrawType::Mesh3D { transform, .. } => transform.position[2],
            DrawType::Sprite3D { transform, .. } => transform.position[2],
            _ => self.position.y,
        }
    }
}

/// Extract render state from hecs ECS world
pub fn extract_render_state(world: &hecs::World) -> RenderState {
    use pod_core::{
        ColorRect, Material, Mesh, Parent3D, Sprite, Transform, Transform3D,
    };

    let mut state = RenderState::new();
    let mut transform_graph = HashMap::<u32, pod_core::Transform3D>::new();
    let mut parent_map = HashMap::<u32, u32>::new();
    let sentinel = u64::MAX;

    for (entity, (transform,)) in world.query::<(&Transform3D,)>().iter() {
        let id = entity.id();
        transform_graph.insert(id, *transform);
        if let Ok(parent) = world.get::<&Parent3D>(entity) {
            if parent.parent != sentinel {
                parent_map.insert(id, parent.parent as u32);
            }
        }
    }

    let mut resolved_cache = HashMap::new();
    let mut transform_color_rects = HashSet::new();

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

    for (entity, (transform, color_rect)) in world
        .query::<(&Transform, &ColorRect)>()
        .iter()
    {
        transform_color_rects.insert(entity.id());
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

    for (entity, color_rect) in world.query::<&ColorRect>().iter() {
        if transform_color_rects.contains(&entity.id()) {
            continue;
        }

        let item = RenderItem {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
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

    for (entity, (_transform3d, mesh, material)) in world
        .query::<(&Transform3D, &Mesh, &Material)>()
        .iter()
    {
        if !mesh.visible || !material.visible {
            continue;
        }
        let world_transform = resolve_world_transform(
            entity.id(),
            &transform_graph,
            &parent_map,
            &mut resolved_cache,
        );

        let item = RenderItem {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            layer: mesh.layer,
            draw_type: DrawType::Mesh3D {
                mesh: mesh.asset_id.clone(),
                material: material.asset_id.clone(),
                transform: world_transform,
                cast_shadows: mesh.cast_shadows,
                receive_shadows: mesh.receive_shadows,
            },
            visible: true,
        };
        state.add_item(item);
    }

    // Query entities with Transform3D + Sprite for 2.5D pseudo-depth rendering
    for (entity, (_transform3d, sprite)) in world
        .query::<(&Transform3D, &Sprite)>()
        .iter()
    {
        if !sprite.visible {
            continue;
        }
        let world_transform = resolve_world_transform(
            entity.id(),
            &transform_graph,
            &parent_map,
            &mut resolved_cache,
        );

        let item = RenderItem {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            layer: sprite.layer,
            draw_type: DrawType::Sprite3D {
                texture: sprite.texture.clone(),
                frame: sprite.frame,
                tint: sprite.color,
                transform: world_transform,
                billboard: true,
            },
            visible: true,
        };
        state.add_item(item);
    }

    state.sort_by_layer();
    state
}

fn resolve_world_transform(
    entity_id: u32,
    transform_graph: &HashMap<u32, pod_core::Transform3D>,
    parent_map: &HashMap<u32, u32>,
    resolved_cache: &mut HashMap<u32, RenderTransform3D>,
) -> RenderTransform3D {
    if let Some(cached) = resolved_cache.get(&entity_id) {
        return *cached;
    }

    let mut visiting = HashSet::new();
    let resolved = resolve_world_transform_recursive(
        entity_id,
        transform_graph,
        parent_map,
        &mut visiting,
        resolved_cache,
    );

    resolved_cache.insert(entity_id, resolved);
    resolved
}

fn resolve_world_transform_recursive(
    entity_id: u32,
    transform_graph: &HashMap<u32, pod_core::Transform3D>,
    parent_map: &HashMap<u32, u32>,
    visiting: &mut HashSet<u32>,
    resolved_cache: &mut HashMap<u32, RenderTransform3D>,
) -> RenderTransform3D {
    if let Some(cached) = resolved_cache.get(&entity_id) {
        return *cached;
    }

    if !visiting.insert(entity_id) {
        if let Some(local) = transform_graph.get(&entity_id) {
            return RenderTransform3D::from(local);
        }
        return RenderTransform3D::from(&pod_core::Transform3D::default());
    }

    let local_transform = match transform_graph.get(&entity_id) {
        Some(local) => local,
        None => {
            visiting.remove(&entity_id);
            return RenderTransform3D::from(&pod_core::Transform3D::default());
        }
    };

    let resolved = if let Some(parent_id) = parent_map.get(&entity_id) {
        if *parent_id == entity_id {
            RenderTransform3D::from(local_transform)
        } else {
            let parent = resolve_world_transform_recursive(
                *parent_id,
                transform_graph,
                parent_map,
                visiting,
                resolved_cache,
            );

            let local_pos = local_transform.position;
            let local_rot = glam::Quat::from_array(local_transform.rotation);
            let local_scale = local_transform.scale;

            let parent_pos = Vec3::from(parent.position);
            let parent_rot = glam::Quat::from_array(parent.rotation);
            let parent_scale = Vec3::from(parent.scale);

            RenderTransform3D {
                position: (parent_pos + parent_rot * (local_pos * parent_scale)).to_array(),
                rotation: (parent_rot * local_rot).to_array(),
                scale: (parent_scale * local_scale).to_array(),
            }
        }
    } else {
        RenderTransform3D::from(local_transform)
    };

    visiting.remove(&entity_id);
    resolved_cache.insert(entity_id, resolved);
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use pod_core::{ColorRect, Material, Mesh, Parent3D, Sprite, Transform, Transform3D};

    #[test]
    fn resolve_transform_hierarchy_composes_transform_into_world_space() {
        let mut world = hecs::World::new();

        let root = world.spawn((
            Transform3D {
                position: Vec3::new(2.0, 0.0, -1.0),
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: Vec3::new(2.0, 3.0, 4.0),
            },
            Mesh {
                asset_id: "root".to_string(),
                ..Default::default()
            },
            Material {
                asset_id: "root_mat".to_string(),
                ..Default::default()
            },
        ));

        let _child = world.spawn((
            Transform3D {
                position: Vec3::new(1.0, 1.0, 1.0),
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: Vec3::new(0.5, 2.0, 1.0),
            },
            Mesh {
                asset_id: "child".to_string(),
                layer: 1,
                ..Default::default()
            },
            Material {
                asset_id: "child_mat".to_string(),
                ..Default::default()
            },
            Parent3D {
                parent: root.id() as u64,
            },
        ));

        let mut items = {
            let state = extract_render_state(&world);
            assert_eq!(state.items.len(), 2);
            state.items
        };
        items.sort_by_key(|item| item.layer);

        let root_item = items
            .iter()
            .find_map(|item| match &item.draw_type {
                DrawType::Mesh3D { mesh, transform, .. } if mesh == "root" => Some(transform.clone()),
                _ => None,
            })
            .expect("root mesh should be extracted");

        let child_item = items
            .iter()
            .find_map(|item| match &item.draw_type {
                DrawType::Mesh3D { mesh, transform, .. } if mesh == "child" => Some(transform.clone()),
                _ => None,
            })
            .expect("child mesh should be extracted");

        assert_eq!(
            root_item.position,
            [2.0, 0.0, -1.0],
            "root transform should stay as authored when no parent"
        );
        assert_eq!(
            child_item.position,
            [4.0, 3.0, 3.0],
            "child world position should compose parent transform"
        );
        assert_eq!(
            child_item.scale,
            [1.0, 6.0, 4.0],
            "child world scale should be parent scale * local scale"
        );
        assert_eq!(child_item.rotation, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn extract_render_state_filters_hidden_items_but_keeps_2d_and_3d_mix() {
        let mut world = hecs::World::new();

        world.spawn((
            Transform {
                position: Vec2::new(-2.0, 5.0),
                ..Default::default()
            },
            Sprite {
                texture: "ui_bg".to_string(),
                layer: 1,
                ..Default::default()
            },
        ));

        world.spawn((
            Transform {
                position: Vec2::new(3.0, -1.0),
                ..Default::default()
            },
            Sprite {
                texture: "ui_hidden".to_string(),
                visible: false,
                ..Default::default()
            },
        ));

        world.spawn((
            Transform3D {
                position: Vec3::new(1.0, 2.0, 3.0),
                ..Default::default()
            },
            Mesh {
                asset_id: "model_visible".to_string(),
                layer: 0,
                ..Default::default()
            },
            Material {
                asset_id: "albedo".to_string(),
                ..Default::default()
            },
        ));

        world.spawn((
            Transform3D {
                position: Vec3::new(4.0, 0.0, 1.0),
                ..Default::default()
            },
            Mesh {
                asset_id: "model_hidden".to_string(),
                visible: false,
                ..Default::default()
            },
            Material {
                asset_id: "hidden_mat".to_string(),
                ..Default::default()
            },
        ));

        world.spawn((
            ColorRect::new(16.0, 9.0, [0.1, 0.1, 0.1, 1.0]),
        ));

        let state = extract_render_state(&world);
        assert_eq!(
            state.items.len(),
            3,
            "should include 2D sprite, color rect, and visible mesh only"
        );

        let extracted_names: Vec<&str> = state
            .items
            .iter()
            .map(|item| match &item.draw_type {
                DrawType::Mesh3D { mesh, .. } => mesh.as_str(),
                DrawType::Sprite { texture, .. } => texture.as_str(),
                _ => "",
            })
            .collect();

        assert!(extracted_names.contains(&"ui_bg"));
        assert!(extracted_names.contains(&"model_visible"));
        assert!(!extracted_names.contains(&"ui_hidden"));
        assert!(!extracted_names.contains(&"model_hidden"));
    }

    #[test]
    fn extract_render_state_uses_depth_sort_for_same_layer_items() {
        let mut world = hecs::World::new();

        world.spawn((
            Transform3D {
                position: Vec3::new(0.0, 0.0, 8.0),
                ..Default::default()
            },
            Mesh {
                asset_id: "far".to_string(),
                layer: 4,
                ..Default::default()
            },
            Material {
                asset_id: "mat".to_string(),
                ..Default::default()
            },
        ));

        world.spawn((
            Transform3D {
                position: Vec3::new(0.0, 0.0, 2.0),
                ..Default::default()
            },
            Mesh {
                asset_id: "near".to_string(),
                layer: 4,
                ..Default::default()
            },
            Material {
                asset_id: "mat".to_string(),
                ..Default::default()
            },
        ));

        let state = extract_render_state(&world);
        assert_eq!(state.items.len(), 2);

        let first = &state.items[0];
        let second = &state.items[1];
        let first_mesh = match &first.draw_type {
            DrawType::Mesh3D { mesh, .. } => mesh.as_str(),
            _ => "",
        };
        let second_mesh = match &second.draw_type {
            DrawType::Mesh3D { mesh, .. } => mesh.as_str(),
            _ => "",
        };

        assert_eq!(first_mesh, "near");
        assert_eq!(second_mesh, "far");
    }
}
