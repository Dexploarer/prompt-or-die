//! Native renderer for desktop platforms (wgpu + winit)
//! Supports Vulkan, Metal, and DirectX 12

use crate::renderer::{RenderState, DrawType};
use crate::camera::Camera;
use crate::WindowConfig;
use glam::Vec2;
use log::info;
use std::sync::Arc;
use wgpu::{Device, Queue, Surface, SurfaceConfiguration};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

/// Vertex format: position (2f) + uv (2f) + color (4f)
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

/// Simple 2D batch renderer for native platforms
pub struct NativeRenderer {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    window: Arc<Window>,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    max_vertices: usize,
    max_indices: usize,
    bind_group_layout: wgpu::BindGroupLayout,
    texture_cache: std::collections::HashMap<String, wgpu::Texture>,
}

impl NativeRenderer {
    /// Create a new native renderer
    pub async fn new(config: WindowConfig) -> Result<(Self, EventLoop<()>), String> {
        let event_loop = EventLoop::new().map_err(|e| format!("Failed to create event loop: {}", e))?;

        // Create window
        let window = WindowBuilder::new()
            .with_title(&config.title)
            .with_inner_size(PhysicalSize::new(config.width, config.height))
            .with_resizable(config.resizable)
            .with_fullscreen(if config.fullscreen {
                Some(winit::window::Fullscreen::Borderless(None))
            } else {
                None
            })
            .build(&event_loop)
            .map_err(|e| format!("Failed to create window: {}", e))?;

        let window = Arc::new(window);

        // Initialize wgpu
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("Failed to create surface: {}", e))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback: false,
            })
            .await
            .ok_or("Failed to find suitable adapter")?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .map_err(|e| format!("Failed to request device: {}", e))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| surface_caps.formats.first().copied())
            .ok_or("No supported surface format")?;

        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: config.width,
            height: config.height,
            present_mode: if config.vsync {
                wgpu::PresentMode::Fifo
            } else {
                wgpu::PresentMode::Immediate
            },
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        // Create shader
        let shader_code = r#"
struct VertexInput {
    @location(0) position: vec2f,
    @location(1) uv: vec2f,
    @location(2) color: vec4f,
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4f(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    return input.color;
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Render Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_code.into()),
        });

        // Create bind group layout for textures
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Float32x4,
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        // Create buffers
        let max_vertices = 100_000;
        let max_indices = 150_000;

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: (max_vertices * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Index Buffer"),
            size: (max_indices * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        info!("Native renderer initialized ({}x{})", config.width, config.height);

        Ok((
            Self {
                surface,
                device,
                queue,
                config,
                window,
                pipeline,
                vertex_buffer,
                index_buffer,
                max_vertices,
                max_indices,
                bind_group_layout,
                texture_cache: Default::default(),
            },
            event_loop,
        ))
    }

    /// Render a frame
    pub fn render(&mut self, state: &RenderState, camera: &Camera) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        // Build vertex/index buffers from render state
        let (vertices, indices) = self.build_geometry(state, camera);

        if !vertices.is_empty() && !indices.is_empty() {
            // Upload buffers
            self.queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&vertices),
            );
            self.queue.write_buffer(
                &self.index_buffer,
                0,
                bytemuck::cast_slice(&indices),
            );

            // Render pass
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.1,
                                b: 0.1,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                render_pass.set_pipeline(&self.pipeline);
                render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                render_pass.set_index_buffer(
                    self.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                render_pass.draw_indexed(0..(indices.len() as u32), 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Handle window resize
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            info!("Renderer resized to {}x{}", new_size.width, new_size.height);
        }
    }

    /// Get window reference
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Get device reference
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Get queue reference
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    // Build geometry from render state
    fn build_geometry(&self, state: &RenderState, camera: &Camera) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let projection = camera.projection_matrix();
        let viewport = (camera.viewport_width, camera.viewport_height);

        for item in &state.items {
            if !item.visible {
                continue;
            }

            let base_index = vertices.len() as u32;

            match &item.draw_type {
                DrawType::Rect {
                    width,
                    height,
                    color,
                } => {
                    // Create quad
                    let hw = width * item.scale.x / 2.0;
                    let hh = height * item.scale.y / 2.0;

                    // Transform positions through camera
                    let cos_r = item.rotation.cos();
                    let sin_r = item.rotation.sin();

                    let positions = [
                        (-hw, -hh),
                        (hw, -hh),
                        (hw, hh),
                        (-hw, hh),
                    ];

                    for (x, y) in &positions {
                        let rx = x * cos_r - y * sin_r;
                        let ry = x * sin_r + y * cos_r;
                        let world = Vec2::new(item.position.x + rx, item.position.y + ry);
                        let screen = camera.world_to_screen(world);

                        vertices.push(Vertex {
                            position: [
                                (screen.x / viewport.0) * 2.0 - 1.0,
                                1.0 - (screen.y / viewport.1) * 2.0,
                            ],
                            uv: [0.0, 0.0], // Not used for colored quads
                            color: *color,
                        });
                    }

                    // Quad indices (2 triangles)
                    indices.extend_from_slice(&[
                        base_index,
                        base_index + 1,
                        base_index + 2,
                        base_index,
                        base_index + 2,
                        base_index + 3,
                    ]);
                }
                DrawType::Sprite {
                    texture: _,
                    frame: _,
                    tint,
                } => {
                    // Similar to rect but with texture coordinates
                    let hw = 32.0 * item.scale.x / 2.0; // Default sprite size
                    let hh = 32.0 * item.scale.y / 2.0;

                    let cos_r = item.rotation.cos();
                    let sin_r = item.rotation.sin();

                    let positions = [
                        (-hw, -hh),
                        (hw, -hh),
                        (hw, hh),
                        (-hw, hh),
                    ];

                    let uvs = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

                    for (i, (x, y)) in positions.iter().enumerate() {
                        let rx = x * cos_r - y * sin_r;
                        let ry = x * sin_r + y * cos_r;
                        let world = Vec2::new(item.position.x + rx, item.position.y + ry);
                        let screen = camera.world_to_screen(world);

                        vertices.push(Vertex {
                            position: [
                                (screen.x / viewport.0) * 2.0 - 1.0,
                                1.0 - (screen.y / viewport.1) * 2.0,
                            ],
                            uv: uvs[i],
                            color: *tint,
                        });
                    }

                    indices.extend_from_slice(&[
                        base_index,
                        base_index + 1,
                        base_index + 2,
                        base_index,
                        base_index + 2,
                        base_index + 3,
                    ]);
                }
            }
        }

        (vertices, indices)
    }
}

/// Application handler for event loop
pub struct RenderApp {
    renderer: Option<NativeRenderer>,
    camera: Camera,
    render_state: RenderState,
    should_close: bool,
}

impl RenderApp {
    pub fn new() -> Self {
        Self {
            renderer: None,
            camera: Camera::new(Vec2::ZERO, 1280.0, 720.0),
            render_state: RenderState::new(),
            should_close: false,
        }
    }

    pub fn set_renderer(&mut self, renderer: NativeRenderer) {
        self.renderer = Some(renderer);
    }

    pub fn renderer(&mut self) -> Option<&mut NativeRenderer> {
        self.renderer.as_mut()
    }

    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    pub fn render_state_mut(&mut self) -> &mut RenderState {
        &mut self.render_state
    }

    pub fn should_close(&self) -> bool {
        self.should_close
    }
}

impl ApplicationHandler for RenderApp {
    fn resumed(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.should_close = true;
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = &mut self.renderer {
                    let _ = renderer.render(&self.render_state, &self.camera);
                }
            }
            _ => {}
        }
    }
}
