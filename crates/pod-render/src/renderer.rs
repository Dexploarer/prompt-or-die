//! Platform-agnostic render state extraction and management

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// Type of drawable entity.
///
/// Mixed-mode contract:
/// - `Rect`/`Sprite` are treated as 2D pass data and use `position`/`rotation`/`scale` directly.
/// - `Sprite3D`/`Mesh3D` are rendered in the 3D pass and use their `transform` world-space values.
/// - Depth sorting for equal layers is delegated to `RenderItem::sort_key`.
/// - `source_entity` provides deterministic provenance for editor/debug tooling.
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
        tint: [f32; 4],
        roughness: f32,
        metallic: f32,
        emissive: [f32; 3],
        double_sided: bool,
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
    /// Source ECS entity id for editor/debug tooling.
    pub source_entity: Option<u32>,
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
        Self { items: Vec::new() }
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
                    a.sort_key()
                        .partial_cmp(&b.sort_key())
                        .unwrap_or(Ordering::Equal)
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
    use pod_core::{ColorRect, Material, Mesh, Parent3D, Sprite, Transform, Transform3D};

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
    for (entity, (transform, sprite)) in world.query::<(&Transform, &Sprite)>().iter() {
        if !sprite.visible {
            continue;
        }

        let item = RenderItem {
            position: transform.position,
            rotation: transform.rotation,
            scale: transform.scale,
            layer: sprite.layer,
            source_entity: Some(entity.id()),
            draw_type: DrawType::Sprite {
                texture: sprite.texture.clone(),
                frame: sprite.frame,
                tint: sprite.color,
            },
            visible: sprite.visible,
        };
        state.add_item(item);
    }

    for (entity, (transform, color_rect)) in world.query::<(&Transform, &ColorRect)>().iter() {
        transform_color_rects.insert(entity.id());
        let item = RenderItem {
            position: transform.position,
            rotation: transform.rotation,
            scale: transform.scale,
            layer: color_rect.layer,
            source_entity: Some(entity.id()),
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
            source_entity: Some(entity.id()),
            draw_type: DrawType::Rect {
                width: color_rect.width,
                height: color_rect.height,
                color: color_rect.color,
            },
            visible: true,
        };
        state.add_item(item);
    }

    for (entity, (_transform3d, mesh, material)) in
        world.query::<(&Transform3D, &Mesh, &Material)>().iter()
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
            source_entity: Some(entity.id()),
            draw_type: DrawType::Mesh3D {
                mesh: mesh.asset_id.clone(),
                material: material.asset_id.clone(),
                tint: material.tint,
                roughness: material.roughness,
                metallic: material.metallic,
                emissive: material.emissive,
                double_sided: material.double_sided,
                transform: world_transform,
                cast_shadows: mesh.cast_shadows,
                receive_shadows: mesh.receive_shadows,
            },
            visible: true,
        };
        state.add_item(item);
    }

    // Query entities with Transform3D + Sprite for 2.5D pseudo-depth rendering
    for (entity, (_transform3d, sprite)) in world.query::<(&Transform3D, &Sprite)>().iter() {
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
            source_entity: Some(entity.id()),
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
                DrawType::Mesh3D {
                    mesh, transform, ..
                } if mesh == "root" => Some(transform.clone()),
                _ => None,
            })
            .expect("root mesh should be extracted");

        let child_item = items
            .iter()
            .find_map(|item| match &item.draw_type {
                DrawType::Mesh3D {
                    mesh, transform, ..
                } if mesh == "child" => Some(transform.clone()),
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

        world.spawn((ColorRect::new(16.0, 9.0, [0.1, 0.1, 0.1, 1.0]),));

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

    #[test]
    fn extract_render_state_preserves_mesh_material_surface_metadata() {
        let mut world = hecs::World::new();

        world.spawn((
            Transform3D {
                position: Vec3::new(0.0, 1.0, 2.0),
                ..Default::default()
            },
            Mesh {
                asset_id: "surface_mesh".to_string(),
                cast_shadows: false,
                receive_shadows: true,
                ..Default::default()
            },
            Material {
                asset_id: "surface_mat".to_string(),
                tint: [0.25, 0.5, 0.75, 0.6],
                roughness: 0.9,
                metallic: 0.2,
                emissive: [0.1, 0.2, 0.3],
                double_sided: true,
                ..Default::default()
            },
        ));

        let state = extract_render_state(&world);
        let mesh = state
            .items
            .iter()
            .find_map(|item| match &item.draw_type {
                DrawType::Mesh3D {
                    mesh,
                    material,
                    tint,
                    roughness,
                    metallic,
                    emissive,
                    double_sided,
                    cast_shadows,
                    receive_shadows,
                    ..
                } if mesh == "surface_mesh" && material == "surface_mat" => Some((
                    *tint,
                    *roughness,
                    *metallic,
                    *emissive,
                    *double_sided,
                    *cast_shadows,
                    *receive_shadows,
                )),
                _ => None,
            })
            .expect("surface mesh should be extracted");

        assert_eq!(mesh.0, [0.25, 0.5, 0.75, 0.6]);
        assert!((mesh.1 - 0.9).abs() < 1e-6);
        assert!((mesh.2 - 0.2).abs() < 1e-6);
        assert_eq!(mesh.3, [0.1, 0.2, 0.3]);
        assert!(mesh.4);
        assert!(!mesh.5);
        assert!(mesh.6);
    }

    #[test]
    fn extract_render_state_2d_and_3d_items_same_layer_sort_by_mode_depth_key() {
        let mut world = hecs::World::new();

        world.spawn((
            Transform {
                position: Vec2::new(4.0, 10.0),
                ..Default::default()
            },
            Sprite {
                texture: "ui_far".to_string(),
                layer: 2,
                ..Default::default()
            },
        ));

        world.spawn((
            Transform {
                position: Vec2::new(8.0, -1.0),
                ..Default::default()
            },
            Sprite {
                texture: "ui_near".to_string(),
                layer: 2,
                ..Default::default()
            },
        ));

        world.spawn((
            Transform3D {
                position: Vec3::new(0.0, 0.0, 8.0),
                ..Default::default()
            },
            Mesh {
                asset_id: "mesh_far".to_string(),
                layer: 2,
                ..Default::default()
            },
            Material {
                asset_id: "mesh_far_mat".to_string(),
                ..Default::default()
            },
        ));

        world.spawn((
            Transform3D {
                position: Vec3::new(0.0, 0.0, 2.0),
                ..Default::default()
            },
            Sprite {
                texture: "sprite3d_near".to_string(),
                layer: 2,
                ..Default::default()
            },
        ));

        let state = extract_render_state(&world);
        let ordered: Vec<String> = state
            .items
            .iter()
            .filter_map(|item| match &item.draw_type {
                DrawType::Sprite { texture, .. } => Some(texture.clone()),
                DrawType::Mesh3D { mesh, .. } => Some(mesh.clone()),
                DrawType::Sprite3D { texture, .. } => Some(format!("sprite3d:{texture}")),
                _ => None,
            })
            .collect();

        assert_eq!(ordered.len(), 4);
        assert_eq!(
            ordered,
            vec![
                "ui_near".to_string(),
                "sprite3d:sprite3d_near".to_string(),
                "mesh_far".to_string(),
                "ui_far".to_string(),
            ]
        );
    }

    #[test]
    fn extract_render_state_supports_sprite3d_visibility_and_parent_transform_hierarchy() {
        let mut world = hecs::World::new();

        let root = world.spawn((
            Transform3D {
                position: Vec3::new(1.0, 2.0, 3.0),
                ..Default::default()
            },
            ColorRect::new(1.0, 1.0, [1.0, 1.0, 1.0, 1.0]),
        ));

        world.spawn((
            Transform3D {
                position: Vec3::new(2.0, 1.0, 1.0),
                ..Default::default()
            },
            Mesh {
                asset_id: "billboard_pivot_mesh".to_string(),
                layer: 3,
                ..Default::default()
            },
            Material {
                asset_id: "mesh_pivot_mat".to_string(),
                ..Default::default()
            },
            Parent3D {
                parent: root.id() as u64,
            },
        ));

        world.spawn((
            Transform3D {
                position: Vec3::new(3.0, 4.0, 5.0),
                ..Default::default()
            },
            Sprite {
                texture: "billboard_sprite".to_string(),
                layer: 3,
                ..Default::default()
            },
            Parent3D {
                parent: root.id() as u64,
            },
        ));

        world.spawn((
            Transform3D {
                position: Vec3::new(9.0, 9.0, 9.0),
                ..Default::default()
            },
            Sprite {
                texture: "hidden_2d3d".to_string(),
                visible: false,
                ..Default::default()
            },
        ));

        let state = extract_render_state(&world);
        assert_eq!(
            state.items.len(),
            3,
            "root color rect + mesh + sprite3d should be extracted"
        );

        let mesh_transform = state
            .items
            .iter()
            .find_map(|item| match &item.draw_type {
                DrawType::Mesh3D { transform, .. } => Some(transform.position),
                _ => None,
            })
            .expect("mesh3d should be extracted");

        let sprite3d_transform = state
            .items
            .iter()
            .find_map(|item| match &item.draw_type {
                DrawType::Sprite3D { transform, .. } => Some(transform.position),
                _ => None,
            })
            .expect("sprite3d should be extracted");

        let extracted_names: Vec<&str> = state
            .items
            .iter()
            .map(|item| match &item.draw_type {
                DrawType::Sprite { texture, .. } => texture.as_str(),
                DrawType::Mesh3D { mesh, .. } => mesh.as_str(),
                DrawType::Sprite3D { texture, .. } => texture.as_str(),
                _ => "",
            })
            .collect();

        assert_eq!(mesh_transform, [3.0, 3.0, 4.0]);
        assert_eq!(sprite3d_transform, [4.0, 6.0, 8.0]);
        assert!(
            !extracted_names.contains(&"hidden_2d3d"),
            "hidden sprite3d should not be extracted"
        );
    }

    #[test]
    fn extract_render_state_uses_parent_world_depth_for_sprite3d_sorting() {
        let mut world = hecs::World::new();

        let parent = world.spawn((Transform3D {
            position: Vec3::new(0.0, 0.0, 100.0),
            ..Default::default()
        },));

        world.spawn((
            Transform {
                position: Vec2::new(-25.0, 15.0),
                ..Default::default()
            },
            Sprite {
                texture: "hud_ui".to_string(),
                layer: 7,
                ..Default::default()
            },
        ));

        world.spawn((
            Transform3D {
                position: Vec3::new(0.0, 0.0, 50.0),
                ..Default::default()
            },
            Mesh {
                asset_id: "mesh_mid".to_string(),
                layer: 7,
                ..Default::default()
            },
            Material {
                asset_id: "mesh_mid_mat".to_string(),
                ..Default::default()
            },
        ));

        world.spawn((
            Transform3D {
                position: Vec3::new(0.0, 0.0, 5.0),
                ..Default::default()
            },
            Sprite {
                texture: "sprite3d_from_parent".to_string(),
                layer: 7,
                ..Default::default()
            },
            Parent3D {
                parent: parent.id() as u64,
            },
        ));

        let state = extract_render_state(&world);
        assert_eq!(state.items.len(), 3);

        let ordered: Vec<String> = state
            .items
            .iter()
            .map(|item| match &item.draw_type {
                DrawType::Sprite { texture, .. } => texture.clone(),
                DrawType::Mesh3D { mesh, .. } => mesh.clone(),
                DrawType::Sprite3D { texture, .. } => format!("sprite3d:{texture}"),
                _ => String::new(),
            })
            .collect();

        assert_eq!(
            ordered,
            vec![
                "hud_ui".to_string(),
                "mesh_mid".to_string(),
                "sprite3d:sprite3d_from_parent".to_string(),
            ]
        );

        let sprite3d_world_z = state
            .items
            .iter()
            .find_map(|item| match &item.draw_type {
                DrawType::Sprite3D { transform, .. } => Some(transform.position[2]),
                _ => None,
            })
            .expect("sprite3d should be extracted");

        assert_eq!(sprite3d_world_z, 105.0);
    }

    #[test]
    fn extract_render_state_handles_missing_3d_parent_by_falling_back_to_local_transform() {
        let mut world = hecs::World::new();
        let local_transform = Transform3D {
            position: Vec3::new(2.5, 1.0, -4.0),
            scale: Vec3::new(1.5, 2.0, 0.5),
            ..Default::default()
        };

        world.spawn((
            local_transform,
            Sprite {
                texture: "orphan_sprite3d".to_string(),
                ..Default::default()
            },
            Parent3D { parent: 99_999 },
        ));

        let state = extract_render_state(&world);
        assert_eq!(state.items.len(), 1);

        let extract = state.items.iter().find_map(|item| match &item.draw_type {
            DrawType::Sprite3D {
                transform, texture, ..
            } if texture == "orphan_sprite3d" => Some(*transform),
            _ => None,
        });

        let world_transform = extract.expect("orphan sprite3d should be extracted");
        assert_eq!(world_transform.position, [2.5, 1.0, -4.0]);
        assert_eq!(world_transform.scale, [1.5, 2.0, 0.5]);
        assert_eq!(
            world_transform.rotation,
            [0.0, 0.0, 0.0, 1.0],
            "missing 3d parent should not alter identity rotation"
        );
    }

    #[test]
    fn extract_render_state_handles_parent_cycle_without_infinite_recursion() {
        let mut world = hecs::World::new();

        let entity_a = world.spawn((
            Transform3D {
                position: Vec3::new(1.0, 0.0, 0.0),
                ..Default::default()
            },
            Mesh {
                asset_id: "cycle_a".to_string(),
                layer: 4,
                ..Default::default()
            },
            Material {
                asset_id: "cycle_mat_a".to_string(),
                ..Default::default()
            },
            Parent3D { parent: 1 },
        ));

        let entity_b = world.spawn((
            Transform3D {
                position: Vec3::new(10.0, 0.0, 0.0),
                ..Default::default()
            },
            Mesh {
                asset_id: "cycle_b".to_string(),
                layer: 4,
                ..Default::default()
            },
            Material {
                asset_id: "cycle_mat_b".to_string(),
                ..Default::default()
            },
            Parent3D {
                parent: entity_a.id() as u64,
            },
        ));

        world
            .insert_one(
                entity_a,
                Parent3D {
                    parent: entity_b.id() as u64,
                },
            )
            .expect("update loop entity A parent to B");

        let state = extract_render_state(&world);
        assert_eq!(
            state.items.len(),
            2,
            "cycle meshes should be extracted without recursion failure"
        );

        let mut cycle_positions = state
            .items
            .iter()
            .filter_map(|item| match &item.draw_type {
                DrawType::Mesh3D {
                    mesh, transform, ..
                } if mesh == "cycle_a" => Some(transform.position[0]),
                DrawType::Mesh3D {
                    mesh, transform, ..
                } if mesh == "cycle_b" => Some(transform.position[0]),
                _ => None,
            })
            .collect::<Vec<_>>();
        cycle_positions.sort_by(|a, b| a.partial_cmp(b).unwrap());

        assert_eq!(cycle_positions, vec![11.0, 12.0]);
    }

    #[test]
    fn extract_render_state_sprite3d_handles_parent_cycle_without_infinite_recursion() {
        let mut world = hecs::World::new();

        let entity_a = world.spawn((
            Transform3D {
                position: Vec3::new(1.0, 0.0, 0.0),
                ..Default::default()
            },
            Sprite {
                texture: "cycle_a".to_string(),
                ..Default::default()
            },
            Parent3D { parent: 1 },
        ));

        let entity_b = world.spawn((
            Transform3D {
                position: Vec3::new(10.0, 0.0, 0.0),
                ..Default::default()
            },
            Sprite {
                texture: "cycle_b".to_string(),
                ..Default::default()
            },
            Parent3D {
                parent: entity_a.id() as u64,
            },
        ));

        world
            .insert_one(
                entity_a,
                Parent3D {
                    parent: entity_b.id() as u64,
                },
            )
            .expect("update loop entity A parent to B");

        let state = extract_render_state(&world);
        assert_eq!(
            state.items.len(),
            2,
            "cycle sprites should be extracted without recursion failure"
        );

        let mut cycle_positions = state
            .items
            .iter()
            .filter_map(|item| match &item.draw_type {
                DrawType::Sprite3D {
                    texture, transform, ..
                } if texture == "cycle_a" => Some(transform.position[0]),
                DrawType::Sprite3D {
                    texture, transform, ..
                } if texture == "cycle_b" => Some(transform.position[0]),
                _ => None,
            })
            .collect::<Vec<_>>();
        cycle_positions.sort_by(|a, b| a.partial_cmp(b).unwrap());

        assert_eq!(cycle_positions, vec![11.0, 12.0]);

        let mut sprite3d_count = 0usize;
        for item in &state.items {
            if let DrawType::Sprite3D { .. } = &item.draw_type {
                sprite3d_count += 1;
            }
        }
        assert_eq!(
            sprite3d_count, 2,
            "all sprite3d cycle entries should be extracted"
        );
    }

    #[test]
    fn extract_render_state_sets_source_entity_on_all_items() {
        let mut world = hecs::World::new();

        let sprite_entity = world.spawn((
            Transform {
                position: Vec2::new(1.0, 2.0),
                ..Default::default()
            },
            Sprite {
                texture: "ui_sprite".to_string(),
                layer: 1,
                ..Default::default()
            },
        ));

        let color_rect_entity = world.spawn((
            Transform {
                position: Vec2::new(3.0, 4.0),
                ..Default::default()
            },
            ColorRect::new(2.0, 3.0, [1.0, 1.0, 1.0, 1.0]),
        ));

        let mesh_entity = world.spawn((
            Transform3D::default(),
            Mesh {
                asset_id: "mesh".to_string(),
                layer: 2,
                ..Default::default()
            },
            Material {
                asset_id: "mat".to_string(),
                ..Default::default()
            },
        ));

        let sprite3d_entity = world.spawn((
            Transform3D::default(),
            Sprite {
                texture: "sprite_3d".to_string(),
                layer: 3,
                ..Default::default()
            },
        ));

        let state = extract_render_state(&world);
        let source_ids = state
            .items
            .iter()
            .filter_map(|item| item.source_entity)
            .collect::<std::collections::HashSet<_>>();

        assert_eq!(
            source_ids.len(),
            4,
            "all supported renderables should carry a source entity id"
        );

        assert!(source_ids.contains(&(sprite_entity.id() as u32)));
        assert!(source_ids.contains(&(color_rect_entity.id() as u32)));
        assert!(source_ids.contains(&(mesh_entity.id() as u32)));
        assert!(source_ids.contains(&(sprite3d_entity.id() as u32)));
    }
}
