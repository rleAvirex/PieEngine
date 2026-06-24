//! Editor viewport renderer — GPU pipeline, mesh/texture/material upload, and rendering.

use std::path::Path;

/// Quality level for the sky atmosphere ray-marching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkyQuality {
    /// Sky atmosphere disabled — flat clear color only.
    Off,
    /// Low quality: 8 ray steps, 4 sun steps. Fast, visible banding.
    Low,
    /// Medium quality: 16 ray steps, 8 sun steps. Good balance.
    Medium,
    /// High quality: 32 ray steps, 16 sun steps. Smooth, expensive.
    High,
}

impl SkyQuality {
    pub fn sample_counts(self) -> (u32, u32) {
        match self {
            SkyQuality::Off => (0, 0),
            SkyQuality::Low => (8, 4),
            SkyQuality::Medium => (16, 8),
            SkyQuality::High => (32, 16),
        }
    }
}
use std::sync::Arc;

use egui::TextureId;
use glam::{Mat4, Vec3};
use hecs::Entity;
use pie_runtime::assets::{
    AssetRegistry, MaterialAsset, MaterialHandle, MeshAsset, MeshVertex, load_shader_named,
};
use pie_runtime::components::{Camera, DirectionalLight, SkyLight, Transform};
use pie_runtime::rendering::{CameraUniform, camera_view_proj};
use wgpu::util::DeviceExt;

use crate::gizmo::{
    Axis, GIZMO_WORLD_SCALE, GizmoState, GizmoVertex, build_fbx_gizmo_mesh, build_gizmo_mesh,
};
use crate::theme;

// ---------------------------------------------------------------------------
// Depth texture helpers
// ---------------------------------------------------------------------------

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Maximum number of drawables the editor viewport can render in a single frame.
///
/// Each drawable occupies one slot in the model uniform buffer; if the scene
/// exceeds this, the extras are skipped with a warning. Sized to cover any
/// realistic editor scene without over-allocating GPU memory.
const MAX_DRAWABLES: usize = 4096;

/// Stride between drawable slots in the model uniform buffer, and between
/// cubemap face slots in the cubemap-camera buffer.
///
/// Both `EditorModelUniform` (128 bytes) and `CameraUniform` (144 bytes) are
/// smaller than wgpu's `min_uniform_buffer_offset_alignment` (256 on most
/// GPUs). We round each slot up to 256 so per-draw / per-face dynamic offsets
/// stay 256-aligned.
const UNIFORM_BUFFER_STRIDE: usize = 256;

fn create_editor_depth_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("editor viewport depth texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_hdr_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("editor HDR render target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

// ---------------------------------------------------------------------------
// Viewport texture
// ---------------------------------------------------------------------------

pub struct EditorViewportTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub texture_id: TextureId,
    pub size: [u32; 2],
}

// ---------------------------------------------------------------------------
// GPU types
// ---------------------------------------------------------------------------

struct EditorGpuMaterial {
    _buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    _texture: Option<wgpu::Texture>,
    _normal_texture: Option<wgpu::Texture>,
}

struct EditorGpuDrawable {
    entity: Entity,
    mesh: EditorGpuMesh,
    material: MaterialHandle,
}

