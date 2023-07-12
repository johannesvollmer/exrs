// see https://github.com/AcademySoftwareFoundation/openexr/blob/main/src/lib/OpenEXR/ImfDwaCompressorSimd.h
// TODO SIMD
use std::f32::consts::PI;
use half::prelude::*;

#[cfg(test)]
pub mod test {
    use std::convert::TryInto;
    use half::slice::{HalfBitsSliceExt, HalfFloatSliceExt};
    use rand::random;
    use crate::image::validate_results::ValidateResult;
    use super::*;

    #[test]
    pub fn interleave() {
        let mut dst = [0,0,0,0];
        interleave_byte2(&mut dst, &[5, 8], &[3, 2]);// TODO reuse simd impl from compression module??
        assert_eq!(dst, [5,3,8,2]);
    }

    #[test]
    pub fn round_trip_dct() {
        let input: [f32; 8 * 8] = rand_8x8_f32();
        let mut result = input.clone();

        for _ in 0..9 {
            dct_forward_8x8(&mut result);
            dct_inverse_8x8(&mut result, 0);
        }

        input.as_slice().assert_approx_equals_result(&result.as_slice());
    }

    #[test]
    pub fn round_trip_csc709() {
        let input_a: [f32; 8 * 8] = rand_8x8_f32();
        let input_b: [f32; 8 * 8] = rand_8x8_f32();
        let input_c: [f32; 8 * 8] = rand_8x8_f32();

        let mut result_a = input_a.clone();
        let mut result_b = input_b.clone();
        let mut result_c = input_c.clone();

        for _ in 0..9 {
            csc709_forward(&mut result_a, &mut result_b, &mut result_c);
            csc709_inverse(&mut result_a, &mut result_b, &mut result_c);
        }

        input_a.as_slice().assert_approx_equals_result(&result_a.as_slice());
        input_b.as_slice().assert_approx_equals_result(&result_b.as_slice());
        input_c.as_slice().assert_approx_equals_result(&result_c.as_slice());
    }

    #[test]
    fn roundtrip_zigzag(){
        let input = rand_8x8_f32();

        let mut tmp_zigzag_u16 = [0_u16; 8*8];
        f32_to_zig_zag_f16(&mut tmp_zigzag_u16, &input);

        let mut un_zigzagged = [1.0_f32; 8*8];
        f32_from_zig_zag_f16(&tmp_zigzag_u16, &mut un_zigzagged);

        input.as_slice().assert_approx_equals_result(&un_zigzagged.as_slice());
    }

    fn rand_8x8_f32() -> [f32; 64] {
        (0..8 * 8)
            .map(|_| 31.0 * random::<f32>())
            .collect::<Vec<_>>().try_into().unwrap()
    }
}


/// Forward 709 CSC, R'G'B' -> Y'CbCr
pub fn csc709_forward(a: &mut [f32; 8 * 8], b: &mut [f32; 8 * 8], c: &mut [f32; 8 * 8]) {
    for ((a, b), c) in a.iter_mut().zip(b).zip(c) {
        let (va, vb, vc) = (*a, *b, *c);
        *a = 0.2126 * va + 0.7152 * vb + 0.0722 * vc;
        *b = -0.1146 * va - 0.3854 * vb + 0.5000 * vc;
        *c = 0.5000 * va - 0.4542 * vb - 0.0458 * vc;
    }
}

/// Inverse 709 CSC, Y'CbCr -> R'G'B'
pub fn csc709_inverse(a: &mut [f32; 8 * 8], b: &mut [f32; 8 * 8], c: &mut [f32; 8 * 8]) {
    for ((a, b), c) in a.iter_mut().zip(b).zip(c) {
        let src = [*a, *b, *c];
        *a = src[0] + 1.5747 * src[2];
        *b = src[0] - 0.1873 * src[1] - 0.4682 * src[2];
        *c = src[0] + 1.8556 * src[1];
    }
}

fn _dct_inverse_8x8_dc_only(data: &mut [f32; 8 * 8]) {
    let val = data[0] * 3.535536e-01 * 3.535536e-01;
    data.fill(val);
}

#[inline]
pub fn interleave_byte2(dst: &mut [u8], src0: &[u8], src1: &[u8]) {
    let interleaved = src0.iter().zip(src1).flat_map(|(a, b)| [*a,*b]);
    for (slot, val) in dst.iter_mut().zip(interleaved) { *slot = val; }
}


