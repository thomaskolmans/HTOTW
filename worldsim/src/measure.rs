//! **Instruments**: the read-only measurements taken off a finished or
//! in-progress [`World`](crate::world::World). Every macro quantity the project
//! cares about — population, GDP, the wealth **Gini**, life expectancy,
//! well-being, the global temperature anomaly, CO₂, the clean-energy share,
//! biodiversity, commons health — is computed *here* from raw state and **never
//! set anywhere**. The hard rule, enforced by the type system: instruments take
//! `&World` and cannot mutate it.

/// A snapshot of every headline emergent quantity in a given year.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurements {
    pub year: u64,
    /// Living population.
    pub population: usize,
    /// Total GDP this year (value of all output at emergent prices, numéraire).
    pub gdp: f64,
    /// GDP per capita.
    pub gdp_per_capita: f64,
    /// Mean personal wealth (savings).
    pub mean_wealth: f64,
    /// Wealth **Gini** in [0,1] — emergent inequality.
    pub wealth_gini: f64,
    /// Mean realised lifespan of those who have died (life expectancy), or NaN
    /// before the first death.
    pub life_expectancy: f64,
    /// Mean subjective well-being in [0,1].
    pub wellbeing: f64,
    /// Share of the population whose survival needs went unmet this year.
    pub deprivation_rate: f64,
    /// Mean human capital (skill).
    pub mean_skill: f64,

    // --- planetary / ecological ---
    /// Global-mean surface warming anomaly above pre-industrial (K).
    pub temp_anomaly: f64,
    /// Atmospheric CO₂ (ppm).
    pub co2: f64,
    /// Clean-energy share of energy produced, 0..1.
    pub clean_share: f64,
    /// Fraction of pristine standing biomass remaining (commons health), 0..1.
    pub commons_health: f64,
    /// Mean biodiversity index over land, 0..1.
    pub biodiversity: f64,
    /// Remaining fraction of the initial fossil endowment, 0..1.
    pub fossil_remaining: f64,
}

/// **The values an evaluator brings to the world** — the weights on the four
/// welfare pillars. *Values are not a property of the world*; they belong to
/// whoever is judging it, so they are an explicit, configurable input, never
/// baked in. Two observers with different `Objective`s will (rightly) crown
/// different societies: a headcount-maximiser prefers a populous hot planet, a
/// green objective prefers a small population on an intact biosphere. The
/// simulator's job is to measure honestly; the objective is where the value
/// judgement is made, out in the open.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Objective {
    pub wellbeing: f64,
    pub equity: f64,
    pub sustainability: f64,
    pub survival: f64,
}

impl Default for Objective {
    /// Equal weight on all four pillars (the balanced, no-substitutes default).
    fn default() -> Objective {
        Objective { wellbeing: 1.0, equity: 1.0, sustainability: 1.0, survival: 1.0 }
    }
}

impl Objective {
    /// A named preset, or `None` if unknown.
    pub fn preset(name: &str) -> Option<Objective> {
        Some(match name {
            "balanced" => Objective::default(),
            // Care chiefly about how many people live well now.
            "headcount" => Objective { wellbeing: 1.0, equity: 0.3, sustainability: 0.3, survival: 2.0 },
            // Care chiefly about the planet enduring.
            "green" => Objective { wellbeing: 0.5, equity: 0.5, sustainability: 3.0, survival: 0.5 },
            // Care chiefly about a fair distribution.
            "equity" => Objective { wellbeing: 1.0, equity: 3.0, sustainability: 0.5, survival: 0.5 },
            _ => return None,
        })
    }

    pub const PRESETS: [&'static str; 4] = ["balanced", "headcount", "green", "equity"];
}

impl Measurements {
    /// **The welfare functional**: a Sen/Stiglitz-style *no-substitutes*
    /// composite — the **weighted** geometric mean of four measured, normalised
    /// pillars, so collapsing any positively-weighted one (mass deprivation,
    /// total inequality, a wrecked biosphere, or a population crash) collapses
    /// the score. It is computed *from* measurements, so it can never be gamed
    /// by setting an outcome; the **weights are the evaluator's [`Objective`]**,
    /// an explicit input, not the simulator's opinion.
    ///
    /// - **well-being** — mean subjective well-being (already 0..1),
    /// - **equity** — `1 − Gini`,
    /// - **sustainability** — `geomean(commons_health, biodiversity, 1 − warming/6K)`,
    /// - **survival** — `1 − deprivation_rate`, times a population-floor factor
    ///   so an extinct or crashing world scores zero.
    pub fn welfare_with(&self, initial_population: usize, obj: &Objective) -> f64 {
        if self.population == 0 || initial_population == 0 {
            return 0.0;
        }
        let wellbeing = self.wellbeing.clamp(0.0, 1.0);
        let equity = (1.0 - self.wealth_gini).clamp(0.0, 1.0);
        let climate_ok = (1.0 - self.temp_anomaly / 6.0).clamp(0.0, 1.0);
        let sustainability = (self.commons_health.clamp(0.0, 1.0)
            * self.biodiversity.clamp(0.0, 1.0)
            * climate_ok)
            .powf(1.0 / 3.0);
        let pop_factor =
            (self.population as f64 / initial_population as f64).clamp(0.0, 1.0);
        let survival = (1.0 - self.deprivation_rate).clamp(0.0, 1.0) * pop_factor;

        let (a, b, c, d) = (obj.wellbeing, obj.equity, obj.sustainability, obj.survival);
        let wsum = (a + b + c + d).max(1e-9);
        // Weighted geometric mean: exp(Σ wᵢ ln xᵢ / Σ wᵢ). A zero in any
        // positively-weighted pillar still sends the whole score to zero.
        let term = |w: f64, x: f64| w * x.max(0.0).ln();
        ((term(a, wellbeing) + term(b, equity) + term(c, sustainability) + term(d, survival)) / wsum)
            .exp()
    }

