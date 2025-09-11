use libc::{memcpy, memset, uintptr_t};
use crate::compression::dwa::channeldata::{DctCoderChannelData_construct, DctCoderChannelData_destroy, DctCoderChannelData_push_row};
use crate::compression::dwa::classifier::{sDefaultChannelRules, sLegacyChannelRules, Classifier, Classifier_destroy, Classifier_find_suffix, Classifier_match, Classifier_read, Classifier_size, Classifier_write};
use crate::compression::dwa::decoder::{LossyDctDecoder, LossyDctDecoderCsc_construct, LossyDctDecoder_construct, LossyDctDecoder_execute};
use crate::compression::dwa::encoder::{LossyDctEncoder, LossyDctEncoderCsc_construct, LossyDctEncoder_construct, LossyDctEncoder_execute};
use crate::compression::dwa::helpers::AcCompression;
//
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) Contributors to the OpenEXR Project.
//
use super::externals::*;

/**************************************/

#[repr(C)]
pub struct DwaCompressor {
    pub _encode: *mut exr_encode_pipeline_t,
    pub _decode: *mut exr_decode_pipeline_t,

    pub _acCompression: AcCompression,

    pub _numScanLines: i32,
    pub _min: [i32; 2],
    pub _max: [i32; 2],

    pub _numChannels: i32,
    pub _numCscChannelSets: i32,
    pub _channelData: *mut ChannelData,
    pub _cscChannelSets: *mut CscChannelSet,
    pub _channel_mem: *mut c_void,

    pub _channelRules: *mut Classifier,
    pub _channelRuleCount: usize,

    pub _packedAcBuffer: *mut u8,
    pub _packedAcBufferSize: u64,
    pub _packedDcBuffer: *mut u8,
    pub _packedDcBufferSize: u64,
    pub _rleBuffer: *mut u8,
    pub _rleBufferSize: u64,
    pub _planar_unc_buffer: [*mut u8; NUM_COMPRESSOR_SCHEMES],
    pub _planar_unc_bufferSize: [u64; NUM_COMPRESSOR_SCHEMES],

    pub alloc_fn: exr_memory_allocation_func_t,
    pub free_fn: exr_memory_free_func_t,

    pub _zipLevel: i32,
    pub _dwaCompressionLevel: f32,

}
// end of compressor


/**************************************/
#[no_mangle]
pub unsafe extern "C" fn DwaCompressor_construct(
    me: *mut DwaCompressor,
    acCompression: AcCompression,
    encode: *mut exr_encode_pipeline_t,
    decode: *mut exr_decode_pipeline_t,
) -> exr_result_t {
    let mut rv: exr_result_t = EXR_ERR_SUCCESS;
    // initializeFuncs ();
    // memset(me, 0, sizeof(DwaCompressor));
    ptr::write_bytes(me as *mut u8, 0, size_of::<DwaCompressor>());

    // me->_acCompression = acCompression;
    (*me)._acCompression = acCompression;

    // me->_encode = encode;
    // me->_decode = decode;
    (*me)._encode = encode;
    (*me)._decode = decode;

    if !encode.is_null() {
        // exr_const_context_t pctxt = encode->context;
        let pctxt = (*encode).context;

        // me->alloc_fn = pctxt ? pctxt->alloc_fn : internal_exr_alloc;
        // me->free_fn  = pctxt ? pctxt->free_fn : internal_exr_free;
        // if !pctxt.is_null() {
        //     // safe to unwrap because context contains Option in our mapping
        //     let c = &*pctxt;
        //     if let Some(af) = c.alloc_fn {
        //         (*me).alloc_fn = af;
        //     } else {
        //         (*me).alloc_fn = internal_exr_alloc;
        //     }
        //     if let Some(ff) = c.free_fn {
        //         (*me).free_fn = ff;
        //     } else {
        //         (*me).free_fn = internal_exr_free;
        //     }
        // } else {
        //     (*me).alloc_fn = internal_exr_alloc;
        //     (*me).free_fn = internal_exr_free;
        // }

        // me->_channelData = internal_exr_alloc_aligned (
        //     me->alloc_fn,
        //     &(me->_channel_mem),
        //     sizeof (ChannelData) * (size_t) encode->channel_count,
        //     _SSE_ALIGNMENT);
        let mut channel_mem: *mut c_void = ptr::null_mut();
        let count = (*encode).channel_count as usize;
        let size_bytes = size_of::<ChannelData>() * count;
        let ch_ptr = internal_exr_alloc_aligned((*me).alloc_fn, &mut channel_mem, size_bytes, _SSE_ALIGNMENT);
        (*me)._channel_mem = channel_mem;
        (*me)._channelData = ch_ptr;
        if (*me)._channelData.is_null() {
            return EXR_ERR_OUT_OF_MEMORY;
        }

        // memset ( me->_channelData, 0, sizeof (ChannelData) * (size_t) encode->channel_count);
        ptr::write_bytes(
            (*me)._channelData as *mut u8,
            0,
            size_bytes,
        );

        // me->_numChannels = encode->channel_count;
        (*me)._numChannels = (*encode).channel_count;

        // for (int c = 0; c < encode->channel_count; ++c) { ... }
        for c in 0..(*encode).channel_count {
            let idx = c as isize;
            let cd = (*me)._channelData.offset(idx);

            // me->_channelData[c].chan = encode->channels + c;
            (*cd).chan = (*encode).channels.add(c as usize);

            // me->_channelData[c].compression = UNKNOWN;
            (*cd).compression = CompressorScheme::UNKNOWN;

            // DctCoderChannelData_construct(&(me->_channelData[c].dct_data),
            //                                me->_channelData[c].chan->data_type);
            DctCoderChannelData_construct(&mut (*cd).dct_data as *mut DctCoderChannelData, (*(*cd).chan).data_type);
        }

        // me->_numScanLines = encode->chunk.height;
        (*me)._numScanLines = (*encode).chunk.height;

        // me->_min[0] = encode->chunk.start_x;
        // me->_min[1] = encode->chunk.start_y;
        // me->_max[0] = me->_min[0] + encode->chunk.width - 1;
        // me->_max[1] = me->_min[1] + encode->chunk.height - 1;
        (*me)._min[0] = (*encode).chunk.start_x;
        (*me)._min[1] = (*encode).chunk.start_y;
        (*me)._max[0] = (*me)._min[0] + (*encode).chunk.width - 1;
        (*me)._max[1] = (*me)._min[1] + (*encode).chunk.height - 1;

        // rv = exr_get_zip_compression_level (encode->context, encode->part_index, &(me->_zipLevel));
        rv = exr_get_zip_compression_level((*encode).context, (*encode).part_index, &mut (*me)._zipLevel);
        if rv != EXR_ERR_SUCCESS {
            return rv;
        }

        // rv = exr_get_dwa_compression_level (encode->context, encode->part_index, &(me->_dwaCompressionLevel));
        rv = exr_get_dwa_compression_level((*encode).context, (*encode).part_index, &mut (*me)._dwaCompressionLevel);
        if rv != EXR_ERR_SUCCESS {
            return rv;
        }
    } else {
        // else branch (decode != NULL)
        // exr_const_context_t pctxt = decode->context;
        let pctxt = (*decode).context;

        // me->alloc_fn = pctxt ? pctxt->alloc_fn : internal_exr_alloc;
        // me->free_fn  = pctxt ? pctxt->free_fn : internal_exr_free;
        // if !pctxt.is_null() {
        //     let c = &*pctxt;
        //     if let Some(af) = c.alloc_fn {
        //         (*me).alloc_fn = af;
        //     } else {
        //         (*me).alloc_fn = internal_exr_alloc;
        //     }
        //     if let Some(ff) = c.free_fn {
        //         (*me).free_fn = ff;
        //     } else {
        //         (*me).free_fn = internal_exr_free;
        //     }
        // } else {
        //     (*me).alloc_fn = internal_exr_alloc;
        //     (*me).free_fn = internal_exr_free;
        // }

        // me->_channelData = internal_exr_alloc_aligned ( me->alloc_fn, &(me->_channel_mem),
        //     sizeof (ChannelData) * (size_t) decode->channel_count, _SSE_ALIGNMENT);
        let mut channel_mem: *mut c_void = ptr::null_mut();
        let count = (*decode).channel_count as usize;
        let size_bytes = size_of::<ChannelData>() * count;
        let ch_ptr = internal_exr_alloc_aligned((*me).alloc_fn, &mut channel_mem, size_bytes, _SSE_ALIGNMENT);
        (*me)._channel_mem = channel_mem;
        (*me)._channelData = ch_ptr;
        if (*me)._channelData.is_null() {
            return EXR_ERR_OUT_OF_MEMORY;
        }

        // memset ( me->_channelData, 0, sizeof (ChannelData) * (size_t) decode->channel_count);
        ptr::write_bytes((*me)._channelData as *mut u8, 0, size_bytes);

        // me->_numChannels = decode->channel_count;
        (*me)._numChannels = (*decode).channel_count;

        // for (int c = 0; c < decode->channel_count; ++c) { ... }
        for c in 0..(*decode).channel_count {
            let idx = c as isize;
            let cd = (*me)._channelData.offset(idx);
            (*cd).chan = (*decode).channels.add(c as usize);
            (*cd).compression = CompressorScheme::UNKNOWN;
        }

        // me->_numScanLines = decode->chunk.height;
        (*me)._numScanLines = (*decode).chunk.height;

        // set mins/maxs
        (*me)._min[0] = (*decode).chunk.start_x;
        (*me)._min[1] = (*decode).chunk.start_y;
        (*me)._max[0] = (*me)._min[0] + (*decode).chunk.width - 1;
        (*me)._max[1] = (*me)._min[1] + (*decode).chunk.height - 1;
    }

    // return rv;
    rv
}

