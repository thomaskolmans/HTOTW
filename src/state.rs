//! The mutable state of the simulated world, in **real, sourced units**.
//!
//! Domains (each a struct below):
//! * [`Human`] — population & population-level human development (life
//!   expectancy, education, health).
//! * [`Society`] — the *lived experience*: wellbeing, social support, freedom,
//!   livability, work. This is the "society is more than the environment"
//!   layer the model gives first-class weight.
//! * [`Economy`] — output, capital, productivity, jobs, inequality, the budget.
//! * [`Environment`] — climate, pollution, resources, forests.
//! * [`Animal`] — the biosphere (biodiversity intactness, wildlife abundance).
//! * [`Governance`] — the polity: state capacity, corruption, legitimacy,
//!   democracy, polarization and the governing ideology. Policy is produced and
//!   *implemented* through this domain (see `docs/GOVERNANCE.md`).
//!
//! Units and baseline values are documented in `docs/RESEARCH.md` and cited at
//! each field. Indices are dimensionless `[0,1]`; everything else carries a real
//! unit. The dynamics keep indices clamped so feedback loops cannot diverge.

use crate::util::clamp01;

/// Population & human-development domain.
#[derive(Debug, Clone, PartialEq)]
pub struct Human {
    /// Population, in **billions of people**. Baseline 8.2 (UN WPP 2024).
    pub population: f64,
    /// Life expectancy at birth, in **years**. Baseline 73.3 (UN WPP 2024).
    /// Driven by a Preston-curve relation to income plus pollution/health.
    pub life_expectancy: f64,
    /// Educational-attainment index (HDI-style) — `[0,1]`. Baseline ~0.63.
    pub education: f64,
    /// Population health index — `[0,1]`, derived from life expectancy and
    /// environmental burden. Baseline ~0.70.
    pub health: f64,
}

impl Human {
    /// Population in billions × 1 = billions (kept for clarity at call sites).
    pub fn billions(&self) -> f64 {
        self.population
    }
}

/// The lived experience of society — wellbeing and its WHR drivers, plus
/// work and livability. Given first-class weight alongside the economy.
#[derive(Debug, Clone, PartialEq)]
pub struct Society {
    /// Subjective wellbeing on the **Cantril ladder, 0–10** (World Happiness
    /// Report). Global mean ≈ 5.5.
    pub wellbeing: f64,
    /// Social support / community strength — `[0,1]` (a WHR driver).
    pub social_support: f64,
    /// Freedom to make life choices — `[0,1]` (a WHR driver).
    pub freedom: f64,
    /// Livability — `[0,1]` composite of housing/services/safety/environment
    /// quality as actually experienced day to day.
    pub livability: f64,
    /// Average working time relative to a 40h baseline — `[0.5,1.2]`. Lower =
    /// more leisure/time-affluence. A quality-of-work / time-use dimension.
    pub work_intensity: f64,
}

/// Economic domain (Solow/DICE-style).
#[derive(Debug, Clone, PartialEq)]
pub struct Economy {
    /// Annual output (GDP), in **trillions of international-$ (PPP)**.
    /// Baseline 195 (IMF WEO 2024).
    pub gdp: f64,
    /// Productive capital stock, in trillions int-$. Baseline ~546 (K/Y≈2.8).
    pub capital: f64,
    /// Total factor productivity scale; calibrated at init, grows endogenously.
    pub productivity: f64,
    /// Income inequality (Gini) — `[0,1]`. Baseline 0.39 (World Bank, median).
    pub gini: f64,
    /// Tax revenue as a share of GDP — `[0,1]`. Baseline ~0.175; capped by
    /// state capacity (low-capacity states cannot collect much). IMF/OECD.
    pub tax_rate: f64,
    /// Unemployment rate — `[0,1]`. Baseline 0.049 (ILO 2024).
    pub unemployment: f64,
    /// Public debt stock, in trillions int-$. Baseline ~179 (≈0.92×GDP, IMF).
    pub public_debt: f64,
}

impl Economy {
    /// GDP per capita in **international-$ (PPP)**, given the population.
    /// GDP is in trillions, population in billions ⇒ ×1000.
    pub fn gdp_per_capita(&self, human: &Human) -> f64 {
        if human.population.abs() < f64::EPSILON {
            0.0
        } else {
            self.gdp / human.population * 1_000.0
        }
    }

