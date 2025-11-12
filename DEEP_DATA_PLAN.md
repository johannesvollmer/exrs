# Deep Data Implementation Plan for exrs

**Status**: Planning Phase
**Target**: Add OpenEXR deep data support to exrs crate
**Approach**: Incremental, correctness-first, feature-gated

## Implementation Strategy

### Feature Flag
All deep data functionality will be behind a `deep-data` feature flag:
```toml
[features]
deep-data = []
```

### Implementation Phases

#### Phase 1: Core Data Structures (Week 1)
**Files to create:**
- `src/image/deep_samples.rs` - Deep sample storage
- `src/meta/deep_state.rs` - DeepImageState enum

**Files to modify:**
- `src/meta/attribute.rs` - Add DeepImageState to AttributeValue
- `src/image.rs` - Add DeepImage, DeepLayer, DeepChannels types

**Key types:**
```rust
pub struct DeepSamples {
    sample_counts: Vec<u32>,     // Per-pixel sample counts
    samples: FlatSamples,         // Flat array of all samples
    resolution: Vec2<usize>,
}

pub struct DeepImage<Layers> {
    pub layer_data: Layers,
    pub attributes: ImageAttributes,
}

pub struct DeepLayer<Channels> {
    pub channel_data: Channels,
    pub attributes: LayerAttributes,
    pub size: Vec2<usize>,
    pub encoding: Encoding,
}
```

**Validation:**
- Must have Z channel (FLOAT)
- ZBack optional (FLOAT if present)
- Channels alphabetically sorted
- Sample counts non-negative

---

#### Phase 2: Block-Level I/O (Week 2-3)
**Files to modify:**
- `src/block/chunk.rs` - Complete DeepScanLineBlock/DeepTileBlock
- `src/block/reader.rs` - Add deep block decompression
- `src/block/writer.rs` - Add deep block compression

**Compression support (initial subset):**
- ✅ UNCOMPRESSED
- ✅ RLE
- ✅ ZIP1 / ZIPS
- ✅ ZIP16
- ❌ PIZ (defer to later)
- ❌ PXR24 (defer to later)
- ❌ B44/B44A (not supported for deep)

**Deep block format:**
```
[Pixel Offset Table - compressed]
  - Array of u64 packed offsets
  - Each offset points to start of pixel's samples

[Sample Data - compressed]
  - Interleaved by channel then by pixel
  - For each channel: all samples for all pixels
```

**Key functions:**
```rust
fn decompress_deep_block(
    compressed: &CompressedDeepScanLineBlock,
    header: &Header,
) -> Result<UncompressedDeepBlock>

fn compress_deep_block(
    uncompressed: &UncompressedDeepBlock,
    header: &Header,
) -> Result<CompressedDeepScanLineBlock>
```

---

#### Phase 3: High-Level Reading API (Week 4)
**Files to create:**
- `src/image/read/deep.rs` - Deep image reading

**Files to modify:**
- `src/image/read/mod.rs` - Add deep reading entry points

**API Design:**

**Single-phase (simple):**
```rust
#[cfg(feature = "deep-data")]
let image: DeepImage = read()
    .deep_data()
    .all_channels()
    .all_layers()
    .from_file(path)?;
```

**Two-phase (advanced):**
```rust
#[cfg(feature = "deep-data")]
let mut reader = read().deep_data().from_file(path)?;

// Phase 1: Read sample counts
let counts = reader.read_sample_counts()?;

// Inspect and allocate
let mut image = DeepImage::allocate_from_counts(&counts);

// Phase 2: Read sample data
reader.read_samples_into(&mut image)?;
```

**Reading process:**
1. Verify `header.deep == true` and `header.blocks` type
2. Read offset table for chunks
3. For each chunk:
   - Read compressed deep block
   - Decompress pixel offset table → extract sample counts
   - Decompress sample data
   - Populate DeepSamples arrays
4. Assemble into DeepImage

---

#### Phase 4: High-Level Writing API (Week 5)
**Files to create:**
- `src/image/write/deep.rs` - Deep image writing

**Files to modify:**
- `src/image/write/mod.rs` - Add deep writing entry points

**API Design:**
```rust
#[cfg(feature = "deep-data")]
DeepImage::new(layers)
    .write()
    .on_progress(|p| println!("{}%", p * 100.0))
    .to_file(path)?;
```

**Writing process:**
1. Validate deep image (Z channel, sample counts, etc.)
2. Set `Requirements.has_deep_data = true`
3. Set `Header.deep = true`, `Header.blocks` type
4. Write file header and metadata
5. For each chunk:
   - Extract sample counts → compute offset table
   - Compress offset table
   - Extract and interleave sample data
   - Compress sample data
   - Write compressed deep block
6. Update chunk offset table

---

#### Phase 5: Compositing Utilities (Week 6)
**Files to create:**
- `src/image/deep/compositing.rs` - Deep compositing operations
- `src/image/deep/mod.rs` - Deep utilities module

**Key operations:**
```rust
#[cfg(feature = "deep-data")]
pub fn flatten(deep: &DeepImage) -> Result<Image> {
    // Composite deep samples to flat image
}

pub fn make_tidy(deep: &mut DeepImage) -> Result<()> {
    // Sort and remove overlaps
}

pub fn composite_pixel(
    samples: &[(f32, f32, Color)],  // (Z, ZBack, Color+Alpha)
) -> Color {
    // Front-to-back "over" compositing
}
```

