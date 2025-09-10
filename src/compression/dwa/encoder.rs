use std::os::raw::{c_int, c_void};
use std::ptr;
use std::mem::size_of;

type uint8_t = u8;
type uint16_t = u16;
type uint32_t = u32;
type uint64_t = u64;
type size_t = usize;
pub type exr_result_t = c_int;

pub const EXR_ERR_SUCCESS: exr_result_t = 0;
pub const EXR_ERR_OUT_OF_MEMORY: exr_result_t = -1;

const _SSE_ALIGNMENT: usize = 16;

/// External helpers expected elsewhere in the port:
extern "C" {
    fn float_to_half(f: f32) -> uint16_t;
    fn half_to_float(h: uint16_t) -> f32;
    fn one_from_native16(v: uint16_t) -> uint16_t;
    fn one_to_native16(v: uint16_t) -> uint16_t;
    fn one_to_native_float(f: f32) -> f32;

    fn dctForward8x8(data: *mut f32);
    fn csc709Forward64(r: *mut f32, g: *mut f32, b: *mut f32);
    fn convertFloatToHalf64(dst: *mut uint16_t, src: *const f32);

    // memory helpers
    // alloc_fn: extern "C" fn(size_t) -> *mut c_void
    // free_fn: extern "C" fn(*mut c_void)
}

/// Minimal stubs for things referenced from previous ports (fill in real definitions elsewhere).
#[repr(C)]
pub struct DctCoderChannelData {
    pub _dctData: *mut f32,
    pub _halfZigData: *mut uint16_t,
    pub _dc_comp: *mut uint16_t,
    pub _rows: *mut *mut uint8_t,
    pub _row_alloc_count: size_t,
    pub _size: size_t,
    pub _type: c_int,
}

/// LossyDctEncoder struct mirroring the C layout
#[repr(C)]
pub struct LossyDctEncoder {
    pub _toNonlinear: *const uint16_t,

    pub _numAcComp: uint64_t,
    pub _numDcComp: uint64_t,

    pub _channel_encode_data: [*mut DctCoderChannelData; 3],
    pub _channel_encode_data_count: c_int,

    pub _width: c_int,
    pub _height: c_int,
    pub _quantBaseError: f32,

    pub _packedAc: *mut uint8_t,
    pub _packedDc: *mut uint8_t,

    pub _quantTableY: [f32; 64],
    pub _hquantTableY: [uint16_t; 64],

    pub _quantTableCbCr: [f32; 64],
    pub _hquantTableCbCr: [uint16_t; 64],
}

impl Default for LossyDctEncoder {
    fn default() -> Self {
        LossyDctEncoder {
            _toNonlinear: ptr::null(),
            _numAcComp: 0,
            _numDcComp: 0,
            _channel_encode_data: [ptr::null_mut(), ptr::null_mut(), ptr::null_mut()],
            _channel_encode_data_count: 0,
            _width: 0,
            _height: 0,
            _quantBaseError: 0.0,
            _packedAc: ptr::null_mut(),
            _packedDc: ptr::null_mut(),
            _quantTableY: [0.0; 64],
            _hquantTableY: [0; 64],
            _quantTableCbCr: [0.0; 64],
            _hquantTableCbCr: [0; 64],
        }
    }
}

/// Base constructor: initialize quant tables and fields
#[no_mangle]
pub unsafe extern "C" fn LossyDctEncoder_base_construct(
    e: *mut LossyDctEncoder,
    quantBaseError: f32,
    packedAc: *mut uint8_t,
    packedDc: *mut uint8_t,
    toNonlinear: *const uint16_t,
    width: c_int,
    height: c_int,
) -> exr_result_t {
    if e.is_null() {
        return EXR_ERR_OUT_OF_MEMORY;
    }
    // JPEG default tables (as in C)
    let jpegQuantTableY: [i32; 64] = [
        16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69,
        56, 14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81,
        104, 113, 92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
    ];
    let jpegQuantTableYMin: i32 = 10;

    let jpegQuantTableCbCr: [i32; 64] = [
        17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99,
        99, 47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
        99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    ];
    let jpegQuantTableCbCrMin: i32 = 17;

    // populate fields
    (*e)._quantBaseError = quantBaseError;
    (*e)._width = width;
    (*e)._height = height;
    (*e)._toNonlinear = toNonlinear;
    (*e)._numAcComp = 0;
    (*e)._numDcComp = 0;
    (*e)._packedAc = packedAc;
    (*e)._packedDc = packedDc;
    if (*e)._quantBaseError < 0.0 {
        (*e)._quantBaseError = 0.0;
    }

    for idx in 0..64 {
        (*e)._quantTableY[idx] =
            ((*e)._quantBaseError * (jpegQuantTableY[idx] as f32) / (jpegQuantTableYMin as f32));
        (*e)._hquantTableY[idx] = float_to_half((*e)._quantTableY[idx]);

        (*e)._quantTableCbCr[idx] = ((*e)._quantBaseError
            * (jpegQuantTableCbCr[idx] as f32)
            / (jpegQuantTableCbCrMin as f32));
        (*e)._hquantTableCbCr[idx] = float_to_half((*e)._quantTableCbCr[idx]);
    }

    (*e)._channel_encode_data[0] = ptr::null_mut();
    (*e)._channel_encode_data[1] = ptr::null_mut();
    (*e)._channel_encode_data[2] = ptr::null_mut();
    (*e)._channel_encode_data_count = 0;

    EXR_ERR_SUCCESS
}

