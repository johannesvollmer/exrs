//! Channel data structures for DWA codec (ported from OpenEXR Core internal_dwa_channeldata.h)

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub(crate) struct ChannelInfo {
    pub name_index: u32,
    pub sampling_x: u32,
    pub sampling_y: u32,
    pub pixel_type: u8, // 0=F16,1=F32,2=U32 to match crate SampleType mapping later
}
