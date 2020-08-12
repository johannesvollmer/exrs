use crate::meta::header::{ImageAttributes, LayerAttributes};
use crate::meta::attribute::Text;
use half::f16;

//! Data structures that contain the image.

pub mod read;
// pub mod write;

/// `L`: either `RgbaChannels` or `AnyChannels<AnySamples>` or `AnyChannels<FlatSamples>` or a `Vec` of any of those
#[derive(Debug, Clone, PartialEq)]
pub struct Image<L> {
    image_attributes: ImageAttributes,

    /// `L`: either `RgbaChannels` or `AnyChannels<AnySamples>` or `AnyChannels<FlatSamples>` or a `Vec` of any of those
    layers: L, // TODO SmallVec?
}

/// `C`: either `RgbaChannels` or `AnyChannels<AnySamples>` or `AnyChannels<FlatSamples>`
#[derive(Debug, Clone, PartialEq)]
pub struct Layer<C> {
    layer_attributes: LayerAttributes,
    channels: C,
}

/// `S`: Anything, from `Vec<f16>` to `Vec<Vec<AnySample>>`, as desired by the user
#[derive(Debug, Clone, PartialEq)]
pub struct RgbaChannels<S> {
    has_alpha: bool,

    /// Anything, from `Vec<f16>` to `Vec<Vec<AnySample>>`, as desired by the user
    samples: S
}

/// `S`: Either `AnySamples` or `FlatSamples`
#[derive(Debug, Clone, PartialEq)]
pub type AnyChannels<S> = Vec<AnyChannel<S>>; // TODO SmallVec?

/// `S`: Either `AnySamples` or `FlatSamples`
#[derive(Debug, Clone, PartialEq)]
pub struct AnyChannel<S> {
    name: Text,

    /// Either `AnySamples` or `FlatSamples`
    samples: S
}

#[derive(Clone, PartialEq)]
pub enum FlatSamples {
    F16(Vec<f16>),
    F32(Vec<f32>),
    U32(Vec<u32>),
}

#[derive(Clone, PartialEq)]
pub enum AnySamples {
    Deep(DeepSamples),
    Flat(FlatSamples)
}

#[derive(Clone, PartialEq)]
pub enum DeepSamples {
    F16(Vec<Vec<f16>>),
    F32(Vec<Vec<f32>>),
    U32(Vec<Vec<u32>>),
}


