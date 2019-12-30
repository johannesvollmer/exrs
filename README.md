# rs-exr

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

- [x] Loading Metadata
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
        - [ ] RipMaps _(coded, but untested)_
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
    - [ ] Rip/Mip Maps  _(coded, but untested)_
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
    - [ ] Tiles
    - [ ] Multipart
    - [ ] Deep Data
    - [ ] Line Order
    - [ ] Rip/Mip Maps
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
- [x] Compressing multiple blocks in parallel

- [ ] Nice API for loading Metadata and specific tiles or blocks separately
- [ ] Profiling and real optimization
    - [ ] Memory Mapping?
- [ ] IO Progress callback
- [ ] SIMD

__Be sure to come back in a few weeks.__

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

### Architecture

The main parts of this library are:

-   `file` - Provides raw access to the files contents.

    The File is loaded from a byte stream into really
    low level structures. No decompression will take place up to this stage,
    and no data will be rearranged compared to the file layout.
    This representation is as close to the file layout as feasible,
    in order to be really fast if no decompression is required.
    Basic file content validation is made to avoid processing invalid files.
    
-   `image` - Supports interpreting the raw file 
    and supports (but is not enforcing) exr conventions.
 
    It is able to convert between the raw file contents
    and usable formats, for example `RGBA` arrays. This is optional
    because the OpenEXR format is very detailed, and converting to
    simpler representation will lose that detail. This part of the 
    library is provided mainly for some very simple use-cases, like
    displaying a larger preview than OpenEXR already contains.

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
