# exrs (exr-rs)

This library is a 100% Rust and 100% safe code 
encoding and decoding library for the OpenEXR image file format.

[OpenEXR](http://www.openexr.com/) 
is the de-facto standard image format in animation, VFX, and 
other computer graphics pipelines, for it can represent an immense variety of pixel data with lossless compression. 

Features include:
- any number of images placed anywhere in 2d space
- any number of channels in an image (rgb, xyz, lab, depth, motion, mask, ...)
- any type of high dynamic range values (16bit float, 32bit float, 32bit unsigned integer) per channel
- any number of samples per pixel ("deep data")
- uncompressed pixel data for fast file access
- lossless compression for any image type 
- lossy compression for non-deep image types for very small files
- load specific sections of an image without processing the whole file
- compress and decompress images in parallel
- embed any kind of meta data, including custom bytes, with full backwards compatibility

### Current Status

This library is in an early stage of development. It only supports a few of all possible image types.
Currently, deep data and complex compression algorithms are not supported yet.

_Highly experimental!_

__Currently supported:__

- Supported OpenEXR Features
    - [x] custom attributes
    - [x] multi-part images
    - [x] multi-resolution images: mip maps, rip maps
    - [ ] deep data
    - [ ] line order
        - [x] read any
        - [x] write increasing-y
        - [ ] write any
        
    - [ ] compression methods (help wanted)
        - [x] uncompressed
        - [x] zip line
        - [x] zip block
        - [x] rle
        - [ ] piz
        - [ ] pxr24
        - [ ] b44, b44a
        - [ ] dwaa, dwab

- Nice Things
    - [x] read meta data without having to load image data
    - [x] read all contents at once
        - [x] decompress image sections either 
              in parallel or with low memory overhead
    - [x] write all contents at once
        - [ ] compress blocks in parallel
    - [ ] read only some blocks dynamically
    - [ ] write blocks streams, one after another
    - [ ] progress callback
    - [ ] memory mapping
    
    
<!--
- [x] Inspecting Metadata
    - [x] Singlepart
        - [x] Tiles
        - [x] Scan lines
        - [x] Deep Tiles
        - [ ] Deep Scan Lines _(coded, but untested)_
    - [x] Multipart
        - [x] Tiles
        - [x] Scan lines
        - [ ] Deep Tiles _(coded, but untested)_
        - [x] Deep Scan Lines
    - [x] Multi Resolution
        - [x] Singular Resolution
        - [x] MipMaps
        - [x] RipMaps _(coded, but untested)_
    - [x] Non-Standard Attributes
        - [x] Reading those with known names and unknown names
        - [x] Reading those with known types
        - [x] Reading those with unknown types into a plain byte buffer
    - [ ] Nice API for preview attribute extraction
    
- [ ] Decompressing Pixel Data
    - [x] Any LineOrder
    - [x] Any Pixel Type (`f16`, `f32`, `u32`)
    - [x] Multipart
    - [ ] Deep Data
    - [x] Rip/Mip Maps  _(coded, but untested)_
    - [ ] Nice API for RGBA conversion and displaying other color spaces?
    - [ ] Compression Methods
        - [x] Uncompressed
        - [x] ZIPS
        - [x] ZIP
        - [x] RLE
        - [ ] PIZ
        - [ ] RXR24
        - [ ] B44, B44A
        - [ ] DWAA, DWAB

- [ ] Writing images
    - [x] Scan Lines
    - [x] Tiles
    - [x] Multipart
    - [ ] Deep Data
    - [ ] User supplied line order
    - [x] Rip/Mip Maps _(coded, but untested)_
    - [ ] 100% correct meta data
    - [x] Compression Methods
        - [x] Uncompressed
        - [x] ZIPS
        - [x] ZIP
        - [x] RLE
        - [ ] PIZ
        - [ ] RXR24
        - [ ] B44, B44A
        - [ ] DWAA, DWAB
    
- [x] Decompressing multiple blocks in parallel
- [ ] Compressing multiple blocks in parallel

- [ ] Profiling and real optimization
    - [ ] Memory Mapping?
- [ ] IO Progress callback?
- [ ] SIMD
- [ ] Detailed file validation
    - [ ] Channels with an x or y sampling rate other than 1 are allowed only in flat, scan-line based images.
    - [ ] If an image is deep or tiled, then the x and y sampling rates for all of its channels must be 1.
    - [ ] Scan-line based images cannot be multi-resolution images.
    - [ ] Enforce minimum length of 1 for arrays

- [ ] Explore different APIs
    - [ ] Let user decide how to store data
    - [ ] Loading Metadata and specific tiles or blocks separately
-->
    
__Be sure to come back in a few weeks.__

### Example Usage

Read all contents of the exr file at once,
including deep data, mip maps, and u32, f64, and f32 pixel data.
```rust
use exr::prelude::*;

// ReadOptions::default() includes multicore decompression
let image = FullImage::read_from_file("/images/test.exr", ReadOptions::default())?;
println("file meta data: {:#?}", image); // does not print actual pixel values
```

Writing all image contents at once:
```rust
use exr::prelude::*;

let image: FullImage = unimplemented!();
image.write_to_file("/images/written.exr", WriteOptions::default())?;
```

### Cleanup Tasks Before Version 1.0
- [ ] remove all calls to `Option::unwrap()` and `Result::unwrap()`
- [ ] remove all print statements
- [ ] remove inappropriate `assert!` and `debug_assert!` calls,
        all `unimplemented!` calls,
        and use real Error handling instead
- [ ] reduce all not required `pub` usages
- [ ] revisit all TODO items
- [ ] remove all `as` casts 
- [ ] revisit all `[unchecked_index]` array accesses


### Motivation

Using the [Rust bindings to OpenEXR](https://github.com/cessen/openexr-rs) 
requires compiling multiple C++ Libraries 
and setting environment variables, 
which I didn't quite feel like to do, 
so I wrote this library instead.

Also, I really wanted to have a library 
which had an 'X' in its name in my git repositories.

### Goals

`exrs` aims to provide a safe and convenient 
interface to the OpenEXR file format.



This library does not try to be a general purpose image file or image processing library.
Therefore, color conversion, subsampling, and mip map generation are left to other crates for now.
As the original OpenEXR implementation supports those operations, this library may choose to support them later.
Furthermore, this implementation does not try to produce byte-exact file output
matching the original implementation, but only correct output.

#### Safety
This library uses no unsafe code. In fact, this crate is annotated with `#[forbid(unsafe_code)]`.
Its dependencies use unsafe code, though.


All information from a file is handled with caution.
Allocations have a safe maximum size that will not be exceeded at once.


### What I am proud of

-   Flexible API allows for custom parallelization
-   This is a pretty detailed README
-   (more to come)

### Specification

This library is modeled after the 
official [`OpenEXRFileLayout.pdf`](http://www.openexr.com/documentation.html)
document. Unspecified behavior is concluded from the C++ library.

__Things that are not as specified in the PDF file__ (Or were forgotten):

-   String Attributes don't store their length,
    because it can be inferred from the Attribute byte-size.
-   Chunk Part-Number is not u64, but i32.

### PRIORITIES
1. Decode all compression formats
1. Simple rendering of common image formats
1. Profiling and other optimization
1. Tooling (Image Viewer App)
