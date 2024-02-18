use bytemuck::{Pod, Zeroable};
use glam::Vec4;

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
    pub perceptual_roughness: f32,
    pub metallic: f32,
    pub reflectance: f32,
    pub pad: f32,
}

pub struct GPUMaterial {
    pub buffer: BufferView,
    pub offset: usize,
}
