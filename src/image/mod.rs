//! The `image` module is for interpreting the loaded file data.
//!


use smallvec::SmallVec;
use half::f16;
use rayon::prelude::{IntoParallelIterator};
use rayon::iter::{ParallelIterator};

use crate::file::data::*;
use crate::file::io::*;
use crate::file::meta::*;
use crate::error::validity::*;
use crate::file::meta::attributes::*;
use crate::file::*;

use crate::file::compute_level_count;
use crate::file::data::compression::{ByteVec, Compression};
use crate::error::{ReadResult};
use std::io::BufReader;


pub use crate::file::io::Data;

// TODO notes:
// Channels with an x or y sampling rate other than 1 are allowed only in flat, scan-line based images. If an image is deep or tiled, then the x and y sampling rates for all of its channels must be 1.
// Scan-line based images cannot be multi-resolution images.


pub mod meta {
    use crate::file::meta::MetaData;
    use std::io::{Read, BufReader};
    use crate::error::ReadResult;
    use crate::file::io::PeekRead;
    use std::fs::File;

    #[must_use]
    pub fn read_from_file(path: &::std::path::Path) -> ReadResult<(MetaData, PeekRead<BufReader<File>>)> {
        read_from_unbuffered(File::open(path)?)
    }

    /// assumes that the provided reader is not buffered, and will create a buffer for it
    #[must_use]
    pub fn read_from_unbuffered<R: Read>(unbuffered: R) -> ReadResult<(MetaData, PeekRead<BufReader<R>>)> {
        read_from_buffered(BufReader::new(unbuffered))
    }

    #[must_use]
    pub fn read_from_buffered<R: Read>(buffered: R) -> ReadResult<(MetaData, PeekRead<R>)> {
        let mut read = PeekRead::new(buffered);
        let meta = MetaData::read_validated(&mut read)?;
        Ok((meta, read))
    }
}




#[derive(Clone, PartialEq, Debug)]
pub struct Image {
    pub parts: Parts
}

/// an exr image can store multiple parts (multiple bitmaps inside one image)
pub type Parts = SmallVec<[Part; 3]>;

#[derive(Clone, PartialEq, Debug)]
pub struct Part {
    pub data_window: I32Box2,
    pub display_window: I32Box2,

    pub pixel_aspect: f32,
    pub screen_window_center: (f32, f32),
    pub screen_window_width: f32,

    pub name: Option<Text>,
    pub attributes: Attributes,

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
    pub channels: Channels,
}


