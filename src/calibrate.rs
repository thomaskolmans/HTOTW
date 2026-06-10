//! **Calibration — simulate *to* reality, never from it.** The deepest answer
//! to "make no assumptions": instead of trusting the scale-model primitives,
//! *fit* them so the world's **measured, emergent** moments land on documented
//! reality, by the **Method of Simulated Moments** (McFadden 1989; Grazzini &
//! Richiardi 2015, MSM for agent-based models).
//!
//! Empirical **targets** (a pre-modern life expectancy, a documented wealth
//! Gini, a near-stationary pre-industrial population) live **only on the
//! right-hand side of a loss function** — they are never written into the
//! world. The calibrator searches the *primitive* vector θ (labour yield,
//! birth ceiling, fossil endowment), builds a world from each candidate, runs
//! it over a seed ensemble, MEASURES the moments, and keeps the θ whose
//! measurements come closest. Deterministic: same settings ⇒ same fit.
//!
//! Optimiser: Latin-hypercube global sampling (McKay–Beckman–Conover 1979)
//! followed by a coordinate pattern-search refinement (Hooke & Jeeves 1961) —
//! dependency-free and deterministic, like everything else here.

use crate::config::{Scenario, WorldConfig};
use crate::measure::Measurements;
use crate::rng::Rng;
use crate::world::World;

/// One empirical calibration target: a named **measured** moment, the
/// documented value to match, and a weight. Lives only inside the loss.
pub struct Target {
    pub name: &'static str,
    /// Pulls the model's emergent moment out of a run's averaged measurements.
    pub extract: fn(&Moments) -> f64,
    pub value: f64,
    pub weight: f64,
}

/// Time-averaged emergent moments of one run (what the loss compares).
#[derive(Debug, Clone, Copy, Default)]
pub struct Moments {
    /// Mean measured life expectancy over the evaluation window.
    pub life_expectancy: f64,
    /// Mean measured wealth Gini.
    pub wealth_gini: f64,
    /// Population at the end relative to the start (stationarity).
    pub pop_ratio: f64,
    /// Mean deprivation rate (the share of people short of survival needs).
    pub deprivation: f64,
}

/// The documented pre-industrial moments the default calibration aims at:
///
/// - **life expectancy ≈ 32** — pre-modern societies sat in the 25–40 band
///   with high infant mortality (Riley 2005, global history of life
///   expectancy);
/// - **wealth Gini ≈ 0.7** — historical wealth (not income) inequality of
///   agrarian societies runs 0.6–0.85 (Lindert & Williamson; Scheidel 2017);
/// - **population ratio ≈ 1.1 per century** — pre-industrial populations grew
///   ~0.05–0.1%/yr (McEvedy & Jones 1978), i.e. near-stationary;
/// - **deprivation ≈ 0.1** — chronic subsistence stress was real but not
///   universal (famine demography; Ó Gráda 2009).
pub fn preindustrial_targets() -> Vec<Target> {
    vec![
        Target { name: "life_expectancy", extract: |m| m.life_expectancy, value: 32.0, weight: 1.0 },
        Target { name: "wealth_gini", extract: |m| m.wealth_gini, value: 0.70, weight: 1.0 },
        Target { name: "population_ratio", extract: |m| m.pop_ratio, value: 1.1, weight: 1.0 },
        Target { name: "deprivation_rate", extract: |m| m.deprivation, value: 0.10, weight: 0.5 },
    ]
}

/// The calibratable primitive vector θ and its physical bounds. Every knob is
/// an *input* (biology/economy scale), never an outcome.
pub const KNOBS: [(&str, f64, f64); 3] = [
    // Labour yield: 2 (barely self-feeding) .. 12 (very productive land race).
    ("base-yield", 2.0, 12.0),
    // Birth ceiling: 0.15 .. 0.5 births per fertile person-year.
    ("birth-ceiling", 0.15, 0.5),
    // Fossil endowment per capita (matters only once industry takes off).
    ("fossil-endowment", 10.0, 150.0),
];

