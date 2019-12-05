//! The `image` module is for interpreting the loaded file data.
//!

pub use ::seek_bufread::BufReader as SeekBufRead;

use smallvec::SmallVec;
use crate::file::meta::{Header, MetaData, compute_level_count, compute_level_size};
use crate::file::data::uncompressed::{PerChannel, Array, DeepScanLineBlock};
use crate::file::data::compressed::Chunks;
use crate::file::io::*;
use crate::file::meta::attributes::{PixelType, LevelMode, Kind};


pub mod meta {
    use super::SeekBufRead;
    use crate::file::meta::MetaData;
    use std::io::{Read, Seek};
    use crate::file::io::ReadResult;

    #[must_use]
    pub fn read_from_file(path: &::std::path::Path) -> ReadResult<MetaData> {
        read_unbuffered(::std::fs::File::open(path)?)
    }

    /// assumes that the provided reader is not buffered, and will create a buffer for it
    #[must_use]
    pub fn read_unbuffered<R: Read + Seek>(unbuffered: R) -> ReadResult<MetaData> {
        read_seekable_prebuffered(&mut SeekBufRead::new(unbuffered))
    }

    #[must_use]
    pub fn read_seekable_prebuffered<R: Read + Seek>(buffered: &mut R) -> ReadResult<MetaData> {
        MetaData::read_validated(buffered)
    }
}


#[must_use]
pub fn read_from_file(path: &::std::path::Path) -> ReadResult<Image> {
    read_unbuffered(::std::fs::File::open(path)?)
}

/// assumes that the provided reader is not buffered, and will create a buffer for it
#[must_use]
pub fn read_unbuffered<R: Read + Seek>(unbuffered: R) -> ReadResult<Image> {
    read_seekable_prebuffered(&mut SeekBufRead::new(unbuffered))
}

#[must_use]
pub fn read_seekable_prebuffered<R: Read + Seek>(buffered_read: &mut R) -> ReadResult<Image> {
    let MetaData { headers, offset_tables, .. } = MetaData::read_validated(buffered_read)?;
    let chunks = Chunks::read(buffered_read, &headers, offset_tables)?;

    let mut image = Image {
        parts: headers.into_iter().map(Part::new).collect(),
    };

    match chunks {
        Chunks::SinglePart(part_contents) => {
            assert_eq!(image.parts.len(), 1, "single part chunk with multiple headers");
            let part = &mut image.parts[0];

//            let data_size = part.header.data_window.dimensions();
            let compression = part.header.compression;

            use crate::file::data::compressed::SinglePartChunks::*;

            match part_contents {
                ScanLine(scan_line_contents) => {
                    for data_block in scan_line_contents {
                        let y = data_block.y_coordinate - part.header.data_window.y_min;
                        debug_assert!(y >= 0); // TODO Err() instead

                        let y = y as u32;
                        let block_size = part.header.get_scan_line_block_size(y);

                        let expected_byte_size = block_size.0 * block_size.1 * part.header.channels.bytes_per_pixel;
                        let decompressed_bytes = compression.decompress_bytes(data_block.compressed_pixels, expected_byte_size as usize)?;
                        part.read_block(&mut decompressed_bytes.as_slice(), (0,0), (0, y), block_size)?;
                    }
                },

                Tile(tiles) => {
                    let tile_description = part.header.tiles
                        .expect("Check failed: `tiles` missing");

                    for tile in tiles {
                        let tile_size = part.header.get_tile_size(tile_description, tile.coordinates);
                        let levels = (tile.coordinates.level_x as u32, tile.coordinates.level_y as u32);
                        let x = tile.coordinates.tile_x - part.header.data_window.x_min;
                        let y = tile.coordinates.tile_y - part.header.data_window.y_min;
                        debug_assert!(x >= 0 && y >= 0); // TODO Err() instead

                        let expected_byte_size = tile_size.0 * tile_size.1 * part.header.channels.bytes_per_pixel;
                        let decompressed_bytes = compression.decompress_bytes(tile.compressed_pixels, expected_byte_size as usize)?;
                        part.read_block(&mut decompressed_bytes.as_slice(), levels, (x as u32, y as u32), tile_size)?;
                    }
                },

                // let map_level_x = unimplemented!("are mip map levels only for tiles?");
                // let map_level_y = unimplemented!();

                _ => eprintln!("deep data accumulation not supported")
            }
        },

        Chunks::MultiPart(_parts) => eprintln!("multipart accumulation not supported")
    }

    Ok(image)
}



#[derive(Clone, PartialEq, Debug)]
pub struct Image {
    pub parts: Parts
}

/// an exr image can store multiple parts (multiple bitmaps inside one image)
pub type Parts = SmallVec<[Part; 2]>;

#[derive(Clone, PartialEq, Debug)]
pub struct Part {
    pub header: Header,

    /// only the data for this single part,
    /// index can be computed from pixel location and block_kind.
    /// one part can only have one block_kind, not a different kind per block
    /// number of x and y levels can be computed using the header
    ///
    /// That Vec contains one entry per mip map level, or only one if it does not have any,
    /// or a row-major flattened vector of all rip maps like
    /// 1x1, 2x1, 4x1, 8x1, and then
    /// 1x2, 2x2, 4x2, 8x2, and then
    /// 1x4, 2x4, 4x4, 8x4, and then
    /// 1x8, 2x8, 4x8, 8x8.
    ///
    // FIXME should be descending and starting with full-res instead!
    pub levels: Levels

    // offset tables are already processed while loading 'data'
    // TODO skip reading offset tables if not required?
}

#[derive(Clone, PartialEq, Debug)]
pub enum Levels {
    Singular(PartData),
    Mip(Maps),
    Rip(RipMaps),
}

