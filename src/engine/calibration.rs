//! **Calibration & experiment harness** (Phase 4) — the formal "simulate **to**
//! the numbers".
//!
//! The hard rule is unchanged and is the whole point of this module: real-world
//! statistics (a within-country Gini near ~0.39, a plausible life expectancy)
//! appear **only on the right-hand side of a loss function**. They are *never*
//! assigned to the [`World`]. A world is always built from [`Primitives`] alone;
//! the macro moments it is scored against are **measured** out of it by the
//! read-only [`crate::engine::instruments`]. Calibration therefore tunes the
//! *primitives* (the laws/biology/geography) until the *emergent* moments match
//! the targets — inverse modelling, not back-fitting the outputs.
//!
//! ## Method
//!
//! This is the **Method of Simulated Moments** (MSM) / indirect inference: choose
//! the structural primitives `θ` that minimise a weighted distance between the
//! moments the model *produces* and the empirical moments,
//! `L(θ) = Σ_k w_k · (m_k(θ) − m̂_k)²`, where each `m_k(θ)` is obtained by
//! *simulating* the agent model at `θ` and measuring it. Because the model is
//! stochastic we average each moment over a small ensemble of seeds to damp
//! Monte-Carlo noise before forming the loss (Grazzini & Richiardi 2015 discuss
//! exactly this for agent-based models). The optimiser is a dependency-free
//! global→local pipeline: a **Latin-Hypercube** random search for a good basin,
//! then a hand-rolled **Nelder–Mead** simplex refinement. (A full Bayesian
//! treatment would replace the point estimate with **Approximate Bayesian
//! Computation**, Beaumont 2010, accepting θ whose simulated moments fall within
//! a tolerance of the data — the same forward-only simulate-and-compare loop.)
//!
//! ## Citations
//! - McFadden (1989), *A Method of Simulated Moments for Estimation of Discrete
//!   Response Models Without Numerical Integration*, Econometrica.
//! - Grazzini & Richiardi (2015), *Estimation of ergodic agent-based models by
//!   simulated minimum distance*, J. Economic Dynamics & Control.
//! - Beaumont (2010), *Approximate Bayesian Computation in Evolution and
//!   Ecology*, Annual Review of Ecology, Evolution, and Systematics.
//! - Nelder & Mead (1965), *A Simplex Method for Function Minimization*.
//! - McKay, Beckman & Conover (1979), Latin Hypercube Sampling.

use super::instruments::measure;
use super::institutions::Rule;
use super::rng::Rng;
use super::world::{Primitives, World};

/// Summary of the **emergent** moments of a single run, taken at its end (after
/// the population has turned over). Every field is *measured* by an instrument;
/// none is ever set. These are the model's `m_k(θ)` in the MSM loss.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunSummary {
    /// Living population (carrying capacity the primitives produced).
    pub population: f64,
    /// Emergent wealth **Gini** (price-valued bundle + energy).
    pub gini: f64,
    /// Emergent **life expectancy** (mean realised lifespan).
    pub life_expectancy: f64,
    /// Mean agent wealth at the emergent price.
    pub mean_wealth: f64,
    /// Mean per-agent need-satisfaction welfare (prosperity proxy).
    pub welfare_per_capita: f64,
    /// Commons health in `[0,1]` (sustainability).
    pub commons_health: f64,
    /// The seeded starting population (a primitive INPUT, carried here only to
    /// normalise the emergent population into a `[0,1]` survival pillar). Never a
    /// social outcome — it is the count the run was launched with.
    pub initial_population: f64,
}

impl RunSummary {
    /// The all-zero summary, used as the additive identity when averaging.
    fn zero() -> RunSummary {
        RunSummary {
            population: 0.0,
            gini: 0.0,
            life_expectancy: 0.0,
            mean_wealth: 0.0,
            welfare_per_capita: 0.0,
            commons_health: 0.0,
            initial_population: 0.0,
        }
    }