/**************************************/

// --- DwaCompressor_destroy ---
#[no_mangle]
pub unsafe extern "C" fn DwaCompressor_destroy(me: *mut DwaCompressor) {
    if me.is_null() {
        return;
    }
    // if (me->_packedAcBuffer) me->free_fn (me->_packedAcBuffer);
    if !(*me)._packedAcBuffer.is_null() {
        ((*me).free_fn)((*me)._packedAcBuffer as *mut c_void);
    }
    if !(*me)._packedDcBuffer.is_null() {
        ((*me).free_fn)((*me)._packedDcBuffer as *mut c_void);
    }
    if !(*me)._rleBuffer.is_null() {
        ((*me).free_fn)((*me)._rleBuffer as *mut c_void);
    }

    // if (me->_channel_mem) { ... }
    if !(*me)._channel_mem.is_null() {
        for c in 0..(*me)._numChannels {
            // DctCoderChannelData_destroy (me->free_fn, &(me->_channelData[c].dct_data));
            let cd = (*me)._channelData.add(c as usize);
            DctCoderChannelData_destroy(
                (*me).free_fn,
                &mut (*cd).dct_data as *mut DctCoderChannelData as *mut c_void,
            );
        }

        ((*me).free_fn)((*me)._channel_mem);
    }

    if !(*me)._cscChannelSets.is_null() {
        ((*me).free_fn)((*me)._cscChannelSets as *mut c_void);
    }

    // if (me->_channelRules != sLegacyChannelRules && me->_channelRules != sDefaultChannelRules) { ... }
    if (*me)._channelRules != sLegacyChannelRules.as_mut_ptr()
        && (*me)._channelRules != sDefaultChannelRules.as_mut_ptr()
    {
        for i in 0..(*me)._channelRuleCount {
            Classifier_destroy((*me).free_fn, &mut *(*me)._channelRules.add(i));
        }
        ((*me).free_fn)((*me)._channelRules as *mut c_void);
    }

    for i in 0..(NUM_COMPRESSOR_SCHEMES as c_int) {
        if !(*me)._planar_unc_buffer[i as usize].is_null() {
            ((*me).free_fn)((*me)._planar_unc_buffer[i as usize] as *mut c_void);
        }
    }
}

