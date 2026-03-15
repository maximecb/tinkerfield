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
mod gpu;

struct App
{
    gpu_state: Option<gpu::GPUState>,
    world: world::World,
    start_time: Instant,
    last_update: Instant,
    key_down: HashSet<KeyCode>,
    frame_count: u32,
    last_fps_print: Instant,
}

impl App
{
    fn new() -> Self
    {
        let now = Instant::now();
        Self {
            gpu_state: None,
            world: world::World::new(),
            start_time: now,
            last_update: now,
            key_down: HashSet::new(),
            frame_count: 0,
            last_fps_print: now,
        }
    }

    fn update(&mut self)
    {
        let dt = self.last_update.elapsed().as_secs_f32();
        self.last_update = Instant::now();

        // Update FPS counter
        self.frame_count += 1;
        let fps_elapsed = self.last_fps_print.elapsed();
        if fps_elapsed.as_secs_f32() >= 1.0 {
            let fps = self.frame_count as f32 / fps_elapsed.as_secs_f32();
            println!("FPS: {:.2}", fps);
            self.frame_count = 0;
            self.last_fps_print = Instant::now();
        }

        let move_speed = 10.0;
        let mut fwd_dist = 0.0;
        let mut side_dist = 0.0;

        if self.key_down.contains(&KeyCode::KeyW) || self.key_down.contains(&KeyCode::ArrowUp) {
            fwd_dist += move_speed * dt;
        }
        if self.key_down.contains(&KeyCode::KeyS) || self.key_down.contains(&KeyCode::ArrowDown) {
            fwd_dist -= move_speed * dt;
        }
        if self.key_down.contains(&KeyCode::KeyA) || self.key_down.contains(&KeyCode::ArrowLeft) {
            side_dist -= move_speed * dt;
        }
        if self.key_down.contains(&KeyCode::KeyD) || self.key_down.contains(&KeyCode::ArrowRight) {
            side_dist += move_speed * dt;
        }

        if fwd_dist != 0.0 || side_dist != 0.0 {
            self.world.move_player(fwd_dist, side_dist);
        }
    }
}

impl ApplicationHandler for App
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop)
    {
        if self.gpu_state.is_some() {
            return;
        }

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

        let gpu_state = pollster::block_on(gpu::GPUState::new(Arc::clone(&window)));

        // Perform initial upload
        self.world.upload_world(&gpu_state.queue, &gpu_state.gpu_world);
        self.world.upload_player(&gpu_state.queue, &gpu_state.gpu_world);

        self.gpu_state = Some(gpu_state);
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
                    ElementState::Pressed => {
                        self.key_down.insert(key);

                        if key == KeyCode::KeyO {
                            let pos = self.world.player.position + self.world.player.forward * 5.0;
                            self.world.add_brush(world::Brush {
                                pos,
                                kind: world::KIND_BOX,
                                scale: math::Vec3::new(2.0, 2.0, 2.0),
                                material: world::MAT_WOOD,
                                rot: math::Quat::IDENTITY,
                                op: world::OP_ADD,
                                _pad: [0; 3],
                            });
                            if let Some(gpu_state) = &self.gpu_state {
                                self.world.upload_world(&gpu_state.queue, &gpu_state.gpu_world);
                            }
                        }

                        if key == KeyCode::KeyP {
                            let pos = self.world.player.position + self.world.player.forward * 3.0;
                            let rot = math::Quat::from_rotation_y(self.world.player.yaw.to_radians()) *
                                      math::Quat::from_rotation_x(-self.world.player.pitch.to_radians());
                            self.world.add_brush(world::Brush {
                                pos,
                                kind: world::KIND_CYLINDER,
                                scale: math::Vec3::new(1.0, 1.0, 6.0),
                                material: world::MAT_METAL,
                                rot,
                                op: world::OP_SUB,
                                _pad: [0; 3],
                            });
                            if let Some(gpu_state) = &self.gpu_state {
                                self.world.upload_world(&gpu_state.queue, &gpu_state.gpu_world);
                            }
                        }
                    }
                    ElementState::Released => { self.key_down.remove(&key); }
                }
            }

            WindowEvent::Focused(true) => {
                if let Some(gpu_state) = self.gpu_state.as_ref() {
                    let _ = gpu_state.window.set_cursor_grab(CursorGrabMode::Locked)
                        .or_else(|_| gpu_state.window.set_cursor_grab(CursorGrabMode::Confined));
                    gpu_state.window.set_cursor_visible(false);
                }
            }

            WindowEvent::RedrawRequested => {
                self.update();
                if let Some(gpu_state) = self.gpu_state.as_mut() {

                    self.world.upload_player(&gpu_state.queue, &gpu_state.gpu_world);

                    match gpu_state.render(&self.start_time) {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                        Err(e) => eprintln!("{:?}", e),
                    }
                    gpu_state.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: DeviceId, event: DeviceEvent)
    {
        if let DeviceEvent::MouseMotion { delta } = event {
            let sensitivity = 0.1;
            self.world.rotate_player(
                delta.0 as f32 * sensitivity,
                -delta.1 as f32 * sensitivity,
            );
        }
    }
}

fn main()
{
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
