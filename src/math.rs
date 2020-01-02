
// calculations inspired by
// https://github.com/AcademySoftwareFoundation/openexr/blob/master/OpenEXR/IlmImf/ImfTiledMisc.cpp


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
