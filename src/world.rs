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

/// Octree constants
pub const OCTREE_MAX_DEPTH: u32 = 8;
pub const OCTREE_SPLIT_THRESHOLD: usize = 4;
pub const OCTREE_MIN_SIZE: f32 = 32.0;

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OctreeNode
{
    /// If > 0, index of the first of 8 children in the nodes buffer.
    /// If 0, this is a leaf node.
    pub child_base_idx: u32,
    /// For leaves: number of brushes in this node.
    pub brush_count: u32,
    /// For leaves: offset into the global brush indices buffer.
    pub brush_offset: u32,
    pub _pad: u32,
}

pub struct World
{
    brushes: Vec<Brush>,
    free_indices: Vec<u16>,

    pub octree_nodes: Vec<OctreeNode>,
    pub octree_indices: Vec<u16>,
    pub octree_root_min: Vec3,
    pub octree_root_size: f32,

    pub player: Player,
}

impl World
{
    pub fn new() -> Self
    {
        let mut world = Self {
            brushes: Vec::with_capacity(1024),
            free_indices: Vec::new(),
            octree_nodes: Vec::new(),
            octree_indices: Vec::new(),
            octree_root_min: Vec3::default(),
            octree_root_size: OCTREE_MIN_SIZE,
            player: Player {
                position: Vec3::new(128.0, 17.8, 128.0),
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
            pos: Vec3::new(128.0, 8.0, 128.0),
            kind: KIND_BOX,
            scale: Vec3::new(40.0, 16.0, 40.0),
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

    /// Move the player
    pub fn move_player(&mut self, fwd_dist: f32, side_dist: f32)
    {
        self.player.position += self.player.forward * fwd_dist;
        self.player.position += self.player.right * side_dist;
    }

    pub fn rebuild_octree(&mut self)
    {
        let start = Instant::now();
        self.octree_nodes.clear();
        self.octree_indices.clear();

        // Determine world bounds and active AABBs
        let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut active_indices = Vec::new();
        let mut active_aabbs = Vec::new();

        for (i, brush) in self.brushes.iter().enumerate() {
            if brush.scale.x > 0.0 || brush.scale.y > 0.0 || brush.scale.z > 0.0 {
                let (b_min, b_max) = brush.get_aabb();
                min.x = min.x.min(b_min.x);
                min.y = min.y.min(b_min.y);
                min.z = min.z.min(b_min.z);
                max.x = max.x.max(b_max.x);
                max.y = max.y.max(b_max.y);
                max.z = max.z.max(b_max.z);
                active_indices.push(i as u16);
                active_aabbs.push((i as u16, b_min, b_max));
            }
        }

        if active_indices.is_empty() {
            self.octree_root_min = Vec3::default();
            self.octree_root_size = OCTREE_MIN_SIZE;
            self.octree_nodes.push(OctreeNode::default());
            return;
        }

        let size = (max.x - min.x).max(max.y - min.y).max(max.z - min.z).max(OCTREE_MIN_SIZE);
        let center = (min + max) * 0.5;
        self.octree_root_min = center - Vec3::new(size * 0.5, size * 0.5, size * 0.5);
        self.octree_root_size = size;

        // Reserve root node
        self.octree_nodes.push(OctreeNode::default());
        self.split_node(0, self.octree_root_min, self.octree_root_size, active_aabbs, 0);

        let elapsed = start.elapsed();
        println!("Octree rebuild: {:.2}ms, nodes: {}, indices: {}",
            elapsed.as_secs_f32() * 1000.0,
            self.octree_nodes.len(),
            self.octree_indices.len()
        );
    }

    fn split_node(&mut self, node_idx: usize, min: Vec3, size: f32, brush_data: Vec<(u16, Vec3, Vec3)>, depth: u32)
    {
        if brush_data.len() <= OCTREE_SPLIT_THRESHOLD || depth >= OCTREE_MAX_DEPTH {
            self.octree_nodes[node_idx].brush_count = brush_data.len() as u32;
            self.octree_nodes[node_idx].brush_offset = self.octree_indices.len() as u32;
            for (idx, _, _) in brush_data {
                self.octree_indices.push(idx);
            }
            return;
        }

        let child_base_idx = self.octree_nodes.len();
        self.octree_nodes[node_idx].child_base_idx = child_base_idx as u32;
        for _ in 0..8 {
            self.octree_nodes.push(OctreeNode::default());
        }

        let child_size = size * 0.5;
        for i in 0..8 {
            let offset = Vec3::new(
                if (i & 1) != 0 { child_size } else { 0.0 },
                if (i & 2) != 0 { child_size } else { 0.0 },
                if (i & 4) != 0 { child_size } else { 0.0 },
            );
            let child_min = min + offset;
            let child_max = child_min + Vec3::new(child_size, child_size, child_size);

            let mut child_brushes = Vec::new();
            let mut has_add = false;
            for &(idx, b_min, b_max) in &brush_data {
                if b_min.x <= child_max.x && b_max.x >= child_min.x &&
                   b_min.y <= child_max.y && b_max.y >= child_min.y &&
                   b_min.z <= child_max.z && b_max.z >= child_min.z
                {
                    let op = self.brushes[idx as usize].op;
                    if op == OP_ADD {
                        child_brushes.push((idx, b_min, b_max));
                        has_add = true;
                    } else if op == OP_SUB && has_add {
                        // Only add subtraction if we already have an addition brush in this node
                        child_brushes.push((idx, b_min, b_max));
                    }
                }
            }

            // Efficiency check: if this child still contains ALL the brushes of the parent,
            // further splitting this specific child is useless.
            if child_brushes.len() == brush_data.len() {
                let c_idx = child_base_idx + i;
                self.octree_nodes[c_idx].brush_count = child_brushes.len() as u32;
                self.octree_nodes[c_idx].brush_offset = self.octree_indices.len() as u32;
                for (idx, _, _) in child_brushes {
                    self.octree_indices.push(idx);
                }
            } else {
                self.split_node(child_base_idx + i, child_min, child_size, child_brushes, depth + 1);
            }
        }
    }

    /// Remove a brush from the world
    pub fn remove_brush(&mut self, index: u16)
    {
        if (index as usize) >= self.brushes.len() {
            return;
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
        self.rebuild_octree();
    }

    /// Add a brush to the world
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

        println!("World objects: {}", self.brushes.len() - self.free_indices.len());
        self.rebuild_octree();
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

        let mut root_info = [0.0f32; 4];
        root_info[0] = self.octree_root_min.x;
        root_info[1] = self.octree_root_min.y;
        root_info[2] = self.octree_root_min.z;
        root_info[3] = self.octree_root_size;
        queue.write_buffer(&gpu.octree_root_buffer, 0, bytemuck::cast_slice(&root_info));

        queue.write_buffer(&gpu.octree_nodes_buffer, 0, bytemuck::cast_slice(&self.octree_nodes));

        let mut indices_bytes = bytemuck::cast_slice::<u16, u8>(&self.octree_indices).to_vec();
        if indices_bytes.len() % 4 != 0 {
            indices_bytes.push(0);
            indices_bytes.push(0);
        }
        queue.write_buffer(&gpu.octree_indices_buffer, 0, &indices_bytes);

        let elapsed = start.elapsed();
        println!("World upload time: {:.2}ms", elapsed.as_secs_f32() * 1000.0);
    }
}
