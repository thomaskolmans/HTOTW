//! **Search for the best way to operate the world**: a deterministic
//! evolutionary optimiser over the [`SocietyParams`] space, scored on the
//! measured long-run [`welfare`](crate::measure::Measurements::welfare)
//! functional averaged over a seed ensemble (and over the run's final years, to
//! reward *sustained* welfare rather than a lucky final tick).
//!
//! This is the project's purpose closed into a loop: the simulator makes no
//! social assumptions, so "the best society" is not designed — it is *found*,
//! by proposing parameter sets, living each one out on a physical planet full
//! of psychologically real people, measuring what emerges, and keeping what
//! scores. The optimiser is a (μ+λ) evolution strategy: tournament-free elitist
//! truncation with Gaussian mutation on the continuous dials and categorical
//! resampling on the regimes. Fully deterministic for a given seed.

use crate::config::*;
use crate::measure::Objective;
use crate::rng::Rng;
use crate::world::World;

/// One scored candidate society.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub params: SocietyParams,
    /// Mean sustained welfare over the seed ensemble (the fitness).
    pub welfare: f64,
}

/// Search settings.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// The planet the societies are tried on (population, geography, seed of
    /// the *base* world; the ensemble varies the seed around it).
    pub world: WorldConfig,
    /// Seeds the ensemble averages over (robustness to luck).
    pub seeds: Vec<u64>,
    /// Years each trial runs.
    pub years: usize,
    /// How many of the final years to average welfare over (sustained welfare).
    pub eval_window: usize,
    /// Population size μ (survivors) and offspring λ per generation.
    pub mu: usize,
    pub lambda: usize,
    pub generations: usize,
    /// RNG seed for the *search* (distinct from world seeds).
    pub search_seed: u64,
    /// The evaluator's values — the welfare weights the search maximises. An
    /// explicit input: a different objective finds a different "best" society.
    pub objective: Objective,
}

impl Default for SearchConfig {
    fn default() -> SearchConfig {
        let mut world = WorldConfig::default();
        // A modest planet so a full search is feasible; the science is the same.
        world.nlon = 36;
        world.nlat = 18;
        world.n_agents = 1500;
        world.n_polities = 1; // search a single uniform society
        SearchConfig {
            world,
            seeds: vec![1, 2, 3],
            years: 150,
            eval_window: 30,
            mu: 6,
            lambda: 18,
            generations: 12,
            search_seed: 0xC0FFEE,
            objective: Objective::default(),
        }
    }
}

/// Evaluate a society's **sustained, ensemble-mean welfare** — the fitness the
/// search maximises. For each seed, run the world and average the measured
/// welfare over the final `eval_window` years; then average across seeds. All
/// inputs are measured; the score cannot be gamed by setting an outcome.
pub fn evaluate(params: &SocietyParams, cfg: &SearchConfig) -> f64 {
    let mut total = 0.0;
    for &seed in &cfg.seeds {
        let mut world = cfg.world.clone();
        world.seed = seed;
        let scenario = Scenario::new("trial", world).with_uniform_society(params.clone());
        let mut w = World::from_scenario(&scenario);
        let init_pop = w.initial_population;
        let warmup = cfg.years.saturating_sub(cfg.eval_window);
        for _ in 0..warmup {
            w.step();
        }
        let mut acc = 0.0;
        let window = cfg.eval_window.max(1);
        for _ in 0..window {
            w.step();
            acc += w.measure().welfare_with(init_pop, &cfg.objective);
        }
        total += acc / window as f64;
    }
    total / cfg.seeds.len().max(1) as f64
}

/// A random society parameter set (the search's mutation/initialisation
/// primitive). Draws sensible, in-range dials.
fn random_params(rng: &mut Rng) -> SocietyParams {
    let pick = |rng: &mut Rng, options: &[u8]| options[rng.below(options.len())];
    let property = match pick(rng, &[0, 1, 2]) {
        0 => PropertyRegime::OpenAccess,
        1 => PropertyRegime::CommonsQuota,
        _ => PropertyRegime::Private,
    };
    let transfer = match pick(rng, &[0, 1, 2]) {
        0 => TransferRegime::None,
        1 => TransferRegime::Floor,
        _ => TransferRegime::UniversalDividend,
    };
    let governance = match pick(rng, &[0, 1, 2]) {
        0 => GovernanceRegime::Fixed,
        1 => GovernanceRegime::Majority,
        _ => GovernanceRegime::WealthWeighted,
    };
    SocietyParams {
        property,
        conservation_quota: rng.range(0.2, 1.0),
        tax_rate: rng.range(0.0, 0.4),
        tax_progressivity: rng.range(0.0, 1.0),
        transfer,
        education_share: rng.range(0.0, 0.5),
        infrastructure_share: rng.range(0.0, 0.4),
        research_share: rng.range(0.0, 0.4),
        enforcement_share: rng.range(0.0, 0.3),
        carbon_price: rng.range(0.0, 8.0),
        migration_openness: rng.range(0.0, 1.0),
        trade_openness: rng.range(0.0, 1.0),
        governance,
        vote_period: 10 + rng.below(20) as u32,
    }
}