pub fn f32_to_zig_zag_f16(destination: &mut [u16; 8*8], source: &[f32; 8*8]) {
    let mut source_u16 = [0_u16; 8*8];
    f32_to_uf16(&source, &mut source_u16);
    to_zig_zag(destination, &source_u16);
}

fn f32_to_uf16(source: &[f32; 8*8], destination_f16: &mut [u16; 8*8]){
    let destination_f16: &mut [f16] = destination_f16.reinterpret_cast_mut();
    destination_f16.convert_from_f32_slice(source);
}

pub fn to_zig_zag<T>(destination: &mut [T; 8*8], source: &[T; 8*8]) where T: Copy {
    const REMAP: [usize; 8*8] =  [
        0,  1,  8,  16, 9,  2,  3,  10, 17, 24, 32, 25, 18,
        11, 4,  5,  12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
        13, 6,  7,  14, 21, 28, 35, 42, 49, 56, 57, 50, 43,
        36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45,
        38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63
    ];

    for (slot, index_in_src) in destination.iter_mut().zip(REMAP) {
        *slot = source[index_in_src];
    }
}

pub fn f32_from_zig_zag_f16(src: &[u16; 8 * 8], dst: &mut [f32; 8 * 8]) {
    fn to_f32(half: u16) -> f32 { f16::to_f32(f16::from_bits(half)) }

    dst[0] = to_f32(src[0]);
    dst[1] = to_f32(src[1]);
    dst[2] = to_f32(src[5]);
    dst[3] = to_f32(src[6]);
    dst[4] = to_f32(src[14]);
    dst[5] = to_f32(src[15]);
    dst[6] = to_f32(src[27]);
    dst[7] = to_f32(src[28]);
    dst[8] = to_f32(src[2]);
    dst[9] = to_f32(src[4]);

    dst[10] = to_f32(src[7]);
    dst[11] = to_f32(src[13]);
    dst[12] = to_f32(src[16]);
    dst[13] = to_f32(src[26]);
    dst[14] = to_f32(src[29]);
    dst[15] = to_f32(src[42]);
    dst[16] = to_f32(src[3]);
    dst[17] = to_f32(src[8]);
    dst[18] = to_f32(src[12]);
    dst[19] = to_f32(src[17]);

    dst[20] = to_f32(src[25]);
    dst[21] = to_f32(src[30]);
    dst[22] = to_f32(src[41]);
    dst[23] = to_f32(src[43]);
    dst[24] = to_f32(src[9]);
    dst[25] = to_f32(src[11]);
    dst[26] = to_f32(src[18]);
    dst[27] = to_f32(src[24]);
    dst[28] = to_f32(src[31]);
    dst[29] = to_f32(src[40]);

    dst[30] = to_f32(src[44]);
    dst[31] = to_f32(src[53]);
    dst[32] = to_f32(src[10]);
    dst[33] = to_f32(src[19]);
    dst[34] = to_f32(src[23]);
    dst[35] = to_f32(src[32]);
    dst[36] = to_f32(src[39]);
    dst[37] = to_f32(src[45]);
    dst[38] = to_f32(src[52]);
    dst[39] = to_f32(src[54]);

    dst[40] = to_f32(src[20]);
    dst[41] = to_f32(src[22]);
    dst[42] = to_f32(src[33]);
    dst[43] = to_f32(src[38]);
    dst[44] = to_f32(src[46]);
    dst[45] = to_f32(src[51]);
    dst[46] = to_f32(src[55]);
    dst[47] = to_f32(src[60]);
    dst[48] = to_f32(src[21]);
    dst[49] = to_f32(src[34]);

    dst[50] = to_f32(src[37]);
    dst[51] = to_f32(src[47]);
    dst[52] = to_f32(src[50]);
    dst[53] = to_f32(src[56]);
    dst[54] = to_f32(src[59]);
    dst[55] = to_f32(src[61]);
    dst[56] = to_f32(src[35]);
    dst[57] = to_f32(src[36]);
    dst[58] = to_f32(src[48]);
    dst[59] = to_f32(src[49]);

    dst[60] = to_f32(src[57]);
    dst[61] = to_f32(src[58]);
    dst[62] = to_f32(src[62]);
    dst[63] = to_f32(src[63]);
}

