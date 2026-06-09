//! A tiny, deterministic, dependency-free PRNG.
//!
//! The whole engine must be **bit-reproducible**: same seed ⇒ same history.
//! That is non-negotiable for science (a colleague can reproduce a finding) and
//! for TDD on *emergent* properties (an assertion like "Gini emerges in
//! [0.3,0.6] at seed 42" must be stable). We therefore carry all randomness in
//! one explicit, seeded stream — never `rand::thread_rng`, never a wall clock.
//!
//! [`SplitMix64`] seeds the state; [`Rng`] is the working `xoshiro256**` stream.
//! Both are public-domain algorithms. No external crate (matches the crate's
//! dependency-free policy).

/// SplitMix64 — expands a single `u64` seed into well-mixed state.
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    #[inline]
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// `xoshiro256**` — the working random stream. Small, fast, good statistics,
/// and (crucially) deterministic and `Clone`-able so a whole run can be
/// snapshotted by cloning the world.
#[derive(Debug, Clone)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Seed the stream from a single `u64`.
    pub fn seed(seed: u64) -> Self {
        let mut sm = SplitMix64(seed);
        Rng { s: [sm.next(), sm.next(), sm.next(), sm.next()] }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform `f64` in `[0, 1)`.
    #[inline]
    pub fn f64(&mut self) -> f64 {
        // Top 53 bits → exact f64 in [0,1).
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform `f64` in `[lo, hi)`.
    #[inline]
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f64()
    }

    /// Uniform integer in `[0, n)`.
    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// Deterministic in-place Fisher–Yates shuffle.
    pub fn shuffle<T>(&mut self, xs: &mut [T]) {
        let n = xs.len();
        for i in (1..n).rev() {
            let j = self.below(i + 1);
            xs.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::seed(42);
        let mut b = Rng::seed(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::seed(1);
        let mut b = Rng::seed(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn f64_in_unit_interval() {
        let mut r = Rng::seed(7);
        for _ in 0..10_000 {
            let x = r.f64();
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn below_zero_is_guarded() {
        assert_eq!(Rng::seed(1).below(0), 0);
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut r = Rng::seed(123);
        let mut v: Vec<u32> = (0..100).collect();
        r.shuffle(&mut v);
        v.sort();
        assert!(v.iter().enumerate().all(|(i, &x)| i as u32 == x));
    }
}