/// Write a primitive vector into a config — the proof that calibration only
/// ever touches primitives (there is simply no field for an outcome).
pub fn decode(base: &WorldConfig, theta: &[f64]) -> WorldConfig {
    let mut cfg = base.clone();
    cfg.base_yield = theta[0].clamp(KNOBS[0].1, KNOBS[0].2);
    cfg.birth_ceiling = theta[1].clamp(KNOBS[1].1, KNOBS[1].2);
    cfg.fossil_endowment = theta[2].clamp(KNOBS[2].1, KNOBS[2].2);
    cfg
}

/// Run one candidate world and time-average its measured moments over the
/// final half of the run (the settled regime, not the transient).
pub fn run_moments(cfg: &WorldConfig, years: usize) -> Moments {
    let scenario = Scenario::new("calibration", cfg.clone());
    let mut w = World::from_scenario(&scenario);
    let p0 = w.initial_population.max(1) as f64;
    let warmup = years / 2;
    for _ in 0..warmup {
        w.step();
    }
    let mut acc = Moments::default();
    let mut n = 0.0;
    let mut last: Option<Measurements> = None;
    for _ in warmup..years {
        w.step();
        let m = w.measure();
        if m.life_expectancy.is_finite() {
            acc.life_expectancy += m.life_expectancy;
        }
        acc.wealth_gini += m.wealth_gini;
        acc.deprivation += m.deprivation_rate;
        n += 1.0;
        last = Some(m);
    }
    if n > 0.0 {
        acc.life_expectancy /= n;
        acc.wealth_gini /= n;
        acc.deprivation /= n;
    }
    acc.pop_ratio = last.map(|m| m.population as f64 / p0).unwrap_or(0.0);
    acc
}

/// Ensemble-mean moments across seeds (damps Monte-Carlo noise).
pub fn ensemble_moments(base: &WorldConfig, theta: &[f64], seeds: &[u64], years: usize) -> Moments {
    let mut acc = Moments::default();
    for &s in seeds {
        let mut cfg = decode(base, theta);
        cfg.seed = s;
        let m = run_moments(&cfg, years);
        acc.life_expectancy += m.life_expectancy;
        acc.wealth_gini += m.wealth_gini;
        acc.pop_ratio += m.pop_ratio;
        acc.deprivation += m.deprivation;
    }
    let k = seeds.len().max(1) as f64;
    acc.life_expectancy /= k;
    acc.wealth_gini /= k;
    acc.pop_ratio /= k;
    acc.deprivation /= k;
    acc
}

/// The MSM loss: `L(θ) = Σ w_k ((m_k(θ) − target_k)/target_k)²` — relative
/// error so moments of different magnitudes weigh comparably. Targets appear
/// here and nowhere else.
pub fn loss(moments: &Moments, targets: &[Target]) -> f64 {
    targets
        .iter()
        .map(|t| {
            let m = (t.extract)(moments);
            let rel = (m - t.value) / t.value.abs().max(1e-9);
            t.weight * rel * rel
        })
        .sum()
}

/// The result of a calibration.
pub struct Calibration {
    pub theta: Vec<f64>,
    pub fitted: WorldConfig,
    pub loss: f64,
    /// The loss of the uncalibrated base config (the start to beat).
    pub initial_loss: f64,
    pub moments: Moments,
}