    /// Welfare under the balanced default objective (back-compatible).
    pub fn welfare(&self, initial_population: usize) -> f64 {
        self.welfare_with(initial_population, &Objective::default())
    }
}

/// Gini coefficient of a slice of non-negative values (0 = perfect equality).
/// O(n log n) via the sorted-cumulative formula. Pure; takes a slice, sets
/// nothing.
pub fn gini(values: &[f64]) -> f64 {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    let n = v.len();
    if n == 0 {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = v.iter().sum();
    if sum <= 0.0 {
        return 0.0;
    }
    let mut cum = 0.0;
    for (i, &x) in v.iter().enumerate() {
        cum += (i as f64 + 1.0) * x;
    }
    (2.0 * cum) / (n as f64 * sum) - (n as f64 + 1.0) / n as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gini_endpoints() {
        assert!(gini(&[5.0, 5.0, 5.0, 5.0]).abs() < 1e-9, "equality ⇒ 0");
        // One person holds everything ⇒ Gini → (n−1)/n.
        let g = gini(&[0.0, 0.0, 0.0, 100.0]);
        assert!((g - 0.75).abs() < 1e-9, "max inequality, got {g}");
        assert_eq!(gini(&[]), 0.0);
    }

    fn base() -> Measurements {
        Measurements {
            year: 0,
            population: 1000,
            gdp: 1000.0,
            gdp_per_capita: 1.0,
            mean_wealth: 2.0,
            wealth_gini: 0.3,
            life_expectancy: 70.0,
            wellbeing: 0.7,
            deprivation_rate: 0.05,
            mean_skill: 1.2,
            temp_anomaly: 1.0,
            co2: 400.0,
            clean_share: 0.5,
            commons_health: 0.8,
            biodiversity: 0.7,
            fossil_remaining: 0.6,
        }
    }

    #[test]
    fn welfare_is_a_no_substitutes_composite() {
        let m = base();
        let w = m.welfare(1000);
        assert!(w > 0.0 && w < 1.0);
        // Each positively-weighted pillar can veto the score.
        assert!(Measurements { population: 0, ..m }.welfare(1000).abs() < 1e-12);
        assert!(Measurements { wealth_gini: 1.0, ..m }.welfare(1000).abs() < 1e-12);
        assert!(Measurements { commons_health: 0.0, ..m }.welfare(1000).abs() < 1e-12);
        assert!(Measurements { deprivation_rate: 1.0, ..m }.welfare(1000).abs() < 1e-12);
        assert!(Measurements { temp_anomaly: 6.0, ..m }.welfare(1000).abs() < 1e-12);
        assert!(Measurements { population: 100, ..m }.welfare(1000) < w);
    }

    /// **Values are an explicit input: different objectives crown different
    /// worlds.** A populous, hot, unequal world and a small, green, fair world
    /// are each preferred by the matching objective — the value judgement lives
    /// in the `Objective`, never in the simulator.
    #[test]
    fn objectives_change_the_ranking() {
        let populous_hot = Measurements {
            population: 2000, deprivation_rate: 0.1, wealth_gini: 0.7,
            temp_anomaly: 3.0, commons_health: 0.3, biodiversity: 0.2, ..base()
        };
        let small_green = Measurements {
            population: 400, deprivation_rate: 0.05, wealth_gini: 0.3,
            temp_anomaly: 0.2, commons_health: 0.95, biodiversity: 0.95, ..base()
        };
        let headcount = Objective::preset("headcount").unwrap();
        let green = Objective::preset("green").unwrap();
        // The headcount objective prefers the populous world...
        assert!(
            populous_hot.welfare_with(1000, &headcount)
                > small_green.welfare_with(1000, &headcount)
        );
        // ...and the green objective prefers the intact-biosphere world.
        assert!(
            small_green.welfare_with(1000, &green)
                > populous_hot.welfare_with(1000, &green)
        );
        assert!(Objective::preset("nonsense").is_none());
    }
}
