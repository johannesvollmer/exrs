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
//! Only the largest resolution level of deep layers is loaded.
//! The samples are returned in the order in which they are stored in the file.
//! See [`crate::image::deep::compositing`] for sorting and flattening the
//! samples.
//!
//! This module requires the `deep` feature.

use std::{
    io::{BufReader, Read, Seek},
    path::Path,
};

use smallvec::SmallVec;

use crate::{
    block::{reader::ChunksReader, UncompressedDeepBlock},
    error::{Error, Result},
    image::{
        deep::DeepChannels, ignore_progress, Blocks, DeepImage, DeepLayersImage, Encoding, Image,
        Layer, Layers,
    },
    meta::{header::Header, BlockDescription, MetaData},
};

/// Read the first deep layer of an image file, including all of its channels.
/// Uses parallel decompression and relaxed error handling.
/// Ignores flat layers. Fails if the image contains no deep layer.
pub fn read_first_deep_layer_from_file(path: impl AsRef<Path>) -> Result<DeepImage> {
    ReadDeepSamples.first_valid_layer().from_file(path)
}

/// Read all layers of an image file, which must all be deep layers.
/// Uses parallel decompression and relaxed error handling.
pub fn read_all_deep_layers_from_file(path: impl AsRef<Path>) -> Result<DeepLayersImage> {
    ReadDeepSamples.all_layers().from_file(path)
}

/// Specify to read deep data, with all channels.
/// Create this using `read().deep_data()`.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct ReadDeepSamples;

impl ReadDeepSamples {
    /// Read all layers of the image, which must all be deep layers.
    /// Returns an error if the image contains a flat layer.
    #[must_use]
    pub fn all_layers(self) -> ReadDeepImage<ReadAllDeepLayers, fn(f64)> {
        ReadDeepImage::new(ReadAllDeepLayers)
    }

    /// Read only the first deep layer of the image, ignoring flat layers.
    /// Returns an error if the image contains no deep layer.
    #[must_use]
    pub fn first_valid_layer(self) -> ReadDeepImage<ReadFirstValidDeepLayer, fn(f64)> {
        ReadDeepImage::new(ReadFirstValidDeepLayer)
    }
}

/// Specifies which deep layers to read from an image,
/// and how to return them.
pub trait ReadDeepLayers {
    /// The resulting layer data, for example a single `Layer` or `Layers`.
    type Layers;

    /// Select the indices of the headers that should be loaded.
    fn select_layers(&self, headers: &[Header]) -> Result<SmallVec<[usize; 2]>>;

    /// Collect the selected layers, in the order that they were selected.
    fn collect_layers(&self, layers: Vec<Layer<DeepChannels>>) -> Self::Layers;
}

/// Read all layers of an image, which must all be deep layers.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct ReadAllDeepLayers;

/// Read only the first deep layer of an image, ignoring flat layers.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct ReadFirstValidDeepLayer;

impl ReadDeepLayers for ReadAllDeepLayers {
    type Layers = Layers<DeepChannels>;

    fn select_layers(&self, headers: &[Header]) -> Result<SmallVec<[usize; 2]>> {
        if headers.iter().any(|header| !header.deep) {
            return Err(Error::unsupported(
                "flat layers cannot be read as deep data (use `first_valid_layer()` to skip them)",
            ));
        }

        Ok((0..headers.len()).collect())
    }

    fn collect_layers(&self, layers: Vec<Layer<DeepChannels>>) -> Self::Layers {
        layers.into_iter().collect()
    }
}

impl ReadDeepLayers for ReadFirstValidDeepLayer {
    type Layers = Layer<DeepChannels>;

    fn select_layers(&self, headers: &[Header]) -> Result<SmallVec<[usize; 2]>> {
        headers
            .iter()
            .position(|header| header.deep)
            .map(|index| SmallVec::from_slice(&[index]))
            .ok_or_else(|| Error::unsupported("the image contains no deep layer"))
    }

    fn collect_layers(&self, layers: Vec<Layer<DeepChannels>>) -> Self::Layers {
        layers.into_iter().next().expect("deep layer selection bug")
    }
}

/// Specify whether to read the image in parallel,
/// whether to use pedantic error handling,
/// and a callback for the reading progress.
/// Finally, read the image using `from_file` or similar methods.
#[derive(Debug, Clone)]
pub struct ReadDeepImage<ReadLayers, OnProgress> {
    read_layers: ReadLayers,
    on_progress: OnProgress,
    pedantic: bool,
    parallel: bool,
}

impl<L> ReadDeepImage<L, fn(f64)> {
    /// Uses relaxed error handling and parallel decompression.
    fn new(read_layers: L) -> Self {
        Self {
            read_layers,
            on_progress: ignore_progress,
            pedantic: false,
            parallel: cfg!(feature = "rayon"),
        }
    }
}

