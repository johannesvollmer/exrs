# rs-exr

This library is a draft of a pure and safe-code-only 
Rust implementation of the OpenEXR image file format.

[OpenEXR](http://www.openexr.com/) 
files are widely used in animation, VFX, or 
other computer graphics pipelines, because it offers
a high flexibility regarding the data it is able to hold. 


### Current Status

Because rs-exr is currently a draft, 
it can only just load some specific exr images.
Highly experimental!

__Be sure to come back in a few weeks.__

### Motivation

Using the [Rust bindings to OpenEXR](https://github.com/cessen/openexr-rs) 
requires compiling multiple C++ Libraries 
and setting environment variables, 
which I didn't quite feel like to do, 
so I wrote this library instead.

### Architecture

The main parts of this library will be:

-   `file` - Provides raw access to the files contents.
    The File is loaded from a byte stream into really
    low level structures. No decompression will take place up to this stage,
    and no data will be rearranged compared to the file layout.
    This representation is as close to the file layout as feasible,
    in order to be really fast if no decompression is required.
-   `image` - Simplifies converting between the raw file contents
    and usable formats, for example `RGBA` arrays. This is optional
    because the OpenEXR format is very detailed, and converting to
    simpler representation will lose that detail. This part of the 
    library is provided mainly for some very simple use-cases, like
    displaying a larger preview than OpenEXR already contains.

### Goals

`rs-exr` aims to provide a safe and convenient 
interface to the OpenEXR file format.
We try to prevent writing invalid OpenEXR files by
either taking advantage of Rusts type system, 
or runtime checks if the type system does not suffice.

### Specification

This library is modeled after the 
official [`OpenEXRFileLayout.pdf`](http://www.openexr.com/documentation.html)
document, but it's not completely up to date
(the C++ library has greater priority).

__Things that are not as specified in the PDF file__ (Or were forgotten):

-   String Attributes don't store their length,
    because it can be inferred from the Attribute byte-size.
-   Chunk Part-Number is not u64, but i32.