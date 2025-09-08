//! DWA coefficient stream scaffolding.
//! This module will contain coefficient unpacking and 8x8 block reconstruction.

use crate::error::{Error, Result};

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub(crate) struct BlockHeader {
    pub block_x: u16,
    pub block_y: u16,
    pub channel_index: u16,
}

#[allow(dead_code)]
pub(crate) fn decompress_blocks(_data: &[u8]) -> Result<()> {
    Err(Error::unsupported("DWA coefficient decoding not yet implemented"))
}
