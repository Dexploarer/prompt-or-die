//! # pod-render — Rendering abstraction layer
//!
//! Platform-agnostic rendering for Prompt or Die.
//! - Native: wgpu + winit (Vulkan/Metal/DX12)
//! - Web: wgpu web + PixiJS bridge (Rust computes state → JS renders)

pub mod renderer;
pub mod camera;
pub mod color;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

#[cfg(target_arch = "wasm32")]
pub mod web;

pub use camera::Camera;
pub use color::Color;
pub use renderer::{RenderState, RenderItem, DrawType};

/// Configuration for window creation
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    pub fullscreen: bool,
    pub resizable: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Prompt or Die".to_string(),
            width: 1280,
            height: 720,
            vsync: true,
            fullscreen: false,
            resizable: true,
        }
    }
}