// https://github.com/AcademySoftwareFoundation/openexr/blob/main/src/lib/OpenEXR/ImfDwaCompressorSimd.h#L930C1-L1031
pub fn dct_inverse_8x8(data: &mut [f32; 8 * 8], zeroed_rows: usize) {
    let a: f32 = 0.5 * cos(PI / 4.0);
    let b: f32 = 0.5 * cos(PI / 16.0);
    let c: f32 = 0.5 * cos(PI / 8.0);
    let d: f32 = 0.5 * cos(3.0 * PI / 16.0);
    let e: f32 = 0.5 * cos(5.0 * PI / 16.0);
    let f: f32 = 0.5 * cos(3.0 * PI / 8.0);
    let g: f32 = 0.5 * cos(7.0 * PI / 16.0);

    for row in 0..8 - zeroed_rows {
        let row_ptr = &mut data[row * 8..(row + 1) * 8];
        let mut alpha = [0.0, 0.0, 0.0, 0.0];
        let mut beta = [0.0, 0.0, 0.0, 0.0];
        let mut theta = [0.0, 0.0, 0.0, 0.0];
        let mut gamma = [0.0, 0.0, 0.0, 0.0];

        alpha[0] = c * row_ptr[2];
        alpha[1] = f * row_ptr[2];
        alpha[2] = c * row_ptr[6];
        alpha[3] = f * row_ptr[6];

        beta[0] = b * row_ptr[1] + d * row_ptr[3] + e * row_ptr[5] + g * row_ptr[7];
        beta[1] = d * row_ptr[1] - g * row_ptr[3] - b * row_ptr[5] - e * row_ptr[7];
        beta[2] = e * row_ptr[1] - b * row_ptr[3] + g * row_ptr[5] + d * row_ptr[7];
        beta[3] = g * row_ptr[1] - e * row_ptr[3] + d * row_ptr[5] - b * row_ptr[7];

        theta[0] = a * (row_ptr[0] + row_ptr[4]);
        theta[3] = a * (row_ptr[0] - row_ptr[4]);
        theta[1] = alpha[0] + alpha[3];
        theta[2] = alpha[1] - alpha[2];

        gamma[0] = theta[0] + theta[1];
        gamma[1] = theta[3] + theta[2];
        gamma[2] = theta[3] - theta[2];
        gamma[3] = theta[0] - theta[1];

        row_ptr[0] = gamma[0] + beta[0];
        row_ptr[1] = gamma[1] + beta[1];
        row_ptr[2] = gamma[2] + beta[2];
        row_ptr[3] = gamma[3] + beta[3];
        row_ptr[4] = gamma[3] - beta[3];
        row_ptr[5] = gamma[2] - beta[2];
        row_ptr[6] = gamma[1] - beta[1];
        row_ptr[7] = gamma[0] - beta[0];
    }

    for column in 0..8 {
        let mut alpha = [0.0, 0.0, 0.0, 0.0];
        let mut beta = [0.0, 0.0, 0.0, 0.0];
        let mut theta = [0.0, 0.0, 0.0, 0.0];
        let mut gamma = [0.0, 0.0, 0.0, 0.0];

        alpha[0] = c * data[16 + column];
        alpha[1] = f * data[16 + column];
        alpha[2] = c * data[48 + column];
        alpha[3] = f * data[48 + column];

        beta[0] = b * data[8 + column] + d * data[24 + column] + e * data[40 + column] + g * data[56 + column];
        beta[1] = d * data[8 + column] - g * data[24 + column] - b * data[40 + column] - e * data[56 + column];
        beta[2] = e * data[8 + column] - b * data[24 + column] + g * data[40 + column] + d * data[56 + column];
        beta[3] = g * data[8 + column] - e * data[24 + column] + d * data[40 + column] - b * data[56 + column];

        theta[0] = a * (data[column] + data[32 + column]);
        theta[3] = a * (data[column] - data[32 + column]);
        theta[1] = alpha[0] + alpha[3];
        theta[2] = alpha[1] - alpha[2];

        gamma[0] = theta[0] + theta[1];
        gamma[1] = theta[3] + theta[2];
        gamma[2] = theta[3] - theta[2];
        gamma[3] = theta[0] - theta[1];

        data[0 + column] = gamma[0] + beta[0];
        data[8 + column] = gamma[1] + beta[1];
        data[16 + column] = gamma[2] + beta[2];
        data[24 + column] = gamma[3] + beta[3];
        data[32 + column] = gamma[3] - beta[3];
        data[40 + column] = gamma[2] - beta[2];
        data[48 + column] = gamma[1] - beta[1];
        data[56 + column] = gamma[0] - beta[0];
    }
}

