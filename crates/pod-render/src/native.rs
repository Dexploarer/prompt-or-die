//! Native renderer for desktop platforms (wgpu + winit)
//! Supports Vulkan, Metal, and DirectX 12.

use crate::camera::Camera;
use crate::renderer::{DrawType, RenderState};
use crate::WindowConfig;
use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use log::info;
use std::mem;
use std::sync::Arc;
use wgpu::{Device, Queue, Surface, SurfaceConfiguration};
use wgpu::{BindGroupLayoutDescriptor, BindingType, BufferUsages, ShaderStages};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::EventLoop;
use winit::window::{Fullscreen, Window};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex2D {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex3D {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

const MAX_2D_VERTICES: usize = 100_000;
const MAX_2D_INDICES: usize = 150_000;
const MAX_3D_VERTICES: usize = 24 * 512;
const MAX_3D_INDICES: usize = 36 * 512;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Debug, Clone)]
struct DrawBatch {
    material: String,
    mesh: String,
    start_index: u32,
    index_count: u32,
}

#[derive(Debug, Clone)]
enum ThreeDPrimitiveKind {
    Mesh3D,
    Sprite3D,
}

#[derive(Debug, Clone)]
struct ThreeDPrimitive {
    material: String,
    mesh: String,
    layer: i32,
    transform_position: [f32; 3],
    transform_rotation: [f32; 4],
    transform_scale: [f32; 3],
    color: [f32; 4],
    kind: ThreeDPrimitiveKind,
    billboard: bool,
}

#[rustfmt::skip]
const CUBE_VERTICES: [Vertex3D; 24] = [
    // Front
    Vertex3D { position: [-0.5, -0.5,  0.5], normal: [0.0, 0.0, 1.0], color: [1.0, 0.6, 0.2, 1.0] },
    Vertex3D { position: [ 0.5, -0.5,  0.5], normal: [0.0, 0.0, 1.0], color: [1.0, 0.6, 0.2, 1.0] },
    Vertex3D { position: [ 0.5,  0.5,  0.5], normal: [0.0, 0.0, 1.0], color: [1.0, 0.6, 0.2, 1.0] },
    Vertex3D { position: [-0.5,  0.5,  0.5], normal: [0.0, 0.0, 1.0], color: [1.0, 0.6, 0.2, 1.0] },
    // Back
    Vertex3D { position: [ 0.5, -0.5, -0.5], normal: [0.0, 0.0, -1.0], color: [0.2, 0.6, 1.0, 1.0] },
    Vertex3D { position: [-0.5, -0.5, -0.5], normal: [0.0, 0.0, -1.0], color: [0.2, 0.6, 1.0, 1.0] },
    Vertex3D { position: [-0.5,  0.5, -0.5], normal: [0.0, 0.0, -1.0], color: [0.2, 0.6, 1.0, 1.0] },
    Vertex3D { position: [ 0.5,  0.5, -0.5], normal: [0.0, 0.0, -1.0], color: [0.2, 0.6, 1.0, 1.0] },
    // Top
    Vertex3D { position: [-0.5,  0.5,  0.5], normal: [0.0, 1.0, 0.0], color: [0.2, 1.0, 0.4, 1.0] },
    Vertex3D { position: [ 0.5,  0.5,  0.5], normal: [0.0, 1.0, 0.0], color: [0.2, 1.0, 0.4, 1.0] },
    Vertex3D { position: [ 0.5,  0.5, -0.5], normal: [0.0, 1.0, 0.0], color: [0.2, 1.0, 0.4, 1.0] },
    Vertex3D { position: [-0.5,  0.5, -0.5], normal: [0.0, 1.0, 0.0], color: [0.2, 1.0, 0.4, 1.0] },
    // Bottom
    Vertex3D { position: [-0.5, -0.5, -0.5], normal: [0.0, -1.0, 0.0], color: [1.0, 0.2, 0.2, 1.0] },
    Vertex3D { position: [ 0.5, -0.5, -0.5], normal: [0.0, -1.0, 0.0], color: [1.0, 0.2, 0.2, 1.0] },
    Vertex3D { position: [ 0.5, -0.5,  0.5], normal: [0.0, -1.0, 0.0], color: [1.0, 0.2, 0.2, 1.0] },
    Vertex3D { position: [-0.5, -0.5,  0.5], normal: [0.0, -1.0, 0.0], color: [1.0, 0.2, 0.2, 1.0] },
    // Right
    Vertex3D { position: [ 0.5, -0.5,  0.5], normal: [1.0, 0.0, 0.0], color: [0.8, 0.8, 0.8, 1.0] },
    Vertex3D { position: [ 0.5, -0.5, -0.5], normal: [1.0, 0.0, 0.0], color: [0.8, 0.8, 0.8, 1.0] },
    Vertex3D { position: [ 0.5,  0.5, -0.5], normal: [1.0, 0.0, 0.0], color: [0.8, 0.8, 0.8, 1.0] },
    Vertex3D { position: [ 0.5,  0.5,  0.5], normal: [1.0, 0.0, 0.0], color: [0.8, 0.8, 0.8, 1.0] },
    // Left
    Vertex3D { position: [-0.5, -0.5, -0.5], normal: [-1.0, 0.0, 0.0], color: [0.8, 0.4, 0.4, 1.0] },
    Vertex3D { position: [-0.5, -0.5,  0.5], normal: [-1.0, 0.0, 0.0], color: [0.8, 0.4, 0.4, 1.0] },
    Vertex3D { position: [-0.5,  0.5,  0.5], normal: [-1.0, 0.0, 0.0], color: [0.8, 0.4, 0.4, 1.0] },
    Vertex3D { position: [-0.5,  0.5, -0.5], normal: [-1.0, 0.0, 0.0], color: [0.8, 0.4, 0.4, 1.0] },
];

