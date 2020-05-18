
//! Supports all features of OpenEXR, as desired.
//! Use `rgba` if this is not required.



use crate::prelude::common::*;
use std::collections::HashMap;
use std::path::Path;
use std::io::{Read, BufReader};
use crate::block::lines::LineRef;



fn read_example() {

    let complex = samples::any() // uses enums as sample type
        .flat_data() // alternatives: any_data_depth() and deep_data()
        .all_resolution_levels() // default: max_resolution_level()
        .all_layers() // alternatives: first_layer() and layer_groups(), default: all_layers
        .on_progress(|progress| println!("loaded {}%", progress)) // default: ()
        .filter(|meta_data| meta_data.byte_size < one_gigabyte) // default: <1GB
        .read_file("file.exr")?;

    // 1 layer, flat_data, largest_level, 1GB filter, no on_progress
    let simple = samples::f16()
        .named_layer("main") // find the layer with this name
        .read_file("simple.exr")?;

    // deep data, no mip maps, one layer or err
    let deep = samples::f32()
        .deep_data()
        .largest_resolution_level()
        .expect_single_layer()
        .read_file("deep.exr")?;

    // flat data, enum samples
    let rgb = samples::any()
        .any_data_depth()
        .all_resolution_levels()
        .select_layer(|headers| header.enumerate().find(|header| header.name == "main")) // `Fn(&[Header]) -> Option<usize>`
        .read_file("deep.exr")?;
}

pub mod samples {
    use super::*;

    pub fn any() -> AnySamples { AnySamples {} }
    pub fn f16() -> F16Samples { F16Samples {} }
    pub fn f32() -> F32Samples { F32Samples {} }
    pub fn u32() -> U32Samples { U32Samples {} }
}

pub struct AnySamples;
pub struct F16Samples;
pub struct F32Samples;
pub struct U32Samples;

pub mod pixels {
    use super::*;

    pub fn flat() -> ReadFlatPixels { ReadFlatPixels {} }
    pub fn any_depth() -> ReadAnyPixels { ReadAnyPixels {} }
    pub fn deep() -> ReadDeepPixels { ReadDeepPixels {} }
}

pub struct ReadFlatPixels<Sample>;
pub struct ReadAnyPixels<Sample>;
pub struct ReadDeepPixels<Sample>;

impl ReadFlatPixels {
    pub fn all_resolution_levels(self) -> ReadAllLevels<Self> { ReadAllLevels { read_pixels: self } }
    pub fn largest_resolution_level(self) -> ReadLargestLevel<Self> { ReadLargestLevel { read_pixels: self } }
}

impl ReadAnyPixels {
    pub fn all_resolution_levels(self) -> ReadAllLevels<Self> { ReadAllLevels { read_pixels: self } }
    pub fn largest_resolution_level(self) -> ReadLargestLevel<Self> { ReadLargestLevel { read_pixels: self } }
}

impl ReadDeepPixels {
    pub fn all_resolution_levels(self) -> ReadAllLevels<Self> { ReadAllLevels { read_pixels: self } }
    pub fn largest_resolution_level(self) -> ReadLargestLevel<Self> { ReadLargestLevel { read_pixels: self } }
}

pub struct ReadAllLevels<Pixels> {
    read_pixels: Pixels
}

pub struct ReadLargestLevel<Pixels> {
    read_pixels: Pixels
}

impl<Pixels> ReadAllLevels<Pixels> {
    pub fn all_layers(self) -> ReadAllLayers<Self> { ReadAllLayers { read_levels: self } }
    pub fn grouped_layers(self) -> ReadGroupedLayers<Self> { ReadGroupedLayers { read_levels: self } }
    pub fn first_layer(self) -> ReadFirstLayer<Self> { ReadFirstLayer { read_levels: self } }
}

impl<Pixels> ReadLargestLevel<Pixels> {
    pub fn all_layers(self) -> ReadAllLayers<Self> { ReadAllLayers { read_levels: self } }
    pub fn first_layer(self) -> ReadFirstLayer<Self> { ReadFirstLayer { read_levels: self } }
}

pub struct ReadAllLayers<Levels> {
    read_levels: Levels
}

pub struct ReadGroupedLayers<Levels> {
    read_levels: Levels
}

pub struct ReadFirstLayer<Levels> {
    read_levels: Levels
}





/*trait ReadImage {

    type Output;



    fn read_from_buffered(&self, read: impl Read + Send) -> Result<Self::Output> {
        // crate::block::lines::read_all_lines_from_buffered(
        //     read,
        //     Image::allocate,
        //     |image, _meta, line| Image::insert_line(image, line),
        //     options
        // )
    }

    fn read_from_file(&self, path: impl AsRef<Path>) -> Result<Self::Output> {
        self.read_from_unbuffered(std::fs::File::open(path)?)
    }

    fn read_from_unbuffered(&self, unbuffered: impl Read + Send) -> Result<Self::Output> {
        self.read_from_buffered(BufReader::new(unbuffered))
    }

    fn on_progress<F>(self, callback: F) -> OnProgress<Self, F> where F: Fn(f64) {
        OnProgress { inner: self, callback }
    }

    fn filter<F>(self, callback: F) -> Filter<Self, F> where F: Fn(f64) {
        Filter { inner: self, callback }
    }
}*/

pub struct OnProgress<I, F> {
    inner: I,
    callback: F,
}

pub struct Filter<I, F> {
    inner: I,
    callback: F,
}







pub type FlatImage = Image<LayerList<FlatLayerContents<>>>;


pub struct Image<Layers> {
    attributes: ImageAttributes,
    layers: Layers
}


pub struct LayerGroups<LayerContents> {
    sub_groups: HashMap<Text, LayerGroups<LayerContents>>,
    layers: List<LayerContents>
}

pub struct LayerList<LayerContents> {
    list: SmallVec<[LayerContents; 3]>
}

pub struct SingleLayer<LayerContents> {
    layer: LayerContents
}

pub struct Layer<LayerContents> {
    attributes: LayerAttributes,
    encoding: LayerEncoding,
    data_size: Vec2<usize>,
    contents: LayerContents,
}

pub struct Channel<Pixels> {
    pub name: Text,
    pub pixels: Pixels,
    pub quantize_linearly: bool,
    pub sampling: Vec2<usize>,
}

// adapter between layer and another contents, while itself also being layer contents
pub enum ResolutionLevels<LayerContents> {
    Singular(LayerContents),
    Mip(LevelMaps<LayerContents>),
    Rip(RipMaps<LayerContents>),
}

pub struct FlatLayerContents {

}

pub enum AnyDepthLayerContents {

}





#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LayerEncoding {
    pub compression: Compression,
    pub line_order: LineOrder,

    // pub tile_size: Option<Vec2<usize>>,
    // OR (DEPENDING ON WHETHER TILING WAS CHOSEN) pub blocks: Blocks,
}


pub type LevelMaps<Level> = Vec<Level>;

#[derive(Clone, PartialEq, Debug)]
pub struct RipMaps<Level> {
    pub map_data: LevelMaps<Level>,
    pub level_count: Vec2<usize>,
}