    fn add(&mut self, o: &RunSummary) {
        self.population += o.population;
        self.gini += o.gini;
        self.life_expectancy += o.life_expectancy;
        self.mean_wealth += o.mean_wealth;
        self.welfare_per_capita += o.welfare_per_capita;
        self.commons_health += o.commons_health;
        self.initial_population += o.initial_population;
    }

    fn scale(&mut self, f: f64) {
        self.population *= f;
        self.gini *= f;
        self.life_expectancy *= f;
        self.mean_wealth *= f;
        self.welfare_per_capita *= f;
        self.commons_health *= f;
        self.initial_population *= f;
    }
}

/// Run a single world from `primitives` (under an optional rule stack) for
/// `ticks` and **measure** its emergent summary. Construction is from primitives
/// only; the summary is read out by instruments — the hard rule, in one place.
pub fn run_summary(primitives: Primitives, rules: &[Box<dyn Rule>], ticks: usize) -> RunSummary {
    let n0 = primitives.n_agents as f64;
    let mut w = World::new(primitives);
    for _ in 0..ticks {
        w.step_with_rules(rules);
    }
    let m = measure(&w);
    let welfare = super::instruments::total_welfare(&w);
    let pop = m.population.max(1) as f64;
    RunSummary {
        population: m.population as f64,
        gini: m.wealth_gini,
        // Life expectancy is NaN until a death; treat a deathless run's "life"
        // as the run length (everyone is still alive past it) so the moment is
        // always finite for the loss.
        life_expectancy: if m.life_expectancy.is_finite() {
            m.life_expectancy
        } else {
            ticks as f64
        },
        mean_wealth: m.mean_wealth,
        welfare_per_capita: welfare / pop,
        commons_health: m.commons_health,
        initial_population: n0,
    }
}

/// Average the emergent summary over a small **ensemble of seeds** (Monte-Carlo
/// noise reduction before forming the loss, per Grazzini & Richiardi). The seed
/// is the only thing that varies across the ensemble — the primitives are held
/// fixed, so this estimates `E[m_k(θ)]`.
pub fn ensemble_summary(
    base: &Primitives,
    seeds: &[u64],
    rules: &[Box<dyn Rule>],
    ticks: usize,
) -> RunSummary {
    let mut acc = RunSummary::zero();
    for &s in seeds {
        let mut p = base.clone();
        p.seed = s;
        acc.add(&run_summary(p, rules, ticks));
    }
    if !seeds.is_empty() {
        acc.scale(1.0 / seeds.len() as f64);
    }
    acc
}

/// One empirical **calibration target**: a named moment, a read-only extractor
/// that pulls the corresponding *measured* moment out of a [`RunSummary`], the
/// empirical value to match, and a weight. The target lives **only** here, on the
/// right-hand side of the loss — it is never written into a world.
pub struct Target {
    pub name: &'static str,
    /// Pulls the model's emergent moment `m_k(θ)` out of a measured summary.
    pub extract: fn(&RunSummary) -> f64,
    /// The empirical value `m̂_k` to match (RHS of the loss only).
    pub target: f64,
    /// Weight `w_k` (e.g. inverse variance / scale) in the weighted distance.
    pub weight: f64,
}

/// A default set of empirical targets drawn from `docs/RESEARCH.md` / `ENGINE.md`
/// (a within-country wealth Gini near ~0.39 and a plausible life expectancy on
/// this world's age scale). The weights normalise the two very different scales
/// (a Gini lives in `[0,1]`; a lifespan is on the order of tens of ticks) so
/// neither moment dominates the loss purely by magnitude. **These constants
/// appear nowhere but here, as the RHS of the loss.**
pub fn default_targets() -> Vec<Target> {
    vec![
        // Within-country wealth inequality ~0.39 (a common empirical figure;
        // the model must *produce* this, never be initialised at it).
        Target {
            name: "wealth_gini",
            extract: |s| s.gini,
            target: 0.39,
            // Gini is already O(1); weight ~ 1/scale² with scale≈0.39.
            weight: 1.0 / 0.39_f64.powi(2),
        },
        // A plausible life expectancy on the engine's age scale (max_age=100):
        // ~70 "years". Weighted by 1/scale² so its larger magnitude doesn't
        // swamp the Gini term.
        Target {
            name: "life_expectancy",
            extract: |s| s.life_expectancy,
            target: 70.0,
            weight: 1.0 / 70.0_f64.powi(2),
        },
    ]
}

