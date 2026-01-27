# Unwrap Safety Audit

This document catalogs all `unwrap()` calls in the codebase and documents their safety.

## Summary

- **Total unwraps found**: 87 across 15 files
- **Test code**: ~50 unwraps (safe - tests are expected to panic on failure)
- **Documented**: 3 unwraps with safety comments
- **Safe by design**: ~30 unwraps (type conversions, invariants)
- **Needs review**: 4 unwraps requiring attention

## Categorized Findings

### Category 1: Test Code (Safe ✅)

All unwraps in test code are acceptable as tests should panic on failure.

**Files with test-only unwraps:**
- `src/io.rs:526-534` - PeekRead tests
- `src/meta/mod.rs:663-664, 718-720, 753-754, 762, 813` - Metadata round-trip tests
- `src/meta/attribute.rs:2099-2259` - Attribute serialization tests
- `src/compression/mod.rs:689, 693` - Compression tests
- `src/compression/piz/wavelet.rs:315-387` - Wavelet transform tests
- `src/compression/piz/huffman.rs:937-981` - Huffman coding tests
- `src/compression/piz/mod.rs:310-311` - PIZ compression tests
- `src/compression/b44/mod.rs:722-985` - B44 compression tests

### Category 2: Already Documented (Safe ✅)

**src/io.rs**
- Line 67: `self.file.as_mut().unwrap()` - Comment: "will not be reached if creation fails"
- Line 112: `self.peeked.as_ref().unwrap()` - Comment: "unwrap cannot fail because we just set it"
- Line 131: `self.peeked.take().unwrap().err().unwrap()` - Comment: "unwrap is safe because this branch cannot be reached otherwise"

### Category 3: Safe Type Conversions (Needs Documentation ⚠️)

**Always Safe Conversions (usize → u64):**
- `src/io.rs:21` - `u64::try_from(count).unwrap()` where count is usize
- `src/io.rs:232` - `u64::try_from(target_position).unwrap()`
- `src/io.rs:246` - `u64::try_from(target_position).unwrap()`
- `src/io.rs:250` - `u64::try_from(target_position - self.position).unwrap()`

**Rationale:** On all supported platforms, `usize` is at most 64 bits (pointer-sized), so conversion to `u64` cannot fail.

**Bounded Value Conversions:**
- `src/compression/piz/huffman.rs:307` - `usize::try_from(zerun_bits + SHORTEST_LONG_RUN).unwrap()`
  - zerun_bits is 8-bit value, so result fits in usize
- `src/compression/piz/huffman.rs:320` - `usize::try_from(code_len - SHORT_ZEROCODE_RUN + 2).unwrap()`
  - code_len is bounded by protocol, result fits in usize

### Category 4: Collection Access with Invariants (Safe ✅)

**src/compression/mod.rs**
- Lines 578, 615: `last_mut().unwrap()` - Called after pushing elements, so collection is non-empty

**src/compression/piz/huffman.rs**
- Line 391: `out.last().unwrap()` - Called after ensuring out has repeated_code elements

**src/compression/b44/mod.rs**
- Line 56: `t.iter().max().unwrap()` - Called on fixed-size constant array, never empty

**src/compression/piz/mod.rs**
- Line 140: `channel_data.last().unwrap()` - In debug_assert, used to verify channel data consistency

### Category 5: Needs Review ❌

**1. src/meta/mod.rs:252 - compute_level_count()**
```rust
usize::try_from(round.log2(u32::try_from(full_res).unwrap())).unwrap() + 1
```
**Issues:**
- First unwrap: Converting `full_res: usize` to `u32` - Could fail if image dimension > 4,294,967,295 pixels
- Second unwrap: Converting log2 result back to usize - Should always succeed (log2 of u32 is at most 32)

**Assessment:** First unwrap is **potentially unsafe**. While no real image would have such dimensions, the code should handle this gracefully.

**Recommendation:** Add validation or return Result:
```rust
pub fn compute_level_count(round: RoundingMode, full_res: usize) -> Result<usize> {
    let full_res_u32 = u32::try_from(full_res)
        .map_err(|_| Error::invalid("image dimension too large for mipmap computation"))?;
    Ok(usize::try_from(round.log2(full_res_u32)).unwrap() + 1)
}
```

**2. src/compression/piz/huffman.rs:252 - unpack_code_lengths()**
```rust
let code_index = u32::try_from(code_index).unwrap();
```
**Context:** `code_index` comes from `enumerate()` over encoding_table[..= max_code_index]

**Assessment:** On 64-bit systems, if max_code_index >= u32::MAX, this could fail.

**Recommendation:** The encoding_table should be bounded by protocol constraints. Add a check:
```rust
let code_index = u32::try_from(code_index)
    .map_err(|_| Error::invalid("code index exceeds u32::MAX in huffman encoding"))?;
```

**3. src/compression/piz/mod.rs:265, 270, 278, 283 - usize_to_u16 conversions**
```rust
*entry = usize_to_u16(count, "piz entry").unwrap();
let max_value = usize_to_u16(table.len() - 1, "table size").unwrap();
```
**Assessment:** These conversions assume PIZ bitmap/table sizes fit in u16 (< 65,536). This is likely guaranteed by PIZ protocol constraints but should be validated.

**Recommendation:** Since `usize_to_u16` already returns Result, these unwraps should be replaced with proper error propagation using `?` operator.

## Recommendations

### High Priority
1. **Fix meta/mod.rs:252** - Change `compute_level_count` to return Result and handle u32 conversion failure
2. **Fix piz/mod.rs unwraps** - Replace `usize_to_u16(...).unwrap()` with `?` operator for proper error propagation

### Medium Priority
3. **Document io.rs conversions** - Add comments explaining why usize→u64 is always safe
4. **Fix huffman.rs:252** - Add error handling for u32 conversion or document protocol constraint

### Low Priority
5. **Add invariant comments** - Document why collection.last().unwrap() is safe in each location

## Conclusion

The majority of unwraps (85 out of 87) are either in test code or have clear safety guarantees. The two critical issues are:
1. Potential u32 overflow in mipmap level computation
2. Missing error propagation in PIZ compression

Neither issue is likely to occur with real-world images, but proper error handling would make the library more robust.
