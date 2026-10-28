//! Write deep images.
//!
//! Deep layers use [`DeepChannels`] as their channel data,
//! and are written using the same API as flat images.
//! A single image may contain both flat and deep layers.
//!
//! ```no_run
//! use exr::{image::deep::DeepChannels, prelude::*};
//!
//! // a 2x1 deep image: the first pixel contains two samples, the second pixel none
//! let channels = DeepChannels::new(
//!     (2, 1),
//!     vec![2, 0],
//!     vec![
//!         AnyChannel::new("Z", FlatSamples::F32(vec![1.0, 2.5])),
//!         AnyChannel::new("A", FlatSamples::F16(vec![f16::from_f32(0.5), f16::ONE])),
//!     ],
//! )?;
//!
//! let encoding = Encoding {
//!     compression: Compression::ZIP1,
//!     ..Encoding::default()
//! };
//!
//! let image = Image::from_encoded_channels((2, 1), encoding, channels);
//! image.write().to_file("deep.exr")?;
//! # Ok::<(), exr::error::Error>(())
//! ```
//!
//! Deep layers support scan line blocks and tiles, but no resolution levels.
//! Only the compression methods `Uncompressed`, `RLE`, and `ZIP1` are
//! supported for deep data. The samples are written in the order they are
//! stored, and no sorting is performed.

use crate::{
    block::{writer::ChunksWriter, BlockIndex, UncompressedBlock, UncompressedDeepBlock},
    error::{Error, Result, UnitResult},
    image::{
        deep::DeepChannels,
        write::{
            channels::{ChannelsWriter, WritableChannels},
            layers::LayersWriter,
        },
    },
    math::RoundingMode,
    meta::{
        attribute::{ChannelList, LevelMode},
        header::Header,
        MetaData,
    },
};

/// A temporary writer for the channels of a deep layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeepChannelsWriter<'channels> {
    channels: &'channels DeepChannels,
}

impl<'channels> WritableChannels<'channels> for DeepChannels {
    type Writer = DeepChannelsWriter<'channels>;

    fn infer_channel_list(&self) -> ChannelList {
        self.channel_list()
    }

    fn infer_level_modes(&self) -> (LevelMode, RoundingMode) {
        (LevelMode::Singular, RoundingMode::Down)
    }

    fn create_writer(&'channels self, _header: &Header) -> Self::Writer {
        DeepChannelsWriter {
            channels: self,
        }
    }

    fn infer_deep_max_samples_per_pixel(&self) -> Option<usize> {
        Some(self.max_samples_per_pixel())
    }
}

impl ChannelsWriter for DeepChannelsWriter<'_> {
    /// Deep channels cannot be written as flat data.
    /// Returns an empty block, which will be rejected when compressing it.
    fn extract_uncompressed_block(&self, _header: &Header, _block: BlockIndex) -> Vec<u8> {
        Vec::new()
    }

    fn extract_uncompressed_deep_block(
        &self,
        header: &Header,
        block: BlockIndex,
    ) -> Result<UncompressedDeepBlock> {
        if header.layer_size != self.channels.resolution() {
            return Err(Error::invalid(format!(
                "deep layer size {:?} does not match the resolution of its channels {:?}",
                header.layer_size,
                self.channels.resolution()
            )));
        }

        if block.level != crate::math::Vec2(0, 0) {
            return Err(Error::unsupported("resolution levels in deep layers"));
        }

        // the sample offsets are validated when constructing the channels,
        // but the channels may have been modified afterwards
        self.channels.validate_channels()?;
        self.channels.extract_block(block)
    }
}

/// Extract all blocks of all layers, flat and deep, then compress them, and
/// write them to the file in the required order.
pub(crate) fn write_flat_and_deep_blocks(
    meta: &MetaData,
    layers: &impl LayersWriter,
    mut chunk_writer: impl ChunksWriter,
    parallel: bool,
) -> UnitResult {
    let extract_flat_block = |block_index: BlockIndex| UncompressedBlock {
        index: block_index,
        data: layers.extract_uncompressed_block(&meta.headers, block_index),
    };

    #[cfg(feature = "rayon")]
    if parallel {
        if let Some(mut compressor) = chunk_writer.parallel_blocks_compressor(meta) {
            for (index_in_header, block_index) in meta.enumerate_ordered_header_block_indices() {
                if meta.headers[block_index.layer].deep {
                    let block =
                        layers.extract_uncompressed_deep_block(&meta.headers, block_index)?;
                    compressor.add_block_to_compression_queue(index_in_header, block)?;
                } else {
                    let block = extract_flat_block(block_index);
                    compressor.add_block_to_compression_queue(index_in_header, block)?;
                }
            }

            return Ok(());
        }
    }

    #[cfg(not(feature = "rayon"))]
    if parallel {
        return Err(Error::unsupported("parallel compression requires the rayon feature"));
    }

    let mut compressor = chunk_writer.sequential_blocks_compressor(meta);

    for (index_in_header, block_index) in meta.enumerate_ordered_header_block_indices() {
        if meta.headers[block_index.layer].deep {
            let block = layers.extract_uncompressed_deep_block(&meta.headers, block_index)?;
            compressor.compress_block(index_in_header, block)?;
        } else {
            compressor.compress_block(index_in_header, extract_flat_block(block_index))?;
        }
    }

    Ok(())
}