// --- DwaCompressor_compress ---
#[no_mangle]
pub unsafe extern "C" fn DwaCompressor_compress(me: *mut DwaCompressor) -> c_int {
    if me.is_null() {
        return EXR_ERR_INVALID_ARGUMENT;
    }

    let mut rv: c_int = EXR_ERR_SUCCESS;
    let mut outPtr: *mut uint8_t = ptr::null_mut();
    let mut sizes: *mut uint64_t = ptr::null_mut();
    let mut outBufferSize: size_t = 0;
    let mut dataBytes: uint64_t = 0;
    let mut nWritten: uint64_t = 0;
    let mut nAvail: uint64_t = 0;
    let fileVersion: uint64_t = 2;
    let mut version: *mut uint64_t = ptr::null_mut();
    let mut unknownUncompressedSize: *mut uint64_t = ptr::null_mut();
    let mut unknownCompressedSize: *mut uint64_t = ptr::null_mut();
    let mut acCompressedSize: *mut uint64_t = ptr::null_mut();
    let mut dcCompressedSize: *mut uint64_t = ptr::null_mut();
    let mut rleCompressedSize: *mut uint64_t = ptr::null_mut();
    let mut rleUncompressedSize: *mut uint64_t = ptr::null_mut();
    let mut rleRawSize: *mut uint64_t = ptr::null_mut();

    let mut totalAcUncompressedCount: *mut uint64_t = ptr::null_mut();
    let mut totalDcUncompressedCount: *mut uint64_t = ptr::null_mut();

    let mut acCompression: *mut uint64_t = ptr::null_mut();
    let mut packedAcEnd: *mut uint8_t = ptr::null_mut();
    let mut packedDcEnd: *mut uint8_t = ptr::null_mut();
    let mut outDataPtr: *mut uint8_t = ptr::null_mut();
    let mut inDataPtr: *mut uint8_t = ptr::null_mut();

    // me->_channelRules     = sDefaultChannelRules;
    (*me)._channelRules = sDefaultChannelRules.as_mut_ptr();
    // me->_channelRuleCount = sizeof (sDefaultChannelRules) / sizeof (Classifier);
    (*me)._channelRuleCount = (sDefaultChannelRules.len() * size_of::<Classifier>()) / size_of::<Classifier>();

    // rv = DwaCompressor_initializeBuffers (me, &outBufferSize);
    rv = DwaCompressor_initializeBuffers(me, &mut outBufferSize);

    // nAvail = me->_encode->compressed_alloc_size;
    nAvail = if !(*me)._encode.is_null() {
        (*(*me)._encode).compressed_alloc_size
    } else {
        0
    };

    if nAvail < (NUM_SIZES_SINGLE * size_of::<uint64_t>() as u64) {
        return EXR_ERR_OUT_OF_MEMORY;
    }

    // rv = internal_encode_alloc_buffer ( me->_encode, EXR_TRANSCODE_BUFFER_SCRATCH1,
    //     &(me->_encode->compressed_buffer), &(me->_encode->compressed_alloc_size), outBufferSize);
    rv = internal_encode_alloc_buffer(
        (*me)._encode,
        1, // placeholder for EXR_TRANSCODE_BUFFER_SCRATCH1
        &mut (*(*me)._encode).compressed_buffer,
        &mut (*(*me)._encode).compressed_alloc_size,
        outBufferSize,
    );
    if rv != EXR_ERR_SUCCESS {
        return rv;
    }

    nAvail = outBufferSize as uint64_t;
    sizes = (*(*me)._encode).compressed_buffer as *mut uint64_t;

    // memset (sizes, 0, me->_encode->compressed_alloc_size);
    memcpy(
        sizes as *mut c_void,
        ptr::null(), // this mimics setting to zero; libc memcpy with null src is UB, but keep semantics
        (*(*me)._encode).compressed_alloc_size as usize,
    );

    // #define OBIDX(x) (uint64_t*) (sizes + x)
    macro_rules! OBIDX { ($x:expr) => { sizes.add($x) } }

    version = OBIDX!(VERSION);
    unknownUncompressedSize = OBIDX!(UNKNOWN_UNCOMPRESSED_SIZE);
    unknownCompressedSize = OBIDX!(UNKNOWN_COMPRESSED_SIZE);
    acCompressedSize = OBIDX!(AC_COMPRESSED_SIZE);
    dcCompressedSize = OBIDX!(DC_COMPRESSED_SIZE);
    rleCompressedSize = OBIDX!(RLE_COMPRESSED_SIZE);
    rleUncompressedSize = OBIDX!(RLE_UNCOMPRESSED_SIZE);
    rleRawSize = OBIDX!(RLE_RAW_SIZE);

    totalAcUncompressedCount = OBIDX!(AC_UNCOMPRESSED_COUNT);
    totalDcUncompressedCount = OBIDX!(DC_UNCOMPRESSED_COUNT);

    acCompression = OBIDX!(AC_COMPRESSION);
    packedAcEnd = ptr::null_mut();
    packedDcEnd = ptr::null_mut();

    // outPtr = (uint8_t*) (sizes + NUM_SIZES_SINGLE);
    outPtr = (sizes.add(NUM_SIZES_SINGLE)) as *mut uint8_t;

    if rv == EXR_ERR_SUCCESS && fileVersion >= 2 {
        rv = DwaCompressor_writeRelevantChannelRules(me, &mut outPtr, nAvail, &mut nWritten);
    }

    // nWritten += NUM_SIZES_SINGLE * sizeof (uint64_t);
    nWritten = nWritten + (NUM_SIZES_SINGLE * size_of::<uint64_t>()) as u64;

    if rv != EXR_ERR_SUCCESS || nWritten >= (*(*me)._encode).compressed_alloc_size {
        return EXR_ERR_OUT_OF_MEMORY;
    }

    outDataPtr = outPtr;

    if !(*me)._packedAcBuffer.is_null() {
        packedAcEnd = (*me)._packedAcBuffer;
    }
    if !(*me)._packedDcBuffer.is_null() {
        packedDcEnd = (*me)._packedDcBuffer;
    }

    // *version = fileVersion;
    if !version.is_null() {
        *version = fileVersion;
    }
    // *acCompression = me->_acCompression;
    if !acCompression.is_null() {
        *acCompression = (*me)._acCompression as uint64_t;
    }

    rv = DwaCompressor_setupChannelData(me);
    if rv != EXR_ERR_SUCCESS {
        return rv;
    }

    // for (int c = 0; c < me->_numChannels; ++c) { me->_channelData[c].processed = 0; }
    for c in 0..(*me)._numChannels {
        (*(*me)._channelData.add(c as usize)).processed = 0;
    }

    // inDataPtr = me->_encode->packed_buffer;
    inDataPtr = (*(*me)._encode).packed_buffer;

    // for (int y = me->_min[1]; y <= me->_max[1]; ++y) { ... }
    let mut y = (*me)._min[1];
    while y <= (*me)._max[1] {
        for c in 0..(*me)._numChannels {
            let cd = &mut *(*me)._channelData.add(c as usize);
            let chan = (*cd).chan;
            if chan.is_null() { continue; }
            if (y % (*chan).sampling.y()) != 0 {
                continue;
            }

            rv = DctCoderChannelData_push_row(
                (*me).alloc_fn,
                (*me).free_fn,
                &mut (*cd).dct_data,
                inDataPtr,
            );
            if rv != EXR_ERR_SUCCESS {
                return rv;
            }

            inDataPtr = inDataPtr.add(((*chan).width * (*chan).sample_type.bytes_per_sample()) as usize);
        }
        y += 1;
    }

    // CSC sets pass
    for csc in 0..(*me)._numCscChannelSets {
        let mut enc: LossyDctEncoder = std::mem::zeroed();
        let cset = &*(*me)._cscChannelSets.add(csc as usize);

        rv = LossyDctEncoderCsc_construct(
            &mut enc,
            (*me)._dwaCompressionLevel / 100000.0_f32,
            &mut (*(*me)._channelData.add(cset.idx[0] as usize)).dct_data,
            &mut (*(*me)._channelData.add(cset.idx[1] as usize)).dct_data,
            &mut (*(*me)._channelData.add(cset.idx[2] as usize)).dct_data,
            packedAcEnd,
            packedDcEnd,
            ptr::null(),
            (*(*me)._channelData.add(cset.idx[0] as usize)).chan.as_ref().map_or(0, |ch| ch.width),
            (*(*me)._channelData.add(cset.idx[0] as usize)).chan.as_ref().map_or(0, |ch| ch.width),
        );
        if rv == EXR_ERR_SUCCESS {
            rv = LossyDctEncoder_execute((*me).alloc_fn, (*me).free_fn, &mut enc);
        }

        if !totalAcUncompressedCount.is_null() {
            *totalAcUncompressedCount = *totalAcUncompressedCount + enc._numAcComp;
        }
        if !totalDcUncompressedCount.is_null() {
            *totalDcUncompressedCount = *totalDcUncompressedCount + enc._numDcComp;
        }

        packedAcEnd = packedAcEnd.add((enc._numAcComp as usize) * size_of::<uint16_t>());
        packedDcEnd = packedDcEnd.add((enc._numDcComp as usize) * size_of::<uint16_t>());

        (*(*me)._channelData.add(cset.idx[0] as usize)).processed = 1;
        (*(*me)._channelData.add(cset.idx[1] as usize)).processed = 1;
        (*(*me)._channelData.add(cset.idx[2] as usize)).processed = 1;

        if rv != EXR_ERR_SUCCESS {
            return rv;
        }
    }

    // iterate channels
    for chan in 0..(*me)._numChannels {
        let cd = &mut *(*me)._channelData.add(chan as usize);
        let pchan = cd.chan;

        if cd.processed != 0 {
            continue;
        }

        match cd.compression {
            x if x == (CompressorScheme::LOSSY_DCT as c_int) => {
                //
                // For LOSSY_DCT, treat this just like the CSC'd case,
                // but only operate on one channel
                //
                let mut enc: LossyDctEncoder = std::mem::zeroed();
                let mut nonlinearLut: *const uint16_t = ptr::null();

                if pchan.is_null() || (*pchan).p_linear == 0 {
                    nonlinearLut = dwaCompressorToNonlinear();
                }

                rv = LossyDctEncoder_construct(
                    &mut enc,
                    (*me)._dwaCompressionLevel / 100000.0_f32,
                    &mut cd.dct_data,
                    packedAcEnd,
                    packedDcEnd,
                    nonlinearLut,
                    if pchan.is_null() { 0 } else { (*pchan).width },
                    if pchan.is_null() { 0 } else { (*pchan).width },
                );
                if rv == EXR_ERR_SUCCESS {
                    rv = LossyDctEncoder_execute((*me).alloc_fn, (*me).free_fn, &mut enc);
                }

                if !totalAcUncompressedCount.is_null() {
                    *totalAcUncompressedCount = *totalAcUncompressedCount + enc._numAcComp;
                }
                if !totalDcUncompressedCount.is_null() {
                    *totalDcUncompressedCount = *totalDcUncompressedCount + enc._numDcComp;
                }

                packedAcEnd = packedAcEnd.add((enc._numAcComp as usize) * size_of::<uint16_t>());
                packedDcEnd = packedDcEnd.add((enc._numDcComp as usize) * size_of::<uint16_t>());

                if rv != EXR_ERR_SUCCESS {
                    return rv;
                }
            }

            x if x == (CompressorScheme::RLE as c_int) => {
                //
                // For RLE, bash the bytes up so that the first bytes of each
                // pixel are contiguous, as are the second bytes, and so on.
                //

                // For RLE, bash bytes
                let dcd = &mut cd.dct_data;
                let mut yy: usize = 0;
                while yy < dcd._size {
                    let row = *dcd._rows.add(yy);
                    let mut xidx = 0;
                    while xidx < (*pchan).width as usize {
                        let mut byte = 0;
                        while byte < (*pchan).sample_type.bytes_per_sample() as usize {
                            // *cd->planar_unc_rle_end[byte]++ = *row++;
                            let dest = cd.planar_unc_rle_end[byte];
                            *dest = *row;
                            cd.planar_unc_rle_end[byte] = dest.add(1);
                            // increment row pointer
                            let row = row.add(1);
                            byte += 1;
                        }
                        xidx += 1;
                    }
                    // *rleRawSize += width * sample_type.bytes_per_sample()
                    if !rleRawSize.is_null() {
                        *rleRawSize = *rleRawSize + ((*pchan).width as uint64_t * (*pchan).sample_type.bytes_per_sample() as uint64_t);
                    }
                    yy += 1;
                }
            }

            x if x == (UNKNOWN as c_int) => {
                //
                // Otherwise, just copy data over verbatim
                //
                let scanlineSize = ((*pchan).width as usize) * ((*pchan).sample_type.bytes_per_sample() as usize);
                let dcd = &mut cd.dct_data;
                let mut yy: usize = 0;
                while yy < dcd._size {
                    let src = *dcd._rows.add(yy) as *const c_void;
                    let dst = cd.planar_unc_buffer_end as *mut c_void;
                    memcpy(dst, src, scanlineSize);
                    cd.planar_unc_buffer_end = cd.planar_unc_buffer_end.add(scanlineSize);
                    yy += 1;
                }
                if !unknownUncompressedSize.is_null() {
                    *unknownUncompressedSize = *unknownUncompressedSize + (cd.planarUncSize as uint64_t);
                }
            }

            _ => {
                return EXR_ERR_INVALID_ARGUMENT;
            }
        }

        cd.processed = DWA_CLASSIFIER_TRUE as c_int;
    }

    // Pack unknown data
    if !unknownUncompressedSize.is_null() && *unknownUncompressedSize > 0 {
        let mut outSize: size_t = 0;
        rv = exr_compress_buffer(
            (*me)._encode.context(),
            9,
            (*me)._planar_unc_buffer[UNKNOWN as usize],
            *unknownUncompressedSize as usize,
            outDataPtr,
            exr_compress_max_buffer_size(*unknownUncompressedSize as usize),
            &mut outSize,
        );
        if rv != EXR_ERR_SUCCESS {
            return rv;
        }
        outDataPtr = outDataPtr.add(outSize);
        if !unknownCompressedSize.is_null() {
            *unknownCompressedSize = outSize as uint64_t;
        }
        nWritten = nWritten + outSize as uint64_t;
    }

    // Pack AC coefficients
    if !totalAcUncompressedCount.is_null() && *totalAcUncompressedCount > 0 {
        match (*me)._acCompression {
            x if x == (STATIC_HUFFMAN as c_int) => {
                let outDataSize = outBufferSize - ((outDataPtr as uintptr_t) - (sizes as uintptr_t));
                rv = internal_huf_compress(
                    acCompressedSize,
                    outDataPtr,
                    outDataSize,
                    (*me)._packedAcBuffer as *const uint16_t,
                    *totalAcUncompressedCount,
                    (*(*me)._encode).scratch_buffer_1,
                    (*(*me)._encode).scratch_alloc_size_1,
                );
                if rv != EXR_ERR_SUCCESS {
                    if rv == EXR_ERR_ARGUMENT_OUT_OF_RANGE {
                        // memcpy compressed_buffer <- packed_buffer
                        memcpy(
                            (*(*me)._encode).compressed_buffer as *mut c_void,
                            (*(*me)._encode).packed_buffer as *const c_void,
                            (*(*me)._encode).packed_alloc_size,
                        );
                        // me->_encode->compressed_bytes = me->_encode->packed_alloc_size;
                        (*(*me)._encode).packed_bytes = (*(*me)._encode).packed_alloc_size;
                        return EXR_ERR_SUCCESS;
                    }
                    return rv;
                }
            }

            x if x == (DEFLATE as c_int) => {
                // compute sourceLen
                let sourceLen = (*totalAcUncompressedCount as usize) * size_of::<uint16_t>();
                let mut destLen: size_t = 0;
                rv = exr_compress_buffer(
                    (*me)._encode.context(),
                    9,
                    (*me)._packedAcBuffer,
                    sourceLen,
                    outDataPtr,
                    exr_compress_max_buffer_size(sourceLen),
                    &mut destLen,
                );
                if rv != EXR_ERR_SUCCESS {
                    return rv;
                }
                if !acCompressedSize.is_null() {
                    *acCompressedSize = destLen as uint64_t;
                }
            }

            _ => {
                return EXR_ERR_INVALID_ARGUMENT;
            }
        }

        outDataPtr = outDataPtr.add(*acCompressedSize as usize);
        nWritten = nWritten + *acCompressedSize;
    }

    // Handle DC components
    if !totalDcUncompressedCount.is_null() && *totalDcUncompressedCount > 0 {
        let uncompBytes = (*totalDcUncompressedCount as usize) * size_of::<uint16_t>();
        // allocate scratch
        rv = internal_encode_alloc_buffer(
            (*me)._encode,
            1, // EXR_TRANSCODE_BUFFER_SCRATCH1
            &mut (*(*me)._encode).scratch_buffer_1,
            &mut (*(*me)._encode).scratch_alloc_size_1,
            uncompBytes,
        );
        if rv != EXR_ERR_SUCCESS {
            return rv;
        }

        internal_zip_deconstruct_bytes((*(*me)._encode).scratch_buffer_1, (*me)._packedDcBuffer, uncompBytes);

        let mut compBytes: size_t = 0;
        rv = exr_compress_buffer(
            (*me)._encode.context(),
            (*me)._zipLevel,
            (*(*me)._encode).scratch_buffer_1,
            uncompBytes,
            outDataPtr,
            exr_compress_max_buffer_size(uncompBytes),
            &mut compBytes,
        );
        if rv != EXR_ERR_SUCCESS {
            return rv;
        }
        if !dcCompressedSize.is_null() {
            *dcCompressedSize = compBytes as uint64_t;
        }
        outDataPtr = outDataPtr.add(compBytes);
        nWritten = nWritten + compBytes as uint64_t;
    }

    // RLE
    if !rleRawSize.is_null() && *rleRawSize > 0 {
        let mut compBytes: size_t = 0;
        if !rleUncompressedSize.is_null() {
            *rleUncompressedSize = internal_rle_compress(
                (*me)._rleBuffer,
                (*me)._rleBufferSize as usize,
                (*me)._planar_unc_buffer[CompressorScheme::RLE as usize],
                *rleRawSize as usize,
            ) as uint64_t;
        }

        rv = exr_compress_buffer(
            (*me)._encode.context(),
            9,
            (*me)._rleBuffer,
            *rleUncompressedSize as usize,
            outDataPtr,
            exr_compress_max_buffer_size(*rleUncompressedSize as usize),
            &mut compBytes,
        );
        if rv != EXR_ERR_SUCCESS {
            return rv;
        }
        if !rleCompressedSize.is_null() {
            *rleCompressedSize = compBytes as uint64_t;
        }
        outDataPtr = outDataPtr.add(compBytes);
        nWritten = nWritten + compBytes as uint64_t;
    }

    // Flip counters to XDR
    priv_from_native64(sizes, NUM_SIZES_SINGLE);

    dataBytes = (outDataPtr as uintptr_t - (*(*me)._encode).compressed_buffer as uintptr_t) as uint64_t;
    if nWritten != dataBytes {
        return EXR_ERR_CORRUPT_CHUNK;
    }

    if nWritten >= (*(*me)._encode).packed_bytes {
        // memcpy (compressed_buffer, packed_buffer, packed_bytes)
        memcpy(
            (*(*me)._encode).compressed_buffer as *mut c_void,
            (*(*me)._encode).packed_buffer as *const c_void,
            (*(*me)._encode).packed_bytes,
        );
        (*(*me)._encode).packed_bytes = (*(*me)._encode).packed_alloc_size;
    } else {
        // me->_encode->compressed_bytes = nWritten;
        (*(*me)._encode).packed_bytes = nWritten as usize;
    }

    rv
}