/// The MSM **loss**: weighted squared distance between the ensemble-averaged
/// *emergent* moments and the empirical targets,
/// `L(θ) = Σ_k w_k (m_k(θ) − m̂_k)²`. The world is built from `θ` (the
/// primitives) and the moments are measured out of it; the targets enter only
/// here, on the right-hand side. Lower is a better match.
pub fn loss(summary: &RunSummary, targets: &[Target]) -> f64 {
    let mut l = 0.0;
    for t in targets {
        let m = (t.extract)(summary);
        let d = m - t.target;
        l += t.weight * d * d;
    }
    l
}

// ---------------------------------------------------------------------------
// Parameter space: the *primitive* vector θ the optimiser searches over.
// CRITICAL: every coordinate is a physical/biological PRIMITIVE. No coordinate
// is a macro outcome. Decoding a θ produces a `Primitives`; the macro moments
// are then measured, never set.
// ---------------------------------------------------------------------------

/// A single searchable primitive: its human-readable name (for reports), the
/// `[lo, hi]` physical bounds the search explores within, and how to write it.
struct Knob {
    name: &'static str,
    lo: f64,
    hi: f64,
    /// Write the (already in-range) value into a `Primitives`.
    set: fn(&mut Primitives, f64),
}

/// The calibration **parameter space**: a handful of primitives that move the
/// emergent Gini and life expectancy (scarcity, metabolic heterogeneity,
/// mortality, perception, reproduction threshold). All are laws/biology — the
/// search never touches an outcome.
fn knobs() -> Vec<Knob> {
    vec![
        Knob {
            name: "peak_capacity",
            lo: 2.0,
            hi: 12.0,
            set: |p, v| p.peak_capacity = v,
        },
        Knob {
            name: "metabolism_max",
            lo: 1.0,
            hi: 4.0,
            // Keep min < max; the decoder clamps below.
            set: |p, v| p.metabolism_max = v,
        },
        Knob {
            name: "senescence",
            lo: 0.0001,
            hi: 0.002,
            set: |p, v| p.senescence = v,
        },
        Knob {
            name: "vision_max",
            lo: 2.0,
            hi: 10.0,
            set: |p, v| p.vision_max = v.round().max(p.vision_min as f64) as u32,
        },
        Knob {
            name: "birth_threshold",
            lo: 12.0,
            hi: 45.0,
            set: |p, v| p.birth_threshold = v,
        },
    ]
}

/// Number of free primitives the optimiser searches over.
pub fn dim() -> usize {
    knobs().len()
}

/// The human-readable names of the searched primitives, in θ order (for
/// reporting a calibration result).
pub fn knob_names() -> Vec<&'static str> {
    knobs().iter().map(|k| k.name).collect()
}

/// Clamp a raw coordinate to its knob's physical bounds.
fn clamp_knob(k: &Knob, v: f64) -> f64 {
    v.max(k.lo).min(k.hi)
}

/// Decode a search vector `theta` (each coordinate in `[lo,hi]` of its knob) into
/// a concrete [`Primitives`]. Out-of-range coordinates are clamped into their
/// physical bounds so the optimiser can never request an unphysical world. The
/// resulting `Primitives` is the *only* thing handed to `World::new` — proof that
/// construction stays primitive-only.
pub fn decode(base: &Primitives, theta: &[f64]) -> Primitives {
    let ks = knobs();
    let mut p = base.clone();
    for (k, &v) in ks.iter().zip(theta.iter()) {
        (k.set)(&mut p, clamp_knob(k, v));
    }
    // Keep metabolism_min strictly below metabolism_max (a physical sanity
    // constraint on the heterogeneity band, not a social outcome).
    if p.metabolism_min >= p.metabolism_max {
        p.metabolism_min = (p.metabolism_max * 0.25).max(0.1);
    }
    p
}

