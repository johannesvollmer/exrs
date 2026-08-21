//! Pinned-friendly A/B for the Zen5 dual-port *component* idea:
//!
//! - **seq**: 3× `inverse_pair` (R then G then B) -> today's fused RGB DCT
//!   shape
//! - **quad**: `inverse_quad(R,G)` + `inverse_pair(B)` — dual-port without
//!   growing the spatial working set past one pair
//!
//! No zigzag / CSC / write. Decision gate from the dual-port investigation:
//! if quad is not clearly faster here (~≥3–4%), do not wire into dispatch.
//!
//! ```text
//! taskset -c 2 cargo run --release --features simd-benches --example dct_rgb_comp_quad_bench
//! ```

#![cfg(any(target_arch = "x86", target_arch = "x86_64"))]

#[cfg(not(feature = "simd-benches"))]
fn main() {
    eprintln!("this example requires the `simd-benches` feature");
}

#[cfg(feature = "simd-benches")]
mod bench {
    use std::time::Instant;

    use exr::compression::dwa::discrete_cosine_transform::{test, x86::avx512dq as avx512};
    use miraculix::x86::{
        detect_features,
        ops::{
            avx::avx::Avx,
            avx512::{avx512dq::Avx512Dq, avx512f::Avx512f},
        },
    };

    const PAIR_COUNT: usize = 2048;
    const WARMUP: usize = 32;
    const RUNS: usize = 61; // odd -> clean median
    const INNER: usize = 4; // invert same buffer INNER times per sample (amortize noise)

    #[derive(Clone, Copy)]
    struct Tokens {
        avx512f: Avx512f,
        avx: Avx,
        avx512dq: Avx512Dq,
    }

