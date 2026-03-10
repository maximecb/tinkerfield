#![allow(dead_code)]

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

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Brush
{
    pub kind: u32,
    pub material: u32,
    pub op: u32,
    pub _padding: u32,
    pub transform: [[f32; 4]; 4],
}

impl Brush
{
    pub fn get_aabb(&self) -> ([f32; 3], [f32; 3])
    {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];

        for i in 0..8 {
            let px_unit = if (i & 1) == 0 { -0.5 } else { 0.5 };
            let py_unit = if (i & 2) == 0 { -0.5 } else { 0.5 };
            let pz_unit = if (i & 4) == 0 { -0.5 } else { 0.5 };

            // Matrix-vector multiplication (column-major: m[col][row])
            let x = self.transform[0][0] * px_unit + self.transform[1][0] * py_unit + self.transform[2][0] * pz_unit + self.transform[3][0];
            let y = self.transform[0][1] * px_unit + self.transform[1][1] * py_unit + self.transform[2][1] * pz_unit + self.transform[3][1];
            let z = self.transform[0][2] * px_unit + self.transform[1][2] * py_unit + self.transform[2][2] * pz_unit + self.transform[3][2];
            let w = self.transform[0][3] * px_unit + self.transform[1][3] * py_unit + self.transform[2][3] * pz_unit + self.transform[3][3];

            let p = [x / w, y / w, z / w];

            for j in 0..3 {
                if p[j] < min[j] { min[j] = p[j]; }
                if p[j] > max[j] { max[j] = p[j]; }
            }
        }

        (min, max)
    }
}

#[repr(C)]
#[derive(Default, Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Player
{
    pub position: [f32; 3],
    pub _padding: f32,
}

/// Maximum number of brushes in our game world
pub const MAX_BRUSHES: usize = u16::MAX as usize;

/// 256 x 256 x 64 x (32 * 2) = 256MB
pub const GRID_W: usize = 256;
pub const GRID_D: usize = 256;
pub const GRID_H: usize = 64;
pub const GRID_C: usize = 32;
pub const GRID_COUNT: usize = GRID_W * GRID_D * GRID_H * GRID_C;

pub struct GPUWorld
{
    pub brush_buffer: wgpu::Buffer,
    pub grid_buffer: wgpu::Buffer,
    pub player_buffer: wgpu::Buffer,
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

        let grid_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Buffer"),
            size: (GRID_COUNT * std::mem::size_of::<u16>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let player_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Player Buffer"),
            size: std::mem::size_of::<Player>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("World Bind Group Layout"),
            entries: &[
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
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
                    resource: player_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            brush_buffer,
            grid_buffer,
            player_buffer,
            bind_group_layout,
            bind_group,
        }
    }
}

pub struct World
{
    brushes: Vec<Brush>,

    /// The grid is a 3D array of cells such that each cell
    /// is 1x1x1 unit (one meter) in size
    /// Each cell contains a list of up to 32 brush indices (u16)
    grid: Box<[u16]>,

    pub player: Player,

    pub gpu: GPUWorld,
}

impl World
{
    pub fn new(device: &wgpu::Device) -> Self
    {
        let mut world = Self {
            brushes: Vec::with_capacity(1024),
            grid: vec![SLOT_EMPTY; GRID_COUNT].into_boxed_slice(),
            player: Player {
                position: [128.0, 5.0, 128.0],
                _padding: 0.0,
            },
            gpu: GPUWorld::new(device),
        };

        // Add a default floor brush
        world.add_brush(Brush {
            kind: KIND_BOX,
            material: 0,
            op: OP_ADD,
            _padding: 0,
            transform: [
                [200.0, 0.0, 0.0, 0.0],
                [0.0, 0.2, 0.0, 0.0],
                [0.0, 0.0, 200.0, 0.0],
                [128.0, 0.0, 128.0, 1.0],
            ],
        });

        world
    }

    pub fn upload_player(&self, queue: &wgpu::Queue)
    {
        queue.write_buffer(&self.gpu.player_buffer, 0, bytemuck::bytes_of(&self.player));
    }

    pub fn upload_world(&self, queue: &wgpu::Queue)
    {
        if !self.brushes.is_empty() {
            queue.write_buffer(&self.gpu.brush_buffer, 0, bytemuck::cast_slice(&self.brushes));
        }
        queue.write_buffer(&self.gpu.grid_buffer, 0, bytemuck::cast_slice(self.grid.as_ref()));
    }

    /// Add a brush to the world grid
    pub fn add_brush(&mut self, brush: Brush)
    {
        let index = self.brushes.len() as u16;
        if index as usize >= MAX_BRUSHES {
            return;
        }
        self.brushes.push(brush);

        // Compute extents of this object in the grid
        let (min, max) = brush.get_aabb();
        let x_min = (min[0].floor() as i32).max(0).min(GRID_W as i32 - 1) as usize;
        let x_max = (max[0].ceil() as i32).max(0).min(GRID_W as i32 - 1) as usize;
        let y_min = (min[1].floor() as i32).max(0).min(GRID_H as i32 - 1) as usize;
        let y_max = (max[1].ceil() as i32).max(0).min(GRID_H as i32 - 1) as usize;
        let z_min = (min[2].floor() as i32).max(0).min(GRID_D as i32 - 1) as usize;
        let z_max = (max[2].ceil() as i32).max(0).min(GRID_D as i32 - 1) as usize;

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
    }
}
