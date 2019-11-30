# rs-exr

This library is a draft of a 100%-Rust and 100%-safe-code 
implementation of the OpenEXR image file format.

[OpenEXR](http://www.openexr.com/) 
files are widely used in animation, VFX, or 
other computer graphics pipelines, because it offers
a high flexibility regarding the data it is able to hold. 


### Current Status

Because rs-exr is currently a draft, it doesn't write images yet.
However, it is already able to decode the metadata of all files from the
[OpenEXR test file repository](https://github.com/openexr/openexr-images/tree/master/TestImages). 

Regarding the actual pixel content, only a few compression methods are supported. 
They are not tested very well yet.
_Highly experimental!_

__Currently supported:__

- [x] Loading bytes without errors
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
        - [x] Reading those with known types
        - [x] Skipping those with unknown types
    
- [ ] Interpreting loaded data as image pixels
    - [ ] Pixel Data Compression
        - [ ] Uncompressed
            - [x] (Half, Float) + scan line + IncreasingY + no mip map levels + no sub sampling + single-part + no deep data, 
            - [ ] Other (untested)
        - [ ] ZIPS
            - [x] (Half, Float) + scan line + IncreasingY + no mip map levels + no sub sampling + single-part + no deep data, 
            - [ ] Other (untested)
        - [ ] ZIP
            - [x] (Half, Float) + scan line + IncreasingY + no mip map levels + no sub sampling + single-part + no deep data, 
            - [ ] Other (untested)
        - [ ] RLE
            - [x] Half + scan line + IncreasingY + no mip map levels + no sub sampling + single-part + no deep data, 
            - [ ] Other (untested)
        - [ ] PIZ
        - [ ] RXR24
        - [ ] B44 / B44A
        - [ ] DWAA / DWAB
    - [ ] Simple RGBA conversion
    - [ ] Simple preview attribute extraction
    
- [ ] Decompressing multiple blocks in parallel
- [ ] Writing images
- [ ] Loading Metadata and specific tiles or blocks separately
- [ ] Memory Mapping
- [ ] Profiling and real optimization
- [ ] Allowing you to parse your own attribute types
- [ ] IO Progress callback
- [ ] Multithreading
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

_Quite unfortunately, there is a ton of unsafe code in the low-level `file` 
decoding and encoding, where an array of i8 is reinterpreted as an array
of u8 in order to write it to memory. I hope we can eventually get rid of that._

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
