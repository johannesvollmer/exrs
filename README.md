[![Rust Docs](https://docs.rs/exr/badge.svg)](https://docs.rs/exr) 
[![Crate Crate](https://img.shields.io/crates/v/exr.svg)](https://crates.io/crates/exr) 
[![Rust Lang Version](https://img.shields.io/badge/rustc-1.43+-lightgray.svg)](https://blog.rust-lang.org/2020/04/23/Rust-1.43.0.html) 
[![Lines of Code](https://tokei.rs/b1/github/johannesvollmer/exrs?category=code)](https://tokei.rs)

# EXRS

This library is a 100% Rust and 100% safe code library for
reading and writing OpenEXR images.
See [the examples](https://github.com/johannesvollmer/exrs/tree/master/examples) for a first impression.

[OpenEXR](http://www.openexr.com/)
is the de-facto standard image format in animation, VFX, and 
other computer graphics pipelines, for it can represent an immense variety of pixel data with lossless compression. 

Features include:
- any number of layers placed anywhere in 2d space
- any number of channels in an image (rgb, xyz, lab, depth, motion, mask, ...)
- any type of high dynamic range values (16bit float, 32bit float, 32bit unsigned integer) per channel
- any number of samples per pixel ("deep data")
- uncompressed pixel data for fast file access
- lossless compression for any image type 
- lossy compression for non-deep image types to produce very small files
- load specific sections of an image without processing the whole file
- compress and decompress image pixels in parallel
- embed any kind of meta data, including custom structs, with full backwards compatibility

### Current Status

This library is in an early stage of development. It only supports a few of all possible image types.
Currently, deep data and complex compression algorithms are not supported yet.

_Highly experimental!_

__Currently supported:__

- Supported OpenEXR Features
    - [x] custom attributes
    - [x] multi-part images
    - [x] multi-resolution images: mip maps, rip maps
    - [x] any line order
    - [x] extract meta data of any file, 
          including files with deep data and any compression format
    - [ ] channel subsampling
    - [ ] deep data
    - [ ] compression methods (help wanted)
        - [x] uncompressed
        - [x] zip line (lossless)
        - [x] zip block (lossless)
        - [x] rle (lossless)
        - [ ] piz (lossless)
        - [x] pxr24 (lossless for f16 and u32)
        - [ ] b44, b44a
        - [ ] dwaa, dwab

- Nice Things
    - [x] no external dependency or environment variable paths to set up
    - [x] read meta data without having to load image data
    - [x] read all contents at once
        - [x] decompress image sections either 
              in parallel or with low memory overhead
    - [x] write all contents at once
        - [x] compress blocks in parallel
    - [x] read only some blocks dynamically
    - [x] read and write progress callback
    - [x] abortable read and write
    - [ ] write blocks streams, one after another
    - [ ] memory mapping
    

If you encounter an exr file that cannot be opened by this crate, 
please leave an issue on this repository, containing the image file.

    
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
    - [x] User supplied line order
    - [x] Rip/Mip Maps _(coded, but untested)_
    - [x] 100% correct meta data
    - [x] Compression Methods
        - [x] Uncompressed
        - [x] ZIPS (lossless)
        - [x] ZIP (lossless)
        - [x] RLE (lossless)
        - [ ] PIZ (lossless)
        - [x] PXR24 (lossless for f16 and u32)
        - [ ] B44, B44A
        - [ ] DWAA, DWAB
    
- [x] Decompressing multiple blocks in parallel
- [x] Compressing multiple blocks in parallel

- [ ] Profiling and real optimization
    - [ ] Memory Mapping?
- [x] IO Progress callback?
- [ ] SIMD
- [x] Detailed file validation
    - [x] Channels with an x or y sampling rate other than 1 are allowed only in flat, scan-line based images.
    - [x] If the headers include timeCode and chromaticities attributes, then the values of those attributes must also be the same for all parts of a file
    - [x] Scan-line based images cannot be multi-resolution images. (encoded in type system)
    - [x] Scan-line based images cannot have unspecified line order apparently?
    - [x] layer name is required for multipart images
    - [x] Enforce minimum length of 1 for arrays
    - [x] [Validate data_window matches data size when writing images] is not required because one is inferred from the other
    - [x] Channel names and layer names must be unique
    
- [x] Explore different APIs
    - [x] Let user decide how to store data
    - [x] Loading Metadata and specific tiles or blocks separately
-->
    

### Usage

Add this to your `Cargo.toml`:
```toml
[dependencies]
exr = "0.7.5"

# also, optionally add this to your crate for smaller binary size 
# and better runtime performance
[profile.release]
lto = true
```

The master branch of this repository is always an up-to-date version.

### Example

Example: Write all image contents to an exr file at once.

```rust
use exr::prelude::rgba_image as exrs;

fn main() {
    let my_image = unimplemented!("this is your own image value");

    let get_pixel = |position: exrs::Vec2<usize>| {
        let [r, g, b, a] = my_image.pixel_at_xy(position.x(), position.y());
        exrs::Pixel::rgba(r, g, b, a)
    };

    // make exr convert all numbers to f16 automatically 
    let mut image_info = exrs::ImageInfo::rgb((my_image.width, my_image.height), SampleType::F16);
    image_info.layer_attributes.comments = "Generated with Rust".try_into().unwrap();

    // write the pixels to a file
    image_info
        .with_encoding(exrs::Encoding::small()) // compress image contents to keep the file small
        .write_to_file(
            "tests/images/out/generated_rgba.exr",
            exrs::write_options::high(), // using all CPUs
            &get_pixel // and our custom image pixels
        ).unwrap();
}
```

See the examples folder for more examples.


### Motivation

Using any Rust bindings to the original OpenEXR 
library unfortunately requires compiling multiple C++ Libraries 
and possibly setting environment variables, 
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
-   Difficult to misuse API
-   This is a pretty detailed README
-   (more to come)

### Running Tests

To run all fast tests, use `cargo test`.
To start fuzzing indefinitely, use `cargo test --package exr --test fuzz fuzz -- --exact --ignored`.

### Specification

This library is modeled after the 
official [`OpenEXRFileLayout.pdf`](http://www.openexr.com/documentation.html)
document. Unspecified behavior is concluded from the C++ library.

### PRIORITIES
1. Support all compression formats
1. Support Deep Data
1. Simple rendering of common image formats
1. Profiling and other optimization
1. Tooling (Image Viewer App)
