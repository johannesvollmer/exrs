
//! The `file` module represents the file how it is laid out in memory.


use crate::file::meta::attributes::RoundingMode;

pub mod io;
pub mod meta;
pub mod data;


pub fn rip_map_resolutions(round: RoundingMode, max_resolution: (u32, u32)) -> impl Iterator<Item=(u32, u32)> {
    let (w, h) = (compute_level_count(round, max_resolution.0), compute_level_count(round, max_resolution.1));

    (0..w) // TODO test this
        .flat_map(move |x_level|{ // TODO may swap y and x?
            (0..h).map(move |y_level| {
                // TODO progressively divide instead??
                let width = compute_level_size(round, max_resolution.0, x_level);
                let height = compute_level_size(round, max_resolution.1, y_level);
                (width, height)
            })
        })
}

// TODO cache all these level values when computing table offset size??
pub fn mip_map_resolutions(round: RoundingMode, max_resolution: (u32, u32)) -> impl Iterator<Item=(u32, u32)> {
    (0..compute_level_count(round, max_resolution.0.max(max_resolution.1)))
        .map(move |level|{
            // TODO progressively divide instead??
            let width = compute_level_size(round, max_resolution.0, level);
            let height = compute_level_size(round, max_resolution.1, level);
            (width, height)
        })
}



// calculations inspired by
// https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfTiledMisc.cpp

pub fn compute_tile_count(full_res: u32, tile_size: u32) -> u32 {
    // round up, because if the image is not evenly divisible by the tiles,
    // we add another tile at the end (which is only partially used)
    RoundingMode::Up.divide(full_res, tile_size)
}

pub fn compute_scan_line_block_count(height: u32, block_size: u32) -> u32 {
    // round up, because if the image is not evenly divisible by the block size,
    // we add another block at the end (which is only partially used)
    RoundingMode::Up.divide(height, block_size)
}

// TODO this should be cached? log2 may be very expensive
pub fn compute_level_count(round: RoundingMode, full_res: u32) -> u32 {
    round.log2(full_res) + 1
}

pub fn compute_level_size(round: RoundingMode, full_res: u32, level_index: u32) -> u32 {
    round.divide(full_res,  1 << level_index).max(1)
}










