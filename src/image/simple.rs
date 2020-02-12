
// TODO documentation

use crate::meta::attributes::{Text, Box2I32};
//use crate::math::Vec2;
use crate::prelude::f16;
use smallvec::SmallVec;


#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    parts: SmallVec<[Part; 6]>,
    display_window: Box2I32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    channels: Vec<Channel>,
    data_window: Box2I32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    name: Text,
    pixels: Pixels,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pixels {
    F16(Vec<f16>),
    F32(Vec<f32>),
    U32(Vec<u32>),
}

