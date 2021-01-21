//! How to read a set of resolution levels.

use crate::meta::*;
use crate::image::*;
use crate::meta::header::{Header};
use crate::error::{Result, UnitResult};
use crate::block::lines::LineRef;
use crate::math::Vec2;
use crate::meta::attribute::{ChannelInfo, LevelMode};
use crate::image::read::any_channels::{SamplesReader, ReadSamples, ReadAnyChannels};
use crate::block::chunk::TileCoordinates;
use crate::image::read::specific_channels::*;


// Note: In the resulting image, the `FlatSamples` are placed
// directly inside the channels, without `LargestLevel<>` indirection
/// Specify to read only the highest resolution level, skipping all smaller variations.
/// The sample storage can be [`ReadFlatSamples`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadLargestLevel<DeepOrFlatSamples> {

    /// The sample reading specification
    pub read_samples: DeepOrFlatSamples
}


// FIXME rgba levels???

// Read the largest level, directly, without intermediate structs
impl<DeepOrFlatSamples> ReadLargestLevel<DeepOrFlatSamples> {

    /// Read all arbitrary channels in each layer.
    pub fn all_channels(self) -> ReadAnyChannels<DeepOrFlatSamples> { ReadAnyChannels { read_samples: self.read_samples } } // Instead of Self, the `FlatSamples` are used directly

    /*/// Read only layers that contain red, green and blue color. If present, also loads alpha channels.
    /// Rejects all layers that don't have rgb channels. Skips all channels other than red, green, blue and alpha.
    /// `Create` can be a closure of type [`Fn(&RgbaChannelsInfo) -> YourPixelStorage`].
    /// `Set` can be a closure of type [`Fn(&mut YourPixelStorage, Vec2<usize>, RgbaPixel)`].
    /// Throws an error for images with deep data.
    pub fn rgba_channels<CreatePixelStorage, SetPixel>(self, create: CreatePixelStorage, set_pixel: SetPixel)
        -> ReadRgbaChannels<CreatePixelStorage, SetPixel>// ReadSpecificChannels<(Sample, Sample, Sample, Option<Sample>), (&'static str, &'static str, &'static str, &'static str), CreatePixelStorage, SetPixel> // TODO type alias?
        where CreatePixelStorage: CreatePixels<RgbaSampleTypes>, SetPixel: SetPixel<CreatePixelStorage::Pixels, RgbaPixel>
    {
        ReadRgbaChannels { channel_names, create, set_pixel, px: Default::default() }

        /*self.specific_channels(
            ("R", "G", "B", "A"),

            |info| {
                create.create(&ChannelsInfo {
                    sample_types: RgbaSampleTypes(info.sample_types.0, info.sample_types.1, info.sample_types.2, info.sample_types.3),
                    resolution: info.resolution,
                })
            },

            |pixels, position, (r,g,b,a): (Sample, Sample, Sample, Option<Sample>)| {
                set_pixel.set_pixel(pixels, position, RgbaPixel::new(r, g, b, a));
            }
        )*/
    }*/

    /*pub fn rgb_channels<Px, Create, Set>(
        self, create: Create, set_pixel: Set
    ) -> ReadSpecificChannels<Px, (&'static str,&'static str,&'static str), Create, Set>
        where
            Channels: ReadFilteredChannels<Px>,
            Create: CreatePixels<<Channels::Filter as ChannelsFilter<Px>>::ChannelsInfo>,
            Set: SetPixel<Create::Pixels, Px>,
    {
        self.specific_channels(("R", "G", "B"), create, set_pixel)
    }*/
    /// Read only layers that contain rgba channels. Skips any other channels in the layer.
    /// `Create` can be a closure of type [`Fn(&RgbaChannelsInfo) -> YourPixelStorage`].
    /// `Set` can be a closure of type [`Fn(&mut YourPixelStorage, Vec2<usize>, RgbaPixel)`].
    /// Throws an error for images with deep data.
    ///
    /// Use `specific_channels` or `all_channels` if you want to read something other than rgba.
    pub fn rgba_channels<Px, Create, Set>(
        self, create: Create, set_pixel: Set
    ) -> ReadSpecificChannels<Px, (&'static str,&'static str,&'static str,&'static str), Create, Set>
        where
            (&'static str,&'static str,&'static str,&'static str): ReadFilteredChannels<Px>, // limits `Px`
            Create: CreatePixels<<<(&'static str,&'static str,&'static str,&'static str) as ReadFilteredChannels<Px>>::Filter as ChannelsFilter<Px>>::ChannelsInfo>,
            Set: SetPixel<Create::Pixels, Px>,
    {
        self.specific_channels(("R", "G", "B", "A"), create, set_pixel)
    }

    // TODO FIXME support directly using `f32` as parameter, not just Sample!

    /// Read only layers that contain the specified channels. Skips any other channels in the layer.
    /// `Create` can be a closure of type [`Fn(&ChannelsInfo) -> YourPixelStorage`].
    /// `Set` can be a closure of type [`Fn(&mut YourPixelStorage, Vec2<usize>, YourPixel)`].
    /// The `set_pixel` closure must define the pixel type, for example with `(f32, f32, f32, Option<f16>)`.
    /// The Pixel type should be a tuple containing any combination of `f32`, `f16`, or `u32` values.
    ///
    // TODO example for pixel s
    /// Throws an error for images with deep data.
    /*pub fn specific_channels<Px, Channels, Create, Set>(
        self, channel_names: Channels, create: Create, set_pixel: Set
    ) -> ReadSpecificChannels<Px, Channels, Create, Set>
        where
            Channels: ReadFilteredChannels<Px>,
            Create: CreatePixels<<Channels::Filter as ChannelsFilter<Px>>::ChannelsInfo>,
            Set: SetPixel<Create::Pixels, Px>,
    {
        ReadSpecificChannels { channel_names, create, set_pixel, px: Default::default() }
    }*/
    pub fn specific_channels<Px, Channels, Create, Set>(
        self, channel_names: Channels, create: Create, set_pixel: Set
    ) -> ReadSpecificChannels<Px, Channels, Create, Set>
        where
            Channels: ReadFilteredChannels<Px>,
            Create: CreatePixels<<Channels::Filter as ChannelsFilter<Px>>::ChannelsInfo>,
            Set: SetPixel<Create::Pixels, Px>,
    {
        ReadSpecificChannels { channel_names, create, set_pixel, px: Default::default() }
    }
}