#[no_mangle]
pub unsafe extern "C" fn LossyDctEncoder_construct(
    e: *mut LossyDctEncoder,
    quantBaseError: f32,
    rowPtrs: *mut DctCoderChannelData,
    packedAc: *mut uint8_t,
    packedDc: *mut uint8_t,
    toNonlinear: *const uint16_t,
    width: c_int,
    height: c_int,
) -> exr_result_t {
    if e.is_null() {
        return EXR_ERR_OUT_OF_MEMORY;
    }
    let rv = LossyDctEncoder_base_construct(e, quantBaseError, packedAc, packedDc, toNonlinear, width, height);
    (*e)._channel_encode_data[0] = rowPtrs;
    (*e)._channel_encode_data_count = 1;
    rv
}

#[no_mangle]
pub unsafe extern "C" fn LossyDctEncoderCsc_construct(
    e: *mut LossyDctEncoder,
    quantBaseError: f32,
    rowPtrsR: *mut DctCoderChannelData,
    rowPtrsG: *mut DctCoderChannelData,
    rowPtrsB: *mut DctCoderChannelData,
    packedAc: *mut uint8_t,
    packedDc: *mut uint8_t,
    toNonlinear: *const uint16_t,
    width: c_int,
    height: c_int,
) -> exr_result_t {
    if e.is_null() {
        return EXR_ERR_OUT_OF_MEMORY;
    }
    let rv = LossyDctEncoder_base_construct(e, quantBaseError, packedAc, packedDc, toNonlinear, width, height);
    (*e)._channel_encode_data[0] = rowPtrsR;
    (*e)._channel_encode_data[1] = rowPtrsG;
    (*e)._channel_encode_data[2] = rowPtrsB;
    (*e)._channel_encode_data_count = 3;
    rv
}

/// Bit utility functions translated from C
#[inline]
fn count_set_bits_u32(x: uint32_t) -> i32 {
    // Hacker's delight method used in C fallback
    let mut y = (x as u64).wrapping_mul(0x0002000400080010u64);
    y &= 0x1111111111111111u64;
    y = y.wrapping_mul(0x1111111111111111u64);
    (y >> 60) as i32
}

#[inline]
fn count_set_bits_u16(src: uint16_t) -> i32 {
    count_set_bits_u32(src as uint32_t)
}

#[inline]
fn count_leading_zeros_u32(mut x: uint32_t) -> i32 {
    // fallback implementation
    x |= x >> 1;
    x |= x >> 2;
    x |= x >> 4;
    x |= x >> 8;
    x |= x >> 16;
    32 - count_set_bits_u32(x)
}

#[inline]
fn count_leading_zeros_u16(src: uint16_t) -> i32 {
    count_leading_zeros_u32(src as uint32_t)
}