    /// Public debt as a fraction of GDP (the headline debt-to-GDP ratio).
    pub fn debt_ratio(&self) -> f64 {
        if self.gdp.abs() < f64::EPSILON {
            0.0
        } else {
            self.public_debt / self.gdp
        }
    }
}

/// Environmental / physical-Earth domain.
#[derive(Debug, Clone, PartialEq)]
pub struct Environment {
    /// Atmospheric CO₂, in **ppm**. Baseline 422.5 (Global Carbon Budget 2024).
    pub co2_ppm: f64,
    /// Temperature anomaly above 1850–1900, in **°C**. Baseline 1.09 (AR6).
    pub temp_anomaly: f64,
    /// Pollution burden (air/water/soil) — `[0,1]`. Baseline ~0.35.
    pub pollution: f64,
    /// Remaining non-renewable resource reserves — `[0,1]`. Baseline ~0.75.
    pub resource_reserves: f64,
    /// Forest cover as a fraction of land area — `[0,1]`. Baseline 0.31
    /// (FAO FRA 2020: 4.06 bn ha = 31% of land).
    pub forest_cover: f64,
    /// Carbon intensity of output relative to the start year — multiplier,
    /// starts 1.0; falls with clean-tech progress and decarbonisation policy.
    pub carbon_intensity: f64,
}

/// Biosphere domain.
#[derive(Debug, Clone, PartialEq)]
pub struct Animal {
    /// Biodiversity Intactness Index — `[0,1]`. Baseline 0.79 (safe boundary
    /// 0.90; Stockholm Resilience / Newbold et al.).
    pub biodiversity: f64,
    /// Living Planet Index (relative wild vertebrate abundance vs 1970=1.0) —
    /// `[0,1]`. Baseline 0.27 (WWF/ZSL 2024: −73% since 1970).
    pub wildlife_index: f64,
}

/// A three-axis ideology / policy orientation, each axis in `[-1, 1]`.
///
/// These describe *where a government stands* and *where a policy sits*; their
/// dot product measures alignment (used when a government decides what to enact;
/// see [`crate::governance`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ideology {
    /// Economic axis: `-1` free-market ↔ `+1` state-led / redistributive.
    pub economic: f64,
    /// Ecological axis: `-1` growth-first ↔ `+1` ecology-first.
    pub ecological: f64,
    /// Social axis: `-1` individualist ↔ `+1` solidaristic.
    pub social: f64,
}

impl Ideology {
    pub fn new(economic: f64, ecological: f64, social: f64) -> Self {
        Ideology {
            economic: economic.clamp(-1.0, 1.0),
            ecological: ecological.clamp(-1.0, 1.0),
            social: social.clamp(-1.0, 1.0),
        }
    }
    /// Centrist / technocratic neutral orientation.
    pub fn centrist() -> Self {
        Ideology { economic: 0.0, ecological: 0.0, social: 0.0 }
    }
    /// Alignment in `[-1,1]` between this orientation and a policy's position
    /// (normalised dot product over the three axes).
    pub fn alignment(&self, policy: &Ideology) -> f64 {
        let dot = self.economic * policy.economic
            + self.ecological * policy.ecological
            + self.social * policy.social;
        (dot / 3.0).clamp(-1.0, 1.0)
    }
}

/// Governance / political domain — the polity that *produces and implements*
/// policy. See `docs/RESEARCH.md` §5 for the sourced indicators behind each
/// field and `docs/GOVERNANCE.md` for the mechanics.
#[derive(Debug, Clone, PartialEq)]
pub struct Governance {
    /// State capacity / government effectiveness — `[0,1]` (World Bank WGI,
    /// rescaled from its z-score). Baseline ~0.55.
    pub state_capacity: f64,
    /// Corruption — `[0,1]`, `1 - CPI/100`. Baseline 0.57 (CPI global avg 43).
    pub corruption: f64,
    /// Political legitimacy / public trust — `[0,1]`. Baseline ~0.39 (OECD).
    pub legitimacy: f64,
    /// Liberal-democracy / accountability — `[0,1]` (V-Dem LDI). Baseline ~0.45.
    pub democracy: f64,
    /// Political polarization — `[0,1]`. Baseline ~0.45.
    pub polarization: f64,
    /// Political capital available to spend on reform — `[0,1]`.
    pub political_capital: f64,
    /// Current governing orientation (mutated by elections).
    pub orientation: Ideology,
    /// Years until the next election.
    pub years_to_election: u32,
    /// Length of an electoral term, in years.
    pub term_length: u32,
}

