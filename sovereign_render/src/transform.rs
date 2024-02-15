use glam::Mat4;

use crate::BufferView;

pub struct Transform {
    pub transform: Mat4,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct GlobalTransform {
    pub transform: Mat4,
}

pub struct GPUTransform {
    pub buffer: BufferView,
    pub offset: usize,
}
