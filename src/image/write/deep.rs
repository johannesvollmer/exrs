//! Writing deep data to EXR files.
//!
//! This module provides infrastructure for writing deep data (multiple samples per pixel).
//! Deep data writing uses the block-level API, with `UncompressedDeepBlock::compress_to_chunk()`.
//!
//! ## Example
//!
//! ```no_run
//! use exr::prelude::*;
//! use exr::block::{self, UncompressedDeepBlock, BlockIndex};
//! use exr::math::Vec2;
//! use exr::meta::{Headers, MetaData};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create headers with deep data flag
//! let mut header = Header::default();
//! header.deep = true;
//! // ... configure other header fields ...
//!
//! let headers = Headers::new(vec![header]);
//!
//! block::write(
//!     std::fs::File::create("deep.exr")?,
//!     headers,
//!     true,
//!     |meta, chunk_writer| {
//!         // For each block in the image:
//!         let block = UncompressedDeepBlock {
//!             index: BlockIndex {
//!                 layer: 0,
//!                 pixel_position: Vec2(0, 0),
//!                 pixel_size: Vec2(64, 64),
//!                 level: Vec2(0, 0),
//!             },
//!             pixel_offset_table: vec![/* cumulative sample counts */],
//!             sample_data: vec![/* all samples for all channels */],
//!         };
//!
//!         let chunk = block.compress_to_chunk(&meta.headers)?;
//!         chunk_writer.write_chunk(&chunk)?;
//!
//!         Ok(())
//!     },
//! )?;
//! # Ok(())
//! # }
//! ```

use crate::block::{self, writer::ChunksWriter, BlockIndex, UncompressedDeepBlock};
use crate::compression::Compression;
use crate::error::{Result, UnitResult};
use crate::meta::attribute::{ChannelList, Text};
use crate::meta::header::Header;
use crate::meta::BlockDescription;
use smallvec::smallvec;
use std::path::Path;

/// Helper to create a header configured for deep data.
pub fn create_deep_header(
    name: impl Into<Text>,
    width: usize,
    height: usize,
    channels: ChannelList,
    compression: Compression,
) -> Result<Header> {
    if !compression.supports_deep_data() {
        return Err(crate::error::Error::unsupported(format!(
            "compression {:?} does not support deep data",
            compression
        )));
    }

    let mut header = Header::new(name.into(), (width, height), channels.list).with_encoding(
        compression,
        BlockDescription::ScanLines,
        crate::meta::attribute::LineOrder::Increasing,
    );
    header.deep = true;
    header.deep_data_version = Some(1);
    // Set a reasonable default for max samples per pixel
    // This can be updated later if needed
    header.max_samples_per_pixel = Some(100);

    Ok(header)
}

/// Write deep samples to an EXR file using the block-level API.
///
/// This is a low-level function that requires manually constructing `UncompressedDeepBlock` instances.
/// The blocks must cover the entire image and be provided in the correct order.
///
/// For a simpler interface, consider using the builder API once it's fully implemented.
///
/// ## Arguments
///
/// * `path` - Output file path
/// * `header` - Image header (must have `deep = true`)
/// * `create_blocks` - Callback that yields deep blocks for the image
///
/// ## Example
///
/// ```no_run
/// use exr::prelude::*;
/// use exr::image::write::deep::*;
/// use exr::block::{BlockIndex, UncompressedDeepBlock};
/// use exr::math::Vec2;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let header = create_deep_header(
///     512, 512,
///     ChannelList::default(),
///     Compression::ZIP1,
/// )?;
///
/// write_deep_blocks_to_file(
///     "output.exr",
///     header,
///     |block_index| {
///         // Create UncompressedDeepBlock for this block_index
///         Ok(UncompressedDeepBlock {
///             index: block_index,
///             pixel_offset_table: vec![/* ... */],
///             sample_data: vec![/* ... */],
///         })
///     },
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn write_deep_blocks_to_file<F>(
    path: impl AsRef<Path>,
    header: Header,
    mut create_block: F,
) -> UnitResult
where
    F: FnMut(BlockIndex) -> Result<UncompressedDeepBlock>,
{
    if !header.deep {
        return Err(crate::error::Error::invalid(
            "header must have deep = true for writing deep data",
        ));
    }

    let headers = smallvec![header];

    crate::io::attempt_delete_file_on_write_error(path.as_ref(), move |file| {
        block::write(file, headers, true, |meta, chunk_writer| {
            // Iterate through all blocks in the image
            for (index_in_header, block_index) in
                block::enumerate_ordered_header_block_indices(&meta.headers)
            {
                let deep_block = create_block(block_index)?;
                let chunk = deep_block.compress_to_chunk(&meta.headers)?;
                chunk_writer.write_chunk(index_in_header, chunk)?;
            }
            Ok(())
        })
    })
}

/// Helper to check if a header is configured for deep data.
pub fn is_deep_header(header: &Header) -> bool {
    header.deep
}
