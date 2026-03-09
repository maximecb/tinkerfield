#![allow(dead_code)]

pub const KIND_NONE: u32 = 0;
pub const KIND_BOX: u32 = 1;
pub const KIND_CYLINDER: u32 = 2;
pub const KIND_SPHERE: u32 = 3;

pub const OP_SUB: u32 = 0;
pub const OP_ADD: u32 = 1;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Object
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

pub const MAX_OBJECTS: usize = u16::MAX as usize;

// 256 x 256 x 64 x (2 * 32) = 256MB
pub const GRID_W: usize = 256;
pub const GRID_D: usize = 256;
pub const GRID_H: usize = 64;
pub const GRID_E: usize = 32;
pub const GRID_COUNT: usize = GRID_W * GRID_D * GRID_H * GRID_E;

pub struct World
{
    objects: Vec<Object>,
    grid: Box<[u16; GRID_COUNT]>,
    player: Player,
}

impl World
{
    pub fn new() -> Self
    {
        Self {
            objects: vec![Object {
                kind: KIND_BOX,
                material: 0,
                op: OP_ADD,
                _padding: 0,
                transform: [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
            }; MAX_OBJECTS],

            grid: Box::new([0; GRID_COUNT]),

            player: Player::default(),
        }
    }
}
