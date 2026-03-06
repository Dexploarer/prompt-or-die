//! Browser renderer bridge for JavaScript frontends.
//!
//! Keeps a legacy per-item render payload for lightweight JS renderers and
//! exposes a Three.js/WebGPU-oriented frame payload with batching metadata for
//! instancing-friendly consumption.

use crate::camera::Camera;
use crate::renderer::{DrawType, RenderItem, RenderState, RenderTransform3D};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

/// Serializable render command for lightweight JS renderers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCommand {
    #[serde(rename = "type")]
    pub item_type: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub color: [f32; 4],
    pub alpha: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub texture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform_3d: Option<RenderTransform3D>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billboard: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cast_shadows: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receive_shadows: Option<bool>,
    pub layer: i32,
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_entity: Option<u32>,
}

/// Camera state for JS-side renderers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraState {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub rotation: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl From<&Camera> for CameraState {
    fn from(camera: &Camera) -> Self {
        Self {
            x: camera.position.x,
            y: camera.position.y,
            zoom: camera.zoom,
            rotation: camera.rotation,
            viewport_width: camera.viewport_width,
            viewport_height: camera.viewport_height,
        }
    }
}

/// Complete render frame for per-item JS renderers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderFrame {
    pub camera: CameraState,
    pub commands: Vec<RenderCommand>,
    pub background_color: [f32; 4],
}

/// Per-instance data for Three.js WebGPU frontends.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreeJsInstance {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_entity: Option<u32>,
}

/// Mesh batch metadata for instancing-friendly Three.js WebGPU rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreeJsMeshBatch {
    pub mesh: String,
    pub material: String,
    pub layer: i32,
    pub cast_shadows: bool,
    pub receive_shadows: bool,
    pub instances: Vec<ThreeJsInstance>,
}

/// Billboard sprite batch metadata for instancing-friendly Three.js WebGPU rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreeJsSpriteBatch {
    pub texture: String,
    pub frame: u32,
    pub layer: i32,
    pub billboard: bool,
    pub instances: Vec<ThreeJsInstance>,
}

/// Runtime hints for a Three.js WebGPU consumer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreeJsWebGpuHints {
    pub renderer: String,
    pub preferred_backend: String,
    pub fallback_backend: String,
    pub use_instancing: bool,
    pub sort_opaque_front_to_back: bool,
    pub preserve_instance_order: bool,
    pub max_pixel_ratio: f32,
}

