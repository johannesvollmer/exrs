# Contract: Deep Metadata

## Scope

Covers `type`, `version`, `chunkCount`, `maxSamplesPerPixel`, `deepImageState`
and the file-version flags.

## Matrix

| Behavior | Status | Source Evidence | Test/QA Evidence | Required Next Evidence |
| --- | --- | --- | --- | --- |
| `deepImageState` is one byte, enum 0..=3 | Covered | `meta/deep_state.rs`, `meta/attribute.rs`; `ImfDeepImageStateAttribute.cpp:35-49` | `deep_image_state_attribute` | — |
| Invalid state values `4+` are rejected | Covered | `DeepImageState::from_u8` | unit tests | malformed fixture |
| `deep` is not required to parse the attribute | Covered | `meta/deep_state.rs` unconditional; `AttributeValue::DeepImageState` | `cargo test --no-default-features` | — |
| `maxSamplesPerPixel == -1` means unknown | Covered | `meta/header.rs` parse branch | `read_all_deep_images_in_repository` | negative-value fixture |
| `version` must be `1` | Covered | `meta/header.rs`; `ImfHeader.cpp:733-747` | `version_flags_of_deep_files` | — |
| `type` is `deepscanline` / `deeptile` | Covered | `meta/header.rs` block type match | scan line and tile round trips | — |
| Single-part deep tiles do not set the flat tiled flag | Covered | `meta/mod.rs` requirement inference | `version_flags_of_deep_files` | — |

## Invariants

- `deep == true` implies a deep part type and a deep-capable compression.
- Attributes use the size OpenEXR documents.
- Missing `maxSamplesPerPixel` is unknown, not zero.

## Failure Modes

- `version != 1`: `Error::NotSupported`.
- Invalid `deepImageState`: `Error::Invalid`.

## Required Evidence Before Marking Complete

- `cargo test --features deep --test deep`
- `cargo test --no-default-features`
- `exrheader` on a Rust-written deep file reports `type`, `version`,
  `chunkCount`, `maxSamplesPerPixel` and `deepImageState`.