struct EditorGpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EditorModelUniform {
    model: [[f32; 4]; 4],
    normal_matrix: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EditorMaterialUniform {
    base_color_factor: [f32; 4],
    parameters: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EditorLightUniform {
    direction: [f32; 4],
    color: [f32; 4],
    // Sky light intensity (scalar multiplier for IBL contribution).
    // Stored as vec4 for 16-byte alignment; xyz unused.
    sky_light: [f32; 4],
}

/// Sky atmosphere parameters — must match sky_atmosphere.wgsl SkyParams layout.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EditorSkyUniform {
    sun_direction: [f32; 4],
    sun_intensity: f32,
    rayleigh_coefficient: f32,
    mie_coefficient: f32,
    rayleigh_scale_height: f32,
    mie_scale_height: f32,
    mie_directionality: f32,
    planet_radius: f32,
    atmosphere_radius: f32,
    ray_samples: u32,
    mie_samples: u32,
    _padding: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LineCameraUniform {
    view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LineColorUniform {
    color: [f32; 4],
}

/// Number of vertices for thick AABB wireframe (12 edges × 2 tris/face × 2 faces × 3 verts = 144)
const SELECTION_VERTEX_COUNT: usize = 144;

struct UploadedEditorTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

// ---------------------------------------------------------------------------
// EditorViewportRenderer
// ---------------------------------------------------------------------------

pub struct EditorViewportRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    /// Separate camera uniform buffer for the sky-light cubemap capture pass.
    /// Distinct from `camera_buffer` so cubemap capture doesn't clobber the
    /// main camera's view-projection before the scene pass reads it.
    cubemap_camera_buffer: wgpu::Buffer,
    cubemap_camera_bind_group: wgpu::BindGroup,
    model_buffer: wgpu::Buffer,
    model_bind_group: wgpu::BindGroup,
    light_buffer: wgpu::Buffer,
    light_bind_group: wgpu::BindGroup,
    material_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    #[allow(dead_code)]
    fallback_texture: wgpu::Texture,
    fallback_texture_view: wgpu::TextureView,
    materials: std::collections::HashMap<MaterialHandle, EditorGpuMaterial>,
    drawables: Vec<EditorGpuDrawable>,
    /// HDR intermediate render target (Rgba16Float) for the scene pass.
    /// The PBR shader outputs linear HDR values here. A resolve pass then
    /// copies to the sRGB swapchain, applying hardware gamma.
    hdr_texture: wgpu::Texture,
    hdr_texture_view: wgpu::TextureView,
    hdr_size: [u32; 2],
    depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,
    depth_size: [u32; 2],
    line_pipeline: wgpu::RenderPipeline,
    line_camera_buffer: wgpu::Buffer,
    line_camera_bind_group: wgpu::BindGroup,
    line_color_buffer: wgpu::Buffer,
    line_color_bind_group: wgpu::BindGroup,
    selection_vertex_buffer: wgpu::Buffer,
    /// Resolve pipeline: fullscreen blit from HDR texture to sRGB swapchain.
    resolve_pipeline: wgpu::RenderPipeline,
    resolve_bind_group: wgpu::BindGroup,
    /// Sampler for the HDR resolve pass.
    resolve_sampler: wgpu::Sampler,
    gizmo_pipeline: wgpu::RenderPipeline,
    gizmo_camera_buffer: wgpu::Buffer,
    gizmo_camera_bind_group: wgpu::BindGroup,
    gizmo_vertex_buffer: wgpu::Buffer,
    gizmo_vertex_capacity: usize,
    /// FBX-loaded gizmo mesh data (vertices + indices) for the move/translate gizmo.
    fbx_gizmo_move: Option<(Vec<pie_runtime::assets::MeshVertex>, Vec<u32>)>,
    /// FBX-loaded gizmo mesh data for the scale/sphere gizmo.
    fbx_gizmo_sphere: Option<(Vec<pie_runtime::assets::MeshVertex>, Vec<u32>)>,
    // -- Sky atmosphere --
    sky_pipeline: wgpu::RenderPipeline,
    sky_uniform_buffer: wgpu::Buffer,
    sky_bind_group: wgpu::BindGroup,
    /// Whether the sky atmosphere is enabled.
    sky_enabled: bool,
    /// Current sky quality level.
    sky_quality: SkyQuality,
    // -- Sky Light (cubemap capture) --
    /// Cubemap texture capturing the sky for indirect lighting.
    sky_light_cubemap: wgpu::Texture,
    sky_light_cubemap_view: wgpu::TextureView,
    /// Sampler for the sky light cubemap (trilinear for mip interpolation).
    sky_light_sampler: wgpu::Sampler,
    /// Whether the sky light cubemap needs to be recaptured this frame.
    sky_light_dirty: bool,
    /// Frame counter used to throttle real-time captures (capture every N frames).
    sky_light_frame_counter: u32,
}

impl EditorViewportRenderer {
    pub fn new(render_state: &egui_wgpu::RenderState, assets_root: &Path) -> Result<Self, String> {
        let device = Arc::new(render_state.device.clone());
        let queue = Arc::new(render_state.queue.clone());
        let target_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let hdr_format = wgpu::TextureFormat::Rgba16Float;

        let shader_source =
            load_shader_named(assets_root, "textured_mesh").map_err(|e| e.to_string())?;
        let shader = device
            .as_ref()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("editor viewport shader"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

        let camera_bgl =
            device
                .as_ref()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("editor viewport camera bind group layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let model_bgl =
            device
                .as_ref()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("editor viewport model bind group layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: Some(std::num::NonZeroU64::new(
                                std::mem::size_of::<EditorModelUniform>() as u64,
                            )
                            .expect("EditorModelUniform is non-zero sized")),
                        },
                        count: None,
                    }],
                });

        let texture_bgl =
            device
                .as_ref()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("editor viewport material bind group layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });

        let light_bgl =
            device
                .as_ref()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("editor viewport light bind group layout"),
                    entries: &[
                        // binding 0: directional light uniform
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // binding 1: sky light cubemap
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // binding 2: sky light sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });

        let pipeline_layout =
            device
                .as_ref()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("editor viewport pipeline layout"),
                    bind_group_layouts: &[
                        &camera_bgl,
                        &model_bgl,
                        &light_bgl,
                        &texture_bgl,
                    ],
                    push_constant_ranges: &[],
                });

        let pipeline = device
            .as_ref()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("editor viewport pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<MeshVertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 0,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                                shader_location: 1,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: (std::mem::size_of::<[f32; 3]>() * 2)
                                    as wgpu::BufferAddress,
                                shader_location: 2,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: (std::mem::size_of::<[f32; 3]>() * 2
                                    + std::mem::size_of::<[f32; 2]>())
                                    as wgpu::BufferAddress,
                                shader_location: 3,
                            },
                        ],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: hdr_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
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
                multiview: None,
                cache: None,
            });

        let camera_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport camera buffer"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind_group = device
            .as_ref()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("editor viewport camera bind group"),
                layout: &camera_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }],
            });

        let model_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport model buffer"),
            // Allocate slots for the worst-case number of drawables per frame.
            // Each slot is one EditorModelUniform; the per-draw bind group uses
            // has_dynamic_offset to address a single slot by byte offset.
            size: (UNIFORM_BUFFER_STRIDE * MAX_DRAWABLES) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let model_bind_group = device
            .as_ref()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("editor viewport model bind group"),
                layout: &model_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &model_buffer,
                        offset: 0,
                        size: Some(std::num::NonZeroU64::new(
                            std::mem::size_of::<EditorModelUniform>() as u64,
                        )
                        .expect("EditorModelUniform is non-zero sized")),
                    }),
                }],
            });

        let light_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport light buffer"),
            size: std::mem::size_of::<EditorLightUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.as_ref().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("editor viewport sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let fallback_texture = device.as_ref().create_texture(&wgpu::TextureDescriptor {
            label: Some("editor viewport fallback texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &fallback_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let fallback_texture_view =
            fallback_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let (depth_texture, depth_texture_view) = create_editor_depth_texture(&device, 1, 1);

        // Line pipeline
        let line_shader_source =
            load_shader_named(assets_root, "debug_line").map_err(|e| e.to_string())?;
        let line_shader = device
            .as_ref()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("editor viewport line shader"),
                source: wgpu::ShaderSource::Wgsl(line_shader_source.into()),
            });

        let line_cam_bgl =
            device
                .as_ref()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("editor viewport line camera bind group layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });
        let line_color_bgl =
            device
                .as_ref()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("editor viewport line color bind group layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let line_pipeline_layout =
            device
                .as_ref()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("editor viewport line pipeline layout"),
                    bind_group_layouts: &[&line_cam_bgl, &line_color_bgl],
                    push_constant_ranges: &[],
                });

        let line_pipeline =
            device
                .as_ref()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("editor viewport line pipeline"),
                    layout: Some(&line_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &line_shader,
                        entry_point: Some("vs_main"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &[wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 0,
                                shader_location: 0,
                            }],
                        }],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &line_shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: target_format,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
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
                    multiview: None,
                    cache: None,
                });

        let line_camera_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport line camera buffer"),
            size: std::mem::size_of::<LineCameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let line_camera_bind_group =
            device
                .as_ref()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("editor viewport line camera bind group"),
                    layout: &line_cam_bgl,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: line_camera_buffer.as_entire_binding(),
                    }],
                });

        let line_color_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport line color buffer"),
            size: std::mem::size_of::<LineColorUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let line_color_bind_group = device
            .as_ref()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("editor viewport line color bind group"),
                layout: &line_color_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: line_color_buffer.as_entire_binding(),
                }],
            });

        let selection_vertex_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport selection vertex buffer"),
            size: (std::mem::size_of::<[f32; 3]>() * SELECTION_VERTEX_COUNT) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // -- Gizmo triangle pipeline (unlit, per-vertex color) --
        let gizmo_shader_source =
            load_shader_named(assets_root, "gizmo_mesh").map_err(|e| e.to_string())?;
        let gizmo_shader = device
            .as_ref()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("editor viewport gizmo shader"),
                source: wgpu::ShaderSource::Wgsl(gizmo_shader_source.into()),
            });

        let gizmo_camera_bgl =
            device
                .as_ref()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("editor viewport gizmo camera bind group layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let gizmo_pipeline_layout =
            device
                .as_ref()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("editor viewport gizmo pipeline layout"),
                    bind_group_layouts: &[&gizmo_camera_bgl],
                    push_constant_ranges: &[],
                });

        let gizmo_pipeline =
            device
                .as_ref()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("editor viewport gizmo pipeline"),
                    layout: Some(&gizmo_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &gizmo_shader,
                        entry_point: Some("vs_main"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<GizmoVertex>() as wgpu::BufferAddress,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &[
                                wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32x3,
                                    offset: 0,
                                    shader_location: 0,
                                },
                                wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32x4,
                                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                                    shader_location: 1,
                                },
                            ],
                        }],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &gizmo_shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: target_format,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: false,
                        depth_compare: wgpu::CompareFunction::Always,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

        let gizmo_camera_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport gizmo camera buffer"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let gizmo_camera_bind_group =
            device
                .as_ref()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("editor viewport gizmo camera bind group"),
                    layout: &gizmo_camera_bgl,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: gizmo_camera_buffer.as_entire_binding(),
                    }],
                });

        // Gizmo vertex buffer — allocate 8192 vertices (generous for 3 axes × cone + shaft + cube).
        const GIZMO_MAX_VERTICES: usize = 8192;
        let gizmo_vertex_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("editor viewport gizmo vertex buffer"),
            size: (std::mem::size_of::<GizmoVertex>() * GIZMO_MAX_VERTICES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let (hdr_texture, hdr_texture_view) = create_hdr_texture(device.as_ref(), 1, 1);

        // -- HDR Resolve pipeline (fullscreen blit HDR → sRGB swapchain) --
        let resolve_shader_source =
            load_shader_named(assets_root, "resolve_hdr").map_err(|e| e.to_string())?;
        let resolve_shader = device
            .as_ref()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("editor HDR resolve shader"),
                source: wgpu::ShaderSource::Wgsl(resolve_shader_source.into()),
            });

        let resolve_sampler = device.as_ref().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("HDR resolve sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let resolve_bgl =
            device
                .as_ref()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("HDR resolve bind group layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                            count: None,
                        },
                    ],
                });

        let resolve_bind_group = device
            .as_ref()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("HDR resolve bind group"),
                layout: &resolve_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&hdr_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&resolve_sampler),
                    },
                ],
            });

        let resolve_pipeline_layout =
            device
                .as_ref()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("HDR resolve pipeline layout"),
                    bind_group_layouts: &[&resolve_bgl],
                    push_constant_ranges: &[],
                });

        let resolve_pipeline =
            device
                .as_ref()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("HDR resolve pipeline"),
                    layout: Some(&resolve_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &resolve_shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &resolve_shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: target_format,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: false,
                        depth_compare: wgpu::CompareFunction::Always,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

        // -- Sky atmosphere pipeline --
        let sky_shader_source =
            load_shader_named(assets_root, "sky_atmosphere").map_err(|e| e.to_string())?;
        let sky_shader = device
            .as_ref()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("editor sky atmosphere shader"),
                source: wgpu::ShaderSource::Wgsl(sky_shader_source.into()),
            });

        let sky_bgl =
            device
                .as_ref()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("sky bind group layout"),
                    entries: &[
                        // binding 0: camera uniform — has_dynamic_offset=true so
                        // the cubemap-capture loop can address per-face slots
                        // in a 6-wide buffer via a dynamic offset. The main
                        // sky pass uses offset 0 against the single-slot
                        // camera_buffer.
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: true,
                                min_binding_size: Some(std::num::NonZeroU64::new(
                                    std::mem::size_of::<CameraUniform>() as u64,
                                )
                                .expect("CameraUniform is non-zero sized")),
                            },
                            count: None,
                        },
                        // binding 1: sky params uniform
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

        let sky_pipeline_layout =
            device
                .as_ref()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("sky pipeline layout"),
                    bind_group_layouts: &[&sky_bgl],
                    push_constant_ranges: &[],
                });

        let sky_pipeline =
            device
                .as_ref()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("sky atmosphere pipeline"),
                    layout: Some(&sky_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &sky_shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &sky_shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: hdr_format,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: false,
                        depth_compare: wgpu::CompareFunction::Always,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

        let sky_uniform_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky uniform buffer"),
            size: std::mem::size_of::<EditorSkyUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sky_bind_group = device
            .as_ref()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sky bind group"),
                layout: &sky_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &camera_buffer,
                            offset: 0,
                            size: Some(std::num::NonZeroU64::new(
                                std::mem::size_of::<CameraUniform>() as u64,
                            )
                            .expect("CameraUniform is non-zero sized")),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: sky_uniform_buffer.as_entire_binding(),
                    },
                ],
            });

        // Separate camera buffer + sky bind group for cubemap capture, so the
        // cubemap face capture loop can write per-face view-projections without
        // clobbering the main camera buffer (which Pass 0 / Pass 1 read).
        // Sized for 6 faces so we can write all 6 uniforms in one
        // `queue.write_buffer` and then select per-face via a dynamic offset.
        // Each slot is UNIFORM_BUFFER_STRIDE (256) bytes, not sizeof(CameraUniform)
        // (144), to satisfy wgpu's min_uniform_buffer_offset_alignment.
        let cubemap_camera_buffer = device.as_ref().create_buffer(&wgpu::BufferDescriptor {
            label: Some("cubemap capture camera buffer"),
            size: (UNIFORM_BUFFER_STRIDE * 6) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cubemap_camera_bind_group = device
            .as_ref()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cubemap capture sky bind group"),
                layout: &sky_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &cubemap_camera_buffer,
                            offset: 0,
                            size: Some(std::num::NonZeroU64::new(
                                std::mem::size_of::<CameraUniform>() as u64,
                            )
                            .expect("CameraUniform is non-zero sized")),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: sky_uniform_buffer.as_entire_binding(),
                    },
                ],
            });

        // -- Sky Light cubemap --
        // Create a cubemap texture for capturing the sky atmosphere.
        // Resolution 64x64 per face is sufficient for diffuse irradiance.
        let sky_light_res = 64u32;
        let sky_light_cubemap = device.as_ref().create_texture(&wgpu::TextureDescriptor {
            label: Some("sky light cubemap"),
            size: wgpu::Extent3d {
                width: sky_light_res,
                height: sky_light_res,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: hdr_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let sky_light_cubemap_view = sky_light_cubemap.create_view(&wgpu::TextureViewDescriptor {
            label: Some("sky light cubemap view"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let sky_light_sampler = device.as_ref().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sky light sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Create the light bind group with the actual sky light cubemap.
        let light_bind_group = device
            .as_ref()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("editor viewport light bind group"),
                layout: &light_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: light_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&sky_light_cubemap_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sky_light_sampler),
                    },
                ],
            });

        Ok(Self {
            device,
            queue,
            pipeline,
            camera_buffer,
            camera_bind_group,
            cubemap_camera_buffer,
            cubemap_camera_bind_group,
            model_buffer,
            model_bind_group,
            light_buffer,
            light_bind_group,
            material_bind_group_layout: texture_bgl,
            sampler,
            fallback_texture,
            fallback_texture_view,
            materials: std::collections::HashMap::new(),
            drawables: Vec::new(),
            hdr_texture,
            hdr_texture_view,
            hdr_size: [1, 1],
            depth_texture,
            depth_texture_view,
            depth_size: [1, 1],
            line_pipeline,
            line_camera_buffer,
            line_camera_bind_group,
            line_color_buffer,
            line_color_bind_group,
            selection_vertex_buffer,
            resolve_pipeline,
            resolve_bind_group,
            resolve_sampler,
            gizmo_pipeline,
            gizmo_camera_buffer,
            gizmo_camera_bind_group,
            gizmo_vertex_buffer,
            gizmo_vertex_capacity: GIZMO_MAX_VERTICES,
            fbx_gizmo_move: None,
            fbx_gizmo_sphere: None,
            sky_pipeline,
            sky_uniform_buffer,
            sky_bind_group,
            sky_enabled: true,
            sky_quality: SkyQuality::Medium,
            sky_light_cubemap,
            sky_light_cubemap_view,
            sky_light_sampler,
            sky_light_dirty: true,
            sky_light_frame_counter: 0,
        })
    }

    /// Load FBX gizmo meshes from the asset registry into the renderer.
    ///
    /// Looks for meshes named "GizmosMoveTool*" and "GizmosSphere*"
    /// (the names assigned by the FBX loader), merges all sub-meshes,
    /// and stores their vertex/index data for use in the gizmo overlay.
    ///
    /// The GizmosMoveTool FBX model contains a single arrow along +Z
    /// (Blender convention). After the Z-up → Y-up rotation in
    /// `build_fbx_gizmo_mesh`, that becomes the +Y arrow. The X and Z
    /// arrows are generated by rotating the Y arrow data ±90° around Z
    /// and X respectively.
    pub fn load_fbx_gizmos(&mut self, registry: &AssetRegistry) {
        // Merge all GizmosMoveTool sub-meshes
        let mut move_verts = Vec::new();
        let mut move_indices = Vec::new();
        let mut sphere_verts = Vec::new();
        let mut sphere_indices = Vec::new();

        for mesh in registry.meshes() {
            if mesh.name.starts_with("GizmosMoveTool") {
                let base = move_verts.len() as u32;
                move_verts.extend_from_slice(&mesh.vertices);
                move_indices.extend(mesh.indices.iter().map(|&i| i + base));
            } else if mesh.name.starts_with("GizmosSphere") {
                let base = sphere_verts.len() as u32;
                sphere_verts.extend_from_slice(&mesh.vertices);
                sphere_indices.extend(mesh.indices.iter().map(|&i| i + base));
            }
        }

        // Resize gizmo vertex buffer if needed to fit the 3-arrow gizmo
        // (3 copies of the move mesh vertices) plus 3 procedural cone
        // arrowheads plus sphere vertices
        if !move_verts.is_empty() {
            let sphere_vert_count = if sphere_verts.is_empty() {
                0
            } else {
                sphere_indices.len()
            };
            // Each procedural cone: CONE_SEGMENTS * 2 triangles * 3 vertices
            let cone_verts = 12 * 6; // CONE_SEGMENTS=12, 2 tris per segment, 3 verts per tri
            let gizmo_vert_count = move_indices.len() * 3 + cone_verts * 3 + sphere_vert_count;
            if gizmo_vert_count > self.gizmo_vertex_capacity {
                let new_capacity = gizmo_vert_count.next_power_of_two();
                self.gizmo_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("gizmo vertex buffer (resized for FBX)"),
                    size: (new_capacity * std::mem::size_of::<GizmoVertex>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.gizmo_vertex_capacity = new_capacity;
            }

            self.fbx_gizmo_move = Some((move_verts, move_indices));
            eprintln!(
                "pie_editor: loaded FBX gizmo move tool (merged {} vertices, {} indices)",
                self.fbx_gizmo_move.as_ref().unwrap().0.len(),
                self.fbx_gizmo_move.as_ref().unwrap().1.len()
            );
        }

        if !sphere_verts.is_empty() {
            self.fbx_gizmo_sphere = Some((sphere_verts, sphere_indices));
            eprintln!(
                "pie_editor: loaded FBX gizmo sphere (merged {} vertices, {} indices)",
                self.fbx_gizmo_sphere.as_ref().unwrap().0.len(),
                self.fbx_gizmo_sphere.as_ref().unwrap().1.len()
            );
        }
    }

    /// Set sky atmosphere quality level.
    pub fn set_sky_quality(&mut self, quality: SkyQuality) {
        self.sky_enabled = quality != SkyQuality::Off;
        // The actual sample counts are applied in render_to_view via the uniform.
        // We store the quality and apply it when writing the uniform.
        self.sky_quality = quality;
    }

    pub fn load_scene(
        &mut self,
        registry: &AssetRegistry,
        simulation: &pie_runtime::core::SimulationCore,
    ) -> Result<(), String> {
        self.materials.clear();
        let mut drawables = Vec::new();
        for (entity, mesh_renderer) in simulation
            .world()
            .query::<&pie_runtime::components::MeshRenderer>()
            .iter()
        {
            let mesh = registry
                .mesh(mesh_renderer.mesh)
                .map_err(|e| e.to_string())?;
            let material = registry
                .material(mesh.material)
                .map_err(|e| e.to_string())?;
            if !self.materials.contains_key(&mesh.material) {
                self.materials
                    .insert(mesh.material, self.upload_material(registry, material)?);
            }
            drawables.push(EditorGpuDrawable {
                entity,
                mesh: upload_editor_mesh(self.device.as_ref(), mesh),
                material: mesh.material,
            });
        }
        self.drawables = drawables;
        Ok(())
    }

    fn upload_material(
        &self,
        registry: &AssetRegistry,
        material: &MaterialAsset,
    ) -> Result<EditorGpuMaterial, String> {
        let uniform = EditorMaterialUniform {
            base_color_factor: material.base_color_factor,
            parameters: [
                material.metallic_factor,
                material.roughness_factor,
                if material.normal_texture.is_some() { 1.0 } else { 0.0 },
                0.0,
            ],
        };
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("editor viewport material buffer"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let base_color_view = if let Some(th) = material.base_color_texture {
            Some(upload_editor_texture(
                self.device.as_ref(),
                self.queue.as_ref(),
                registry.texture(th).map_err(|e| e.to_string())?,
            ))
        } else {
            None
        };

        let (normal_gpu, normal_view) = if let Some(nh) = material.normal_texture {
            let up = upload_editor_texture(
                self.device.as_ref(),
                self.queue.as_ref(),
                registry.texture(nh).map_err(|e| e.to_string())?,
            );
            (Some(up.texture), Some(up.view))
        } else {
            (None, None)
        };

        let base_color_view_ref = base_color_view
            .as_ref()
            .map(|t| &t.view)
            .unwrap_or(&self.fallback_texture_view);
        let normal_view_ref = normal_view.as_ref().unwrap_or(&self.fallback_texture_view);

        let bind_group = self
            .device
            .as_ref()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("editor viewport material bind group"),
                layout: &self.material_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(base_color_view_ref),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(normal_view_ref),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });

        Ok(EditorGpuMaterial {
            _buffer: buffer.clone(),
            bind_group,
            _texture: base_color_view.map(|t| t.texture),
            _normal_texture: normal_gpu,
        })
    }

    /// Render the editor viewport. Takes many independently-configurable inputs
    /// (selection, gizmo origin, hovered axis/center, gizmo state) because each is
    /// set by a distinct editor subsystem — bundling them into a single struct would
    /// obscure which input comes from where, which fights the engine's
    /// explicit-control philosophy. Refactor candidate when the M10 render graph
    /// reshapes the editor draw path; for now the arg count is acknowledged.
    #[allow(clippy::too_many_arguments)]
    pub fn render_to_view(
        &mut self,
        simulation: &pie_runtime::core::SimulationCore,
        view: &wgpu::TextureView,
        size: [u32; 2],
        selection_aabb: Option<(Vec3, Vec3)>,
        gizmo_origin: Option<Vec3>,
        hovered_axis: Option<Axis>,
        hovered_center: bool,
        gizmo_state: GizmoState,
    ) {
        if size[0] == 0 || size[1] == 0 {
            return;
        }

        // Resize HDR and depth textures if viewport size changed
        if self.hdr_size != size {
            let (ht, htv) = create_hdr_texture(self.device.as_ref(), size[0], size[1]);
            self.hdr_texture = ht;
            self.hdr_texture_view = htv;
            self.hdr_size = size;
            // Recreate resolve bind group with new HDR texture view
            self.resolve_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("HDR resolve bind group (resized)"),
                layout: &self.resolve_pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.hdr_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.resolve_sampler),
                    },
                ],
            });
        }
        if self.depth_size != size {
            let (dt, dtv) = create_editor_depth_texture(self.device.as_ref(), size[0], size[1]);
            self.depth_texture = dt;
            self.depth_texture_view = dtv;
            self.depth_size = size;
        }

        let aspect = size[0] as f32 / size[1] as f32;
        let camera_uniform = CameraUniform::from_simulation(simulation, aspect);
        self.queue.as_ref().write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&camera_uniform),
        );

        let fov = simulation
            .active_camera()
            .and_then(|e| simulation.world().get::<&Camera>(e).ok())
            .map(|c| c.fov)
            .unwrap_or_else(|| Camera::default().fov);
        let vp = camera_view_proj(
            simulation
                .active_camera()
                .and_then(|e| simulation.world().get::<&Transform>(e).ok())
                .map(|t| *t)
                .unwrap_or_default(),
            aspect,
            fov,
        );
        self.queue.as_ref().write_buffer(
            &self.line_camera_buffer,
            0,
            bytemuck::bytes_of(&LineCameraUniform {
                view_proj: vp.to_cols_array_2d(),
            }),
        );

        let dl = simulation
            .world()
            .query::<&DirectionalLight>()
            .iter()
            .map(|(_, light)| *light)
            .next()
            .unwrap_or_default();
        let sky_light = simulation
            .world()
            .query::<&SkyLight>()
            .iter()
            .map(|(_, sl)| *sl)
            .next()
            .unwrap_or_default();
        self.queue.as_ref().write_buffer(
            &self.light_buffer,
            0,
            bytemuck::bytes_of(&EditorLightUniform {
                direction: [dl.direction.x, dl.direction.y, dl.direction.z, 0.0],
                color: [dl.color.x, dl.color.y, dl.color.z, dl.intensity],
                sky_light: [sky_light.intensity, 0.0, 0.0, 0.0],
            }),
        );

        // Update sky atmosphere uniform from directional light.
        // The directional light's `direction` is the direction the light
        // *travels* (sun → scene); the sky shader expects the direction *to*
        // the sun (scene → sun), so we negate it before uploading.
        let sun_dir = -dl.direction;
        let (ray_samples, mie_samples) = self.sky_quality.sample_counts();
        self.queue.as_ref().write_buffer(
            &self.sky_uniform_buffer,
            0,
            bytemuck::bytes_of(&EditorSkyUniform {
                sun_direction: [sun_dir.x, sun_dir.y, sun_dir.z, 0.0],
                sun_intensity: dl.intensity.max(1.0),
                // Scattering multipliers — tuned for visual impact rather than
                // physical accuracy. The base coefficients in the shader are
                // ~5.5e-6 (Rayleigh) and ~2e-5 (Mie), so these multipliers
                // bring the values into a visible range.
                rayleigh_coefficient: 800.0,
                mie_coefficient: 400.0,
                rayleigh_scale_height: 8.0,
                mie_scale_height: 1.2,
                mie_directionality: 0.8,
                planet_radius: 6360.0,
                atmosphere_radius: 6460.0,
                ray_samples,
                mie_samples,
                _padding: [0; 2],
            }),
        );

        if let Some((min, max)) = selection_aabb {
            let v = aabb_thick_line_vertices(min, max);
            self.queue.as_ref().write_buffer(
                &self.selection_vertex_buffer,
                0,
                bytemuck::cast_slice(&v),
            );
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("editor viewport encoder"),
            });

        // ====================================================================
        // Pass -1: Capture sky light cubemap (6 faces)
        // ====================================================================
        // Renders the sky atmosphere into each face of the cubemap for use as
        // indirect lighting in the PBR shader. Only captures when the sky
        // light is dirty (first frame or real-time capture enabled).
        {
            let sky_light = simulation
                .world()
                .query::<&SkyLight>()
                .iter()
                .map(|(_, sl)| *sl)
                .next()
                .unwrap_or_default();

            // Throttle real-time captures to every 4th frame for performance.
            // The counter increments *every* frame (not just when capturing)
            // so the throttle actually rotates — the old code only incremented
            // inside the capture branch, so after the first capture the
            // counter was stuck at 1 and real-time capture never re-ran.
            let throttle_mod = self.sky_light_frame_counter;
            self.sky_light_frame_counter = (self.sky_light_frame_counter + 1) % 4;

            let should_capture = self.sky_light_dirty
                || (sky_light.real_time_capture && throttle_mod == 0);

            if should_capture && self.sky_enabled {
                // Cubemap face directions: +X, -X, +Y, -Y, +Z, -Z.
                // The `forward` and `up` vectors follow the Vulkan/WebGPU cube
                // convention so the captured faces sample correctly in the PBR
                // shader's texture_cube lookups.
                //   +X / -X / +Z / -Z faces use up = (0, -1, 0)
                //   +Y face uses up = (0, 0, +1)
                //   -Y face uses up = (0, 0, -1)
                let face_basis: [(Vec3, Vec3); 6] = [
                    (Vec3::X,   Vec3::NEG_Y), // +X
                    (Vec3::NEG_X, Vec3::NEG_Y), // -X
                    (Vec3::Y,   Vec3::Z),    // +Y
                    (Vec3::NEG_Y, Vec3::NEG_Z), // -Y
                    (Vec3::Z,   Vec3::NEG_Y), // +Z
                    (Vec3::NEG_Z, Vec3::NEG_Y), // -Z
                ];

                let cam_pos = simulation
                    .active_camera()
                    .and_then(|e| simulation.world().get::<&Transform>(e).ok())
                    .map(|t| t.translation)
                    .unwrap_or_default();

                // Use the cubemap texture's actual size for the depth attachment
                // (NOT sky_light.capture_resolution, which may differ from the
                // cubemap's hardcoded 64×64 construction size and cause a
                // wgpu validation error: "Attachments have differing sizes").
                let cubemap_size = self.sky_light_cubemap.size();
                let res = cubemap_size.width.max(1);
                let aspect = 1.0f32; // square faces
                let fov = std::f32::consts::FRAC_PI_2; // 90 degrees for cubemap faces

                // Create a small depth texture for the cubemap capture passes.
                let cube_depth = self.device.as_ref().create_texture(&wgpu::TextureDescriptor {
                    label: Some("sky light cubemap depth"),
                    size: wgpu::Extent3d {
                        width: res,
                        height: res,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: DEPTH_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let cube_depth_view = cube_depth.create_view(&wgpu::TextureViewDescriptor::default());

                // Build all 6 face CameraUniforms up front into a staging
                // buffer, then issue a single `queue.write_buffer` for all of
                // them. This avoids the old anti-pattern where 6 separate
                // `write_buffer` calls (one per face, all queued before
                // `submit`) collapsed to only the last face's value being read
                // by the encoder.
                // Build a padded byte buffer: each CameraUniform (144 bytes)
                // is placed at a 256-byte-aligned offset so the per-face
                // dynamic offset `(face * UNIFORM_BUFFER_STRIDE)` satisfies
                // wgpu's min_uniform_buffer_offset_alignment.
                let mut face_uniforms_padded: Vec<u8> = vec![0u8; 6 * UNIFORM_BUFFER_STRIDE];
                for face in 0..6 {
                    let (forward, up) = face_basis[face];
                    let view = Mat4::look_at_rh(cam_pos, cam_pos + forward, up);
                    let proj = Mat4::perspective_rh(fov, aspect, 0.1, 10000.0);
                    let view_proj = proj * view;
                    let uniform = CameraUniform::from_view_proj(
                        view_proj,
                        cam_pos,
                        aspect,
                        fov,
                    );
                    let bytes = bytemuck::bytes_of(&uniform);
                    let offset = face * UNIFORM_BUFFER_STRIDE;
                    face_uniforms_padded[offset..offset + bytes.len()].copy_from_slice(bytes);
                }
                self.queue.as_ref().write_buffer(
                    &self.cubemap_camera_buffer,
                    0,
                    &face_uniforms_padded,
                );

                for face in 0..6 {
                    // Create a texture view for this specific cubemap face.
                    let face_view = self.sky_light_cubemap.create_view(
                        &wgpu::TextureViewDescriptor {
                            label: Some("sky light cubemap face"),
                            dimension: Some(wgpu::TextureViewDimension::D2),
                            base_array_layer: face as u32,
                            array_layer_count: Some(1),
                            ..Default::default()
                        },
                    );

                    let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("sky light cubemap capture"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &face_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &cube_depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    rp.set_pipeline(&self.sky_pipeline);
                    // Use the cubemap-specific bind group so we don't clobber
                    // the main camera buffer (and don't need to restore it).
                    // Dynamic offset selects this face's slot in
                    // cubemap_camera_buffer. Stride is 256 (not
                    // sizeof(CameraUniform) = 144) to satisfy wgpu's
                    // min_uniform_buffer_offset_alignment.
                    let face_offset = (face * UNIFORM_BUFFER_STRIDE) as u32;
                    rp.set_bind_group(0, &self.cubemap_camera_bind_group, &[face_offset]);
                    rp.draw(0..3, 0..1);
                }
            }

            self.sky_light_dirty = false;
        }

        // ====================================================================
        // Pass 0: Sky Atmosphere → HDR render target
        // ====================================================================
        // Renders the sky dome via ray-marching Rayleigh/Mie scattering.
        // Outputs linear HDR values. Runs before the scene so depth test
        // ensures geometry occludes the sky.
        if self.sky_enabled {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("editor sky atmosphere pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.hdr_texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(theme::VIEWPORT_CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_pipeline(&self.sky_pipeline);
            rp.set_bind_group(0, &self.sky_bind_group, &[0]);
            rp.draw(0..3, 0..1);
        }

        // ====================================================================
        // Pass 1: Scene → HDR render target (Rgba16Float, linear space)
        // ====================================================================
        // PBR shader outputs linear HDR values. No gamma correction in shader
        // — that's handled by the sRGB swapchain in pass 2.
        // NOTE: LoadOp::Load preserves the sky from Pass 0 — geometry occludes
        // the sky via the depth buffer, but the sky shows through where there's
        // no geometry.
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("editor scene HDR pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.hdr_texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rp.set_pipeline(&self.pipeline);
            rp.set_bind_group(0, &self.camera_bind_group, &[]);
            rp.set_bind_group(2, &self.light_bind_group, &[]);

            // Pre-compute per-drawable model uniforms into a staging buffer so
            // we can issue a single `queue.write_buffer` for ALL drawables
            // before iterating the draw calls. The old code called
            // `queue.write_buffer` inside the per-drawable render-pass loop;
            // because queue operations are processed in order *before*
            // `queue.submit(encoder.finish())`, every draw ended up reading
            // the last drawable's model matrix.
            let drawable_count = self.drawables.len().min(MAX_DRAWABLES);
            // Build a padded byte buffer: each EditorModelUniform (128 bytes)
            // is placed at a 256-byte-aligned offset so the per-draw dynamic
            // offset `(index * UNIFORM_BUFFER_STRIDE)` satisfies wgpu's
            // min_uniform_buffer_offset_alignment (typically 256).
            let mut model_uniforms_padded: Vec<u8> =
                vec![0u8; drawable_count * UNIFORM_BUFFER_STRIDE];
            for (index, d) in self.drawables.iter().enumerate().take(drawable_count) {
                let t = simulation
                    .world()
                    .get::<&Transform>(d.entity)
                    .ok()
                    .map(|t| *t)
                    .unwrap_or_default();
                let m = Mat4::from_scale_rotation_translation(t.scale, t.rotation, t.translation);
                let uniform = EditorModelUniform {
                    model: m.to_cols_array_2d(),
                    normal_matrix: m.inverse().transpose().to_cols_array_2d(),
                };
                let bytes = bytemuck::bytes_of(&uniform);
                let offset = index * UNIFORM_BUFFER_STRIDE;
                model_uniforms_padded[offset..offset + bytes.len()].copy_from_slice(bytes);
            }
            self.queue.as_ref().write_buffer(
                &self.model_buffer,
                0,
                &model_uniforms_padded,
            );

            for (index, d) in self.drawables.iter().enumerate().take(drawable_count) {
                if index >= MAX_DRAWABLES {
                    break;
                }
                // Address this drawable's slot in the model uniform buffer via
                // dynamic offset. This is processed by the render pass at draw
                // time (not as a separate queue op), so each draw reads the
                // correct matrix. Stride is 256 (not sizeof(EditorModelUniform)
                // = 128) to satisfy wgpu's min_uniform_buffer_offset_alignment.
                let dynamic_offset = (index * UNIFORM_BUFFER_STRIDE) as u32;
                rp.set_bind_group(1, &self.model_bind_group, &[dynamic_offset]);
                let mat = match self.materials.get(&d.material) {
                    Some(m) => m,
                    None => continue, // skip draws with missing materials instead of panicking
                };
                rp.set_bind_group(3, &mat.bind_group, &[]);
                rp.set_vertex_buffer(0, d.mesh.vertex_buffer.slice(..));
                rp.set_index_buffer(d.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..d.mesh.index_count, 0, 0..1);
            }
        }

        // ====================================================================
        // Pass 2: Resolve HDR → sRGB swapchain + overlays (gizmos, selection)
        // ====================================================================
        // Copy HDR texture to swapchain (hardware converts linear → sRGB),
        // then render gizmo and selection overlays directly on the swapchain.
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("editor resolve + overlay pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Resolve: blit HDR scene to sRGB swapchain
            rp.set_pipeline(&self.resolve_pipeline);
            rp.set_bind_group(0, &self.resolve_bind_group, &[]);
            rp.draw(0..3, 0..1);

            // Selection highlight overlay
            if selection_aabb.is_some() {
                self.queue.as_ref().write_buffer(
                    &self.line_color_buffer,
                    0,
                    bytemuck::bytes_of(&LineColorUniform {
                        color: [1.0, 0.5, 0.0, 1.0],
                    }),
                );
                rp.set_pipeline(&self.line_pipeline);
                rp.set_bind_group(0, &self.line_camera_bind_group, &[]);
                rp.set_bind_group(1, &self.line_color_bind_group, &[]);
                rp.set_vertex_buffer(0, self.selection_vertex_buffer.slice(..));
                rp.draw(0..SELECTION_VERTEX_COUNT as u32, 0..1);
            }

            // Gizmo overlay
            if let Some(origin) = gizmo_origin {
                let cam_pos = simulation
                    .active_camera()
                    .and_then(|e| simulation.world().get::<&Transform>(e).ok())
                    .map(|t| t.translation)
                    .unwrap_or(Vec3::new(0.0, 1.0, 5.0));
                let scale = GIZMO_WORLD_SCALE;

                let gizmo_verts = if let Some((ref verts, ref indices)) = self.fbx_gizmo_move {
                    let (sp_verts, sp_indices) = match self.fbx_gizmo_sphere {
                        Some((ref sv, ref si)) => (Some(sv.as_slice()), Some(si.as_slice())),
                        None => (None, None),
                    };
                    build_fbx_gizmo_mesh(
                        origin,
                        verts,
                        indices,
                        sp_verts,
                        sp_indices,
                        scale,
                        hovered_axis,
                        hovered_center,
                        gizmo_state,
                    )
                } else {
                    build_gizmo_mesh(
                        origin,
                        cam_pos,
                        scale,
                        hovered_axis,
                        hovered_center,
                        gizmo_state,
                    )
                };
                if !gizmo_verts.is_empty() && gizmo_verts.len() <= self.gizmo_vertex_capacity {
                    let bytes: &[u8] = bytemuck::cast_slice(&gizmo_verts);
                    self.queue
                        .as_ref()
                        .write_buffer(&self.gizmo_vertex_buffer, 0, bytes);
                    self.queue.as_ref().write_buffer(
                        &self.gizmo_camera_buffer,
                        0,
                        bytemuck::bytes_of(&LineCameraUniform {
                            view_proj: vp.to_cols_array_2d(),
                        }),
                    );
                    rp.set_pipeline(&self.gizmo_pipeline);
                    rp.set_bind_group(0, &self.gizmo_camera_bind_group, &[]);
                    rp.set_vertex_buffer(0, self.gizmo_vertex_buffer.slice(..));
                    rp.draw(0..gizmo_verts.len() as u32, 0..1);
                }
            }
        }

        self.queue
            .as_ref()
            .submit(std::iter::once(encoder.finish()));
    }
}

fn upload_editor_mesh(device: &wgpu::Device, mesh: &MeshAsset) -> EditorGpuMesh {
    let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("editor viewport mesh vertex buffer"),
        contents: bytemuck::cast_slice(&mesh.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("editor viewport mesh index buffer"),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    EditorGpuMesh {
        vertex_buffer: vb,
        index_buffer: ib,
        index_count: mesh.indices.len() as u32,
    }
}

fn upload_editor_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &pie_runtime::assets::TextureAsset,
) -> UploadedEditorTexture {
    let gt = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("editor viewport loaded texture"),
        size: wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &gt,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &texture.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * texture.width),
            rows_per_image: Some(texture.height),
        },
        wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
    );
    let view = gt.create_view(&wgpu::TextureViewDescriptor::default());
    UploadedEditorTexture { texture: gt, view }
}

