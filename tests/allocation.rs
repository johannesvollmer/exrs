//! Damaged files must not make the reader reserve huge amounts of memory up
//! front. A separate test binary, because it installs a global allocator that
//! refuses any single allocation over 1 GiB, as a memory-constrained system
//! would.

use std::alloc::{GlobalAlloc, Layout, System};

use exr::{image::pixel_vec::PixelVec, prelude::*};

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

/// Headers claiming 654 million lines, where the file ends before the offset
/// table does. Only the file's own offsets tell it apart from a real image.
#[test]
fn huge_height_is_an_error() {
    assert_flat_and_rgba_reads_fail("tests/images/fuzzed/huge_height.exr");
}

/// A single 2^29 pixel line, which has a valid one-entry offset table: 8 GiB
/// of RGBA samples.
#[test]
fn huge_width_is_an_error() {
    assert_flat_and_rgba_reads_fail("tests/images/fuzzed/huge_width.exr");
}

/// The image would fit into memory, so only the missing offset table can
/// refuse it, and that must happen before the user's storage is created.
#[test]
fn truncated_offset_table_is_found_before_allocating() {
    let result = read_first_rgba_layer_from_file(
        "tests/images/fuzzed/truncated_offset_table.exr",
        |_, _| -> PixelVec<(f32, f32, f32, f32)> { panic!("allocated for a truncated file") },
        PixelVec::set_pixel,
    );
    assert!(result.is_err());
}

fn assert_flat_and_rgba_reads_fail(path: &str) {
    let flat = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .non_parallel()
        .from_file(path);
    assert!(flat.is_err());

    let rgba = read_first_rgba_layer_from_file(
        path,
        PixelVec::<(f32, f32, f32, f32)>::constructor,
        PixelVec::set_pixel,
    );
    assert!(rgba.is_err());
}
