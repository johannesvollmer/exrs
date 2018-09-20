//! describe a single block of pixels, which may be
//! a tile of a scan line block of float or unsigned integer data.

use ::smallvec::SmallVec;
use ::half::f16;


pub type PerChannel<T> = SmallVec<[T; 5]>;
// TODO pub type DataBlock = PerChannel<Array>; ? what about deep data?

#[derive(Clone)]
pub enum DataBlock {
    ScanLine(PerChannel<Array>),
    Tile(PerChannel<Array>),

    DeepScanLine(PerChannel<DeepScanLineBlock>),
    DeepTile(PerChannel<DeepTileBlock>)
}

/*#[derive(Clone)]
pub struct BlockDescription {
    /// width x height, inferred from either TileDescription or scan line height.
    /// for scan line blocks, the resolution width is always the width of the data_window.
    /// for tile blocks, the resolution is the same regardless of mip map level.
    pub resolution: (i32, i32),
    pub kind: BlockKind,
    pub channels: PerChannel<ChannelDescription>,
}

#[derive(Clone, Copy)]
pub struct ChannelDescription {
    /// (x,y)
    pub sampling: (i32, i32),
    pub pixel_type: PixelType,
}

#[derive(Clone, Copy)]
pub enum BlockKind {
    ScanLine, Tile, DeepScanLine, DeepTile
}*/



/*#[derive(Clone)]
pub struct ScanLineBlock {
    pub data: Array,
}

#[derive(Clone)]
pub struct TileBlock {
    pub data: Array,
}*/

#[derive(Clone)]
pub struct DeepScanLineBlock {
    // TODO
}

#[derive(Clone)]
pub struct DeepTileBlock {
    // TODO
}

// TODO reduce vec indirection
// per channel!

#[derive(Clone)]
pub enum Array {
    U32(Vec<u32>),

    /// The representation of 16-bit floating-point numbers is analogous to IEEE 754,
    /// but with 5 exponent bits and 10 bits for the fraction
    F16(Vec<f16>),

    F32(Vec<f32>),
}

impl Array {
    /// panic on type mismatch
    pub fn extend_from_slice(&mut self, other: &Self) {
        assert!(self.try_extend_from_slice(other), "Array::extend_from_slice type mismatch")
    }

    pub fn try_extend_from_slice(&mut self, other: &Self) -> bool {
        match self {
            Array::U32(values) => {
                if let Array::U32(ref other) = other {
                    values.extend_from_slice(&other);
                    true
                } else {
                    false
                }
            },

            Array::F16(values) => {
                if let Array::F16(ref other) = other {
                    values.extend_from_slice(&other);
                    true
                } else {
                    false
                }
            },

            Array::F32(values) => {
                if let Array::F32(ref other) = other {
                    values.extend_from_slice(&other);
                    true
                } else {
                    false
                }
            }
        }
    }
}
