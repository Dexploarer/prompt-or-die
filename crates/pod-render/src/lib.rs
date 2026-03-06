//! # pod-render — Rendering abstraction layer
//!
//! Platform-agnostic rendering for Prompt or Die.
//! - Native: wgpu + winit (Vulkan/Metal/DX12)
//! - Web: wgpu web + PixiJS bridge (Rust computes state → JS renders)

#![allow(clippy::manual_strip)]
#![allow(clippy::new_without_default)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::unnecessary_cast)]

pub mod camera;
pub mod color;
pub mod gltf_import;
pub mod renderer;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

#[cfg(any(target_arch = "wasm32", test))]
pub mod web;

pub use camera::Camera;
pub use color::Color;
pub use renderer::{DrawType, RenderItem, RenderState};

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