impl<L, F> ReadDeepImage<L, F>
where
    L: ReadDeepLayers,
    F: FnMut(f64),
{
    /// Specify that any missing or unusual information should result in an
    /// error. Otherwise, `exrs` will try to compute or ignore missing
    /// information. See `ReadImage::pedantic` for more information.
    #[must_use]
    pub fn pedantic(self) -> Self {
        Self {
            pedantic: true,
            ..self
        }
    }

    /// Specify that multiple pixel blocks should never be decompressed using
    /// multiple threads at once. This might be slower but uses less memory
    /// and less synchronization.
    #[must_use]
    pub fn non_parallel(self) -> Self {
        Self {
            parallel: false,
            ..self
        }
    }

    /// Specify a function to be called regularly throughout the loading
    /// process. Replaces all previously specified progress functions in
    /// this reader.
    pub fn on_progress<OnProgress>(self, on_progress: OnProgress) -> ReadDeepImage<L, OnProgress>
    where
        OnProgress: FnMut(f64),
    {
        ReadDeepImage {
            on_progress,
            read_layers: self.read_layers,
            pedantic: self.pedantic,
            parallel: self.parallel,
        }
    }

    /// Read the deep exr image from a file.
    #[inline]
    pub fn from_file(self, path: impl AsRef<Path>) -> Result<Image<L::Layers>> {
        self.from_unbuffered(std::fs::File::open(path)?)
    }

    /// Buffer the reader and then read the deep exr image from it.
    /// Use `from_buffered` instead, if your reader is an in-memory reader.
    #[inline]
    pub fn from_unbuffered(self, unbuffered: impl Read + Seek) -> Result<Image<L::Layers>> {
        self.from_buffered(BufReader::new(unbuffered))
    }

    /// Read the deep exr image from a buffered reader.
    pub fn from_buffered(self, buffered: impl Read + Seek) -> Result<Image<L::Layers>> {
        let chunks = crate::block::read(buffered, self.pedantic)?;
        self.from_chunks(chunks)
    }

    /// Read the deep exr image from an initialized chunks reader
    /// that has already extracted the meta data from the file.
    pub fn from_chunks(
        self,
        chunks_reader: crate::block::reader::Reader<impl Read + Seek>,
    ) -> Result<Image<L::Layers>> {
        let Self {
            read_layers,
            on_progress,
            pedantic,
            parallel,
        } = self;

        let headers: SmallVec<[Header; 3]> = chunks_reader.headers().iter().cloned().collect();
        let selected_layers = read_layers.select_layers(&headers)?;

        let mut is_selected = vec![false; headers.len()];
        for &layer_index in &selected_layers {
            is_selected[layer_index] = true;
        }

        // only the largest resolution level of the selected layers is loaded
        let chunks = chunks_reader
            .filter_chunks(pedantic, |_, tile, block| {
                is_selected[block.layer] && tile.is_largest_resolution_level()
            })?
            .on_progress(on_progress);

        let mut blocks_per_layer: Vec<Vec<UncompressedDeepBlock>> = vec![Vec::new(); headers.len()];
        let insert_block = |_: &MetaData, block: UncompressedDeepBlock| {
            blocks_per_layer[block.index.layer].push(block);
            Ok(())
        };

        if parallel {
            #[cfg(not(feature = "rayon"))]
            return Err(Error::unsupported("parallel decompression requires the rayon feature"));

            #[cfg(feature = "rayon")]
            chunks.decompress_deep_parallel(pedantic, insert_block)?;
        } else {
            chunks.decompress_deep_sequential(pedantic, insert_block)?;
        }

        let layers: Result<Vec<Layer<DeepChannels>>> = selected_layers
            .iter()
            .map(|&layer_index| {
                let header = &headers[layer_index];
                let blocks = std::mem::take(&mut blocks_per_layer[layer_index]);
                let channel_data =
                    DeepChannels::from_blocks(header.layer_size, &header.channels, blocks)?;

                Ok(Layer {
                    channel_data,
                    attributes: header.own_attributes.clone(),
                    size: header.layer_size,
                    encoding: Encoding {
                        compression: header.compression,
                        line_order: header.line_order,
                        blocks: match header.blocks {
                            BlockDescription::ScanLines => Blocks::ScanLines,
                            BlockDescription::Tiles(tiles) => Blocks::Tiles(tiles.tile_size),
                        },
                    },
                })
            })
            .collect();

        Ok(Image {
            attributes: headers
                .first()
                .expect("header count validation bug")
                .shared_attributes
                .clone(),
            layer_data: read_layers.collect_layers(layers?),
        })
    }
}
