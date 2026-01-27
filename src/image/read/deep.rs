//! Reading deep data from EXR files.
//!
//! This module provides infrastructure for reading deep data (multiple samples per pixel).
//! Deep data uses the block-level API for reading, as it doesn't fit the line-based model
//! used for flat data reading.
//!
//! ## Example
//!
//! ```no_run
//! use exr::prelude::*;
//! use exr::image::read::deep::read_deep_from_file;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Read all deep blocks from a file
//! let blocks = read_deep_from_file("deep.exr", false)?;
//!
//! for block in blocks {
//!     println!("Block at {:?}", block.index.pixel_position);
//!     println!("  Pixel count: {}", block.pixel_offset_table.len());
//!     let total_samples = block.pixel_offset_table.last().unwrap_or(&0);
//!     println!("  Total samples: {}", total_samples);
//! }
//! # Ok(())
//! # }
//! ```

use crate::block::chunk::TileCoordinates;
use crate::block::lines::LineRef;
use crate::error::{Result, UnitResult};
use crate::image::deep_samples::DeepSamples;
use crate::image::read::any_channels::{ReadSamples, SamplesReader};
use crate::image::read::levels::ReadSamplesLevel;
use crate::image::read::samples::ReadDeepSamples;
use crate::math::Vec2;
use crate::meta::attribute::{ChannelDescription, SampleType};
use crate::meta::header::Header;

/// Processes pixel blocks from a file and accumulates them into deep sample storage.
///
/// This is a placeholder implementation. Full deep data reading requires integration
/// with the block decompression pipeline. For now, use the block-level API directly:
/// `block::read()` and `UncompressedDeepBlock::decompress_chunk()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepSamplesReader {
    level: Vec2<usize>,
    resolution: Vec2<usize>,
    #[allow(dead_code)]
    channel_type: SampleType,
}

impl ReadSamples for ReadDeepSamples {
    type Reader = DeepSamplesReader;

    fn create_sample_reader(
        &self,
        header: &Header,
        channel: &ChannelDescription,
    ) -> Result<Self::Reader> {
        self.create_samples_level_reader(header, channel, Vec2(0, 0), header.layer_size)
    }
}

impl ReadSamplesLevel for ReadDeepSamples {
    type Reader = DeepSamplesReader;

    fn create_samples_level_reader(
        &self,
        _header: &Header,
        channel: &ChannelDescription,
        level: Vec2<usize>,
        resolution: Vec2<usize>,
    ) -> Result<Self::Reader> {
        Ok(DeepSamplesReader {
            level,
            resolution,
            channel_type: channel.sample_type,
        })
    }
}

impl SamplesReader for DeepSamplesReader {
    type Samples = DeepSamples;

    fn filter_block(&self, tile: TileCoordinates) -> bool {
        tile.level_index == self.level
    }

    fn read_line(&mut self, _line: LineRef<'_>) -> UnitResult {
        // Deep data doesn't use the line-based reading approach
        // This will be implemented when we integrate with the block decompression pipeline
        unimplemented!(
            "Deep data line-based reading not yet implemented. \
             Use block-level API: block::read() and UncompressedDeepBlock::decompress_chunk()"
        )
    }

    fn into_samples(self) -> DeepSamples {
        // This will return accumulated deep samples
        // For now, return empty structure
        unimplemented!(
            "Deep data accumulation not yet implemented. \
             Use block-level API: block::read() and UncompressedDeepBlock::decompress_chunk()"
        )
    }
}

/// Helper to check if any header in the file contains deep data.
pub fn has_deep_data(headers: &[Header]) -> bool {
    headers.iter().any(|h| h.deep)
}

/// Read all deep blocks from an EXR file.
///
/// This function reads deep data using the block-level API. It returns a vector of
/// `UncompressedDeepBlock` instances, one for each block in the image.
///
/// ## Arguments
///
/// * `path` - Path to the EXR file
/// * `pedantic` - Whether to use strict error checking
///
/// ## Returns
///
/// A vector of `UncompressedDeepBlock` instances containing the deep pixel data.
/// Each block includes:
/// - `index`: Block position and size information
/// - `pixel_offset_table`: Cumulative sample counts per pixel
/// - `sample_data`: All samples for all channels
///
/// ## Example
///
/// ```no_run
/// use exr::prelude::*;
/// use exr::image::read::deep::read_deep_from_file;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let blocks = read_deep_from_file("deep.exr", false)?;
///
/// for block in &blocks {
///     println!("Block at position {:?}, size {:?}",
///              block.index.pixel_position, block.index.pixel_size);
///
///     // Access pixel data
///     for (pixel_idx, &cumulative_samples) in block.pixel_offset_table.iter().enumerate() {
///         let prev_count = if pixel_idx == 0 { 0 } else { block.pixel_offset_table[pixel_idx - 1] };
///         let sample_count = cumulative_samples - prev_count;
///         println!("  Pixel {} has {} samples", pixel_idx, sample_count);
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub fn read_deep_from_file(
    path: impl AsRef<std::path::Path>,
    pedantic: bool,
) -> Result<Vec<crate::block::UncompressedDeepBlock>> {
    use crate::block;

    let file = std::fs::File::open(path)?;
    let reader = block::read(file, pedantic)?;
    let meta = reader.meta_data().clone();
    let chunks = reader.all_chunks(pedantic)?;

    let mut blocks = Vec::new();

    for chunk_result in chunks {
        let chunk = chunk_result?;
        let deep_block =
            crate::block::UncompressedDeepBlock::decompress_chunk(chunk, &meta, pedantic)?;
        blocks.push(deep_block);
    }

    Ok(blocks)
}
