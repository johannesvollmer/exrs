//! Read deep images.
//!
//! Start with `read().deep_data()`, then choose which layers to read,
//! and finally read the image from a file or a byte buffer:
//!
//! ```no_run
//! use exr::prelude::*;
//!
//! let image: DeepLayersImage = read()
//!     .deep_data()
//!     .all_layers()
//!     .all_attributes()
//!     .on_progress(|progress| println!("progress: {:.1}%", progress * 100.0))
//!     .from_file("deep.exr")?;
//!
//! for layer in &image.layer_data {
//!     let deep = &layer.channel_data;
//!     println!("{} samples in {} pixels", deep.total_sample_count(), deep.pixel_count());
//! }
//! # Ok::<(), exr::error::Error>(())
//! ```
//!
//! The deep layer selections implement the same `ReadLayers` trait as the
//! flat layer selections, so that deep images are read with the same
//! `ReadImage` options, such as `pedantic()` and `non_parallel()`.
//!
//! Only the largest resolution level of deep layers is loaded.
//! The samples are returned in the order in which they are stored in the file.

use std::path::Path;

use smallvec::SmallVec;

use crate::{
    block::{chunk::TileCoordinates, BlockIndex, UncompressedBlock},
    error::{Error, Result, UnitResult},
    image::{
        deep::{AnyDeepChannels, DeepBlocksCollector},
        read::image::{LayersReader, ReadLayers},
        Blocks, DeepImage, DeepLayersImage, Encoding, Layer, Layers,
    },
    meta::{
        header::{Header, LayerAttributes},
        BlockDescription, MetaData,
    },
};

/// Read the first deep layer of an image file, including all of its channels.
/// Uses parallel decompression and relaxed error handling.
/// Ignores flat layers. Fails if the image contains no deep layer.
pub fn read_first_deep_layer_from_file(path: impl AsRef<Path>) -> Result<DeepImage> {
    ReadDeepSamples.first_valid_layer().all_attributes().from_file(path)
}

/// Read all layers of an image file, which must all be deep layers.
/// Uses parallel decompression and relaxed error handling.
pub fn read_all_deep_layers_from_file(path: impl AsRef<Path>) -> Result<DeepLayersImage> {
    ReadDeepSamples.all_layers().all_attributes().from_file(path)
}

/// Specify to read deep data, with all channels.
/// Create this using `read().deep_data()`.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct ReadDeepSamples;

impl ReadDeepSamples {
    /// Read all layers of the image, which must all be deep layers.
    /// Returns an error if the image contains a flat layer.
    /// Use `all_valid_layers()` to skip flat layers instead.
    #[must_use]
    pub const fn all_layers(self) -> ReadAllDeepLayers {
        ReadAllDeepLayers
    }

    /// Read all deep layers of the image, ignoring flat layers.
    /// Returns an error if the image contains no deep layer.
    #[must_use]
    pub const fn all_valid_layers(self) -> ReadAllValidDeepLayers {
        ReadAllValidDeepLayers
    }

    /// Read only the first deep layer of the image, ignoring flat layers.
    /// Returns an error if the image contains no deep layer.
    #[must_use]
    pub const fn first_valid_layer(self) -> ReadFirstValidDeepLayer {
        ReadFirstValidDeepLayer
    }
}

/// Read all layers of an image, which must all be deep layers.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct ReadAllDeepLayers;

/// Read all deep layers of an image, ignoring flat layers.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct ReadAllValidDeepLayers;

/// Read only the first deep layer of an image, ignoring flat layers.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct ReadFirstValidDeepLayer;

impl ReadLayers<'_> for ReadAllDeepLayers {
    type Layers = Layers<AnyDeepChannels>;
    type Reader = AllDeepLayersReader;

    fn create_layers_reader(&self, headers: &[Header]) -> Result<Self::Reader> {
        if headers.iter().any(|header| !header.deep) {
            return Err(Error::invalid(
                "expected deep data but found a flat layer (use `all_valid_layers()` to read \
                 only the deep layers)",
            ));
        }

        AllDeepLayersReader::new(headers, |_| true)
    }
}

impl ReadLayers<'_> for ReadAllValidDeepLayers {
    type Layers = Layers<AnyDeepChannels>;
    type Reader = AllDeepLayersReader;

    fn create_layers_reader(&self, headers: &[Header]) -> Result<Self::Reader> {
        if !headers.iter().any(|header| header.deep) {
            return Err(Error::invalid("expected deep data but the image contains no deep layer"));
        }

        AllDeepLayersReader::new(headers, |header| header.deep)
    }
}