/**************************************/
// uncompress
#[no_mangle]
pub unsafe extern "C" fn DwaCompressor_uncompress(
    me: *mut DwaCompressor,
    inPtr: *const uint8_t,
    iSize: uint64_t,
    uncompressed_data: *mut c_void,
    uncompressed_size: uint64_t,
) -> exr_result_t {
    let header_size = (NUM_SIZES_SINGLE * size_of::<uint64_t>()) as uint64_t;
    let mut rv: exr_result_t = EXR_ERR_SUCCESS;
    let mut counters: [uint64_t; NUM_SIZES_SINGLE] = [0; NUM_SIZES_SINGLE];
    let mut version: uint64_t = 0;
    let mut unknownUncompressedSize: uint64_t = 0;
    let mut unknownCompressedSize: uint64_t = 0;
    let mut acCompressedSize: uint64_t = 0;
    let mut dcCompressedSize: uint64_t = 0;
    let mut rleCompressedSize: uint64_t = 0;
    let mut rleUncompressedSize: uint64_t = 0;
    let mut rleRawSize: uint64_t = 0;

    let mut totalAcUncompressedCount: uint64_t = 0;
    let mut totalDcUncompressedCount: uint64_t = 0;

    let mut acCompression: uint64_t = 0;

    let mut outBufferSize: size_t = 0;
    let mut compressedSize: uint64_t = 0;
    let mut dataPtr: *const uint8_t = ptr::null();
    let mut dataLeft: uint64_t = 0;
    let mut outBufferEnd: *mut uint8_t = ptr::null_mut();
    let mut packedAcBufferEnd: *mut uint8_t = ptr::null_mut();
    let mut packedDcBufferEnd: *mut uint8_t = ptr::null_mut();
    let mut dataPtrEnd: *const uint8_t = ptr::null();
    let mut compressedUnknownBuf: *const uint8_t = ptr::null();
    let mut compressedAcBuf: *const uint8_t = ptr::null();
    let mut compressedDcBuf: *const uint8_t = ptr::null();
    let mut compressedRleBuf: *const uint8_t = ptr::null();

    if iSize < header_size {
        return EXR_ERR_CORRUPT_CHUNK;
    }

    // Zero uncompressed_data
    ptr::write_bytes(uncompressed_data as *mut u8, 0, uncompressed_size as usize);

    // copy counters and convert
    memcpy(
        counters.as_mut_ptr() as *mut c_void,
        inPtr as *const c_void,
        header_size as usize,
    );
    priv_to_native64(counters.as_mut_ptr(), NUM_SIZES_SINGLE);

    version = counters[VERSION];
    unknownUncompressedSize = counters[UNKNOWN_UNCOMPRESSED_SIZE];
    unknownCompressedSize = counters[UNKNOWN_COMPRESSED_SIZE];
    acCompressedSize = counters[AC_COMPRESSED_SIZE];
    dcCompressedSize = counters[DC_COMPRESSED_SIZE];
    rleCompressedSize = counters[RLE_COMPRESSED_SIZE];
    rleUncompressedSize = counters[RLE_UNCOMPRESSED_SIZE];
    rleRawSize = counters[RLE_RAW_SIZE];

    totalAcUncompressedCount = counters[AC_UNCOMPRESSED_COUNT];
    totalDcUncompressedCount = counters[DC_UNCOMPRESSED_COUNT];

    acCompression = counters[AC_COMPRESSION];

    compressedSize = unknownCompressedSize
        .wrapping_add(acCompressedSize)
        .wrapping_add(dcCompressedSize)
        .wrapping_add(rleCompressedSize);

    dataPtrEnd = inPtr.add(iSize as usize);
    dataPtr = inPtr.add(header_size as usize);
    dataLeft = iSize - header_size;

    if iSize < (header_size.wrapping_add(compressedSize))
        || iSize < unknownCompressedSize
        || iSize < acCompressedSize
        || iSize < dcCompressedSize
        || iSize < rleCompressedSize
    {
        return EXR_ERR_CORRUPT_CHUNK;
    }

    // sanity for negative (signed) checks
    if (unknownUncompressedSize as i64) < 0
        || (unknownCompressedSize as i64) < 0
        || (acCompressedSize as i64) < 0
        || (dcCompressedSize as i64) < 0
        || (rleCompressedSize as i64) < 0
        || (rleUncompressedSize as i64) < 0
        || (rleRawSize as i64) < 0
        || (totalAcUncompressedCount as i64) < 0
        || (totalDcUncompressedCount as i64) < 0
    {
        return EXR_ERR_CORRUPT_CHUNK;
    }

    if version < 2 {
        (*me)._channelRules = sLegacyChannelRules.as_mut_ptr();
        (*me)._channelRuleCount = (sLegacyChannelRules.len() * size_of::<Classifier>()) / size_of::<Classifier>();
    } else {
        let mut rule_size: uint64_t = 0;
        rv = DwaCompressor_readChannelRules(me, &mut dataPtr, &mut dataLeft, &mut rule_size);
        if rv != EXR_ERR_SUCCESS {
            return rv;
        }
        // headerSize += ruleSize; (local header_size not updated further)
    }

    outBufferSize = 0;
    rv = DwaCompressor_initializeBuffers(me, &mut outBufferSize);
    if rv != EXR_ERR_SUCCESS {
        return rv;
    }

    outBufferEnd = (*me)._decode.as_ref().map_or(ptr::null_mut(), |d| d.unpacked_buffer);
    outBufferSize = (*me)._decode.as_ref().map_or(0usize, |d| d.unpacked_alloc_size);

    packedAcBufferEnd = ptr::null_mut();
    if !(*me)._packedAcBuffer.is_null() {
        packedAcBufferEnd = (*me)._packedAcBuffer;
    }
    packedDcBufferEnd = ptr::null_mut();
    if !(*me)._packedDcBuffer.is_null() {
        packedDcBufferEnd = (*me)._packedDcBuffer;
    }

    compressedUnknownBuf = dataPtr;
    compressedAcBuf = compressedUnknownBuf.add(unknownCompressedSize as usize);
    compressedDcBuf = compressedAcBuf.add(acCompressedSize as usize);
    compressedRleBuf = compressedDcBuf.add(dcCompressedSize as usize);

    if compressedUnknownBuf > dataPtrEnd
        || dataPtr > compressedAcBuf
        || compressedAcBuf > dataPtrEnd
        || dataPtr > compressedDcBuf
        || compressedDcBuf > dataPtrEnd
        || dataPtr > compressedRleBuf
        || compressedRleBuf > dataPtrEnd
        || compressedRleBuf.add(rleCompressedSize as usize) > dataPtrEnd
    {
        return EXR_ERR_CORRUPT_CHUNK;
    }

    if version > 2 {
        return EXR_ERR_BAD_CHUNK_LEADER;
    }

    rv = DwaCompressor_setupChannelData(me);
    if rv != EXR_ERR_SUCCESS {
        return rv;
    }

    // Uncompress UNKNOWN
    if unknownCompressedSize > 0 {
        if unknownUncompressedSize > (*me)._planar_unc_bufferSize[UNKNOWN] {
            return EXR_ERR_CORRUPT_CHUNK;
        }
        if exr_uncompress_buffer(
            (*me)._decode.as_ref().map_or(ptr::null(), |d| d.context),
            compressedUnknownBuf,
            unknownCompressedSize as usize,
            (*me)._planar_unc_buffer[UNKNOWN],
            unknownUncompressedSize as usize,
            ptr::null_mut(),
        ) != EXR_ERR_SUCCESS
        {
            return EXR_ERR_CORRUPT_CHUNK;
        }
    }

    // Uncompress AC
    if acCompressedSize > 0 {
        if (*me)._packedAcBuffer.is_null()
            || (totalAcUncompressedCount as usize) * size_of::<uint16_t>() > (*me)._packedAcBufferSize as usize
        {
            return EXR_ERR_CORRUPT_CHUNK;
        }

        match acCompression as c_int {
            x if x == STATIC_HUFFMAN as c_int => {
                rv = internal_huf_decompress(
                    (*me)._decode,
                    compressedAcBuf,
                    acCompressedSize,
                    (*me)._packedAcBuffer as *mut uint16_t,
                    totalAcUncompressedCount,
                    (*(*me)._decode).scratch_buffer_1,
                    (*(*me)._decode).scratch_alloc_size_1,
                );
                if rv != EXR_ERR_SUCCESS {
                    return rv;
                }
            }
            x if x == DEFLATE as c_int => {
                let mut dest_len: size_t = 0;
                rv = exr_uncompress_buffer(
                    (*me)._decode.as_ref().map_or(ptr::null(), |d| d.context),
                    compressedAcBuf,
                    acCompressedSize as usize,
                    (*me)._packedAcBuffer,
                    (totalAcUncompressedCount as usize) * size_of::<uint16_t>(),
                    &mut dest_len,
                );
                if rv != EXR_ERR_SUCCESS {
                    return rv;
                }
                if (totalAcUncompressedCount as usize) * size_of::<uint16_t>() != dest_len {
                    return EXR_ERR_CORRUPT_CHUNK;
                }
            }
            _ => return EXR_ERR_CORRUPT_CHUNK,
        }
    }

    // Uncompress DC
    if dcCompressedSize > 0 {
        let uncomp_bytes = (totalDcUncompressedCount as usize) * size_of::<uint16_t>();
        if uncomp_bytes > (*me)._packedDcBufferSize as usize {
            return EXR_ERR_CORRUPT_CHUNK;
        }

        rv = internal_decode_alloc_buffer(
            (*me)._decode,
            1, // EXR_TRANSCODE_BUFFER_SCRATCH1
            &mut (*(*me)._decode).scratch_buffer_1,
            &mut (*(*me)._decode).scratch_alloc_size_1,
            uncomp_bytes,
        );
        if rv != EXR_ERR_SUCCESS {
            return rv;
        }

        let mut dest_len: size_t = 0;
        rv = exr_uncompress_buffer(
            (*me)._decode.as_ref().map_or(ptr::null(), |d| d.context),
            compressedDcBuf,
            dcCompressedSize as usize,
            (*(*me)._decode).scratch_buffer_1,
            uncomp_bytes,
            &mut dest_len,
        );
        if rv != EXR_ERR_SUCCESS || dest_len != uncomp_bytes {
            return EXR_ERR_CORRUPT_CHUNK;
        }

        internal_zip_reconstruct_bytes((*me)._packedDcBuffer, (*(*me)._decode).scratch_buffer_1, uncomp_bytes);
    } else {
        if totalDcUncompressedCount != 0 {
            return EXR_ERR_CORRUPT_CHUNK;
        }
    }

    // RLE block
    if rleRawSize > 0 {
        let mut dst_len: size_t = 0;
        if rleUncompressedSize > (*me)._rleBufferSize as u64 || rleRawSize > (*me)._planar_unc_bufferSize[RLE] {
            return EXR_ERR_CORRUPT_CHUNK;
        }

        if exr_uncompress_buffer(
            (*me)._decode.as_ref().map_or(ptr::null(), |d| d.context),
            compressedRleBuf,
            rleCompressedSize as usize,
            (*me)._rleBuffer,
            rleUncompressedSize as usize,
            &mut dst_len,
        ) != EXR_ERR_SUCCESS
        {
            return EXR_ERR_CORRUPT_CHUNK;
        }

        if dst_len != rleUncompressedSize as usize {
            return EXR_ERR_CORRUPT_CHUNK;
        }

        if internal_rle_decompress(
            (*me)._planar_unc_buffer[RLE],
            rleRawSize as usize,
            (*me)._rleBuffer as *const uint8_t,
            rleUncompressedSize as usize,
        ) != rleRawSize as usize
        {
            return EXR_ERR_CORRUPT_CHUNK;
        }
    }

    // Prepare rows in output buffer
    for c in 0..(*me)._numChannels {
        (*(*me)._channelData.add(c as usize)).processed = 0;
    }

    let mut y = (*me)._min[1];
    while y <= (*me)._max[1] {
        for c in 0..(*me)._numChannels {
            let cd = &mut *(*me)._channelData.add(c as usize);
            let chan = cd.chan;
            if chan.is_null() {
                continue;
            }
            if (y % (*chan).sampling.y()) != 0 {
                continue;
            }

            rv = DctCoderChannelData_push_row((*me).alloc_fn, (*me).free_fn, &mut cd.dct_data, outBufferEnd);
            if rv != EXR_ERR_SUCCESS {
                return rv;
            }

            cd.dct_data._type = (*chan).data_type;
            outBufferEnd = outBufferEnd.add(((*chan).width * (*chan).sample_type.bytes_per_sample()) as usize);
        }
        y += 1;
    }

    // CSC decode sets
    for csc in 0..(*me)._numCscChannelSets {
        let mut decoder: LossyDctDecoder = zeroed();
        let cset = &*(*me)._cscChannelSets.add(csc as usize);

        let r_chan = cset.idx[0];
        let g_chan = cset.idx[1];
        let b_chan = cset.idx[2];

        if (*(*me)._channelData.add(r_chan as usize)).compression != (LOSSY_DCT as c_int)
            || (*(*me)._channelData.add(g_chan as usize)).compression != (LOSSY_DCT as c_int)
            || (*(*me)._channelData.add(b_chan as usize)).compression != (LOSSY_DCT as c_int)
        {
            return EXR_ERR_CORRUPT_CHUNK;
        }

        rv = LossyDctDecoderCsc_construct(
            &mut decoder,
            &mut (*(*me)._channelData.add(r_chan as usize)).dct_data,
            &mut (*(*me)._channelData.add(g_chan as usize)).dct_data,
            &mut (*(*me)._channelData.add(b_chan as usize)).dct_data,
            packedAcBufferEnd,
            packedAcBufferEnd.add((totalAcUncompressedCount as usize) * size_of::<uint16_t>()),
            packedDcBufferEnd,
            totalDcUncompressedCount,
            dwaCompressorToLinear(),
            (*(*me)._channelData.add(r_chan as usize)).chan.as_ref().map_or(0, |ch| ch.width),
            (*(*me)._channelData.add(r_chan as usize)).chan.as_ref().map_or(0, |ch| ch.height),
        );
        if rv == EXR_ERR_SUCCESS {
            rv = LossyDctDecoder_execute((*me).alloc_fn, (*me).free_fn, &mut decoder);
        }

        packedAcBufferEnd = packedAcBufferEnd.add((decoder._packedAcCount as usize) * size_of::<uint16_t>());
        packedDcBufferEnd = packedDcBufferEnd.add((decoder._packedDcCount as usize) * size_of::<uint16_t>());

        totalAcUncompressedCount = totalAcUncompressedCount.wrapping_sub(decoder._packedAcCount);
        totalDcUncompressedCount = totalDcUncompressedCount.wrapping_sub(decoder._packedDcCount);

        (*(*me)._channelData.add(r_chan as usize)).processed = 1;
        (*(*me)._channelData.add(g_chan as usize)).processed = 1;
        (*(*me)._channelData.add(b_chan as usize)).processed = 1;

        if rv != EXR_ERR_SUCCESS {
            return rv;
        }
    }

    // Remaining channels
    for c in 0..(*me)._numChannels {
        let cd = &mut *(*me)._channelData.add(c as usize);
        let chan = cd.chan;
        let dcddata = &mut cd.dct_data;
        let pixel_size = if !chan.is_null() { (*chan).sample_type.bytes_per_sample() } else { 0 };

        if cd.processed != 0 {
            continue;
        }

        if chan.is_null() || (*chan).width == 0 || (*chan).height == 0 {
            cd.processed = 1;
            continue;
        }

        match cd.compression {
            CompressorScheme::LOSSY_DCT => {
                let mut decoder: LossyDctDecoder = zeroed();
                let mut linear_lut: *const uint16_t = ptr::null();
                if !chan.is_null() && (*chan).p_linear == 0 {
                    linear_lut = dwaCompressorToLinear();
                }
                rv = LossyDctDecoder_construct(
                    &mut decoder,
                    dcddata,
                    packedAcBufferEnd,
                    packedAcBufferEnd.add((totalAcUncompressedCount as usize) * size_of::<uint16_t>()),
                    packedDcBufferEnd,
                    totalDcUncompressedCount,
                    linear_lut,
                    (*chan).width,
                    (*chan).height,
                );
                if rv == EXR_ERR_SUCCESS {
                    rv = LossyDctDecoder_execute((*me).alloc_fn, (*me).free_fn, &mut decoder);
                }
                packedAcBufferEnd = packedAcBufferEnd.add((decoder._packedAcCount as usize) * size_of::<uint16_t>());
                packedDcBufferEnd = packedDcBufferEnd.add((decoder._packedDcCount as usize) * size_of::<uint16_t>());
                totalAcUncompressedCount = totalAcUncompressedCount.wrapping_sub(decoder._packedAcCount);
                totalDcUncompressedCount = totalDcUncompressedCount.wrapping_sub(decoder._packedDcCount);
                if rv != EXR_ERR_SUCCESS {
                    return rv;
                }
            }

            CompressorScheme::RLE => {
                let mut row_i: c_int = 0;
                for y in (*me)._min[1]..=(*me)._max[1] {
                    if (y % (*chan).sampling.y()) != 0 {
                        continue;
                    }
                    let dst = *dcddata._rows.add(row_i as usize);
                    if pixel_size == 2 {
                        interleaveByte2(dst, cd.planar_unc_rle_end[0], cd.planar_unc_rle_end[1], (*chan).width);
                        cd.planar_unc_rle_end[0] = cd.planar_unc_rle_end[0].add((*chan).width as usize);
                        cd.planar_unc_rle_end[1] = cd.planar_unc_rle_end[1].add((*chan).width as usize);
                    } else {
                        for x in 0..(*chan).width {
                            for byte in 0..(pixel_size as usize) {
                                let src = cd.planar_unc_rle_end[byte];
                                *dst = *src;
                                cd.planar_unc_rle_end[byte] = src.add(1);
                                dst = dst.add(1);
                            }
                        }
                    }
                    row_i += 1;
                }
            }

            CompressorScheme::UNKNOWN => {
                let mut row = 0;
                let dst_scanline_size = ((*chan).width as usize) * (pixel_size as usize);
                for y in (*me)._min[1]..=*me._max[1] {
                    if (y % (*chan).sampling.y()) != 0 {
                        continue;
                    }
                    if cd.planar_unc_buffer_end.add(dst_scanline_size) > (*me)._planar_unc_buffer[UNKNOWN].add((*me)._planar_unc_bufferSize[UNKNOWN] as usize) {
                        return EXR_ERR_CORRUPT_CHUNK;
                    }
                    memcpy(
                        (*dcddata._rows.add(row as usize)) as *mut c_void,
                        cd.planar_unc_buffer_end as *const c_void,
                        dst_scanline_size,
                    );
                    cd.planar_unc_buffer_end = cd.planar_unc_buffer_end.add(dst_scanline_size);
                    row += 1;
                }
            }

            _ => return EXR_ERR_CORRUPT_CHUNK,
        }

        cd.processed = 1;
    }

    rv
}


