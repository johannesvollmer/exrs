use crate::image::*;
use crate::meta::header::{Header, ImageAttributes};
use crate::error::{Result, UnitResult};
use crate::block::UncompressedBlock;
use crate::block::chunk::TileCoordinates;
use std::path::Path;
use std::io::{Read, BufReader};
use std::io::Seek;

/// Specify whether to read the image in parallel,
/// whether to use pedantic error handling,
/// and a callback for the reading progress.
#[derive(Debug, Clone)]
pub struct ReadImage<OnProgress, ReadLayers> {

    /// Do something after processing a piece of the file
    pub on_progress: OnProgress,

    /// The layer reading specification
    pub read_layers: ReadLayers,

    /// Immediately abort if some data is missing in the file
    pub pedantic: bool,

    /// Decompress pixels with multiple threads
    pub parallel: bool,
}

impl<F, L> ReadImage<F, L> where F: FnMut(f64)
{
    /// Uses relaxed error handling and parallel decompression.
    pub fn new(read_layers: L, on_progress: F) -> Self {
        Self {
            on_progress, read_layers,
            pedantic: false, parallel: true,
        }
    }

    /// Specify that any missing or unusual information should result in an error.
    /// Otherwise, `exrs` will try to compute or ignore missing information.
    pub fn pedantic(self) -> Self { Self { pedantic: true, ..self } }

    /// Specify that multiple pixel blocks should never be decompressed using multiple threads at once.
    /// This might be slower but uses less memory and less synchronization.
    pub fn non_parallel(self) -> Self { Self { parallel: false, ..self } }

    /// Specify a function to be called regularly throughout the loading process.
    /// Replaces all previously specified progress functions in this reader.
    pub fn on_progress(self, on_progress: F) -> Self where F: FnMut(f64) { Self { on_progress, ..self } }


    /// Read the exr image from a file.
    /// Use `read_from_unbuffered` instead, if you do not have a file.
    #[inline]
    #[must_use]
    pub fn from_file<Layers>(self, path: impl AsRef<Path>) -> Result<Image<Layers>>
        where for<'s> L: ReadLayers<'s, Layers = Layers>
    {
        self.from_unbuffered(std::fs::File::open(path)?)
    }

    /// Buffer the reader and then read the exr image from it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory reader.
    /// Use `read_from_file` instead, if you have a file path.
    #[inline]
    #[must_use]
    pub fn from_unbuffered<Layers>(self, unbuffered: impl Read + Seek + Send) -> Result<Image<Layers>>
        where for<'s> L: ReadLayers<'s, Layers = Layers>
    {
        self.from_buffered(BufReader::new(unbuffered))
    }

    /// Read the exr image from a buffered reader.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory reader.
    #[must_use]
    pub fn from_buffered<Layers>(mut self, buffered: impl Read + Seek + Send) -> Result<Image<Layers>>
        where for<'s> L: ReadLayers<'s, Layers = Layers>
    {
        let Self { pedantic, parallel, ref mut on_progress, ref mut read_layers } = self;

        #[derive(Debug, Clone, PartialEq)]
        pub struct ImageWithAttributesReader<L> {
            image_attributes: ImageAttributes,
            layers_reader: L,
        }

        let reader: ImageWithAttributesReader<L::Reader> = crate::block::read_filtered_blocks_from_buffered(
            buffered,

            move |headers| {// Self::create_image_reader(read_layers, headers),
                Ok(ImageWithAttributesReader {
                    image_attributes: headers.first().expect("invalid headers").shared_attributes.clone(),
                    layers_reader: read_layers.create_layers_reader(headers)?,
                })
            },

            |reader, header, (tile_index, tile)| {
                reader.layers_reader.filter_block(header, (tile_index, &tile.location)) // TODO pass TileIndices directly!
            },

            |reader, headers, block| {
                reader.layers_reader.read_block(headers, block)
            },

            |progress| on_progress(progress),
            pedantic, parallel
        )?;

        Ok(Image {
            attributes: reader.image_attributes,
            layer_data: reader.layers_reader.into_layers()
        })
    }
}

/// A template that creates a `LayerReader` for each layer in the file.
pub trait ReadLayers<'s> {

    /// The type of the resulting Layers
    type Layers;

    /// The type of the temporary layer reader
    type Reader: LayersReader<Layers = Self::Layers>;

    /// Create a single reader for a single layer
    fn create_layers_reader(&'s self, headers: &[Header]) -> Result<Self::Reader>;

    /// Specify that all attributes should be read from an image.
    /// Use `from_file(path)` on the return value of this method to actually decode an image.
    fn all_attributes(self) -> ReadImage<fn(f64), Self> where Self: Sized {
        ReadImage::new(self, ignore_progress)
    }
}

/// Processes pixel blocks from a file and accumulates them into a single image layer.
pub trait LayersReader {

    /// The type of resulting layers
    type Layers;

    /// Specify whether a single block of pixels should be loaded from the file
    fn filter_block(&self, header: (usize, &Header), tile: (usize, &TileCoordinates)) -> bool;

    /// Load a single pixel block, which has not been filtered, into the reader, accumulating the layer
    fn read_block(&mut self, headers: &[Header], block: UncompressedBlock) -> UnitResult;

    /// Deliver the final accumulated layers for the image
    fn into_layers(self) -> Self::Layers;
}