/// Evaluate the MSM loss at a search point `theta` for the given targets, by
/// decoding to primitives, simulating the seed ensemble, measuring the emergent
/// moments and forming the weighted distance. This is the function the optimiser
/// minimises — the only place targets and primitives meet, and they meet only as
/// `(measured − target)`.
pub fn loss_at(
    base: &Primitives,
    theta: &[f64],
    seeds: &[u64],
    ticks: usize,
    targets: &[Target],
) -> f64 {
    let p = decode(base, theta);
    let summary = ensemble_summary(&p, seeds, &[], ticks);
    loss(&summary, targets)
}

/// The result of a calibration: the best primitive vector found, its decoded
/// [`Primitives`], the achieved loss, and the loss at the random starting point
/// (so callers/tests can assert the search *measurably reduced* the loss).
#[derive(Debug, Clone)]
pub struct Calibration {
    pub theta: Vec<f64>,
    pub primitives: Primitives,
    pub loss: f64,
    pub initial_loss: f64,
}

/// Knob midpoints — a neutral starting θ.
fn theta_mid() -> Vec<f64> {
    knobs().iter().map(|k| 0.5 * (k.lo + k.hi)).collect()
}

/// Draw one **Latin-Hypercube** sample of `n` points in the knob box. LHS
/// stratifies each dimension into `n` equal bins and permutes them independently,
/// giving far better space coverage than plain uniform sampling at the same
/// budget (McKay–Beckman–Conover). Deterministic given the `rng`.
fn latin_hypercube(rng: &mut Rng, n: usize) -> Vec<Vec<f64>> {
    let ks = knobs();
    let d = ks.len();
    // For each dimension, a permutation of bin indices [0,n).
    let mut perms: Vec<Vec<usize>> = (0..d)
        .map(|_| {
            let mut col: Vec<usize> = (0..n).collect();
            rng.shuffle(&mut col);
            col
        })
        .collect();
    let mut points = Vec::with_capacity(n);
    for i in 0..n {
        let mut pt = Vec::with_capacity(d);
        for (j, k) in ks.iter().enumerate() {
            let bin = perms[j][i] as f64;
            // Jittered centre of the stratum, mapped into [lo,hi].
            let u = (bin + rng.f64()) / n as f64;
            pt.push(k.lo + u * (k.hi - k.lo));
        }
        points.push(pt);
    }
    // touch `perms` mutably to satisfy borrow patterns above without clones.
    let _ = &mut perms;
    points
}

/// Reflect/expand/contract helpers operate on plain `Vec<f64>` points; this adds
/// `a + s·(b − a)` componentwise (the Nelder–Mead affine moves).
fn axpy(a: &[f64], b: &[f64], s: f64) -> Vec<f64> {
    a.iter().zip(b).map(|(&ai, &bi)| ai + s * (bi - ai)).collect()
}

