# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [Unreleased]
### Added
- Adds reading and writing deep images.
  Deep layers contain any number of samples per pixel and use `DeepChannels` as channel data.
  - Read deep images with `read().deep_data()`, and write them with `image.write()`, like flat images.
    Images may contain both flat and deep layers. Use `all_valid_layers()` to read all deep layers of such images,
    or implement `ReadDeepLayers` for a custom selection.
  - Supports deep scan lines and deep tiles, compressed with `Uncompressed`, `RLE` or `ZIP1`.
    Offset tables of partial tiles are written such that all versions of the `OpenEXR` library can read them,
    except for some partial tiles where that is not possible, for example of tiles that are only one or two pixels wide or high,
    and except for uncompressed tiles of more than 4,194,304 pixels, whose tables are not padded to avoid huge files.
    The offset tables written by older versions of the `OpenEXR` library can also be read.
  - The block-level API offers `UncompressedDeepBlock`. Deep blocks are (de)compressed with the same
    sequential and parallel pipeline as flat blocks, which is now generic over the new `PixelBlock` trait.
    For example, `ParallelBlockDecompressor<R, UncompressedDeepBlock>` decompresses deep blocks.
- Adds the `DeepImageState` attribute type, for the standard `deepImageState` attribute.

### Changed
- __Breaking:__ `LayerAttributes::deep_image_state` now has the type `Option<DeepImageState>`
  instead of `Option<Rational>`, matching the `OpenEXR` format,
  and `AttributeValue` has a new `DeepImageState` variant.
- A negative `maxSamplesPerPixel` attribute, which means "unknown", no longer fails to load.
- Flat readers using `first_valid_layer()` skip deep layers,
  and otherwise return `Error::NotSupported` for deep layers.
- Compressing a flat block for a deep layer now returns an error.
- Reading without pedantic checks no longer decompresses the same chunk multiple times
  if the offset table contains duplicate entries.
- `compress_all_blocks_parallel` now also writes the queued chunks if the blocks do not cover the whole file.
- Files with deep layers whose chunks overlap are rejected, as they could require any amount of memory.
- Fixes an integer overflow on 32-bit platforms for files with very large tile indices.
- Fixes reading a chunk that starts 1 to 15 bytes after the end of the previously read chunk,
  for example after skipping a tiny chunk when reading only some layers, which could fail or read the wrong chunk.

## [1.74.3] - 2026-11-09
### Changed
- Use compiler autovectorization for DWA transforms, replacing handwritten SIMD kernels with batch-level AVX2 runtime dispatch via `multiversion`.
- Speeds up the forward DCT of DWA compression by about 50x on ARM NEON.
- Replaces the `pulp` and `num-complex` dependencies with `multiversion`, which is only used on x86 and x86-64.
- Replaces the test-only cargo features `avx2-tests`, `sse2-tests` and `simd-benches` with a single `dct-benches` feature.

### Fixed
- Fixes a panic when decoding damaged B44 files, which aborted the whole process when decoding in parallel.
- Fixes damaged DWA files making the Huffman decoder reserve huge amounts of memory up front, which aborted the whole process.


## [1.74.2] - 2026-07-10
### Added
- Adds encoding images using DWAA/DWAB compression, without any API change.


## [1.74.1] - 2026-07-08
### Added
- Adds decoding images that have DWAA/DWAB compression, without any API change.
- Introduces safe SIMD runtime dispatch using `pulp`, for DWA decoding.
