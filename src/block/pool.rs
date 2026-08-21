//! Reuse of `Vec<u8>` buffers for compressed chunks and decompressed blocks.
//!
//! Keeping a buffer and recycling it is cheaper than allocating a fresh one
//! for every chunk. Take with `take_zeroed` / `take_with_capacity`, give back
//! with `recycle`. Skipping `recycle` just drops the buffer.

use std::sync::{
    Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
};

/// Cap on pooled buffers: one per thread of the default decompress pool.
/// `ParallelBlockDecompressor` keeps at most `max_threads` blocks in flight.
const MAX_POOLED_BUFFERS: usize = 8;

/// Cap on retained bytes. Large tiles give up some reuse rather than keep more.
const MAX_POOLED_BYTES: usize = 64 * 1024 * 1024;

/// Process-wide cache behind the free functions; tests use a private instance.
struct Pool {
    buffers: Mutex<Vec<Vec<u8>>>,
    is_used: AtomicBool,
}

impl Pool {
    const fn new() -> Self {
        Self {
            buffers: Mutex::new(Vec::new()),
            is_used: AtomicBool::new(false),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Vec<u8>>> {
        // Do not poison the pool on panic; a half-updated pool only costs a missed reuse.
        self.buffers.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn take_zeroed(&self, size: usize) -> Vec<u8> {
        self.is_used.store(true, Ordering::Relaxed);

        match self.take_best_fit(size) {
            // Capacity is already enough, so resize zeroes without reallocating.
            Some(mut buffer) => {
                buffer.clear();
                buffer.resize(size, 0);
                buffer
            }
            None => vec![0; size],
        }
    }

    fn take_with_capacity(&self, min_capacity: usize) -> Vec<u8> {
        self.is_used.store(true, Ordering::Relaxed);

        // Recycled buffers are always cleared, so no `clear()` here.
        self.take_best_fit(min_capacity).unwrap_or_else(|| Vec::with_capacity(min_capacity))
    }

    /// Smallest pooled buffer with capacity >= `min_capacity` (best-fit).
    fn take_best_fit(&self, min_capacity: usize) -> Option<Vec<u8>> {
        let mut pool = self.lock();
        let index = pool
            .iter()
            .enumerate()
            .filter(|(_, buffer)| buffer.capacity() >= min_capacity)
            .min_by_key(|(_, buffer)| buffer.capacity())
            .map(|(index, _)| index)?;
        Some(pool.swap_remove(index))
    }

    fn recycle(&self, mut buffer: Vec<u8>) {
        if buffer.capacity() == 0 || !self.is_used.load(Ordering::Relaxed) {
            return;
        }

        let mut pool = self.lock();
        let pooled_bytes: usize = pool.iter().map(Vec::capacity).sum();

        if pool.len() < MAX_POOLED_BUFFERS && pooled_bytes + buffer.capacity() <= MAX_POOLED_BYTES {
            buffer.clear();
            pool.push(buffer);
        }
    }

    fn release(&self) {
        self.lock().clear();
    }
}

static POOL: Pool = Pool::new();

/// Zeroed buffer of exactly `size` bytes. Reuses a pooled allocation when possible.
pub fn take_zeroed(size: usize) -> Vec<u8> {
    POOL.take_zeroed(size)
}

/// Empty buffer with at least `min_capacity`. Prefer this over `take_zeroed`
/// when the caller fills the buffer itself (`write_all`, `extend_from_slice`).
pub fn take_with_capacity(min_capacity: usize) -> Vec<u8> {
    POOL.take_with_capacity(min_capacity)
}

/// Offer a finished buffer back instead of dropping it. Oversized buffers are dropped.
pub fn recycle(buffer: Vec<u8>) {
    POOL.recycle(buffer)
}

/// Drop every retained buffer.
pub fn release() {
    POOL.release()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn recycles_zeroed_buffers_within_bounds() {
        let pool = Pool::new();

        pool.recycle(vec![0; 1024]);
        assert_eq!(pool.lock().len(), 0, "unused pool must not retain buffers");

        let mut buffer = pool.take_zeroed(1024);
        assert_eq!(buffer.len(), 1024);
        assert!(buffer.iter().all(|&byte| byte == 0));

        buffer.fill(0xAB);
        let address = buffer.as_ptr() as usize;
        pool.recycle(buffer);

        let smaller = pool.take_zeroed(512);
        assert_eq!(smaller.len(), 512);
        assert!(smaller.iter().all(|&byte| byte == 0), "reused buffer was not zeroed");
        assert_eq!(smaller.as_ptr() as usize, address, "buffer was not reused");
        drop(smaller);

        pool.release();
        for _ in 0..MAX_POOLED_BUFFERS * 2 {
            pool.recycle(vec![0; 1024]);
        }
        assert_eq!(pool.lock().len(), MAX_POOLED_BUFFERS);

        pool.release();
        pool.recycle(vec![0; MAX_POOLED_BYTES + 1]);
        assert_eq!(pool.lock().len(), 0, "buffer over the byte limit must not be retained");
    }

    #[test]
    fn take_prefers_smallest_sufficient_buffer() {
        let pool = Pool::new();
        pool.is_used.store(true, Ordering::Relaxed);

        let large = vec![0u8; 64 * 1024];
        let small = vec![0u8; 1024];
        let large_addr = large.as_ptr() as usize;
        let small_addr = small.as_ptr() as usize;
        pool.recycle(large);
        pool.recycle(small);

        let taken_small = pool.take_with_capacity(512);
        assert_eq!(taken_small.as_ptr() as usize, small_addr, "small take stole the large buffer");
        assert!(taken_small.capacity() < 64 * 1024);

        let taken_large = pool.take_zeroed(32 * 1024);
        assert_eq!(taken_large.as_ptr() as usize, large_addr, "large take missed the large buffer");
        assert_eq!(taken_large.len(), 32 * 1024);
        assert!(taken_large.iter().all(|&byte| byte == 0));
    }
}
