use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec4};

use crate::{
    id::{ImageId, SamplerId},
    BufferView,
};

#[derive(Debug)]
pub struct Material {
    pub uniform: MaterialUniform,
    pub color_image: Option<ImageId>,
    pub color_sampler: Option<SamplerId>,
}

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct MaterialUniform {
    pub base_color_factors: Vec4,
    pub metal_rough_factors: Vec2,
    pub pad: Vec2,
}

pub struct GPUMaterial {
    pub buffer: BufferView,
    pub offset: usize,
}
