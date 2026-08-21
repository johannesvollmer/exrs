//! Per-stage timing for DWA decode, behind the `dwa-profile` feature.
//!
//! Stages mirror OpenEXR's `DwaCompressor_uncompress` for comparison.
//! Atomics so parallel (rayon) decode can accumulate; under parallel decode
//! totals are summed CPU time across threads, not wall time.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

pub static UNKNOWN_NS: AtomicU64 = AtomicU64::new(0);
pub static AC_NS: AtomicU64 = AtomicU64::new(0);
pub static DC_NS: AtomicU64 = AtomicU64::new(0);
pub static RLE_NS: AtomicU64 = AtomicU64::new(0);
/// RLE sub-stages (inflate / alloc / unpack), matching OpenEXR's timed t3
/// block.
pub static RLE_INFLATE_NS: AtomicU64 = AtomicU64::new(0);
pub static RLE_ALLOC_NS: AtomicU64 = AtomicU64::new(0);
pub static RLE_UNPACK_NS: AtomicU64 = AtomicU64::new(0);
pub static DCT_NS: AtomicU64 = AtomicU64::new(0);
/// AVX-512 fused RGB pair steps that entered `decode_pair_dct_csc` (components
/// == 3).
pub static DCT_RGB_PAIR_STEPS: AtomicU64 = AtomicU64::new(0);
/// Of those, how many used component-level `inverse_quad`.
pub static DCT_RGB_COMP_QUAD: AtomicU64 = AtomicU64::new(0);
pub static ASSEMBLE_NS: AtomicU64 = AtomicU64::new(0);
/// Whole-`decompress` time, and the output buffer allocation inside it.
pub static TOTAL_NS: AtomicU64 = AtomicU64::new(0);
pub static OUT_ALLOC_NS: AtomicU64 = AtomicU64::new(0);
/// Total output bytes allocated (reported as MiB/iter alongside
/// `OUT_ALLOC_NS`).
pub static OUT_BYTES: AtomicU64 = AtomicU64::new(0);

// Block-scheduling / thread-wait counters.
//
// Two topologies stall differently:
// - `ParallelBlockDecompressor`: main thread feeds + `recv()`s →
//   `SCHED_RECV_NS`.
// - `collect_pixels_in_parallel`: feeder runs on a pool worker; after the last
//   spawn it work-steals until the scope closes → `SCHED_DRAIN_NS`.

/// Chunks handed to the thread pool.
pub static SCHED_TASKS: AtomicU64 = AtomicU64::new(0);
/// Sum over tasks of (worker start - `spawn`): queueing delay.
pub static SCHED_QUEUE_NS: AtomicU64 = AtomicU64::new(0);
/// Sum over tasks of `decompress_chunk` duration: worker busy time.
pub static SCHED_TASK_NS: AtomicU64 = AtomicU64::new(0);
/// Main-thread time spent reading compressed chunks and spawning them.
pub static SCHED_FEED_NS: AtomicU64 = AtomicU64::new(0);
/// Main-thread time blocked in `recv()` (channel-based path only).
pub static SCHED_RECV_NS: AtomicU64 = AtomicU64::new(0);
/// Rayon-scope path: time from last `spawn` until the scope closed.
/// Not idle: the feeder work-steals; this is the load-imbalance tail.
pub static SCHED_DRAIN_NS: AtomicU64 = AtomicU64::new(0);
/// Wall-clock span of the whole parallel decompression.
pub static SCHED_WALL_NS: AtomicU64 = AtomicU64::new(0);
/// Worker threads in the pool (utilization denominator).
pub static SCHED_THREADS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
pub struct Timer(Instant);

pub fn start() -> Timer {
    Timer(Instant::now())
}

