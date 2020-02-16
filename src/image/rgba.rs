
//! Read and write a really minimal rgba image: This module loads only images with RGBA channels and f32 values.
//! Use `exr::image::simple` if you need custom channels or display windows.


use std::path::Path;
use std::fs::File;
use std::io::{Read, Seek, BufReader, BufWriter, Write};
use crate::math::Vec2;
use crate::error::{Result, Error, PassiveResult};
use crate::meta::attributes::PixelType;
use std::convert::TryInto;
use crate::meta::{MetaData, Header, attributes::{self}, ImageAttributes, LayerAttributes};


/// References one of the RGBA channels, like an index.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub enum Channel {

    /// The `Red` channel of an RGBA image.
    Red,

    /// The `Green` channel of an RGBA image.
    Green,

    /// The `Blue` channel of an RGBA image.
    Blue,

    /// The `Alpha` channel of an RGBA image.
    Alpha
}

impl Channel {

    /// Returns the index of the channel when indexing into an `[r, g, b, a]` sequence.
    pub fn as_rgba_index(self) -> usize {
        match self {
            Channel::Red => 0,
            Channel::Green => 1,
            Channel::Blue => 2,
            Channel::Alpha => 3,
        }
    }
}


// TODO also use a trait inside exr::image::read_filtered_lines_from_buffered?

pub trait NewImage: Sized {
    fn new(size: Vec2<usize>, alpha_channel: bool, image: &ImageAttributes, layer: &LayerAttributes) -> Self;
    fn set_sample(&mut self, pixel: Vec2<usize>, channel: Channel, value: f32);

    #[must_use]
    fn read_from_file(path: impl AsRef<Path>, parallel: bool) -> Result<Self> {
        Self::read_from_unbuffered(File::open(path)?, parallel)
    }

    #[must_use]
    fn read_from_unbuffered(read: impl Read + Seek + Send, parallel: bool) -> Result<Self> {
        Self::read_from_buffered(BufReader::new(read), parallel)
    }

    #[must_use]
    fn read_from_buffered(read: impl Read + Seek + Send, parallel: bool) -> Result<Self> {
        crate::image::read_all_lines_from_buffered(
            read, parallel,
            |headers| {
                if headers.len() == 1 {
                    let header = &headers[0];
                    let channels = &header.channels.list;

                    if channels.len() == 4
                        && channels[0].name == "A".try_into().unwrap()
                        && channels[1].name == "B".try_into().unwrap()
                        && channels[2].name == "G".try_into().unwrap()
                        && channels[3].name == "R".try_into().unwrap()
                        && channels.iter().all(|channel| channel.pixel_type == PixelType::F32) // TODO also other formats!
                    {
                        return Ok(Self::new(header.data_size, true, &header.shared_attributes, &header.own_attributes))
                    }
                }

                Err(Error::invalid("exr image does not contain one unambiguous set of rgba channels"))
            },

            |image, line| {

                // channels are sorted alphabetically
                let channel = match line.location.channel {
                    0 => Channel::Alpha,
                    1 => Channel::Blue,
                    2 => Channel::Green,
                    3 => Channel::Red,
                    _ => panic!("invalid line channel index bug")
                };

                for (sample_index, sample) in line.read_samples::<f32>().enumerate() { // TODO any pixel_type?
                    let location = line.location.position + Vec2(sample_index, 0);
                    Self::set_sample(image, location, channel, sample?);
                }

                Ok(())
            },

            () // TODO progress callback
        )
    }
}

pub trait GetImage: Sync { // TODO avoid sync requirement
    fn size(&self) -> Vec2<usize>;
    fn get_sample(&self, index: Vec2<usize>, channel: Channel) -> f32;
    fn layer_attributes(&self) -> LayerAttributes;
    fn image_attributes(&self) -> ImageAttributes;

    // TODO delete file on error
    #[must_use]
    fn write_to_file(&self, path: impl AsRef<Path>, parallel: bool, pedantic: bool) -> PassiveResult {
        self.write_to_unbuffered(File::create(path)?, parallel, pedantic)
    }

    #[must_use]
    fn write_to_unbuffered(&self, write: impl Write + Seek, parallel: bool, pedantic: bool) -> PassiveResult {
        self.write_to_buffered(BufWriter::new(write), parallel, pedantic)
    }

    #[must_use]
    fn write_to_buffered(&self, write: impl Write + Seek, parallel: bool, pedantic: bool) -> PassiveResult {
        crate::image::write_all_lines_to_buffered(
            write, parallel, pedantic,
            MetaData::new(smallvec![
                Header::new(
                    "rgba-image".try_into().unwrap(),
                    self.size(),
                    smallvec![
                        attributes::Channel::new("A".try_into().unwrap(), PixelType::F32, true), // TODO make linear a parameter
                        attributes::Channel::new("B".try_into().unwrap(), PixelType::F32, true),
                        attributes::Channel::new("G".try_into().unwrap(), PixelType::F32, true),
                        attributes::Channel::new("R".try_into().unwrap(), PixelType::F32, true),
                    ]
                )
            ]),

            |line_mut| {

                // channels are sorted alphabetically
                let channel = match line_mut.location.channel {
                    0 => Channel::Alpha,
                    1 => Channel::Blue,
                    2 => Channel::Green,
                    3 => Channel::Red,
                    _ => panic!("invalid line channel index bug")
                };

                let position = line_mut.location.position;

                line_mut.write_samples(|sample_index|{
                    let location = position + Vec2(sample_index, 0);
                    self.get_sample(location, channel)
                })
            },

            () // TODO progress callback
        )
    }
}



