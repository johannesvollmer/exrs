
use crate::meta::*;
use crate::image::*;
use crate::meta::header::{Header};
use crate::error::{Result, UnitResult};
use crate::block::lines::LineRef;
use crate::math::Vec2;
use crate::meta::attribute::{ChannelInfo, LevelMode};
use crate::image::read::any_channels::{SamplesReader, ReadSamples, ReadAnyChannels};
use crate::block::chunk::TileCoordinates;
use crate::image::read::rgba_channels::*;


// Note: In the resulting image, the `FlatSamples` are placed
// directly inside the channels, without `LargestLevel<>` indirection
/// Specify to read only the highest resolution level, skipping all smaller variations.
/// The sample storage can be `ReadFlatSamples`.
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

    // TODO only for flat samples
    /// Read only layers that contain red, green and blue color. If present, also loads alpha channels.
    /// Rejects all layers that don't have rgb channels. Skips any other channels in an rgb layer.
    pub fn rgba_channels<Create, Set>(self, create: Create, set_pixel: Set) -> ReadRgbaChannels<Create, Set>
        where Create: CreateRgbaPixels, Set: SetRgbaPixel<Create::Pixels>
    {
        ReadRgbaChannels { create, set_pixel }
    }
}

/// Specify to read all contained resolution levels from the image, if any.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadAllLevels<DeepOrFlatSamples> {
    pub read_samples: DeepOrFlatSamples
}

impl<ReadDeepOrFlatSamples> ReadAllLevels<ReadDeepOrFlatSamples> {

    /// Read all arbitrary channels in each layer.
    pub fn all_channels(self) -> ReadAnyChannels<Self> { ReadAnyChannels { read_samples: self } }

    // TODO only for flat samples
    /// Read only layers that contain red, green and blue color. If present, also loads alpha channels.
    /// Rejects all layers that don't have rgb channels. Skips any other channels in the layer.
    pub fn rgba_channels<Create, Set>(self, create: Create, set_pixel: Set) -> ReadRgbaChannels<Create, Set>
        where Create: CreateRgbaPixels, Set: SetRgbaPixel<Create::Pixels>
    {
        ReadRgbaChannels { create, set_pixel }
    }
}

/*pub struct ReadLevels<S> {
    read_samples: S,
}*/

/// Processes pixel blocks from a file and accumulates them into multiple levels per channel.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AllLevelsReader<SamplesReader> {
    levels: Levels<SamplesReader>,
}

/// A template that creates a `SamplesReader` once for each resolution level.
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
                let round = tiles.rounding_mode;

                match tiles.level_mode {
                    LevelMode::Singular => Levels::Singular(self.read_samples.create_samples_level_reader(header, channel, Vec2(0,0), header.layer_size)?),

                    LevelMode::MipMap => Levels::Mip({
                        let maps: Result<LevelMaps<S::Reader>> = mip_map_levels(round, data_size)
                            .map(|(index, level_size)| self.read_samples.create_samples_level_reader(header, channel, Vec2(index, index), level_size))
                            .collect();

                        maps?
                    }),

                    // TODO put this into Levels::new(..) ?
                    LevelMode::RipMap => Levels::Rip({
                        let level_count_x = compute_level_count(round, data_size.width());
                        let level_count_y = compute_level_count(round, data_size.height());
                        let maps: Result<LevelMaps<S::Reader>> = rip_map_levels(round, data_size)
                            .map(|(index, level_size)| self.read_samples.create_samples_level_reader(header, channel, index, level_size))
                            .collect();

                        RipMaps {
                            map_data: maps?,
                            level_count: Vec2(level_count_x, level_count_y)
                        }
                    })
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
        true // TODO this is not beautiful?
    }

    fn read_line(&mut self, line: LineRef<'_>) -> UnitResult {
        self.levels.get_level_mut(line.location.level)?.read_line(line)
    }

    fn into_samples(self) -> Self::Samples {
        match self.levels {
            Levels::Singular(level) => Levels::Singular(level.into_samples()),
            Levels::Mip(maps) => Levels::Mip(maps.into_iter().map(|s| s.into_samples()).collect()),
            Levels::Rip(maps) => Levels::Rip(RipMaps {
                map_data: maps.map_data.into_iter().map(|s| s.into_samples()).collect(),
                level_count: maps.level_count
            }),
        }
    }
}