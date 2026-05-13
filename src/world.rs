#![allow(dead_code)]

use std::time::Instant;
use crate::gpu::GPUWorld;
use crate::math::*;

/// Brush kinds
pub const KIND_BOX: u32 = 0;
pub const KIND_CYLINDER: u32 = 1;
pub const KIND_SPHERE: u32 = 2;
pub const KIND_CONE: u32 = 3;
pub const NUM_BRUSH_KINDS: u32 = 4;

/// CSG operations
pub const OP_ADD: u32 = 0;
pub const OP_SUB: u32 = 1;

/// Grid cell slot empty
pub const SLOT_EMPTY: u16 = u16::MAX;

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

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WorldUniforms
{
    pub grid_min: Vec3,
    pub grid_size_x: u32,

    pub grid_size_y: u32,
    pub grid_size_z: u32,
    pub _pad: [u32; 2],
}

pub struct World
{
    /// List of brushes in the world
    /// Note that the order in which brushes are added matters,
    /// Because some brushes subtract from the world (OP_SUB)
    brushes: Vec<Brush>,
    free_indices: Vec<u16>,

    /// The world uses a metric coordinate system.
    /// The grid is a 3D array of cells such that each cell
    /// is 1x1x1 unit in size, with 1 unit = 1 meter.
    /// Each cell contains a u32 that stores an offset into the index buffer
    /// and a count of brushes in that cell.
    /// (offset << 8) | (count & 0xFF)
    pub grid: Vec<u32>,
    pub grid_indices: Vec<u16>,

    // Minimum XYZ position for the grid
    pub grid_min: Vec3,

    // Number of grid cells along each axis
    pub grid_size: [u32; 3],

    pub player: Player,
}