/// Generate thick AABB wireframe as triangle quads.
///
/// Each of the 12 edges of the AABB is rendered as a thin rectangular
/// prism (2 faces × 2 triangles × 3 vertices = 12 vertices per edge,
/// 144 vertices total). The `thickness` parameter controls the half-width
/// of each edge strip in world space.
fn aabb_thick_line_vertices(min: Vec3, max: Vec3) -> [[f32; 3]; SELECTION_VERTEX_COUNT] {
    let a = min;
    let g = max;
    let b = Vec3::new(g.x, a.y, a.z);
    let c = Vec3::new(g.x, g.y, a.z);
    let d = Vec3::new(a.x, g.y, a.z);
    let e = Vec3::new(a.x, a.y, g.z);
    let f = Vec3::new(g.x, a.y, g.z);
    let h = Vec3::new(a.x, g.y, g.z);

    // 12 edges of the AABB
    let edges: [(Vec3, Vec3); 12] = [
        (a, b),
        (b, c),
        (c, d),
        (d, a), // front face
        (e, f),
        (f, g),
        (g, h),
        (h, e), // back face
        (a, e),
        (b, f),
        (c, g),
        (d, h), // connecting edges
    ];

    let mut result = [[0.0f32; 3]; SELECTION_VERTEX_COUNT];
    let t = 0.005; // half-thickness in world units

    let mut idx = 0;
    for (p0, p1) in &edges {
        let dir = *p1 - *p0;
        let _len = dir.length();
        let dir_n = dir.normalize_or_zero();

        // If the edge is degenerate (zero length, e.g. an AABB where min == max),
        // emit zeroed vertices for this edge's 12 slots to avoid NaN
        // propagating from `normalize()` on a zero vector.
        if dir_n.length_squared() < 1e-12 {
            for _ in 0..12 {
                result[idx] = [0.0; 3];
                idx += 1;
            }
            continue;
        }

        // Find two perpendicular vectors to the edge direction.
        // Use `normalize_or_zero` (not `normalize`) on the cross products so a
        // degenerate cross result doesn't produce NaN.
        let perp1 = if dir_n.cross(Vec3::Y).length_squared() > 0.0001 {
            dir_n.cross(Vec3::Y).normalize_or_zero()
        } else {
            dir_n.cross(Vec3::X).normalize_or_zero()
        };
        // If perp1 still ended up zero (e.g. dir_n is itself zero, which we
        // already guarded above), fall back to a world axis to avoid NaN.
        let perp1 = if perp1.length_squared() < 1e-12 {
            Vec3::X
        } else {
            perp1
        };
        let perp2 = dir_n.cross(perp1).normalize_or_zero();
        let perp2 = if perp2.length_squared() < 1e-12 {
            Vec3::Y
        } else {
            perp2
        };

        // Generate a thin quad (2 triangles) for each of 2 perpendicular planes
        // This creates a cross-shaped cross-section for each edge
        for &perp in &[perp1, perp2] {
            let offset = perp * t;
            let v0 = *p0 - offset;
            let v1 = *p0 + offset;
            let v2 = *p1 + offset;
            let v3 = *p1 - offset;

            // Triangle 1: v0, v1, v2
            result[idx] = [v0.x, v0.y, v0.z];
            idx += 1;
            result[idx] = [v1.x, v1.y, v1.z];
            idx += 1;
            result[idx] = [v2.x, v2.y, v2.z];
            idx += 1;
            // Triangle 2: v0, v2, v3
            result[idx] = [v0.x, v0.y, v0.z];
            idx += 1;
            result[idx] = [v2.x, v2.y, v2.z];
            idx += 1;
            result[idx] = [v3.x, v3.y, v3.z];
            idx += 1;
        }
    }

    debug_assert_eq!(idx, SELECTION_VERTEX_COUNT);
    result
}
