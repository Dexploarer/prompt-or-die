//! Web renderer bridge for browser platforms
//! Serializes render state to JSON for PixiJS consumption on JS side

use crate::renderer::{RenderState, DrawType};
use crate::camera::Camera;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Serializable render command for PixiJS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCommand {
    pub item_type: String, // "rect", "sprite", "sprite3d", "mesh3d"
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
    pub layer: i32,
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_entity: Option<u32>,
}

/// Camera state for JS side
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

/// Complete render frame for PixiJS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderFrame {
    pub camera: CameraState,
    pub commands: Vec<RenderCommand>,
    pub background_color: [f32; 4],
}

/// Web-specific renderer bridge
pub struct WebRenderBridge;

impl WebRenderBridge {
    /// Convert render state and camera to JSON for PixiJS
    pub fn to_render_json(
        state: &RenderState,
        camera: &Camera,
        background_color: [f32; 4],
    ) -> String {
        let mut commands = Vec::new();

        for item in &state.items {
            if !item.visible {
                continue;
            }

            let command = match &item.draw_type {
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
                    width: 32.0, // Default sprite size
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
                    layer: item.layer,
                    visible: item.visible,
                    source_entity: item.source_entity,
                },
                DrawType::Mesh3D {
                    mesh,
                    material,
                    transform,
                    ..
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
                    width: transform.scale[0],
                    height: transform.scale[1],
                    rotation: if *billboard { 0.0 } else { 0.0 },
                    scale_x: transform.scale[0].abs().max(0.0001),
                    scale_y: transform.scale[1].abs().max(0.0001),
                    color: *tint,
                    alpha: tint[3],
                    texture: Some(texture.clone()),
                    frame: Some(*frame),
                    mesh: None,
                    material: None,
                    z: Some(transform.position[2]),
                    layer: item.layer,
                    visible: item.visible,
                    source_entity: item.source_entity,
                },
            };
            commands.push(command);
        }

        let frame = RenderFrame {
            camera: CameraState::from(camera),
            commands,
            background_color,
        };

