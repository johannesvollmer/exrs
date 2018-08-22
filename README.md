# rs-exr

This library is a draft of a pure and safe-code-only 
Rust implementation of the OpenEXR image file format.

### Current Status

Because rs-exr is currently a draft, 
it can only just load some specific exr images.
Highly experimental!

Stay tuned, and be sure to come back in a few weeks.

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

### Motivation

Using the [Rust bindings to OpenEXR](https://github.com/cessen/openexr-rs) 
requires compiling multiple C++ Libraries 
and setting environment variables, 
which I was too lazy to do, so I just 
wrote this library instead.

### Goals

`rs-exr` aims to provide a safe and convenient 
interface to the OpenEXR file format.
We try to prevent writing invalid OpenEXR files by
either taking advantage of Rusts type system, 
or runtime checks if the type system does not suffice.

### Specification

This library is modeled after the 
official `openexrfilelayout.pdf` document,
but it's not up to date
(the C++ library has greater priority).

### Things that are not up to date in the PDF file:
(Or were forgotten)

-   String Attributes don't always store their length,
    because it can be inferred from the Attribute byte-size.
-   Chunk Part-Number is not u64, but i32.