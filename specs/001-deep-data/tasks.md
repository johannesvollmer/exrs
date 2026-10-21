# Tasks: Deep Data (Combined)

**Input**: `spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/`.

This branch combines PR #292's deep implementation with the SDD artifacts and
API-guideline fixes from the older deep branch.

## Phase 1: Contract Setup

- [x] T001 Point `.specify/feature.json` at `specs/001-deep-data`.
- [x] T002 Create the required artifact set.
- [x] T003 Add contract matrices and required-evidence sections.
- [x] T004 Record the combined-API decision in `research.md`.

## Phase 2: API Review

- [x] T005 Rename `AnyDeepChannels::channel_list` to `to_channel_list`.
- [x] T006 Make the depth/alpha channel-name constants private.
- [x] T007 Rename `pixel_sample_bytes_range` to `pixel_sample_byte_range`.
- [x] T008 Add `Hash` to `UncompressedDeepBlock` and `Debug` to `AnyDeepChannels`.
- [x] T009 Add `Eq`, `Hash` and `Default` to `DeepSampleStatistics`.
- [ ] T010 Review remaining public items against `lang/rust/AGENTS.md`.
- [ ] T011 Decide whether `RGBADepthF32Premultiplied::new_*` should become `from_*`.

## Phase 3: Correctness Evidence

- [x] T012 Keep the OpenEXR reference test with its one-tie tolerance.
- [x] T013 Record why exact equality is the wrong oracle.
- [x] T014 Cross-read Rust scan line files with OpenEXR 3.1.11.
- [x] T015 Cross-read an OpenEXR partial tile with Rust.
- [ ] T016 Add the C++ `DeepCompositing` harness as a documented manual QA step.

## Phase 4: Performance

- [ ] T017 Survey deep hot paths for unnecessary copies.
- [ ] T018 Decide whether compositing should use `rayon` (per-pixel work).
- [ ] T019 Decide whether explicit SIMD is warranted; autovectorization is the
  current default and `multiversion` AVX2 exists only for DWA.

## Phase 5: Verification

- [ ] T020 `cargo fmt --all -- --check`
- [ ] T021 `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] T022 `cargo test --features deep`
- [ ] T023 `cargo test --no-default-features`
- [ ] T024 `cargo test --doc --features deep`
- [ ] T025 Run the example `examples/9_deep_data.rs`.
- [ ] T026 Update the PR body and comment on #292 coordination.

## Completion Gate

- [ ] T027 Every `Covered` contract row cites source and executable evidence.
- [ ] T028 No warnings, no `get_` methods in new public API, formatting clean.

## Phase 7: Maintainer Review Follow-Up

- [ ] T034 Rename `AnyDeepChannels` to `AnyAnyDeepChannels` and related types.
- [ ] T035 Rename the RGB sample value to `RGBADepthF32Premultiplied`; add a
  `DeepPixel` alias.
- [ ] T036 Move duplicate channel detection into `ChannelList` validation.
- [ ] T037 Return `Invalid` when a flat reader sees a deep layer.
- [ ] T038 Move the compositing module to the follow-up PR and rework its
  pixel API as reviewed.
- [ ] T039 Unify the block reader/writer deep and flat pipelines behind one
  enum.
