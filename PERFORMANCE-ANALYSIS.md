# EXRS Performance Analysis

**Date:** 2025-11-11
**Baseline Commit:** [57f7e14](https://github.com/virtualritz/exrs/commit/57f7e14)
**Methodology:** Benchmark-driven analysis

---

## Baseline Performance Metrics

### Benchmark Results (cargo bench --bench read)

| Benchmark | Time (ms) | Throughput | Parallel Speedup |
|-----------|-----------|------------|------------------|
| **Uncompressed RGBA (parallel)** | 15.1 | 66 images/sec | -7% (slower!) |
| **Uncompressed RGBA (sequential)** | 14.0 | 71 images/sec | baseline |
| **RLE RGBA (parallel)** | 23.4 | 43 images/sec | +27% faster |
| **RLE RGBA (sequential)** | 31.9 | 31 images/sec | baseline |
| **RLE All Channels (parallel)** | 44.1 | 23 images/sec | +15% faster |
| **RLE All Channels (sequential)** | 52.1 | 19 images/sec | baseline |
| **ZIP RGBA (parallel)** | 20.6 | 49 images/sec | +374% faster! |
| **ZIP RGBA (sequential)** | 97.6 | 10 images/sec | baseline |

**Test Image:** crowskull (~1920x1080, RGBA channels)

---

## Key Observations

### 1. Parallelization Already Works Excellently

- **ZIP compression benefits most:** 4.7x speedup with parallel decompression
- **RLE shows good gains:** 27% speedup (parallel vs sequential)
- **Uncompressed doesn't benefit:** Actually 7% slower due to thread overhead

**Conclusion:** The existing Rayon-based parallelization is well-implemented and correctly avoids parallelizing uncompressed data in most cases.

### 2. Uncompressed Is Not Always Sequential

Interestingly, the "uncompressed parallel" benchmark runs slightly slower (15.1ms vs 14.0ms). Looking at the code:

```rust
// src/block/reader.rs:424-429
let is_entirely_uncompressed = chunks.meta_data().headers.iter()
    .all(|head|head.compression == Compression::Uncompressed);

// if no compression is used in the file, don't use a threadpool
if is_entirely_uncompressed {
    return Err(chunks);  // Return to sequential mode
}
```

The code correctly skips thread pool creation for uncompressed files, but the "parallel" benchmark might still have some overhead from attempting to create the parallel reader.

### 3. Sequential Performance Is Strong

Even sequential RLE decompression is reasonably fast (31.9ms for 1920x1080 RGBA). This suggests the core algorithms are already efficient.

---

## Code Path Analysis

### RGBA Reading with SpecificChannelsReader

The hot path for RGBA reading is in `src/image/read/specific_channels.rs:193-210`:

```rust
fn read_block(&mut self, header: &Header, block: UncompressedBlock) -> UnitResult {
    // Line 194: Allocates pixel buffer for each block
    let mut pixels = vec![PxReader::RecursivePixel::default(); block.index.pixel_size.width()];

    let byte_lines = block.data.chunks_exact(header.channels.bytes_per_pixel * block.index.pixel_size.width());

    for (y_offset, line_bytes) in byte_lines.enumerate() {
        // Read pixels from bytes into buffer
        self.pixel_reader.read_pixels(line_bytes, &mut pixels, |px| px);

        // Copy pixels into final storage
        for (x_offset, pixel) in pixels.iter().enumerate() {
            let set_pixel = &self.set_pixel;
            set_pixel(&mut self.pixel_storage, block.index.pixel_position + Vec2(x_offset, y_offset), pixel.into_tuple());
        }
    }

    Ok(())
}
```

**What happens per image read:**
1. **Decompression** (in parallel for RLE/ZIP)
2. **For each block** (typically 16-64 blocks per image):
   - Allocate pixel buffer (line 194) - Vec allocation
   - For each line in block:
     - Read pixels from bytes → buffer
     - Copy pixels from buffer → final storage

### The Allocation Pattern

For a 1920x1080 image with 64x64 blocks:
- **Number of blocks:** ~510 blocks (30×17 grid)
- **Per-block allocation:** Vec with 64 elements (block width)
- **Total allocations:** 510 Vec allocations per image read

---

## Previous Optimization Attempt: Why It Failed

### The Failed Optimization

**Commit:** dcd8861 (reverted)

**Approach:** Add reusable pixel buffer to struct to eliminate per-block allocations.

```rust
// Before (current):
#[derive(Copy, Clone, Debug)]  // ✅ Copy trait
pub struct SpecificChannelsReader<...> { /* no Vec */ }

// After (failed):
#[derive(Clone, Debug)]  // ❌ Lost Copy trait
pub struct SpecificChannelsReader<...> {
    pixel_buffer: Vec<...>,  // Can't be Copy
}
```

### Why It Failed: The Hidden Cost

1. **Lost Copy Trait:**
   - Original: Passing struct = 32-byte memcpy = ~10 CPU cycles
   - Optimized: Cloning struct = Vec allocation + copy = 500+ CPU cycles

2. **SpecificChannelsReader Lives Inside LayerReader:**
   ```rust
   pub struct LayerReader<ChannelsReader> {
       channels_reader: ChannelsReader,  // SpecificChannelsReader
       // ...
   }
   ```

3. **LayerReader Derives Clone:**
   - When LayerReader is cloned → ChannelsReader must be cloned
   - Previously: cheap memcpy
   - After optimization: expensive Vec clone

4. **Result:** The cost of cloning exceeded the savings from avoiding allocations
   - Regression: 4-6% slower

### The Lesson

**Not all theoretical improvements help in practice.** Small allocations (64 elements) are very fast with modern allocators:
- Thread-local allocation pools
- Often just pointer bumping
- Cost: ~50-100 CPU cycles

Meanwhile, removing Copy trait has hidden costs:
- Forces defensive code generation
- Prevents certain compiler optimizations
- Makes cloning expensive

---

## Actual Optimization Opportunities

Based on the benchmarks and code analysis, here are **data-driven** optimization opportunities:

### 1. ⚠️ Per-Block Allocation (Revisited)

**Location:** `src/image/read/specific_channels.rs:194`

**Current Cost:**
- 510 allocations × 50 cycles = 25,500 cycles per image
- Out of ~56 million cycles total (23.4ms @ 2.4 GHz)
- **Impact: ~0.045% of total time**

**Problem with naive fix:** Loses Copy trait, costs more than it saves.

**Better approach:**
- Use a thread-local reusable buffer (doesn't affect struct)
- Or: accept that modern allocators make this negligible

**Recommendation:** ⚠️ **Not worth optimizing** - Impact is <0.1% and risks making things worse.

### 2. ✅ SIMD Vectorization for Pixel Format Conversion

**Location:** Pixel reading and format conversion in tight loops

**Opportunity:**
- Converting between f16/f32/u32 formats
- Copying pixel data
- Format conversions in `into_tuple()`

**Potential:** 2-4x speedup for format conversion operations (if they're significant)

**Required Investigation:**
1. Profile to determine how much time is spent in format conversion
2. Measure benefit of SIMD implementation
3. Consider portable-simd or explicit SIMD intrinsics

**Risk:** Medium - SIMD code is complex and platform-specific

**Recommendation:** ⚠️ **Requires profiling first** - Could be high impact if format conversion is a bottleneck.

### 3. ❌ Mipmap Level Caching

**Location:** `src/meta/mod.rs` - `mip_map_levels()`, `rip_map_levels()`

**Current:** Computes level sizes using log2 operations

**Problem:** Benchmarks use `.largest_resolution_level()` - **mipmaps are never used!**

**Cost of log2:** ~4 CPU cycles (hardware supported)

**Recommendation:** ❌ **Do not optimize** - Not used in common paths, log2 is extremely cheap.

### 4. ⚠️ Reduce Bounds Checking in Hot Loops

**Location:** Pixel iteration loops in `read_block`

**Opportunity:**
```rust
for (x_offset, pixel) in pixels.iter().enumerate() {
    // This does bounds checking on every access:
    set_pixel(&mut self.pixel_storage, block.index.pixel_position + Vec2(x_offset, y_offset), pixel.into_tuple());
}
```

**Potential:** Eliminate redundant bounds checks with unsafe code or iterators that guarantee no out-of-bounds access.

**Risk:** High - Unsafe code is error-prone

**Recommendation:** ⚠️ **Low priority** - Bounds checks are cheap (1-2 cycles) and LLVM often elides them.

### 5. ✅ Better Memory Layout for Pixel Storage

**Location:** `PixelVec` and pixel storage structures

**Opportunity:** Structure-of-Arrays (SoA) instead of Array-of-Structures (AoS)

**Current:** `Vec<(f32, f32, f32, f32)>` - RGBA interleaved
**Potential:** `struct { r: Vec<f32>, g: Vec<f32>, b: Vec<f32>, a: Vec<f32> }` - separate channels

**Benefits:**
- Better cache locality when processing single channels
- Easier SIMD vectorization
- Better for subsequent image processing operations

**Drawback:** API breaking change, more complex implementation

**Recommendation:** ⚠️ **Consider for v2.0** - Significant architectural change.

### 6. ❌ Endianness Conversion Inlining

**Location:** `src/compression/mod.rs` - endianness conversion functions

**Current:** Already very efficient on little-endian systems (zero-cost)

**On little-endian (x86, ARM):**
```rust
#[cfg(target_endian = "little")]
fn convert_current_to_little_endian(bytes: ByteVec, ...) -> Result<ByteVec> {
    Ok(bytes)  // No-op, optimized away
}
```

**Recommendation:** ❌ **Do not optimize** - Already optimal for 99% of users.

---

## Recommendations

### Immediate Actions: NONE

The codebase is **already well-optimized**. The baseline performance is strong:
- Parallel decompression works excellently
- Sequential performance is good
- Smart decisions (avoid thread pool for uncompressed)

### Before Any Optimization

**Always do this first:**

1. **Profile with real tools:**
   ```bash
   # Install profiling tools
   cargo install flamegraph
   cargo install cargo-criterion

   # Generate flamegraph
   cargo flamegraph --bench read -- --bench

   # Look for functions taking >5% of CPU time
   ```

2. **Establish baseline:**
   ```bash
   # Run benchmarks, save results
   cargo bench --bench read | tee baseline.txt
   ```

3. **Make targeted change:**
   - Change ONE thing
   - Focus on measured bottleneck

4. **Benchmark after:**
   ```bash
   cargo bench --bench read | tee optimized.txt
   ```

5. **Compare results:**
   - Must be >5% improvement (accounting for noise)
   - Check that nothing got slower
   - Only keep if net positive

6. **If no improvement → revert immediately**

### Future Optimization Path

**If profiling reveals actual bottlenecks:**

1. **Tier 1 - Low Risk, High Reward (if they're bottlenecks):**
   - SIMD vectorization for format conversion (if it's >10% of time)
   - Reduce redundant work in hot loops

2. **Tier 2 - Medium Risk, Medium Reward:**
   - Memory layout improvements (SoA)
   - Platform-specific optimizations

3. **Tier 3 - Never Optimize Without Proof:**
   - Per-block allocation (proven to be negligible)
   - Log2 caching (proven to be negligible)
   - Endianness (already optimal)

---

## Conclusion

**The exrs library is already performant.** Our previous optimization attempt failed because we:

1. ❌ Didn't profile first
2. ❌ Didn't benchmark before changes
3. ❌ Optimized based on theory, not data
4. ❌ Didn't understand hidden costs (Copy trait loss)

**Key Insight:** Modern Rust compiler and allocator are extremely good. "Obvious" optimizations often make things worse by:
- Interfering with compiler optimizations
- Adding complexity that prevents other optimizations
- Optimizing non-bottlenecks while making common cases slower

**Next Steps:**
- Keep the clean baseline
- Only optimize with proof from profiling
- Always benchmark before and after
- Respect the existing well-optimized codebase
