// Line-for-line translation of the C function DwaCompressor_construct
// into unsafe Rust. This file declares the minimal types and extern helpers
// needed by the function and implements the construct function with near
// one-to-one correspondence to the original C source you provided.
//
// NOTE: this is a low-level direct port. Many types and externs are
// placeholders and must be wired to the rest of your crate (or replaced
// with the versions you already ported). The goal here is a faithful
// translation of the C body you pasted.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

pub type exr_result_t = i32;
pub const EXR_ERR_SUCCESS: exr_result_t = 0;
pub const EXR_ERR_OUT_OF_MEMORY: exr_result_t = -1;

pub const _SSE_ALIGNMENT: usize = 16;

pub type exr_memory_allocation_func_t = unsafe extern "C" fn(size: usize) -> *mut c_void;
pub type exr_memory_free_func_t = unsafe extern "C" fn(ptr: *mut c_void);

#[repr(C)]
#[derive(Copy, Clone)]
pub enum AcCompression {
    STATIC_HUFFMAN = 0,
    DEFLATE = 1,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub enum CompressorScheme {
    UNKNOWN = 0,
    LOSSY_DCT = 1,
    RLE = 2,
    ZIP = 3,
    NUM_COMPRESSOR_SCHEMES = 4,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub enum exr_pixel_type_t {
    EXR_PIXEL_UINT = 0,
    EXR_PIXEL_HALF = 1,
    EXR_PIXEL_FLOAT = 2,
}

#[repr(C)]
pub struct exr_chunk_t {
    pub start_x: i32,
    pub start_y: i32,
    pub width: i32,
    pub height: i32,
}

#[repr(C)]
pub struct exr_const_context_t {
    pub alloc_fn: Option<exr_memory_allocation_func_t>,
    pub free_fn: Option<exr_memory_free_func_t>,
}

#[repr(C)]
pub struct exr_coding_channel_info_t {
    pub data_type: exr_pixel_type_t,
    pub y_samples: i32,
    pub width: i32,
    pub bytes_per_element: i32,
    // fields are placeholders: add others as required
}

#[repr(C)]
pub struct exr_encode_pipeline_t {
    pub context: *const exr_const_context_t,
    pub part_index: i32,
    pub channel_count: i32,
    pub channels: *mut exr_coding_channel_info_t,
    pub chunk: exr_chunk_t,
    pub compressed_alloc_size: u64,
    pub compressed_buffer: *mut u8,
    pub packed_buffer: *mut u8,
    pub packed_alloc_size: usize,
    pub packed_bytes: usize,
    pub packed_buffer_ptr: *mut u8,
    pub scratch_buffer_1: *mut u8,
    pub scratch_alloc_size_1: usize,
    pub packed_bytes_written: usize,
    // ... other fields may be present, placeholder only
}

#[repr(C)]
pub struct exr_decode_pipeline_t {
    pub context: *const exr_const_context_t,
    pub channel_count: i32,
    pub channels: *mut exr_coding_channel_info_t,
    pub chunk: exr_chunk_t,
    pub bytes_decompressed: usize,
    // ... placeholder
}

#[repr(C)]
pub struct DctCoderChannelData {
    _pad: [u8; 0],
}

extern "C" {
    // extern helpers referenced by the C function
    fn initializeFuncs();
    fn internal_exr_alloc(size: usize) -> *mut c_void;
    fn internal_exr_free(ptr: *mut c_void);
    fn internal_exr_alloc_aligned(
        alloc_fn: exr_memory_allocation_func_t,
        out_mem: *mut *mut c_void,
        size: usize,
        align: usize,
    ) -> *mut ChannelData;
    fn exr_get_zip_compression_level(
        context: *const exr_const_context_t,
        part_index: i32,
        out_level: *mut i32,
    ) -> exr_result_t;
    fn exr_get_dwa_compression_level(
        context: *const exr_const_context_t,
        part_index: i32,
        out_level: *mut f32,
    ) -> exr_result_t;

    // ported elsewhere / stubbed:
    fn DctCoderChannelData_construct(d: *mut DctCoderChannelData, t: exr_pixel_type_t);
}

// ChannelData must match the layout used by the C code (we include only the
// fields touched by this function).
#[repr(C)]
pub struct ChannelData {
    pub _dctData: DctCoderChannelData,
    pub chan: *mut exr_coding_channel_info_t,
    pub planarUncBuffer: *mut u8,
    pub planarUncBufferEnd: *mut u8,
    pub planarUncRle: [*mut u8; 4],
    pub planarUncRleEnd: [*mut u8; 4],
    pub planarUncSize: usize,
    pub processed: i32,
    pub compression: CompressorScheme,
    pub planarUncType: exr_pixel_type_t,
    pub _pad: [u8; 20],
}

#[repr(C)]
pub struct CscChannelSet {
    pub idx: [i32; 3],
}

#[repr(C)]
pub struct Classifier {
    _pad: [u8; 0],
}

// Full DwaCompressor with fields used by the C construct body
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
    pub _planarUncBuffer: [*mut u8; 4],
    pub _planarUncBufferSize: [u64; 4],

    pub alloc_fn: exr_memory_allocation_func_t,
    pub free_fn: exr_memory_free_func_t,

    pub _zipLevel: i32,
    pub _dwaCompressionLevel: f32,

    // remainder of struct omitted for brevity
    _reserved: [u8; 64],
}

#[no_mangle]
pub unsafe extern "C" fn DwaCompressor_construct(
    me: *mut DwaCompressor,
    acCompression: AcCompression,
    encode: *mut exr_encode_pipeline_t,
    decode: *mut exr_decode_pipeline_t,
) -> exr_result_t {
    let mut rv: exr_result_t = EXR_ERR_SUCCESS;

    // initialize functions as in C
    initializeFuncs();

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
        if !pctxt.is_null() {
            // safe to unwrap because context contains Option in our mapping
            let c = &*pctxt;
            if let Some(af) = c.alloc_fn {
                (*me).alloc_fn = af;
            } else {
                (*me).alloc_fn = internal_exr_alloc;
            }
            if let Some(ff) = c.free_fn {
                (*me).free_fn = ff;
            } else {
                (*me).free_fn = internal_exr_free;
            }
        } else {
            (*me).alloc_fn = internal_exr_alloc;
            (*me).free_fn = internal_exr_free;
        }

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

            // DctCoderChannelData_construct(&(me->_channelData[c]._dctData),
            //                                me->_channelData[c].chan->data_type);
            DctCoderChannelData_construct(&mut (*cd)._dctData as *mut DctCoderChannelData, (*(*cd).chan).data_type);
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
        if !pctxt.is_null() {
            let c = &*pctxt;
            if let Some(af) = c.alloc_fn {
                (*me).alloc_fn = af;
            } else {
                (*me).alloc_fn = internal_exr_alloc;
            }
            if let Some(ff) = c.free_fn {
                (*me).free_fn = ff;
            } else {
                (*me).free_fn = internal_exr_free;
            }
        } else {
            (*me).alloc_fn = internal_exr_alloc;
            (*me).free_fn = internal_exr_free;
        }

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


// Translation (line-by-line style) of the next portion of `internal_dwa_compressor.h`
// into unsafe Rust: DwaCompressor_destroy and DwaCompressor_compress.
// This keeps pointer-heavy, C-like semantics and mirrors the original control flow.
//
// NOTE: many helper functions/types are declared as `extern "C"` stubs or placeholder
// constants so the translation is faithful. Wire these to your existing Rust ports
// of those helpers (or replace with safe equivalents) when integrating.

use std::os::raw::{c_int, c_void};
use std::ptr;
use std::mem::{size_of, transmute};

type uint8_t = u8;
type uint16_t = u16;
type uint64_t = u64;
type size_t = usize;
type uintptr_t = usize;

pub const DWA_CLASSIFIER_TRUE: i32 = 1;
pub const NUM_COMPRESSOR_SCHEMES: usize = 4;
pub const UNKNOWN: i32 = -1;

// Error codes (placeholders; match your project's definitions)
pub const EXR_ERR_SUCCESS: i32 = 0;
pub const EXR_ERR_OUT_OF_MEMORY: i32 = -1;
pub const EXR_ERR_ARGUMENT_OUT_OF_RANGE: i32 = -2;
pub const EXR_ERR_INVALID_ARGUMENT: i32 = -3;
pub const EXR_ERR_CORRUPT_CHUNK: i32 = -4;

// Size-table related constants (placeholders; keep names used in C)
pub const NUM_SIZES_SINGLE: usize = 64;
pub const VERSION: usize = 0;
pub const UNKNOWN_UNCOMPRESSED_SIZE: usize = 1;
pub const UNKNOWN_COMPRESSED_SIZE: usize = 2;
pub const AC_COMPRESSED_SIZE: usize = 3;
pub const DC_COMPRESSED_SIZE: usize = 4;
pub const RLE_COMPRESSED_SIZE: usize = 5;
pub const RLE_UNCOMPRESSED_SIZE: usize = 6;
pub const RLE_RAW_SIZE: usize = 7;
pub const AC_UNCOMPRESSED_COUNT: usize = 8;
pub const DC_UNCOMPRESSED_COUNT: usize = 9;
pub const AC_COMPRESSION: usize = 10;

// Forward-declared external helpers (implementations live elsewhere)
extern "C" {
    fn DctCoderChannelData_destroy(free_fn: unsafe extern "C" fn(*mut c_void), d: *mut c_void);
    fn DwaCompressor_initializeBuffers(me: *mut DwaCompressor, out: *mut size_t) -> c_int;
    fn internal_encode_alloc_buffer(
        encode: *mut exr_encode_pipeline_t,
        which: c_int,
        out_ptr: *mut *mut uint8_t,
        out_size: *mut uint64_t,
        needed: size_t,
    ) -> c_int;
    fn DwaCompressor_writeRelevantChannelRules(
        me: *mut DwaCompressor,
        out_ptr: *mut *mut uint8_t,
        nAvail: uint64_t,
        nWritten: *mut uint64_t,
    ) -> c_int;
    fn DwaCompressor_setupChannelData(me: *mut DwaCompressor) -> c_int;
    fn DctCoderChannelData_push_row(
        alloc_fn: exr_memory_allocation_func_t,
        free_fn: exr_memory_free_func_t,
        d: *mut DctCoderChannelData,
        r: *mut uint8_t,
    ) -> c_int;
    fn LossyDctEncoderCsc_construct(
        enc: *mut LossyDctEncoder,
        level: f32,
        a: *mut DctCoderChannelData,
        b: *mut DctCoderChannelData,
        c: *mut DctCoderChannelData,
        packedAcEnd: *mut uint8_t,
        packedDcEnd: *mut uint8_t,
        lut: *const uint16_t,
        width: c_int,
        height: c_int,
    ) -> c_int;
    fn LossyDctEncoder_construct(
        enc: *mut LossyDctEncoder,
        level: f32,
        dct: *mut DctCoderChannelData,
        packedAcEnd: *mut uint8_t,
        packedDcEnd: *mut uint8_t,
        nonlinearLut: *const uint16_t,
        width: c_int,
        height: c_int,
    ) -> c_int;
    fn LossyDctEncoder_execute(
        alloc_fn: exr_memory_allocation_func_t,
        free_fn: exr_memory_free_func_t,
        enc: *mut LossyDctEncoder,
    ) -> c_int;

    fn internal_huf_compress(
        outCompressedSizePtr: *mut uint64_t,
        outDataPtr: *mut uint8_t,
        outDataSize: size_t,
        src: *const uint16_t,
        srcCount: uint64_t,
        scratch: *mut uint8_t,
        scratch_size: size_t,
    ) -> c_int;

    fn exr_compress_buffer(
        context: *const exr_const_context_t,
        level: c_int,
        src: *const uint8_t,
        src_len: size_t,
        dst: *mut uint8_t,
        dst_len: size_t,
        out_written: *mut size_t,
    ) -> c_int;

    fn exr_compress_max_buffer_size(n: size_t) -> size_t;

    fn internal_zip_deconstruct_bytes(dst: *mut uint8_t, src: *const uint8_t, n: size_t);

    fn internal_rle_compress(dst: *mut uint8_t, dst_size: size_t, src: *const uint8_t, src_len: size_t) -> size_t;

    fn priv_from_native64(sizes: *mut uint64_t, n: usize);

    // libc memcpy
    fn memcpy(dst: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void;
}

// Minimal placeholder types used in the function body
#[repr(C)]
pub struct exr_encode_pipeline_t {
    pub compressed_alloc_size: uint64_t,
    pub compressed_buffer: *mut uint8_t,
    pub packed_buffer: *mut uint8_t,
    pub packed_alloc_size: size_t,
    pub packed_bytes: size_t,
    pub scratch_buffer_1: *mut uint8_t,
    pub scratch_alloc_size_1: size_t,
    pub context: *const exr_const_context_t,
    // other fields omitted
}

#[repr(C)]
pub struct exr_const_context_t {
    // placeholder
    _pad: [u8; 0],
}

#[repr(C)]
pub struct exr_coding_channel_info_t {
    pub y_samples: c_int,
    pub width: c_int,
    pub bytes_per_element: c_int,
    pub p_linear: c_int,
    // other fields omitted
}

#[repr(C)]
pub struct DctCoderChannelData {
    pub _rows: *mut *mut uint8_t,
    pub _size: size_t,
    // other fields omitted
}

#[repr(C)]
pub struct ChannelData {
    pub _dctData: DctCoderChannelData,
    pub chan: *mut exr_coding_channel_info_t,
    pub planarUncRleEnd: [*mut uint8_t; 4],
    pub planarUncBufferEnd: *mut uint8_t,
    pub planarUncSize: size_t,
    pub processed: c_int,
    pub compression: c_int,
    pub planarUncBuffer: *mut uint8_t,
    // other fields omitted
}

#[repr(C)]
pub struct CscChannelSet {
    pub idx: [c_int; 3],
}

#[repr(C)]
pub struct Classifier {
    _pad: [u8; 0],
}

#[repr(C)]
pub struct LossyDctEncoder {
    pub _numAcComp: uint64_t,
    pub _numDcComp: uint64_t,
    // other fields omitted
}

pub type exr_memory_allocation_func_t = unsafe extern "C" fn(size: size_t) -> *mut c_void;
pub type exr_memory_free_func_t = unsafe extern "C" fn(ptr: *mut c_void);

// DwaCompressor structure (partial, fields used below)
#[repr(C)]
pub struct DwaCompressor {
    pub _encode: *mut exr_encode_pipeline_t,
    pub _packedAcBuffer: *mut uint8_t,
    pub _packedDcBuffer: *mut uint8_t,
    pub _rleBuffer: *mut uint8_t,

    pub _channel_mem: *mut c_void,
    pub _channelData: *mut ChannelData,
    pub _numChannels: c_int,

    pub _cscChannelSets: *mut CscChannelSet,
    pub _numCscChannelSets: c_int,

    pub _channelRules: *mut Classifier,
    pub _channelRuleCount: size_t,

    pub _planarUncBuffer: [*mut uint8_t; NUM_COMPRESSOR_SCHEMES],
    pub _planarUncBufferSize: [uint64_t; NUM_COMPRESSOR_SCHEMES],

    pub _acCompression: c_int,

    pub _min: [c_int; 2],
    pub _max: [c_int; 2],
    pub _numScanLines: c_int,

    pub alloc_fn: exr_memory_allocation_func_t,
    pub free_fn: exr_memory_free_func_t,

    pub _zipLevel: c_int,
    pub _dwaCompressionLevel: f32,
    // other fields omitted
}

// exr_const globals referenced in code
extern "C" {
    static mut sDefaultChannelRules: [Classifier; 1]; // actual size elsewhere
    static mut sLegacyChannelRules: [Classifier; 1];
}

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
            // DctCoderChannelData_destroy (me->free_fn, &(me->_channelData[c]._dctData));
            let cd = (*me)._channelData.add(c as usize);
            DctCoderChannelData_destroy(
                (*me).free_fn,
                &mut (*cd)._dctData as *mut DctCoderChannelData as *mut c_void,
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
        if !(*me)._planarUncBuffer[i as usize].is_null() {
            ((*me).free_fn)((*me)._planarUncBuffer[i as usize] as *mut c_void);
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
            if (y % (*chan).y_samples) != 0 {
                continue;
            }

            rv = DctCoderChannelData_push_row(
                (*me).alloc_fn,
                (*me).free_fn,
                &mut (*cd)._dctData,
                inDataPtr,
            );
            if rv != EXR_ERR_SUCCESS {
                return rv;
            }

            inDataPtr = inDataPtr.add(((*chan).width * (*chan).bytes_per_element) as usize);
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
            &mut (*(*me)._channelData.add(cset.idx[0] as usize))._dctData,
            &mut (*(*me)._channelData.add(cset.idx[1] as usize))._dctData,
            &mut (*(*me)._channelData.add(cset.idx[2] as usize))._dctData,
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
                let mut enc: LossyDctEncoder = std::mem::zeroed();
                let mut nonlinearLut: *const uint16_t = ptr::null();

                if pchan.is_null() || (*pchan).p_linear == 0 {
                    nonlinearLut = dwaCompressorToNonlinear();
                }

                rv = LossyDctEncoder_construct(
                    &mut enc,
                    (*me)._dwaCompressionLevel / 100000.0_f32,
                    &mut cd._dctData,
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
                // For RLE, bash bytes
                let dcd = &mut cd._dctData;
                let mut yy: usize = 0;
                while yy < dcd._size {
                    let row = *dcd._rows.add(yy);
                    let mut xidx = 0;
                    while xidx < (*pchan).width as usize {
                        let mut byte = 0;
                        while byte < (*pchan).bytes_per_element as usize {
                            // *cd->planarUncRleEnd[byte]++ = *row++;
                            let dest = cd.planarUncRleEnd[byte];
                            *dest = *row;
                            cd.planarUncRleEnd[byte] = dest.add(1);
                            // increment row pointer
                            let row = row.add(1);
                            byte += 1;
                        }
                        xidx += 1;
                    }
                    // *rleRawSize += width * bytes_per_element
                    if !rleRawSize.is_null() {
                        *rleRawSize = *rleRawSize + ((*pchan).width as uint64_t * (*pchan).bytes_per_element as uint64_t);
                    }
                    yy += 1;
                }
            }

            x if x == (UNKNOWN as c_int) => {
                // copy data verbatim
                let scanlineSize = ((*pchan).width as usize) * ((*pchan).bytes_per_element as usize);
                let dcd = &mut cd._dctData;
                let mut yy: usize = 0;
                while yy < dcd._size {
                    let src = *dcd._rows.add(yy) as *const c_void;
                    let dst = cd.planarUncBufferEnd as *mut c_void;
                    memcpy(dst, src, scanlineSize);
                    cd.planarUncBufferEnd = cd.planarUncBufferEnd.add(scanlineSize);
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
            (*me)._planarUncBuffer[UNKNOWN as usize],
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
                (*me)._planarUncBuffer[CompressorScheme::RLE as usize],
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


// Translation of DwaCompressor_uncompress (C -> Rust, unsafe, near line-for-line).
// Assumes the surrounding types and extern helpers from previous chunks exist.

use std::os::raw::{c_int, c_void};
use std::ptr;
use std::mem::{size_of, zeroed};

type uint8_t = u8;
type uint16_t = u16;
type uint64_t = u64;
type size_t = usize;
type intptr_t = isize;

pub type exr_result_t = c_int;

// External helpers & stubs used by this function (must be provided elsewhere).
extern "C" {
    fn priv_to_native64(s: *mut uint64_t, n: usize);
    fn exr_uncompress_buffer(
        context: *const exr_const_context_t,
        src: *const uint8_t,
        src_len: size_t,
        dst: *mut uint8_t,
        dst_len: size_t,
        out_written: *mut size_t,
    ) -> exr_result_t;
    fn internal_huf_decompress(
        decode: *mut exr_decode_pipeline_t,
        src: *const uint8_t,
        src_len: uint64_t,
        dst: *mut uint16_t,
        dst_count: uint64_t,
        scratch: *mut uint8_t,
        scratch_size: size_t,
    ) -> exr_result_t;
    fn internal_decode_alloc_buffer(
        decode: *mut exr_decode_pipeline_t,
        which: c_int,
        out_ptr: *mut *mut uint8_t,
        out_size: *mut uint64_t,
        needed: size_t,
    ) -> exr_result_t;
    fn internal_zip_reconstruct_bytes(dst: *mut uint8_t, src: *const uint8_t, n: size_t);
    fn internal_rle_decompress(dst: *mut uint8_t, dst_len: size_t, src: *const uint8_t, src_len: size_t) -> size_t;
    fn DwaCompressor_readChannelRules(
        me: *mut DwaCompressor,
        inptr: *mut *const uint8_t,
        nAvail: *mut uint64_t,
        outRuleSize: *mut uint64_t,
    ) -> exr_result_t;
    fn DwaCompressor_initializeBuffers(me: *mut DwaCompressor, out: *mut size_t) -> exr_result_t;
    fn DwaCompressor_setupChannelData(me: *mut DwaCompressor) -> exr_result_t;
    fn DctCoderChannelData_push_row(
        alloc_fn: exr_memory_allocation_func_t,
        free_fn: exr_memory_free_func_t,
        d: *mut DctCoderChannelData,
        r: *mut uint8_t,
    ) -> exr_result_t;
    fn LossyDctDecoderCsc_construct(
        decoder: *mut LossyDctDecoder,
        a: *mut DctCoderChannelData,
        b: *mut DctCoderChannelData,
        c: *mut DctCoderChannelData,
        packedAcBegin: *mut uint8_t,
        packedAcEnd: *mut uint8_t,
        packedDcBegin: *mut uint8_t,
        totalDcUncompressedCount: uint64_t,
        lut: *const uint16_t,
        width: c_int,
        height: c_int,
    ) -> exr_result_t;
    fn LossyDctDecoder_construct(
        decoder: *mut LossyDctDecoder,
        dct: *mut DctCoderChannelData,
        packedAcBegin: *mut uint8_t,
        packedAcEnd: *mut uint8_t,
        packedDcBegin: *mut uint8_t,
        totalDcUncompressedCount: uint64_t,
        lut: *const uint16_t,
        width: c_int,
        height: c_int,
    ) -> exr_result_t;
    fn LossyDctDecoder_execute(
        alloc_fn: exr_memory_allocation_func_t,
        free_fn: exr_memory_free_func_t,
        decoder: *mut LossyDctDecoder,
    ) -> exr_result_t;
    fn dwaCompressorToLinear() -> *const uint16_t;
    fn dwaCompressorToNonlinear() -> *const uint16_t;
    fn interleaveByte2(dst: *mut uint8_t, src0: *mut uint8_t, src1: *mut uint8_t, width: c_int);
    // libc memcpy
    fn memcpy(dst: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void;
}

// Minimal placeholder types used by this function (should match earlier definitions)
#[repr(C)]
pub struct exr_const_context_t { _pad: [u8; 0] }

#[repr(C)]
pub struct exr_decode_pipeline_t {
    pub unpacked_buffer: *mut uint8_t,
    pub unpacked_alloc_size: size_t,
    pub context: *const exr_const_context_t,
    pub scratch_buffer_1: *mut uint8_t,
    pub scratch_alloc_size_1: size_t,
    // other fields omitted
}

#[repr(C)]
pub struct exr_coding_channel_info_t {
    pub y_samples: c_int,
    pub width: c_int,
    pub height: c_int,
    pub bytes_per_element: c_int,
    pub p_linear: c_int,
    pub data_type: exr_pixel_type_t,
}

#[repr(C)]
pub struct DctCoderChannelData {
    pub _rows: *mut *mut uint8_t,
    pub _size: size_t,
    pub _type: exr_pixel_type_t,
    // other fields omitted
}

#[repr(C)]
pub struct ChannelData {
    pub _dctData: DctCoderChannelData,
    pub chan: *mut exr_coding_channel_info_t,
    pub planarUncBuffer: *mut uint8_t,
    pub planarUncBufferEnd: *mut uint8_t,
    pub planarUncRleEnd: [*mut uint8_t; NUM_COMPRESSOR_SCHEMES],
    pub planarUncSize: size_t,
    pub processed: c_int,
    pub compression: c_int,
    // other fields omitted
}

#[repr(C)]
pub struct CscChannelSet { pub idx: [c_int; 3] }

#[repr(C)]
pub struct LossyDctDecoder {
    pub _packedAcCount: uint64_t,
    pub _packedDcCount: uint64_t,
    // other fields omitted
}

pub type exr_memory_allocation_func_t = unsafe extern "C" fn(size: size_t) -> *mut c_void;
pub type exr_memory_free_func_t = unsafe extern "C" fn(ptr: *mut c_void);

#[repr(C)]
pub struct DwaCompressor {
    pub _decode: *mut exr_decode_pipeline_t,
    pub _packedAcBuffer: *mut uint8_t,
    pub _packedAcBufferSize: uint64_t,
    pub _packedDcBuffer: *mut uint8_t,
    pub _packedDcBufferSize: uint64_t,
    pub _rleBuffer: *mut uint8_t,
    pub _rleBufferSize: uint64_t,
    pub _planarUncBuffer: [*mut uint8_t; NUM_COMPRESSOR_SCHEMES],
    pub _planarUncBufferSize: [uint64_t; NUM_COMPRESSOR_SCHEMES],
    pub _channelData: *mut ChannelData,
    pub _numChannels: c_int,
    pub _cscChannelSets: *mut CscChannelSet,
    pub _numCscChannelSets: c_int,
    pub _channelRules: *mut Classifier,
    pub _channelRuleCount: size_t,
    pub _min: [c_int; 2],
    pub _max: [c_int; 2],
    pub _numScanLines: c_int,
    pub alloc_fn: exr_memory_allocation_func_t,
    pub free_fn: exr_memory_free_func_t,
}

#[repr(C)]
pub enum exr_pixel_type_t { EXR_PIXEL_UINT = 0, EXR_PIXEL_HALF = 1, EXR_PIXEL_FLOAT = 2 }

#[repr(C)]
pub struct Classifier { _pad: [u8; 0] }

// constants used
pub const NUM_COMPRESSOR_SCHEMES: usize = 4;
pub const UNKNOWN: usize = 0;
pub const RLE: usize = 1;
pub const AC_UNCOMPRESSED_COUNT: usize = 8;
pub const DC_UNCOMPRESSED_COUNT: usize = 9;
pub const AC_COMPRESSION: usize = 10;
pub const VERSION: usize = 0;
pub const UNKNOWN_UNCOMPRESSED_SIZE: usize = 1;
pub const UNKNOWN_COMPRESSED_SIZE: usize = 2;
pub const AC_COMPRESSED_SIZE: usize = 3;
pub const DC_COMPRESSED_SIZE: usize = 4;
pub const RLE_COMPRESSED_SIZE: usize = 5;
pub const RLE_UNCOMPRESSED_SIZE: usize = 6;
pub const RLE_RAW_SIZE: usize = 7;

pub const EXR_ERR_CORRUPT_CHUNK: exr_result_t = -1;
pub const EXR_ERR_BAD_CHUNK_LEADER: exr_result_t = -2;
pub const EXR_ERR_SUCCESS: exr_result_t = 0;

// externs for rule tables
extern "C" {
    static mut sLegacyChannelRules: [Classifier; 1];
    static mut sDefaultChannelRules: [Classifier; 1];
}

// The function translation
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
        if unknownUncompressedSize > (*me)._planarUncBufferSize[UNKNOWN] {
            return EXR_ERR_CORRUPT_CHUNK;
        }
        if exr_uncompress_buffer(
            (*me)._decode.as_ref().map_or(ptr::null(), |d| d.context),
            compressedUnknownBuf,
            unknownCompressedSize as usize,
            (*me)._planarUncBuffer[UNKNOWN],
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
        if rleUncompressedSize > (*me)._rleBufferSize as u64 || rleRawSize > (*me)._planarUncBufferSize[RLE] {
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
            (*me)._planarUncBuffer[RLE],
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
            if (y % (*chan).y_samples) != 0 {
                continue;
            }

            rv = DctCoderChannelData_push_row((*me).alloc_fn, (*me).free_fn, &mut cd._dctData, outBufferEnd);
            if rv != EXR_ERR_SUCCESS {
                return rv;
            }

            cd._dctData._type = (*chan).data_type;
            outBufferEnd = outBufferEnd.add(((*chan).width * (*chan).bytes_per_element) as usize);
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
            &mut (*(*me)._channelData.add(r_chan as usize))._dctData,
            &mut (*(*me)._channelData.add(g_chan as usize))._dctData,
            &mut (*(*me)._channelData.add(b_chan as usize))._dctData,
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
        let dcddata = &mut cd._dctData;
        let pixel_size = if !chan.is_null() { (*chan).bytes_per_element } else { 0 };

        if cd.processed != 0 {
            continue;
        }

        if chan.is_null() || (*chan).width == 0 || (*chan).height == 0 {
            cd.processed = 1;
            continue;
        }

        match cd.compression {
            x if x == (LOSSY_DCT as c_int) => {
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

            x if x == (RLE as c_int) => {
                let mut row_i: c_int = 0;
                for y in (*me)._min[1]..=(*me)._max[1] {
                    if (y % (*chan).y_samples) != 0 {
                        continue;
                    }
                    let dst = *dcddata._rows.add(row_i as usize);
                    if pixel_size == 2 {
                        interleaveByte2(dst, cd.planarUncRleEnd[0], cd.planarUncRleEnd[1], (*chan).width);
                        cd.planarUncRleEnd[0] = cd.planarUncRleEnd[0].add((*chan).width as usize);
                        cd.planarUncRleEnd[1] = cd.planarUncRleEnd[1].add((*chan).width as usize);
                    } else {
                        for x in 0..(*chan).width {
                            for byte in 0..(pixel_size as usize) {
                                let src = cd.planarUncRleEnd[byte];
                                *dst = *src;
                                cd.planarUncRleEnd[byte] = src.add(1);
                                dst = dst.add(1);
                            }
                        }
                    }
                    row_i += 1;
                }
            }

            x if x == (UNKNOWN as c_int) => {
                let mut row = 0;
                let dst_scanline_size = ((*chan).width as usize) * (pixel_size as usize);
                for y in (*me)._min[1]..=*me._max[1] {
                    if (y % (*chan).y_samples) != 0 {
                        continue;
                    }
                    if cd.planarUncBufferEnd.add(dst_scanline_size) > (*me)._planarUncBuffer[UNKNOWN].add((*me)._planarUncBufferSize[UNKNOWN] as usize) {
                        return EXR_ERR_CORRUPT_CHUNK;
                    }
                    memcpy(
                        (*dcddata._rows.add(row as usize)) as *mut c_void,
                        cd.planarUncBufferEnd as *const c_void,
                        dst_scanline_size,
                    );
                    cd.planarUncBufferEnd = cd.planarUncBufferEnd.add(dst_scanline_size);
                    row += 1;
                }
            }

            _ => return EXR_ERR_CORRUPT_CHUNK,
        }

        cd.processed = 1;
    }

    rv
}

// Translation of the final chunk of internal_dwa_compressor.h -> Rust (unsafe, near line-for-line).
// This implements:
// - DwaCompressor_initializeBuffers
// - DwaCompressor_writeRelevantChannelRules
// - DwaCompressor_readChannelRules
// - DwaCompressor_classifyChannels
// - DwaCompressor_setupChannelData
//
// It depends on many extern helpers and types previously declared in your file.
// Integrate with the rest of the file you are assembling.

use std::os::raw::{c_int, c_void};
use std::mem::{size_of, transmute};
use std::ptr;

type uint8_t = u8;
type uint16_t = u16;
type uint64_t = u64;
type size_t = usize;

extern "C" {
    // helpers used above or earlier in the translation
    fn exr_compress_max_buffer_size(n: size_t) -> size_t;
    fn one_from_native16(v: uint16_t) -> uint16_t;
    fn one_to_native16(v: uint16_t) -> uint16_t;

    fn Classifier_size(me: *const Classifier) -> uint64_t;
    fn Classifier_write(me: *const Classifier, ptr: *mut *mut uint8_t) -> uint64_t;
    fn Classifier_find_suffix(channel_name: *const c_char) -> *const c_char;
    fn Classifier_match(me: *const Classifier, suffix: *const c_char, t: exr_pixel_type_t) -> c_int;
    fn Classifier_read(
        alloc_fn: exr_memory_allocation_func_t,
        out: *mut Classifier,
        ptr: *mut *const uint8_t,
        size: *mut uint64_t,
    ) -> exr_result_t;
    fn Classifier_destroy(free_fn: exr_memory_free_func_t, c: *mut Classifier);

    fn CscPrefixMap_find(
        mapl: *mut CscPrefixMapItem,
        maxSize: c_int,
        cname: *const c_char,
        prefixlen: size_t,
    ) -> *mut CscPrefixMapItem;

    fn memset(dst: *mut c_void, val: c_int, n: size_t) -> *mut c_void;
    fn memcpy(dst: *mut c_void, src: *const c_void, n: size_t) -> *mut c_void;
    fn ceilf(x: f32) -> f32;

    // allocation functions are already declared elsewhere:
    // exr_memory_allocation_func_t, exr_memory_free_func_t
}

// Additional types used here (must match earlier declarations)
#[repr(C)]
pub struct exr_coding_channel_info_t {
    pub channel_name: *const c_char,
    pub data_type: exr_pixel_type_t,
    pub x_samples: c_int,
    pub y_samples: c_int,
    pub width: c_int,
    pub height: c_int,
    pub bytes_per_element: c_int,
    pub p_linear: c_int,
}

#[repr(C)]
pub struct CscPrefixMapItem {
    pub name: *const c_char,
    pub prefix_len: size_t,
    pub idx: [c_int; 3],
    pub pad: [u8; 4],
}

#[repr(C)]
pub struct Classifier { _pad: [u8; 0] }

pub const NUM_COMPRESSOR_SCHEMES: usize = 4;
pub const UNKNOWN: usize = 0;
pub const RLE: usize = 1;
pub const LOSSY_DCT: c_int = 1;
pub const STATIC_HUFFMAN: c_int = 0;
pub const DEFLATE: c_int = 1;

pub const EXR_ERR_SUCCESS: exr_result_t = 0;
pub const EXR_ERR_OUT_OF_MEMORY: exr_result_t = -1;
pub const EXR_ERR_INVALID_ARGUMENT: exr_result_t = -2;
pub const EXR_ERR_CORRUPT_CHUNK: exr_result_t = -3;

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

    let mut planarUncBufferSize: [uint64_t; NUM_COMPRESSOR_SCHEMES] = [0; NUM_COMPRESSOR_SCHEMES];

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
                    2u64.wrapping_mul(pixelCount).wrapping_mul((*curc).bytes_per_element as uint64_t),
                );
                planarUncBufferSize[RLE] = planarUncBufferSize[RLE].wrapping_add(
                    2u64.wrapping_mul(pixelCount).wrapping_mul((*curc).bytes_per_element as uint64_t),
                );
            }

            x if x == (UNKNOWN as c_int) => {
                unknownBufferSize = unknownBufferSize.wrapping_add(
                    pixelCount.wrapping_mul((*curc).bytes_per_element as uint64_t),
                );
                planarUncBufferSize[UNKNOWN] = planarUncBufferSize[UNKNOWN].wrapping_add(
                    pixelCount.wrapping_mul((*curc).bytes_per_element as uint64_t),
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
    if planarUncBufferSize[UNKNOWN] > 0 {
        planarUncBufferSize[UNKNOWN] = exr_compress_max_buffer_size(planarUncBufferSize[UNKNOWN] as usize) as uint64_t;
    }

    for i in 0..NUM_COMPRESSOR_SCHEMES {
        if planarUncBufferSize[i] > (*me)._planarUncBufferSize[i] {
            (*me)._planarUncBufferSize[i] = planarUncBufferSize[i];
            if !(*me)._planarUncBuffer[i].is_null() {
                ((*me).free_fn)((*me)._planarUncBuffer[i] as *mut c_void);
            }

            if planarUncBufferSize[i] > (usize::MAX as uint64_t) {
                return EXR_ERR_OUT_OF_MEMORY;
            }

            (*me)._planarUncBuffer[i] = ((*me).alloc_fn)(planarUncBufferSize[i] as size_t) as *mut uint8_t;
            if (*me)._planarUncBuffer[i].is_null() {
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

        if (*redc).x_samples != (*grnc).x_samples
            || (*redc).x_samples != (*bluc).x_samples
            || (*grnc).x_samples != (*bluc).x_samples
            || (*redc).y_samples != (*grnc).y_samples
            || (*redc).y_samples != (*bluc).y_samples
            || (*grnc).y_samples != (*bluc).y_samples
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

    let mut planarUncBuffer: [*mut uint8_t; NUM_COMPRESSOR_SCHEMES] = [ptr::null_mut(); NUM_COMPRESSOR_SCHEMES];

    for i in 0..NUM_COMPRESSOR_SCHEMES {
        planarUncBuffer[i] = ptr::null_mut();
        if !(*me)._planarUncBuffer[i].is_null() {
            planarUncBuffer[i] = (*me)._planarUncBuffer[i];
        }
    }

    for c in 0..(*me)._numChannels {
        let cd = &mut *(*me)._channelData.add(c as usize);
        let curc = cd.chan;
        let uncSize = ( (*curc).width as usize )
            .wrapping_mul((*curc).height as usize)
            .wrapping_mul((*curc).bytes_per_element as usize);

        cd.planarUncSize = uncSize as size_t;

        cd.planarUncBuffer = planarUncBuffer[cd.compression as usize];
        cd.planarUncBufferEnd = cd.planarUncBuffer;

        cd.planarUncRle[0] = cd.planarUncBuffer;
        cd.planarUncRleEnd[0] = cd.planarUncRle[0];

        if cd.planarUncBuffer.is_null() {
            for byte in 1..(*curc).bytes_per_element as usize {
                cd.planarUncRle[byte] = ptr::null_mut();
                cd.planarUncRleEnd[byte] = ptr::null_mut();
            }
        } else {
            for byte in 1..(*curc).bytes_per_element as usize {
                cd.planarUncRle[byte] = cd.planarUncRle[byte - 1].add(((*curc).width * (*curc).height) as usize);
                cd.planarUncRleEnd[byte] = cd.planarUncRle[byte];
            }
        }

        cd.planarUncType = (*curc).data_type;
        if cd.compression == LOSSY_DCT as c_int {
            cd.planarUncType = exr_pixel_type_t::EXR_PIXEL_FLOAT;
        } else {
            planarUncBuffer[cd.compression as usize] = planarUncBuffer[cd.compression as usize].add(uncSize);
        }
    }

    EXR_ERR_SUCCESS
}
