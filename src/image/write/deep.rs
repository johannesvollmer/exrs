//! Write deep images.
//!
//! Deep layers use [`AnyDeepChannels`] as their channel data,
//! and are written using the same API as flat images.
//! A single image may contain both flat and deep layers.
//!
//! ```no_run
//! use exr::{image::deep::AnyDeepChannels, prelude::*};
//!
//! // a 2x1 deep image: the first pixel contains two samples, the second pixel none
//! let channels = AnyDeepChannels::new(
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
    block::{BlockIndex, UncompressedBlock},
    error::{Error, Result},
    image::{
        deep::AnyDeepChannels,
        write::channels::{ChannelsWriter, WritableChannels},
    },
    math::RoundingMode,
    meta::{
        attribute::{ChannelList, LevelMode},
        header::Header,
    },
};

/// A temporary writer for the channels of a deep layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnyDeepChannelsWriter<'channels> {
    channels: &'channels AnyDeepChannels,
}

impl<'channels> WritableChannels<'channels> for AnyDeepChannels {
    type Writer = AnyDeepChannelsWriter<'channels>;

    fn infer_channel_list(&self) -> ChannelList {
        self.to_channel_list()
    }

    fn infer_level_modes(&self) -> (LevelMode, RoundingMode) {
        (LevelMode::Singular, RoundingMode::Down)
    }

    fn create_writer(&'channels self, _header: &Header) -> Self::Writer {
        AnyDeepChannelsWriter {
            channels: self,
        }
    }

    fn infer_deep_max_samples_per_pixel(&self) -> Option<usize> {
        Some(self.max_samples_per_pixel())
    }
}

impl ChannelsWriter for AnyDeepChannelsWriter<'_> {
    fn extract_uncompressed_block(
        &self,
        header: &Header,
        block: BlockIndex,
    ) -> Result<UncompressedBlock> {
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
        Ok(self.channels.extract_block(block)?.into())
    }
}
