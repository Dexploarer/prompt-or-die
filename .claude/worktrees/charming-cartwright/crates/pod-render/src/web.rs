//! Web renderer bridge for browser platforms
//! Serializes render state to JSON for PixiJS consumption on JS side

use crate::renderer::{RenderState, DrawType};
use crate::camera::Camera;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Serializable render command for PixiJS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCommand {
    pub item_type: String, // "rect" or "sprite"
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
    pub layer: i32,
    pub visible: bool,
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
                    layer: item.layer,
                    visible: item.visible,
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
        });

        let camera = Camera::new(Vec2::ZERO, 1280.0, 720.0);
        let json = WebRenderBridge::to_render_json(&state, &camera, [0.1, 0.1, 0.1, 1.0]);

        assert!(json.contains("\"type\":\"rect\""));
        assert!(json.contains("\"x\":100"));
        assert!(json.contains("\"y\":200"));
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
        });

        let camera = Camera::new(Vec2::new(100.0, 100.0), 1280.0, 720.0);
        let value = WebRenderBridge::to_render_value(&state, &camera, [0.0, 0.0, 0.0, 1.0]);

        assert_eq!(value["camera"]["zoom"], 1.0);
        assert_eq!(value["commands"][0]["type"], "sprite");
        assert_eq!(value["commands"][0]["texture"], "player");
    }
}