/// The various helper quantize routines translated from C
unsafe fn handle_quantize_denorm_tol(
    abssrc: uint32_t,
    tolSig: uint32_t,
    errTol: f32,
    srcFloat: f32,
) -> uint32_t {
    let tsigshift = 32 - count_leading_zeros_u32(tolSig);
    let npow2 = 1u32 << tsigshift;
    let lowermask = npow2 - 1;
    let mask = !lowermask;
    let mask2 = mask ^ npow2;

    let mut smallest = abssrc;
    let mut smallbits = count_set_bits_u32(abssrc);
    let mut smalldelta = errTol;

    let mut test = |alt: uint32_t| {
        let bits = count_set_bits_u32(alt);
        if bits < smallbits {
            let delta = half_to_float(alt as uint16_t) - srcFloat;
            if delta < errTol {
                smallbits = bits;
                smalldelta = delta;
                smallest = alt;
            }
        } else if bits == smallbits {
            let delta = half_to_float(alt as uint16_t) - srcFloat;
            if delta < smalldelta {
                smallest = alt;
                smalldelta = delta;
                smallbits = bits;
            }
        }
    };

    test(abssrc & mask2);
    test(abssrc & mask);
    test((abssrc + npow2) & mask);
    test((abssrc + (npow2 << 1)) & mask);

    smallest
}

unsafe fn handle_quantize_generic(
    abssrc: uint32_t,
    tolSig: uint32_t,
    errTol: f32,
    srcFloat: f32,
) -> uint32_t {
    let tsigshift = 32 - count_leading_zeros_u32(tolSig);
    let npow2 = 1u32 << tsigshift;
    let lowermask = npow2 - 1;
    let mask = !lowermask;
    let mask2 = mask ^ npow2;
    let srcMaskedVal = abssrc & lowermask;
    let extrabit = if tolSig > srcMaskedVal { 1 } else { 0 };

    let mask3 = mask2 ^ (((npow2 << 1) * (extrabit)) | ((npow2 >> 1) * ((!extrabit) as u32)));

    let mut smallest = abssrc;
    let mut smallbits = count_set_bits_u32(abssrc);
    let mut smalldelta = errTol;

    let mut test_small = |x: uint32_t| {
        let alt = x;
        let bits = count_set_bits_u32(alt);
        if bits < smallbits {
            let delta = srcFloat - half_to_float(alt as uint16_t);
            if delta < errTol {
                smallbits = bits;
                smalldelta = delta;
                smallest = alt;
            }
        } else if bits == smallbits {
            let delta = srcFloat - half_to_float(alt as uint16_t);
            if delta < smalldelta {
                smallest = alt;
                smalldelta = delta;
                smallbits = bits;
            }
        }
    };

    if extrabit != 0 {
        test_small(abssrc & mask3);
        test_small(abssrc & mask2);
        test_small(abssrc & mask);
    } else if (abssrc & npow2) != 0 {
        test_small(abssrc & mask2);
        test_small(abssrc & mask3);
        test_small(abssrc & mask);
    } else {
        test_small(abssrc & mask2);
        test_small(abssrc & mask);
        test_small(abssrc & mask3);
    }

    // large-side tests
    let mut test_large = |x: uint32_t| {
        let alt = x;
        let bits = count_set_bits_u32(alt);
        if bits < smallbits {
            let delta = half_to_float(alt as uint16_t) - srcFloat;
            if delta < errTol {
                smallbits = bits;
                smalldelta = delta;
                smallest = alt;
            }
        } else if bits == smallbits {
            let delta = half_to_float(alt as uint16_t) - srcFloat;
            if delta < smalldelta {
                smallest = alt;
                smalldelta = delta;
                smallbits = bits;
            }
        }
    };

    test_large((abssrc + npow2) & mask);
    smallest
}

