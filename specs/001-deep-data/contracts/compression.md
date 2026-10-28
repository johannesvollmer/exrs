# Contract: Deep Compression

## Scope

Covers `Uncompressed`, `RLE` and `ZIP1` payloads for deep sample data and
sample-count tables. Lossy codecs and `ZIP16` are out of scope; OpenEXR rejects
them for deep parts.

## Matrix

| Behavior | Status | Source Evidence | Test/QA Evidence | Required Next Evidence |
| --- | --- | --- | --- | --- |
| ZIP uses reorder + predictor + zlib | Covered | `compression/zip.rs`; `ImfZip.cpp:55-124` | `round_trip_with_all_compression_methods_and_blocks` | OpenEXR reads Rust ZIP1 (manual, done) |
| RLE uses reorder + predictor + RLE | Covered | `compression/rle.rs`; `internal_rle.c:68-121` | same test | OpenEXR reads Rust RLE (manual, done) |
| Store raw when compression does not shrink | Covered | `compression/{zip,rle}.rs` | round-trip tests | incompressible fixture |
| Reader treats `packed == unpacked` as raw | Covered | `compression/{zip,rle}.rs`; `compression.c:412-416` | round-trip tests | — |
| Sample-count table uses the same preprocessing | Covered | `compression/{zip,rle}.rs`, `block/deep.rs` | round-trip tests | — |
| `ZIP16` is rejected for deep parts | Covered | `Compression::supports_deep_data` | `reject_invalid_deep_layers_when_writing` | — |
| Big-endian hosts convert deep values | Partial | `block/deep.rs` conversion | CI `big-endian` job | confirm CI pass |

## Invariants

- A decompressed payload has exactly the declared size.
- A payload smaller than raw is compressed; equal or larger is stored raw.
- Preprocessing order is reorder, then predictor on write; reverse on read.

## Failure Modes

- zlib failure: `Error::Invalid`.
- RLE size mismatch: `Error::Invalid`.
- Unsupported codec: `Error::NotSupported`.

## Required Evidence Before Marking Complete

- `cargo test --test deep`
- Write each codec with Rust, read with OpenEXR, compare every sample; repeat in
  the other direction.
