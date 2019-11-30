
use crate::file::meta::Header;
//use ::std::io::{Read, Seek, SeekFrom};
use ::smallvec::SmallVec;
use crate::file::data::uncompressed::*;
//use crate::file::attributes::{Text};
//use crate::file::chunks::TileCoordinates;




/// any openexr image, loads all available data immediately into memory
/// can be constructed using `::file::File`
pub struct Image {
    pub version: crate::file::meta::Version,
    pub parts: Parts, // TODO HashMap<Text, Part> ?
}

pub type Parts = SmallVec<[Part; 3]>;

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

pub enum Levels {
    Singular(PartData),
    Mip(SmallVec<[PartData; 16]>),
    Rip(RipMaps)
}

pub struct RipMaps {
    data: Vec<PartData>,
    x_levels: usize,
    y_levels: usize,
}


/// one `type` per Part
pub enum PartData {
    /// One single array containing all pixels, row major left to right, top to bottom
    /// same length as `Part.channels` field
    // TODO should store sampling_x/_y for simple accessors?
    Flat(PerChannel<Array>),

    /// scan line blocks are stored from top to bottom, row major.
    Deep/*ScanLine*/(PerChannel<Vec<DeepScanLineBlock>>),

    // /// Blocks are stored from top left to bottom right, row major.
    // DeepTile(PerChannel<Vec<DeepTileBlock>>),
}



use crate::file::meta::MetaData;
use crate::file::data::compressed::Chunks;
use crate::file::io::*;
use crate::file::meta::attributes::PixelType;

#[must_use]
pub fn read_file(path: &::std::path::Path) -> ReadResult<Image> {
    buffered_read(::std::fs::File::open(path)?)
}

/// assumes that the provided reader is not buffered, and will create a buffer for it
#[must_use]
pub fn buffered_read<R: Read + Seek>(unbuffered: R) -> ReadResult<Image> {
    read_seekable_buffered(&mut SeekBufRead::new(unbuffered))
}

/// reads the whole image at once
#[must_use]
pub fn read_raw_parts<R: Read + Seek>(read: &mut R) -> ReadResult<(MetaData, Chunks)> {
    let meta_data = MetaData::read(read)?;
    let chunks = Chunks::read(read, &meta_data)?;
    Ok((meta_data, chunks))
}

/// assumes that `Read` is buffered
#[must_use]
pub fn read_seekable_buffered<R: Read + Seek>(read: &mut R) -> ReadResult<Image> {
    let (meta_data, chunks) = read_raw_parts(read)?;
    Image::from_raw(meta_data, chunks) // TODO start compressing while reading more blocks, not just after finishing
}


