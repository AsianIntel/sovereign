use glam::Mat4;

#[repr(C)]
pub struct ViewUniform {
    pub projection: Mat4,
    pub view: Mat4,
}