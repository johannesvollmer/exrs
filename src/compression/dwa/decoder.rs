// LossyDctDecoder port (decoder.h) -> Rust (unsafe, near line-for-line translation).
// Integrate into the single-file port; relies on many extern helpers provided elsewhere.

use std::os::raw::{c_int, c_void, c_char};
use std::ptr;
use std::mem::size_of;

type uint8_t = u8;
type uint16_t = u16;
type uint64_t = u64;
type size_t = usize;
pub type exr_result_t = c_int;

pub const EXR_ERR_SUCCESS: exr_result_t = 0;
pub const EXR_ERR_OUT_OF_MEMORY: exr_result_t = -1;
pub const EXR_ERR_CORRUPT_CHUNK: exr_result_t = -2;

pub const DWA_CLASSIFIER_TRUE: uint8_t = 1;

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

#[repr(C)]
pub struct LossyDctDecoder {
    pub _packedAcCount: uint64_t,
    pub _packedDcCount: uint64_t,
    pub _packedAc: *mut uint8_t,
    pub _packedAcEnd: *mut uint8_t,
    pub _packedDc: *mut uint8_t,
    pub _remDcCount: uint64_t,
    pub _toLinear: *const uint16_t,
    pub _width: c_int,
    pub _height: c_int,
    pub _channel_decode_data: [*mut DctCoderChannelData; 3],
    pub _channel_decode_data_count: c_int,
    pub _pad: [u8; 4],
}

extern "C" {
    // helpers used by decoder - must be provided elsewhere in the port.
    fn priv_to_native16(data: *mut uint16_t, n: usize);
    fn simd_align_pointer(p: *mut uint8_t) -> *mut uint8_t;
    fn half_to_float(h: uint16_t) -> f32;
    fn fromHalfZigZag(src: *const uint16_t, dst: *mut f32);
    fn dctInverse8x8DcOnly(dst: *mut f32);
    fn dctInverse8x8_7(dst: *mut f32);
    fn dctInverse8x8_6(dst: *mut f32);
    fn dctInverse8x8_5(dst: *mut f32);
    fn dctInverse8x8_4(dst: *mut f32);
    fn dctInverse8x8_3(dst: *mut f32);
    fn dctInverse8x8_2(dst: *mut f32);
    fn dctInverse8x8_1(dst: *mut f32);
    fn dctInverse8x8_0(dst: *mut f32);

    fn csc709Inverse64(a: *mut f32, b: *mut f32, c: *mut f32);
    fn csc709Inverse(a: *mut f32, b: *mut f32, c: *mut f32);

    fn convertFloatToHalf64(dst: *mut uint16_t, src: *const f32);
    fn float_to_half(f: f32) -> uint16_t;

    fn one_from_native_float(f: f32) -> f32;
    fn one_to_native16(v: uint16_t) -> uint16_t;

    // memory helpers (provided earlier)
    // alloc_fn: extern "C" fn(size_t) -> *mut c_void
    // free_fn: extern "C" fn(*mut c_void)
}

#[no_mangle]
pub unsafe extern "C" fn LossyDctDecoder_unRleAc(
    d: *mut LossyDctDecoder,
    lastNonZero: *mut c_int,
    currAcComp: *mut *mut uint16_t,
    packedAcEnd: *mut uint16_t,
    halfZigBlock: *mut uint16_t,
) -> exr_result_t {
    if d.is_null() || lastNonZero.is_null() || currAcComp.is_null() || halfZigBlock.is_null() {
        return EXR_ERR_CORRUPT_CHUNK;
    }

    let mut dctComp: c_int = 1;
    let mut acComp: *mut uint16_t = *currAcComp;
    let mut lnz: c_int = 0;
    let mut ac_count: uint64_t = 0;

    while dctComp < 64 {
        if acComp >= packedAcEnd {
            return EXR_ERR_CORRUPT_CHUNK;
        }
        let val: uint16_t = ptr::read(acComp);
        if (val & 0xff00) == 0xff00 {
            let count = (val & 0xff) as c_int;
            dctComp += if count == 0 { 64 } else { count };
        } else {
            lnz = dctComp;
            // halfZigBlock[dctComp] = val;
            ptr::write(halfZigBlock.add(dctComp as usize), val);
            dctComp += 1;
        }
        ac_count += 1;
        acComp = acComp.add(1);
    }

    (*d)._packedAcCount = (*d)._packedAcCount.wrapping_add(ac_count);
    *lastNonZero = lnz;
    *currAcComp = acComp;
    EXR_ERR_SUCCESS
}

