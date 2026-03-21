mod world;
mod math;
mod gpu;
mod materials;

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
use materials::MaterialRegistry;
use math::*;
use world::Brush;

#[derive(PartialEq)]
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
    materials: MaterialRegistry,

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

        let mut world = world::World::new();
        let materials = MaterialRegistry::load();

        // Add a default floor brush
        world.add_brush(Brush {
            pos: Vec3::new(0.0, -0.05, 0.0),
            kind: world::KIND_BOX,
            scale: Vec3::new(40.0, 0.1, 40.0),
            material: materials.id_from_name("grass_01"),
            rot: Quat::IDENTITY,
            op: world::OP_ADD,
            _pad: [0; 3],
        });

        Self {
            gpu_state: None,
            world,
            materials,
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
        use crate::math::Vec3;
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

                let mut pos = self.world.player.position + self.world.player.forward * 3.0;

                // Align the brush position to the nearest multiple of 0.1
                pos.x = (pos.x * 10.0).round() / 10.0;
                pos.y = (pos.y * 10.0).round() / 10.0;
                pos.z = (pos.z * 10.0).round() / 10.0;

                let brush_id = self.world.add_brush(world::Brush {
                    pos,
                    kind: world::KIND_BOX,
                    scale: math::Vec3::new(1.0, 1.0, 1.0),
                    material: self.materials.id_from_name("metal_01"),
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

            // Flip to the next material
            KeyM => {
                if let Some(brush_id) = self.selected {
                    let num_materials = self.materials.num_materials();
                    let mut brush = self.world.remove_brush(brush_id);

                    brush.material = (brush.material + 1) % num_materials;
                    let material_name = self.materials.material_name(brush.material);
                    println!("Material: {} (material id={})", material_name, brush.material);

                    self.selected = Some(self.world.add_brush(brush));
                    self.upload_world();
                    return;
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

            KeyP => {
                println!("Position edit mode");
                self.edit_mode = EditMode::Position;
            }
            KeyX => {
                println!("Scale edit mode");
                self.edit_mode = EditMode::Scale;
            }
            KeyR => {
                println!("Rotation edit mode");
                self.edit_mode = EditMode::Rotation;
            }

            // Scale the currently selected brush in EditMode::Scale
            KeyI | KeyK | KeyJ | KeyL | KeyU | KeyH if self.edit_mode == EditMode::Scale => {
                let Some(brush_id) = self.selected else { return; };

                let mut brush = self.world.remove_brush(brush_id);
                let player = &self.world.player;

                let (axis_idx, delta) = match key {
                    // Y/H always control the vertical Y axis
                    KeyU => (1, 0.1),
                    KeyH => (1, -0.1),

                    // J/L scales along the axis most aligned with player's right
                    KeyJ | KeyL => {
                        let axis = if player.right.x.abs() > player.right.z.abs() { 0 } else { 2 };
                        let delta = if key == KeyL { 0.1 } else { -0.1 };
                        (axis, delta)
                    }

                    // I/K scales along the axis most aligned with player's forward
                    KeyI | KeyK => {
                        let axis = if player.forward.x.abs() > player.forward.z.abs() { 0 } else { 2 };
                        let delta = if key == KeyI { 0.1 } else { -0.1 };
                        (axis, delta)
                    }
                    _ => unreachable!(),
                };

                // Capture old scale to calculate actual change after clamping
                let old_scale = brush.scale;

                // Apply the scaling to the chosen axis
                if axis_idx == 0 { brush.scale.x += delta; }
                else if axis_idx == 1 { brush.scale.y += delta; }
                else { brush.scale.z += delta; }

                // Ensure scale doesn't become too small or negative
                brush.scale.x = brush.scale.x.max(0.1);
                brush.scale.y = brush.scale.y.max(0.1);
                brush.scale.z = brush.scale.z.max(0.1);

                // Calculate the actual change in scale
                let actual_delta = brush.scale - old_scale;

                // For Box, adjust position to keep a specific corner fixed
                if brush.kind == world::KIND_BOX {
                    let player = &self.world.player;

                    // Identify the 4 corners of the base (lowest Y)
                    let x_min = brush.pos.x - 0.5 * brush.scale.x;
                    let x_max = brush.pos.x + 0.5 * brush.scale.x;
                    let z_min = brush.pos.z - 0.5 * brush.scale.z;
                    let z_max = brush.pos.z + 0.5 * brush.scale.z;
                    let y_min = brush.pos.y - 0.5 * brush.scale.y;

                    let corners = [
                        Vec3::new(x_min, y_min, z_min), // 0: min, min
                        Vec3::new(x_max, y_min, z_min), // 1: max, min
                        Vec3::new(x_min, y_min, z_max), // 2: min, max
                        Vec3::new(x_max, y_min, z_max), // 3: max, max
                    ];

                    let mut best_corner_idx = 0;
                    let mut min_score = f32::INFINITY;

                    // Find the corner that is "leftmost and nearest" relative to player view
                    for (i, c) in corners.iter().enumerate() {
                        let to_corner = *c - player.position;
                        // Score: distance along view (forward) + distance to right
                        // Minimizing this finds the "front-left" corner from player's perspective
                        let score = to_corner.dot(player.forward) + to_corner.dot(player.right);
                        if score < min_score {
                            min_score = score;
                            best_corner_idx = i;
                        }
                    }

                    // Determine signs for position adjustment based on the static corner
                    let s_x = if best_corner_idx % 2 == 0 { 1.0 } else { -1.0 };
                    let s_z = if best_corner_idx / 2 == 0 { 1.0 } else { -1.0 };

                    // Apply the adjustment: fix X/Z at the chosen corner, always fix Y at the base
                    brush.pos.x += actual_delta.x * 0.5 * s_x;
                    brush.pos.z += actual_delta.z * 0.5 * s_z;
                    brush.pos.y += actual_delta.y * 0.5;
                } else if brush.kind == world::KIND_CYLINDER || brush.kind == world::KIND_CONE {
                    // For Cylinder/Cone, fix the base Y position when scaling along Y
                    brush.pos.y += actual_delta.y * 0.5;
                }

                self.selected = Some(self.world.add_brush(brush));
                self.upload_world();
            }

            // Move the currently selected brush in EditMode::Position
            // Movement is axis-aligned but chosen based on player view
            KeyI | KeyK | KeyJ | KeyL | KeyU | KeyH if self.edit_mode == EditMode::Position => {
                let Some(brush_id) = self.selected else { return; };

                let mut brush = self.world.remove_brush(brush_id);
                let player = &self.world.player;

                let move_vec = match key {
                    // Y/H always control the vertical Y axis
                    KeyU => Vec3::new(0.0, 1.0, 0.0),
                    KeyH => Vec3::new(0.0, -1.0, 0.0),

                    // J/L moves left/right relative to player, constrained to horizontal X or Z
                    KeyJ | KeyL => {
                        let dir = if key == KeyL { player.right } else { -player.right };
                        if dir.x.abs() > dir.z.abs() {
                            Vec3::new(if dir.x > 0.0 { 1.0 } else { -1.0 }, 0.0, 0.0)
                        } else {
                            Vec3::new(0.0, 0.0, if dir.z > 0.0 { 1.0 } else { -1.0 })
                        }
                    }

                    // I/K moves away/closer relative to player, constrained to horizontal X or Z
                    KeyI | KeyK => {
                        let dir = if key == KeyI { player.forward } else { -player.forward };
                        if dir.x.abs() > dir.z.abs() {
                            Vec3::new(if dir.x > 0.0 { 1.0 } else { -1.0 }, 0.0, 0.0)
                        } else {
                            Vec3::new(0.0, 0.0, if dir.z > 0.0 { 1.0 } else { -1.0 })
                        }
                    }
                    _ => unreachable!(),
                };

                // Apply the axis-aligned movement
                brush.pos += move_vec * 0.1;
                self.selected = Some(self.world.add_brush(brush));
                self.upload_world();
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

        let gpu_state = pollster::block_on(gpu::GPUState::new(Arc::clone(&window), &self.materials));

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