unsafe fn handle_quantize_equal_exp(
    abssrc: uint32_t,
    _tolSig: uint32_t,
    errTol: f32,
    srcFloat: f32,
) -> uint32_t {
    let npow2 = 0x0800u32;
    let lowermask = npow2 - 1;
    let mask = !lowermask;
    let mask2 = mask ^ npow2;

    let srcMaskedVal = abssrc & lowermask;
    let extrabit = if _tolSig > srcMaskedVal { 1 } else { 0 };

    let mask3 = mask2 ^ (((npow2 << 1) * (extrabit)) | ((npow2 >> 1) * ((!extrabit) as u32)));

    let mut smallest = abssrc;
    let mut smallbits = count_set_bits_u32(abssrc);
    let mut smalldelta = errTol;

    if srcMaskedVal == abssrc {
        // test mask3
        let alt = abssrc & mask3;
        let bits = count_set_bits_u32(alt);
        if bits < smallbits {
            let delta = srcFloat - half_to_float(alt as uint16_t);
            if delta < errTol {
                smallbits = bits;
                smalldelta = delta;
                smallest = alt;
            }
        } else if bits == smallbits {
            let delta = srcFloat - half_to_float(alt as uint16_t);
            if delta < smalldelta {
                smallest = alt;
                smalldelta = delta;
                smallbits = bits;
            }
        }
    } else {
        let mut alt0 = abssrc & mask2;
        let mut alt1 = abssrc & mask;
        if alt0 == alt1 {
            alt0 = abssrc & mask3;
        }
        // test alt0
        let bits0 = count_set_bits_u32(alt0);
        if bits0 < smallbits {
            let delta = srcFloat - half_to_float(alt0 as uint16_t);
            if delta < errTol {
                smallbits = bits0;
                smalldelta = delta;
                smallest = alt0;
            }
        } else if bits0 == smallbits {
            let delta = srcFloat - half_to_float(alt0 as uint16_t);
            if delta < smalldelta {
                smallest = alt0;
                smalldelta = delta;
                smallbits = bits0;
            }
        }
        // test alt1
        let bits1 = count_set_bits_u32(alt1);
        if bits1 < smallbits {
            let delta = srcFloat - half_to_float(alt1 as uint16_t);
            if delta < errTol {
                smallbits = bits1;
                smalldelta = delta;
                smallest = alt1;
            }
        } else if bits1 == smallbits {
            let delta = srcFloat - half_to_float(alt1 as uint16_t);
            if delta < smalldelta {
                smallest = alt1;
                smalldelta = delta;
                smallbits = bits1;
            }
        }
    }

    // large-side
    let alt = (abssrc + npow2) & mask;
    let bits = count_set_bits_u32(alt);
    if bits < smallbits {
        let delta = half_to_float(alt as uint16_t) - srcFloat;
        if delta < errTol {
            smallbits = bits;
            smalldelta = delta;
            smallest = alt;
        }
    } else if bits == smallbits {
        let delta = half_to_float(alt as uint16_t) - srcFloat;
        if delta < smalldelta {
            smallest = alt;
            smalldelta = delta;
            smallbits = bits;
        }
    }

    smallest
}

unsafe fn handle_quantize_close_exp(
    abssrc: uint32_t,
    _tolSig: uint32_t,
    errTol: f32,
    srcFloat: f32,
) -> uint32_t {
    let npow2 = 0x0400u32;
    let lowermask = npow2 - 1;
    let mask = !lowermask;
    let mask2 = mask ^ npow2;

    let srcMaskedVal = abssrc & lowermask;
    let extrabit = if _tolSig > srcMaskedVal { 1 } else { 0 };

    let mask3 = mask2 ^ (((npow2 << 1) * (extrabit)) | ((npow2 >> 1) * ((!extrabit) as u32)));

    let mut alternates: [uint32_t; 3] = [0; 3];

    if (abssrc & npow2) == 0 {
        if extrabit != 0 {
            alternates[0] = abssrc & mask3;
            alternates[1] = abssrc & mask;
        } else {
            alternates[0] = abssrc & mask;
            alternates[1] = abssrc & mask3;
        }
    } else {
        if extrabit != 0 {
            alternates[0] = abssrc & mask3;
            alternates[1] = abssrc & mask2;
            let alt1delta = srcFloat - half_to_float(alternates[1] as uint16_t);
            if alt1delta >= errTol {
                alternates[1] = abssrc & mask;
            }
        } else {
            alternates[0] = abssrc & mask2;
            alternates[1] = abssrc & mask3;
            let alt0delta = srcFloat - half_to_float(alternates[0] as uint16_t);
            if alt0delta >= errTol {
                alternates[0] = abssrc & mask;
            }
        }
    }
    alternates[2] = (abssrc + npow2) & mask;

    let mut smallest = abssrc;
    let mut smallbits = count_set_bits_u32(abssrc);
    let mut smalldelta = errTol;

    for &alt in &alternates {
        let bits = count_set_bits_u32(alt);
        if bits < smallbits {
            let delta = srcFloat - half_to_float(alt as uint16_t);
            if delta < errTol {
                smallbits = bits;
                smalldelta = delta;
                smallest = alt;
            }
        } else if bits == smallbits {
            let delta = srcFloat - half_to_float(alt as uint16_t);
            if delta < smalldelta {
                smallest = alt;
                smalldelta = delta;
                smallbits = bits;
            }
        }
    }

    smallest
}

