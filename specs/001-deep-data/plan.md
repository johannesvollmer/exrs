# Implementation Plan: Deep Data

**Branch**: `claude/phase3-runtime-integration-011CV4cYA84DRjXLZ6J1rrpU` |
**Date**: 2026-10-20 | **Spec**: `specs/001-deep-data/spec.md`.

## Summary

Combine the deep-data work with PR #292, which supplies the high-level
`AnyDeepChannels` image API on current `master` and passes the OpenEXR cross-checks.
This branch keeps that API, keeps the block API as an escape hatch, and adds the
SDD artifacts, API-guideline fixes, and the documented compositing tolerance.

## Technical Context

**Language/Version**: Rust 2018, MSRV 1.61 (crate `exr` 1.74.0).

**Primary Dependencies**: `half`, `miniz_oxide`, `zune-inflate`, `smallvec`,
`lebe`; optional `rayon`/`rayon-core`.

**Storage**: OpenEXR deep scan line and deep tile parts: header attributes,
chunk offset table, cumulative sample-count table, channel-interleaved sample
data. Native-endian in memory, little-endian on the wire.

**Testing**: `cargo test`, focused integration tests in
`tests/deep.rs`, plus manual cross-checks against OpenEXR 3.1.11.

**Target Platform**: Portable Rust; 32-bit targets must not overflow.

**Performance Goals**: No new allocation on the flat path; deep decoding is
linear in payload size. Flattening may look up a block per pixel; no quadratic
growth in blocks.

**Constraints**: Deep code must compile with the default features and with
`--no-default-features`. No new panics on untrusted input. Public API changes
must be named in the PR.

## Constitution Check

- Source-of-truth: `specs/001-deep-data/spec.md`, pointer in `.specify/feature.json`.
- Required artifacts: present in this directory.
- Evidence policy: each contract has `Required Evidence Before Marking Complete`.
- Scope: one contract row per change; no unrelated refactors.

## Project Structure

```text
specs/001-deep-data/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── tasks.md
├── checklists/requirements.md
└── contracts/
    ├── block-codec.md
    ├── compression.md
    ├── metadata.md
    ├── compositing.md
    └── robustness.md
```

## Execution Rules

1. Resolve ambiguous behavior before implementation.
2. Work one contract row at a time.
3. Add or update tests from the contract invariants.
4. Mark rows `Covered` only after evidence is present.

## Artifact Checklist

- [x] Active feature pointer is updated.
- [x] Required artifact set exists.
- [x] Each contract file has `Required Evidence Before Marking Complete`.
- [x] Each `Covered` row cites source evidence.
- [ ] Each `Covered` row cites executable or manual QA evidence.

## Implementation Slices

1. **Metadata** — type `deepImageState` as `DeepImageState`, accept
   `maxSamplesPerPixel == -1`, keep parsing available without `deep`. Evidence:
   `meta::test::round_trip`, reading an OpenEXR deep header.
2. **Block codec** — verify `UncompressedDeepBlock::decompress_chunk` and
   `compress_to_chunk` against the C++ chunk layout; remove debug prints; add
   cross-read of Rust-written files by OpenEXR. Evidence: round-trip test plus
   `exrinfo`/C++ read.
3. **Compression** — verify `compress_raw`/`decompress_raw` for `RLE`, `ZIP1`,
   `ZIP1`, including the "store raw when not smaller" rule. Evidence: files
   written by Rust read by OpenEXR and vice versa.
4. **Compositing (deferred)** — moved to a follow-up PR. The contract and
   research notes remain the specification for that work.
5. **API review** — apply Rust API guideline fixes to the combined API:
   `to_channel_list`, private channel-name constants, descriptive byte-range
   method, and the derives that `f32` fields permit.
6. **Verification** — run default and `--no-default-features` builds and tests,
   update the PR body, and split the changes into the stacked commits listed in
   `tasks.md`.
