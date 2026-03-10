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

pub struct World
{
    brushes: Vec<Brush>,

    /// The grid is a 3D array of cells such that each cell
    /// is 1x1x1 unit (one meter) in size
    /// Each cell contains a list of up to 32 brush indices (u16)
    grid: Box<[u16; GRID_COUNT]>,

    player: Player,
}

impl World
{
    pub fn new() -> Self
    {
        Self {
            brushes: vec![Brush {
                kind: KIND_BOX,
                material: 0,
                op: OP_ADD,
                _padding: 0,
                transform: [
                    [40.0, 0.0, 0.0, 0.0],
                    [0.0, 0.2, 0.0, 0.0],
                    [0.0, 0.0, 40.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
            }; MAX_BRUSHES],

            grid: Box::new([0; GRID_COUNT]),

            player: Player::default(),
        }
    }
}