#[rustfmt::skip]
const CUBE_INDICES: [u32; 36] = [
    0,  1,  2,  0,  2,  3,
    4,  5,  6,  4,  6,  7,
    8,  9, 10,  8, 10, 11,
    12, 13, 14, 12, 14, 15,
    16, 17, 18, 16, 18, 19,
    20, 21, 22, 20, 22, 23,
];

#[rustfmt::skip]
const SPRITE3D_VERTICES: [Vertex3D; 4] = [
    Vertex3D {
        position: [-0.5, 0.0, -0.5],
        normal: [0.0, 0.0, 1.0],
        color: [1.0, 1.0, 1.0, 1.0],
    },
    Vertex3D {
        position: [0.5, 0.0, -0.5],
        normal: [0.0, 0.0, 1.0],
        color: [1.0, 1.0, 1.0, 1.0],
    },
    Vertex3D {
        position: [0.5, 0.0, 0.5],
        normal: [0.0, 0.0, 1.0],
        color: [1.0, 1.0, 1.0, 1.0],
    },
    Vertex3D {
        position: [-0.5, 0.0, 0.5],
        normal: [0.0, 0.0, 1.0],
        color: [1.0, 1.0, 1.0, 1.0],
    },
];

const SPRITE3D_INDICES: [u32; 6] = [0, 1, 2, 0, 2, 3];

/// Native renderer with a forward 3D pass (depth-buffered) and a legacy 2D pass.
pub struct NativeRenderer {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    window: Arc<Window>,

    pipeline_2d: wgpu::RenderPipeline,
    pipeline_3d: wgpu::RenderPipeline,

    vertex_buffer_2d: wgpu::Buffer,
    index_buffer_2d: wgpu::Buffer,
    max_vertices_2d: usize,
    max_indices_2d: usize,

    vertex_buffer_3d: wgpu::Buffer,
    index_buffer_3d: wgpu::Buffer,
    max_vertices_3d: usize,
    max_indices_3d: usize,

    camera_uniform_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl NativeRenderer {
    pub async fn new(config: WindowConfig) -> Result<(Self, EventLoop<()>), String> {
        let event_loop = EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;

        let window = {
            #[allow(deprecated)]
            event_loop.create_window(
                Window::default_attributes()
                    .with_title(&config.title)
                    .with_inner_size(PhysicalSize::new(config.width, config.height))
                    .with_resizable(config.resizable),
            )
        }
        .map_err(|e| format!("Failed to create window: {e}"))?;
        window.set_fullscreen(if config.fullscreen {
            Some(Fullscreen::Borderless(None))
        } else {
            None
        });

        let window = Arc::new(window);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("Failed to create surface: {e}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or("Failed to find suitable adapter")?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .map_err(|e| format!("Failed to request device: {e}"))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
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
                wgpu::PresentMode::Mailbox
            },
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        let shader_2d = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("2D Shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
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
    _ = input.uv;
    return input.color;
}
"#
                .into(),
            ),
        });

        let shader_3d = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3D Shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) color: vec4f,
}