unsafe fn handle_quantize_larger_sig(
    abssrc: uint32_t,
    npow2: uint32_t,
    mask: uint32_t,
    errTol: f32,
    srcFloat: f32,
) -> uint32_t {
    let mask2 = mask ^ (npow2 | (npow2 >> 1));
    let alt0 = abssrc & mask2;
    let alt1 = (abssrc + npow2) & mask;

    let bits0 = count_set_bits_u32(alt0);
    let bits1 = count_set_bits_u32(alt1);

    if bits1 < bits0 {
        let delta = half_to_float(alt1 as uint16_t) - srcFloat;
        if delta < errTol {
            return alt1;
        }
        let delta2 = srcFloat - half_to_float(alt0 as uint16_t);
        if delta2 < errTol {
            return alt0;
        }
    } else if bits1 == bits0 {
        let delta = srcFloat - half_to_float(alt0 as uint16_t);
        let delta1 = half_to_float(alt1 as uint16_t) - srcFloat;
        if delta < errTol {
            return if delta1 < delta { alt1 } else { alt0 };
        }
        if delta1 < errTol {
            return alt1;
        }
    } else {
        let delta = srcFloat - half_to_float(alt0 as uint16_t);
        if delta < errTol {
            return alt0;
        }
        let srcbits = count_set_bits_u32(abssrc);
        if bits1 < srcbits {
            let delta = half_to_float(alt1 as uint16_t) - srcFloat;
            if delta < errTol {
                return alt1;
            }
        }
    }
    abssrc
}

unsafe fn handle_quantize_smaller_sig(
    abssrc: uint32_t,
    npow2: uint32_t,
    mask: uint32_t,
    errTol: f32,
    srcFloat: f32,
) -> uint32_t {
    let alt0 = abssrc & mask;
    let alt1 = (abssrc + npow2) & mask;

    let bits0 = count_set_bits_u32(alt0);
    let bits1 = count_set_bits_u32(alt1);

    if bits1 < bits0 {
        let delta = half_to_float(alt1 as uint16_t) - srcFloat;
        if delta < errTol {
            return alt1;
        }
        let delta2 = srcFloat - half_to_float(alt0 as uint16_t);
        if delta2 < errTol {
            return alt0;
        }
    } else if bits1 == bits0 {
        let delta = srcFloat - half_to_float(alt0 as uint16_t);
        let delta1 = half_to_float(alt1 as uint16_t) - srcFloat;
        if delta < errTol {
            return if delta1 < delta { alt1 } else { alt0 };
        }
        if delta1 < errTol {
            return alt1;
        }
    } else {
        let delta = srcFloat - half_to_float(alt0 as uint16_t);
        if delta < errTol {
            return alt0;
        }
        let srcbits = count_set_bits_u32(abssrc);
        if bits1 < srcbits {
            let delta = half_to_float(alt1 as uint16_t) - srcFloat;
            if delta < errTol {
                return alt1;
            }
        }
    }
    abssrc
}

unsafe fn handle_quantize_equal_sig(
    abssrc: uint32_t,
    npow2: uint32_t,
    mask: uint32_t,
    errTol: f32,
    srcFloat: f32,
) -> uint32_t {
    let alt0 = abssrc & mask;
    let alt1 = (abssrc + npow2) & mask;

    let delta0 = srcFloat - half_to_float(alt0 as uint16_t);
    if delta0 >= errTol {
        let mask2 = mask ^ (npow2 | (npow2 >> 1));
        let alt0_new = abssrc & mask2;
        let delta0_new = srcFloat - half_to_float(alt0_new as uint16_t);
        if delta0_new >= errTol {
            let delta1 = half_to_float(alt1 as uint16_t) - srcFloat;
            if delta1 < errTol {
                let bits1 = count_set_bits_u32(alt1);
                let srcbits = count_set_bits_u32(abssrc);
                if bits1 < srcbits {
                    return alt1;
                }
            }
            return abssrc;
        }
    }

    let bits0 = count_set_bits_u32(alt0);
    let bits1 = count_set_bits_u32(alt1);

    if bits1 < bits0 {
        let delta1 = half_to_float(alt1 as uint16_t) - srcFloat;
        if delta1 < errTol {
            return alt1;
        }
    } else if bits1 == bits0 {
        let delta1 = half_to_float(alt1 as uint16_t) - srcFloat;
        if delta1 < delta0 {
            return alt1;
        }
    }

    alt0
}