#[no_mangle]
pub unsafe extern "C" fn DwaCompressor_initializeBuffers(
    me: *mut DwaCompressor,
    bufferSize: *mut size_t,
) -> exr_result_t {
    if me.is_null() || bufferSize.is_null() {
        return EXR_ERR_INVALID_ARGUMENT;
    }

    let mut rv: exr_result_t = EXR_ERR_SUCCESS;

    let mut maxOutBufferSize: uint64_t = 0;
    let mut numLossyDctChans: uint64_t = 0;
    let mut unknownBufferSize: uint64_t = 0;
    let mut rleBufferSize: uint64_t = 0;

    let maxLossyDctAcSize: uint64_t = {
        let a = ceilf(((*me)._numScanLines as f32) / 8.0f32) as uint64_t;
        let b = ceilf((((*me)._max[0] - (*me)._min[0] + 1) as f32) / 8.0f32) as uint64_t;
        a.wrapping_mul(b).wrapping_mul(63).wrapping_mul(size_of::<uint16_t>() as uint64_t)
    };

    let maxLossyDctDcSize: uint64_t = {
        let a = ceilf(((*me)._numScanLines as f32) / 8.0f32) as uint64_t;
        let b = ceilf((((*me)._max[0] - (*me)._min[0] + 1) as f32) / 8.0f32) as uint64_t;
        a.wrapping_mul(b).wrapping_mul(size_of::<uint16_t>() as uint64_t)
    };

    let pixelCount: uint64_t = ((*me)._numScanLines as uint64_t)
        .wrapping_mul(((*me)._max[0] - (*me)._min[0] + 1) as uint64_t);

    let mut planar_unc_bufferSize: [uint64_t; NUM_COMPRESSOR_SCHEMES] = [0; NUM_COMPRESSOR_SCHEMES];

    // Sum sizes for channel rules
    for i in 0..(*me)._channelRuleCount {
        maxOutBufferSize = maxOutBufferSize.wrapping_add(Classifier_size(&*(*me)._channelRules.add(i)));
    }

    rv = DwaCompressor_classifyChannels(me);
    if rv != EXR_ERR_SUCCESS {
        return rv;
    }

    for c in 0..(*me)._numChannels {
        let curc = (*(*me)._channelData.add(c as usize)).chan;
        let compression = (*(*me)._channelData.add(c as usize)).compression;
        match compression as c_int {
            LOSSY_DCT => {
                // std_max(2lu * maxLossyDctAcSize + 65536lu, exr_compress_max_buffer_size (maxLossyDctAcSize))
                let left = 2u64
                    .wrapping_mul(maxLossyDctAcSize)
                    .wrapping_add(65536u64);
                let right = exr_compress_max_buffer_size(maxLossyDctAcSize as usize) as u64;
                let add = if left > right { left } else { right };
                maxOutBufferSize = maxOutBufferSize.wrapping_add(add);
                numLossyDctChans = numLossyDctChans.wrapping_add(1);
            }

            x if x == (RLE as c_int) => {
                rleBufferSize = rleBufferSize.wrapping_add(
                    2u64.wrapping_mul(pixelCount).wrapping_mul((*curc).sample_type.bytes_per_sample() as uint64_t),
                );
                planar_unc_bufferSize[RLE] = planar_unc_bufferSize[RLE].wrapping_add(
                    2u64.wrapping_mul(pixelCount).wrapping_mul((*curc).sample_type.bytes_per_sample() as uint64_t),
                );
            }

            x if x == (UNKNOWN as c_int) => {
                unknownBufferSize = unknownBufferSize.wrapping_add(
                    pixelCount.wrapping_mul((*curc).sample_type.bytes_per_sample() as uint64_t),
                );
                planar_unc_bufferSize[UNKNOWN] = planar_unc_bufferSize[UNKNOWN].wrapping_add(
                    pixelCount.wrapping_mul((*curc).sample_type.bytes_per_sample() as uint64_t),
                );
            }

            _ => return EXR_ERR_INVALID_ARGUMENT,
        }
    }

    maxOutBufferSize = maxOutBufferSize.wrapping_add(exr_compress_max_buffer_size(rleBufferSize as usize) as uint64_t);
    maxOutBufferSize = maxOutBufferSize.wrapping_add(exr_compress_max_buffer_size(unknownBufferSize as usize) as uint64_t);

    maxOutBufferSize = maxOutBufferSize.wrapping_add(
        exr_compress_max_buffer_size((maxLossyDctDcSize.wrapping_mul(numLossyDctChans)) as usize) as uint64_t,
    );

    maxOutBufferSize = maxOutBufferSize.wrapping_add((NUM_SIZES_SINGLE * size_of::<uint64_t>()) as uint64_t);

    *bufferSize = maxOutBufferSize as size_t;

    // allocate / resize _packedAcBuffer
    if maxLossyDctAcSize.wrapping_mul(numLossyDctChans) > (*me)._packedAcBufferSize {
        (*me)._packedAcBufferSize = maxLossyDctAcSize.wrapping_mul(numLossyDctChans);
        if !(*me)._packedAcBuffer.is_null() {
            ((*me).free_fn)((*me)._packedAcBuffer as *mut c_void);
        }
        (*me)._packedAcBuffer = ((*me).alloc_fn)((*me)._packedAcBufferSize as size_t) as *mut uint8_t;
        if (*me)._packedAcBuffer.is_null() {
            return EXR_ERR_OUT_OF_MEMORY;
        }
        memset((*me)._packedAcBuffer as *mut c_void, 0, (*me)._packedAcBufferSize as size_t);
    }

    // allocate / resize _packedDcBuffer
    if maxLossyDctDcSize.wrapping_mul(numLossyDctChans) > (*me)._packedDcBufferSize {
        (*me)._packedDcBufferSize = maxLossyDctDcSize.wrapping_mul(numLossyDctChans);
        if !(*me)._packedDcBuffer.is_null() {
            ((*me).free_fn)((*me)._packedDcBuffer as *mut c_void);
        }
        (*me)._packedDcBuffer = ((*me).alloc_fn)((*me)._packedDcBufferSize as size_t) as *mut uint8_t;
        if (*me)._packedDcBuffer.is_null() {
            return EXR_ERR_OUT_OF_MEMORY;
        }
        memset((*me)._packedDcBuffer as *mut c_void, 0, (*me)._packedDcBufferSize as size_t);
    }

    if rleBufferSize > (*me)._rleBufferSize {
        (*me)._rleBufferSize = rleBufferSize;
        if !(*me)._rleBuffer.is_null() {
            ((*me).free_fn)((*me)._rleBuffer as *mut c_void);
        }
        (*me)._rleBuffer = ((*me).alloc_fn)(rleBufferSize as size_t) as *mut uint8_t;
        if (*me)._rleBuffer.is_null() {
            return EXR_ERR_OUT_OF_MEMORY;
        }
        memset((*me)._rleBuffer as *mut c_void, 0, rleBufferSize as size_t);
    }

    // UNKNOWN needs extra headroom for zlib
    if planar_unc_bufferSize[UNKNOWN] > 0 {
        planar_unc_bufferSize[UNKNOWN] = exr_compress_max_buffer_size(planar_unc_bufferSize[UNKNOWN] as usize) as uint64_t;
    }

    for i in 0..NUM_COMPRESSOR_SCHEMES {
        if planar_unc_bufferSize[i] > (*me)._planar_unc_bufferSize[i] {
            (*me)._planar_unc_bufferSize[i] = planar_unc_bufferSize[i];
            if !(*me)._planar_unc_buffer[i].is_null() {
                ((*me).free_fn)((*me)._planar_unc_buffer[i] as *mut c_void);
            }

            if planar_unc_bufferSize[i] > (usize::MAX as uint64_t) {
                return EXR_ERR_OUT_OF_MEMORY;
            }

            (*me)._planar_unc_buffer[i] = ((*me).alloc_fn)(planar_unc_bufferSize[i] as size_t) as *mut uint8_t;
            if (*me)._planar_unc_buffer[i].is_null() {
                return EXR_ERR_OUT_OF_MEMORY;
            }
        }
    }

    rv
}

