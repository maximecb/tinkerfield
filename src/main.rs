#![allow(unused_parens)]

mod world;
mod math;
mod gpu;
mod materials;
mod lexer;
mod maps;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, KeyEvent, ElementState, MouseButton, WindowEvent},
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

    /// Keys that are currently pressed
    key_down: HashSet<KeyCode>,

    /// Currently selected brush
    selected: Option<u16>,

    /// Copied brush
    copied: Option<Brush>,

    /// Current brush edit mode
    edit_mode: EditMode,

    /// World axes captured when Shift/Alt is pressed, used for mouse-driven editing.
    /// axis0 is driven by mouse X, axis1 by mouse Y (inverted).
    edit_axes: Option<(Vec3, Vec3)>,

    /// Accumulated sub-grid mouse movement, carried forward until it crosses a grid boundary.
    drag_remainder: Vec3,

    /// Set on Focused(true); cursor recenter is deferred to the next frame to avoid
    /// triggering a window drag when refocusing via the title bar on macOS.
    pending_recenter: bool,

    /// Map file to load, also used for Ctrl+R hot-reload and Ctrl+S save.
    /// Defaults to "untitled.map" when no map argument was provided.
    map_file: PathBuf,
}

impl App
{
    fn new(map_file: Option<PathBuf>) -> Self
    {
        let now = Instant::now();

        let world = world::World::new();
        let materials = MaterialRegistry::load();

        let load_from_arg = map_file.is_some();
        let map_file = map_file.unwrap_or_else(|| PathBuf::from("untitled.map"));

        let mut app = Self {
            gpu_state: None,
            world,
            materials,
            start_time: now,
            last_update: now,
            frame_count: 0,
            last_fps_print: now,
            key_down: HashSet::new(),
            selected: None,
            copied: None,
            edit_mode: EditMode::Position,
            edit_axes: None,
            drag_remainder: Vec3::new(0.0, 0.0, 0.0),
            pending_recenter: false,
            map_file,
        };

        if load_from_arg {
            app.load_map();
            return app;
        }

        // Grass surface
        app.world.add_brush(Brush {
            pos: Vec3::new(0.0, -0.05, 0.0),
            kind: world::KIND_BOX,
            scale: Vec3::new(60.0, 0.1, 60.0),
            material: app.materials.id_from_name("grass_01"),
            rot: Quat::IDENTITY,
            op: world::OP_ADD,
            _pad: [0; 3],
        });

        // Dirt under the grass
        app.world.add_brush(Brush {
            pos: Vec3::new(0.0, -8.05, 0.0),
            kind: world::KIND_BOX,
            scale: Vec3::new(60.0, 16.0, 60.0),
            material: app.materials.id_from_name("dirt_01"),
            rot: Quat::IDENTITY,
            op: world::OP_ADD,
            _pad: [0; 3],
        });

        app
    }

    /// Load or reload the map
    fn load_map(&mut self)
    {
        println!("Loading map: {}", self.map_file.display());

        match maps::parse_map(&self.map_file, &self.materials) {
            Ok(world) => {
                self.world = world;
                self.selected = None;
                self.upload_world();
            }
            Err(e) => {
                eprintln!("Map parse error: {}", e);
            }
        }
    }

