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

enum EditMode
{
    Position,
    Rotation,
    Scale,
}

struct App
{
    gpu_state: Option<gpu::GPUState>,
    world: world::World,

    /// Delta time measurement
    start_time: Instant,
    last_update: Instant,

    /// Frame count measurement
    frame_count: u32,
    last_fps_print: Instant,

    key_down: HashSet<KeyCode>,

    /// Currently selected brush
    selected: Option<u16>,

    /// Current brush edit mode
    edit_mode: EditMode,
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
            frame_count: 0,
            last_fps_print: now,
            key_down: HashSet::new(),
            selected: None,
            edit_mode: EditMode::Position,
        }
    }

    fn update(&mut self)
    {
        // Compute delta time since last update
        let dt = self.last_update.elapsed().as_secs_f32();
        self.last_update = Instant::now();

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

    fn key_press(&mut self, key: KeyCode)
    {
        use crate::world::*;
        use KeyCode::*;

        match key {
            KeyO => {
                // If a brush is currently selected
                if let Some(brush_id) = self.selected {
                    let mut brush = self.world.remove_brush(brush_id);
                    brush.kind = (brush.kind + 1) % NUM_BRUSH_KINDS;
                    self.selected = Some(self.world.add_brush(brush));
                    self.upload_world();
                    return;
                }

                // TODO: wall brush
                let pos = self.world.player.position + self.world.player.forward * 3.0;
                let brush_id = self.world.add_brush(world::Brush {
                    pos,
                    kind: world::KIND_BOX,
                    scale: math::Vec3::new(1.0, 1.0, 1.0),
                    material: world::MAT_WOOD,
                    rot: math::Quat::IDENTITY,
                    op: world::OP_ADD,
                    _pad: [0; 3],
                });

                self.selected = Some(brush_id);
                self.edit_mode = EditMode::Position;

                self.upload_world();
            }

            Delete | Backspace => {
                println!("delete key");
                if let Some(brush_id) = self.selected {
                    self.world.remove_brush(brush_id);
                    self.upload_world();
                    self.selected = None;
                }
            }

            Enter => {
                // Add the brush to the world but keep a selected copy
                if let Some(brush_id) = self.selected {
                    let brush = self.world.remove_brush(brush_id);
                    self.world.add_brush(brush);
                    self.selected = Some(self.world.add_brush(brush));
                    self.edit_mode = EditMode::Position;
                    self.upload_world();
                    return;
                }
            }

            KeyP => { self.edit_mode = EditMode::Position; }
            KeyS => { self.edit_mode = EditMode::Scale; }
            KeyR => { self.edit_mode = EditMode::Rotation; }

            // Move the currently selected brush in EditMode::Position
            KeyI | KeyK | KeyJ | KeyL => {
                if let Some(brush_id) = self.selected {
                    if matches!(self.edit_mode, EditMode::Position) {
                        let mut brush = self.world.remove_brush(brush_id);

                        match key {
                            KeyI => { brush.pos.x += 0.1; }
                            KeyK => { brush.pos.x -= 0.1; }
                            KeyJ => { brush.pos.z -= 0.1; }
                            KeyL => { brush.pos.z += 0.1; }
                            _ => {}
                        }

                        self.selected = Some(self.world.add_brush(brush));
                        self.upload_world();
                    }
                }
            }

            _ => {}
        }
    }

    fn upload_world(&self)
    {
        if let Some(gpu_state) = &self.gpu_state {
            self.world.upload_world(&gpu_state.queue, &gpu_state.gpu_world);
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
                        self.key_press(key);
                    }
                    ElementState::Released => {
                        self.key_down.remove(&key);
                    }
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
                // Update FPS counter
                self.frame_count += 1;
                let fps_elapsed = self.last_fps_print.elapsed();
                if fps_elapsed.as_secs_f32() >= 1.0 {
                    let fps = self.frame_count as f32 / fps_elapsed.as_secs_f32();
                    println!("FPS: {:.2}", fps);
                    self.frame_count = 0;
                    self.last_fps_print = Instant::now();
                }

                self.update();

                if let Some(gpu_state) = self.gpu_state.as_mut() {
                    self.world.upload_player(&gpu_state.queue, &gpu_state.gpu_world);

                    match gpu_state.render(&self.start_time, self.world.player.focal_length) {
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