    pub(super) fn main() {
        let features = detect_features();
        let tokens = Tokens {
            avx512f: Avx512f::from_features(features)
                .expect("host needs AVX-512F for this microbench"),
            avx: Avx::from_features(features).expect("host needs AVX for this microbench"),
            avx512dq: Avx512Dq::from_features(features)
                .expect("host needs AVX-512DQ for this microbench"),
        };
        let base = make_pairs(PAIR_COUNT);

        // Correctness smoke: one pass each, compare.
        {
            let mut seq = base.clone();
            let mut quad = base.clone();
            avx512::dct_inverse_rgb_pair_components_seq(
                tokens.avx512f,
                tokens.avx,
                tokens.avx512dq,
                &mut seq,
            );
            avx512::dct_inverse_rgb_pair_components_quad(
                tokens.avx512f,
                tokens.avx,
                tokens.avx512dq,
                &mut quad,
            );
            for (s, q) in seq.iter().zip(quad.iter()) {
                for c in 0..3 {
                    assert_eq!(s.0[c], q.0[c], "component {c} block A mismatch");
                    assert_eq!(s.1[c], q.1[c], "component {c} block B mismatch");
                }
            }
            eprintln!("correctness: seq == quad (bit-exact on this input)");
        }

        // Two independent working sets so each mode always starts from identical
        // cold-for-kernel / hot-for-data state without clone_from in the timed path.
        let mut seq_pairs = base.clone();
        let mut quad_pairs = base.clone();

        for _ in 0..WARMUP {
            avx512::dct_inverse_rgb_pair_components_seq(
                tokens.avx512f,
                tokens.avx,
                tokens.avx512dq,
                &mut seq_pairs,
            );
            avx512::dct_inverse_rgb_pair_components_quad(
                tokens.avx512f,
                tokens.avx,
                tokens.avx512dq,
                &mut quad_pairs,
            );
        }

        // Reset to base so arithmetic stays in a normal float range after warmup
        // (repeated inverse drifts magnitudes but throughput should be similar).
        seq_pairs.clone_from(&base);
        quad_pairs.clone_from(&base);

        let mut seq_ns = Vec::with_capacity(RUNS);
        let mut quad_ns = Vec::with_capacity(RUNS);
        for i in 0..RUNS {
            // Alternate order; each sample times INNER back-to-back kernel calls
            // on an already-resident buffer (no clone inside the stopwatch).
            if i % 2 == 0 {
                seq_ns.push(time_kernel(tokens, &mut seq_pairs, Mode::Seq));
                quad_ns.push(time_kernel(tokens, &mut quad_pairs, Mode::Quad));
            } else {
                quad_ns.push(time_kernel(tokens, &mut quad_pairs, Mode::Quad));
                seq_ns.push(time_kernel(tokens, &mut seq_pairs, Mode::Seq));
            }
        }

        // Per-pass ns (sample was INNER kernel invocations).
        for v in &mut seq_ns {
            *v /= INNER as f64;
        }
        for v in &mut quad_ns {
            *v /= INNER as f64;
        }

        let seq_stats = stats(&mut seq_ns);
        let quad_stats = stats(&mut quad_ns);
        // p10 ≈ steady-state floor on a noisy desktop; p50 can be wrecked by
        // boost/thermal outliers (seen p90/p10 up to ~4× on this host).
        let speedup_floor = seq_stats.p10 / quad_stats.p10;
        let speedup_med = seq_stats.p50 / quad_stats.p50;
        let block_count = (PAIR_COUNT * 6) as f64;

        println!("dct RGB component dual-port microbench (AVX-512)");
        println!("  pairs/pass: {PAIR_COUNT}  (={block_count} single-block iDCTs)");
        println!("  samples: {RUNS} × INNER={INNER} (interleaved order, no clone in timer)");
        println!(
            "  seq  p10/p50/p90: {:8.0} / {:8.0} / {:8.0} ns   ({:.2} ns/block @p10)",
            seq_stats.p10,
            seq_stats.p50,
            seq_stats.p90,
            seq_stats.p10 / block_count
        );
        println!(
            "  quad p10/p50/p90: {:8.0} / {:8.0} / {:8.0} ns   ({:.2} ns/block @p10)",
            quad_stats.p10,
            quad_stats.p50,
            quad_stats.p90,
            quad_stats.p10 / block_count
        );
        println!(
            "  quad vs seq @p10 (floor): {:.3}×  ({:+.2}% wall)",
            speedup_floor,
            (speedup_floor - 1.0) * 100.0
        );
        println!(
            "  quad vs seq @p50 (median): {:.3}×  ({:+.2}% wall)",
            speedup_med,
            (speedup_med - 1.0) * 100.0
        );
        println!(
            "  spread (p90/p10): seq {:.3}×  quad {:.3}×",
            seq_stats.p90 / seq_stats.p10,
            quad_stats.p90 / quad_stats.p10
        );
        // Gate on floor: if the best steady-state of each side still shows a win,
        // the dual-port idea is real. Median is advisory when the host is noisy.
        if speedup_floor >= 1.04 {
            println!("  gate: clear floor win (≥4% @p10) — worth a fused-path wiring trial");
        } else if speedup_floor >= 1.02 {
            println!("  gate: marginal floor (2–4%) — whole-pipeline A/B only if curious");
        } else {
            println!("  gate: no clear floor win — do not wire; OoO already dual-issues");
        }
    }

    #[derive(Clone, Copy)]
    enum Mode {
        Seq,
        Quad,
    }

    fn time_kernel(tokens: Tokens, pairs: &mut [avx512::RgbPairBlocks], mode: Mode) -> f64 {
        let t0 = Instant::now();
        for _ in 0..INNER {
            match mode {
                Mode::Seq => avx512::dct_inverse_rgb_pair_components_seq(
                    tokens.avx512f,
                    tokens.avx,
                    tokens.avx512dq,
                    pairs,
                ),
                Mode::Quad => avx512::dct_inverse_rgb_pair_components_quad(
                    tokens.avx512f,
                    tokens.avx,
                    tokens.avx512dq,
                    pairs,
                ),
            }
            std::hint::black_box(&*pairs);
        }
        t0.elapsed().as_secs_f64() * 1e9
    }

    fn make_pairs(n: usize) -> Vec<avx512::RgbPairBlocks> {
        let blocks = test::pseudo_random_blocks(n * 6);
        blocks.chunks_exact(6).map(|c| ([c[0], c[1], c[2]], [c[3], c[4], c[5]])).collect()
    }

    struct Stats {
        p10: f64,
        p50: f64,
        p90: f64,
    }

    fn stats(xs: &mut [f64]) -> Stats {
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = xs.len();
        Stats {
            p10: xs[(n as f64 * 0.10) as usize],
            p50: xs[n / 2],
            p90: xs[(n as f64 * 0.90) as usize],
        }
    }
}

#[cfg(feature = "simd-benches")]
fn main() {
    bench::main();
}
