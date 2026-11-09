# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [Unreleased]


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