        serde_json::to_string(&frame).unwrap_or_else(|e| {
            log::error!("Failed to serialize render frame: {}", e);
            "{}".to_string()
        })
    }

    /// Convert to JSON value for easier manipulation
    pub fn to_render_value(
        state: &RenderState,
        camera: &Camera,
        background_color: [f32; 4],
    ) -> serde_json::Value {
        let mut commands = Vec::new();

        for item in &state.items {
            if !item.visible {
                continue;
            }

            let command = match &item.draw_type {
                DrawType::Rect {
                    width,
                    height,
                    color,
                } => json!({
                    "type": "rect",
                    "x": item.position.x,
                    "y": item.position.y,
                    "width": width,
                    "height": height,
                    "rotation": item.rotation,
                    "scaleX": item.scale.x,
                    "scaleY": item.scale.y,
                    "color": {
                        "r": (color[0] * 255.0) as u8,
                        "g": (color[1] * 255.0) as u8,
                        "b": (color[2] * 255.0) as u8,
                    },
                    "alpha": color[3],
                    "layer": item.layer,
                    "source_entity": item.source_entity,
                }),
                DrawType::Sprite {
                    texture,
                    frame,
                    tint,
                } => json!({
                    "type": "sprite",
                    "x": item.position.x,
                    "y": item.position.y,
                    "width": 32.0,
                    "height": 32.0,
                    "rotation": item.rotation,
                    "scaleX": item.scale.x,
                    "scaleY": item.scale.y,
                    "texture": texture,
                    "frame": frame,
                    "tint": {
                        "r": (tint[0] * 255.0) as u8,
                        "g": (tint[1] * 255.0) as u8,
                        "b": (tint[2] * 255.0) as u8,
                    },
                    "alpha": tint[3],
                    "layer": item.layer,
                    "source_entity": item.source_entity,
                }),
                DrawType::Mesh3D {
                    mesh,
                    material,
                    transform,
                    ..
                } => json!({
                    "type": "mesh3d",
                    "x": transform.position[0],
                    "y": transform.position[1],
                    "z": transform.position[2],
                    "rotation": {
                        "x": transform.rotation[0],
                        "y": transform.rotation[1],
                        "z": transform.rotation[2],
                        "w": transform.rotation[3],
                    },
                    "scale": {
                        "x": transform.scale[0],
                        "y": transform.scale[1],
                        "z": transform.scale[2],
                    },
                    "mesh": mesh,
                    "material": material,
                    "layer": item.layer,
                    "visible": item.visible,
                    "source_entity": item.source_entity,
                }),
                DrawType::Sprite3D {
                    texture,
                    frame,
                    tint,
                    transform,
                    billboard,
                } => json!({
                    "type": "sprite3d",
                    "x": transform.position[0],
                    "y": transform.position[1],
                    "z": transform.position[2],
                    "rotation": {
                        "x": transform.rotation[0],
                        "y": transform.rotation[1],
                        "z": transform.rotation[2],
                        "w": transform.rotation[3],
                    },
                    "scale": {
                        "x": transform.scale[0],
                        "y": transform.scale[1],
                        "z": transform.scale[2],
                    },
                    "billboard": billboard,
                    "texture": texture,
                    "frame": frame,
                    "tint": {
                        "r": (tint[0] * 255.0) as u8,
                        "g": (tint[1] * 255.0) as u8,
                        "b": (tint[2] * 255.0) as u8,
                    },
                    "alpha": tint[3],
                    "layer": item.layer,
                    "visible": item.visible,
                    "source_entity": item.source_entity,
                }),
            };
            commands.push(command);
        }

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

    /// Render a frame (posts to JavaScript)
    pub fn render(
        state: &RenderState,
        camera: &Camera,
        background_color: [f32; 4],
    ) {
        let json_str = Self::to_render_json(state, camera, background_color);
        Self::post_to_js("render", &json_str);
    }

    /// Post data to JavaScript side
    fn post_to_js(method: &str, data: &str) {
        #[cfg(target_arch = "wasm32")]
        {
            use web_sys::window;
            if let Some(window) = window() {
                let _ = window.eval(&format!(
                    "if (window.podRender) {{ window.podRender.{}('{}'); }}",
                    method,
                    data.replace("'", "\\'")
                ));
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (method, data);
        }
    }
}

/// JavaScript bridge (exposed for wasm_bindgen)
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
        // This would be called by Rust to trigger rendering
        log::debug!("Render frame: {}", json_data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::{RenderItem, DrawType};
    use glam::Vec2;

    #[test]
    fn test_render_frame_serialization() {
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

        let camera = Camera::new(Vec2::ZERO, 1280.0, 720.0);
        let json = WebRenderBridge::to_render_json(&state, &camera, [0.1, 0.1, 0.1, 1.0]);

        assert!(json.contains("\"type\":\"rect\""));
        assert!(json.contains("\"x\":100"));
        assert!(json.contains("\"y\":200"));
        assert!(json.contains("\"source_entity\":101"));
    }

    #[test]
    fn test_render_value_generation() {
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
    fn test_render_value_includes_sprite3d() {
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
                transform: crate::renderer::RenderTransform3D {
                    position: [0.0, 1.0, 3.5],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [0.75, 1.5, 1.0],
                },
                billboard: true,
            },
            visible: true,
            source_entity: Some(303),
        });

        let camera = Camera::new(Vec2::ZERO, 1280.0, 720.0);
        let value = WebRenderBridge::to_render_value(&state, &camera, [0.0, 0.0, 0.0, 1.0]);

        assert_eq!(value["commands"][0]["type"], "sprite3d");
        assert_eq!(value["commands"][0]["texture"], "npc_sprite");
        assert!(value["commands"][0]["visible"].as_bool().unwrap_or(false));
        assert_eq!(value["commands"][0]["source_entity"], 303);
    }
}
