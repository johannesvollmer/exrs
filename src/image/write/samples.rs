use crate::meta::attribute::{LevelMode, SampleType};
use crate::meta::header::Header;
use crate::block::lines::LineRefMut;
use crate::image::{FlatSamples, Levels, RipMaps};


pub trait WritableSamples<'s> {
    // fn is_deep(&self) -> bool;
    fn sample_type(&self) -> SampleType;
    fn level_mode(&self) -> LevelMode;

    type Writer: 's + SamplesWriter;
    fn create_writer(&'s self, header: &Header) -> Self::Writer;
}

pub trait SamplesWriter: Sync {
    fn extract_line(&self, header: &Header, line: LineRefMut<'_>);
}



impl<'s> WritableSamples<'s> for FlatSamples {
    fn sample_type(&self) -> SampleType {
        match self {
            FlatSamples::F16(_) => SampleType::F16,
            FlatSamples::F32(_) => SampleType::F32,
            FlatSamples::U32(_) => SampleType::U32,
        }
    }

    fn level_mode(&self) -> LevelMode { LevelMode::Singular }

    type Writer = &'s FlatSamples;
    fn create_writer(&'s self, _: &Header) -> Self::Writer { self }
}

impl<'s> SamplesWriter for &'s FlatSamples {
    fn extract_line(&self, header: &Header, line: LineRefMut<'_>) {
        let image_width = header.layer_size.width();
        debug_assert_ne!(image_width, 0, "image width calculation bug");

        let start_index = line.location.position.y() * image_width + line.location.position.x();
        let end_index = start_index + line.location.sample_count;

        match self {
            FlatSamples::F16(samples) => line.write_samples_from_slice(&samples[start_index .. end_index]),
            FlatSamples::F32(samples) => line.write_samples_from_slice(&samples[start_index .. end_index]),
            FlatSamples::U32(samples) => line.write_samples_from_slice(&samples[start_index .. end_index]),
        }.expect("writing line bytes failed");
    }
}


impl<'s, S> WritableSamples<'s> for Levels<S> where S: WritableSamples<'s> {
    fn sample_type(&self) -> SampleType {
        let sample_type = self.levels_as_slice().first().unwrap().sample_type();
        debug_assert!(self.levels_as_slice().iter().skip(1).all(|ty| ty.sample_type() == sample_type));
        sample_type
    }

    fn level_mode(&self) -> LevelMode {
        debug_assert!(
            self.levels_as_slice().iter().all(|level| level.level_mode() == LevelMode::Singular),
            "Levels should not contain more levels"
        );

        match self {
            Levels::Singular(_) => LevelMode::Singular,
            Levels::Mip(_) => LevelMode::MipMap,
            Levels::Rip(_) => LevelMode::RipMap,
        }
    }

    type Writer = LevelsWriter<S::Writer>;
    fn create_writer(&'s self, header: &Header) -> Self::Writer {
        LevelsWriter {
            levels: match self {
                Levels::Singular(level) => Levels::Singular(level.create_writer(header)),
                Levels::Mip(levels) => Levels::Mip(levels.iter().map(|level| level.create_writer(header)).collect()),
                Levels::Rip(maps) => Levels::Rip(RipMaps {
                    map_data: maps.map_data.iter().map(|level| level.create_writer(header)).collect(),
                    level_count: maps.level_count
                })
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LevelsWriter<S> {
    levels: Levels<S>,
}

impl<S> SamplesWriter for LevelsWriter<S> where S: SamplesWriter {
    fn extract_line(&self, header: &Header, line: LineRefMut<'_>) {
        self.levels.get_level(line.location.level).expect("invalid level index")
            .extract_line(header, line)
    }
}
