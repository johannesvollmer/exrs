//! Channel data structures for DWA codec (ported from OpenEXR Core internal_dwa_channeldata.h)

use crate::meta::attribute::{ChannelList, SampleType};

#[allow(dead_code)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum DwaPixelType {
    F16 = 0,
    F32 = 1,
    U32 = 2,
}

impl From<SampleType> for DwaPixelType {
    fn from(s: SampleType) -> Self {
        match s {
            SampleType::F16 => DwaPixelType::F16,
            SampleType::F32 => DwaPixelType::F32,
            SampleType::U32 => DwaPixelType::U32,
        }
    }
}

impl Default for DwaPixelType {
    fn default() -> Self { DwaPixelType::F16 }
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub(crate) struct ChannelInfo {
    pub name_index: u32,
    pub sampling_x: u32,
    pub sampling_y: u32,
    pub pixel_type: DwaPixelType,
}

#[allow(dead_code)]
pub(crate) fn collect_channel_infos(channels: &ChannelList) -> Vec<ChannelInfo> {
    let mut v = Vec::with_capacity(channels.list.len());
    for (idx, ch) in channels.list.iter().enumerate() {
        v.push(ChannelInfo {
            name_index: idx as u32,
            sampling_x: ch.sampling.x() as u32,
            sampling_y: ch.sampling.y() as u32,
            pixel_type: DwaPixelType::from(ch.sample_type),
        });
    }
    v
}