impl ReadLayers<'_> for ReadFirstValidDeepLayer {
    type Layers = Layer<AnyDeepChannels>;
    type Reader = FirstValidDeepLayerReader;

    fn create_layers_reader(&self, headers: &[Header]) -> Result<Self::Reader> {
        FirstValidDeepLayerReader::new(headers, |header| header.deep)
    }
}

/// Collects the blocks of all selected deep layers of an image.
#[derive(Debug, Clone, PartialEq)]
pub struct AllDeepLayersReader {
    /// One entry for each header, `None` if the layer is not selected.
    layers: SmallVec<[Option<DeepLayerReader>; 2]>,
}

/// Collects the blocks of the first deep layer of an image.
#[derive(Debug, Clone, PartialEq)]
pub struct FirstValidDeepLayerReader {
    layers: AllDeepLayersReader,
}

/// Collects the blocks of a single deep layer.
#[derive(Debug, Clone, PartialEq)]
struct DeepLayerReader {
    blocks: DeepBlocksCollector,
    attributes: LayerAttributes,
    encoding: Encoding,
}

impl FirstValidDeepLayerReader {
    /// Prepare reading the first deep layer for which `is_selected` returns
    /// true. Use this to implement `ReadLayers` for your own layer selection.
    pub fn new(headers: &[Header], is_selected: impl Fn(&Header) -> bool) -> Result<Self> {
        let first_selected_layer = headers
            .iter()
            .position(|header| header.deep && is_selected(header))
            .ok_or_else(|| Error::invalid("no deep layer matched the specified requirements"))?;

        Ok(Self {
            layers: AllDeepLayersReader::new(headers, |header| {
                std::ptr::eq(header, &headers[first_selected_layer])
            })?,
        })
    }
}

impl AllDeepLayersReader {
    /// Prepare reading all deep layers for which `is_selected` returns true.
    /// Flat layers are never selected.
    /// Use this to implement `ReadLayers` for your own layer selection.
    pub fn new(headers: &[Header], is_selected: impl Fn(&Header) -> bool) -> Result<Self> {
        let layers = headers
            .iter()
            .map(|header| {
                if !header.deep || !is_selected(header) {
                    return Ok(None);
                }

                Ok(Some(DeepLayerReader {
                    blocks: DeepBlocksCollector::new(
                        header.layer_size,
                        header.max_block_pixel_size(),
                        header.channels.clone(),
                    )?,
                    attributes: header.own_attributes.clone(),
                    encoding: Encoding {
                        compression: header.compression,
                        line_order: header.line_order,
                        blocks: match header.blocks {
                            BlockDescription::ScanLines => Blocks::ScanLines,
                            BlockDescription::Tiles(tiles) => Blocks::Tiles(tiles.tile_size),
                        },
                    },
                }))
            })
            .collect::<Result<_>>()?;

        Ok(Self {
            layers,
        })
    }
}

impl LayersReader for AllDeepLayersReader {
    type Layers = Layers<AnyDeepChannels>;

    fn filter_block(&self, _: &MetaData, tile: TileCoordinates, block: BlockIndex) -> bool {
        // only the largest resolution level of the selected layers is loaded
        tile.is_largest_resolution_level()
            && self.layers.get(block.layer).is_some_and(Option::is_some)
    }

    fn read_block(&mut self, _: &[Header], block: UncompressedBlock) -> UnitResult {
        let block = block.into_deep()?;

        self.layers
            .get_mut(block.index.layer)
            .and_then(Option::as_mut)
            .ok_or_else(|| Error::invalid("unexpected deep block"))?
            .blocks
            .insert_block(block)
    }

    /// Assemble the selected layers, in the order of the headers.
    fn into_layers(self) -> Self::Layers {
        self.layers
            .into_iter()
            .flatten()
            .map(|layer| {
                let channel_data = layer.blocks.into_channels();
                Layer {
                    size: channel_data.resolution(),
                    channel_data,
                    attributes: layer.attributes,
                    encoding: layer.encoding,
                }
            })
            .collect()
    }
}

impl LayersReader for FirstValidDeepLayerReader {
    type Layers = Layer<AnyDeepChannels>;

    fn filter_block(&self, meta: &MetaData, tile: TileCoordinates, block: BlockIndex) -> bool {
        self.layers.filter_block(meta, tile, block)
    }

    fn read_block(&mut self, headers: &[Header], block: UncompressedBlock) -> UnitResult {
        self.layers.read_block(headers, block)
    }

    fn into_layers(self) -> Self::Layers {
        self.layers.into_layers().into_iter().next().expect("deep layer selection bug")
    }
}
