use crate::image::*;
use crate::meta::header::{Header};
use crate::error::{Result, UnitResult};
use crate::block::UncompressedBlock;
use crate::block::lines::{LineRef, LineIndex, LineSlice};
use crate::math::Vec2;
use crate::meta::attribute::{Text, ChannelInfo};
use crate::image::read::layers::{ReadChannels, ChannelsReader, ReadAllLayers, ReadFirstValidLayer};
use crate::block::chunk::TileCoordinates;


#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadAnyChannels<S> {
    pub read_samples: S
}

// FIXME do not throw error on deep data but just skip it!
impl<'s, S:'s> ReadAnyChannels<S> where Self: ReadChannels<'s> {
    pub fn first_valid_layer(self) -> ReadFirstValidLayer<Self> { ReadFirstValidLayer { read_channels: self } }
    pub fn all_layers(self) -> ReadAllLayers<Self> { ReadAllLayers { read_channels: self } }
}

pub trait ReadSamples {
    type Reader: SamplesReader;
    fn create_sample_reader(&self, header: &Header, channel: &ChannelInfo) -> Result<Self::Reader>;
}


/// `S`: Either `AnySamplesReader` or `FlatSamplesReader`
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AnyChannelsReader<S> {

    /// one per channel
    sample_channels_reader: SmallVec<[AnyChannelReader<S>; 4]>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AnyChannelReader<S> {
    samples: S,
    name: Text,
    sampling_rate: Vec2<usize>,
    quantize_linearly: bool,
}

pub trait SamplesReader {
    type Samples: 'static;
    fn read_line(&mut self, line: LineRef<'_>) -> UnitResult;
    fn filter_block(&self, tile: (usize, &TileCoordinates)) -> bool;
    fn into_samples(self) -> Self::Samples;
}


impl<'s, S: 's + ReadSamples> ReadChannels<'s> for ReadAnyChannels<S> {
    type Reader = AnyChannelsReader<S::Reader>;

    fn create_channels_reader(&self, header: &Header) -> Result<Self::Reader> {
        let samples: Result<_> = header.channels.list.iter()
            .map(|channel: &ChannelInfo| Ok(AnyChannelReader {
                samples: self.read_samples.create_sample_reader(header, channel)?,
                name: channel.name.clone(),
                sampling_rate: channel.sampling,
                quantize_linearly: channel.quantize_linearly
            }))
            .collect();

        Ok(AnyChannelsReader { sample_channels_reader: samples? })
    }
}

impl<S: SamplesReader> ChannelsReader for AnyChannelsReader<S> {
    type Channels = AnyChannels<S::Samples>;

    fn read_block(&mut self, header: &Header, decompressed: UncompressedBlock) -> UnitResult {
        for (bytes, line) in LineIndex::lines_in_block(decompressed.index, header) {
            let channel = self.sample_channels_reader.get_mut(line.channel).unwrap();
            channel.samples.read_line(LineSlice { location: line, value: &decompressed.data[bytes] })?;
        }

        Ok(())
    }

    fn filter_block(&self, tile: (usize, &TileCoordinates)) -> bool {
        self.sample_channels_reader.iter().any(|channel| channel.samples.filter_block(tile))
    }

    fn into_channels(self) -> Self::Channels {
        AnyChannels { // not using `new()` as the channels are already sorted
            list: self.sample_channels_reader.into_iter()
                .map(|channel| AnyChannel {
                    sample_data: channel.samples.into_samples(),

                    name: channel.name,
                    quantize_linearly: channel.quantize_linearly,
                    sampling: channel.sampling_rate
                })
                .collect()
        }
    }
}