pub type Channels = SmallVec<[Channel; 5]>;

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub name: Text,
    pub content: ChannelData,
    pub is_linear: bool,
    pub sampling: (usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChannelData {
    /// The representation of 16-bit floating-point numbers is analogous to IEEE 754,
    /// but with 5 exponent bits and 10 bits for the fraction.
    ///
    /// Currently this crate is using the `half` crate, which is an implementation of the IEEE 754-2008 standard, meeting that requirement.
    F16(SampleMaps<f16>),

    F32(SampleMaps<f32>),

    U32(SampleMaps<u32>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SampleMaps<Sample> {
    Flat (Levels<FlatSamples<Sample>>),
    Deep (Levels<DeepSamples<Sample>>), // TODO can deep images even have levels?
}

// FIXME should be descending and starting with full-res instead!
#[derive(Clone, PartialEq, Debug)]
pub enum Levels<Samples> {
    Singular(SampleBlock<Samples>),
    Mip(LevelMaps<Samples>),
    Rip(RipMaps<Samples>),
}

pub type LevelMaps<Samples> = Vec<SampleBlock<Samples>>;

#[derive(Clone, PartialEq, Debug)]
pub struct RipMaps<Samples> {
    pub map_data: LevelMaps<Samples>,
    pub level_count: (usize, usize),
}

#[derive(Clone, PartialEq, Debug)]
pub struct SampleBlock<Samples> {
    pub resolution: (usize, usize),
    pub samples: Samples
}

pub type FlatSamples<Sample> = Vec<Sample>;

pub type DeepSamples<Sample> = Vec<DeepLine<Sample>>;
// TODO do not store line by line in a separate vector!

#[derive(Clone, Debug, PartialEq)]
pub struct DeepLine<Sample> {
    // TODO do not store line by line in a separate vector!
    pub samples: Vec<Sample>,
    pub index_table: Vec<u32>,
}








/// temporarily used to construct images in parallel
#[derive(Clone, PartialEq, Debug)]
pub struct DecompressedBlock {
    part_index: usize,
    tile: TileIndices,
    data: ByteVec,
}

#[must_use]
pub fn read_from_file(path: &::std::path::Path, parallel: bool) -> ReadResult<Image> {
    read_from_unbuffered(::std::fs::File::open(path)?, parallel)
}

/// assumes that the provided reader is not buffered, and will create a buffer for it
#[must_use]
pub fn read_from_unbuffered(unbuffered: impl Read, parallel: bool) -> ReadResult<Image> {
    read_from_buffered(BufReader::new(unbuffered), parallel)
}

// TODO use custom simple peek-read instead of seekable read?
#[must_use]
pub fn read_from_buffered(buffered_read: impl Read, parallel: bool) -> ReadResult<Image> {
    let mut read = PeekRead::new(buffered_read);

    let MetaData { headers, offset_tables, requirements } = MetaData::read_validated(&mut read)?;
    let chunk_reader = ChunkReader::new(read, requirements.is_multipart(), &headers, &offset_tables);

    let mut image = Image::new(&headers);

    let has_compression = headers.iter() // do not use parallel stuff for uncompressed images
        .find(|header| header.compression != Compression::None).is_some();

    if parallel && has_compression {
        let chunks: Vec<ReadResult<Chunk>> = chunk_reader.collect();
        let blocks = chunks.into_par_iter().map(|chunk| chunk.and_then(|chunk|
            DecompressedBlock::decompress(chunk, &headers)
        ));

        let blocks: Vec<ReadResult<DecompressedBlock>> = blocks.collect(); // TODO without double collect!

        for block in blocks {
            image.insert_block(block?)?;
        }
    }
    else {
        for block in chunk_reader
            .map(|chunk| chunk.and_then(|chunk|
                DecompressedBlock::decompress(chunk, &headers)
            ))
        {
            image.insert_block(block?)?;
        }
    }

    Ok(image)
}

impl DecompressedBlock {
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    pub fn decompress(chunk: Chunk, headers: &Headers) -> ReadResult<Self> {
        let part_count = headers.len();
        let header: &Header = headers.get(chunk.part_number as usize)
            .ok_or(Invalid::Content(Value::Chunk("part index"), Required::Max(part_count)))?;

        let raw_coordinates = header.get_raw_block_coordinates(&chunk.block)?;
        let tile_data_indices = header.get_block_data_indices(&chunk.block)?;

        match chunk.block {
            Block::Tile(TileBlock { compressed_pixels, .. }) | Block::ScanLine(ScanLineBlock { compressed_pixels, .. })=> {
                let data = header.compression.decompress_bytes(header, compressed_pixels, raw_coordinates)?;
                Ok(DecompressedBlock { part_index: chunk.part_number as usize, tile: tile_data_indices, data,  })
            },

            _ => unimplemented!()
        }
    }
}

/*pub fn decompress_blocks_parallel(
    headers: &Headers, chunk_count: usize, mut chunks: impl Iterator<Item=ReadResult<Chunk>>
) -> (ThreadPool, impl Iterator<Item=ReadResult<DecompressedBlock>>)
{
    use threadpool::ThreadPool;
    use std::sync::mpsc::channel;

    // contains reference to Reader, which serially reads chunks from the file
    let mut chunks = Mutex::new(&mut chunks);
    let pool = ThreadPool::new(num_cpus::get());
    let part_count = headers.len();

    use std::sync::mpsc::channel;
    let (sender, receiver) = channel();

    for _ in 0 .. chunk_count {
        let sender = sender.clone();
        pool.execute(move || {
            let chunk: ReadResult<Chunk> = {
                let mut chunks = chunks.lock().expect("mutex locking error");
                if let Some(chunk_result) = chunks.next() {
                    chunk_result
                }
                else {
                    sender.send(Err(ReadError::Invalid(Invalid::Missing(Value::Chunk("data")))));
                    return;
                }
            };

            let decompressed: ReadResult<DecompressedBlock> = chunk
                .and_then(|chunk| DecompressedBlock::decompress(chunk, headers));

            sender.send(decompressed).expect("thread pool error");
        });
    }

    (pool, receiver.into_iter())
}*/

impl Image {
    pub fn new(headers: &Headers) -> Self {
        Image {
            parts: headers.iter().map(Part::new).collect()
        }
    }

    pub fn insert_block(&mut self, block: DecompressedBlock) -> ReadResult<()> {
        let part_count = self.parts.len();
        let part = self.parts.get_mut(block.part_index)
            .ok_or(Invalid::Content(Value::Chunk("part index"), Required::Max(part_count)))?;

        part.insert_block(&mut block.data.as_slice(), block.tile)
    }
}

impl Part {

    /// allocates all the memory necessary to hold the pixel data,
    /// zeroed out, ready to be filled with actual pixel data
    pub fn new(header: &Header) -> Self {
        let _data_size = header.data_window.dimensions();

        match header.kind {
            None | Some(Kind::ScanLine) | Some(Kind::Tile) => {
                /*let levels = {

                    let part_data = |dimensions: (u32, u32)| {
                        let data = header.channels.list.iter()
                            .map(|channel| { match channel.pixel_type {
                                PixelType::F16 => ChannelData::F16(vec![half::f16::ZERO; channel.subsampled_pixels(dimensions) as usize]),
                                PixelType::F32 => ChannelData::F32(vec![0.0; channel.subsampled_pixels(dimensions) as usize]),
                                PixelType::U32 => ChannelData::U32(vec![0; channel.subsampled_pixels(dimensions) as usize]),
                            }})
                            .collect();

                        PartData::Flat(SampleBlock { resolution: dimensions, channel_data: data })
                    };

                    if let Some(tiles) = &header.tiles {
                        debug_assert_eq!(header.kind, Some(Kind::Tile));

                        let round = tiles.rounding_mode;
                        let level_count = |full_res: u32| {
                            compute_level_count(round, full_res)
                        };

                        let level_size = |full_res: u32, level_index: u32| {
                            compute_level_size(round, full_res, level_index)
                        };

                        // TODO cache all these level values?? and reuse algorithm from crate::file::meta::compute_offset_table_sizes?

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

                                RipMaps { map_data: maps, level_count }
                            })
                        }
                    }

                    // scan line blocks never have mip maps? // TODO check if this is true
                    else {
                        Levels::Singular(part_data(data_size))
                    }
                };*/

                Part {
                    data_window: header.data_window,
                    display_window: header.display_window,
                    pixel_aspect: header.pixel_aspect,
                    screen_window_center: header.screen_window_center,
                    screen_window_width: header.screen_window_width,
                    name: header.name.clone(),
                    attributes: header.custom_attributes.clone(),
                    channels: header.channels.list.iter().map(|channel| Channel::new(header, channel)).collect()
                }
            },

            Some(Kind::DeepScanLine) | Some(Kind::DeepTile) => {
                unimplemented!()
            },
        }
    }


    pub fn insert_block(&mut self, mut data: &[u8], area: TileIndices) -> ReadResult<()> {
        let level = (area.level.0 as usize, area.level.1 as usize);

        for y in area.position.1 .. area.position.1 + area.size.1 {
            for channel in &mut self.channels {
                channel.read_line(&mut data, level, (area.position.0 as usize, y as usize), area.size.0 as usize)?;
            }
        }

        Ok(())
    }
}

impl Channel {
    pub fn new(header: &Header, channel: &crate::file::meta::attributes::Channel) -> Self {
        Channel {
            name: channel.name.clone(),
            is_linear: channel.is_linear,
            sampling: (channel.sampling.0 as usize, channel.sampling.1 as usize),

            content: match channel.pixel_type {
                PixelType::F16 => ChannelData::F16(SampleMaps::new(header)),
                PixelType::F32 => ChannelData::F32(SampleMaps::new(header)),
                PixelType::U32 => ChannelData::U32(SampleMaps::new(header)),
            },
        }
    }

    pub fn read_line(&mut self, block: &mut impl Read, level:(usize, usize), position: (usize, usize), length: usize) -> ReadResult<()> {
        match &mut self.content {
            ChannelData::F16(maps) => maps.read_line(block, level, position, length),
            ChannelData::F32(maps) => maps.read_line(block, level, position, length),
            ChannelData::U32(maps) => maps.read_line(block, level, position, length),
        }
    }
}

impl<Sample: Data + std::fmt::Debug> SampleMaps<Sample> {
    pub fn new(header: &Header) -> Self {
        if header.has_deep_data() {
            SampleMaps::Deep(Levels::new(header))
        }
        else {
            SampleMaps::Flat(Levels::new(header))
        }
    }

    pub fn read_line(&mut self, block: &mut impl Read, level:(usize, usize), position: (usize, usize), length: usize) -> ReadResult<()> {
        match self {
            SampleMaps::Deep(ref mut levels) => levels.read_line(block, level, position, length),
            SampleMaps::Flat(ref mut levels) => levels.read_line(block, level, position, length),
        }
    }

    pub fn flat_samples(&self) -> Option<&Levels<FlatSamples<Sample>>> {
        match self {
            SampleMaps::Flat(ref levels) => Some(levels),
            _ => None
        }
    }

    pub fn deep_samples(&self) -> Option<&Levels<DeepSamples<Sample>>> {
        match self {
            SampleMaps::Deep(ref levels) => Some(levels),
            _ => None
        }
    }
}

impl<S: Samples> Levels<S> {
    pub fn new(header: &Header) -> Self {
        let data_size = header.data_window.dimensions();

        if let Some(tiles) = &header.tiles {
            debug_assert_eq!(header.kind, Some(Kind::Tile));
            let round = tiles.rounding_mode;

            match tiles.level_mode {
                LevelMode::Singular => Levels::Singular(SampleBlock::new(data_size)),

                LevelMode::MipMap => Levels::Mip(
                    mip_map_resolutions(round, data_size)
                        .map(|level_size| SampleBlock::new(level_size)).collect()
                ),

                // TODO put this into Levels::new(..) ?
                LevelMode::RipMap => Levels::Rip({
                    let level_count_x = compute_level_count(round, data_size.0);
                    let level_count_y = compute_level_count(round, data_size.1);
                    let maps = rip_map_resolutions(round, data_size)
                        .map(|level_size| SampleBlock::new(level_size)).collect();

                    RipMaps { map_data: maps, level_count: (level_count_x as usize, level_count_y as usize) }
                })
            }
        }

        // scan line blocks never have mip maps? // TODO check if this is true
        else {
            Levels::Singular(SampleBlock::new(data_size))
        }
    }

    pub fn read_line(&mut self, read: &mut impl Read, level:(usize, usize), position: (usize, usize), length: usize) -> ReadResult<()> {
        match self {
            Levels::Singular(ref mut block) => {
                debug_assert_eq!(level, (0,0), "singular image cannot read leveled blocks");
                block.read_line(read, position, length)?;
            },

            Levels::Mip(block) => {
                debug_assert_eq!(level.0, level.1, "mip map levels must be equal on x and y"); // TODO err instead?
                let max = block.len();

                block.get_mut(level.0)
                    .ok_or(Invalid::Content(Value::MapLevel, Required::Max(max)))?
                    .read_line(read, position, length)?;
            },

            Levels::Rip(block) => {
                let max = block.map_data.len();

                block.get_by_level_mut(level)
                    .ok_or(Invalid::Content(Value::MapLevel, Required::Max(max)))?
                    .read_line(read, position, length)?;
            }
        }

        Ok(())
    }

    pub fn largest(&self) -> &SampleBlock<S> {
        match self {
            Levels::Singular(data) => data,
            Levels::Mip(maps) => &maps[0], // TODO is this really the largest one?
            Levels::Rip(rip_map) => &rip_map.map_data[0], // TODO test!
        }
    }

    pub fn levels(&self) -> &[SampleBlock<S>] {
        match self {
            Levels::Singular(ref data) => std::slice::from_ref(data),
            Levels::Mip(ref maps) => maps, // TODO is this really the largest one?
            Levels::Rip(ref rip_map) => &rip_map.map_data, // TODO test!
        }
    }
}


impl<S: Samples> SampleBlock<S> {
    pub fn new(resolution: (u32, u32)) -> Self {
        let resolution = (resolution.0 as usize, resolution.1 as usize);
        SampleBlock { resolution, samples: S::new(resolution) }
    }

    pub fn read_line(&mut self, read: &mut impl Read, position: (usize, usize), length: usize) -> ReadResult<()> {
        // TODO assert area lies inside this buffer
        self.samples.read_line(read, position, length, self.resolution.0)
    }
}

pub trait Samples {
    fn new(resolution: (usize, usize)) -> Self;
    fn read_line(&mut self, read: &mut impl Read, position: (usize, usize), length: usize, image_width: usize) -> ReadResult<()>;
}

impl<Sample: io::Data> Samples for DeepSamples<Sample> {
    fn new(resolution: (usize, usize)) -> Self {
        vec![
            DeepLine { samples: Vec::new(), index_table: vec![0; resolution.0] };
            resolution.1
        ]
    }

    fn read_line(&mut self, read: &mut impl Read, _position: (usize, usize), length: usize, _image_width: usize) -> ReadResult<()> {
        // TODO err on invalid tile position
        self[_position.1 as usize] = DeepLine {
            samples: Sample::read_vec(read, length, 1024*1024*1024)?, // FIXME where tiles, will not be hole line
            index_table: unimplemented!()
        };

        Ok(())
    }
}

impl<Sample: io::Data + Default + Clone + std::fmt::Debug> Samples for FlatSamples<Sample> {
    fn new(resolution: (usize, usize)) -> Self {
        let resolution = (resolution.0 as usize, resolution.1 as usize);
        vec![Sample::default(); resolution.0 * resolution.1]
    }

    fn read_line(&mut self, read: &mut impl Read, position: (usize, usize), length: usize, image_width: usize) -> ReadResult<()> {
        let start_index = position.1 as usize * image_width + position.0 as usize;
        let end_index = start_index + length;

        Sample::read_slice(read, &mut self[start_index .. end_index])?;
        Ok(())
    }
}


/*impl PartData {
    fn read_lines(&mut self, read: &mut impl Read, position: (u32, u32), block_size: (u32, u32)) -> ReadResult<()> {
        match self {
            PartData::Flat(ref mut pixels) => {
                let image_width = pixels.dimensions.0;

                for line_index in 0..block_size.1 {
                    let start_index = ((position.1 + line_index) * image_width) as usize;
                    let end_index = start_index + block_size.0 as usize;

                    for channel in &mut pixels.channel_data {
                        match channel {
                            ChannelData::F16(ref mut target) =>
                                read_f16_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data

                            ChannelData::F32(ref mut target) =>
                                read_f32_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data

                            ChannelData::U32(ref mut target) =>
                                read_u32_array(read, &mut target[start_index .. end_index])?, // could read directly from file for uncompressed data
                        }
                    }
                }

                Ok(())
            },

            _ => unimplemented!("deep pixel accumulation")
        }
    }
}*/

impl<Samples> RipMaps<Samples> {
    pub fn get_level_index(&self, level: (usize, usize)) -> usize {
        self.level_count.0 * level.1 as usize + level.0 as usize  // TODO check this calculation (x vs y)
    }

    pub fn get_by_level(&self, level: (usize, usize)) -> Option<&SampleBlock<Samples>> {
        self.map_data.get(self.get_level_index(level))
    }

    pub fn get_by_level_mut(&mut self, level: (usize, usize)) -> Option<&mut SampleBlock<Samples>> {
        let index = self.get_level_index(level);
        self.map_data.get_mut(index)
    }
}
