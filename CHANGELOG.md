# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [Unreleased]
### Added
- Adds reading, writing, and compositing deep images, behind the new `deep` feature.
  Deep layers contain any number of samples per pixel and use `DeepChannels` as channel data.
  - Read deep images with `read().deep_data()`, and write them with `image.write()`, like flat images.
    Images may contain both flat and deep layers. Use `all_valid_layers()` to read all deep layers of such images,
    or implement `ReadDeepLayers` for a custom selection.
  - Supports deep scan lines and deep tiles, compressed with `Uncompressed`, `RLE` or `ZIP1`.
    Offset tables of partial tiles are written such that all versions of the `OpenEXR` library can read them,
    except for some tiles that are only one or two pixels wide or high, where that is not possible.
    The offset tables written by older versions of the `OpenEXR` library can also be read.
  - `image::deep::compositing` can sort, merge, and flatten deep layers.
  - The block-level API offers `UncompressedDeepBlock`, with sequential and parallel (de)compression.
- Adds the `DeepImageState` attribute type, for the standard `deepImageState` attribute.

### Changed
- Use compiler autovectorization for DWA transforms, replacing handwritten SIMD kernels with batch-level AVX2 runtime dispatch via `multiversion`.
- __Breaking:__ `LayerAttributes::deep_image_state` now has the type `Option<DeepImageState>`
  instead of `Option<Rational>`, matching the `OpenEXR` format,
  and `AttributeValue` has a new `DeepImageState` variant.
- A negative `maxSamplesPerPixel` attribute, which means "unknown", no longer fails to load.
- With the `deep` feature, flat readers using `first_valid_layer()` skip deep layers,
  and otherwise return `Error::NotSupported` for deep layers.
- Compressing a flat block for a deep layer now returns an error.
- Reading without pedantic checks no longer decompresses the same chunk multiple times
  if the offset table contains duplicate entries.
- `compress_all_blocks_parallel` now also writes the queued chunks if the blocks do not cover the whole file.
- Files with deep layers whose chunks overlap are rejected, as they could require any amount of memory.
- Fixes an integer overflow on 32-bit platforms for files with very large tile indices.

## [1.74.2] - 2026-07-10
### Added
- Adds encoding images using DWAA/DWAB compression, without any API change.


## [1.74.1] - 2026-07-08
### Added
- Adds decoding images that have DWAA/DWAB compression, without any API change.
- Introduces safe SIMD runtime dispatch using `pulp`, for DWA decoding.
