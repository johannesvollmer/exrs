# Contract: Deep Robustness

## Scope

Covers malformed deep files, feature gating and resource bounds.

## Matrix

| Behavior | Status | Source Evidence | Test/QA Evidence | Required Next Evidence |
| --- | --- | --- | --- | --- |
| Duplicate chunk offsets read once | Covered | `block/reader.rs` | `duplicate_chunk_offsets_are_read_once` | — |
| Overlapping chunks rejected | Covered | `block/reader.rs` | `overlapping_chunks_are_rejected` | — |
| Tiny skipped chunks keep the reader aligned | Covered | `io.rs::Tracking::seek_read_to`; `block/reader.rs` | `read_chunks_after_skipping_tiny_chunks` | — |
| Size arithmetic is checked or saturating | Covered | `block/deep.rs`, `meta/header.rs` | round-trip and hostile-size tests | fuzz target |
| Invalid deep layers rejected on write | Covered | `image/write/deep.rs` | `reject_invalid_deep_layers_when_writing` | — |
| Sheared deep layers rejected | Covered | `block/deep.rs::validate` | `reject_invalid_deep_layers_when_writing` | — |
| Deep code absent without `deep` | Covered | `#[cfg(feature = "deep")]` modules | `cargo build --no-default-features` | — |
| `deep` enables read, write and compositing | Covered | `Cargo.toml` | `cargo build --features deep` | — |

## Invariants

- No reachable panic on malformed deep input.
- Allocations derived from file counts are bounded and checked.
- Errors are typed `Error` values.

## Failure Modes

- Corrupt table: `Error::Invalid`.
- Truncated chunk: `Error::Invalid`.
- Unsupported codec: `Error::NotSupported`.

## Required Evidence Before Marking Complete

- `cargo test --features deep --test deep`
- `cargo test --features deep` and `cargo test --no-default-features`
- Run the invalid deep fixtures with `pedantic = true`; expect errors, not panics.
