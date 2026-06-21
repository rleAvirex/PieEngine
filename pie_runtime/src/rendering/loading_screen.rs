//! Loading screen — a lightweight renderer that shows a progress indicator
//! while scene assets are being loaded.
//!
//! Draws a dark background with an animated progress bar using a minimal
//! wgpu pipeline (no textures, no complex shaders — just a fullscreen quad
//! with a progress uniform).

use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::window::Window;

/// Loading screen state: tracks progress and owns GPU resources for the
/// minimal loading visualization.
pub struct LoadingScreen {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    progress_buffer: wgpu::Buffer,
    progress_bind_group: wgpu::BindGroup,
    /// Current loading progress in [0, 1].
    progress: f32,
    /// A label describing what's currently loading.
    status_text: String,
}

/// Uniform buffer for the loading screen shader.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LoadingUniform {
    /// Progress value in [0, 1].
    progress: f32,
    /// Padding to 16-byte alignment.
    _padding: [f32; 3],
    /// Background color (dark charcoal).
    bg_color: [f32; 4],
    /// Bar color (Pie Engine orange).
    bar_color: [f32; 4],
}

const LOADING_SHADER: &str = r"
struct LoadingUniform {
    progress: f32,
    _padding: vec3f,
    bg_color: vec4f,
    bar_color: vec4f,
};

@group(0) @binding(0) var<uniform> u_loading: LoadingUniform;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle (3 vertices covering the entire clip space)
    var positions = array<vec2f, 3>(
        vec2f(-1.0, -1.0),
        vec2f(3.0, -1.0),
        vec2f(-1.0, 3.0),
    );
    var uvs = array<vec2f, 3>(
        vec2f(0.0, 1.0),
        vec2f(2.0, 1.0),
        vec2f(0.0, -1.0),
    );

    var output: VertexOutput;
    output.position = vec4f(positions[vertex_index], 0.0, 1.0);
    output.uv = uvs[vertex_index];
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    let bg = u_loading.bg_color;

    // Progress bar: centered, 60% of screen width, 4px tall (approximated as 0.8% of height)
    let bar_height = 0.008;
    let bar_y_center = 0.0;
    let bar_width = 0.6;
    let bar_x_start = (1.0 - bar_width) / 2.0;

    let in_bar_y = abs(input.uv.y - (1.0 - bar_y_center) - bar_height / 2.0) < bar_height;
    let in_bar_x = input.uv.x >= bar_x_start && input.uv.x <= bar_x_start + bar_width * u_loading.progress;

    if (in_bar_y && in_bar_x) {
        return u_loading.bar_color;
    }

    // Background track (slightly lighter than background)
    let in_track_x = input.uv.x >= bar_x_start && input.uv.x <= bar_x_start + bar_width;
    if (in_bar_y && in_track_x) {
        return vec4f(bg.x + 0.05, bg.y + 0.05, bg.z + 0.05, 1.0);
    }

    // Subtle lighter strip for text area
    let in_text_area = input.uv.y > 0.42 && input.uv.y < 0.46;
    if (in_text_area) {
        let alpha = 0.3 * (1.0 - abs(input.uv.y - 0.44) / 0.02);
        return vec4f(0.7, 0.7, 0.7, alpha);
    }

    return bg;
}
";

impl LoadingScreen {
    /// Create a new loading screen renderer for the given window.
    ///
    /// This sets up the GPU device, surface, and a minimal render pipeline.
    /// It does NOT load any external assets — everything is self-contained.
    pub fn new(window: Arc<Window>) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window)
            .map_err(|error| error.to_string())?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| "compatible GPU adapter should be available".to_string())?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("loading screen device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .map_err(|error| error.to_string())?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("loading screen shader"),
            source: wgpu::ShaderSource::Wgsl(LOADING_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("loading bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("loading screen pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("loading screen pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let uniform = LoadingUniform {
            progress: 0.0,
            _padding: [0.0; 3],
            bg_color: [0.08, 0.08, 0.10, 1.0],  // dark charcoal
            bar_color: [0.95, 0.55, 0.15, 1.0], // Pie Engine orange
        };

        let progress_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("loading uniform buffer"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let progress_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("loading bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: progress_buffer.as_entire_binding(),
            }],
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            progress_buffer,
            progress_bind_group,
            progress: 0.0,
            status_text: String::new(),
        })
    }

    /// Update loading progress (0.0 to 1.0) and status text.
    pub fn set_progress(&mut self, progress: f32, status: &str) {
        self.progress = progress.clamp(0.0, 1.0);
        self.status_text = status.to_string();
    }

    /// Handle window resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Render one frame of the loading screen.
    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // Update the uniform buffer
        let uniform = LoadingUniform {
            progress: self.progress,
            _padding: [0.0; 3],
            bg_color: [0.08, 0.08, 0.10, 1.0],
            bar_color: [0.95, 0.55, 0.15, 1.0],
        };
        self.queue
            .write_buffer(&self.progress_buffer, 0, bytemuck::cast_slice(&[uniform]));

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("loading screen encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loading screen pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.10,
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
            render_pass.set_bind_group(0, &self.progress_bind_group, &[]);
            render_pass.draw(0..3, 0..1); // fullscreen triangle
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Consume this loading screen and return the GPU surface, device, and queue
    /// so they can be reused by the main renderer.
    ///
    /// This avoids creating a second GPU adapter/device when transitioning
    /// from the loading screen to the full renderer.
    pub fn into_gpu_parts(
        self,
    ) -> (
        wgpu::Surface<'static>,
        wgpu::Device,
        wgpu::Queue,
        wgpu::SurfaceConfiguration,
    ) {
        (self.surface, self.device, self.queue, self.config)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn loading_uniform_size_is_16_byte_aligned() {
        // The uniform struct must be valid for bytemuck casting
        let size = std::mem::size_of::<super::LoadingUniform>();
        assert_eq!(
            size % 16,
            0,
            "LoadingUniform must be 16-byte aligned for GPU"
        );
    }
}