    /// Save the map back to its file
    fn save_map(&self)
    {
        match maps::save_map(&self.map_file, &self.world, &self.materials) {
            Ok(()) => println!("Saved map: {}", self.map_file.display()),
            Err(e) => eprintln!("Map save error: {}", e),
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

        // Ctrl + Key
        if self.key_down.contains(&ControlLeft) || self.key_down.contains(&SuperLeft) {
            match key {
                // Copy selected object
                KeyC => {
                    if let Some(brush_id) = self.selected {
                        let brush = self.world.get_brush(brush_id);
                        self.copied = Some(brush);
                    }
                }

                // Paste copied object in front of player
                KeyV => {
                    if let Some(copied) = self.copied.clone() {
                        let mut brush = copied;
                        let mut pos = self.world.player.position + self.world.player.forward * 3.0;
                        pos.x = (pos.x * 10.0).round() / 10.0;
                        pos.y = (pos.y * 10.0).round() / 10.0;
                        pos.z = (pos.z * 10.0).round() / 10.0;
                        brush.pos = pos;
                        let brush_id = self.world.add_brush(brush);
                        self.selected = Some(brush_id);
                        self.edit_mode = EditMode::Position;
                        self.upload_world();
                    }
                }

                // Hot reload the map from file
                KeyR => {
                    self.load_map();
                }

                // Save the map back to file
                KeyS => {
                    self.save_map();
                }

                _ => {}
            }

            return;
        }

        match key {
            // Capture edit axes when Shift is pressed: right-aligned horizontal + Y
            ShiftLeft | ShiftRight => {
                let player = &self.world.player;
                let axis0 = if player.right.x.abs() > player.right.z.abs() {
                    Vec3::new(player.right.x.signum(), 0.0, 0.0)
                } else {
                    Vec3::new(0.0, 0.0, player.right.z.signum())
                };
                self.edit_axes = Some((axis0, Vec3::new(0.0, 1.0, 0.0)));
                self.drag_remainder = Vec3::new(0.0, 0.0, 0.0);
            }

            // Capture edit axes when Alt is pressed: right-aligned horizontal + forward-aligned horizontal
            AltLeft | AltRight => {
                let player = &self.world.player;
                let axis0 = if player.right.x.abs() > player.right.z.abs() {
                    Vec3::new(player.right.x.signum(), 0.0, 0.0)
                } else {
                    Vec3::new(0.0, 0.0, player.right.z.signum())
                };
                let axis1 = if player.forward.x.abs() > player.forward.z.abs() {
                    Vec3::new(player.forward.x.signum(), 0.0, 0.0)
                } else {
                    Vec3::new(0.0, 0.0, player.forward.z.signum())
                };
                self.edit_axes = Some((axis0, axis1));
                self.drag_remainder = Vec3::new(0.0, 0.0, 0.0);
            }

            // Create a new object
            KeyO => {
                let mut pos = self.world.player.position + self.world.player.forward * 3.0;

                // Align the brush position to the nearest multiple of 0.1
                pos.x = (pos.x * 10.0).round() / 10.0;
                pos.y = (pos.y * 10.0).round() / 10.0;
                pos.z = (pos.z * 10.0).round() / 10.0;

                let brush_id = self.world.add_brush(Brush {
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

            // Delete selected object
            Delete | Backspace => {
                if let Some(brush_id) = self.selected {
                    self.world.remove_brush(brush_id);
                    self.upload_world();
                    self.selected = None;
                }
            }

            // Unselect object
            Enter => {
                self.selected = None;
                //self.upload_world();
            }

            // Subtract a cylinder in front of the player
            KeyC => {
                let pos = self.world.player.position + self.world.player.forward * 1.0;

                // Create a rotation that aligns the cylinder's local Y axis with the camera forward
                let yaw_rad = self.world.player.yaw.to_radians();
                let pitch_rad = self.world.player.pitch.to_radians();
                let q_player = math::Quat::from_rotation_y(yaw_rad) * math::Quat::from_rotation_x(-pitch_rad);
                let rotation = q_player * math::Quat::from_rotation_x(90.0f32.to_radians());

                self.world.add_brush(Brush {
                    pos,
                    kind: world::KIND_CYLINDER,
                    scale: math::Vec3::new(0.7, 5.0, 0.7),
                    material: self.materials.id_from_name("metal_01"),
                    rot: rotation,
                    op: world::OP_SUB,
                    _pad: [0; 3],
                });

                self.upload_world();
            }

            // Subtract the selected object from the world
            KeyQ => {
                if let Some(brush_id) = self.selected {
                    let mut brush = self.world.get_brush(brush_id);
                    brush.op = world::OP_SUB;
                    self.world.set_brush(brush_id, brush);
                    self.upload_world();
                }
            }

            // Switch the type of the selected object
            KeyT => {
                // If a brush is currently selected
                if let Some(brush_id) = self.selected {
                    let mut brush = self.world.get_brush(brush_id);
                    brush.kind = (brush.kind + 1) % NUM_BRUSH_KINDS;
                    self.world.set_brush(brush_id, brush);
                    self.upload_world();
                    return;
                }
            }

            // Flip to the previous material
            KeyN => {
                if let Some(brush_id) = self.selected {
                    let num_materials = self.materials.num_materials();
                    let mut brush = self.world.get_brush(brush_id);

                    brush.material = if brush.material > 0 {
                        brush.material - 1
                    } else {
                        num_materials - 1
                    };
                    let material_name = self.materials.material_name(brush.material);
                    println!("Material: {} (material id={})", material_name, brush.material);

                    self.world.set_brush(brush_id, brush);
                    self.upload_world();
                    return;
                }
            }

            // Flip to the next material
            KeyM => {
                if let Some(brush_id) = self.selected {
                    let num_materials = self.materials.num_materials();
                    let mut brush = self.world.get_brush(brush_id);

                    brush.material = (brush.material + 1) % num_materials;
                    let material_name = self.materials.material_name(brush.material);
                    println!("Material: {} (material id={})", material_name, brush.material);

                    self.world.set_brush(brush_id, brush);
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

            _ => {}
        }
    }

    fn mouse_move(&mut self, dx: f64, dy: f64)
    {
        let shift_held = self.key_down.contains(&KeyCode::ShiftLeft)
            || self.key_down.contains(&KeyCode::ShiftRight);
        let alt_held = self.key_down.contains(&KeyCode::AltLeft)
            || self.key_down.contains(&KeyCode::AltRight);

        if (shift_held || alt_held) && self.edit_mode == EditMode::Position {
            if let (Some(brush_id), Some((axis0, axis1))) = (self.selected, self.edit_axes) {
                // Accumulate raw mouse delta along the two edit axes
                let sensitivity = 0.01;
                self.drag_remainder += axis0 * (dx as f32 * sensitivity);
                self.drag_remainder += axis1 * (-dy as f32 * sensitivity);

                // Extract the grid-aligned portion; carry the sub-grid remainder forward
                let snapped = self.drag_remainder.snap(0.1);
                self.drag_remainder -= snapped;

                // Only rebuild the world if the position actually changed
                if snapped.length_sq() > 0.0 {
                    let mut brush = self.world.get_brush(brush_id);
                    brush.pos = (brush.pos + snapped).snap(0.1);
                    self.world.set_brush(brush_id, brush);
                    self.upload_world();
                }
                return;
            }
        }

        if (shift_held || alt_held) && self.edit_mode == EditMode::Scale {
            if let (Some(brush_id), Some((axis0, axis1))) = (self.selected, self.edit_axes) {
                // Accumulate along absolute-value axes so mouse-right always grows
                // the player's right face regardless of which world direction axis0
                // points in. The signed axes are still used below to pin the correct face.
                let sensitivity = 0.01;
                let abs0 = Vec3::new(axis0.x.abs(), axis0.y.abs(), axis0.z.abs());
                let abs1 = Vec3::new(axis1.x.abs(), axis1.y.abs(), axis1.z.abs());
                self.drag_remainder += abs0 * (dx as f32 * sensitivity);
                self.drag_remainder += abs1 * (-dy as f32 * sensitivity);

                // Extract the grid-aligned portion; carry the sub-grid remainder forward
                let snapped = self.drag_remainder.snap(0.1);
                self.drag_remainder -= snapped;

                // Only rebuild the world if the scale actually changed
                if snapped.length_sq() > 0.0 {
                    let mut brush = self.world.get_brush(brush_id);
                    let old_scale = brush.scale;

                    brush.scale += snapped;
                    brush.scale.x = brush.scale.x.max(0.1);
                    brush.scale.y = brush.scale.y.max(0.1);
                    brush.scale.z = brush.scale.z.max(0.1);

                    let actual_delta = brush.scale - old_scale;

                    if brush.kind == world::KIND_BOX {
                        // Pin the face opposite to each drag axis: if the axis points in +Z,
                        // scale grows toward +Z so the -Z face (z_min) stays fixed, and vice versa.
                        let s_x = (axis0.x + axis1.x).signum();
                        let s_z = (axis0.z + axis1.z).signum();
                        brush.pos.x += actual_delta.x * 0.5 * s_x;
                        brush.pos.z += actual_delta.z * 0.5 * s_z;
                        brush.pos.y += actual_delta.y * 0.5;
                    } else if brush.kind == world::KIND_CYLINDER || brush.kind == world::KIND_CONE {
                        // Cylinders and cones grow upwards from the base
                        brush.pos.y += actual_delta.y * 0.5;
                    }

                    self.world.set_brush(brush_id, brush);
                    self.upload_world();
                }
                return;
            }
        }

        let sensitivity = 0.1;
        self.world.rotate_player(
            dx as f32 * sensitivity,
            -dy as f32 * sensitivity,
        );
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
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

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

        // Center, hide, and grab the cursor now. Safe to do directly here since no
        // title bar drag is in progress at window creation time.
        let size = window.inner_size();
        let _ = window.set_cursor_position(winit::dpi::PhysicalPosition::new(size.width / 2, size.height / 2));
        let _ = window.set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
        window.set_cursor_visible(false);

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
                        let ctrl_down = (
                            self.key_down.contains(&KeyCode::ControlLeft) ||
                            self.key_down.contains(&KeyCode::SuperLeft)
                        );

                        if !ctrl_down {
                            self.key_down.insert(key);
                        }

                        self.key_press(key);
                    }
                    ElementState::Released => {
                        self.key_down.remove(&key);
                    }
                }
            }

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let origin = self.world.player.position;
                let dir = self.world.player.forward;
                self.selected = self.world.ray_cast(origin, dir, true);
            }

            WindowEvent::Focused(true) => {
                if let Some(gpu_state) = self.gpu_state.as_ref() {
                    gpu_state.window.set_cursor_visible(false);
                }
                self.pending_recenter = true;
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

                    // Get the selected brush ID
                    let selected_id = self.selected.map(|id| id as i32).unwrap_or(-1);
                    gpu_state.render(&self.start_time, self.world.player.focal_length, selected_id);
                }
            }
            _ => {}
        }
    }

