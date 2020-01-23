# exrs (exr-rs)

This library is a 100% Rust and 100% safe code 
encoding and decoding library for the OpenEXR image file format.

[OpenEXR](http://www.openexr.com/) 
files are widely used in animation, VFX, and 
other computer graphics pipelines, because it offers
a high flexibility regarding the data it is able to hold. 


### Current Status

This library is in an early stage of development. It only supports a few of all possible image types.
Currently, deep data and complex compression algorithms are not supported yet.

_Highly experimental!_

__Currently supported:__

- Supported OpenEXR Features
    - [x] custom attributes
    - [x] multi-part images
    - [x] multi-resolution images: mip maps, rip maps
    - [ ] deep data (next up)
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

### Cleanup Tasks Before Version 1.0
- [ ] remove all calls to `Option::unwrap()` and `Result::unwrap()`
- [ ] remove all print statements
- [ ] remove inappropriate `assert!` and `debug_assert!` calls,
        all `unimplemented!` calls,
        and use real Error handling instead
- [ ] reduce all not required `pub` usages
- [ ] revisit all TODO items
- [ ] remove all `as` casts 


### Motivation

Using the [Rust bindings to OpenEXR](https://github.com/cessen/openexr-rs) 
requires compiling multiple C++ Libraries 
and setting environment variables, 
which I didn't quite feel like to do, 
so I wrote this library instead.

Also, I really wanted to have a library 
which had an 'X' in its name in my git repositories.

### Goals

`rs-exr` aims to provide a safe and convenient 
interface to the OpenEXR file format.
We try to prevent writing invalid OpenEXR files by
either taking advantage of Rusts type system, 
or runtime checks if the type system does not suffice.

### What I am proud of

-   For simple files, very few heap allocations are made during loading
    (only for offset table data and actual pixel data)
-   This is a pretty detailed README
-   (more to come)

### Specification

This library is modeled after the 
official [`OpenEXRFileLayout.pdf`](http://www.openexr.com/documentation.html)
document, but it's not completely up to date
(the C++ library has greater priority).

__Things that are not as specified in the PDF file__ (Or were forgotten):

-   String Attributes don't store their length,
    because it can be inferred from the Attribute byte-size.
-   Chunk Part-Number is not u64, but i32.
-   Calculating the offset table is really really complicated,
    and it could have been a single u64 in the file
    (which would not even need more memory if one decided to make
    the `type` attribute an enum instead of a string)
    
Okay, the last one was a rant, you got me.

### PRIORITIES
1. Also write simple exr files 
1. Decode all compression formats
1. Simple rendering of common image formats
1. Profiling and other optimization
