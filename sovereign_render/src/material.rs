use glam::{Vec2, Vec4};

use crate::id::{BufferId, SamplerId, ImageId};

#[derive(Debug)]
pub struct Material {
    pub uniform: MaterialUniform,
    pub buffer: BufferId,
    pub offset: usize,
    pub color_image: Option<ImageId>,
    pub color_sampler: Option<SamplerId>,
}

#[derive(Clone, Debug)]
pub struct MaterialUniform {
    pub base_color_factors: Vec4,
    pub metal_rough_factors: Vec2
}