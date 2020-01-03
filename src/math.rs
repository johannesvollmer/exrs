
// calculations inspired by
// https://github.com/AcademySoftwareFoundation/openexr/blob/master/OpenEXR/IlmImf/ImfTiledMisc.cpp


use crate::compression::Compression;
use crate::meta::attributes::{I32Box2, TileDescription};

/// computes floor(log(x)/log(2))
pub fn floor_log_2(mut number: u32) -> u32 {
    debug_assert_ne!(number, 0);

    let mut log = 0;

//     TODO check if this unrolls properly?
    while number > 1 {
        log += 1;
        number >>= 1;
    }

    log
}


/// computes ceil(log(x)/log(2))
// taken from https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfTiledMisc.cpp
pub fn ceil_log_2(mut number: u32) -> u32 {
    debug_assert_ne!(number, 0);

    let mut log = 0;
    let mut round_up = 0;

    // TODO check if this unrolls properly
    while number > 1 {
        if number & 1 != 0 {
            round_up = 1;
        }

        log +=  1;
        number >>= 1;
    }

    log + round_up
}



#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RoundingMode {
    Down, Up,
}

impl RoundingMode {
    pub fn log2(self, number: u32) -> u32 {
        match self {
            RoundingMode::Down => self::floor_log_2(number),
            RoundingMode::Up => self::ceil_log_2(number),
        }
    }

    pub fn divide(self, dividend: u32, divisor: u32) -> u32 {
        match self {
            RoundingMode::Up => (dividend + divisor - 1) / divisor, // only works for positive numbers
            RoundingMode::Down => dividend / divisor,
        }
    }
}

pub fn compute_tile_count(full_res: u32, tile_size: u32) -> u32 {
    // round up, because if the image is not evenly divisible by the tiles,
    // we add another tile at the end (which is only partially used)
    RoundingMode::Up.divide(full_res, tile_size)
}


// TODO this should be cached? log2 may be very expensive
pub fn compute_level_count(round: RoundingMode, full_res: u32) -> u32 {
    round.log2(full_res) + 1
}

pub fn compute_level_size(round: RoundingMode, full_res: u32, level_index: u32) -> u32 {
    round.divide(full_res,  1 << level_index).max(1)
}

// TODO cache these?
// TODO compute these directly instead of summing up an iterator?
pub fn rip_map_resolutions(round: RoundingMode, max_resolution: (u32, u32)) -> impl Iterator<Item=(u32, u32)> {
    let (w, h) = (compute_level_count(round, max_resolution.0), compute_level_count(round, max_resolution.1));

    (0..w) // TODO test this
        .flat_map(move |x_level|{ // FIXME may swap y and x order?
            (0..h).map(move |y_level| {
                // TODO progressively divide instead??
                let width = compute_level_size(round, max_resolution.0, x_level);
                let height = compute_level_size(round, max_resolution.1, y_level);
                (width, height)
            })
        })
}

// TODO cache all these level values when computing table offset size??
// TODO compute these directly instead of summing up an iterator?
pub fn mip_map_resolutions(round: RoundingMode, max_resolution: (u32, u32)) -> impl Iterator<Item=(u32, u32)> {
    (0..compute_level_count(round, max_resolution.0.max(max_resolution.1)))
        .map(move |level|{
            // TODO progressively divide instead??
            let width = compute_level_size(round, max_resolution.0, level);
            let height = compute_level_size(round, max_resolution.1, level);
            (width, height)
        })
}


pub fn compute_chunk_count(compression: Compression, data_window: I32Box2, tiles: Option<TileDescription>) -> crate::error::Result<u32> {
    // If not multipart and chunkCount not present,
    // the number of entries in the chunk table is computed
    // using the dataWindow and tileDesc attributes and the compression format
    data_window.validate(None)?;

    let data_size = data_window.dimensions();

    if let Some(tiles) = tiles {
        let round = tiles.rounding_mode;
        let (tile_width, tile_height) = tiles.tile_size;

        // TODO cache all these level values??
        use crate::meta::attributes::LevelMode::*;
        Ok(match tiles.level_mode {
            Singular => {
                let tiles_x = compute_tile_count(data_size.0, tile_width);
                let tiles_y = compute_tile_count(data_size.1, tile_height);
                tiles_x * tiles_y
            }

            MipMap => {
                mip_map_resolutions(round, data_size).map(|(level_width, level_height)| {
                    compute_tile_count(level_width, tile_width) * compute_tile_count(level_height, tile_height)
                }).sum()
            },

            RipMap => {
                // TODO test this
                rip_map_resolutions(round, data_size).map(|(level_width, level_height)| {
                    compute_tile_count(level_width, tile_width) * compute_tile_count(level_height, tile_height)
                }).sum()
            }
        })
    }

    // scan line blocks never have mip maps // TODO check if this is true
    else {
        Ok(compute_tile_count(data_size.1, compression.scan_lines_per_block() as u32))
    }
}