struct CameraUniform {
    view_proj: mat4x4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) color: vec4f,
    @location(1) normal: vec3f,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = camera.view_proj * vec4f(input.position, 1.0);
    output.color = input.color;
    output.normal = normalize(input.normal);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    let light_dir = normalize(vec3f(0.2, -0.8, -1.0));
    let diffuse = max(dot(input.normal, -light_dir), 0.15);
    return vec4f(input.color.rgb * diffuse, input.color.a);
}
"#
                .into(),
            ),
        });

        let pipeline_layout_2d = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("2D Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let pipeline_2d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("2D Pipeline"),
            layout: Some(&pipeline_layout_2d),
            vertex: wgpu::VertexState {
                module: &shader_2d,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<Vertex2D>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Float32x4,
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
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
                module: &shader_2d,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });

        let camera_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("3D Camera Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout_3d = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3D Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline_3d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3D Pipeline"),
            layout: Some(&pipeline_layout_3d),
            vertex: wgpu::VertexState {
                module: &shader_3d,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<Vertex3D>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3,
                        1 => Float32x3,
                        2 => Float32x4,
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader_3d,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });

        let vertex_buffer_2d = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("2D Vertex Buffer"),
            size: (MAX_2D_VERTICES * mem::size_of::<Vertex2D>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer_2d = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("2D Index Buffer"),
            size: (MAX_2D_INDICES * mem::size_of::<u32>()) as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let vertex_buffer_3d = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3D Vertex Buffer"),
            size: (MAX_3D_VERTICES * mem::size_of::<Vertex3D>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer_3d = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3D Index Buffer"),
            size: (MAX_3D_INDICES * mem::size_of::<u32>()) as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_uniform = CameraUniform {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
        };
        let camera_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Uniform Buffer"),
            size: mem::size_of::<CameraUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&camera_uniform_buffer, 0, bytemuck::cast_slice(&[camera_uniform]));

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3D Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_uniform_buffer.as_entire_binding(),
            }],
        });

        let (depth_texture, depth_view) =
            Self::create_depth_texture(&device, config.width, config.height);

        info!("Native renderer initialized ({}x{})", config.width, config.height);

        Ok((
            Self {
                surface,
                device,
                queue,
                config,
                window,
                pipeline_2d,
                pipeline_3d,
                vertex_buffer_2d,
                index_buffer_2d,
                max_vertices_2d: MAX_2D_VERTICES,
                max_indices_2d: MAX_2D_INDICES,
                vertex_buffer_3d,
                index_buffer_3d,
                max_vertices_3d: MAX_3D_VERTICES,
                max_indices_3d: MAX_3D_INDICES,
                camera_uniform_buffer,
                camera_bind_group,
                depth_texture,
                depth_view,
            },
            event_loop,
        ))
    }

    fn create_depth_texture(device: &Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(DEPTH_FORMAT),
            ..Default::default()
        });

        (depth_texture, depth_view)
    }

    fn color_clear() -> wgpu::Color {
        wgpu::Color {
            r: 0.1,
            g: 0.1,
            b: 0.1,
            a: 1.0,
        }
    }

    fn compute_camera_matrices(&self, camera: &Camera) -> (Mat4, Mat4) {
        let viewport = self.viewport_size();
        let aspect = if viewport.y > 0.0 {
            (viewport.x / viewport.y).max(1.0 / 16.0)
        } else {
            16.0 / 9.0
        };

        let fov = (60.0f32 / camera.zoom.max(0.001)).to_radians();
        let proj = Mat4::perspective_rh_gl(fov, aspect, 0.05, 10_000.0);
        let camera_pos = Vec3::new(camera.position.x, camera.position.y, 8.0 / camera.zoom.max(0.001));
        let target = Vec3::new(camera.position.x, camera.position.y, 0.0);
        let view = Mat4::look_at_rh(camera_pos, target, Vec3::Y);
        let view_proj = proj * view;

        (view, view_proj)
    }

    fn compute_camera_uniform(&self, view_proj: &Mat4) -> CameraUniform {
        CameraUniform {
            view_proj: view_proj.to_cols_array_2d(),
        }
    }

    fn viewport_size(&self) -> Vec2 {
        Vec2::new(self.config.width as f32, self.config.height as f32)
    }

    fn cube_instance_color(&self, mesh: &str, material: &str) -> [f32; 4] {
        let mix = mesh.bytes().chain(material.bytes()).fold(0u64, |acc, byte| {
            acc.wrapping_mul(167).wrapping_add(u64::from(byte))
        });
        let r = ((mix & 0xFF) as f32) / 255.0;
        let g = (((mix >> 8) & 0xFF) as f32) / 255.0;
        let b = (((mix >> 16) & 0xFF) as f32) / 255.0;
        [0.15 + 0.7 * r, 0.15 + 0.7 * g, 0.15 + 0.7 * b, 1.0]
    }

    fn estimate_primitive_radius(kind: &ThreeDPrimitiveKind, scale: Vec3) -> f32 {
        let max_scale = scale.abs().max_element();
        match kind {
            ThreeDPrimitiveKind::Mesh3D => max_scale * 0.75,
            ThreeDPrimitiveKind::Sprite3D => max_scale * 0.8,
        }
    }

    fn frustum_cull_sphere(
        world_position: Vec3,
        radius: f32,
        view_proj: &Mat4,
    ) -> bool {
        let clip = *view_proj * Vec4::new(world_position.x, world_position.y, world_position.z, 1.0);
        if clip.w <= 0.0 {
            return false;
        }

        let inv_w = 1.0 / clip.w;
        let ndc = Vec3::new(clip.x * inv_w, clip.y * inv_w, clip.z * inv_w);
        let projected_radius = radius / clip.w.abs();

        ndc.x >= -1.0 - projected_radius
            && ndc.x <= 1.0 + projected_radius
            && ndc.y >= -1.0 - projected_radius
            && ndc.y <= 1.0 + projected_radius
            && ndc.z >= -1.0 - projected_radius
            && ndc.z <= 1.0 + projected_radius
    }

    fn billboard_axes(view_matrix: &Mat4) -> (Vec3, Vec3) {
        let inv_view = view_matrix.inverse();
        (inv_view.x_axis.truncate(), inv_view.y_axis.truncate())
    }

    fn push_3d_batch(
        batches: &mut Vec<DrawBatch>,
        material: &str,
        mesh: &str,
        start_index: u32,
        index_count: u32,
    ) {
        match batches.last_mut() {
            Some(last) if last.material == material && last.mesh == mesh => {
                last.index_count += index_count;
            }
            _ => batches.push(DrawBatch {
                material: material.to_string(),
                mesh: mesh.to_string(),
                start_index,
                index_count,
            }),
        }
    }

    pub fn render(&mut self, state: &RenderState, camera: &Camera) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let clear_color = Self::color_clear();
        let (view_matrix, view_proj_matrix) = self.compute_camera_matrices(camera);
        let camera_uniform = self.compute_camera_uniform(&view_proj_matrix);
        let (vertices_2d, indices_2d) = self.build_geometry_2d(state, camera);
        let (vertices_3d, indices_3d, draw_batches_3d) =
            self.build_geometry_3d(state, &view_matrix, &view_proj_matrix);

        if !vertices_2d.is_empty() && !indices_2d.is_empty() {
            self.queue.write_buffer(&self.vertex_buffer_2d, 0, bytemuck::cast_slice(&vertices_2d));
            self.queue.write_buffer(&self.index_buffer_2d, 0, bytemuck::cast_slice(&indices_2d));
        }

        if !vertices_3d.is_empty() && !indices_3d.is_empty() {
            self.queue.write_buffer(&self.vertex_buffer_3d, 0, bytemuck::cast_slice(&vertices_3d));
            self.queue.write_buffer(&self.index_buffer_3d, 0, bytemuck::cast_slice(&indices_3d));
        }

        self.queue
            .write_buffer(&self.camera_uniform_buffer, 0, bytemuck::cast_slice(&[camera_uniform]));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let has_2d = !vertices_2d.is_empty() && !indices_2d.is_empty();
        let has_3d = !vertices_3d.is_empty() && !indices_3d.is_empty();

        if has_3d {
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("3D Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear_color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                pass.set_pipeline(&self.pipeline_3d);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer_3d.slice(..));
                pass.set_index_buffer(self.index_buffer_3d.slice(..), wgpu::IndexFormat::Uint32);

                for batch in &draw_batches_3d {
                    if batch.index_count == 0 {
                        continue;
                    }

                    let end = batch.start_index + batch.index_count;
                    pass.draw_indexed(batch.start_index..end, 0, 0..1);
                }
            }
        }

        if has_2d {
            let load_op = if has_3d {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(clear_color)
            };

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("2D Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: load_op,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                pass.set_pipeline(&self.pipeline_2d);
                pass.set_vertex_buffer(0, self.vertex_buffer_2d.slice(..));
                pass.set_index_buffer(self.index_buffer_2d.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..(indices_2d.len() as u32), 0, 0..1);
            }
        } else if !has_3d {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline_2d);
            drop(pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Handle window resize.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }

        if self.config.width != new_size.width || self.config.height != new_size.height {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            let (depth_texture, depth_view) =
                Self::create_depth_texture(&self.device, new_size.width, new_size.height);
            self.depth_texture = depth_texture;
            self.depth_view = depth_view;
            info!("Renderer resized to {}x{}", new_size.width, new_size.height);
        }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    fn build_geometry_2d(&self, state: &RenderState, camera: &Camera) -> (Vec<Vertex2D>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let viewport = (camera.viewport_width, camera.viewport_height);

        for item in &state.items {
            if !item.visible {
                continue;
            }

            if vertices.len() + 4 > self.max_vertices_2d || indices.len() + 6 > self.max_indices_2d {
                break;
            }

            let base_index = vertices.len() as u32;

            match &item.draw_type {
                DrawType::Rect { width, height, color } => {
                    let hw = width * item.scale.x / 2.0;
                    let hh = height * item.scale.y / 2.0;
                    let cos_r = item.rotation.cos();
                    let sin_r = item.rotation.sin();
                    let points = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];

                    for (x, y) in points {
                        let rx = x * cos_r - y * sin_r;
                        let ry = x * sin_r + y * cos_r;
                        let world = Vec2::new(item.position.x + rx, item.position.y + ry);
                        let screen = camera.world_to_screen(world);
                        vertices.push(Vertex2D {
                            position: [
                                (screen.x / viewport.0) * 2.0 - 1.0,
                                1.0 - (screen.y / viewport.1) * 2.0,
                            ],
                            uv: [0.0, 0.0],
                            color: *color,
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
                DrawType::Sprite {
                    texture: _,
                    frame: _,
                    tint,
                } => {
                    let hw = 32.0 * item.scale.x / 2.0;
                    let hh = 32.0 * item.scale.y / 2.0;
                    let cos_r = item.rotation.cos();
                    let sin_r = item.rotation.sin();
                    let points = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
                    let uvs = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

                    for (i, (x, y)) in points.iter().enumerate() {
                        let rx = x * cos_r - y * sin_r;
                        let ry = x * sin_r + y * cos_r;
                        let world = Vec2::new(item.position.x + rx, item.position.y + ry);
                        let screen = camera.world_to_screen(world);
                        vertices.push(Vertex2D {
                            position: [
                                (screen.x / viewport.0) * 2.0 - 1.0,
                                1.0 - (screen.y / viewport.1) * 2.0,
                            ],
                            uv: [uvs[i].0, uvs[i].1],
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
                DrawType::Sprite3D { .. } => {}
                DrawType::Mesh3D { .. } => {}
            }
        }

        (vertices, indices)
    }

    fn build_geometry_3d(
        &self,
        state: &RenderState,
        view_matrix: &Mat4,
        view_proj_matrix: &Mat4,
    ) -> (Vec<Vertex3D>, Vec<u32>, Vec<DrawBatch>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut batches = Vec::new();
        let mut primitives = Vec::new();

        let (billboard_right, billboard_up) = Self::billboard_axes(view_matrix);

        for item in &state.items {
            if !item.visible {
                continue;
            }

            match &item.draw_type {
                DrawType::Mesh3D {
                    mesh,
                    material,
                    transform,
                    ..
                } => {
                    let position = Vec3::from(transform.position);
                    let scale = Vec3::from(transform.scale);
                    let radius = Self::estimate_primitive_radius(&ThreeDPrimitiveKind::Mesh3D, scale);
                    if !Self::frustum_cull_sphere(position, radius, view_proj_matrix) {
                        continue;
                    }

                    primitives.push(ThreeDPrimitive {
                        material: material.clone(),
                        mesh: mesh.clone(),
                        layer: item.layer,
                        transform_position: transform.position,
                        transform_rotation: transform.rotation,
                        transform_scale: transform.scale,
                        color: self.cube_instance_color(mesh, material),
                        kind: ThreeDPrimitiveKind::Mesh3D,
                        billboard: false,
                    });
                }
                DrawType::Sprite3D {
                    texture,
                    tint,
                    transform,
                    billboard,
                    ..
                } => {
                    let position = Vec3::from(transform.position);
                    let scale = Vec3::from(transform.scale);
                    let radius =
                        Self::estimate_primitive_radius(&ThreeDPrimitiveKind::Sprite3D, scale);
                    if !Self::frustum_cull_sphere(position, radius, view_proj_matrix) {
                        continue;
                    }

                    primitives.push(ThreeDPrimitive {
                        material: texture.clone(),
                        mesh: "sprite3d".to_string(),
                        layer: item.layer,
                        transform_position: transform.position,
                        transform_rotation: transform.rotation,
                        transform_scale: transform.scale,
                        color: *tint,
                        kind: ThreeDPrimitiveKind::Sprite3D,
                        billboard: *billboard,
                    });
                }
                _ => {}
            }
        }

        primitives.sort_by(|a, b| {
            a.layer
                .cmp(&b.layer)
                .then_with(|| a.material.cmp(&b.material))
                .then_with(|| a.mesh.cmp(&b.mesh))
        });

        for primitive in primitives {
            let (template_vertices, template_indices) = match primitive.kind {
                ThreeDPrimitiveKind::Mesh3D => (CUBE_VERTICES.as_ref(), CUBE_INDICES.as_ref()),
                ThreeDPrimitiveKind::Sprite3D => (SPRITE3D_VERTICES.as_ref(), SPRITE3D_INDICES.as_ref()),
            };

            if vertices.len() + template_vertices.len() > self.max_vertices_3d
                || indices.len() + template_indices.len() > self.max_indices_3d
            {
                break;
            }

            let base_index = vertices.len() as u32;
            let position = Vec3::from(primitive.transform_position);
            let rotation = Quat::from_xyzw(
                primitive.transform_rotation[0],
                primitive.transform_rotation[1],
                primitive.transform_rotation[2],
                primitive.transform_rotation[3],
            );
            let scale = Vec3::from(primitive.transform_scale);

            match primitive.kind {
                ThreeDPrimitiveKind::Mesh3D => {
                    let model = Mat4::from_translation(position)
                        * Mat4::from_quat(rotation)
                        * Mat4::from_scale(scale);
                    let normal_matrix = Mat4::from_quat(rotation);

                    for base_vertex in template_vertices {
                        let local_pos = Vec3::from(base_vertex.position);
                        let local_normal = Vec3::from(base_vertex.normal);
                        let world_pos = model.transform_point3(local_pos);
                        let world_normal = normal_matrix.transform_vector3(local_normal);
                        vertices.push(Vertex3D {
                            position: world_pos.to_array(),
                            normal: world_normal.to_array(),
                            color: primitive.color,
                        });
                    }
                }
                ThreeDPrimitiveKind::Sprite3D => {
                    if primitive.billboard {
                        for base_vertex in template_vertices {
                            let local = Vec3::from(base_vertex.position);
                            let billboard_pos = position
                                + billboard_right * (local.x * scale.x)
                                + billboard_up * (local.z * scale.y);
                            let facing = billboard_right.cross(billboard_up).normalize_or_zero();
                            vertices.push(Vertex3D {
                                position: billboard_pos.to_array(),
                                normal: facing.to_array(),
                                color: primitive.color,
                            });
                        }
                    } else {
                        let model = Mat4::from_translation(position)
                            * Mat4::from_quat(rotation)
                            * Mat4::from_scale(scale);
                        let normal_matrix = Mat4::from_quat(rotation);

                        for base_vertex in template_vertices {
                            let local_pos = Vec3::from(base_vertex.position);
                            let local_normal = Vec3::from(base_vertex.normal);
                            let world_pos = model.transform_point3(local_pos);
                            let world_normal = normal_matrix.transform_vector3(local_normal);
                            vertices.push(Vertex3D {
                                position: world_pos.to_array(),
                                normal: world_normal.to_array(),
                                color: primitive.color,
                            });
                        }
                    }
                }
            }

            indices.extend(template_indices.iter().map(|idx| base_index + idx));
            Self::push_3d_batch(
                &mut batches,
                &primitive.material,
                &primitive.mesh,
                base_index,
                template_indices.len() as u32,
            );
        }

        (vertices, indices, batches)
    }
}

/// Application handler for event loop.
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
                self.camera.set_viewport(size.width as f32, size.height as f32);
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

#[cfg(test)]
mod tests {
    #[test]
    fn headless_wgpu_device_is_available_for_ci() {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            });

            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .expect("headless CI should expose a wgpu adapter");

            let info = adapter.get_info();
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("headless adapter should create a wgpu device");

            assert!(
                !info.name.trim().is_empty(),
                "wgpu adapter should report a backend name"
            );
            drop(queue);
            drop(device);
        });
    }
}