impl Governance {
    /// Implementation effectiveness in `[0.47, 0.90]`: the fraction of a
    /// policy's *intended* effect that actually reaches the world.
    ///
    /// Grounded in IMF (2023): public-investment efficiency loss runs from
    /// ~10% in clean, high-capacity states to ~53% in low-capacity, corrupt
    /// ones. We map `efficiency_loss = 0.10 + 0.43·(½(1−capacity) + ½·corruption)`
    /// and return `1 − efficiency_loss`.
    pub fn effectiveness(&self) -> f64 {
        let loss = 0.10
            + 0.43 * (0.5 * (1.0 - clamp01(self.state_capacity)) + 0.5 * clamp01(self.corruption));
        clamp01(1.0 - loss)
    }

    /// Whether an election falls this year.
    pub fn is_election_year(&self) -> bool {
        self.years_to_election == 0
    }
}

/// Derived, read-only composite scores summarising the whole world across **all**
/// pillars — ecological, social/livability, economic prosperity, and governance.
///
/// `overall` is the *geometric* mean of the pillars, so collapse in any one
/// (a dead biosphere, a failed state, mass immiseration) drags the whole score
/// down: you cannot trade a ruined pillar away against a strong one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Planet {
    pub ecological: f64,
    pub social: f64,
    pub prosperity: f64,
    pub governance: f64,
    pub overall: f64,
}

impl Planet {
    pub fn derive(state: &WorldState) -> Planet {
        let env_quality = clamp01(
            0.40 * (1.0 - state.environment.pollution)
                + 0.30 * state.environment.forest_cover / 0.5 // 0.5 ≈ "healthy" cover
                + 0.30 * state.environment.resource_reserves,
        );
        let ecological = clamp01(
            0.45 * env_quality
                + 0.35 * state.animal.biodiversity
                + 0.20 * state.animal.wildlife_index,
        );

        // Social pillar: wellbeing (0–10 → 0–1) tempered by equity & livability.
        let equity = clamp01(1.0 - state.economy.gini);
        let social = clamp01(
            0.50 * (state.society.wellbeing / 10.0)
                + 0.25 * state.society.livability
                + 0.25 * equity,
        );

        let prosperity = prosperity_index(state.economy.gdp_per_capita(&state.human));

        let governance = clamp01(
            0.35 * state.governance.state_capacity
                + 0.25 * (1.0 - state.governance.corruption)
                + 0.20 * state.governance.legitimacy
                + 0.20 * state.governance.democracy,
        );

        let overall = (ecological.max(1e-6)
            * social.max(1e-6)
            * prosperity.max(1e-6)
            * governance.max(1e-6))
        .powf(0.25);

        Planet {
            ecological,
            social,
            prosperity,
            governance,
            overall: clamp01(overall),
        }
    }
}

/// Map GDP per capita (international-$ PPP) to a `[0,1]` prosperity index with
/// strong diminishing returns (log-saturating). Shared by the dynamics, the
/// wellbeing model and the [`Planet`] composites so they all agree on "rich".
///
/// Calibrated so that ~$5k ≈ 0.3, ~$24k (world avg) ≈ 0.58, ~$65k ≈ 0.85.
pub fn prosperity_index(gdp_per_capita: f64) -> f64 {
    let pc = gdp_per_capita.max(0.0);
    let scaled = (1.0 + pc / 5_000.0).ln() / (1.0 + 100_000.0 / 5_000.0f64).ln();
    clamp01(scaled)
}

/// The complete world state at one instant.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldState {
    pub year: u32,
    pub human: Human,
    pub society: Society,
    pub economy: Economy,
    pub environment: Environment,
    pub animal: Animal,
    pub governance: Governance,
}

impl WorldState {
    pub fn planet(&self) -> Planet {
        Planet::derive(self)
    }

