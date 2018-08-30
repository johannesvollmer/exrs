//! describe a single block of pixels, which may be
//! a tile of a scan line block of float or unsigned integer data.

use ::file::meta::attributes::PixelType;
use ::smallvec::SmallVec;
use ::half::f16;


pub type PerChannel<T> = SmallVec<[T; 5]>;
// TODO pub type DataBlock = PerChannel<Array>; ? what about deep data?

#[derive(Clone)]
pub enum DataBlock {
    ScanLine(PerChannel<ScanLineBlock>),
    Tile(PerChannel<TileBlock>),

    DeepScanLine(PerChannel<DeepScanLineBlock>),
    DeepTile(PerChannel<DeepTileBlock>)
}

#[derive(Clone)]
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
}



#[derive(Clone)]
pub struct ScanLineBlock {
    pub data: Array,
}

#[derive(Clone)]
pub struct TileBlock {
    pub data: Array,
}

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
    U32(U32Data),

    /// The representation of 16-bit floating-point numbers is analogous to IEEE 754,
    /// but with 5 exponent bits and 10 bits for the fraction
    F16(F16Data),

    F32(F32Data),
}


#[derive(Clone)]
pub struct F16Data(Vec<u16>);
pub type U32Data = Vec<u32>;
pub type F32Data = Vec<f32>;


impl F16Data {
    pub fn from_bits(bits: Vec<u16>) -> Self {
        F16Data(bits)
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

    /*pub fn iter(&self) -> impl Iterator {
        self.0.iter().map(|&u| f16::from_bits(u))
    }*/

    pub fn into_bits(self) -> Vec<u16> {
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