#[no_mangle]
pub unsafe extern "C" fn LossyDctDecoder_construct(
    d: *mut LossyDctDecoder,
    rowPtrs: *mut DctCoderChannelData,
    packedAc: *mut uint8_t,
    packedAcEnd: *mut uint8_t,
    packedDc: *mut uint8_t,
    remDcCount: uint64_t,
    toLinear: *const uint16_t,
    width: c_int,
    height: c_int,
) -> exr_result_t {
    if d.is_null() {
        return EXR_ERR_CORRUPT_CHUNK;
    }
    let rv = LossyDctDecoder_base_construct(d, packedAc, packedAcEnd, packedDc, remDcCount, toLinear, width, height);
    if rv != EXR_ERR_SUCCESS {
        return rv;
    }
    (*d)._channel_decode_data[0] = rowPtrs;
    (*d)._channel_decode_data_count = 1;
    rv
}

#[no_mangle]
pub unsafe extern "C" fn LossyDctDecoderCsc_construct(
    d: *mut LossyDctDecoder,
    rowPtrsR: *mut DctCoderChannelData,
    rowPtrsG: *mut DctCoderChannelData,
    rowPtrsB: *mut DctCoderChannelData,
    packedAc: *mut uint8_t,
    packedAcEnd: *mut uint8_t,
    packedDc: *mut uint8_t,
    remDcCount: uint64_t,
    toLinear: *const uint16_t,
    width: c_int,
    height: c_int,
) -> exr_result_t {
    if d.is_null() {
        return EXR_ERR_CORRUPT_CHUNK;
    }
    let rv = LossyDctDecoder_base_construct(d, packedAc, packedAcEnd, packedDc, remDcCount, toLinear, width, height);
    if rv != EXR_ERR_SUCCESS {
        return rv;
    }
    (*d)._channel_decode_data[0] = rowPtrsR;
    (*d)._channel_decode_data[1] = rowPtrsG;
    (*d)._channel_decode_data[2] = rowPtrsB;
    (*d)._channel_decode_data_count = 3;
    rv
}

#[no_mangle]
pub unsafe extern "C" fn LossyDctDecoder_base_construct(
    d: *mut LossyDctDecoder,
    packedAc: *mut uint8_t,
    packedAcEnd: *mut uint8_t,
    packedDc: *mut uint8_t,
    remDcCount: uint64_t,
    toLinear: *const uint16_t,
    width: c_int,
    height: c_int,
) -> exr_result_t {
    if d.is_null() {
        return EXR_ERR_CORRUPT_CHUNK;
    }
    (*d)._packedAcCount = 0;
    (*d)._packedDcCount = 0;
    (*d)._packedAc = packedAc;
    (*d)._packedAcEnd = packedAcEnd;
    (*d)._packedDc = packedDc;
    (*d)._remDcCount = remDcCount;
    (*d)._toLinear = toLinear;
    (*d)._width = width;
    (*d)._height = height;
    (*d)._channel_decode_data[0] = ptr::null_mut();
    (*d)._channel_decode_data[1] = ptr::null_mut();
    (*d)._channel_decode_data[2] = ptr::null_mut();
    (*d)._channel_decode_data_count = 0;
    EXR_ERR_SUCCESS
}

