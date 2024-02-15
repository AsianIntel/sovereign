use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec4};

use crate::{id::BufferId, BufferView};

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct Vertex {
    pub position: Vec4,
    pub normal: Vec4,
    pub color: Vec4,
    pub uv: Vec2,
    pub pad: Vec2,
}

#[derive(Debug)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug)]
pub struct GPUMesh {
    pub vertex_buffer: BufferView,
    pub index_buffer: BufferId,
    pub index_count: usize,
}