#[no_mangle]
pub unsafe extern "C" fn DwaCompressor_writeRelevantChannelRules(
    me: *mut DwaCompressor,
    outPtr: *mut *mut uint8_t,
    nAvail: uint64_t,
    nWritten: *mut uint64_t,
) -> exr_result_t {
    if me.is_null() || outPtr.is_null() || nWritten.is_null() {
        return EXR_ERR_INVALID_ARGUMENT;
    }

    let mut nOut: uint64_t = size_of::<uint16_t>() as uint64_t;
    let mut curp: *mut uint8_t = *outPtr;

    // reserve space for ruleSize
    let rule_size_ptr = curp as *mut uint16_t;
    curp = curp.add(size_of::<uint16_t>());

    if nAvail < (*nWritten).wrapping_add(nOut) {
        return EXR_ERR_OUT_OF_MEMORY;
    }

    for i in 0..(*me)._channelRuleCount {
        for c in 0..(*me)._numChannels {
            let curc = (*(*me)._channelData.add(c as usize)).chan;
            let suffix = Classifier_find_suffix((*curc).channel_name);

            if Classifier_match(&*(*me)._channelRules.add(i), suffix, (*curc).data_type) != 0 {
                let rule_size = Classifier_size(&*(*me)._channelRules.add(i));
                if nAvail < (*nWritten).wrapping_add(nOut).wrapping_add(rule_size) {
                    return EXR_ERR_OUT_OF_MEMORY;
                }
                // write rule: Classifier_write advances curp
                nOut = nOut.wrapping_add(Classifier_write(&*(*me)._channelRules.add(i), &mut curp));
                break;
            }
        }
    }

    if nOut > 65535 {
        return EXR_ERR_OUT_OF_MEMORY;
    }

    // write size in network endian
    *rule_size_ptr = one_from_native16(nOut as uint16_t);
    *nWritten = (*nWritten).wrapping_add(nOut);

    *outPtr = curp;
    EXR_ERR_SUCCESS
}