// https://github.com/AcademySoftwareFoundation/openexr/blob/main/src/lib/OpenEXR/ImfDwaCompressorSimd.h#L1815-L1984
pub fn dct_forward_8x8(data: &mut [f32; 8 * 8]) {
    let c1: f32 = cos(PI * 1.0 / 16.0);
    let c2: f32 = cos(PI * 2.0 / 16.0);
    let c3: f32 = cos(PI * 3.0 / 16.0);
    let c4: f32 = cos(PI * 4.0 / 16.0);
    let c5: f32 = cos(PI * 5.0 / 16.0);
    let c6: f32 = cos(PI * 6.0 / 16.0);
    let c7: f32 = cos(PI * 7.0 / 16.0);

    let c1half: f32 = 0.5 * c1;
    let c2half: f32 = 0.5 * c2;
    let c3half: f32 = 0.5 * c3;
    let c5half: f32 = 0.5 * c5;
    let c6half: f32 = 0.5 * c6;
    let c7half: f32 = 0.5 * c7;

    for row in 0..8 { // TODO iter rows using chunks_exact()
        let row = &mut data[8 * row..8 * (1 + row)];
        let a0 = row[0] + row[7];
        let a1 = row[1] + row[2];
        let a2 = row[1] - row[2];
        let a3 = row[3] + row[4];
        let a4 = row[3] - row[4];
        let a5 = row[5] + row[6];
        let a6 = row[5] - row[6];
        let a7 = row[0] - row[7];

        let k0 = c4 * (a0 + a3);
        let k1 = c4 * (a1 + a5);
        let row_0 = 0.5 * (k0 + k1);
        let row_4 = 0.5 * (k0 - k1);

        let rot_x = a2 - a6;
        let rot_y = a0 - a3;
        let row_2 = c6half * rot_x + c2half * rot_y;
        let row_6 = c6half * rot_y - c2half * rot_x;

        let k0 = c4 * (a1 - a5);
        let k1 = -1.0 * c4 * (a2 + a6);

        let rot_x = a7 - k0;
        let rot_y = a4 + k1;
        let row_3 = c3half * rot_x - c5half * rot_y;
        let row_5 = c5half * rot_x + c3half * rot_y;

        let rot_x = a7 + k0;
        let rot_y = k1 - a4;
        let row_1 = c1half * rot_x - c7half * rot_y;
        let row_7 = c7half * rot_x + c1half * rot_y;

        row.copy_from_slice(&[row_0, row_1, row_2, row_3, row_4, row_5, row_6, row_7]);
    }

    for column in 0..8 { // TODO zip chunks_exact()?
        let a0 = data[0 + column] + data[56 + column];
        let a7 = data[0 + column] - data[56 + column];

        let a1 = data[8 + column] + data[16 + column];
        let a2 = data[8 + column] - data[16 + column];

        let a3 = data[24 + column] + data[32 + column];
        let a4 = data[24 + column] - data[32 + column];

        let a5 = data[40 + column] + data[48 + column];
        let a6 = data[40 + column] - data[48 + column];

        let k0 = c4 * (a0 + a3);
        let k1 = c4 * (a1 + a5);

        let col_0 = 0.5 * (k0 + k1);
        let col_32 = 0.5 * (k0 - k1);

        let rot_x = a2 - a6;
        let rot_y = a0 - a3;
        let col_16 = 0.5 * (c6 * rot_x + c2 * rot_y); // TODO c6half * rot_x + c2half * rot_y;?
        let col_48 = 0.5 * (c6 * rot_y - c2 * rot_x); // TODO as above?

        let k0 = c4 * (a1 - a5);
        let k1 = -1.0 * c4 * (a2 + a6);

        let rot_x = a7 - k0;
        let rot_y = a4 + k1;
        let col_24 = 0.5 * (c3 * rot_x - c5 * rot_y); // TODO as above?
        let col_40 = 0.5 * (c5 * rot_x + c3 * rot_y); // TODO as above?

        let rot_x = a7 + k0;
        let rot_y = k1 - a4;
        let col_8 = 0.5 * (c1 * rot_x - c7 * rot_y); // TODO as above?
        let col_56 = 0.5 * (c7 * rot_x + c1 * rot_y); // TODO as above?

        data[0 + column] = col_0;
        data[8 + column] = col_8;
        data[16 + column] = col_16;
        data[24 + column] = col_24;
        data[32 + column] = col_32;
        data[40 + column] = col_40;
        data[48 + column] = col_48;
        data[56 + column] = col_56;
    }
}

fn cos(x: f32) -> f32 { x.cos() }