//! How to write either a single or a list of layers.

use crate::meta::header::{ImageAttributes, Header};
use crate::meta::{Headers, compute_chunk_count};
use crate::block::BlockIndex;
use crate::image::{Layers, Layer};
use crate::meta::attribute::{TileDescription};
use crate::prelude::{SmallVec};
use crate::image::write::channels::{WritableChannels, ChannelsWriter};

/// Enables an image containing this list of layers to be written to a file.
pub trait WritableLayers<'slf> {

    /// Generate the file meta data for this list of layers
    fn infer_headers(&self, image_attributes: &ImageAttributes) -> Headers;

    /// The type of temporary writer
    type Writer: LayersWriter;

    /// Create a temporary writer for this list of layers
    fn create_writer(&'slf self, headers: &[Header]) -> Self::Writer;
}

/// A temporary writer for a list of channels
pub trait LayersWriter: Sync {

    /// Deliver a block of pixels from a single layer to be stored in the file
    fn extract_uncompressed_block(&self, headers: &[Header], block: BlockIndex) -> Vec<u8>;
}

/// A temporary writer for an arbitrary list of layers
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AllLayersWriter<ChannelsWriter> {
    layers: SmallVec<[LayerWriter<ChannelsWriter>; 2]>
}

/// A temporary writer for a single layer
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LayerWriter</*'a,*/ ChannelsWriter> {
    channels: ChannelsWriter, // impl ChannelsWriter
}



impl<'slf, Channels: 'slf> WritableLayers<'slf> for Layers<Channels> where Channels: WritableChannels<'slf> {
    fn infer_headers(&self, image_attributes: &ImageAttributes) -> Headers {
        self.iter().map(|layer| layer.infer_headers(image_attributes).remove(0)).collect() // TODO no array-vs-first
    }

    type Writer = AllLayersWriter<Channels::Writer>;
    fn create_writer(&'slf self, headers: &[Header]) -> Self::Writer {
        AllLayersWriter {
            layers: self.iter().zip(headers.chunks_exact(1)) // TODO no array-vs-first
                .map(|(layer, header)| layer.create_writer(header))
                .collect()
        }
    }
}

impl<'slf, Channels: WritableChannels<'slf>> WritableLayers<'slf> for Layer<Channels> {
    fn infer_headers(&self, image_attributes: &ImageAttributes) -> Headers {
        let blocks = match self.encoding.blocks {
            crate::image::Blocks::ScanLines => crate::meta::Blocks::ScanLines,
            crate::image::Blocks::Tiles(tile_size) => {
                let (level_mode, rounding_mode) = self.channel_data.infer_level_modes();
                crate::meta::Blocks::Tiles(TileDescription { level_mode, rounding_mode, tile_size, })
            },
        };

        let chunk_count = compute_chunk_count(
            self.encoding.compression, self.size, blocks
        );

        let header = Header {
            channels: self.channel_data.infer_channel_list(),
            compression: self.encoding.compression,

            blocks,
            chunk_count,

            line_order: self.encoding.line_order,
            layer_size: self.size,
            shared_attributes: image_attributes.clone(),
            own_attributes: self.attributes.clone(),


            deep: false, // TODO deep data
            deep_data_version: None,
            max_samples_per_pixel: None,
        };

        smallvec![ header ]// TODO no array-vs-first
    }

    type Writer = LayerWriter</*'l,*/ Channels::Writer>;
    fn create_writer(&'slf self, headers: &[Header]) -> Self::Writer {
        let channels = self.channel_data
            .create_writer(headers.first().unwrap()); // TODO no array-vs-first

        LayerWriter { channels }
    }
}

impl<C> LayersWriter for AllLayersWriter<C> where C: ChannelsWriter {
    fn extract_uncompressed_block(&self, headers: &[Header], block: BlockIndex) -> Vec<u8> {
        self.layers[block.layer].extract_uncompressed_block(std::slice::from_ref(&headers[block.layer]), block) // TODO no array-vs-first
    }
}

impl<C> LayersWriter for LayerWriter<C> where C: ChannelsWriter {
    fn extract_uncompressed_block(&self, headers: &[Header], block: BlockIndex) -> Vec<u8> {
        self.channels.extract_uncompressed_block(headers.first().unwrap(), block) // TODO no array-vs-first
    }
}