impl World
{
    pub fn new() -> Self
    {
        let mut world = Self {
            brushes: Vec::with_capacity(1024),
            free_indices: Vec::new(),
            grid: Vec::new(),
            grid_indices: Vec::new(),
            grid_min: Vec3::new(0.0, 0.0, 0.0),
            grid_size: [0, 0, 0],
            player: Player {
                position: Vec3::new(0.0, 1.8, 0.0),
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

    /// Get a copy of the brush with the given id
    pub fn get_brush(&mut self, index: u16) -> Brush
    {
        assert!((index as usize) < self.brushes.len());
        self.brushes[index as usize]
    }

    /// Remove a brush from the world
    pub fn remove_brush(&mut self, index: u16) -> Brush
    {
        assert!((index as usize) < self.brushes.len());

        let brush = self.brushes[index as usize];

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
        self.rebuild_grid();

        // Return the brush data
        brush
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

        self.rebuild_grid();

        println!("World objects: {}", self.brushes.len() - self.free_indices.len());

        index
    }

    pub fn rebuild_grid(&mut self)
    {
        let start = Instant::now();

        // 1. Find AABB of all active brushes
        let mut world_min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut world_max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);

        let mut active_indices = Vec::with_capacity(self.brushes.len());
        for i in 0..self.brushes.len() {
            if self.free_indices.contains(&(i as u16)) {
                continue;
            }
            let (b_min, b_max) = self.brushes[i].get_aabb();
            world_min = world_min.min(b_min);
            world_max = world_max.max(b_max);
            active_indices.push(i as u16);
        }

        if active_indices.is_empty() {
            self.grid.clear();
            self.grid_indices.clear();
            self.grid_size = [0, 0, 0];
            return;
        }

        // 2. Pad AABB and determine grid size
        world_min -= Vec3::new(1.0, 1.0, 1.0);
        world_max += Vec3::new(1.0, 1.0, 1.0);
        self.grid_min = Vec3::new(world_min.x.floor(), world_min.y.floor(), world_min.z.floor());
        let grid_max = Vec3::new(world_max.x.ceil(), world_max.y.ceil(), world_max.z.ceil());
        let diff = grid_max - self.grid_min;
        self.grid_size = [diff.x as u32, diff.y as u32, diff.z as u32];

        // Sanity check for grid size
        if self.grid_size[0] > 512 || self.grid_size[1] > 512 || self.grid_size[2] > 512 {
             println!("Warning: grid size too large: {:?}", self.grid_size);
        }

        let count = (self.grid_size[0] * self.grid_size[1] * self.grid_size[2]) as usize;
        self.grid.clear();
        self.grid.resize(count, 0);

        // 3. Count brushes per cell
        for &idx in &active_indices {
            let brush = &self.brushes[idx as usize];
            let (b_min, b_max) = brush.get_aabb();

            let x_min = ((b_min.x - self.grid_min.x).floor() as i32).max(0) as u32;
            let x_max = ((b_max.x - self.grid_min.x).ceil() as i32).max(0).min(self.grid_size[0] as i32 - 1) as u32;
            let y_min = ((b_min.y - self.grid_min.y).floor() as i32).max(0) as u32;
            let y_max = ((b_max.y - self.grid_min.y).ceil() as i32).max(0).min(self.grid_size[1] as i32 - 1) as u32;
            let z_min = ((b_min.z - self.grid_min.z).floor() as i32).max(0) as u32;
            let z_max = ((b_max.z - self.grid_min.z).ceil() as i32).max(0).min(self.grid_size[2] as i32 - 1) as u32;

            for y in y_min..=y_max {
                for z in z_min..=z_max {
                    for x in x_min..=x_max {
                        let c_idx = ((y * self.grid_size[2] + z) * self.grid_size[0] + x) as usize;
                        let count = self.grid[c_idx] & 0xFF;

                        // Only count OP_SUB if there is already something in the cell
                        if brush.op != OP_SUB || count > 0 {
                            if count < u8::MAX.into() {
                                self.grid[c_idx] += 1;
                            }
                        }
                    }
                }
            }
        }

        // 4. Prefix sum to find offsets
        let mut total_indices: usize = 0;
        for i in 0..count {
            let n = (self.grid[i] & 0xFF) as usize;
            let offset = total_indices;

            // Ensure the offset fits in 24 bits
            assert!(offset <= 0x00FFFFFF, "Grid index buffer offset overflow: {} exceeds 24 bits", offset);

            self.grid[i] = ((offset as u32) << 8) | (n as u32);
            total_indices += n;
        }

        self.grid_indices.clear();
        self.grid_indices.resize(total_indices, 0);
        let mut current_offset = vec![0u32; count];

        // 5. Fill index buffer
        for &idx in &active_indices {
            let brush = &self.brushes[idx as usize];
            let (b_min, b_max) = brush.get_aabb();

            let x_min = ((b_min.x - self.grid_min.x).floor() as i32).max(0) as u32;
            let x_max = ((b_max.x - self.grid_min.x).ceil() as i32).max(0).min(self.grid_size[0] as i32 - 1) as u32;
            let y_min = ((b_min.y - self.grid_min.y).floor() as i32).max(0) as u32;
            let y_max = ((b_max.y - self.grid_min.y).ceil() as i32).max(0).min(self.grid_size[1] as i32 - 1) as u32;
            let z_min = ((b_min.z - self.grid_min.z).floor() as i32).max(0) as u32;
            let z_max = ((b_max.z - self.grid_min.z).ceil() as i32).max(0).min(self.grid_size[2] as i32 - 1) as u32;

            for y in y_min..=y_max {
                for z in z_min..=z_max {
                    for x in x_min..=x_max {
                        let c_idx = ((y * self.grid_size[2] + z) * self.grid_size[0] + x) as usize;
                        let cell_info = self.grid[c_idx];
                        let offset = cell_info >> 8;
                        let max_n = cell_info & 0xFF;
                        let n = current_offset[c_idx];

                        // Only add OP_SUB if there's already something in the cell
                        if brush.op != OP_SUB || n > 0 {
                            if n < max_n {
                                self.grid_indices[offset as usize + n as usize] = idx;
                                current_offset[c_idx] += 1;
                            }
                        }
                    }
                }
            }
        }

        let elapsed = start.elapsed();
        if self.grid_indices.len() % 2 != 0 {
            self.grid_indices.push(0);
        }

        println!("Grid rebuild: {:.2}ms, {}x{}x{} ({} cells), {} indices",
            elapsed.as_secs_f32() * 1000.0,
            self.grid_size[0], self.grid_size[1], self.grid_size[2],
            count, total_indices
        );
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
        let mut total_bytes = 0;

        let uniforms = WorldUniforms {
            grid_min: self.grid_min,
            grid_size_x: self.grid_size[0],
            grid_size_y: self.grid_size[1],
            grid_size_z: self.grid_size[2],
            _pad: [0; 2],
        };
        queue.write_buffer(&gpu.world_uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        total_bytes += std::mem::size_of::<WorldUniforms>();

        if !self.brushes.is_empty() {
            let bytes = self.brushes.len() * std::mem::size_of::<Brush>();
            queue.write_buffer(&gpu.brush_buffer, 0, bytemuck::cast_slice(&self.brushes));
            total_bytes += bytes;
        }

        if !self.grid.is_empty() {
            let bytes = self.grid.len() * std::mem::size_of::<u32>();
            queue.write_buffer(&gpu.grid_buffer, 0, bytemuck::cast_slice(&self.grid));
            total_bytes += bytes;
        }

        if !self.grid_indices.is_empty() {
            let bytes = self.grid_indices.len() * std::mem::size_of::<u16>();
            queue.write_buffer(&gpu.index_buffer, 0, bytemuck::cast_slice(&self.grid_indices));
            total_bytes += bytes;
        }

        let elapsed = start.elapsed();
        println!("World upload: {:.2}MB, {:.2}ms",
            total_bytes as f32 / (1024.0 * 1024.0),
            elapsed.as_secs_f32() * 1000.0);
    }
}
