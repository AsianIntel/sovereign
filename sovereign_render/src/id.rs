#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SamplerId(pub usize);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ImageId(pub usize);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BufferId(pub usize);