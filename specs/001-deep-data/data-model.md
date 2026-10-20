# Data Model: Deep Data

## Entities

### Header (deep part)

- **Owner**: `crate::meta::header::Header`.
- **Identity**: layer index plus `(dataWindow, name)`.
- **Fields**: `deep: bool`, `deep_data_version: Option<i32>` (only `1` is
  supported), `max_samples_per_pixel: Option<usize>` (`None` means unknown /
  `-1`), `deep_image_state: Option<DeepImageState>`, `blocks`, `compression`.
- **Invariants**: `deep` requires a deep block type (`deepscanline` /
  `deeptile`) and a compression that supports deep data. `chunk_count` matches
  the block enumeration.
- **Persistence**: OpenEXR header attributes; `version` is `1`, `type` selects
  the part kind, `chunkCount` is an `i32`.

### DeepImageState

- **Owner**: `crate::meta::deep_state::DeepImageState` (always compiled).
- **Identity**: one-byte value `0..=3`.
- **Fields**: `Messy`, `Sorted`, `NonOverlapping`, `Tidy`.
- **Invariants**: matches `ImfDeepImageState.h` exactly; `4+` is invalid.
- **Persistence**: `deepImageState` attribute, one byte.

### UncompressedDeepBlock

- **Owner**: `crate::block::deep::UncompressedDeepBlock`.
- **Identity**: `BlockIndex { layer, pixel_position, pixel_size, level }`.
- **Fields**:
  - `pixel_offset_table: Vec<u32>`: block-wide cumulative sample counts, one
    entry per pixel in row-major order. `table[0]` is the first pixel's count;
    pixel `i` has `table[i] - table[i-1]` samples.
  - `sample_data: ByteVec`: native-endian, in file order: for each row, for each
    channel (alphabetical), for each pixel in the row, all samples of that
    pixel.
- **Invariants**: counts are non-decreasing; total samples times
  `channels.bytes_per_pixel` equals `sample_data.len()`.
- **Persistence**: encoded to a deep chunk; the on-disk table restarts at each
  row and is converted to block-wide cumulative form.

### DeepChannels

- **Owner**: `crate::image::deep::DeepChannels`.
- **Identity**: resolution plus one `sample_offsets` array for all pixels.
- **Fields**: `resolution: Vec2<usize>`, `sample_offsets: Vec<usize>` (one more
  entry than pixels), `list: SmallVec<[AnyChannel<FlatSamples>; 4]>` sorted by
  name.
- **Invariants**: channel sample data length equals
  `sample_offsets.last()`; every channel has the same sample count; channels
  are unique and sorted.
- **Persistence**: converted to/from `UncompressedDeepBlock` during I/O.

### DeepImage / DeepLayersImage

- **Owner**: `crate::image`.
- **Identity**: one layer (`DeepImage`) or many layers (`DeepLayersImage`).
- **Fields**: shared `ImageAttributes`, layer data with `DeepChannels`.
- **Invariants**: each deep layer has at least one channel; flat and deep layers
  may be mixed in a file but not in one layer.

### DeepSample

- **Owner**: `crate::image::deep::compositing::DeepSample`.
- **Fields**: `depth: f32`, `color: [f32; 3]` (premultiplied), `alpha: f32`.
- **Persistence**: in-memory helper; not stored directly.

### DeepSampleStatistics

- **Owner**: `crate::image::deep::DeepSampleStatistics`.
- **Fields**: pixel count, total samples, min/max samples per pixel.
- **Persistence**: computed on demand.

## Ownership Matrix

| Concept | Owner | Consumers | Status | Evidence |
| --- | --- | --- | --- | --- |
| Chunk wire layout | `block::deep` | block reader/writer | Covered | `tests/deep.rs`, OpenEXR cross-reads |
| Table/layout conversion | `block::deep` | `UncompressedDeepBlock` | Covered | `tests/deep.rs` round trips |
| Deep compression | `compression::{zip,rle}` | deep chunk codec | Covered | `round_trip_with_all_compression_methods_and_blocks` |
| Metadata | `meta::header`, `meta::deep_state` | readers, writers | Covered | `deep_image_state_attribute` |
| High-level image | `image::deep`, `image::read::deep`, `image::write::deep` | users | Covered | `read_all_deep_images_in_repository`, `round_trip_synthetic_layers` |
| Compositing | `image::deep::compositing` | users | Covered | `composite_matches_openexr_reference_image` |

## Migration Rules

- Files written with the old `Rational` `deepImageState` are invalid OpenEXR and
  are not supported.
- `maxSamplesPerPixel == -1` is accepted and mapped to `None`; callers must not
  treat `None` as zero.
- Offset tables of partial tiles may be padded; readers use the first
  `area * 4` bytes and reject shorter tables.
