
//! Read and write simple aspects of an exr image, excluding deep data and multiresolution levels.
//! Use `exr::image::full` instead, if you need deep data or resolution levels.

use smallvec::SmallVec;
use half::f16;
use crate::io::*;
use crate::meta::*;
use crate::meta::attributes::*;
use crate::error::{Result, PassiveResult, Error};
use crate::math::*;
use std::io::{Seek, BufReader, BufWriter};
use crate::io::Data;
use crate::image::{Line, LineIndex};



#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub parts: SmallVec<[Part; 6]>,
    pub display_window: Box2I32,
    pub pixel_aspect: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub channels: Vec<Channel>,
    pub line_order: LineOrder,
    pub compression: Compression,
    pub blocks: Blocks,
    pub data_window: Box2I32,
    pub attributes: Attributes,
    pub name: Option<Text>,

    pub screen_window_center: Vec2<f32>, // TODO use sensible defaults instead of returning an error on missing?
    pub screen_window_width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub name: Text,
    pub pixels: Pixels,
    pub is_linear: bool,
    pub sampling: Vec2<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pixels {
    F16(Vec<f16>),
    F32(Vec<f32>),
    U32(Vec<u32>),
}

