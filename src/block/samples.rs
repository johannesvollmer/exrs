//! Extract pixel samples from a block of pixel bytes.

use crate::prelude::*;


/// A single red, green, blue, or alpha value.
#[derive(Copy, Clone, Debug)]
pub enum Sample {

    /// A 16-bit float sample.
    F16(f16),

    /// A 32-bit float sample.
    F32(f32),

    /// An unsigned integer sample.
    U32(u32)
}

impl Sample {

    /// Create a sample containing a 32-bit float.
    pub fn f32(f32: f32) -> Self { Sample::F32(f32) }

    /// Create a sample containing a 16-bit float.
    pub fn f16(f16: f16) -> Self { Sample::F16(f16) }

    /// Create a sample containing a 32-bit integer.
    pub fn u32(u32: u32) -> Self { Sample::U32(u32) }

    /// Convert the sample to an f16 value. This has lower precision than f32.
    /// Note: An f32 can only represent integers up to `1024` as precise as a u32 could.
    #[inline]
    pub fn to_f16(self) -> f16 {
        match self {
            Sample::F16(sample) => sample,
            Sample::F32(sample) => f16::from_f32(sample),
            Sample::U32(sample) => f16::from_f32(sample as f32),
        }
    }

    /// Convert the sample to an f32 value.
    /// Note: An f32 can only represent integers up to `8388608` as precise as a u32 could.
    #[inline]
    pub fn to_f32(self) -> f32 {
        match self {
            Sample::F32(sample) => sample,
            Sample::F16(sample) => sample.to_f32(),
            Sample::U32(sample) => sample as f32,
        }
    }

    /// Convert the sample to a u32. Rounds floats to integers the same way that `3.1 as u32` does.
    #[inline]
    pub fn to_u32(self) -> u32 {
        match self {
            Sample::F16(sample) => sample.to_f32() as u32,
            Sample::F32(sample) => sample as u32,
            Sample::U32(sample) => sample,
        }
    }

    /// Is this value not a number?
    #[inline]
    pub fn is_nan(self) -> bool {
        match self {
            Sample::F16(value) => value.is_nan(),
            Sample::F32(value) => value.is_nan(),
            Sample::U32(_) => false,
        }
    }

    /// Is this value zero or negative zero?
    #[inline]
    pub fn is_zero(&self) -> bool {
        match *self {
            Sample::F16(value) => value == f16::ZERO || value == f16::NEG_ZERO,
            Sample::F32(value) => value == 0.0,
            Sample::U32(value) => value == 0,
        }
    }
}

impl PartialEq for Sample {
    fn eq(&self, other: &Self) -> bool {
        match *self {
            Sample::F16(num) => num == other.to_f16(),
            Sample::F32(num) => num == other.to_f32(),
            Sample::U32(num) => num == other.to_u32(),
        }
    }
}

// this is not recommended because it may hide whether a color is transparent or opaque and might be undesired for depth channels
impl Default for Sample {
    fn default() -> Self { Sample::F32(0.0) }
}

impl From<f16> for Sample { #[inline] fn from(f: f16) -> Self { Sample::F16(f) } }
impl From<f32> for Sample { #[inline] fn from(f: f32) -> Self { Sample::F32(f) } }
impl From<u32> for Sample { #[inline] fn from(f: u32) -> Self { Sample::U32(f) } }

impl<T> From<Option<T>> for Sample where T: Into<Sample> + Default {
    #[inline] fn from(num: Option<T>) -> Self { num.unwrap_or_default().into() }
}


impl From<Sample> for f16 { #[inline] fn from(s: Sample) -> Self { s.to_f16() } }
impl From<Sample> for f32 { #[inline] fn from(s: Sample) -> Self { s.to_f32() } }
impl From<Sample> for u32 { #[inline] fn from(s: Sample) -> Self { s.to_u32() } }