unsafe fn handle_quantize_default(
    abssrc: uint32_t,
    tolSig: uint32_t,
    errTol: f32,
    srcFloat: f32,
) -> uint32_t {
    let tsigshift = 32 - count_leading_zeros_u32(tolSig);
    let npow2 = 1u32 << tsigshift;
    let lowermask = npow2 - 1;
    let mask = !lowermask;
    let srcMaskedVal = abssrc & lowermask;

    if srcMaskedVal > tolSig {
        handle_quantize_larger_sig(abssrc, npow2, mask, errTol, srcFloat)
    } else if srcMaskedVal < tolSig {
        handle_quantize_smaller_sig(abssrc, npow2, mask, errTol, srcFloat)
    } else {
        handle_quantize_equal_sig(abssrc, npow2, mask, errTol, srcFloat)
    }
}

unsafe fn algo_quantize(src: uint32_t, herrTol: uint32_t, errTol: f32, mut srcFloat: f32) -> uint16_t {
    let sign = src & 0x8000;
    let abssrc = src & 0x7FFF;

    srcFloat = srcFloat.abs();

    let srcExpBiased = src & 0x7C00;
    let tolExpBiased = herrTol & 0x7C00;

    if srcExpBiased == 0x7C00 {
        return src as uint16_t; // NaN/inf, bail
    }

    if srcFloat < errTol {
        return 0u16;
    }

    let expDiff = ((srcExpBiased as i32 - tolExpBiased as i32) >> 10) as i32;
    let mut tolSig = ((herrTol & 0x3FF) | (1 << 10)) as uint32_t;
    if expDiff != 0 {
        tolSig = tolSig >> (expDiff as u32);
    }

    if tolExpBiased == 0 {
        if expDiff == 0 || expDiff == 1 {
            tolSig = (herrTol & 0x3FF) as uint32_t;
            if tolSig == 0 {
                return src as uint16_t;
            }
            return (sign | handle_quantize_generic(abssrc, tolSig, errTol, srcFloat)) as uint16_t;
        }

        tolSig = (herrTol & 0x3FF) as uint32_t;
        if tolSig == 0 {
            return src as uint16_t;
        }
        tolSig >>= expDiff as u32;
        if tolSig == 0 {
            tolSig = 1;
        }
        return (sign | handle_quantize_denorm_tol(abssrc, tolSig, errTol, srcFloat)) as uint16_t;
    }

    if tolSig == 0 {
        return src as uint16_t;
    }

    if expDiff > 1 || srcExpBiased == 0 {
        (sign | handle_quantize_default(abssrc, tolSig, errTol, srcFloat)) as uint16_t
    } else if expDiff == 0 {
        (sign | handle_quantize_equal_exp(abssrc, tolSig, errTol, srcFloat)) as uint16_t
    } else {
        (sign | handle_quantize_close_exp(abssrc, tolSig, errTol, srcFloat)) as uint16_t
    }
}

/// Quantize and zigzag into halfZigCoeff (XDR/native conversion handled by one_from_native16)
#[no_mangle]
pub unsafe extern "C" fn quantizeCoeffAndZigXDR(
    halfZigCoeff: *mut uint16_t,
    dctvals: *const f32,
    tolerances: *const f32,
    halftols: *const uint16_t,
) {
    // inv_remap as in C
    const INV_REMAP: [usize; 64] = [
        0, 1, 5, 6, 14, 15, 27, 28, 2, 4, 7, 13, 16, 26, 29, 42, 3, 8, 12, 17, 25, 30, 41, 43, 9, 11,
        18, 24, 31, 40, 44, 53, 10, 19, 23, 32, 39, 45, 52, 54, 20, 22, 33, 38, 46, 51, 55, 60, 21, 34,
        37, 47, 50, 56, 59, 61, 35, 36, 48, 49, 57, 58, 62, 63,
    ];

    // manual unrolling in steps of 4
    let mut i = 0usize;
    while i < 64 {
        let src0 = float_to_half(*dctvals.add(i + 0));
        let src1 = float_to_half(*dctvals.add(i + 1));
        let src2 = float_to_half(*dctvals.add(i + 2));
        let src3 = float_to_half(*dctvals.add(i + 3));

        let errTol0 = *tolerances.add(i + 0);
        let errTol1 = *tolerances.add(i + 1);
        let errTol2 = *tolerances.add(i + 2);
        let errTol3 = *tolerances.add(i + 3);

        let herrTol0 = *halftols.add(i + 0);
        let herrTol1 = *halftols.add(i + 1);
        let herrTol2 = *halftols.add(i + 2);
        let herrTol3 = *halftols.add(i + 3);

        let a0 = algo_quantize(src0 as uint32_t, herrTol0 as uint32_t, errTol0, half_to_float(src0));
        let a1 = algo_quantize(src1 as uint32_t, herrTol1 as uint32_t, errTol1, half_to_float(src1));
        let a2 = algo_quantize(src2 as uint32_t, herrTol2 as uint32_t, errTol2, half_to_float(src2));
        let a3 = algo_quantize(src3 as uint32_t, herrTol3 as uint32_t, errTol3, half_to_float(src3));

        ptr::write(halfZigCoeff.add(INV_REMAP[i + 0]), one_from_native16(a0));
        ptr::write(halfZigCoeff.add(INV_REMAP[i + 1]), one_from_native16(a1));
        ptr::write(halfZigCoeff.add(INV_REMAP[i + 2]), one_from_native16(a2));
        ptr::write(halfZigCoeff.add(INV_REMAP[i + 3]), one_from_native16(a3));

        i += 4;
    }
}