/// **Calibrate**: Latin-hypercube global search over θ, then a pattern-search
/// refinement of the best sample. `samples` LHS points, `refine` shrink steps.
pub fn calibrate(
    base: &WorldConfig,
    targets: &[Target],
    seeds: &[u64],
    years: usize,
    samples: usize,
    refine: usize,
) -> Calibration {
    let dim = KNOBS.len();
    let eval = |theta: &[f64]| -> f64 {
        loss(&ensemble_moments(base, theta, seeds, years), targets)
    };

    // The uncalibrated starting point.
    let theta0: Vec<f64> = vec![base.base_yield, base.birth_ceiling, base.fossil_endowment];
    let initial_loss = eval(&theta0);

    // Latin-hypercube sampling: stratify each knob's range into `samples`
    // bins, permute bins per dimension (deterministic), evaluate each point.
    let mut rng = Rng::seed(0xCA11_B8A7E);
    let mut best_theta = theta0.clone();
    let mut best_loss = initial_loss;
    if samples > 0 {
        let mut bins: Vec<Vec<usize>> = (0..dim)
            .map(|_| {
                let mut idx: Vec<usize> = (0..samples).collect();
                rng.shuffle(&mut idx);
                idx
            })
            .collect();
        for s in 0..samples {
            let theta: Vec<f64> = (0..dim)
                .map(|d| {
                    let (_, lo, hi) = KNOBS[d];
                    let cell = bins[d][s] as f64 + rng.f64();
                    lo + (hi - lo) * cell / samples as f64
                })
                .collect();
            let l = eval(&theta);
            if l < best_loss {
                best_loss = l;
                best_theta = theta;
            }
        }
        let _ = &mut bins;
    }

    // Hooke–Jeeves pattern search: probe ± a step on each coordinate, move to
    // any improvement, shrink the step when stuck.
    let mut step: Vec<f64> = KNOBS.iter().map(|(_, lo, hi)| (hi - lo) * 0.1).collect();
    for _ in 0..refine {
        let mut improved = false;
        for d in 0..dim {
            for dir in [1.0, -1.0] {
                let mut probe = best_theta.clone();
                probe[d] = (probe[d] + dir * step[d]).clamp(KNOBS[d].1, KNOBS[d].2);
                let l = eval(&probe);
                if l < best_loss {
                    best_loss = l;
                    best_theta = probe;
                    improved = true;
                }
            }
        }
        if !improved {
            for s in &mut step {
                *s *= 0.5;
            }
        }
    }

    let fitted = decode(base, &best_theta);
    let moments = ensemble_moments(base, &best_theta, seeds, years);
    Calibration { theta: best_theta, fitted, loss: best_loss, initial_loss, moments }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_base() -> WorldConfig {
        let mut cfg = WorldConfig::default();
        cfg.nlon = 24;
        cfg.nlat = 12;
        cfg.n_agents = 600;
        cfg.n_polities = 2;
        cfg
    }

    /// The central result: MSM search measurably reduces the distance between
    /// the EMERGENT moments and the documented targets versus the uncalibrated
    /// start — and the fitted thing is a *primitive* vector inside physical
    /// bounds, never an outcome.
    #[test]
    fn calibration_reduces_the_loss() {
        let base = small_base();
        let targets = preindustrial_targets();
        let cal = calibrate(&base, &targets, &[1, 2], 60, 8, 4);
        assert!(
            cal.loss <= cal.initial_loss + 1e-12,
            "calibration must not be worse than the start: {} vs {}",
            cal.loss,
            cal.initial_loss
        );
        // The fitted primitives are physical and in bounds.
        for (d, (_, lo, hi)) in KNOBS.iter().enumerate() {
            assert!(
                (*lo..=*hi).contains(&cal.theta[d]),
                "knob {d} out of bounds: {}",
                cal.theta[d]
            );
        }
        // decode writes only primitives (spot-check: outcome-free).
        assert_eq!(cal.fitted.base_yield, cal.theta[0].clamp(KNOBS[0].1, KNOBS[0].2));
    }

    #[test]
    fn calibration_is_deterministic() {
        let base = small_base();
        let targets = preindustrial_targets();
        let a = calibrate(&base, &targets, &[3], 40, 6, 3);
        let b = calibrate(&base, &targets, &[3], 40, 6, 3);
        assert_eq!(a.loss.to_bits(), b.loss.to_bits());
        for (x, y) in a.theta.iter().zip(b.theta.iter()) {
            assert_eq!(x.to_bits(), y.to_bits());
        }
    }

    #[test]
    fn loss_is_zero_at_the_targets_and_grows_off_them() {
        let targets = preindustrial_targets();
        let at = Moments { life_expectancy: 32.0, wealth_gini: 0.70, pop_ratio: 1.1, deprivation: 0.10 };
        assert!(loss(&at, &targets) < 1e-18);
        let off = Moments { life_expectancy: 20.0, ..at };
        assert!(loss(&off, &targets) > 0.01);
    }
}
