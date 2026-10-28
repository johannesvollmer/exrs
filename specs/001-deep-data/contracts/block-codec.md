# Contract: Deep Block Codec

## Scope

Covers deep scan line and deep tile chunks, the sample-count table, sample
layout, size validation and chunk coordinates. Compression algorithms are
covered by `compression.md`.

## Matrix

| Behavior | Status | Source Evidence | Test/QA Evidence | Required Next Evidence |
| --- | --- | --- | --- | --- |
| Deep scan line chunk is `i32 y`, three `u64` sizes, table, data | Covered | `block/deep.rs::UncompressedDeepBlock::{decompress_chunk,compress_to_chunk}`, `block/chunk.rs`; `ImfDeepScanLineInputFile.cpp:394-410` | `round_trip_with_all_compression_methods_and_blocks` | — |
| Deep tile chunk is `i32 dx,dy,lx,ly`, then the same payload | Covered | `block/chunk.rs`; `ImfDeepTiledInputFile.cpp` | `round_trip_single_tiled_layer` | — |
| On-disk table restarts cumulative counts at each row | Covered | `block/deep.rs` conversion; `ImfDeepScanLineOutputFile.cpp:694-705` | `tests/deep.rs` round trips | — |
| Block API table is block-wide cumulative `u32` | Covered | `UncompressedDeepBlock::pixel_offset_table` | `tests/deep.rs` block assertions | — |
| Sample bytes are `(row, channel, pixel, sample)` | Covered | `block/deep.rs` docs and conversion; `ImfDeepScanLineOutputFile.cpp:530-670` | `read_known_sample_values` | — |
| Total sample count matches payload size | Covered | `block/deep.rs::validate` | `reject_invalid_deep_layers_when_writing` | — |
| Partial tile tables may be padded | Covered | `block/deep.rs` read path | `round_trip_single_tiled_layer`; OpenEXR partial-tile read | — |
| Duplicate/overlapping chunk offsets rejected | Covered | `block/reader.rs` validation | `duplicate_chunk_offsets_are_read_once`, `overlapping_chunks_are_rejected` | — |
| Pedantic deep reads do not panic | Covered | `block/reader.rs`, `meta/header.rs` | tests with `pedantic` readers | fuzz target |
| Misaligned scan line coordinates rejected | Covered | `meta/header.rs::scan_line_block_tile_coordinates` | `round_trip_with_all_compression_methods_and_blocks` | malformed fixture |

## Invariants

- `pixel_offset_table.len() == pixel_count`.
- Counts are non-decreasing; `table[i] - table[i-1]` is the sample count.
- `table.last() * channels.bytes_per_pixel == sample_data.len()`.
- The on-disk table is never negative.

## Failure Modes

- Wrong table length: `Error::Invalid`.
- Payload size mismatch: `Error::Invalid`.
- Unsupported codec: `Error::NotSupported`.

## Required Evidence Before Marking Complete

- `cargo test --test deep`
- Cross-read: Rust-written scan line and tile files read by OpenEXR 3.1.11;
  OpenEXR-written partial tile read by Rust. Record the OpenEXR version.