/// Specify to read all contained resolution levels from the image, if any.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadAllLevels<DeepOrFlatSamples> {

    /// The sample reading specification
    pub read_samples: DeepOrFlatSamples
}

impl<ReadDeepOrFlatSamples> ReadAllLevels<ReadDeepOrFlatSamples> {

    /// Read all arbitrary channels in each layer.
    pub fn all_channels(self) -> ReadAnyChannels<Self> { ReadAnyChannels { read_samples: self } }

    // TODO rgba resolution levels
    /*/// Read only layers that contain red, green and blue color. If present, also loads alpha channels.
    /// Rejects all layers that don't have rgb channels. Skips any other channels in the layer.
    /// `Create` can be a closure of type [`Fn(&RgbaChannelsInfo) -> YourPixelStorage`].
    /// `Set` can be a closure of type [`Fn(&mut YourPixelStorage, Vec2<usize>, RgbaPixel)`].
    /// Throws an error for images with deep data.
    pub fn rgba_channels<Create, Set>(self, create: Create, set_pixel: Set) -> ReadRgbaChannels<Create, Set>
        where Create: CreateRgbaPixels, Set: SetRgbaPixel<Create::Pixels>
    {
        ReadRgbaChannels { create, set_pixel }
    }*/
}

/*pub struct ReadLevels<S> {
    read_samples: S,
}*/

/// Processes pixel blocks from a file and accumulates them into multiple levels per channel.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AllLevelsReader<SamplesReader> {
    levels: Levels<SamplesReader>,
}

/// A template that creates a [`SamplesReader`] once for each resolution level.
pub trait ReadSamplesLevel {

    /// The type of the temporary level reader
    type Reader: SamplesReader;

    /// Create a single reader for a single resolution level
    fn create_samples_level_reader(&self, header: &Header, channel: &ChannelInfo, level: Vec2<usize>, resolution: Vec2<usize>) -> Result<Self::Reader>;
}


impl<S: ReadSamplesLevel> ReadSamples for ReadAllLevels<S> {
    type Reader = AllLevelsReader<S::Reader>;

    fn create_sample_reader(&self, header: &Header, channel: &ChannelInfo) -> Result<Self::Reader> {
        let data_size = header.layer_size / channel.sampling;

        let levels = {
            if let crate::meta::Blocks::Tiles(tiles) = &header.blocks {
                match tiles.level_mode {
                    LevelMode::Singular => Levels::Singular(self.read_samples.create_samples_level_reader(header, channel, Vec2(0,0), header.layer_size)?),

                    LevelMode::MipMap => Levels::Mip {
                        rounding_mode: tiles.rounding_mode,
                        level_data: {
                            let round = tiles.rounding_mode;
                            let maps: Result<LevelMaps<S::Reader>> = mip_map_levels(round, data_size)
                                .map(|(index, level_size)| self.read_samples.create_samples_level_reader(header, channel, Vec2(index, index), level_size))
                                .collect();

                            maps?
                        },
                    },

                    // TODO put this into Levels::new(..) ?
                    LevelMode::RipMap => Levels::Rip {
                        rounding_mode: tiles.rounding_mode,
                        level_data: {
                            let round = tiles.rounding_mode;
                            let level_count_x = compute_level_count(round, data_size.width());
                            let level_count_y = compute_level_count(round, data_size.height());
                            let maps: Result<LevelMaps<S::Reader>> = rip_map_levels(round, data_size)
                                .map(|(index, level_size)| self.read_samples.create_samples_level_reader(header, channel, index, level_size))
                                .collect();

                            RipMaps {
                                map_data: maps?,
                                level_count: Vec2(level_count_x, level_count_y)
                            }
                        },
                    },
                }
            }

            // scan line blocks never have mip maps
            else {
                Levels::Singular(self.read_samples.create_samples_level_reader(header, channel, Vec2(0, 0), data_size)?)
            }
        };

        Ok(AllLevelsReader { levels })
    }
}


impl<S: SamplesReader> SamplesReader for AllLevelsReader<S> {
    type Samples = Levels<S::Samples>;

    fn filter_block(&self, _: (usize, &TileCoordinates)) -> bool {
        true
    }

    fn read_line(&mut self, line: LineRef<'_>) -> UnitResult {
        self.levels.get_level_mut(line.location.level)?.read_line(line)
    }

    fn into_samples(self) -> Self::Samples {
        match self.levels {
            Levels::Singular(level) => Levels::Singular(level.into_samples()),
            Levels::Mip { rounding_mode, level_data } => Levels::Mip {
                rounding_mode, level_data: level_data.into_iter().map(|s| s.into_samples()).collect(),
            },

            Levels::Rip { rounding_mode, level_data } => Levels::Rip {
                rounding_mode,
                level_data: RipMaps {
                    level_count: level_data.level_count,
                    map_data: level_data.map_data.into_iter().map(|s| s.into_samples()).collect(),
                }
            },
        }
    }
}