/// Main encoding routine
#[no_mangle]
pub unsafe extern "C" fn LossyDctEncoder_execute(
    alloc_fn: extern "C" fn(size_t) -> *mut c_void,
    free_fn: extern "C" fn(*mut c_void),
    e: *mut LossyDctEncoder,
) -> exr_result_t {
    if e.is_null() {
        return EXR_ERR_OUT_OF_MEMORY;
    }
    let numComp = (*e)._channel_encode_data_count as usize;
    let mut chanData: [*mut DctCoderChannelData; 3] = [ptr::null_mut(); 3];

    let numBlocksX = (( (*e)._width as f32 / 8.0).ceil()) as i32;
    let numBlocksY = (( (*e)._height as f32 / 8.0).ceil()) as i32;

    let mut halfZigCoef: [uint16_t; 64] = [0u16; 64];

    let mut currAcComp = (*e)._packedAc as *mut uint16_t;
    let mut tmpHalfBufferElements = 0usize;
    let mut tmpHalfBuffer: *mut uint16_t = ptr::null_mut();
    let mut tmpHalfBufferPtr: *mut uint16_t = ptr::null_mut();

    (*e)._numAcComp = 0;
    (*e)._numDcComp = 0;

    // count float elements
    for chan in 0..numComp {
        chanData[chan] = (*e)._channel_encode_data[chan];
        if !chanData[chan].is_null() && (*chanData[chan])._type == 2 /* EXR_PIXEL_FLOAT */ {
            tmpHalfBufferElements += ((*e)._width as usize) * ((*e)._height as usize);
        }
    }

    if tmpHalfBufferElements > 0 {
        tmpHalfBuffer = alloc_fn(tmpHalfBufferElements * size_of::<uint16_t>()) as *mut uint16_t;
        if tmpHalfBuffer.is_null() {
            return EXR_ERR_OUT_OF_MEMORY;
        }
        tmpHalfBufferPtr = tmpHalfBuffer;
    }

    // convert float channels to half in temp buffer and reassign rows
    for chan in 0..numComp {
        if chanData[chan].is_null() { continue; }
        if (*chanData[chan])._type != 2 { continue; } // EXR_PIXEL_FLOAT
        for y in 0..(*e)._height {
            let srcXdr = (*chanData[chan])._rows.add(y as usize);
            let src = *srcXdr as *const f32;
            for x in 0..(*e)._width {
                let val = one_to_native_float(*src.add(x as usize));
                let clamped = if val > 65504.0 { 65504.0 } else if val < -65504.0 { -65504.0 } else { val };
                ptr::write(tmpHalfBufferPtr.add(x as usize), one_from_native16(float_to_half(clamped)));
            }
            (*chanData[chan])._rows.add(y as usize).write(tmpHalfBufferPtr as *mut uint8_t);
            tmpHalfBufferPtr = tmpHalfBufferPtr.add((*e)._width as usize);
        }
    }

    // pack DC components pointers (per plane)
    if !chanData[0].is_null() {
        (*chanData[0])._dc_comp = (*e)._packedDc as *mut uint16_t;
    }
    for chan in 1..numComp {
        if chanData[chan].is_null() || chanData[chan - 1].is_null() { continue; }
        (*chanData[chan])._dc_comp = (*chanData[chan - 1])._dc_comp.add((numBlocksX * numBlocksY) as usize);
    }

    // iterate blocks
    for blocky in 0..numBlocksY {
        for blockx in 0..numBlocksX {
            let mut quantTable: *const f32 = (*e)._quantTableY.as_ptr();
            let mut hquantTable: *const uint16_t = (*e)._hquantTableY.as_ptr();

            for chan in 0..numComp {
                if chanData[chan].is_null() { continue; }
                // build 8x8 block, mirror edges as in C
                for y in 0..8 {
                    for x in 0..8 {
                        let mut vx = 8 * blockx + x;
                        let mut vy = 8 * blocky + y;
                        if vx >= (*e)._width {
                            vx = (*e)._width - (vx - ((*e)._width - 1));
                        }
                        if vx < 0 { vx = (*e)._width - 1; }
                        if vy >= (*e)._height {
                            vy = (*e)._height - (vy - ((*e)._height - 1));
                        }
                        if vy < 0 { vy = (*e)._height - 1; }

                        let row_ptr = (*chanData[chan])._rows.add(vy as usize);
                        let h = *((*row_ptr) as *const uint16_t).add(vx as usize);
                        let h_mapped = if !(*e)._toNonlinear.is_null() {
                            *(*e)._toNonlinear.add(h as usize)
                        } else {
                            one_to_native16(h)
                        };
                        *(*chanData[chan])._dctData.add((y * 8 + x) as usize) = half_to_float(h_mapped);
                    }
                }
            }

            // color space conversion if 3 comps
            if numComp == 3 {
                csc709Forward64(
                    (*chanData[0])._dctData,
                    (*chanData[1])._dctData,
                    (*chanData[2])._dctData,
                );
            }

            // quant / forward DCT per channel
            quantTable = (*e)._quantTableY.as_ptr();
            hquantTable = (*e)._hquantTableY.as_ptr();
            for chan in 0..numComp {
                if chanData[chan].is_null() { continue; }
                dctForward8x8((*chanData[chan])._dctData);
                quantizeCoeffAndZigXDR(
                    halfZigCoef.as_mut_ptr(),
                    (*chanData[chan])._dctData,
                    quantTable,
                    hquantTable,
                );

                // DC component write and increment
                *((*chanData[chan])._dc_comp) = halfZigCoef[0];
                (*chanData[chan])._dc_comp = (*chanData[chan])._dc_comp.add(1);
                (*e)._numDcComp += 1;

                // RLE AC into currAcComp
                LossyDctEncoder_rleAc(e, halfZigCoef.as_mut_ptr(), &mut currAcComp);

                // switch quant tables for chroma after first channel
                quantTable = (*e)._quantTableCbCr.as_ptr();
                hquantTable = (*e)._hquantTableCbCr.as_ptr();
            }
        }
    }

    if !tmpHalfBuffer.is_null() {
        free_fn(tmpHalfBuffer as *mut c_void);
    }

    EXR_ERR_SUCCESS
}

