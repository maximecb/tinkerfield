use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, KeyEvent, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

mod world;
mod math;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms
{
    time: f32,
    aspect_ratio: f32,
    _padding: [f32; 2],
}

struct State
{
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    render_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    start_time: Instant,
    last_update: Instant,
    window: Arc<Window>,
    world: world::World,
}

impl State
{
    async fn new(window: Arc<Window>) -> Self
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
                        max_buffer_size: 256 * 1024 * 1024,
                        ..wgpu::Limits::default()
                    },
                    memory_hints: wgpu::MemoryHints::default(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: 800,
            height: 600,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
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

        let world = world::World::new(&device);
        world.upload_world(&queue);
        world.upload_player(&queue);

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&uniform_bind_group_layout, &world.gpu.bind_group_layout],
                push_constant_ranges: &[],
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
            multiview: None,
            cache: None,
        });

        Self {
            surface,
            device,
            queue,
            render_pipeline,
            uniform_buffer,
            uniform_bind_group,
            start_time: Instant::now(),
            last_update: Instant::now(),
            window,
            world,
        }
    }

    fn update(&mut self, key_down: &HashSet<KeyCode>)
    {
        let dt = self.last_update.elapsed().as_secs_f32();
        self.last_update = Instant::now();

        let move_speed = 10.0;
        let mut fwd_dist = 0.0;
        let mut side_dist = 0.0;

        if key_down.contains(&KeyCode::KeyW) || key_down.contains(&KeyCode::ArrowUp) {
            fwd_dist += move_speed * dt;
        }
        if key_down.contains(&KeyCode::KeyS) || key_down.contains(&KeyCode::ArrowDown) {
            fwd_dist -= move_speed * dt;
        }
        if key_down.contains(&KeyCode::KeyA) || key_down.contains(&KeyCode::ArrowLeft) {
            side_dist -= move_speed * dt;
        }
        if key_down.contains(&KeyCode::KeyD) || key_down.contains(&KeyCode::ArrowRight) {
            side_dist += move_speed * dt;
        }

        if fwd_dist != 0.0 || side_dist != 0.0 {
            self.world.move_player(fwd_dist, side_dist);
        }

        let uniforms = Uniforms {
            time: self.start_time.elapsed().as_secs_f32(),
            aspect_ratio: 800.0 / 600.0,
            _padding: [0.0; 2],
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        self.world.upload_player(&self.queue);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError>
    {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
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
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_bind_group(1, &self.world.gpu.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
        drop(render_pass);

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

struct App
{
    state: Option<State>,
    cursor_initialized: bool,
    key_down: HashSet<KeyCode>,
}

impl ApplicationHandler for App
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop)
    {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("TinkerField")
                        .with_inner_size(LogicalSize::new(800.0, 600.0))
                        .with_resizable(false),
                )
                .unwrap(),
        );

        let state = pollster::block_on(State::new(Arc::clone(&window)));

        // Try to grab the cursor after everything is initialized
        let _ = window.set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        window.set_cursor_visible(false);

        self.state = Some(state);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent)
    {
        match event {
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        ..
                    },
                ..
            } => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state,
                        ..
                    },
                ..
            } => {
                match state {
                    ElementState::Pressed => { self.key_down.insert(key); }
                    ElementState::Released => { self.key_down.remove(&key); }
                }
            }
            WindowEvent::RedrawRequested => {
                let state = self.state.as_mut().unwrap();
                if !self.cursor_initialized {
                    let _ = state.window.set_cursor_grab(CursorGrabMode::Locked)
                        .or_else(|_| state.window.set_cursor_grab(CursorGrabMode::Confined));
                    state.window.set_cursor_visible(false);
                    self.cursor_initialized = true;
                }

                state.update(&self.key_down);
                match state.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(e) => eprintln!("{:?}", e),
                }
                state.window.request_redraw();
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: DeviceId, event: DeviceEvent)
    {
        if let Some(state) = self.state.as_mut() {
            if let DeviceEvent::MouseMotion { delta } = event {
                let sensitivity = 0.1;
                state.world.rotate_player(
                    delta.0 as f32 * sensitivity,
                    -delta.1 as f32 * sensitivity,
                );
            }
        }
    }
}

fn main()
{
    let event_loop = EventLoop::new().unwrap();
    let mut app = App {
        state: None,
        cursor_initialized: false,
        key_down: HashSet::new()
    };
    event_loop.run_app(&mut app).unwrap();
}