/// Hand-rolled **Nelder–Mead** simplex minimisation of `f` over `dim()`-D space,
/// started from `start`, for `iters` reflection steps. Dependency-free and
/// deterministic (no randomness inside). Standard coefficients (reflection 1,
/// expansion 2, contraction 0.5, shrink 0.5). Used to *refine* the best
/// Latin-Hypercube point into its local basin.
fn nelder_mead<F: FnMut(&[f64]) -> f64>(mut f: F, start: &[f64], iters: usize) -> (Vec<f64>, f64) {
    let d = start.len();
    let ks = knobs();
    // Initial simplex: start plus a step along each axis (scaled to each knob).
    let mut simplex: Vec<Vec<f64>> = Vec::with_capacity(d + 1);
    simplex.push(start.to_vec());
    for j in 0..d {
        let mut p = start.to_vec();
        let step = 0.15 * (ks[j].hi - ks[j].lo);
        p[j] = (p[j] + step).min(ks[j].hi);
        simplex.push(p);
    }
    let mut fval: Vec<f64> = simplex.iter().map(|p| f(p)).collect();

    for _ in 0..iters {
        // Order vertices by ascending f (best first). Simple selection sort on a
        // tiny simplex keeps it deterministic and dependency-free.
        let mut idx: Vec<usize> = (0..=d).collect();
        idx.sort_by(|&a, &b| fval[a].partial_cmp(&fval[b]).unwrap());
        let best = idx[0];
        let worst = idx[d];
        let second_worst = idx[d - 1];

        // Centroid of all but the worst.
        let mut centroid = vec![0.0; d];
        for (k, &v) in idx.iter().enumerate() {
            if k == d {
                continue;
            }
            for (c, x) in centroid.iter_mut().zip(&simplex[v]) {
                *c += x;
            }
        }
        for c in &mut centroid {
            *c /= d as f64;
        }

        // Reflection.
        let xr = axpy(&centroid, &simplex[worst], -1.0);
        let fr = f(&xr);
        if fr < fval[best] {
            // Expansion.
            let xe = axpy(&centroid, &simplex[worst], -2.0);
            let fe = f(&xe);
            if fe < fr {
                simplex[worst] = xe;
                fval[worst] = fe;
            } else {
                simplex[worst] = xr;
                fval[worst] = fr;
            }
        } else if fr < fval[second_worst] {
            simplex[worst] = xr;
            fval[worst] = fr;
        } else {
            // Contraction.
            let xc = axpy(&centroid, &simplex[worst], 0.5);
            let fc = f(&xc);
            if fc < fval[worst] {
                simplex[worst] = xc;
                fval[worst] = fc;
            } else {
                // Shrink toward the best vertex.
                let b = simplex[best].clone();
                for (k, v) in simplex.iter_mut().enumerate() {
                    if k == best {
                        continue;
                    }
                    *v = axpy(&b, v, 0.5);
                    fval[k] = f(v);
                }
            }
        }
    }

    // Return the best vertex.
    let mut bi = 0;
    for i in 1..=d {
        if fval[i] < fval[bi] {
            bi = i;
        }
    }
    (simplex[bi].clone(), fval[bi])
}

/// **Calibrate** the primitives to the targets by Method of Simulated Moments:
/// a Latin-Hypercube global search (`lhs_samples` points) for a good basin,
/// followed by `nm_iters` Nelder–Mead refinement steps. `seeds` is the ensemble
/// each candidate is averaged over; `ticks` the run length. Returns the best θ,
/// its decoded [`Primitives`], the achieved loss, and the loss at the *neutral
/// midpoint start* (so a caller can verify the search reduced the loss). Fully
/// deterministic given `base.seed`.
///
/// The targets are passed in and used **only** inside the loss; the world is only
/// ever constructed from decoded primitives — the architectural guarantee that we
/// "simulate *to* the numbers, not from them".
pub fn calibrate(
    base: &Primitives,
    targets: &[Target],
    seeds: &[u64],
    ticks: usize,
    lhs_samples: usize,
    nm_iters: usize,
) -> Calibration {
    let mut rng = Rng::seed(base.seed ^ 0xCA11B_u64);

    // Loss at a neutral, un-searched starting point (the baseline to beat).
    let start = theta_mid();
    let initial_loss = loss_at(base, &start, seeds, ticks, targets);

    // --- Global stage: Latin-Hypercube random search. ---
    let mut best_theta = start.clone();
    let mut best_loss = initial_loss;
    for pt in latin_hypercube(&mut rng, lhs_samples.max(1)) {
        let l = loss_at(base, &pt, seeds, ticks, targets);
        if l < best_loss {
            best_loss = l;
            best_theta = pt;
        }
    }

    // --- Local stage: Nelder–Mead refinement from the best LHS point. ---
    if nm_iters > 0 {
        let (theta_nm, loss_nm) = nelder_mead(
            |t| loss_at(base, t, seeds, ticks, targets),
            &best_theta,
            nm_iters,
        );
        if loss_nm < best_loss {
            best_loss = loss_nm;
            best_theta = theta_nm;
        }
    }

    let primitives = decode(base, &best_theta);
    Calibration {
        theta: best_theta,
        primitives,
        loss: best_loss,
        initial_loss,
    }
}