/// RLE encoder for AC coefficients (translated line-for-line)
#[no_mangle]
pub unsafe extern "C" fn LossyDctEncoder_rleAc(
    e: *mut LossyDctEncoder,
    block: *mut uint16_t,
    acPtr: *mut *mut uint16_t,
) {
    if e.is_null() || block.is_null() || acPtr.is_null() { return; }
    let mut dctComp = 1usize;
    let rleSymbol: uint16_t = 0x0;
    let mut curAC = *acPtr;

    while dctComp < 64 {
        let mut runLen: usize = 1;

        if *block.add(dctComp) != rleSymbol {
            ptr::write(curAC, *block.add(dctComp));
            curAC = curAC.add(1);
            (*e)._numAcComp += 1;
            dctComp += runLen;
            continue;
        }

        while (dctComp + runLen < 64) && (*block.add(dctComp + runLen) == rleSymbol) {
            runLen += 1;
        }

        if runLen == 1 {
            runLen = 1;
            ptr::write(curAC, *block.add(dctComp));
            curAC = curAC.add(1);
            (*e)._numAcComp += 1;
        } else if runLen + dctComp == 64 {
            ptr::write(curAC, 0xff00u16);
            curAC = curAC.add(1);
            (*e)._numAcComp += 1;
        } else {
            ptr::write(curAC, (0xff00u16) | (runLen as uint16_t));
            curAC = curAC.add(1);
            (*e)._numAcComp += 1;
        }
        dctComp += runLen;
    }
    *acPtr = curAC;
}