    /// GDP per capita convenience accessor.
    pub fn gdp_per_capita(&self) -> f64 {
        self.economy.gdp_per_capita(&self.human)
    }

    /// Clamp every index field into range and floor physical quantities at zero.
    pub fn sanitize(&mut self) {
        self.human.population = self.human.population.max(0.0);
        self.human.life_expectancy = self.human.life_expectancy.clamp(20.0, 100.0);
        self.human.education = clamp01(self.human.education);
        self.human.health = clamp01(self.human.health);

        self.society.wellbeing = self.society.wellbeing.clamp(0.0, 10.0);
        self.society.social_support = clamp01(self.society.social_support);
        self.society.freedom = clamp01(self.society.freedom);
        self.society.livability = clamp01(self.society.livability);
        self.society.work_intensity = self.society.work_intensity.clamp(0.5, 1.2);

        self.economy.gdp = self.economy.gdp.max(0.0);
        self.economy.capital = self.economy.capital.max(0.0);
        self.economy.productivity = self.economy.productivity.max(0.0);
        self.economy.gini = clamp01(self.economy.gini);
        self.economy.tax_rate = clamp01(self.economy.tax_rate);
        self.economy.unemployment = self.economy.unemployment.clamp(0.0, 1.0);

        self.environment.co2_ppm = self.environment.co2_ppm.max(0.0);
        self.environment.pollution = clamp01(self.environment.pollution);
        self.environment.resource_reserves = clamp01(self.environment.resource_reserves);
        self.environment.forest_cover = clamp01(self.environment.forest_cover);
        self.environment.carbon_intensity = self.environment.carbon_intensity.max(0.0);

        self.animal.biodiversity = clamp01(self.animal.biodiversity);
        self.animal.wildlife_index = clamp01(self.animal.wildlife_index);

        self.governance.state_capacity = clamp01(self.governance.state_capacity);
        self.governance.corruption = clamp01(self.governance.corruption);
        self.governance.legitimacy = clamp01(self.governance.legitimacy);
        self.governance.democracy = clamp01(self.governance.democracy);
        self.governance.polarization = clamp01(self.governance.polarization);
        self.governance.political_capital = clamp01(self.governance.political_capital);
    }
}

#[cfg(test)]
mod tests {
    use crate::scenario::Scenario;

    #[test]
    fn baseline_gdp_per_capita_is_realistic() {
        let s = Scenario::baseline_2025().initial_state();
        let pc = s.gdp_per_capita();
        // ~$23-24k PPP (IMF: $194.6T / 8.2bn).
        assert!((20_000.0..27_000.0).contains(&pc), "gdp/capita off: {pc}");
    }

    #[test]
    fn planet_composites_in_range() {
        let s = Scenario::baseline_2025().initial_state();
        let p = s.planet();
        for v in [p.ecological, p.social, p.prosperity, p.governance, p.overall] {
            assert!((0.0..=1.0).contains(&v), "composite out of range: {v}");
        }
    }

    #[test]
    fn effectiveness_matches_imf_efficiency_loss_range() {
        let mut s = Scenario::baseline_2025().initial_state();
        // Clean, high-capacity state ≈ 90% effective (10% loss).
        s.governance.state_capacity = 1.0;
        s.governance.corruption = 0.0;
        assert!((s.governance.effectiveness() - 0.90).abs() < 1e-9);
        // Failed, corrupt state ≈ 47% effective (53% loss).
        s.governance.state_capacity = 0.0;
        s.governance.corruption = 1.0;
        assert!((s.governance.effectiveness() - 0.47).abs() < 1e-9);
    }

    #[test]
    fn overall_is_geometric_so_a_dead_pillar_dominates() {
        let mut s = Scenario::baseline_2025().initial_state();
        s.animal.biodiversity = 0.0;
        s.animal.wildlife_index = 0.0;
        s.environment.pollution = 1.0;
        s.environment.forest_cover = 0.0;
        s.environment.resource_reserves = 0.0;
        let p = s.planet();
        assert!(p.ecological < 0.05, "ecological should be crushed: {}", p.ecological);
        assert!(p.overall < 0.25, "geometric mean should be dragged down: {}", p.overall);
    }
}
