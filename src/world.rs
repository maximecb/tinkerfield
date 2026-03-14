#![allow(dead_code)]

use std::time::Instant;
use crate::gpu::GPUWorld;
use crate::math::*;

/// Brush kinds
pub const KIND_BOX: u32 = 0;
pub const KIND_CYLINDER: u32 = 1;
pub const KIND_SPHERE: u32 = 2;
pub const KIND_CONE: u32 = 3;

/// CSG operations
pub const OP_ADD: u32 = 0;
pub const OP_SUB: u32 = 1;

/// Grid cell slot empty
pub const SLOT_EMPTY: u16 = u16::MAX;

/// Materials
pub const MAT_CONCRETE: u32 = 0;
pub const MAT_METAL: u32 = 1;
pub const MAT_WOOD: u32 = 2;
pub const MAT_GRASS: u32 = 3;

// Total size: 64 bytes
// Every vec3/vec4 field is 16-byte aligned
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Brush
{
    pub pos: Vec3,
    pub kind: u32,

    pub scale: Vec3,
    pub material: u32,

    pub rot: Quat,

    pub op: u32,
    pub _pad: [u32; 3],
}

impl Brush
{
    pub fn get_aabb(&self) -> (Vec3, Vec3)
    {
        let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);

        for i in 0..8 {
            let px_unit = if (i & 1) == 0 { -0.5 } else { 0.5 };
            let py_unit = if (i & 2) == 0 { -0.5 } else { 0.5 };
            let pz_unit = if (i & 4) == 0 { -0.5 } else { 0.5 };

            let p_local = self.scale * Vec3::new(px_unit, py_unit, pz_unit);
            let p_rotated = self.rot.rotate_vec(p_local);
            let p = self.pos + p_rotated;

            if p.x < min.x { min.x = p.x; }
            if p.y < min.y { min.y = p.y; }
            if p.z < min.z { min.z = p.z; }
            if p.x > max.x { max.x = p.x; }
            if p.y > max.y { max.y = p.y; }
            if p.z > max.z { max.z = p.z; }
        }

        (min, max)
    }
}

#[repr(C)]
#[derive(Default, Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Player
{
    pub position: Vec3,
    pub focal_length: f32,

    pub forward: Vec3,
    pub _pad1: f32,

    pub right: Vec3,
    pub _pad2: f32,

    pub up: Vec3,
    pub _pad3: f32,

    pub yaw: f32,
    pub pitch: f32,
    pub _pad4: [f32; 2],
}

impl Player
{
    pub fn update_basis(&mut self)
    {
        let yaw_rad = self.yaw.to_radians();
        let pitch_rad = self.pitch.to_radians();

        self.forward = Vec3::new(
            yaw_rad.sin() * pitch_rad.cos(),
            pitch_rad.sin(),
            yaw_rad.cos() * pitch_rad.cos(),
        );

        let fov_y = 70.0f32;
        self.focal_length = 1.0 / (fov_y.to_radians() * 0.5).tan();

        let world_up = Vec3::new(0.0, 1.0, 0.0);
        self.right = world_up.cross(self.forward).normalize();
        self.up = self.forward.cross(self.right);
    }
}

/// Maximum number of brushes in our game world
pub const MAX_BRUSHES: usize = (u16::MAX - 1) as usize;

/// 256 x 256 x 64 x (32 * 2) = 256MB
pub const GRID_W: usize = 256;
pub const GRID_D: usize = 256;
pub const GRID_H: usize = 64;
pub const GRID_C: usize = 32;
pub const GRID_COUNT: usize = GRID_W * GRID_D * GRID_H * GRID_C;

pub struct World
{
    brushes: Vec<Brush>,
    free_indices: Vec<u16>,

    /// The world uses a metric coordinate system.
    /// The grid is a 3D array of cells such that each cell
    /// is 1x1x1 unit in size, with 1 unit = 1 meter.
    /// Each cell contains a list of up to 32 brush indices (u16)
    grid: Box<[u16]>,

    pub player: Player,
}

impl World
{
    pub fn new() -> Self
    {
        let mut world = Self {
            brushes: Vec::with_capacity(1024),
            free_indices: Vec::new(),
            grid: vec![SLOT_EMPTY; GRID_COUNT].into_boxed_slice(),
            player: Player {
                position: Vec3::new(128.0, 1.8, 128.0),
                focal_length: 1.5,
                forward: Vec3::new(0.0, 0.0, 1.0),
                _pad1: 0.0,
                right: Vec3::new(1.0, 0.0, 0.0),
                _pad2: 0.0,
                up: Vec3::new(0.0, 1.0, 0.0),
                _pad3: 0.0,
                yaw: 0.0,
                pitch: 0.0,
                _pad4: [0.0; 2],
            },
        };

        world.player.update_basis();

        // Add a default floor brush
        world.add_brush(Brush {
            pos: Vec3::new(128.0, 0.0, 128.0),
            kind: KIND_BOX,
            scale: Vec3::new(40.0, 0.2, 40.0),
            material: MAT_CONCRETE,
            rot: Quat::IDENTITY,
            op: OP_ADD,
            _pad: [0; 3],
        });

        world
    }

