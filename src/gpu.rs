use std::sync::Arc;
use std::time::Instant;
use winit::window::Window;
use crate::world::{Brush, Player, MAX_BRUSHES, WorldUniforms};
use crate::materials::{GPUMaterials, MaterialRegistry};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms
{
    pub time: f32,
    pub aspect_ratio: f32,
    pub pixel_size_at_1m: f32,
    pub selected_id: i32,
}

pub struct GPUWorld
{
    pub brush_buffer: wgpu::Buffer,
    pub grid_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub player_buffer: wgpu::Buffer,
    pub world_uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl GPUWorld
{
    pub fn new(device: &wgpu::Device) -> Self
    {
        let brush_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Brush Buffer"),
            size: (MAX_BRUSHES * std::mem::size_of::<Brush>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Max grid size 512x512x512 * 4 bytes = 512MB
        // But we'll use a more reasonable limit for now.
        // Let's say 256MB for grid.
        let grid_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Buffer"),
            size: 256 * 1024 * 1024,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Index Buffer"),
            size: 128 * 1024 * 1024,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let player_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Player Buffer"),
            size: std::mem::size_of::<Player>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let world_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("World Uniform Buffer"),
            size: std::mem::size_of::<WorldUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("World Bind Group Layout"),
            entries: &[
                // Brushes
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Grid
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Indices
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Player
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // World Uniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("World Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: brush_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: grid_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: index_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: player_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: world_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            brush_buffer,
            grid_buffer,
            index_buffer,
            player_buffer,
            world_uniform_buffer,
            bind_group_layout,
            bind_group,
        }
    }
}

/// Rendering and windowing state
/// On mobile, this can be recreated multiple times
#[allow(dead_code)]
pub struct GPUState
{
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub render_pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
    pub window: Arc<Window>,
    pub gpu_world: GPUWorld,
    pub gpu_materials: GPUMaterials,
}

impl GPUState
{
    pub async fn new(window: Arc<Window>, materials: &MaterialRegistry) -> Self
    {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(Arc::clone(&window)).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits {
                        max_storage_buffer_binding_size: 256 * 1024 * 1024,
                        max_buffer_size: 512 * 1024 * 1024,
                        ..wgpu::Limits::default()
                    },
                    memory_hints: wgpu::MemoryHints::default(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        device.on_uncaptured_error(std::sync::Arc::new(|err| {
            eprintln!("wgpu uncaptured GPU error: {err}");
        }));

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let present_mode = wgpu::PresentMode::AutoNoVsync;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: 800,
            height: 600,
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                label: Some("uniform_bind_group_layout"),
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some("uniform_bind_group"),
        });

        let gpu_world = GPUWorld::new(&device);

        let material_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Material Bind Group Layout"),
            entries: &[
                // Texture Array
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Specular Buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let gpu_materials = GPUMaterials::new(&device, &queue, materials, &material_bind_group_layout);

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    Some(&uniform_bind_group_layout),
                    Some(&gpu_world.bind_group_layout),
                    Some(&material_bind_group_layout),
                ],
                immediate_size: 0,
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            uniform_buffer,
            uniform_bind_group,
            window,
            gpu_world,
            gpu_materials,
        }
    }

    pub fn render(&mut self, start_time: &Instant, focal_length: f32, selected_id: i32)
    {
        // Flush any pending GPU errors from the previous frame so the
        // uncaptured error handler fires promptly if the GPU timed out.
        let _ = self.device.poll(wgpu::PollType::Poll);
        let size = self.window.inner_size();

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated => {
                eprintln!("Surface outdated — reconfiguring");
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                eprintln!("Surface lost");
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                eprintln!("Surface timeout");
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("Surface validation error — GPU likely rejected the previous frame");
                return;
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                // This is normal, it happens when the app is not in focus
                return;
            }
        };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.encode_frame(&view, size.width, size.height, start_time, focal_length, selected_id);
        output.present();
    }

    /// Render a single frame to an offscreen texture and write it to `path` as a PNG.
    pub fn capture_screenshot(&self, start_time: &Instant, focal_length: f32, path: &std::path::Path)
    {
        let size = self.window.inner_size();
        let width = size.width;
        let height = size.height;

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Screenshot Texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.encode_frame(&view, width, height, start_time, focal_length, -1);

        // wgpu requires bytes_per_row to be a multiple of COPY_BYTES_PER_ROW_ALIGNMENT,
        // so we pad each row in the readback buffer and strip the padding when writing the PNG.
        let bytes_per_pixel = 4u32;
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;

        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Screenshot Readback"),
            size: (padded_bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Screenshot Copy Encoder"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());

        let padded = slice.get_mapped_range();
        let bgra = matches!(
            self.config.format,
            wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm
        );
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let row_start = (row * padded_bytes_per_row) as usize;
            for col in 0..width {
                let i = row_start + (col * 4) as usize;
                if bgra {
                    rgba.extend_from_slice(&[padded[i + 2], padded[i + 1], padded[i], padded[i + 3]]);
                } else {
                    rgba.extend_from_slice(&padded[i..i + 4]);
                }
            }
        }
        drop(padded);
        readback.unmap();

        let file = std::fs::File::create(path).expect("failed to create screenshot file");
        let writer = std::io::BufWriter::new(file);
        let mut png_encoder = png::Encoder::new(writer, width, height);
        png_encoder.set_color(png::ColorType::Rgba);
        png_encoder.set_depth(png::BitDepth::Eight);
        let mut png_writer = png_encoder.write_header().expect("failed to write PNG header");
        png_writer.write_image_data(&rgba).expect("failed to write PNG data");

        println!("Saved screenshot: {}", path.display());
    }

    fn encode_frame(
        &self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        start_time: &Instant,
        focal_length: f32,
        selected_id: i32,
    ) {
        let pixel_size_at_1m = (2.0 / height as f32) / focal_length;
        let uniforms = Uniforms {
            time: start_time.elapsed().as_secs_f32(),
            aspect_ratio: width as f32 / height as f32,
            pixel_size_at_1m,
            selected_id,
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_bind_group(1, &self.gpu_world.bind_group, &[]);
        render_pass.set_bind_group(2, &self.gpu_materials.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
        drop(render_pass);

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
