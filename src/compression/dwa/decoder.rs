use crate::compression::dwa::classifier::DWA_CLASSIFIER_FALSE;
use crate::compression::dwa::transform_8x8::{csc709_inverse, dct_inverse_8x8};
// LossyDctDecoder port (decoder.h) -> Rust (unsafe, near line-for-line translation).
// Integrate into the single-file port; relies on many extern helpers provided elsewhere.
use super::externals::*;
use super::dwa::*;

#[repr(C)]
pub struct LossyDctDecoder {
    //
    // if NATIVE and XDR are really the same values, we can
    // skip some processing and speed things along
    //

    //
    // Counts of how many items have been packed into the
    // AC and DC buffers
    //
    pub _packedAcCount: uint64_t,
    pub _packedDcCount: uint64_t,

    //
    // AC and DC buffers to pack
    //
    pub _packedAc: *mut uint8_t,
    pub _packedAcEnd: *mut uint8_t,
    pub _packedDc: *mut uint8_t,
    pub _remDcCount: uint64_t,

    //
    // half -> half LUT to transform from nonlinear to linear
    //
    pub _toLinear: *const uint16_t,

    //
    // image dimensions
    //
    pub _width: c_int,
    pub _height: c_int,
    pub _channel_decode_data: [*mut DctCoderChannelData; 3],
    pub _channel_decode_data_count: c_int,
    pub _pad: [u8; 4],
}