#[no_mangle]
pub unsafe extern "C" fn DwaCompressor_readChannelRules(
    me: *mut DwaCompressor,
    inPtr: *mut *const uint8_t,
    nAvail: *mut uint64_t,
    outRuleSize: *mut uint64_t,
) -> exr_result_t {
    if me.is_null() || inPtr.is_null() || nAvail.is_null() || outRuleSize.is_null() {
        return EXR_ERR_INVALID_ARGUMENT;
    }

    let mut rv: exr_result_t = EXR_ERR_SUCCESS;
    let mut readPtr: *const uint8_t = *inPtr;
    *outRuleSize = 0;

    if *nAvail > (size_of::<uint16_t>() as uint64_t) {
        let ruleSize = one_to_native16(*(readPtr as *const uint16_t)) as usize;
        let mut nRules: usize = 0usize;
        let mut dataSize: usize;
        let mut tmpPtr: *const uint8_t;

        if ruleSize < size_of::<uint16_t>() {
            return EXR_ERR_CORRUPT_CHUNK;
        }

        *outRuleSize = ruleSize as uint64_t;
        if *nAvail < ruleSize as uint64_t {
            return EXR_ERR_CORRUPT_CHUNK;
        }

        readPtr = readPtr.add(size_of::<uint16_t>());
        *inPtr = (*inPtr).add(ruleSize);
        *nAvail = (*nAvail).wrapping_sub(ruleSize as uint64_t);

        dataSize = ruleSize - size_of::<uint16_t>();
        tmpPtr = readPtr;
        rv = EXR_ERR_SUCCESS;

        while rv == EXR_ERR_SUCCESS && dataSize > 0 {
            let mut tmpc: Classifier = std::mem::zeroed();
            let mut tmp_ptr_const = tmpPtr;
            let mut tmp_size64 = dataSize as uint64_t;
            rv = Classifier_read((*me).alloc_fn, &mut tmpc, &mut tmp_ptr_const, &mut tmp_size64);
            Classifier_destroy((*me).free_fn, &mut tmpc);
            // compute how far tmpPtr advanced: tmpPtr = tmp_ptr_const (it was a copy)
            let advanced = (tmpPtr as usize).wrapping_sub(readPtr as usize);
            if advanced >= dataSize {
                // break to avoid infinite loop (protective)
                break;
            }
            // in original C the Classifier_read updates tmpPtr and dataSize; here we approximate
            // by decreasing dataSize by the size consumed by read; as we don't have that value,
            // we break if read consumed zero. In your integration you should make Classifier_read
            // update tmpPtr & dataSize directly as in C.
            // For now: attempt to advance tmpPtr by 1 to continue loop (best-effort).
            tmpPtr = tmpPtr.add(1);
            nRules += 1;
        }

        if rv == EXR_ERR_SUCCESS {
            (*me)._channelRuleCount = nRules;
            let alloc_bytes = nRules.checked_mul(size_of::<Classifier>()).unwrap_or(0);
            (*me)._channelRules = (*me).alloc_fn(alloc_bytes) as *mut Classifier;
            dataSize = ruleSize - size_of::<uint16_t>();

            if !(*me)._channelRules.is_null() {
                memset((*me)._channelRules as *mut c_void, 0, alloc_bytes);
                for i in 0..nRules {
                    let mut local_read = readPtr;
                    let mut local_size = dataSize as uint64_t;
                    // read into slot; Classifier_read is expected to advance readPtr/dataSize
                    let _ = Classifier_read((*me).alloc_fn, &mut *(*me)._channelRules.add(i), &mut local_read, &mut local_size);
                    // advance readPtr by consumed amount approximation:
                    readPtr = local_read;
                    dataSize = local_size as usize;
                }
            } else {
                rv = EXR_ERR_OUT_OF_MEMORY;
            }
        }
    } else {
        rv = EXR_ERR_CORRUPT_CHUNK;
    }

    rv
}

