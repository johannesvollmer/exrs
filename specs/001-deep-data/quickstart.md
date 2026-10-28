# Quickstart: Deep Data

## Prerequisites

- Rust toolchain supported by the crate (MSRV 1.61).
- Repository test images under
  `tests/images/valid/openexr/v2/LowResLeftView`.
- Optional: OpenEXR 3.1.11 tools (`exrinfo`, `exrheader`) for cross-checks.

## Validation Commands

```bash
cargo build --no-default-features
cargo build
cargo test
cargo test --no-default-features
cargo test --doc
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo run --example 9_deep_data
```

## Manual QA

1. `exrinfo tests/images/valid/openexr/v2/LowResLeftView/Balls.exr` and compare
   `type`, `version`, `chunkCount`, `maxSamplesPerPixel` and the channel list
   with `read().deep_data().first_valid_layer()`.
2. Write a deep scan line file with `Uncompressed`, `RLE` and `ZIP1`; open each
   with `exrinfo` and, where available, read it with OpenEXR 3.1.11.
3. Write a deep tile with a partial edge tile and read it with OpenEXR.

## Evidence Recording

After validation, update the contract row with the test name, command, OpenEXR
version and date.
