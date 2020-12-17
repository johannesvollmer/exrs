use crate::image::*;
use crate::meta::header::{Header, ImageAttributes};
use crate::error::{Result, UnitResult};
use crate::block::UncompressedBlock;
use crate::block::chunk::TileCoordinates;
use std::path::Path;
use std::io::{Read, BufReader};
use std::io::Seek;


#[derive(Debug, Clone)]
pub struct ReadImage<F, L> {
    on_progress: F,
    read_layers: L,
    pedantic: bool,
    parallel: bool,
}

impl<F, L> ReadImage<F, L> where F: FnMut(f64)
{
    pub fn new(read_layers: L, on_progress: F) -> Self {
        Self {
            on_progress,
            pedantic: false,
            parallel: true,
            read_layers,
        }
    }

    pub fn pedantic(self) -> Self { Self { pedantic: true, ..self } }
    pub fn non_parallel(self) -> Self { Self { parallel: false, ..self } }
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

        let reader: ImageWithAttributesReader<L::Reader> = crate::block::read_filtered_blocks_from_buffered(
            buffered,

            move |headers| {// Self::create_image_reader(read_layers, headers),
                Ok(ImageWithAttributesReader {
                    image_attributes: headers.first().expect("invalid headers").shared_attributes.clone(),
                    layers_reader: read_layers.create_layers_reader(headers)?,
                })
            },

            |reader, header, (tile_index, tile)| {
                reader.filter_block(header, (tile_index, &tile.location)) // TODO pass TileIndices directly!
            },

            |reader, headers, block| {
                reader.read_block(headers, block)
            },

            |progress| on_progress(progress),
            pedantic, parallel
        )?;

        Ok(reader.into_image())
    }
}

// TODO eliminate this intermediate private data?
#[derive(Debug, Clone, PartialEq)]
pub struct ImageWithAttributesReader<L> {
    image_attributes: ImageAttributes,
    layers_reader: L,
}


pub trait ReadLayers<'s> {
    type Layers;
    type Reader: LayersReader<Layers = Self::Layers>;
    fn create_layers_reader(&'s self, headers: &[Header]) -> Result<Self::Reader>;

    fn all_attributes(self) -> ReadImage<fn(f64), Self> where Self: Sized {
        ReadImage::new(self, ignore_progress)
    }
}

pub trait LayersReader {
    type Layers;
    fn filter_block(&self, header: (usize, &Header), tile: (usize, &TileCoordinates)) -> bool;
    fn read_block(&mut self, headers: &[Header], block: UncompressedBlock) -> UnitResult;
    fn into_layers(self) -> Self::Layers;
}


/*
/// enable calling `from_file` directly on any layer reader
impl<'s, R:'s> ReadImage<'s> for R where R: ReadLayers<'s> {
    type Reader = ImageWithAttributesReader<R::Reader>;

    fn create_image_reader(&'s mut self, headers: &[Header]) -> Result<Self::Reader> {
        Ok(ImageWithAttributesReader {
            image_attributes: headers.first().expect("invalid headers").shared_attributes.clone(),
            layers_reader: self.create_layers_reader(headers)?,
        })
    }
}*/

/*impl<'s, L: 's> ReadImage<'s> for ReadImageWithAttributes<L> where L: ReadLayers<'s> {
    type Reader = ImageWithAttributesReader<L::Reader>;

    fn create_image_reader(&'s self, headers: &[Header]) -> Result<ImageWithAttributesReader<L::Reader>> {
        Ok(ImageWithAttributesReader {
            image_attributes: headers.first().expect("invalid headers").shared_attributes.clone(),
            layers_reader: self.read_layers.create_layers_reader(headers)?,
        })
    }
}*/

// TODO eliminate this intermediate private data?
impl<L> ImageWithAttributesReader<L> where L: LayersReader {

    fn filter_block(&self, header: (usize, &Header), tile: (usize, &TileCoordinates)) -> bool {
        self.layers_reader.filter_block(header, tile)
    }

    fn read_block(&mut self, headers: &[Header], block: UncompressedBlock) -> UnitResult {
        self.layers_reader.read_block(headers, block)
    }

    fn into_image(self) -> Image<L::Layers> {
        Image {
            attributes: self.image_attributes,
            layer_data: self.layers_reader.into_layers()
        }
    }
}