#[no_mangle]
pub unsafe extern "C" fn LossyDctDecoder_execute(
    alloc_fn: extern "C" fn(size_t) -> *mut c_void,
    free_fn: extern "C" fn(*mut c_void),
    d: *mut LossyDctDecoder,
) -> exr_result_t {
    if d.is_null() {
        return EXR_ERR_CORRUPT_CHUNK;
    }

    let numComp = (*d)._channel_decode_data_count as usize;
    let mut chanData: [*mut DctCoderChannelData; 3] = [ptr::null_mut(); 3];
    let mut lastNonZero: c_int = 0;
    let numBlocksX = ((*d)._width + 7) / 8;
    let numBlocksY = ((*d)._height + 7) / 8;
    let leftoverX = (*d)._width - (numBlocksX - 1) * 8;
    let leftoverY = (*d)._height - (numBlocksY - 1) * 8;
    let numFullBlocksX = (*d)._width / 8;

    let mut currAcComp = (*d)._packedAc as *mut uint16_t;
    let acCompEnd = (*d)._packedAcEnd as *mut uint16_t;
    let mut currDcComp: [*mut uint16_t; 3] = [ptr::null_mut(); 3];

    if (*d)._remDcCount < (numComp as uint64_t).wrapping_mul(numBlocksX as uint64_t).wrapping_mul(numBlocksY as uint64_t) {
        return EXR_ERR_CORRUPT_CHUNK;
    }

    for chan in 0..numComp {
        chanData[chan] = (*d)._channel_decode_data[chan];
    }

    // allocate temp aligned buffer
    let row_block_bytes = (numComp * numBlocksX * 64 * size_of::<uint16_t>()) + _SSE_ALIGNMENT;
    let rowBlockHandle = alloc_fn(row_block_bytes);
    if rowBlockHandle.is_null() {
        return EXR_ERR_OUT_OF_MEMORY;
    }
    let rowBlockAligned = simd_align_pointer(rowBlockHandle as *mut uint8_t) as *mut uint16_t;
    let mut rowBlock: [*mut uint16_t; 3] = [ptr::null_mut(); 3];
    rowBlock[0] = rowBlockAligned;
    for comp in 1..numComp {
        rowBlock[comp] = rowBlock[comp - 1].add(numBlocksX * 64);
    }

    // pack DC components pointers
    currDcComp[0] = (*d)._packedDc as *mut uint16_t;
    for comp in 1..numComp {
        currDcComp[comp] = currDcComp[comp - 1].add(numBlocksX * numBlocksY);
    }

    // main loop over blocks
    for blocky in 0..numBlocksY {
        let mut maxY = 8;
        let mut maxX = 8;
        if blocky == numBlocksY - 1 { maxY = leftoverY; }

        for blockx in 0..numBlocksX {
            let mut blockIsConstant: uint8_t = DWA_CLASSIFIER_TRUE;
            if blockx == numBlocksX - 1 { maxX = leftoverX; }

            for comp in 0..numComp {
                let chan = chanData[comp];
                if chan.is_null() {
                    free_fn(rowBlockHandle);
                    return EXR_ERR_CORRUPT_CHUNK;
                }
                let halfZigData_ptr = (*chan)._halfZigData;
                let dctData_ptr = (*chan)._dctData;

                // DC component
                // zero halfZigData and set [0] = *currDcComp
                for i in 0..64 {
                    ptr::write(halfZigData_ptr.add(i), 0u16);
                }
                let dc_val = ptr::read(currDcComp[comp]);
                ptr::write(halfZigData_ptr, dc_val);
                currDcComp[comp] = currDcComp[comp].add(1);
                (*d)._packedDcCount = (*d)._packedDcCount.wrapping_add(1);

                // UnRLE AC
                let mut last_nz: c_int = 0;
                let mut curr_ac_local = currAcComp;
                let rv_unrle = LossyDctDecoder_unRleAc(
                    d,
                    &mut last_nz,
                    &mut curr_ac_local,
                    acCompEnd,
                    halfZigData_ptr,
                );
                if rv_unrle != EXR_ERR_SUCCESS {
                    free_fn(rowBlockHandle);
                    return rv_unrle;
                }
                currAcComp = curr_ac_local;

                // convert XDR -> native
                priv_to_native16(halfZigData_ptr, 64);

                if last_nz == 0 {
                    // DC only
                    let f = half_to_float(ptr::read(halfZigData_ptr));
                    ptr::write(dctData_ptr, f);
                    dctInverse8x8DcOnly(dctData_ptr);
                } else {
                    blockIsConstant = 0;
                    fromHalfZigZag(halfZigData_ptr as *const uint16_t, dctData_ptr);

                    if last_nz < 2 { dctInverse8x8_7(dctData_ptr); }
                    else if last_nz < 3 { dctInverse8x8_6(dctData_ptr); }
                    else if last_nz < 9 { dctInverse8x8_5(dctData_ptr); }
                    else if last_nz < 10 { dctInverse8x8_4(dctData_ptr); }
                    else if last_nz < 20 { dctInverse8x8_3(dctData_ptr); }
                    else if last_nz < 21 { dctInverse8x8_2(dctData_ptr); }
                    else if last_nz < 35 { dctInverse8x8_1(dctData_ptr); }
                    else { dctInverse8x8_0(dctData_ptr); }
                }
            } // comp

            // CSC
            if numComp == 3 {
                if blockIsConstant == 0 {
                    csc709Inverse64(
                        (*chanData[0])._dctData,
                        (*chanData[1])._dctData,
                        (*chanData[2])._dctData,
                    );
                } else {
                    csc709Inverse(
                        (*chanData[0])._dctData,
                        (*chanData[1])._dctData,
                        (*chanData[2])._dctData,
                    );
                }
            }

            // Float -> Half conversion into rowBlock
            for comp in 0..numComp {
                if blockIsConstant == 0 {
                    convertFloatToHalf64(
                        rowBlock[comp].add(blockx * 64),
                        (*chanData[comp])._dctData,
                    );
                } else {
                    // constant block
                    let val = float_to_half((*(*chanData[comp])._dctData));
                    let dst = rowBlock[comp].add(blockx * 64);
                    for i in 0..64 {
                        ptr::write(dst.add(i), val);
                    }
                }
            }
        } // blockx

        // Unblock rowBlock into channel row pointers
        for comp in 0..numComp {
            // full-blocks fast path (non-SSE scalar implementation)
            if (*d)._toLinear != ptr::null() {
                for y in (8 * blocky)..(8 * blocky + maxY) {
                    let dst_row = (*chanData[comp])._rows.add(y as usize);
                    let dst = *dst_row as *mut uint16_t;
                    for blockx in 0..numFullBlocksX {
                        let src = rowBlock[comp].add(blockx * 64 + ((y & 0x7) * 8));
                        // copy and map through toLinear
                        for i in 0..8 {
                            let v = ptr::read(src.add(i));
                            let mapped = *(*d)._toLinear.add(v as usize);
                            ptr::write(dst.add(blockx as usize * 8 + i), mapped);
                        }
                    }
                }
            } else {
                for y in (8 * blocky)..(8 * blocky + maxY) {
                    let dst_row = (*chanData[comp])._rows.add(y as usize);
                    let dst = *dst_row as *mut uint16_t;
                    for blockx in 0..numFullBlocksX {
                        let src = rowBlock[comp].add(blockx * 64 + ((y & 0x7) * 8));
                        // memcpy 8 * sizeof(uint16_t)
                        for i in 0..8 {
                            let v = ptr::read(src.add(i));
                            ptr::write(dst.add(blockx as usize * 8 + i), v);
                        }
                    }
                }
            }

            // partial X blocks
            if numFullBlocksX != numBlocksX {
                for y in (8 * blocky)..(8 * blocky + maxY) {
                    let src = rowBlock[comp].add(numFullBlocksX * 64 + ((y & 0x7) * 8));
                    let dst_row = (*chanData[comp])._rows.add(y as usize);
                    let dst = *dst_row as *mut uint16_t;
                    let mut dst_ptr = dst.add(8 * numFullBlocksX);
                    for x in 0..maxX {
                        let val = ptr::read(src.add(x as usize));
                        if (*d)._toLinear != ptr::null() {
                            let mapped = *(*d)._toLinear.add(val as usize);
                            ptr::write(dst_ptr, mapped);
                        } else {
                            ptr::write(dst_ptr, val);
                        }
                        dst_ptr = dst_ptr.add(1);
                    }
                }
            }
        } // comp
    } // blocky

    // Convert half->float for channels with EXR_PIXEL_FLOAT
    for chan in 0..numComp {
        // chanData[chan]._type check - using _type field from channel struct
        if (*chanData[chan])._type as c_int != (2 as c_int) {
            continue;
        }
        for y in 0..(*d)._height {
            let float_ptr = (*chanData[chan])._rows.add(y as usize) as *mut f32;
            let half_ptr = float_ptr as *mut uint16_t;
            // process in reverse
            let mut x = (*d)._width - 1;
            while x >= 0 {
                let h = ptr::read(half_ptr.add(x as usize));
                let native = one_to_native16(h);
                let f = half_to_float(native);
                let out_f = one_from_native_float(f);
                ptr::write(float_ptr.add(x as usize), out_f);
                x -= 1;
                if x < 0 { break; } // avoid infinite loop on unsigned
            }
        }
    }

    free_fn(rowBlockHandle);
    EXR_ERR_SUCCESS
}

// helper constant defined elsewhere
const _SSE_ALIGNMENT: usize = 16;