/// Three.js/WebGPU-oriented render frame with batching metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreeJsWebGpuFrame {
    pub camera: CameraState,
    pub background_color: [f32; 4],
    pub overlay_commands: Vec<RenderCommand>,
    pub mesh_batches: Vec<ThreeJsMeshBatch>,
    pub sprite_batches: Vec<ThreeJsSpriteBatch>,
    pub hints: ThreeJsWebGpuHints,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MeshBatchKey {
    layer: i32,
    mesh: String,
    material: String,
    cast_shadows: bool,
    receive_shadows: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SpriteBatchKey {
    layer: i32,
    texture: String,
    frame: u32,
    billboard: bool,
}

/// Web-specific renderer bridge.
pub struct WebRenderBridge;

impl WebRenderBridge {
    /// Convert render state and camera to JSON for lightweight JS renderers.
    pub fn to_render_json(
        state: &RenderState,
        camera: &Camera,
        background_color: [f32; 4],
    ) -> String {
        let frame = RenderFrame {
            camera: CameraState::from(camera),
            commands: Self::collect_visible_commands(state),
            background_color,
        };

        serde_json::to_string(&frame).unwrap_or_else(|error| {
            log::error!("Failed to serialize render frame: {error}");
            "{}".to_string()
        })
    }

    /// Convert the legacy per-item frame to a JSON value.
    pub fn to_render_value(
        state: &RenderState,
        camera: &Camera,
        background_color: [f32; 4],
    ) -> serde_json::Value {
        let commands = Self::collect_visible_commands(state)
            .into_iter()
            .map(|command| {
                serde_json::to_value(command).unwrap_or_else(|error| {
                    log::error!("Failed to serialize render command: {error}");
                    json!({})
                })
            })
            .collect::<Vec<_>>();

        json!({
            "camera": {
                "x": camera.position.x,
                "y": camera.position.y,
                "zoom": camera.zoom,
                "rotation": camera.rotation,
                "viewportWidth": camera.viewport_width,
                "viewportHeight": camera.viewport_height,
            },
            "commands": commands,
            "backgroundColor": {
                "r": (background_color[0] * 255.0) as u8,
                "g": (background_color[1] * 255.0) as u8,
                "b": (background_color[2] * 255.0) as u8,
                "a": (background_color[3] * 255.0) as u8,
            },
        })
    }

    /// Build a batching-oriented frame for Three.js/WebGPU frontends.
    pub fn to_threejs_webgpu_frame(
        state: &RenderState,
        camera: &Camera,
        background_color: [f32; 4],
    ) -> ThreeJsWebGpuFrame {
        let mut overlay_commands = Vec::new();
        let mut mesh_batches = BTreeMap::<MeshBatchKey, Vec<ThreeJsInstance>>::new();
        let mut sprite_batches = BTreeMap::<SpriteBatchKey, Vec<ThreeJsInstance>>::new();

        for item in &state.items {
            if !item.visible {
                continue;
            }

            match &item.draw_type {
                DrawType::Rect { .. } | DrawType::Sprite { .. } => {
                    overlay_commands.push(Self::render_command_from_item(item));
                }
                DrawType::Mesh3D {
                    mesh,
                    material,
                    transform,
                    cast_shadows,
                    receive_shadows,
                } => {
                    let key = MeshBatchKey {
                        layer: item.layer,
                        mesh: mesh.clone(),
                        material: material.clone(),
                        cast_shadows: *cast_shadows,
                        receive_shadows: *receive_shadows,
                    };
                    mesh_batches
                        .entry(key)
                        .or_default()
                        .push(Self::instance_from_transform(
                            *transform,
                            None,
                            item.source_entity,
                        ));
                }
                DrawType::Sprite3D {
                    texture,
                    frame,
                    tint,
                    transform,
                    billboard,
                } => {
                    let key = SpriteBatchKey {
                        layer: item.layer,
                        texture: texture.clone(),
                        frame: *frame,
                        billboard: *billboard,
                    };
                    sprite_batches
                        .entry(key)
                        .or_default()
                        .push(Self::instance_from_transform(
                            *transform,
                            Some(*tint),
                            item.source_entity,
                        ));
                }
            }
        }

        ThreeJsWebGpuFrame {
            camera: CameraState::from(camera),
            background_color,
            overlay_commands,
            mesh_batches: mesh_batches
                .into_iter()
                .map(|(key, instances)| ThreeJsMeshBatch {
                    mesh: key.mesh,
                    material: key.material,
                    layer: key.layer,
                    cast_shadows: key.cast_shadows,
                    receive_shadows: key.receive_shadows,
                    instances,
                })
                .collect(),
            sprite_batches: sprite_batches
                .into_iter()
                .map(|(key, instances)| ThreeJsSpriteBatch {
                    texture: key.texture,
                    frame: key.frame,
                    layer: key.layer,
                    billboard: key.billboard,
                    instances,
                })
                .collect(),
            hints: ThreeJsWebGpuHints {
                renderer: "three/webgpu".to_string(),
                preferred_backend: "webgpu".to_string(),
                fallback_backend: "webgl2".to_string(),
                use_instancing: true,
                sort_opaque_front_to_back: true,
                preserve_instance_order: true,
                max_pixel_ratio: 2.0,
            },
        }
    }

    /// Serialize the Three.js/WebGPU frame payload.
    pub fn to_threejs_webgpu_json(
        state: &RenderState,
        camera: &Camera,
        background_color: [f32; 4],
    ) -> String {
        serde_json::to_string(&Self::to_threejs_webgpu_frame(
            state,
            camera,
            background_color,
        ))
        .unwrap_or_else(|error| {
            log::error!("Failed to serialize Three.js WebGPU frame: {error}");
            "{}".to_string()
        })
    }

    /// Convert the Three.js/WebGPU frame payload into a JSON value.
    pub fn to_threejs_webgpu_value(
        state: &RenderState,
        camera: &Camera,
        background_color: [f32; 4],
    ) -> serde_json::Value {
        serde_json::to_value(Self::to_threejs_webgpu_frame(
            state,
            camera,
            background_color,
        ))
        .unwrap_or_else(|error| {
            log::error!("Failed to serialize Three.js WebGPU frame value: {error}");
            json!({})
        })
    }

    /// Render a frame (posts to JavaScript).
    pub fn render(state: &RenderState, camera: &Camera, background_color: [f32; 4]) {
        let json_str = Self::to_render_json(state, camera, background_color);
        Self::post_to_js("render", &json_str);
    }

    fn collect_visible_commands(state: &RenderState) -> Vec<RenderCommand> {
        state
            .items
            .iter()
            .filter(|item| item.visible)
            .map(Self::render_command_from_item)
            .collect()
    }

    fn render_command_from_item(item: &RenderItem) -> RenderCommand {
        match &item.draw_type {
            DrawType::Rect {
                width,
                height,
                color,
            } => RenderCommand {
                item_type: "rect".to_string(),
                x: item.position.x,
                y: item.position.y,
                width: *width,
                height: *height,
                rotation: item.rotation,
                scale_x: item.scale.x,
                scale_y: item.scale.y,
                color: *color,
                alpha: color[3],
                texture: None,
                frame: None,
                mesh: None,
                material: None,
                z: None,
                transform_3d: None,
                billboard: None,
                cast_shadows: None,
                receive_shadows: None,
                layer: item.layer,
                visible: item.visible,
                source_entity: item.source_entity,
            },
            DrawType::Sprite {
                texture,
                frame,
                tint,
            } => RenderCommand {
                item_type: "sprite".to_string(),
                x: item.position.x,
                y: item.position.y,
                width: 32.0,
                height: 32.0,
                rotation: item.rotation,
                scale_x: item.scale.x,
                scale_y: item.scale.y,
                color: *tint,
                alpha: tint[3],
                texture: Some(texture.clone()),
                frame: Some(*frame),
                mesh: None,
                material: None,
                z: None,
                transform_3d: None,
                billboard: None,
                cast_shadows: None,
                receive_shadows: None,
                layer: item.layer,
                visible: item.visible,
                source_entity: item.source_entity,
            },
            DrawType::Mesh3D {
                mesh,
                material,
                transform,
                cast_shadows,
                receive_shadows,
            } => RenderCommand {
                item_type: "mesh3d".to_string(),
                x: transform.position[0],
                y: transform.position[1],
                width: 0.0,
                height: 0.0,
                rotation: 0.0,
                scale_x: transform.scale[0],
                scale_y: transform.scale[1],
                color: [1.0, 1.0, 1.0, 1.0],
                alpha: 1.0,
                texture: None,
                frame: None,
                mesh: Some(mesh.clone()),
                material: Some(material.clone()),
                z: Some(transform.position[2]),
                transform_3d: Some(*transform),
                billboard: None,
                cast_shadows: Some(*cast_shadows),
                receive_shadows: Some(*receive_shadows),
                layer: item.layer,
                visible: item.visible,
                source_entity: item.source_entity,
            },
            DrawType::Sprite3D {
                texture,
                frame,
                tint,
                transform,
                billboard,
            } => RenderCommand {
                item_type: "sprite3d".to_string(),
                x: transform.position[0],
                y: transform.position[1],
                width: transform.scale[0].abs().max(0.0001),
                height: transform.scale[1].abs().max(0.0001),
                rotation: 0.0,
                scale_x: transform.scale[0].abs().max(0.0001),
                scale_y: transform.scale[1].abs().max(0.0001),
                color: *tint,
                alpha: tint[3],
                texture: Some(texture.clone()),
                frame: Some(*frame),
                mesh: None,
                material: None,
                z: Some(transform.position[2]),
                transform_3d: Some(*transform),
                billboard: Some(*billboard),
                cast_shadows: None,
                receive_shadows: None,
                layer: item.layer,
                visible: item.visible,
                source_entity: item.source_entity,
            },
        }
    }

    fn instance_from_transform(
        transform: RenderTransform3D,
        color: Option<[f32; 4]>,
        source_entity: Option<u32>,
    ) -> ThreeJsInstance {
        ThreeJsInstance {
            position: transform.position,
            rotation: transform.rotation,
            scale: transform.scale,
            color,
            source_entity,
        }
    }

    /// Post data to JavaScript side.
    fn post_to_js(method: &str, data: &str) {
        #[cfg(target_arch = "wasm32")]
        {
            use js_sys::{Function, Reflect};
            use wasm_bindgen::{JsCast, JsValue};
            use web_sys::window;
            if let Some(window) = window() {
                if let Ok(pod_render) =
                    Reflect::get(window.as_ref(), &JsValue::from_str("podRender"))
                {
                    if let Some(function) = Reflect::get(&pod_render, &JsValue::from_str(method))
                        .ok()
                        .and_then(|value| value.dyn_into::<Function>().ok())
                    {
                        let _ = function.call1(&pod_render, &JsValue::from_str(data));
                    }
                }
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (method, data);
        }
    }
}

/// JavaScript bridge (exposed for wasm_bindgen).
#[cfg(target_arch = "wasm32")]
pub mod js_bridge {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub fn pod_render_init() {
        log::info!("pod-render web bridge initialized");
    }

    #[wasm_bindgen]
    pub fn pod_render_set_viewport(width: f32, height: f32) {
        log::info!("Viewport set to {}x{}", width, height);
    }

    #[wasm_bindgen]
    pub fn pod_render_frame(json_data: String) {
        log::debug!("Render frame: {}", json_data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::{DrawType, RenderItem, RenderTransform3D};
    use glam::Vec2;

    fn default_camera() -> Camera {
        Camera::new(Vec2::ZERO, 1280.0, 720.0)
    }

    #[test]
    fn render_frame_serialization_uses_type_field() {
        let mut state = RenderState::new();
        state.add_item(RenderItem {
            position: Vec2::new(100.0, 200.0),
            rotation: 0.0,
            scale: Vec2::ONE,
            layer: 0,
            draw_type: DrawType::Rect {
                width: 50.0,
                height: 50.0,
                color: [1.0, 0.0, 0.0, 1.0],
            },
            visible: true,
            source_entity: Some(101),
        });

        let json = WebRenderBridge::to_render_json(&state, &default_camera(), [0.1, 0.1, 0.1, 1.0]);

        assert!(json.contains("\"type\":\"rect\""));
        assert!(json.contains("\"x\":100"));
        assert!(json.contains("\"y\":200"));
        assert!(json.contains("\"source_entity\":101"));
    }

    #[test]
    fn render_value_generation_keeps_sprite_payload() {
        let mut state = RenderState::new();
        state.add_item(RenderItem {
            position: Vec2::new(50.0, 75.0),
            rotation: 0.5,
            scale: Vec2::new(2.0, 1.5),
            layer: 1,
            draw_type: DrawType::Sprite {
                texture: "player".to_string(),
                frame: 0,
                tint: [1.0, 1.0, 1.0, 1.0],
            },
            visible: true,
            source_entity: Some(202),
        });

        let camera = Camera::new(Vec2::new(100.0, 100.0), 1280.0, 720.0);
        let value = WebRenderBridge::to_render_value(&state, &camera, [0.0, 0.0, 0.0, 1.0]);

        assert_eq!(value["camera"]["zoom"], 1.0);
        assert_eq!(value["commands"][0]["type"], "sprite");
        assert_eq!(value["commands"][0]["texture"], "player");
        assert_eq!(value["commands"][0]["source_entity"], 202);
    }

    #[test]
    fn render_value_includes_full_sprite3d_transform() {
        let mut state = RenderState::new();
        state.add_item(RenderItem {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            layer: 2,
            draw_type: DrawType::Sprite3D {
                texture: "npc_sprite".to_string(),
                frame: 1,
                tint: [0.2, 0.4, 0.8, 0.75],
                transform: RenderTransform3D {
                    position: [0.0, 1.0, 3.5],
                    rotation: [0.0, 0.1, 0.0, 0.995],
                    scale: [0.75, 1.5, 1.0],
                },
                billboard: true,
            },
            visible: true,
            source_entity: Some(303),
        });

        let value =
            WebRenderBridge::to_render_value(&state, &default_camera(), [0.0, 0.0, 0.0, 1.0]);

        assert_eq!(value["commands"][0]["type"], "sprite3d");
        assert_eq!(value["commands"][0]["texture"], "npc_sprite");
        assert_eq!(value["commands"][0]["billboard"], true);
        let rotation_y = value["commands"][0]["transform_3d"]["rotation"][1]
            .as_f64()
            .unwrap_or_default();
        assert!((rotation_y - 0.1).abs() < 1e-6);
        assert_eq!(value["commands"][0]["source_entity"], 303);
    }

    #[test]
    fn threejs_webgpu_frame_batches_mesh_instances_by_asset_pair() {
        let mut state = RenderState::new();
        for (source_entity, x) in [(401_u32, 1.0_f32), (402_u32, 4.0_f32)] {
            state.add_item(RenderItem {
                position: Vec2::ZERO,
                rotation: 0.0,
                scale: Vec2::ONE,
                layer: 3,
                draw_type: DrawType::Mesh3D {
                    mesh: "tree".to_string(),
                    material: "forest".to_string(),
                    transform: RenderTransform3D {
                        position: [x, 0.0, 8.0 - x],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                        scale: [1.0, 1.0, 1.0],
                    },
                    cast_shadows: true,
                    receive_shadows: true,
                },
                visible: true,
                source_entity: Some(source_entity),
            });
        }
        state.add_item(RenderItem {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            layer: 3,
            draw_type: DrawType::Mesh3D {
                mesh: "rock".to_string(),
                material: "stone".to_string(),
                transform: RenderTransform3D {
                    position: [10.0, 0.0, 2.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [0.75, 0.75, 0.75],
                },
                cast_shadows: false,
                receive_shadows: true,
            },
            visible: true,
            source_entity: Some(403),
        });

        let frame = WebRenderBridge::to_threejs_webgpu_frame(
            &state,
            &default_camera(),
            [0.0, 0.0, 0.0, 1.0],
        );

        assert_eq!(frame.mesh_batches.len(), 2);
        let tree_batch = frame
            .mesh_batches
            .iter()
            .find(|batch| batch.mesh == "tree")
            .expect("tree batch should exist");
        assert_eq!(tree_batch.instances.len(), 2);
        assert_eq!(tree_batch.instances[0].source_entity, Some(401));
        assert_eq!(tree_batch.instances[1].source_entity, Some(402));
        assert_eq!(frame.hints.renderer, "three/webgpu");
        assert!(frame.hints.use_instancing);
    }

    #[test]
    fn threejs_webgpu_frame_batches_billboard_sprites_by_texture_frame_and_layer() {
        let mut state = RenderState::new();
        state.add_item(RenderItem {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            layer: 5,
            draw_type: DrawType::Sprite3D {
                texture: "npc.png".to_string(),
                frame: 2,
                tint: [1.0, 0.5, 0.5, 1.0],
                transform: RenderTransform3D {
                    position: [1.0, 2.0, 3.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 2.0, 1.0],
                },
                billboard: true,
            },
            visible: true,
            source_entity: Some(501),
        });
        state.add_item(RenderItem {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            layer: 5,
            draw_type: DrawType::Sprite3D {
                texture: "npc.png".to_string(),
                frame: 2,
                tint: [0.5, 1.0, 0.5, 0.8],
                transform: RenderTransform3D {
                    position: [4.0, 5.0, 6.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.5, 2.5, 1.0],
                },
                billboard: true,
            },
            visible: true,
            source_entity: Some(502),
        });
        state.add_item(RenderItem {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            layer: 5,
            draw_type: DrawType::Sprite3D {
                texture: "npc.png".to_string(),
                frame: 3,
                tint: [1.0, 1.0, 1.0, 1.0],
                transform: RenderTransform3D {
                    position: [7.0, 8.0, 9.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                },
                billboard: true,
            },
            visible: true,
            source_entity: Some(503),
        });

        let frame = WebRenderBridge::to_threejs_webgpu_frame(
            &state,
            &default_camera(),
            [0.0, 0.0, 0.0, 1.0],
        );

        assert_eq!(frame.sprite_batches.len(), 2);
        let npc_batch = frame
            .sprite_batches
            .iter()
            .find(|batch| batch.texture == "npc.png" && batch.frame == 2)
            .expect("frame-2 sprite batch should exist");
        assert_eq!(npc_batch.instances.len(), 2);
        assert_eq!(npc_batch.instances[0].color, Some([1.0, 0.5, 0.5, 1.0]));
        assert_eq!(npc_batch.instances[1].source_entity, Some(502));
    }

    #[test]
    fn threejs_webgpu_frame_keeps_2d_overlay_commands_separate() {
        let mut state = RenderState::new();
        state.add_item(RenderItem {
            position: Vec2::new(32.0, 48.0),
            rotation: 0.25,
            scale: Vec2::new(1.5, 2.0),
            layer: 10,
            draw_type: DrawType::Sprite {
                texture: "hud.png".to_string(),
                frame: 0,
                tint: [1.0, 1.0, 1.0, 1.0],
            },
            visible: true,
            source_entity: Some(601),
        });

        let frame = WebRenderBridge::to_threejs_webgpu_frame(
            &state,
            &default_camera(),
            [0.0, 0.0, 0.0, 1.0],
        );

        assert_eq!(frame.overlay_commands.len(), 1);
        assert_eq!(frame.overlay_commands[0].item_type, "sprite");
        assert!(frame.mesh_batches.is_empty());
        assert!(frame.sprite_batches.is_empty());
    }
}
