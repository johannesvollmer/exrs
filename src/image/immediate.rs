
use ::file::meta::Header;
//use ::std::io::{Read, Seek, SeekFrom};
use ::smallvec::SmallVec;
use ::file::data::uncompressed::*;
//use ::file::attributes::{Text};
//use ::file::chunks::TileCoordinates;




/// any openexr image, loads all available data immediately into memory
/// can be constructed using `::file::File`
pub struct Image {
    pub version: ::file::meta::Version,
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
    // FIXME should be descending, starting with full-res instead!
    pub levels: Levels

    // offset tables are already processed while loading 'data'
    // TODO skip reading offset tables if not required?
}

pub type Levels = SmallVec<[PartData; 12]>;

/// one `type` per Part
pub enum PartData {
    /// One single array containing all pixels, row major left to right, top to bottom
    /// same length as `Part.channels` field
    Flat(PerChannel<Array>),

    /// scan line blocks are stored from top to bottom, row major.
    Deep/*ScanLine*/(PerChannel<Vec<DeepScanLineBlock>>),

    // /// Blocks are stored from top left to bottom right, row major.
    // DeepTile(PerChannel<Vec<DeepTileBlock>>),
}



use ::file::meta::MetaData;
use ::file::data::compressed::Chunks;
use ::file::io::*;
use file::meta::attributes::PixelType;
use half::f16;

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


impl Image {
    pub fn from_raw(meta_data: MetaData, chunks: Chunks) -> ReadResult<Self> {
        meta_data.validate()?;
//        data.validate()?;

        let MetaData { version, mut headers, .. } = meta_data; // TODO skip offset table reading if possible

        // TODO parallel decompressing
        match chunks {
            Chunks::SinglePart(part) => {
                let header = headers.pop().expect("single part without header");

                // contains ALL pixels per channel
                // TODO LEVELS
                let mut decompressed_channels: PerChannel<Array> = header.channels()
                    .iter().map(|channel|{
                        match channel.pixel_type {
                            PixelType::U32 => Array::U32(Vec::with_capacity(64/* TODO */)),
                            PixelType::F16 => Array::F16(Vec::with_capacity(64/* TODO */)),
                            PixelType::F32 => Array::F32(Vec::with_capacity(64/* TODO */)),
                        }
                    })
                    .collect();

                {
                    let (data_width, data_height) = header.data_window().dimensions();
                    let (data_width, data_height) = (data_width as usize, data_height as usize);

                    let channels = header.channels();
                    let compression = header.compression();

                    use ::file::data::compressed::SinglePartChunks::*;

                    match part {
                        ScanLine(scan_lines) => {
                            let lines_per_block = compression.scan_lines_per_block();
                            println!("what about line order");
                            println!("what about mip map levels");

                            for (index, compressed_data) in scan_lines.iter().enumerate() {
                                // how much the last row is cut off:
                                let block_end = (index + 1) * lines_per_block;
                                let block_overflow = block_end.checked_sub(data_height).unwrap_or(0);
                                let height = lines_per_block - block_overflow;

                                let mut target = PerChannel::with_capacity(channels.len());
                                for channel in channels {
                                    let x_size = data_width / channel.x_sampling as usize; // TODO is that how sampling works?
                                    let y_size = height / channel.y_sampling as usize;
                                    let size = x_size * y_size;
                                    match channel.pixel_type {
                                        PixelType::U32 => target.push(Array::U32(vec![0; size])),
                                        PixelType::F16 => target.push(Array::F16(vec![f16::from_f32(0.0); size])),
                                        PixelType::F32 => target.push(Array::F32(vec![0.0; size])),
                                    }
                                }

                                let decompressed = compression.decompress(
                                    DataBlock::ScanLine(target),
                                    &compressed_data.compressed_pixels, None // uncompressed_size
                                ).unwrap(/* TODO */);

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
                            use ::file::meta::compute_level_size;

                            // TODO what about line order
                            let tile_description = header.tiles()
                                .expect("Check failed: `tiles` missing");

                            let default_width = tile_description.x_size;
                            let default_height = tile_description.y_size;
                            let round = tile_description.rounding_mode;

                            for tile in &tiles {
                                let level_x = tile.coordinates.level_x;
                                let level_data_width = compute_level_size(round, data_width as u32, level_x as u32);

                                let default_right = tile.coordinates.tile_x as u32 + default_width;
                                let right_overflow = default_right.checked_sub(level_data_width).unwrap_or(0);

                                let level_y = tile.coordinates.level_y;
                                let level_data_height = compute_level_size(round, data_height as u32, level_y as u32);

                                assert!(level_x != 1 || level_y != 1, "unimplemented: tiled levels data unpacking");

                                let default_bottom = tile.coordinates.tile_y as u32 + default_height;
                                let bottom_overflow = default_bottom.checked_sub(level_data_height).unwrap_or(0);

                                let width = default_width - right_overflow;
                                let height = default_height - bottom_overflow;

                                let mut target = PerChannel::with_capacity(channels.len());
                                for channel in channels {
                                    let x_size = width / channel.x_sampling as u32; // TODO is that how sampling works?
                                    let y_size = height / channel.y_sampling as u32; // TODO rounding mode?
                                    let size = (x_size * y_size) as usize;
                                    match channel.pixel_type {
                                        PixelType::U32 => target.push(Array::U32(vec![0; size])),
                                        PixelType::F16 => target.push(Array::F16(vec![f16::from_f32(0.0); size])),
                                        PixelType::F32 => target.push(Array::F32(vec![0.0; size])),
                                    }
                                }

                                let decompressed = compression.decompress(
                                    DataBlock::Tile(target),
                                    &tile.compressed_pixels, None // uncompressed_size
                                ).unwrap(/* TODO */);

                                expect_variant!(decompressed, DataBlock::Tile(decompressed_scan_line_channels) => {
                                    for (channel_index, decompressed_channel) in decompressed_scan_line_channels.iter().enumerate() {
                                        decompressed_channels[channel_index].extend_from_slice(&decompressed_channel);
                                    }
                                })

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
                        levels: smallvec![PartData::Flat(decompressed_channels)],
                    }],
                })
            },
            Chunks::MultiPart(parts) => unimplemented!()
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