//
// Un-RLE the packed AC components into
// a half buffer. The half block should
// be the full 8x8 block (in zig-zag order
// still), not the first AC component.
//
// currAcComp is advanced as bytes are decoded.
//
// This returns the index of the last non-zero
// value in the buffer - with the index into zig zag
// order data. If we return 0, we have DC only data.
//
//
// This is assuminging that halfZigBlock is zero'ed
// prior to calling
//
#[no_mangle]
pub unsafe extern "C" fn LossyDctDecoder_unRleAc(
    d: *mut LossyDctDecoder,
    lastNonZero: *mut c_int,
    currAcComp: *mut *mut uint16_t,
    packedAcEnd: *mut uint16_t,
    halfZigBlock: *mut uint16_t,
) -> exr_result_t {
    //
    // Un-RLE the RLE'd blocks. If we find an item whose
    // high byte is 0xff, then insert the number of 0's
    // as indicated by the low byte.
    //
    // Otherwise, just copy the number verbatim.
    //
    if d.is_null() || lastNonZero.is_null() || currAcComp.is_null() || halfZigBlock.is_null() {
        return EXR_ERR_CORRUPT_CHUNK;
    }

    let mut dctComp: c_int = 1;
    let mut acComp: *mut uint16_t = *currAcComp;
    let mut lnz: c_int = 0;
    let mut ac_count: uint64_t = 0;

    //
    // Start with a zero'ed block, so we don't have to
    // write when we hit a run symbol
    //

    while dctComp < 64 {
        if acComp >= packedAcEnd {
            return EXR_ERR_CORRUPT_CHUNK;
        }
        let val: uint16_t = ptr::read(acComp);
        if (val & 0xff00) == 0xff00 {
            let count = (val & 0xff) as c_int;
            // run, insert 0s - since block pre-zeroed, nothing to do
            // just increment dctComp but test for end of block...
            dctComp += if count == 0 { 64 } else { count };
        } else {
            //
            // Not a run, just copy over the value
            //
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

//
// Used to decode a single channel of LOSSY_DCT data.
//
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
    //
    // toLinear is a half-float LUT to convert the encoded values
    // back to linear light. If you want to skip this step, pass
    // in NULL here.
    //
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

//
// Used to decode 3 channels of LOSSY_DCT data that
// are grouped together and color space converted.
//
//
// toLinear is a half-float LUT to convert the encoded values
// back to linear light. If you want to skip this step, pass
// in NULL here.
//
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

/**************************************/
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

/**************************************/
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
    //
    // Allocate a temp aligned buffer to hold a rows worth of full
    // 8x8 half-float blocks
    //
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

            //
            // If we can detect that the block is constant values
            // (all components only have DC values, and all AC is 0),
            // we can do everything only on 1 value, instead of all
            // 64.
            //
            // This won't really help for regular images, but it is
            // meant more for layers with large swaths of black
            //
            for comp in 0..numComp {
                let chan = chanData[comp];
                if chan.is_null() {
                    free_fn(rowBlockHandle);
                    return EXR_ERR_CORRUPT_CHUNK;
                }
                let halfZigData_ptr = (*chan)._halfZigData;
                let dctData_ptr = (*chan).dct_data;

                // DC component
                // zero halfZigData and set [0] = *currDcComp
                for i in 0..64 {
                    ptr::write(halfZigData_ptr.add(i), 0u16);
                }
                let dc_val = ptr::read(currDcComp[comp]);
                ptr::write(halfZigData_ptr, dc_val);
                currDcComp[comp] = currDcComp[comp].add(1);
                (*d)._packedDcCount = (*d)._packedDcCount.wrapping_add(1);


                //
                // UnRLE the AC. This will modify currAcComp
                //
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

                //
                // Convert from XDR to NATIVE
                //
                priv_to_native16(halfZigData_ptr, 64);

                if last_nz == 0 {
                    //
                    // DC only case - AC components are all 0
                    //
                    let f = half_to_float(ptr::read(halfZigData_ptr));
                    ptr::write(dctData_ptr, f);
                    dctInverse8x8DcOnly(dctData_ptr);
                } else {
                    //
                    // We have some AC components that are non-zero.
                    // Can't use the 'constant block' optimization
                    //
                    blockIsConstant = DWA_CLASSIFIER_FALSE;

                    //
                    // Un-Zig zag
                    //
                    fromHalfZigZag(halfZigData_ptr as *const uint16_t, dctData_ptr);

                    //
                    // Zig-Zag indices in normal layout are as follows:
                    //
                    // 0   1   5   6   14  15  27  28
                    // 2   4   7   13  16  26  29  42
                    // 3   8   12  17  25  30  41  43
                    // 9   11  18  24  31  40  44  53
                    // 10  19  23  32  39  45  52  54
                    // 20  22  33  38  46  51  55  60
                    // 21  34  37  47  50  56  59  61
                    // 35  36  48  49  57  58  62  63
                    //
                    // If lastNonZero is less than the first item on
                    // each row, we know that the whole row is zero and
                    // can be skipped in the row-oriented part of the
                    // iDCT.
                    //
                    // The unrolled logic here is:
                    //
                    //    if lastNonZero < rowStartIdx[i],
                    //    zeroedRows = rowsEmpty[i]
                    //
                    // where:
                    //
                    //    const int rowStartIdx[] = {2, 3, 9, 10, 20, 21, 35};
                    //    const int rowsEmpty[]   = {7, 6, 5,  4,  3,  2,  1};
                    //
                    if last_nz < 2 { dct_inverse_8x8(dctData_ptr, 7); }
                    else if last_nz < 3 { dct_inverse_8x8(dctData_ptr, 6); }
                    else if last_nz < 9 { dct_inverse_8x8(dctData_ptr, 5); }
                    else if last_nz < 10 { dct_inverse_8x8(dctData_ptr, 4); }
                    else if last_nz < 20 { dct_inverse_8x8(dctData_ptr, 3); }
                    else if last_nz < 21 { dct_inverse_8x8(dctData_ptr, 2); }
                    else if last_nz < 35 { dct_inverse_8x8(dctData_ptr, 1); }
                    else { dct_inverse_8x8(dctData_ptr, 0); }
                }
            } // comp

            //
            // Perform the CSC
            //
            if numComp == 3 {
                if blockIsConstant == 0 {
                    csc709_inverse_64(
                        (*chanData[0]).dct_data,
                        (*chanData[1]).dct_data,
                        (*chanData[2]).dct_data,
                    );
                } else {
                    csc709_inverse(
                        (*chanData[0]).dct_data,
                        (*chanData[1]).dct_data,
                        (*chanData[2]).dct_data,
                    );
                }
            }

            //
            // Float -> Half conversion.
            //
            // If the block has a constant value, just convert the first pixel.
            //
            for comp in 0..numComp {
                if blockIsConstant == 0 {
                    float_to_half(
                        rowBlock[comp].add(blockx * 64),
                        (*chanData[comp]).dct_data,
                    );
                } else {
                    // constant block
                    let val = float_to_half((*(*chanData[comp]).dct_data));
                    let dst = rowBlock[comp].add(blockx * 64);
                    for i in 0..64 {
                        ptr::write(dst.add(i), val);
                    }
                }
            }
        } // blockx
        //
        // At this point, we have half-float nonlinear value blocked
        // in rowBlock[][]. We need to unblock the data, transfer
        // back to linear, and write the results in the _rowPtrs[].
        //
        // There is a fast-path for aligned rows, which helps
        // things a little. Since this fast path is only valid
        // for full 8-element wide blocks, the partial x blocks
        // are broken into a separate loop below.
        //
        // At the moment, the fast path requires:
        //   * sse support
        //   * aligned row pointers
        //   * full 8-element wide blocks
        //
        // Unblock rowBlock into channel row pointers
        for comp in 0..numComp {
            // full-blocks fast path (non-SSE scalar implementation)
            //
            // Basic scalar kinda slow path for handling the full X blocks
            //
            if (*d)._toLinear != ptr::null() {
                for y in (8 * blocky)..(8 * blocky + maxY) {
                    let dst_row = (*chanData[comp]).rows.add(y as usize);
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
                    let dst_row = (*chanData[comp]).rows.add(y as usize);
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

            //
            // If we have partial X blocks, deal with all those now
            // Since this should be minimal work, there currently
            // is only one path that should work for everyone.
            //
            if numFullBlocksX != numBlocksX {
                for y in (8 * blocky)..(8 * blocky + maxY) {
                    let src = rowBlock[comp].add(numFullBlocksX * 64 + ((y & 0x7) * 8));
                    let dst_row = (*chanData[comp]).rows.add(y as usize);
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

    //
    // Walk over all the channels that are of type FLOAT.
    // Convert from HALF XDR back to FLOAT XDR.
    //

    for chan in 0..numComp {
        // chanData[chan]._type check - using _type field from channel struct
        if (*chanData[chan])._type as c_int != (2 as c_int) {
            continue;
        }
        for y in 0..(*d)._height {
            let float_ptr = (*chanData[chan]).rows.add(y as usize) as *mut f32;
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

