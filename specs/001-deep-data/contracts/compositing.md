# Contract: Deep Compositing (deferred)

## Scope

Deferred to the compositing follow-up PR. Covers
`compositing::{composite, flatten, merge, sort_samples_by_depth}` and the
`DeepSample` helpers. Overlapping volumetric samples are not split.

## Matrix

| Behavior | Status | Source Evidence | Test/QA Evidence | Required Next Evidence |
| --- | --- | --- | --- | --- |
| Samples are gathered from all layers and sorted front to back | Deferred | `image/deep/compositing.rs::Sources::gather_sorted_samples` | `sort_samples_of_real_image`, composite test | — |
| Sort key is exact `Z`, then `ZBack`; ties keep layer order | Deferred | `compare_depths`, `stored_depth_of`; `ImfDeepCompositing.cpp:69-73` | `sort_samples_of_real_image` | — |
| Accumulate with `(1 - alpha)` and stop at full opacity | Deferred | `composite`; `ImfDeepCompositing.cpp:54-60` | `composite_matches_openexr_reference_image` | — |
| `ZBack` falls back to `Z` when absent | Deferred | `Source::depth_samples`; `ImfCompositeDeepScanLine.cpp:180-196` | composite test | — |
| Missing channels contribute zero | Deferred | `Source::value` | composite test | — |
| Result covers the union of layer bounds | Deferred | `union_of_bounds`, `Sources::new` | composite test (`1024x576` at `(0,0)`) | — |
| Result agrees with `composited.exr` | Deferred | `composite` | `composite_matches_openexr_reference_image` (2e-3, one tie pixel) | — |
| `merge` then `flatten` equals `composite` | Deferred | `merge`, `flatten` | composite test | — |
| `flatten_to_rgba` / `composite_samples_front_to_back` early-out | Deferred | `image/deep/compositing.rs` | unit tests in the module | — |

## Invariants

- Output length equals the union data-window area.
- Samples behind an opaque accumulation do not change the result.
- Channels are matched by name, not by position.
- The one near-tie pixel in the reference is the only allowed mismatch.

## Failure Modes

- Empty layers: output is zero.
- Invalid layer bounds: `Error::Invalid`.
- Missing `Z` when sorting: `Error::Invalid`.

## Required Evidence Before Marking Complete

- `cargo test --test deep`
- Optional manual check: run OpenEXR 3.1.11 `DeepCompositing` over the four
  LowResLeftView images and compare with the reference; expect one pixel above
  `2e-3` at `(716, 262)`.
