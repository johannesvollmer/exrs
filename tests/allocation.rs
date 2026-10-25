//! Damaged files must not make the reader reserve huge amounts of memory up
//! front. A separate test binary, because it installs a global allocator that
//! refuses any single allocation over 1 GiB, as a memory-constrained system
//! would.

use std::alloc::{GlobalAlloc, Layout, System};

use exr::prelude::*;

struct Capped;

const MAX_ALLOCATION: usize = 1 << 30;

unsafe impl GlobalAlloc for Capped {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() > MAX_ALLOCATION {
            return std::ptr::null_mut();
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Capped = Capped;

/// The DWA header claims 2^35 AC values; the decoder used to reserve room for
/// all of them.
#[test]
fn dwa_huge_ac_count_is_an_error() {
    let result = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .non_parallel()
        .from_file("tests/images/fuzzed/dwa_huffman_huge_ac_count.exr");

    assert!(result.is_err());
}
