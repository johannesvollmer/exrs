
// TODO use thread_local static to avoid memory usage?
/*
use std::convert::TryInto;
use std::ops::IndexMut;

pub const DWA_COMPRESSOR_NO_OP: AllU16Array = gen_no_op_linear_le_u16_table();
pub const DWA_COMPRESSOR_TO_LINEAR: AllU16Array = gen_ne_to_linear_le_u16_table();
pub const DWA_COMPRESSOR_TO_NONLINEAR: AllU16Array = gen_le_to_non_linear_ne_u16_table();
pub const CLOSEST_DATA_OFFSET: AllU16Array = gen_data_offset();
pub const CLOSEST_DATA: AllU16Array = gen_closest_data();


type AllU16Array = [u16; u16::MAX as usize];

// note: taken from dwaLookup.cpp, not DwaCompressor.cpp
const fn gen_no_op_linear_le_u16_table() -> AllU16Array {
    unimplemented!()
    // (0 .. u16::MAX).map(|u| u.to_le()).collect()
}

const fn gen_ne_to_linear_le_u16_table() -> AllU16Array {
    unimplemented!()
}

const fn gen_le_to_non_linear_ne_u16_table() -> AllU16Array {
    unimplemented!()
}





*/
