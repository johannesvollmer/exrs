//! Reading deep data from EXR files.
//!
//! This module provides infrastructure for reading deep data (multiple samples per pixel).
//! Deep data is currently read using the block-level API, with plans to integrate
//! into the high-level builder API in the future.

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