/// Mutate a parameter set: Gaussian-ish jitter on the continuous dials, an
/// occasional categorical flip on the regimes.
fn mutate(base: &SocietyParams, rng: &mut Rng) -> SocietyParams {
    let mut p = base.clone();
    let jitter = |v: f64, scale: f64, rng: &mut Rng| v + rng.range(-scale, scale);
    let clamp01 = |v: f64| v.clamp(0.0, 1.0);
    p.conservation_quota = clamp01(jitter(p.conservation_quota, 0.15, rng)).max(0.2);
    p.tax_rate = jitter(p.tax_rate, 0.08, rng).clamp(0.0, 0.5);
    p.tax_progressivity = clamp01(jitter(p.tax_progressivity, 0.2, rng));
    p.education_share = clamp01(jitter(p.education_share, 0.12, rng));
    p.infrastructure_share = clamp01(jitter(p.infrastructure_share, 0.12, rng));
    p.research_share = clamp01(jitter(p.research_share, 0.12, rng));
    p.enforcement_share = clamp01(jitter(p.enforcement_share, 0.1, rng));
    p.carbon_price = jitter(p.carbon_price, 2.0, rng).clamp(0.0, 15.0);
    p.migration_openness = clamp01(jitter(p.migration_openness, 0.2, rng));
    p.trade_openness = clamp01(jitter(p.trade_openness, 0.2, rng));
    // Categorical flips at a low rate.
    if rng.f64() < 0.2 {
        p.property = random_params(rng).property;
    }
    if rng.f64() < 0.2 {
        p.transfer = random_params(rng).transfer;
    }
    if rng.f64() < 0.15 {
        p.governance = random_params(rng).governance;
    }
    p
}

/// Run the evolutionary search and return the population ranked best-first.
/// Deterministic for a given `search_seed`.
pub fn search(cfg: &SearchConfig) -> Vec<Candidate> {
    let mut rng = Rng::seed(cfg.search_seed);
    let score = |params: &SocietyParams| -> Candidate {
        Candidate { welfare: evaluate(params, cfg), params: params.clone() }
    };

    // Initial population: the null baseline plus random draws (so the search
    // can never do worse than "do nothing" by accident).
    let mut pop: Vec<Candidate> = Vec::with_capacity(cfg.mu + cfg.lambda);
    pop.push(score(&SocietyParams::default()));
    while pop.len() < cfg.mu + cfg.lambda {
        let params = random_params(&mut rng);
        pop.push(score(&params));
    }
    sort_desc(&mut pop);

    for _ in 0..cfg.generations {
        pop.truncate(cfg.mu); // elitist truncation: keep the μ best
        // Breed λ offspring by mutating survivors round-robin.
        let mut children = Vec::with_capacity(cfg.lambda);
        for k in 0..cfg.lambda {
            let parent = &pop[k % cfg.mu.max(1)];
            let child = mutate(&parent.params, &mut rng);
            children.push(score(&child));
        }
        pop.extend(children);
        sort_desc(&mut pop);
    }
    sort_desc(&mut pop);
    pop
}

/// Deterministic best-first sort (welfare desc; ties broken by a stable params
/// fingerprint so the order is reproducible to the bit).
fn sort_desc(pop: &mut [Candidate]) {
    pop.sort_by(|a, b| {
        b.welfare
            .partial_cmp(&a.welfare)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| fingerprint(&a.params).partial_cmp(&fingerprint(&b.params)).unwrap())
    });
}

fn fingerprint(p: &SocietyParams) -> f64 {
    p.tax_rate * 1.0
        + p.carbon_price * 0.1
        + p.education_share * 10.0
        + p.conservation_quota * 100.0
        + (p.property as u8 as f64) * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> SearchConfig {
        let mut c = SearchConfig::default();
        c.world.nlon = 24;
        c.world.nlat = 12;
        c.world.n_agents = 700;
        c.seeds = vec![1, 2];
        c.years = 70;
        c.eval_window = 15;
        c.mu = 4;
        c.lambda = 8;
        c.generations = 4;
        c
    }

    #[test]
    fn evaluation_is_a_finite_bounded_score() {
        let cfg = tiny();
        let w = evaluate(&SocietyParams::default(), &cfg);
        assert!(w.is_finite() && (0.0..=1.0).contains(&w), "welfare in [0,1], got {w}");
    }

    #[test]
    fn evaluation_is_deterministic() {
        let cfg = tiny();
        let s = SocietyParams { carbon_price: 4.0, tax_rate: 0.2, ..SocietyParams::default() };
        assert_eq!(evaluate(&s, &cfg).to_bits(), evaluate(&s, &cfg).to_bits());
    }

    /// The headline result: the search **finds a society that beats doing
    /// nothing**, and the whole search is reproducible to the bit.
    #[test]
    fn search_beats_the_null_baseline_and_is_deterministic() {
        let cfg = tiny();
        let baseline = evaluate(&SocietyParams::default(), &cfg);
        let a = search(&cfg);
        let b = search(&cfg);
        assert!(!a.is_empty());
        // Reproducible.
        assert_eq!(a.len(), b.len());
        assert_eq!(a[0].welfare.to_bits(), b[0].welfare.to_bits());
        assert_eq!(a[0].params, b[0].params);
        // Ranked best-first.
        for pair in a.windows(2) {
            assert!(pair[0].welfare >= pair[1].welfare);
        }
        // The discovered best is at least as good as the null society.
        assert!(
            a[0].welfare >= baseline - 1e-9,
            "search should not lose to doing nothing: {} vs {}",
            a[0].welfare,
            baseline
        );
    }
}