    /// Called when the event loop is done processing events
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop)
    {
        // Request a redraw as soon as we've finished processing events
        // This keeps the game loop running at maximum speed in Poll mode
        if let Some(gpu_state) = self.gpu_state.as_ref() {
            gpu_state.window.request_redraw();
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: DeviceId, event: DeviceEvent)
    {
        match event {
            DeviceEvent::MouseMotion { delta } => {
                self.mouse_move(delta.0, delta.1);
            }
            // Recenter on raw mouse-up; this fires for title bar clicks too,
            // which WindowEvent::MouseInput does not.
            DeviceEvent::Button { state: ElementState::Released, .. } => {
                if self.pending_recenter {
                    if let Some(gpu_state) = self.gpu_state.as_ref() {
                        let size = gpu_state.window.inner_size();
                        let _ = gpu_state.window.set_cursor_position(winit::dpi::PhysicalPosition::new(size.width / 2, size.height / 2));
                        let _ = gpu_state.window.set_cursor_grab(CursorGrabMode::Locked)
                            .or_else(|_| gpu_state.window.set_cursor_grab(CursorGrabMode::Confined));
                    }
                    self.pending_recenter = false;
                }
            }
            _ => {}
        }
    }
}

fn main()
{
    let map_file = std::env::args().nth(1).map(PathBuf::from);

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(map_file);
    event_loop.run_app(&mut app).unwrap();
}
