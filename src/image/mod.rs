
pub mod full;

use crate::meta::attributes::*;
use crate::compression::{Compression, ByteVec};
use crate::math::*;
use std::io::Read;
use crate::error::{Result, Error};
use crate::meta::{MetaData, TileIndices, Header};
use crate::chunks::{Chunk, Block, TileBlock, ScanLineBlock};
use crate::io::PeekRead;
use rayon::iter::{IntoParallelIterator, ParallelIterator};


#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WriteOptions {
    pub parallel_compression: bool,
    pub compression_method: Compression,
    pub line_order: LineOrder,
    pub blocks: BlockOptions
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BlockOptions {
    ScanLineBlocks,

    TileBlocks {
        size: (u32, u32),
        rounding: RoundingMode
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ReadOptions {
    pub parallel_decompression: bool,
}




/// temporarily used to construct images in parallel
#[derive(Clone, PartialEq, Debug)]
pub struct UncompressedBlock {
    part_index: usize,
    tile: TileIndices,
    data: ByteVec,
}


/// reads all chunks sequentially without seeking
pub fn read_all_chunks<T>(
    read: impl Read, options: ReadOptions,
    new: impl Fn(&[Header]) -> Result<T>,
    insert: impl Fn(T, UncompressedBlock) -> Result<T>
) -> Result<T>
{

    struct ByteCounter<T> {
        bytes: usize,
        inner: T
    }

    impl<T: Read> Read for ByteCounter<T> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let byte_count = self.inner.read(buf)?;
//            println!("read bytes {} to {} ({})", self.bytes, self.bytes + byte_count, byte_count);
            self.bytes += byte_count;
            Ok(byte_count)
        }
    }

    impl<T> ByteCounter<T> {
        pub fn byte_count(&self) -> usize { self.bytes }
    }

    let read = ByteCounter { inner: read, bytes: 0, };


    let mut read = PeekRead::new(read);
    let meta_data = MetaData::read_from_buffered_peekable(&mut read)?;
    let chunk_count = MetaData::skip_offset_tables(&mut read, &meta_data.headers)? as usize;
    println!("skipping tables (chunk count: {})", chunk_count);

    let mut value = new(meta_data.headers.as_slice())?;

    let has_compression = meta_data.headers.iter() // do not use parallel stuff for uncompressed images
        .find(|header| header.compression != Compression::Uncompressed).is_some();

    if options.parallel_decompression && has_compression {
        // TODO without double collect!
        let compressed: Result<Vec<Chunk>> = (0..chunk_count)
            .map(|_| Chunk::read(&mut read, &meta_data))
            .collect();

        let decompress = compressed?.into_par_iter().map(|chunk|
            UncompressedBlock::from_compressed(chunk, &meta_data)
        );

        // TODO without double collect!
        let decompressed: Result<Vec<UncompressedBlock>> = decompress.collect();

        for decompressed in decompressed? {
            value = insert(value, decompressed)?;
        }
    }
    else {
        for c in 0..chunk_count {
            // TODO avoid all allocations for uncompressed data
            let chunk = Chunk::read(&mut read, &meta_data)?;
            let decompressed = UncompressedBlock::from_compressed(chunk, &meta_data)?;

//            println!("reading chunk {}",  c);

            value = insert(value, decompressed)?;
        }
    }

    Ok(value)
}


impl UncompressedBlock {
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    pub fn from_compressed(chunk: Chunk, meta_data: &MetaData) -> Result<Self> {
        let header: &Header = meta_data.headers.get(chunk.part_number as usize)
            .ok_or(Error::invalid("chunk part index"))?;

        let raw_coordinates = header.get_raw_block_coordinates(&chunk.block)?;
        let tile_data_indices = header.get_block_data_indices(&chunk.block)?;
        raw_coordinates.validate(Some(header.data_window.dimensions()))?;

        match chunk.block {
            Block::Tile(TileBlock { compressed_pixels, .. }) |
            Block::ScanLine(ScanLineBlock { compressed_pixels, .. }) => {
                let data = header.compression.decompress_image_section(header, compressed_pixels, raw_coordinates)?;
                Ok(UncompressedBlock { part_index: chunk.part_number as usize, tile: tile_data_indices, data,  })
            },

            _ => return Err(Error::unsupported("deep data"))
        }
    }

}


impl Default for WriteOptions {
    fn default() -> Self { Self::fast_writing() }
}

impl Default for ReadOptions {
    fn default() -> Self { Self::fast_loading() }
}


impl WriteOptions {
    pub fn fast_writing() -> Self {
        WriteOptions {
            parallel_compression: true,

            // RLE has low runtime cost but great compression for areas with solid color
            compression_method: Compression::RLE,

            line_order: LineOrder::Unspecified,
            blocks: BlockOptions::ScanLineBlocks
        }
    }

    pub fn small_image() -> Self {
        WriteOptions {
            parallel_compression: true,
            compression_method: Compression::ZIP16, // TODO test if this is one of the smallest
            line_order: LineOrder::Unspecified,
            blocks: BlockOptions::ScanLineBlocks
        }
    }

    pub fn small_writing() -> Self {
        WriteOptions {
            parallel_compression: false,
            compression_method: Compression::Uncompressed,
            line_order: LineOrder::Unspecified,
            blocks: BlockOptions::ScanLineBlocks
        }
    }

    pub fn debug() -> Self {
        WriteOptions {
            parallel_compression: false,
            compression_method: Compression::Uncompressed,
            line_order: LineOrder::Unspecified,
            blocks: BlockOptions::ScanLineBlocks
        }
    }
}

impl ReadOptions {

    pub fn fast_loading() -> Self {
        ReadOptions {
            parallel_decompression: true
        }
    }

    pub fn small_loading() -> Self {
        ReadOptions {
            parallel_decompression: false
        }
    }

    pub fn debug() -> Self {
        ReadOptions {
            parallel_decompression: false
        }
    }

}

impl BlockOptions {
    pub fn has_tiles(&self) -> bool {
        match self {
            BlockOptions::TileBlocks { .. } => true,
            _ => false
        }
    }
}