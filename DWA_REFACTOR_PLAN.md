# DWA Decoder Refactoring Plan

## Current Status (after zip_reconstruct_bytes + toLinear LUT)

### ✅ Completed
1. `zip_reconstruct_bytes` - DC data now properly decoded with byte-delta + interleaving
2. Exact u16->u16 toLinear LUT - matches OpenEXR bit-exact, no double rounding
3. Code compiles and runs

### Test Results
- **0 passed, 6 failed** (all tests)
- DCT channels produce non-zero spatial values ✅
- toLinear LUT applied correctly (debug shows u16->u16 transform) ✅
- **RLE channels broken** - outputting 0 instead of 1.0 ❌

## Root Cause: Architectural Mismatch

### Our Current Approach (WRONG)
```
for each channel:
    decode all data for channel
    write all channel data to output
```

### OpenEXR Approach (CORRECT)
```
for each block_row:
    for each block in row:
        for each channel:
            decode 8x8 block -> rowBlock[channel][blockx*64]

    for each scanline in block_row:
        for each channel:
            write scanline[y] from rowBlock to output
```

## Required Changes

### 1. DC Plane Organization
**Current:** Read DC sequentially from single stream
**Required:** Separate DC into per-channel planes

```rust
// OpenEXR: internal_dwa_decoder.h:339-341
currDcComp[0] = packedDc;
for comp in 1..numComp:
    currDcComp[comp] = currDcComp[comp-1] + numBlocksX * numBlocksY;
```

### 2. Block-Row Processing Loop
**Required structure:**
```rust
let num_blocks_x = (width + 7) / 8;
let num_blocks_y = (height + 7) / 8;

// Allocate rowBlock: temp buffer for one row of 8x8 blocks (as f16)
let mut row_blocks: Vec<Vec<[u16; 64]>> = ...;

for block_y in 0..num_blocks_y {
    // Decode all blocks in this row to rowBlock
    for block_x in 0..num_blocks_x {
        for channel in lossy_dct_channels {
            let dc = currDcComp[channel][(block_y * num_blocks_x + block_x)];
            let ac = read_ac_block(&mut ac_cursor)?;

            // Decode to dctData (f32), convert to f16 nonlinear
            dct_decode(...);
            row_blocks[channel][block_x] = spatial_block_as_f16;
        }
    }

    // Write scanlines for this block row
    for y in (block_y*8)..min((block_y+1)*8, height) {
        for channel in all_channels {
            match channel.scheme {
                LossyDct => write_from_rowBlock_with_toLinear_LUT(...),
                Rle => write_from_rle_cursor(...),
                Unknown => write_from_unknown_cursor(...),
            }
        }
    }
}
```

### 3. Per-Channel State Tracking
Create structures similar to PIZ's channel handling:

```rust
struct ChannelDecodeState {
    scheme: CompressionScheme,
    sample_type: SampleType,
    width: usize,
    height: usize,
    x_sampling: usize,
    y_sampling: usize,
    bytes_per_sample: usize,

    // Cursors for RLE/Unknown data
    rle_cursor: usize,
    unknown_cursor: usize,
}
```

### 4. RLE Data Handling
**Current:** Copy entire channel at once
**Required:** Advance cursor row-by-row accounting for subsampling

```rust
if (y % channel.y_sampling) == 0 {
    let row_width = channel.width;  // accounts for x_sampling
    let row_bytes = row_width * bytes_per_sample;
    output[out_pos..out_pos+row_bytes]
        .copy_from_slice(&rle_data[rle_cursor..rle_cursor+row_bytes]);
    rle_cursor += row_bytes;
}
```

## Implementation Order

1. **Create channel state structures** (similar to PIZ)
2. **Reorganize DC plane reading** into per-channel arrays
3. **Implement block-row loop** with rowBlock buffer
4. **Implement per-scanline write loop** with RLE/Unknown cursors
5. **Apply toLinear LUT** during rowBlock->output copy (F16 channels)
6. **Test and debug** with existing test cases

## Expected Outcome
After this refactor, the decoder will match OpenEXR's exact pipeline:
- ✅ DC organized into per-channel planes
- ✅ Block-by-block decode to rowBlock temp buffer
- ✅ Row-by-row output with proper RLE/Unknown/LossyDct interleaving
- ✅ toLinear LUT applied bit-exact during F16 write
- ✅ All 6 tests passing