// ===========================================================================
// Experiment harness: compare ways of organising society across a seed ensemble
// on a MEASURED welfare functional.
// ===========================================================================

/// A way of organising society to be evaluated: the physical/biological
/// [`Primitives`] plus the stack of Phase-3 policy [`Rule`]s in force. A scenario
/// fixes *everything except the seed*, so a multi-seed run isolates the regime's
/// effect from Monte-Carlo noise (Ostrom's comparative method, made statistical).
pub struct Scenario {
    pub name: String,
    pub primitives: Primitives,
    pub rules: Vec<Box<dyn Rule>>,
}

impl Scenario {
    pub fn new(name: impl Into<String>, primitives: Primitives, rules: Vec<Box<dyn Rule>>) -> Self {
        Scenario { name: name.into(), primitives, rules }
    }
}

/// The distribution of emergent outcomes of a [`Scenario`] across a seed
/// ensemble, plus a single **welfare** score. Everything is MEASURED.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub name: String,
    /// Per-seed emergent summaries (the raw distribution, for inspection/tests).
    pub runs: Vec<RunSummary>,
    /// Mean of the per-seed welfare functional (the headline comparison score).
    pub welfare: f64,
}

impl Outcome {
    /// Mean over the ensemble of a chosen emergent moment.
    pub fn mean(&self, f: impl Fn(&RunSummary) -> f64) -> f64 {
        if self.runs.is_empty() {
            return 0.0;
        }
        self.runs.iter().map(&f).sum::<f64>() / self.runs.len() as f64
    }
}

/// The **welfare functional**: a Sen/Stiglitz-style composite that rewards a
/// society only if it is *simultaneously* prosperous, equitable, sustainable and
/// *populous*. We use the **geometric mean** of four MEASURED, normalised
/// ingredients —
///
/// - **prosperity** = mean per-capita need-satisfaction welfare (saturated to
///   `[0,1]` so it can't dominate by sheer magnitude),
/// - **equity** = `1 − Gini`,
/// - **sustainability** = commons health,
/// - **survival** = living population as a fraction of the seeded population,
///   saturated at 1 (a society that carries *more* people at decent conditions is
///   better — this is what stops a per-capita metric from "winning" by letting
///   most agents die, the classic total-vs-average welfare trap).
///
/// so that collapsing any one pillar (mass poverty, extreme inequality, an
/// exhausted commons, a population crash) drags the whole score down — exactly
/// the multidimensional trade-off a single GDP number hides. The geometric mean
/// is the standard "no-substitutes" aggregator (it is how the UN HDI combines its
/// pillars). Every input is read off the run by an instrument; only the
/// normalisers (the satiation scale, the *input* seed population) are primitives.
pub fn welfare(s: &RunSummary) -> f64 {
    // A dead society scores zero (no one to flourish).
    if s.population <= 0.0 {
        return 0.0;
    }
    // Saturate per-capita welfare into [0,1]: u/(u+1) is a need-satisfaction
    // curve again (diminishing returns to private abundance), so prosperity is a
    // bounded pillar rather than an unbounded multiplier.
    let prosperity = {
        let u = s.welfare_per_capita.max(0.0);
        u / (u + 1.0)
    };
    let equity = (1.0 - s.gini).clamp(0.0, 1.0);
    let sustainability = s.commons_health.clamp(0.0, 1.0);
    let survival = if s.initial_population > 0.0 {
        (s.population / s.initial_population).clamp(0.0, 1.0)
    } else {
        1.0
    };
    (prosperity * equity * sustainability * survival)
        .max(0.0)
        .powf(0.25)
}

