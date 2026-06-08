//! Small numeric helpers shared across the model.
//!
//! Many quantities in the simulation are **indices** constrained to the unit
//! interval `[0, 1]` (e.g. health, biodiversity, inequality). Keeping them
//! clamped is essential: an un-clamped feedback loop can diverge to infinity or
//! go negative and produce nonsensical, NaN-poisoned results. These helpers
//! centralise that discipline so the dynamics code reads cleanly.

/// Clamp a value to the closed unit interval `[0.0, 1.0]`.
///
/// ```
/// use society_sim::util::clamp01;
/// assert_eq!(clamp01(-0.5), 0.0);
/// assert_eq!(clamp01(0.5), 0.5);
/// assert_eq!(clamp01(1.5), 1.0);
/// ```
#[inline]
pub fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

/// Clamp a value to an arbitrary closed interval `[lo, hi]`.
///
/// `lo` must be `<= hi`.
#[inline]
pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    debug_assert!(lo <= hi, "clamp: lo ({lo}) must be <= hi ({hi})");
    x.clamp(lo, hi)
}

/// Move `current` a fraction `rate` of the way toward `target`.
///
/// This is a first-order relaxation (exponential approach). Many real social
/// and ecological quantities do not jump instantly to their equilibrium; they
/// drift toward it. A `rate` of `1.0` snaps immediately; `0.0` never moves.
///
/// ```
/// use society_sim::util::relax;
/// // Half-way each step.
/// assert!((relax(0.0, 1.0, 0.5) - 0.5).abs() < 1e-12);
/// ```
#[inline]
pub fn relax(current: f64, target: f64, rate: f64) -> f64 {
    let rate = rate.clamp(0.0, 1.0);
    current + (target - current) * rate
}

/// Base-2 logarithm, used by the climate forcing relation.
#[inline]
pub fn log2(x: f64) -> f64 {
    x.log2()
}

/// Linear interpolation between `a` and `b` by `t` (typically in `[0, 1]`).
#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp01_bounds() {
        assert_eq!(clamp01(-1.0), 0.0);
        assert_eq!(clamp01(2.0), 1.0);
        assert_eq!(clamp01(0.3), 0.3);
    }

    #[test]
    fn relax_moves_toward_target_and_converges() {
        let mut x = 0.0;
        for _ in 0..200 {
            x = relax(x, 1.0, 0.1);
        }
        assert!((x - 1.0).abs() < 1e-3, "should converge to target, got {x}");
    }

    #[test]
    fn relax_rate_zero_is_fixed_point() {
        assert_eq!(relax(0.2, 0.9, 0.0), 0.2);
    }

    #[test]
    fn lerp_endpoints() {
        assert_eq!(lerp(2.0, 4.0, 0.0), 2.0);
        assert_eq!(lerp(2.0, 4.0, 1.0), 4.0);
        assert_eq!(lerp(2.0, 4.0, 0.5), 3.0);
    }
}