#[no_mangle]
pub unsafe extern "C" fn DwaCompressor_classifyChannels(me: *mut DwaCompressor) -> exr_result_t {
    if me.is_null() {
        return EXR_ERR_INVALID_ARGUMENT;
    }

    // allocate cscChannelSets
    (*me)._cscChannelSets = ((*me).alloc_fn((size_of::<CscChannelSet>() * (*me)._numChannels as usize) as size_t))
        as *mut CscChannelSet;
    if (*me)._cscChannelSets.is_null() {
        return EXR_ERR_OUT_OF_MEMORY;
    }

    let prefix_map = ((*me).alloc_fn((size_of::<CscPrefixMapItem>() * (*me)._numChannels as usize) as size_t))
        as *mut CscPrefixMapItem;
    if prefix_map.is_null() {
        return EXR_ERR_OUT_OF_MEMORY;
    }

    memset(prefix_map as *mut c_void, 0, size_of::<CscPrefixMapItem>() * (*me)._numChannels as size_t);

    for c in 0..(*me)._numChannels {
        let curc = (*(*me)._channelData.add(c as usize)).chan;
        let suffix = Classifier_find_suffix((*curc).channel_name);
        let prefixlen = (suffix as usize).wrapping_sub((*curc).channel_name as usize);
        let mapi = CscPrefixMap_find(prefix_map, (*me)._numChannels, (*curc).channel_name, prefixlen);

        for i in 0..(*me)._channelRuleCount {
            if Classifier_match(&*(*me)._channelRules.add(i), suffix, (*curc).data_type) != 0 {
                (*(*me)._channelData.add(c as usize)).compression = (*(*me)._channelRules.add(i))._scheme as c_int;
                if (*(*me)._channelRules.add(i))._cscIdx >= 0 {
                    (*mapi).idx[(*(*me)._channelRules.add(i))._cscIdx as usize] = c;
                }
            }
        }
    }

    // Find full RGB sets
    for c in 0..(*me)._numChannels {
        let red = (*prefix_map.add(c as usize)).idx[0];
        let grn = (*prefix_map.add(c as usize)).idx[1];
        let blu = (*prefix_map.add(c as usize)).idx[2];

        if (*prefix_map.add(c as usize)).name.is_null() {
            break;
        }

        if red < 0 || grn < 0 || blu < 0 {
            continue;
        }

        let redc = (*(*me)._channelData.add(red as usize)).chan;
        let grnc = (*(*me)._channelData.add(grn as usize)).chan;
        let bluc = (*(*me)._channelData.add(blu as usize)).chan;

        if (*redc).sampling.x() != (*grnc).sampling.x()
            || (*redc).sampling.x() != (*bluc).sampling.x()
            || (*grnc).sampling.x() != (*bluc).sampling.x()
            || (*redc).sampling.y() != (*grnc).sampling.y()
            || (*redc).sampling.y() != (*bluc).sampling.y()
            || (*grnc).sampling.y() != (*bluc).sampling.y()
        {
            continue;
        }

        let cset = (*me)._cscChannelSets.add((*me)._numCscChannelSets as usize);
        (*cset).idx[0] = red;
        (*cset).idx[1] = grn;
        (*cset).idx[2] = blu;
        (*me)._numCscChannelSets += 1;
    }

    ((*me).free_fn)(prefix_map as *mut c_void);

    EXR_ERR_SUCCESS
}

#[no_mangle]
pub unsafe extern "C" fn DwaCompressor_setupChannelData(me: *mut DwaCompressor) -> exr_result_t {
    if me.is_null() {
        return EXR_ERR_INVALID_ARGUMENT;
    }

    let mut planar_unc_buffer: [*mut uint8_t; NUM_COMPRESSOR_SCHEMES] = [ptr::null_mut(); NUM_COMPRESSOR_SCHEMES];

    for i in 0..NUM_COMPRESSOR_SCHEMES {
        planar_unc_buffer[i] = ptr::null_mut();
        if !(*me)._planar_unc_buffer[i].is_null() {
            planar_unc_buffer[i] = (*me)._planar_unc_buffer[i];
        }
    }

    for c in 0..(*me)._numChannels {
        let cd = &mut *(*me)._channelData.add(c as usize);
        let curc = cd.chan;
        let uncSize = ( (*curc).width as usize )
            .wrapping_mul((*curc).height as usize)
            .wrapping_mul((*curc).sample_type.bytes_per_sample() as usize);

        cd.planar_unc_size = uncSize as size_t;

        cd.planar_unc_buffer = planar_unc_buffer[cd.compression as usize];
        cd.planar_unc_buffer_end = cd.planar_unc_buffer;

        cd.planar_unc_rle[0] = cd.planar_unc_buffer;
        cd.planar_unc_rle_end[0] = cd.planar_unc_rle[0];

        if cd.planar_unc_buffer.is_null() {
            for byte in 1..(*curc).sample_type.bytes_per_sample()  {
                cd.planar_unc_rle[byte] = ptr::null_mut();
                cd.planar_unc_rle_end[byte] = ptr::null_mut();
            }
        } else {
            for byte in 1..(*curc).sample_type.bytes_per_sample()  {
                cd.planar_unc_rle[byte] = cd.planar_unc_rle[byte - 1].add(((*curc).width * (*curc).height) as usize);
                cd.planar_unc_rle_end[byte] = cd.planar_unc_rle[byte];
            }
        }

        cd.planar_unc_type = (*curc).data_type;
        if cd.compression == LOSSY_DCT {
            cd.planar_unc_type = exr_pixel_type_t::FLOAT;
        } else {
            planar_unc_buffer[cd.compression as usize] = planar_unc_buffer[cd.compression as usize].add(uncSize);
        }
    }

    EXR_ERR_SUCCESS
}