/// Evaluate a [`Scenario`] across a seed ensemble: run each seed, measure its
/// emergent summary and welfare, and return the outcome distribution with the
/// mean welfare. Deterministic given the seeds.
pub fn evaluate(scenario: &Scenario, seeds: &[u64], ticks: usize) -> Outcome {
    let mut runs = Vec::with_capacity(seeds.len());
    let mut wsum = 0.0;
    for &s in seeds {
        let mut p = scenario.primitives.clone();
        p.seed = s;
        let summary = run_summary(p, &scenario.rules, ticks);
        wsum += welfare(&summary);
        runs.push(summary);
    }
    let welfare = if seeds.is_empty() { 0.0 } else { wsum / seeds.len() as f64 };
    Outcome { name: scenario.name.clone(), runs, welfare }
}

/// Ordering of two regimes by emergent welfare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The first scenario has strictly higher measured welfare.
    First,
    /// The second scenario has strictly higher measured welfare.
    Second,
    /// A tie (within `f64` equality of the mean welfare).
    Tie,
}

/// **Compare** two ways of organising society on the welfare functional across a
/// shared seed ensemble. Returns each outcome and a deterministic verdict — the
/// canonical Phase-4 experiment ("which regime is better, and by how much?"),
/// answered entirely from MEASURED outcomes. Same seeds ⇒ identical verdict.
pub fn compare(a: &Scenario, b: &Scenario, seeds: &[u64], ticks: usize) -> (Outcome, Outcome, Verdict) {
    let oa = evaluate(a, seeds, ticks);
    let ob = evaluate(b, seeds, ticks);
    let verdict = if oa.welfare > ob.welfare {
        Verdict::First
    } else if ob.welfare > oa.welfare {
        Verdict::Second
    } else {
        Verdict::Tie
    };
    (oa, ob, verdict)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loss_is_zero_when_moments_hit_targets() {
        let targets = default_targets();
        let s = RunSummary {
            population: 100.0,
            gini: 0.39,
            life_expectancy: 70.0,
            mean_wealth: 10.0,
            welfare_per_capita: 1.0,
            commons_health: 1.0,
            initial_population: 100.0,
        };
        assert!(loss(&s, &targets).abs() < 1e-12);
    }

    #[test]
    fn loss_grows_with_distance_from_target() {
        let targets = default_targets();
        let near = RunSummary {
            population: 100.0,
            gini: 0.40,
            life_expectancy: 70.0,
            mean_wealth: 10.0,
            welfare_per_capita: 1.0,
            commons_health: 1.0,
            initial_population: 100.0,
        };
        let far = RunSummary { gini: 0.9, ..near };
        assert!(loss(&far, &targets) > loss(&near, &targets));
    }

    #[test]
    fn decode_only_writes_primitives_and_clamps() {
        let base = Primitives::demo();
        // Wildly out-of-range theta must clamp into physical bounds.
        let theta = vec![1e9, -1e9, 1e9, 1e9, -1e9];
        let p = decode(&base, &theta);
        assert!(p.peak_capacity <= 12.0 && p.peak_capacity >= 2.0);
        assert!(p.metabolism_min < p.metabolism_max);
        assert!(p.birth_threshold >= 12.0 && p.birth_threshold <= 45.0);
    }

    #[test]
    fn latin_hypercube_covers_each_dimension() {
        let mut rng = Rng::seed(1);
        let n = 8;
        let pts = latin_hypercube(&mut rng, n);
        assert_eq!(pts.len(), n);
        let ks = knobs();
        // Every point lies in the box.
        for p in &pts {
            for (j, k) in ks.iter().enumerate() {
                assert!(p[j] >= k.lo && p[j] <= k.hi);
            }
        }
        // First dimension: each of the n strata is hit exactly once (the LHS
        // guarantee), so the sampled values span the range without gaps.
        let lo = ks[0].lo;
        let span = ks[0].hi - ks[0].lo;
        let mut bins = vec![0u32; n];
        for p in &pts {
            let b = (((p[0] - lo) / span) * n as f64).floor() as usize;
            bins[b.min(n - 1)] += 1;
        }
        assert!(bins.iter().all(|&c| c == 1), "LHS should hit each stratum once: {bins:?}");
    }
}
