use crate::meta::attribute::{LevelMode, SampleType, TileDescription};
use crate::meta::header::Header;
use crate::block::lines::LineRefMut;
use crate::image::{FlatSamples, Levels, RipMaps};
use crate::math::Vec2;
use crate::meta::{rip_map_levels, mip_map_levels, rip_map_indices, mip_map_indices, Blocks};

/// inside `Channels`
pub trait WritableSamples<'s> {
    // fn is_deep(&self) -> bool;
    fn sample_type(&self) -> SampleType;
    fn level_mode(&self) -> LevelMode;

    type Writer: 's + SamplesWriter;
    fn create_samples_writer(&'s self, header: &Header) -> Self::Writer;
}

/// inside `Levels`
pub trait WritableLevel<'s> {
    fn sample_type(&self) -> SampleType;

    type Writer: 's + SamplesWriter;
    fn create_level_writer(&'s self, size: Vec2<usize>) -> Self::Writer;
}

pub trait SamplesWriter: Sync {
    fn extract_line(&self, line: LineRefMut<'_>);
}

/*pub trait InferSampleType { const SAMPLE_TYPE: SampleType; }
impl InferSampleType for f16 { const SAMPLE_TYPE: SampleType = SampleType::F16; }
impl InferSampleType for f32 { const SAMPLE_TYPE: SampleType = SampleType::F32; }
impl InferSampleType for u32 { const SAMPLE_TYPE: SampleType = SampleType::U32; }*/

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct FlatSamplesWriter<'s> {
    resolution: Vec2<usize>, // respects resolution level
    samples: &'s FlatSamples
}

/*impl<'s, F:'s, S:'s> WritableSamples<'s> for F where F: Sync + Fn(Vec2<usize>) -> S, S: InferSampleType + Data {
    fn sample_type(&self) -> SampleType { S::SAMPLE_TYPE }
    fn level_mode(&self) -> LevelMode { LevelMode::Singular } // TODO impl WritableLevels!

    type Writer = FnSampleWriter<'s, F>;

    fn create_samples_writer(&'s self, _: &Header) -> Self::Writer {
        FnSampleWriter { closure: self }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnSampleWriter<'f, F> {
    closure: &'f F,
}

impl<'f, S, F> SamplesWriter for FnSampleWriter<'f, F> where F: Sync + Fn(Vec2<usize>) -> S, S: Data {
    fn extract_line(&self, line: LineRefMut<'_>) {
        let start_position = line.location.position;
        let closure = &self.closure;

        line.write_samples(|x| closure(start_position + Vec2(x, 0)))
            .expect("writing to in-memory buffer failed");
    }
}*/



/// used, if no layers are used and the flat samples are directly inside the channels
impl<'s> WritableSamples<'s> for FlatSamples {
    fn sample_type(&self) -> SampleType {
        match self {
            FlatSamples::F16(_) => SampleType::F16,
            FlatSamples::F32(_) => SampleType::F32,
            FlatSamples::U32(_) => SampleType::U32,
        }
    }

    fn level_mode(&self) -> LevelMode { LevelMode::Singular }

    type Writer = FlatSamplesWriter<'s>; //&'s FlatSamples;
    fn create_samples_writer(&'s self, header: &Header) -> Self::Writer {
        FlatSamplesWriter {
            resolution: header.layer_size,
            samples: self
        }
    }
}

/// used, if layers are used and the flat samples are inside the levels
impl<'s> WritableLevel<'s> for FlatSamples {
    fn sample_type(&self) -> SampleType {
        match self {
            FlatSamples::F16(_) => SampleType::F16,
            FlatSamples::F32(_) => SampleType::F32,
            FlatSamples::U32(_) => SampleType::U32,
        }
    }

    type Writer = FlatSamplesWriter<'s>;
    fn create_level_writer(&'s self, size: Vec2<usize>) -> Self::Writer {
        FlatSamplesWriter {
            resolution: size,
            samples: self
        }
    }
}

impl<'s> SamplesWriter for FlatSamplesWriter<'s> {
    fn extract_line(&self, line: LineRefMut<'_>) {
        let image_width = self.resolution.width(); // header.layer_size.width();
        debug_assert_ne!(image_width, 0, "image width calculation bug");

        let start_index = line.location.position.y() * image_width + line.location.position.x();
        let end_index = start_index + line.location.sample_count;

        debug_assert!(
            start_index < end_index && end_index <= self.samples.len(),
            "for resolution {:?}, this is an invalid line: {:?}",
            self.resolution, line.location
        );

        match self.samples {
            FlatSamples::F16(samples) => line.write_samples_from_slice(&samples[start_index .. end_index]),
            FlatSamples::F32(samples) => line.write_samples_from_slice(&samples[start_index .. end_index]),
            FlatSamples::U32(samples) => line.write_samples_from_slice(&samples[start_index .. end_index]),
        }.expect("writing line bytes failed");
    }
}


impl<'s, S> WritableSamples<'s> for Levels<S> where S: WritableLevel<'s> {
    fn sample_type(&self) -> SampleType {
        let sample_type = self.levels_as_slice().first().unwrap().sample_type();
        debug_assert!(self.levels_as_slice().iter().skip(1).all(|ty| ty.sample_type() == sample_type));
        sample_type
    }

    fn level_mode(&self) -> LevelMode {
        match self {
            Levels::Singular(_) => LevelMode::Singular,
            Levels::Mip(_) => LevelMode::MipMap,
            Levels::Rip(_) => LevelMode::RipMap,
        }
    }

    type Writer = LevelsWriter<S::Writer>;
    fn create_samples_writer(&'s self, header: &Header) -> Self::Writer {
        let rounding = match header.blocks {
            Blocks::Tiles(TileDescription { rounding_mode, .. }) => Some(rounding_mode),
            Blocks::ScanLines => None,
        };

        LevelsWriter {
            levels: match self {
                Levels::Singular(level) => Levels::Singular(level.create_level_writer(header.layer_size)),
                Levels::Mip(levels) => {
                    debug_assert_eq!(
                        levels.len(),
                        mip_map_indices(rounding.expect("mip maps only with tiles"), header.layer_size).count(),
                        "invalid mip map count"
                    );

                    Levels::Mip( // TODO store level size in image??
                        levels.iter()
                            .zip(mip_map_levels(rounding.expect("mip maps only with tiles"), header.layer_size))
                            // .map(|level| level.create_samples_writer(header))
                            .map(|(level, (_level_index, level_size))| level.create_level_writer(level_size))
                            .collect()
                    )
                },
                Levels::Rip(maps) => {
                    debug_assert_eq!(maps.map_data.len(), maps.level_count.area());
                    debug_assert_eq!(
                        maps.map_data.len(),
                        rip_map_indices(rounding.expect("rip maps only with tiles"), header.layer_size).count(),
                        "invalid rip map count"
                    );

                    Levels::Rip(RipMaps {
                        level_count: maps.level_count,
                        map_data: maps.map_data.iter()
                            .zip(rip_map_levels(rounding.expect("rip maps only with tiles"), header.layer_size))
                            .map(|(level, (_level_index, level_size))| level.create_level_writer(level_size))
                            .collect(),
                    })
                }
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LevelsWriter<S> {
    levels: Levels<S>,
}

impl<S> SamplesWriter for LevelsWriter<S> where S: SamplesWriter {
    fn extract_line(&self, line: LineRefMut<'_>) {
        self.levels.get_level(line.location.level).expect("invalid level index") // TODO compute level size from line index??
            .extract_line(line)
    }
}
