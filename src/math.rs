
// calculations inspired by
// https://github.com/AcademySoftwareFoundation/openexr/blob/master/OpenEXR/IlmImf/ImfTiledMisc.cpp


use crate::compression::Compression;
use crate::meta::attributes::Box2I32;
use std::convert::TryFrom;
use crate::error::{i32_to_u32, i32_to_usize};
use crate::error::Result;
use crate::meta::Blocks;


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Vec2<T> (pub T, pub T);

impl<T> Vec2<T> {
    pub fn map<B>(self, map: impl Fn(T) -> B) -> Vec2<B> {
        Vec2(map(self.0), map(self.1))
    }

    pub fn try_from<S>(value: Vec2<S>) -> std::result::Result<Self, T::Error> where T: TryFrom<S> {
        let x = T::try_from(value.0)?;
        let y = T::try_from(value.1)?;
        Ok(Vec2(x, y))
    }

    pub fn area(self) -> T where T: std::ops::Mul<T, Output = T> {
        self.0 * self.1
    }
}



impl Vec2<i32> {
    pub fn to_usize(self) -> Result<Vec2<usize>> {
        let x = i32_to_usize(self.0)?;
        let y = i32_to_usize(self.1)?;
        Ok(Vec2(x, y))
    }

    pub fn to_u32(self) -> Result<Vec2<u32>> {
        let x = i32_to_u32(self.0)?;
        let y = i32_to_u32(self.1)?;
        Ok(Vec2(x, y))
    }
}

impl<T: std::ops::Add<T>> std::ops::Add<Vec2<T>> for Vec2<T> {
    type Output = Vec2<T::Output>;
    fn add(self, other: Vec2<T>) -> Self::Output {
        Vec2(self.0 + other.0, self.1 + other.1)
    }
}

impl<T: std::ops::Sub<T>> std::ops::Sub<Vec2<T>> for Vec2<T> {
    type Output = Vec2<T::Output>;
    fn sub(self, other: Vec2<T>) -> Self::Output {
        Vec2(self.0 - other.0, self.1 - other.1)
    }
}

impl<T: std::ops::Div<T>> std::ops::Div<Vec2<T>> for Vec2<T> {
    type Output = Vec2<T::Output>;
    fn div(self, other: Vec2<T>) -> Self::Output {
        Vec2(self.0 / other.0, self.1 / other.1)
    }
}

impl<T: std::ops::Mul<T>> std::ops::Mul<Vec2<T>> for Vec2<T> {
    type Output = Vec2<T::Output>;
    fn mul(self, other: Vec2<T>) -> Self::Output {
        Vec2(self.0 * other.0, self.1 * other.1)
    }
}


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
    // debug_assert!(level_index < compute_level_count(round, full_res), "level index {} too large for resolution {}", level_index, full_res);

    round.divide(full_res,  1 << level_index).max(1)
}

// TODO cache these?
// TODO compute these directly instead of summing up an iterator?
pub fn rip_map_levels(round: RoundingMode, max_resolution: Vec2<u32>) -> impl Iterator<Item=(Vec2<u32>, Vec2<u32>)> {
    rip_map_indices(round, max_resolution).map(move |level_indices|{
        // TODO progressively divide instead??
        let width = compute_level_size(round, max_resolution.0, level_indices.0);
        let height = compute_level_size(round, max_resolution.1, level_indices.1);
        (level_indices, Vec2(width, height))
    })
}

// TODO cache all these level values when computing table offset size??
// TODO compute these directly instead of summing up an iterator?
pub fn mip_map_levels(round: RoundingMode, max_resolution: Vec2<u32>) -> impl Iterator<Item=(u32, Vec2<u32>)> {
    mip_map_indices(round, max_resolution)
        .map(move |level_index|{
            // TODO progressively divide instead??
            let width = compute_level_size(round, max_resolution.0, level_index);
            let height = compute_level_size(round, max_resolution.1, level_index);
            (level_index, Vec2(width, height))
        })
}

pub fn rip_map_indices(round: RoundingMode, max_resolution: Vec2<u32>) -> impl Iterator<Item=Vec2<u32>> {
    let (width, height) = (
        compute_level_count(round, max_resolution.0),
        compute_level_count(round, max_resolution.1)
    );

    (0..height).flat_map(move |y_level|{
        (0..width).map(move |x_level|{
            Vec2(x_level, y_level)
        })
    })
}

pub fn mip_map_indices(round: RoundingMode, max_resolution: Vec2<u32>) -> impl Iterator<Item=u32> {
    (0..compute_level_count(round, max_resolution.0.max(max_resolution.1)))
}

pub fn compute_chunk_count(compression: Compression, data_window: Box2I32, blocks: Blocks) -> crate::error::Result<u32> {
    // If not multipart and chunkCount not present,
    // the number of entries in the chunk table is computed
    // using the dataWindow and tileDesc attributes and the compression format
    let data_size = data_window.size;

    if let Blocks::Tiles(tiles) = blocks {
        let round = tiles.rounding_mode;
        let Vec2(tile_width, tile_height) = tiles.tile_size;

        // TODO cache all these level values??
        use crate::meta::attributes::LevelMode::*;
        Ok(match tiles.level_mode {
            Singular => {
                let tiles_x = compute_tile_count(data_size.0, tile_width);
                let tiles_y = compute_tile_count(data_size.1, tile_height);
                tiles_x * tiles_y
            }

            MipMap => {
                mip_map_levels(round, data_size).map(|(_, Vec2(level_width, level_height))| {
                    compute_tile_count(level_width, tile_width) * compute_tile_count(level_height, tile_height)
                }).sum()
            },

            RipMap => {
                // TODO test this
                rip_map_levels(round, data_size).map(|(_, Vec2(level_width, level_height))| {
                    compute_tile_count(level_width, tile_width) * compute_tile_count(level_height, tile_height)
                }).sum()
            }
        })
    }

    // scan line blocks never have mip maps // TODO check if this is true
    else {
        Ok(compute_tile_count(data_size.1, compression.scan_lines_per_block()))
    }
}