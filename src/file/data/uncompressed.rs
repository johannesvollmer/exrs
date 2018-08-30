//! describe a single block of pixels, which may be
//! a tile of a scan line block of float or unsigned integer data.

use ::file::meta::attributes::PixelType;
use ::smallvec::SmallVec;
use ::half::f16;


pub type PerChannel<T> = SmallVec<[T; 5]>;
// TODO pub type DataBlock = PerChannel<Array>; ? what about deep data?

pub enum DataBlock {
    ScanLine(PerChannel<ScanLineBlock>),
    Tile(PerChannel<TileBlock>),

    DeepScanLine(PerChannel<DeepScanLineBlock>),
    DeepTile(PerChannel<DeepTileBlock>)
}


pub struct BlockDescription {
    /// width x height, inferred from either TileDescription or scan line height.
    /// for scan line blocks, the resolution width is always the width of the data_window.
    /// for tile blocks, the resolution is the same regardless of mip map level.
    pub resolution: (u32, u32),
    pub kind: BlockKind,
    pub channels: PerChannel<ChannelDescription>,
}

pub struct ChannelDescription {
    /// (x,y)
    pub sampling: (u32, u32),
    pub pixel_type: PixelType,
}

pub enum BlockKind {
    ScanLine, Tile, DeepScanLine, DeepTile
}


pub struct ScanLineBlock {
    pub data: Array,
}

pub struct TileBlock {
    pub data: Array,
}

pub struct DeepScanLineBlock {
    // TODO
}

pub struct DeepTileBlock {
    // TODO
}

// TODO reduce vec indirection
// per channel!
pub enum Array {
    U32(U32Array),

    /// The representation of 16-bit floating-point numbers is analogous to IEEE 754,
    /// but with 5 exponent bits and 10 bits for the fraction
    F16(F16Array),

    F32(F32Array),
}

pub type U32Array = Box<[u32]>;
pub type F32Array = Box<[f32]>;
pub struct F16Array(Box<[u16]>);

impl F16Array {
    pub fn from_bits(bits: Box<[u16]>) -> Self {
        F16Array(bits)
    }

    pub fn get(&self, index: usize) -> f16 {
        f16::from_bits(self.0[index])
    }

    pub fn set(&self, index: usize, value: f16){
        self.0[index] = value.as_bits();
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator {
        self.0.iter().map(|&u| f16::from_bits(u))
    }

    pub fn into_bits(self) -> Box<[u16]> {
        self.0
    }

    pub fn as_bits(&self) -> &[u16] {
        &self.0
    }
}

/*impl ::std::ops::Index<usize> for F16Array {
    type Output = f16;
    fn index(&self, index: usize) -> &f16 {

    }
}
impl ::std::ops::IndexMut<usize> for F16Array {
    fn index_mut(&self, index: usize) -> &mut f16 {

    }
}*/
