//! The `image` module is for interpreting the loaded file data.
//!

use ::half::f16;
use ::file::meta::{OffsetTable, Header};
use ::std::io::{Read, Seek, SeekFrom};
use ::smallvec::SmallVec;
use ::file::attributes::{Attribute, Text, I32Box2, Compression, PixelType};
use ::file::chunks::TileCoordinates;


/// any openexr image, loads all available data immediately into memory
pub struct FullImage {
    pub parts: Parts, // TODO HashMap<Text, Part> ?
}

pub type Parts = SmallVec<[Part; 3]>;
pub type Channels = SmallVec<[Channel; 5]>;
pub type PixelDataPerChannel = SmallVec<[PixelData; 5]>;

pub struct Part {
    // ### required attributes: ###
    pub channels: Channels,
    pub name: Option<Text>,
    pub compression: Compression,
    pub data_window: I32Box2,
    pub display_window: I32Box2,
    pub pixel_aspect: f64,
    pub screen_window_center: (f64, f64),
    pub screen_window_width: f64,
    // line order already consumed to sort self.data
    //

    /// currently contains only custom attributes __with standard type__
    pub custom_attributes: Vec<Attribute>,

    /// only the data for this single part,
    /// index can be computed from pixel location and block_kind.
    /// one part can only have one block_kind, not a different kind per block
    pub data_sections: DataSections
}

/// one `type` per Part
pub enum DataSections {
    ScanLine(Vec<ScanLineBlock>),
    Tile(Vec<TileBlock>),

    DeepScanLine(Vec<DeepScanLineBlock>),
    DeepTile(Vec<DeepTileBlock>),
}

pub struct ScanLineBlock {
    // y_coordinate can be inferred by index in PixelData vector
    /// same length as `Part.channels` field
    pub per_channel_data: PixelDataPerChannel,
}

pub struct TileBlock {
    // tile_coordinates can be inferred by index in PixelData vector
    /// same length as `Part.channels` field
    pub per_channel_data: PixelDataPerChannel,
}

pub struct DeepScanLineBlock {
    // y_coordinate can be inferred by index in PixelData vector
    /// same length as `Part.channels` field
    pub per_channel_data: PixelDataPerChannel,
}

pub struct DeepTileBlock {
    // tile_coordinates can be inferred by index in PixelData vector
    /// same length as `Part.channels` field
    pub per_channel_data: PixelDataPerChannel,
}

// TODO reduce vec indirection
// per channel!
pub enum PixelData {
    U32(Box<[u32]>),

    /// The representation of 16-bit floating-point numbers is analogous to IEEE 754,
    /// but with 5 exponent bits and 10 bits for the fraction
    F16(Box<[f16]>),

    F32(Box<[f32]>),
}




pub struct Channel {
    pub name: Text,
    pub is_linear: bool,
    pub x_sampling: usize,
    pub y_sampling: usize,
    pub pixel_type: PixelType,
}

pub enum BlockKind {
    ScanLine, Tile, DeepScanLine, DeepTile
}






/* TODO loads from file where required
pub struct LateData<P, R: Read + Seek> {
    offset_table: OffsetTable,
    pixel_cache: Vec<P>,
    stream: R,
}

/// immediately loaded
pub struct FullData<P> {
    pixels: Vec<P>, // TODO per channel
}
impl<R: Read + Seek> Image<R> {
    pub fn load(mut source: R) -> ::file::ReadResult<Self> {
        let meta_data = ::file::io::read_meta_data(source)?;
        Ok(Image {
            source,
            meta_data,
            data: Vec::new(),
        })
    }

    pub fn load_chunk(&mut self, part: usize, tile_index: usize) {
        let offset_table = &self.meta_data.offset_tables[part];

        // go to start of chunk
        self.source.seek(SeekFrom::Start(offset_table[tile_index]))
            .unwrap();

        if self.meta_data.version.has_multiple_parts {
            let chunk = ::file::chunks::MultiPartChunk::read(&mut source, &self.meta_data)
                .unwrap();

            cache[&(part, tile_index)] = unpack_pixel_data(chunk)

        } else {
            let chunk: SinglePartChunk = ::file::chunks::SinglePartChunks::read_chunk(&mut source, &self.meta_data)
                .unwrap();

            cache[&(part, tile_index)] = unpack_pixel_data(chunk)
        }
    }

    pub fn cache_chunk_for_pixel(&mut self, part: usize, pixel: (usize, usize)) {
        let dimensions =
        self.load_chunk(part, pixel.1 % self.meta_data.headers[part].data_window().width + pixel.0)
    }
}
*/
