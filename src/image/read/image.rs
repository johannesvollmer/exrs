use crate::image::*;
use crate::meta::header::{Header, ImageAttributes};
use crate::error::{Result, UnitResult};
use crate::block::UncompressedBlock;
use crate::image::read::{ReadImage, ImageReader};
use crate::block::chunk::TileCoordinates;



pub trait ReadLayers<'s> {
    type Reader: LayersReader;
    fn create_layers_reader(&'s self, headers: &[Header]) -> Result<Self::Reader>;
}

/// enable calling `from_file` directly on any layer reader
impl<'s, R:'s> ReadImage<'s> for R where R: ReadLayers<'s> {
    type Reader = ImageWithAttributesReader<R::Reader>;

    fn create_image_reader(&'s self, headers: &[Header]) -> Result<Self::Reader> {
        Ok(ImageWithAttributesReader {
            image_attributes: headers.first().expect("invalid headers").shared_attributes.clone(),
            layers_reader: self.create_layers_reader(headers)?,
        })
    }
}


#[derive(Debug, Clone, PartialEq)]
pub struct ImageWithAttributesReader<L> {
    image_attributes: ImageAttributes,
    layers_reader: L,
}

pub trait LayersReader {
    type Layers: 'static;
    fn filter_block(&self, header: (usize, &Header), tile: (usize, &TileCoordinates)) -> bool;
    fn read_block(&mut self, headers: &[Header], block: UncompressedBlock) -> UnitResult;
    fn into_layers(self) -> Self::Layers;
}


/*impl<'s, L: 's> ReadImage<'s> for ReadImageWithAttributes<L> where L: ReadLayers<'s> {
    type Reader = ImageWithAttributesReader<L::Reader>;

    fn create_image_reader(&'s self, headers: &[Header]) -> Result<ImageWithAttributesReader<L::Reader>> {
        Ok(ImageWithAttributesReader {
            image_attributes: headers.first().expect("invalid headers").shared_attributes.clone(),
            layers_reader: self.read_layers.create_layers_reader(headers)?,
        })
    }
}*/

impl<L> ImageReader for ImageWithAttributesReader<L> where L: LayersReader {
    type Image = Image<L::Layers>;

    fn filter_block(&self, header: (usize, &Header), tile: (usize, &TileCoordinates)) -> bool {
        self.layers_reader.filter_block(header, tile)
    }

    fn read_block(&mut self, headers: &[Header], block: UncompressedBlock) -> UnitResult {
        self.layers_reader.read_block(headers, block)
    }

    fn into_image(self) -> Self::Image {
        Image {
            attributes: self.image_attributes,
            layer_data: self.layers_reader.into_layers()
        }
    }
}