pub type Maps = SmallVec<[PartData; 16]>;

#[derive(Clone, PartialEq, Debug)]
pub struct RipMaps {
    pub maps: Maps,
    pub level_count: (u32, u32),
}

/// one `type` per Part
#[derive(Clone, PartialEq, Debug)]
pub enum PartData {
    /// One single array containing all pixels, row major left to right, top to bottom
    /// same length as `Part.channels` field
    // TODO should store sampling_x/_y for simple accessors?
    Flat(Pixels<Array>),

    /// scan line blocks are stored from top to bottom, row major.
    Deep/*ScanLine*/(Pixels<Vec<DeepScanLineBlock>>),

    // /// Blocks are stored from top left to bottom right, row major.
    // DeepTile(PerChannel<Vec<DeepTileBlock>>),
}

#[derive(Clone, PartialEq, Debug)]
pub struct Pixels<T> {
    pub dimensions: (u32, u32),
    pub channels: PerChannel<T>
}


impl Levels {
    pub fn largest(&self) -> &PartData {
        match *self {
            Levels::Singular(ref data) => data,
            Levels::Mip(ref maps) => &maps[0], // TODO is this really the largest one?
            Levels::Rip(ref rip_map) => &rip_map.maps[0], // TODO test!
        }
    }
}


impl Part {

    /// allocates all the memory necessary to hold the pixel data,
    /// zeroed out, ready to be filled with actual pixel data
    pub fn new(header: Header) -> Self {
        match &header.kind {
            None | &Some(Kind::ScanLine) | &Some(Kind::Tile) => {
                let levels = {
                    let data_size = header.data_window.dimensions();

                    let part_data = |dimensions: (u32, u32)| {
                        let data = header.channels.list.iter()
                            .map(|channel| { match channel.pixel_type {
                                PixelType::F16 => Array::F16(vec![half::f16::ZERO; channel.subsampled_pixels(dimensions) as usize]),
                                PixelType::F32 => Array::F32(vec![0.0; channel.subsampled_pixels(dimensions) as usize]),
                                PixelType::U32 => Array::U32(vec![0; channel.subsampled_pixels(dimensions) as usize]),
                            }})
                            .collect();

                        PartData::Flat(Pixels { dimensions, channels: data })
                    };

                    if let Some(tiles) = &header.tiles {
                        debug_assert_eq!(header.kind, Some(Kind::Tile));

                        let round = tiles.rounding_mode;
//                        let tile_size = tiles.dimensions();

                        let level_count = |full_res: u32| {
                            compute_level_count(round, full_res)
                        };

                        let level_size = |full_res: u32, level_index: u32| {
                            compute_level_size(round, full_res, level_index)
                        };

                        // TODO cache all these level values?? and reuse algorithm from crate::file::meta

                        match tiles.level_mode {
                            LevelMode::Singular => Levels::Singular(part_data(data_size)),

                            LevelMode::MipMap => Levels::Mip(
                                (0..level_count(data_size.0.max(data_size.1)))
                                    .map(|level|{
                                        let width = level_size(data_size.0, level);
                                        let height = level_size(data_size.1, level);
                                        part_data((width, height))
                                    })
                                    .collect()
                            ),

                            // TODO put this into Levels::new(..) ?
                            LevelMode::RipMap => Levels::Rip({
                                let level_count = (level_count(data_size.0), level_count(data_size.1));

                                let maps = (0..level_count.0) // TODO test this
                                    .flat_map(|x_level|{ // TODO may swap y and x?
                                        (0..level_count.1).map(move |y_level| {
                                            let width = level_size(data_size.0, x_level);
                                            let height = level_size(data_size.1, y_level);
                                            part_data((width, height))
                                        })
                                    })
                                    .collect();

                                RipMaps { maps, level_count }
                            })
                        }
                    }

                    // scan line blocks never have mip maps? // TODO check if this is true
                    else {
                        Levels::Singular(part_data(data_size))
                    }
                };

                Part { levels, header }
            },

            Some(Kind::DeepScanLine) | Some(Kind::DeepTile) => unimplemented!("deep allocation"),
        }
    }

    pub fn read_block(&mut self, read: &mut impl Read, _level: (u32, u32), position: (u32, u32), block_size: (u32, u32)) -> ReadResult<()> {
        match &mut self.levels {
            Levels::Singular(ref mut part) => {
                match part {
                    PartData::Flat(ref mut pixels) => {
                        let image_width = pixels.dimensions.0;

                        for line_index in 0..block_size.1 {
                            let start_index = ((position.1 + line_index) * image_width) as usize;
                            let end_index = start_index + image_width as usize;

                            for channel in &mut pixels.channels { // FIXME must be sorted alphabetically!
                                match channel {
                                    Array::F16(ref mut target) =>
                                        read_f16_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data

                                    Array::F32(ref mut target) =>
                                        read_f32_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data

                                    Array::U32(ref mut target) =>
                                        read_u32_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data
                                }
                            }
                        }
                    },

                    _ => unimplemented!("deep pixel accumulation")
                }
            },

            _ => unimplemented!("mip/rip pixel accumulation")
        };

        Ok(())
    }
    /*
    let image_width = pixels.dimensions.0 as usize;
    for channel in &mut pixels.channels { // FIXME must be sorted alphabetically!
        let block_y = position.1 as usize;
        let block_height = size.1 as usize;

        let start_index = block_y * image_width;
        let end_index = start_index + block_height * image_width;

        match channel {
            Array::F16(ref mut target) =>
                read_f16_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data

            Array::F32(ref mut target) =>
                read_f32_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data

            Array::U32(ref mut target) =>
                read_u32_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data
        }
    }
    */

}


// TODO
//impl Levels {
//    pub fn new() -> Self {
//
//    }
//}