    /// Rotate the player's view
    pub fn rotate_player(&mut self, yaw: f32, pitch: f32)
    {
        self.player.yaw += yaw;
        self.player.pitch = (self.player.pitch + pitch).clamp(-89.0, 89.0);
        self.player.update_basis();
    }

    /// Rotate the player's view
    pub fn move_player(&mut self, fwd_dist: f32, side_dist: f32)
    {
        self.player.position += self.player.forward * fwd_dist;
        self.player.position += self.player.right * side_dist;
    }

    /// Remove a brush from the world
    pub fn remove_brush(&mut self, index: u16)
    {
        if (index as usize) >= self.brushes.len() {
            return;
        }

        let brush = self.brushes[index as usize];
        let (min, max) = brush.get_aabb();

        // Grid bounds
        let x_min = (min.x.floor() as i32).max(0).min(GRID_W as i32 - 1) as usize;
        let x_max = (max.x.ceil() as i32).max(0).min(GRID_W as i32 - 1) as usize;
        let y_min = (min.y.floor() as i32).max(0).min(GRID_H as i32 - 1) as usize;
        let y_max = (max.y.ceil() as i32).max(0).min(GRID_H as i32 - 1) as usize;
        let z_min = (min.z.floor() as i32).max(0).min(GRID_D as i32 - 1) as usize;
        let z_max = (max.z.ceil() as i32).max(0).min(GRID_D as i32 - 1) as usize;

        for y in y_min..=y_max {
            for z in z_min..=z_max {
                for x in x_min..=x_max {
                    let cell_idx = ((y * GRID_D + z) * GRID_W + x) * GRID_C;

                    // Find and remove from slot list
                    let mut found_at = None;
                    for slot in 0..GRID_C {
                        if self.grid[cell_idx + slot] == index {
                            found_at = Some(slot);
                            break;
                        }
                    }

                    if let Some(start_slot) = found_at {
                        // Shift left
                        for slot in start_slot..GRID_C - 1 {
                            self.grid[cell_idx + slot] = self.grid[cell_idx + slot + 1];
                            if self.grid[cell_idx + slot] == SLOT_EMPTY {
                                break;
                            }
                        }
                        // Last slot always becomes empty if we shifted
                        self.grid[cell_idx + GRID_C - 1] = SLOT_EMPTY;
                    }
                }
            }
        }

        // Nullify the brush data
        self.brushes[index as usize] = Brush {
            pos: Vec3::new(0.0, 0.0, 0.0),
            kind: KIND_BOX,
            scale: Vec3::new(0.0, 0.0, 0.0),
            material: 0,
            rot: Quat::IDENTITY,
            op: OP_ADD,
            _pad: [0; 3],
        };

        self.free_indices.push(index);
    }

    /// Add a brush to the world grid
    pub fn add_brush(&mut self, brush: Brush) -> u16
    {
        let index = if let Some(free_idx) = self.free_indices.pop() {
            self.brushes[free_idx as usize] = brush;
            free_idx
        } else {
            let idx = self.brushes.len() as u16;
            if idx as usize >= MAX_BRUSHES {
                return u16::MAX;
            }
            self.brushes.push(brush);
            idx
        };

        // Compute extents of this object in the grid
        let (min, max) = brush.get_aabb();
        let x_min = (min.x.floor() as i32).max(0).min(GRID_W as i32 - 1) as usize;
        let x_max = (max.x.ceil() as i32).max(0).min(GRID_W as i32 - 1) as usize;
        let y_min = (min.y.floor() as i32).max(0).min(GRID_H as i32 - 1) as usize;
        let y_max = (max.y.ceil() as i32).max(0).min(GRID_H as i32 - 1) as usize;
        let z_min = (min.z.floor() as i32).max(0).min(GRID_D as i32 - 1) as usize;
        let z_max = (max.z.ceil() as i32).max(0).min(GRID_D as i32 - 1) as usize;

        for y in y_min..=y_max {
            for z in z_min..=z_max {
                for x in x_min..=x_max {
                    let cell_idx = ((y * GRID_D + z) * GRID_W + x) * GRID_C;
                    for slot in 0..GRID_C {
                        if self.grid[cell_idx + slot] == SLOT_EMPTY {
                            self.grid[cell_idx + slot] = index;
                            break;
                        }
                    }
                }
            }
        }

        println!("World objects: {}", self.brushes.len() - self.free_indices.len());

        index
    }

    /// Send player data to the GPU
    pub fn upload_player(&self, queue: &wgpu::Queue, gpu: &GPUWorld)
    {
        queue.write_buffer(&gpu.player_buffer, 0, bytemuck::bytes_of(&self.player));
    }

    /// Send world data to the GPU
    pub fn upload_world(&self, queue: &wgpu::Queue, gpu: &GPUWorld)
    {
        let start = Instant::now();
        if !self.brushes.is_empty() {
            queue.write_buffer(&gpu.brush_buffer, 0, bytemuck::cast_slice(&self.brushes));
        }
        queue.write_buffer(&gpu.grid_buffer, 0, bytemuck::cast_slice(self.grid.as_ref()));
        let elapsed = start.elapsed();
        println!("World upload time: {:.2}ms", elapsed.as_secs_f32() * 1000.0);
    }
}
