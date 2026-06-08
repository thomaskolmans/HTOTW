//! **Deterministic, dependency-free parallelism** (Phase 8 — scale).
//!
//! The hard rule of this engine is *bit-reproducibility*: same seed ⇒ same
//! history, regardless of how many threads run it. Parallelism here therefore
//! obeys one discipline — it is only ever applied to phases that are
//! **order-independent and RNG-free**, and the work is partitioned into
//! **contiguous, disjoint index ranges**. Each element's result depends only on
//! its own input, so the floating-point arithmetic each thread performs is
//! *identical* to what the single-threaded loop would compute for those same
//! indices. No reduction across threads, no shared accumulator, no atomics on
//! floats: there is nothing whose result can depend on thread interleaving or
//! count. The sequential path stays the canonical golden oracle, and the
//! parallel path is asserted bit-identical to it (see the engine tests).
//!
//! Implementation is **std-only** (`std::thread::scope`); no new crate enters
//! the default build. Order-dependent phases (movement, bilateral trade,
//! reproduction, enforcement, the RNG-consuming vital-events loop) are *not*
//! routed through here — they remain strictly sequential by design.

/// Global cap on worker threads for the data-parallel phases. `1` forces the
/// canonical single-threaded path (used by the determinism oracle and the
/// `parallel == sequential` test). Defaults to the detected core count, clamped
/// to a sane maximum so a pathological machine can't spawn thousands of threads.
use std::sync::atomic::{AtomicUsize, Ordering};

static MAX_THREADS: AtomicUsize = AtomicUsize::new(0);

/// Set the worker-thread cap for the data-parallel phases (clamped to ≥ 1).
/// Setting it to 1 makes every parallel phase run on the calling thread, i.e.
/// exactly the sequential code path. Returns the value actually stored.
pub fn set_max_threads(n: usize) -> usize {
    let n = n.max(1);
    MAX_THREADS.store(n, Ordering::Relaxed);
    n
}

/// The configured worker-thread cap, lazily defaulting to the machine's
/// available parallelism (clamped to `[1, 64]`). Determinism does **not** depend
/// on this value — it only changes *how fast* an order-independent phase runs,
/// never *what* it computes.
pub fn max_threads() -> usize {
    let v = MAX_THREADS.load(Ordering::Relaxed);
    if v != 0 {
        return v;
    }
    let detected = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
        .clamp(1, 64);
    // Cache it so repeated calls are cheap and stable within a process.
    let _ = MAX_THREADS.compare_exchange(0, detected, Ordering::Relaxed, Ordering::Relaxed);
    detected
}

/// Below this many items it is not worth spawning threads — the scheduling
/// overhead dominates the cheap per-element work in the data-parallel phases.
/// Small worlds (every existing test, and any modest run) therefore take the
/// single-threaded path, byte-for-byte unchanged; only genuinely large
/// landscapes (hundreds of thousands of cells / agents) cross into threading,
/// where the speedup is real (see the `bench` subcommand). Determinism is
/// independent of this constant — it only gates *when* threads spin up.
const PARALLEL_THRESHOLD: usize = 65_536;

/// Apply `f` to every disjoint, contiguous chunk of `data`, in parallel when the
/// slice is large enough and more than one worker is configured.
///
/// **Determinism contract:** `f(chunk_start_index, chunk)` must compute each
/// element's new value as a pure function of that element's own prior value (and
/// shared *read-only* state). Because the chunks are contiguous and disjoint and
/// the per-element computation is independent, the result is **bit-identical** to
/// calling `f(0, data)` once on the whole slice — for any thread count. This is
/// the invariant the engine's `parallel == sequential` test pins down.
///
/// `f` receives the global start index of its chunk so it can address any
/// parallel companion arrays by absolute index.
pub fn for_each_chunk_mut<T, F>(data: &mut [T], f: F)
where
    T: Send,
    F: Fn(usize, &mut [T]) + Sync,
{
    let n = data.len();
    let workers = max_threads();
    if workers <= 1 || n < PARALLEL_THRESHOLD {
        // Canonical sequential path.
        f(0, data);
        return;
    }

    // Partition into `workers` contiguous chunks of near-equal size. Contiguity
    // + disjointness is what makes the float arithmetic per element identical to
    // the sequential loop regardless of how the range is split.
    let chunk = n.div_ceil(workers);
    let f = &f;
    std::thread::scope(|scope| {
        let mut rest = data;
        let mut start = 0usize;
        while !rest.is_empty() {
            let take = chunk.min(rest.len());
            let (head, tail) = rest.split_at_mut(take);
            let base = start;
            scope.spawn(move || f(base, head));
            rest = tail;
            start += take;
        }
    });
}

/// Test-only serialization lock. The worker cap is process-global state, and
/// the cargo test harness runs tests concurrently, so any test that *pins* the
/// cap (to compare configurations or to assert determinism) must hold this lock
/// for its whole duration — otherwise a sibling test could change the cap
/// mid-run. Engine determinism does not depend on the cap; this only stops one
/// test from disturbing another's pinned configuration.
#[cfg(test)]
pub(crate) static THREAD_CAP_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_matches_whole_slice_for_any_thread_count() {
        let _guard = THREAD_CAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // A pure per-element map must give identical results sequentially and in
        // parallel, for several thread counts and a non-trivial length.
        let n = 100_003usize;
        let base: Vec<f64> = (0..n).map(|i| (i as f64).sin() * 1.000_000_3).collect();

        let map = |start: usize, c: &mut [f64]| {
            for (k, v) in c.iter_mut().enumerate() {
                let idx = start + k;
                // Some non-associative float work so any cross-thread reduction
                // bug would show up as a bit difference.
                *v = (*v * 1.000_000_7 + idx as f64).sqrt().abs();
            }
        };

        set_max_threads(1);
        let mut seq = base.clone();
        for_each_chunk_mut(&mut seq, map);

        for threads in [2usize, 3, 4, 7, 16] {
            set_max_threads(threads);
            let mut par = base.clone();
            for_each_chunk_mut(&mut par, map);
            assert_eq!(
                seq.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                par.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                "parallel result must be bit-identical to sequential ({threads} threads)"
            );
        }
        set_max_threads(1);
    }

    #[test]
    fn small_slices_take_the_sequential_path() {
        let _guard = THREAD_CAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Below the threshold the parallel entry point still produces the right
        // answer (it just runs inline).
        set_max_threads(8);
        let mut v: Vec<u64> = (0..1000).collect();
        for_each_chunk_mut(&mut v, |start, c| {
            for (k, x) in c.iter_mut().enumerate() {
                *x = (start + k) as u64 * 2;
            }
        });
        assert!(v.iter().enumerate().all(|(i, &x)| x == i as u64 * 2));
        set_max_threads(1);
    }
}