**Algorithms to implement:**
1. **Sort samples** by Z depth
2. **Split overlapping volumes** using formula: `α' = 1 - (1-α)^r`
3. **Merge coincident samples** using: `α_c = 1 - (1-α_a)(1-α_b)`
4. **Composite front-to-back** using "over" operator
5. **Update DeepImageState** attribute

---

#### Phase 6: Testing & Validation (Week 7-8)
**Files to create:**
- `tests/deep_roundtrip.rs` - Read/write round-trip tests
- `tests/deep_compatibility.rs` - OpenEXR C++ compatibility
- `examples/read_deep.rs` - Example: read and inspect
- `examples/write_deep.rs` - Example: create deep image
- `examples/flatten_deep.rs` - Example: composite to flat

**Test coverage:**
- ✅ Round-trip: write then read, verify data matches
- ✅ Sample storage: various pixel sample counts (0, 1, 100+)
- ✅ Block compression: each supported compression type
- ✅ Channel validation: Z required, ZBack optional
- ✅ Multi-layer: multiple deep layers in one file
- ✅ Compositing: verify against known results
- ✅ OpenEXR compatibility: read official test files
- ✅ Error cases: missing Z, negative counts, corrupt data

**Test files from OpenEXR:**
- User will provide links to official deep test images
- We'll verify we can read them correctly

---

#### Phase 7: Documentation (Week 9)
**Files to modify:**
- `README.md` - Add deep data feature section
- `CHANGELOG.md` - Document new feature
- All public APIs - Add comprehensive doc comments

**Documentation content:**
- What is deep data and when to use it
- Feature flag activation: `features = ["deep-data"]`
- API examples for reading and writing
- Compositing guide
- Limitations (no mip/rip, compression subset)
- Migration notes

---

## Technical Decisions

### 1. Sample Storage Design
Use **separate arrays** for sample counts and sample data:
```rust
// NOT Vec<Vec<Sample>> (fragmented, slow allocation)
// YES:
sample_counts: Vec<u32>        // [3, 0, 5, 2, ...]
samples: Vec<Sample>           // [s0,s1,s2, s0,s1,s2,s3,s4, ...]
```

**Indexing**: Compute offsets via cumulative sum of sample_counts.

### 2. API Consistency
Match existing exrs patterns:
- Builder API for configuration
- Trait-based extensibility
- Progress callbacks
- Parallel/non-parallel options

### 3. Type Safety
Separate types for deep vs flat:
- `Image<Layers>` - Flat images
- `DeepImage<Layers>` - Deep images
- Conversion via `flatten()`

### 4. Feature Flag
All deep code gated with:
```rust
#[cfg(feature = "deep-data")]
```

This allows:
- Zero overhead when feature disabled
- Clear opt-in for users
- Incremental testing during development

### 5. Compression Subset
Start with:
- UNCOMPRESSED (simplest, for testing)
- RLE (simple, good for validation)
- ZIP1/ZIPS (most common for deep data)
- ZIP16 (scanline variant)

Defer to later:
- PIZ (complex wavelet algorithm)
- PXR24 (f32→f24 conversion complexity)

### 6. No Mip/Rip Levels
OpenEXR spec doesn't define deep mipmaps, so:
- Deep images are always single-resolution
- `DeepLayer` has no `Levels` wrapper
- Simpler implementation

---

## Implementation Order

### Week 1: Foundation
- Create `deep_samples.rs` with storage and indexing
- Add `DeepImageState` attribute type
- Add feature flag to `Cargo.toml`
- Unit tests for sample storage

### Week 2: Block Reading
- Implement deep block decompression
- Support UNCOMPRESSED first
- Add RLE support
- Add ZIP support
- Unit tests for each compression type

### Week 3: Block Writing
- Implement deep block compression
- Offset table computation
- Sample data interleaving
- Round-trip tests

### Week 4: High-Level Reading
- Build reading API
- Integrate with block reader
- Sample count reading
- Full image reading
- Integration tests with real files

### Week 5: High-Level Writing
- Build writing API
- Integrate with block writer
- Header inference
- Validation
- Round-trip integration tests

### Week 6: Compositing
- Implement sorting
- Implement splitting
- Implement merging
- Implement flattening
- Algorithm correctness tests

### Week 7-8: Testing
- OpenEXR compatibility tests
- Edge case tests
- Performance profiling
- Example programs

### Week 9: Documentation
- API docs
- User guide
- Examples
- CHANGELOG

---

## Success Criteria

✅ Can read deep scanline images written by OpenEXR C++ library
✅ Can write deep scanline images readable by OpenEXR C++ library
✅ Can read deep tiled images written by OpenEXR C++ library
✅ Can write deep tiled images readable by OpenEXR C++ library
✅ Round-trip preserves all data exactly
✅ All supported compression types work
✅ Compositing produces correct results
✅ Feature flag properly gates all code
✅ Zero overhead when feature disabled
✅ Comprehensive test coverage (>90%)
✅ Clear documentation with examples

---

## Open Questions

1. **Memory limits**: Should we have a safety limit on max total samples per image?
2. **Streaming**: Should we support streaming large deep images (read chunks on demand)?
3. **Validation strictness**: How strict should channel validation be? (pedantic vs relaxed modes)
4. **Custom sample types**: Should users be able to provide custom deep sample storage?

These can be addressed during implementation as we encounter specific use cases.

---

## Next Steps

1. User provides links to OpenEXR deep test files
2. Begin Phase 1 implementation (core data structures)
3. Set up feature flag in Cargo.toml
4. Create initial test scaffolding

Ready to begin implementation upon approval!