// TODO feed file_read iterator directly into decompression iterator, without intermediate storage? must be buffered though
// TODO this would enable letting the user handle storing all pixels by just handing him the iterator
impl Image {
    pub fn from_raw(meta_data: MetaData, chunks: Chunks) -> ReadResult<Self> {
        meta_data.validate()?;
//        chunks.validate()?;

        let MetaData { version, mut headers, .. } = meta_data; // TODO skip offset table reading if possible

        // TODO parallel decompressing
        match chunks {
            Chunks::SinglePart(part) => {
                let header = headers.pop().expect("single part without header");
                assert!(headers.is_empty(), "single part with multiple headers");
                assert_eq!(header.line_order(), crate::file::meta::attributes::LineOrder::IncreasingY);

                let (data_width, data_height) = header.data_window().dimensions();
                let (data_width, data_height) = (data_width as usize, data_height as usize);


                /*TODO
                let levels = match header.tiles().level_mode {
                    Singular => Levels::Singular,
                    Mip => Levels::Mip,
                    Rip => Levels::Rip,
                };*/

                // contains ALL pixels per channel for the whole image
                // TODO LEVELS
                let mut decompressed_channels: PerChannel<Array> = header.channels().iter().map(|channel|{
                    let pixels = channel.subsampled_pixels(data_width, data_height);
                    match channel.pixel_type {
                        PixelType::U32 => Array::U32(Vec::with_capacity(pixels)),
                        PixelType::F16 => Array::F16(Vec::with_capacity(pixels)),
                        PixelType::F32 => Array::F32(Vec::with_capacity(pixels)),
                    }
                })
                .collect();


                {
                    let channels = header.channels();
                    let compression = header.compression();
                    use crate::file::data::compressed::SinglePartChunks::*;

                    match part {
                        ScanLine(scan_lines) => {
                            let lines_per_block = compression.scan_lines_per_block();
                            println!("what about line order");
                            println!("what about mip/rip map levels?");

                            // FIXME only iterate if line_order is increasing y, else we need to interleave!!
                            for (index, compressed_data) in scan_lines.iter().enumerate() {
                                // how much the last row is cut off:
                                let block_end = (index + 1) * lines_per_block;
                                let block_overflow = block_end.checked_sub(data_height).unwrap_or(0);
                                let height = lines_per_block - block_overflow;

                                let mut target = PerChannel::with_capacity(channels.len());
                                for channel in channels {
                                    let size = channel.subsampled_pixels(data_width, height);

                                    match channel.pixel_type {
                                        PixelType::U32 => target.push(Array::U32(Vec::with_capacity(size))),
                                        PixelType::F16 => target.push(Array::F16(Vec::with_capacity(size))),
                                        PixelType::F32 => target.push(Array::F32(Vec::with_capacity(size))),
                                    }
                                }

                                let decompressed = compression.decompress(
                                    DataBlock::ScanLine(target),
                                    &compressed_data.compressed_pixels,
                                    data_width
                                )?;

                                expect_variant!(decompressed, DataBlock::ScanLine(decompressed_scan_line_channels) => {
                                    for (channel_index, decompressed_channel) in decompressed_scan_line_channels.iter().enumerate() {
                                        decompressed_channels[channel_index].extend_from_slice(&decompressed_channel);
                                    }
                                })
                                /*if let DataBlock::ScanLine(decompressed_scan_line_channels) = decompressed {
                                    for (channel_index, decompressed_channel) in decompressed_scan_line_channels.iter().enumerate() {
                                        decompressed_channels[channel_index].extend_from_slice(&decompressed_channel);
                                    }
                                } else {
                                    panic!("`decompress` returned wrong block type")
                                }*/
                            }
                        },

                        Tile(tiles) => {
                            use crate::file::meta::compute_level_size;

                            // TODO what about line order
                            let tile_description = header.tiles()
                                .expect("Check failed: `tiles` missing");

                            let default_width = tile_description.x_size;
                            let default_height = tile_description.y_size;
                            let round = tile_description.rounding_mode;


                            // FIXME only iterate if line_order is increasing y, else we need to interleave!!
                            for tile in &tiles {
                                let level_x = tile.coordinates.level_x;
                                let level_data_width = compute_level_size(round, data_width as u32, level_x as u32);

                                let default_right = tile.coordinates.tile_x as u32 + default_width;
                                let right_overflow = default_right.checked_sub(level_data_width).unwrap_or(0);

                                let level_y = tile.coordinates.level_y;
                                let level_data_height = compute_level_size(round, data_height as u32, level_y as u32);

                                assert!(level_x == 1 && level_y == 1, "unimplemented: tiled levels data unpacking");

                                let default_bottom = tile.coordinates.tile_y as u32 + default_height;
                                let bottom_overflow = default_bottom.checked_sub(level_data_height).unwrap_or(0);

                                let width = default_width - right_overflow;
                                let height = default_height - bottom_overflow;

                                let mut target = PerChannel::with_capacity(channels.len());
                                for channel in channels {
                                    let size = channel.subsampled_pixels(width as usize, height as usize); // TODO use usize only
                                    match channel.pixel_type {
                                        PixelType::U32 => target.push(Array::U32(Vec::with_capacity(size))),
                                        PixelType::F16 => target.push(Array::F16(Vec::with_capacity(size))),
                                        PixelType::F32 => target.push(Array::F32(Vec::with_capacity(size))),
                                    }
                                }

                                let decompressed = compression.decompress(
                                    DataBlock::Tile(target),
                                    &tile.compressed_pixels,
                                    width as usize
                                )?;

                                unimplemented!("cannot just append tiles to a flat array");
                                /*expect_variant!(decompressed, DataBlock::Tile(decompressed_scan_line_channels) => {
                                    for (channel_index, decompressed_channel) in decompressed_scan_line_channels.iter().enumerate() {
                                        decompressed_channels[channel_index].extend_from_slice(&decompressed_channel);
                                    }
                                })*/

                                /*if let DataBlock::Tile(decompressed_scan_line_channels) = decompressed {
                                    for (channel_index, decompressed_channel) in decompressed_scan_line_channels.iter().enumerate() {
                                        decompressed_channels[channel_index].extend_from_slice(&decompressed_channel);
                                    }
                                } else {
                                    panic!("`decompress` returned wrong block type")
                                }*/
                            }
                        },


                        // let map_level_x = unimplemented!("are mip map levels only for tiles?");
                        // let map_level_y = unimplemented!();

                        _ => unimplemented!("non-scanline uncompressed images")
                    };
                }

                Ok(Image {
                    version,
                    parts: smallvec![Part {
                        header,
                        levels: crate::image::immediate::Levels::Singular(
                            PartData::Flat(decompressed_channels)
                        ),
                    }],
                })
            },
            Chunks::MultiPart(parts) => unimplemented!()
        }
    }
}

impl Levels {
    pub fn full(&self) -> &PartData {
        match *self {
            Levels::Singular(ref data) => data,
            Levels::Mip(ref data) => &data[0],
            Levels::Rip(ref rip_map) => &rip_map.data[0], // TODO test!
        }
    }
}

#[must_use]
pub fn write_file(path: &str, image: &Image) -> WriteResult {
    write(&mut ::std::fs::File::open(path)?, image)
}

#[must_use]
pub fn write<W: Write>(write: &mut W, image: &Image) -> WriteResult {
    // image.meta_data.write(write)?;
    // image.chunks.write(write, &image.meta_data)

//    When a scan-line based file is read, random
//    access to the scan lines is possible; the scan lines can be read in any order. However, reading the scan
//    lines in the same order as they were written causes the file to be read sequentially, without “seek”
//    operations, and as fast as possible.

    // write max samples and offset tables
    unimplemented!()
}