impl Timer {
    pub fn stop(self, counter: &AtomicU64) {
        counter.fetch_add(self.0.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }
}

pub fn reset() {
    for counter in [
        &UNKNOWN_NS,
        &AC_NS,
        &DC_NS,
        &RLE_NS,
        &RLE_INFLATE_NS,
        &RLE_ALLOC_NS,
        &RLE_UNPACK_NS,
        &DCT_NS,
        &DCT_RGB_PAIR_STEPS,
        &DCT_RGB_COMP_QUAD,
        &ASSEMBLE_NS,
        &TOTAL_NS,
        &OUT_ALLOC_NS,
        &OUT_BYTES,
        &SCHED_TASKS,
        &SCHED_QUEUE_NS,
        &SCHED_TASK_NS,
        &SCHED_FEED_NS,
        &SCHED_RECV_NS,
        &SCHED_DRAIN_NS,
        &SCHED_WALL_NS,
        &SCHED_THREADS,
    ] {
        counter.store(0, Ordering::Relaxed);
    }
}

/// Print accumulated totals divided by `divisor` (e.g. iteration count), in ms.
pub fn report(divisor: u64) {
    let ms = |counter: &AtomicU64| {
        counter.load(Ordering::Relaxed) as f64 / divisor.max(1) as f64 / 1_000_000.0
    };
    eprintln!(
        "dwa-profile (avg ms/iter): unknown={:.3} ac_huffman={:.3} dc={:.3} rle={:.3} (rle_inflate={:.3} rle_alloc={:.3} rle_unpack={:.3}) lossy_dct={:.3} assemble={:.3} out_alloc={:.3} total={:.3}",
        ms(&UNKNOWN_NS),
        ms(&AC_NS),
        ms(&DC_NS),
        ms(&RLE_NS),
        ms(&RLE_INFLATE_NS),
        ms(&RLE_ALLOC_NS),
        ms(&RLE_UNPACK_NS),
        ms(&DCT_NS),
        ms(&ASSEMBLE_NS),
        ms(&OUT_ALLOC_NS),
        ms(&TOTAL_NS),
    );
    eprintln!(
        "dwa-profile: out_alloc volume = {:.1} MiB/iter",
        OUT_BYTES.load(Ordering::Relaxed) as f64 / divisor.max(1) as f64 / (1024.0 * 1024.0),
    );

    // Parallel-read only; serial paths leave these at zero.
    let tasks = SCHED_TASKS.load(Ordering::Relaxed);
    if tasks > 0 {
        let threads = SCHED_THREADS.load(Ordering::Relaxed).max(1);
        let wall_ns = SCHED_WALL_NS.load(Ordering::Relaxed) as f64 / divisor.max(1) as f64;
        let busy_ns = SCHED_TASK_NS.load(Ordering::Relaxed) as f64 / divisor.max(1) as f64;
        let feed_ns = SCHED_FEED_NS.load(Ordering::Relaxed) as f64 / divisor.max(1) as f64;
        let wait_ns = SCHED_RECV_NS.load(Ordering::Relaxed) as f64 / divisor.max(1) as f64;
        let drain_ns = SCHED_DRAIN_NS.load(Ordering::Relaxed) as f64 / divisor.max(1) as f64;
        let queue_ns = SCHED_QUEUE_NS.load(Ordering::Relaxed) as f64 / divisor.max(1) as f64;
        let capacity_ns = wall_ns * threads as f64;
        // Feeder runs on a pool worker (`ThreadPool::scope`), so feed time is
        // occupied, not idle. Leftover capacity is unused core-time.
        let idle_ns = (capacity_ns - busy_ns - feed_ns).max(0.0);

        eprintln!(
            "dwa-profile sched (avg ms/iter): wall={:.3} threads={} worker_busy={:.3} feed={:.3} idle={:.3} util={:.1}% recv_wait={:.3} drain={:.3} ({:.1}% of wall) queue_delay={:.3} tasks={:.0} queue_delay_per_task_us={:.2}",
            wall_ns / 1e6,
            threads,
            busy_ns / 1e6,
            feed_ns / 1e6,
            idle_ns / 1e6,
            if capacity_ns > 0.0 {
                100.0 * (busy_ns + feed_ns) / capacity_ns
            } else {
                0.0
            },
            wait_ns / 1e6,
            drain_ns / 1e6,
            if wall_ns > 0.0 {
                100.0 * drain_ns / wall_ns
            } else {
                0.0
            },
            queue_ns / 1e6,
            tasks as f64 / divisor.max(1) as f64,
            queue_ns * divisor.max(1) as f64 / tasks as f64 / 1e3,
        );
    }

    let pair_steps = DCT_RGB_PAIR_STEPS.load(Ordering::Relaxed);
    let comp_quad = DCT_RGB_COMP_QUAD.load(Ordering::Relaxed);
    if pair_steps > 0 {
        eprintln!(
            "dwa-profile: dct rgb pair steps={}  component-quad hits={}  ({:.1}% of pair steps)",
            pair_steps as f64 / divisor.max(1) as f64,
            comp_quad as f64 / divisor.max(1) as f64,
            100.0 * comp_quad as f64 / pair_steps as f64,
        );
    }
}
