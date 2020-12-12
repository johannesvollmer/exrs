use crate::meta::attribute::{LevelMode, ChannelInfo, SampleType, ChannelList};
use smallvec::SmallVec;
use crate::meta::header::Header;
use crate::block::BlockIndex;
use crate::image::{AnyChannels, RgbaChannels, RgbaPixel, RgbaSampleTypes};
use crate::block::lines::{LineIndex, LineRefMut};
use crate::math::Vec2;
use crate::prelude::TryInto;
use crate::io::Write;
use crate::block::samples::Sample;
use std::io::Cursor;
use crate::image::write::samples::{WritableSamples, SamplesWriter};


pub trait WritableChannels<'s> {
    fn infer_channel_list(&self) -> ChannelList;
    fn level_mode(&self) -> LevelMode;

    type Writer: ChannelsWriter;
    fn create_writer(&'s self, header: &Header) -> Self::Writer;
}

pub trait ChannelsWriter: Sync {
    fn extract_uncompressed_block(&self, header: &Header, block: BlockIndex) -> Vec<u8>;
}



pub trait GetRgbaPixel: Sync { // TODO allow fnmut?
    fn get_pixel(&self, position: Vec2<usize>) -> RgbaPixel;
}

impl<F> GetRgbaPixel for F where F: Sync + Fn(Vec2<usize>) -> RgbaPixel {
    fn get_pixel(&self, position: Vec2<usize>) -> RgbaPixel { self(position) }
}

impl<'s, S: 's + WritableSamples<'s>> WritableChannels<'s> for AnyChannels<S> {
    fn level_mode(&self) -> LevelMode {
        let mode = self.iter().next().unwrap().sample_data.level_mode();

        debug_assert!(
            std::iter::repeat(mode).zip(self.iter().skip(1))
                .all(|(first, other)| other.sample_data.level_mode() == first)
        );

        mode
    }

    fn infer_channel_list(&self) -> ChannelList {
        ChannelList::new(self.iter().map(|channel| ChannelInfo {
            name: channel.name.clone(),
            sample_type: channel.sample_data.sample_type(),
            quantize_linearly: channel.quantize_linearly,
            sampling: channel.sampling
        }).collect())
    }

    type Writer = AnyChannelsWriter<S::Writer>;
    fn create_writer(&'s self, header: &Header) -> Self::Writer {
        let channels = self.iter().map(|chan| chan.sample_data.create_writer(header)).collect();
        AnyChannelsWriter { channels }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AnyChannelsWriter<S> {
    channels: SmallVec<[S; 4]>
}

impl<S> ChannelsWriter for AnyChannelsWriter<S> where S: SamplesWriter {
    fn extract_uncompressed_block(&self, header: &Header, block_index: BlockIndex) -> Vec<u8> {
        let byte_count = block_index.pixel_size.area() * header.channels.bytes_per_pixel;
        let mut block_bytes = vec![0_u8; byte_count];

        for (byte_range, line_index) in LineIndex::lines_in_block(block_index, header) {
            self.channels.get(line_index.channel).unwrap().extract_line(header, LineRefMut { // TODO subsampling
                value: &mut block_bytes[byte_range],
                location: line_index,
            });
        }

        block_bytes
    }
}




impl<'f, F: 'f> WritableChannels<'f> for RgbaChannels<F> where F: GetRgbaPixel {
    fn infer_channel_list(&self) -> ChannelList {
        let r = ChannelInfo::new("R".try_into().unwrap(), self.sample_types.0, false); // FIXME TODO sampling!
        let g = ChannelInfo::new("G".try_into().unwrap(), self.sample_types.1, false);
        let b = ChannelInfo::new("B".try_into().unwrap(), self.sample_types.2, false);
        let a = self.sample_types.3.map(|ty| ChannelInfo::new("A".try_into().unwrap(), ty, true));

        // TODO Rgb__Channels and Rgb_A_Channels as separate writers?
        ChannelList::new(if let Some(a) = a {
            smallvec![ a, b, g, r ]
        }
        else {
            smallvec![ b, g, r ]
        })
        // ChannelList::new(a.map(|a| smallvec![ a, b, g, r ]).unwrap_or_else(|| smallvec![ b, g, r ]))
    }

    fn level_mode(&self) -> LevelMode { LevelMode::Singular }

    type Writer = RgbaChannelsWriter<'f, F>;
    fn create_writer(&'f self, _: &Header) -> Self::Writer {
        RgbaChannelsWriter { rgba: self }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RgbaChannelsWriter<'f, F> where F: GetRgbaPixel {
    rgba: &'f RgbaChannels<F>, // TODO this need not be a reference??
}

impl<'f, F> ChannelsWriter for RgbaChannelsWriter<'f, F> where F: GetRgbaPixel {
    fn extract_uncompressed_block(&self, header: &Header, block_index: BlockIndex) -> Vec<u8> {
        let block_bytes = block_index.pixel_size.area() * header.channels.bytes_per_pixel;

        let width = block_index.pixel_size.0;
        let line_bytes = width * header.channels.bytes_per_pixel;

        // alpha would always start at 0, then comes b, g, r.
        let RgbaSampleTypes(r_type, g_type, b_type, a_type) = self.rgba.sample_types;
        let r_line_bytes = width * r_type.bytes_per_sample();
        let g_line_bytes = width * g_type.bytes_per_sample();
        let b_line_bytes = width * b_type.bytes_per_sample();
        let a_line_bytes = a_type
            .map(|a_type| width * a_type.bytes_per_sample())
            .unwrap_or(0);

        let mut block_bytes = vec![0_u8; block_bytes];

        let y_coordinates = 0..block_index.pixel_size.height();
        let byte_lines = block_bytes.chunks_exact_mut(line_bytes);
        for (y, line_bytes) in y_coordinates.zip(byte_lines) {

            let (a, line_bytes) = line_bytes.split_at_mut(a_line_bytes);
            let (b, line_bytes) = line_bytes.split_at_mut(b_line_bytes);
            let (g, line_bytes) = line_bytes.split_at_mut(g_line_bytes);
            let (r, line_bytes) = line_bytes.split_at_mut(r_line_bytes);
            debug_assert!(line_bytes.is_empty());

            fn sample_writer(sample_type: SampleType, mut write: impl Write) -> impl FnMut(Sample) {
                use crate::io::Data;

                move |sample| {
                    match sample_type {
                        SampleType::F16 => sample.to_f16().write(&mut write).expect("write to buffer error"),
                        SampleType::F32 => sample.to_f32().write(&mut write).expect("write to buffer error"),
                        SampleType::U32 => sample.to_u32().write(&mut write).expect("write to buffer error"),
                    }
                }
            }

            let mut write_r = sample_writer(r_type, Cursor::new(r));
            let mut write_g = sample_writer(g_type, Cursor::new(g));
            let mut write_b = sample_writer(b_type, Cursor::new(b));
            let mut write_a = a_type.map(|a_type| sample_writer(a_type, Cursor::new(a)));

            for x in 0..width {
                let position = block_index.pixel_position + Vec2(x,y);
                let pixel: RgbaPixel = self.rgba.storage.get_pixel(position);

                write_r(pixel.red);
                write_g(pixel.green);
                write_b(pixel.blue);

                if let Some(write_a) = &mut write_a {
                    write_a(pixel.alpha_or_default()); // no alpha channel provided = not transparent
                }
            }
        }

        block_bytes
    }
}























