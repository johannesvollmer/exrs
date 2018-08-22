# rs-exr

This library is a draft of a pure and safe-code-only 
Rust implementation of the OpenEXR image file format.

Because rs-exr is currently a draft, 
it can only extract some meta-data from exr images.
Highly experimental!

Stay tuned, and be sure to come back in a few weeks.

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
-   Chunk Part Numbers are not always u64, but may be u8
    (which is sufficient for most images)