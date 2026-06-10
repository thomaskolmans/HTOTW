//! The **people**: individual humans in struct-of-arrays layout. Each carries a
//! location, age, accumulated **wealth** (savings in numéraire), a **skill**
//! (human capital, raised by education and learning-by-doing), heritable
//! **psychology** (patience, risk aversion, fairness, conformity — Phase-9
//! traits, now core drivers), a polity membership, and a subjective
//! **well-being** ledger.
//!
//! Nothing social is set here. Births, deaths, consumption and wealth are the
//! outcome of physical need, market access and individual decisions taken in
//! [`crate::world`]; this module is the data + the pure biological/psychological
//! response functions (mortality hazard, food need by age, fertility ceiling).

use crate::config::WorldConfig;
use crate::constants::*;
use crate::rng::Rng;

#[derive(Debug, Clone, Default)]
pub struct People {
    pub alive: Vec<bool>,
    pub age: Vec<u32>,
    pub cell: Vec<usize>,
    pub polity: Vec<u16>,
    /// Savings in numéraire (the wealth the Gini measures).
    pub wealth: Vec<f64>,
    /// Human capital ≥ 0.5; multiplies labour productivity.
    pub skill: Vec<f64>,
    /// Years of unmet survival need accumulated (drives deprivation hazard &
    /// out-migration pressure). Resets when needs are met.
    pub deprivation: Vec<f64>,
    /// The sector this person currently works in (`NO_JOB` if none yet, or not
    /// of working age). Chosen by the person from observed wages — there is no
    /// planner assigning labour.
    pub sector: Vec<u8>,
    /// Index of the person's (one tracked) parent, or `NO_PARENT`. Kin
    /// provisioning — parents feeding their own children — is a human
    /// universal (Kaplan 1996), modelled as biology, not as a policy.
    pub parent: Vec<usize>,
    // Heritable psychology in [0,1].
    pub patience: Vec<f64>,
    pub risk_aversion: Vec<f64>,
    pub fairness: Vec<f64>,
    pub conformity: Vec<f64>,
    /// Subjective well-being EMA in [0,1] (measured, never set).
    pub wellbeing: Vec<f64>,
}

/// Sentinel: no current job.
pub const NO_JOB: u8 = u8::MAX;
/// Sentinel: no tracked parent (the seed generation).
pub const NO_PARENT: usize = usize::MAX;

impl People {
    pub fn len(&self) -> usize {
        self.alive.len()
    }
    pub fn is_empty(&self) -> bool {
        self.alive.is_empty()
    }
    pub fn alive_count(&self) -> usize {
        self.alive.iter().filter(|&&a| a).count()
    }

    /// Seed an initial population at random habitable (land) cells, with equal
    /// wealth (so inequality is emergent), heterogeneous psychology drawn from
    /// the configured ranges, and a polity assigned by the cell's polity map.
    pub fn seed(cfg: &WorldConfig, habitable: &[usize], polity_of: &[u16], rng: &mut Rng) -> People {
        let mut p = People::default();
        if habitable.is_empty() {
            return p;
        }
        for _ in 0..cfg.n_agents {
            let cell = habitable[rng.below(habitable.len())];
            p.push(
                cell,
                polity_of[cell],
                // A start with some savings buffer (one year of food), equal
                // for all — any spread that appears later is measured, not set.
                1.0,
                1.0,
                [
                    cfg.patience.sample(rng),
                    cfg.risk_aversion.sample(rng),
                    cfg.fairness.sample(rng),
                    cfg.conformity.sample(rng),
                ],
                // Seed adults across the working-age range so the first
                // generation doesn't die or reproduce all at once.
                rng.below(40) as u32 + 15,
                NO_PARENT,
            );
        }
        p
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push(
        &mut self,
        cell: usize,
        polity: u16,
        wealth: f64,
        skill: f64,
        psyche: [f64; 4],
        age: u32,
        parent: usize,
    ) -> usize {
        let id = self.alive.len();
        self.alive.push(true);
        self.age.push(age);
        self.cell.push(cell);
        self.polity.push(polity);
        self.wealth.push(wealth);
        self.skill.push(skill.max(0.5));
        self.deprivation.push(0.0);
        self.sector.push(NO_JOB);
        self.parent.push(parent);
        self.patience.push(psyche[0]);
        self.risk_aversion.push(psyche[1]);
        self.fairness.push(psyche[2]);
        self.conformity.push(psyche[3]);
        self.wellbeing.push(0.5);
        id
    }

    /// Total yearly survival + comfort need of person `i` in numéraire, given
    /// the local temperature (heating fuel rises in the cold). Children and the
    /// elderly need less food; everyone needs water; goods scale with adulthood.
    pub fn need(&self, i: usize, local_temp: f64) -> Need {
        let a = self.age[i];
        let food = FOOD_NEED * need_scale(a);
        let water = WATER_NEED;
        let cold = (COMFORT_TEMP - local_temp).max(0.0);
        let fuel = FUEL_NEED_PER_K * cold;
        let goods = if a >= 15 { GOODS_NEED } else { GOODS_NEED * 0.5 };
        Need { food, water, fuel, goods }
    }

    /// Per-year all-cause mortality hazard: Gompertz–Makeham senescence + an
    /// infant penalty + deprivation (unmet need) + local heat stress. A pure
    /// biological response; the realised death is drawn against it.
    pub fn mortality_hazard(&self, i: usize, local_temp: f64) -> f64 {
        let a = self.age[i] as f64;
        // Knowledge (human capital) cuts the *environmental* mortality terms —
        // the background (Makeham) hazard and the infant penalty — via hygiene,
        // clean water handling, and care. This is the McKeown (1976) mechanism
        // behind the historical mortality decline: senescence (the Gompertz
        // term) stays biological and untouched. The demographic transition can
        // therefore EMERGE: education lowers mortality first, then fertility.
        let knowledge = 1.0 + 0.5 * (self.skill[i] - 1.0).max(0.0);
        let mut h = MAKEHAM / knowledge + GOMPERTZ_A * (GOMPERTZ_B * a).exp();
        if self.age[i] < 5 {
            h += INFANT_HAZARD / knowledge;
        }
        // Nonlinear in severity: mild chronic shortfall stunts rather than
        // kills; mortality climbs steeply only as deprivation deepens (famine
        // demography: excess deaths concentrate in severe episodes — Ó Gráda
        // 2007).
        let d = self.deprivation[i].min(2.0);
        h += DEPRIVATION_HAZARD * d * d / 2.0;
        if local_temp > HEAT_STRESS_TEMP {
            h += HEAT_STRESS_HAZARD * (local_temp - HEAT_STRESS_TEMP);
        }
        h.clamp(0.0, 1.0)
    }

    pub fn is_fertile(&self, i: usize) -> bool {
        (FERTILE_AGE.0..FERTILE_AGE.1).contains(&self.age[i])
    }
}

/// A person's decomposed yearly need (numéraire units).
#[derive(Debug, Clone, Copy)]
pub struct Need {
    pub food: f64,
    pub water: f64,
    pub fuel: f64,
    pub goods: f64,
}

impl Need {
    pub fn total(&self) -> f64 {
        self.food + self.water + self.fuel + self.goods
    }
    /// The survival-critical part (going without is lethal, unlike goods).
    pub fn survival(&self) -> f64 {
        self.food + self.water + self.fuel
    }
}

/// Food/energy requirement by age relative to an adult (children and the
/// elderly need less; FAO age-energy schedules, smoothed).
pub fn need_scale(age: u32) -> f64 {
    match age {
        0..=4 => 0.35,
        5..=14 => 0.65,
        15..=64 => 1.0,
        _ => 0.8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mortality_has_the_gompertz_shape() {
        let mut p = People::default();
        p.push(0, 0, 1.0, 1.0, [0.5; 4], 0, NO_PARENT);
        p.push(0, 0, 1.0, 1.0, [0.5; 4], 30, NO_PARENT);
        p.push(0, 0, 1.0, 1.0, [0.5; 4], 80, NO_PARENT);
        let h_inf = p.mortality_hazard(0, COMFORT_TEMP);
        let h_adult = p.mortality_hazard(1, COMFORT_TEMP);
        let h_old = p.mortality_hazard(2, COMFORT_TEMP);
        assert!(h_adult < h_inf, "infant hazard exceeds healthy adult");
        assert!(h_old > h_adult, "senescence raises old-age hazard");
        assert!(h_old < 1.0);
    }

    #[test]
    fn deprivation_and_heat_raise_hazard() {
        let mut p = People::default();
        p.push(0, 0, 1.0, 1.0, [0.5; 4], 30, NO_PARENT);
        let base = p.mortality_hazard(0, COMFORT_TEMP);
        p.deprivation[0] = 1.0;
        assert!(p.mortality_hazard(0, COMFORT_TEMP) > base, "starvation kills");
        p.deprivation[0] = 0.0;
        assert!(
            p.mortality_hazard(0, HEAT_STRESS_TEMP + 5.0) > base,
            "extreme heat kills"
        );
    }

    #[test]
    fn need_rises_in_the_cold_and_with_adulthood() {
        let mut p = People::default();
        p.push(0, 0, 1.0, 1.0, [0.5; 4], 30, NO_PARENT);
        p.push(0, 0, 1.0, 1.0, [0.5; 4], 2, NO_PARENT);
        let warm = p.need(0, COMFORT_TEMP);
        let cold = p.need(0, COMFORT_TEMP - 20.0);
        assert!(cold.fuel > warm.fuel, "cold demands heating fuel");
        assert!(p.need(1, COMFORT_TEMP).food < warm.food, "a toddler eats less");
    }

    #[test]
    fn seeding_is_equal_in_wealth_but_heterogeneous_in_psyche() {
        let cfg = WorldConfig::default();
        let habitable: Vec<usize> = (0..50).collect();
        let polity_of = vec![0u16; 100];
        let mut p = People::seed(&cfg, &habitable, &polity_of, &mut Rng::seed(3));
        assert_eq!(p.alive_count(), cfg.n_agents.min(p.len()));
        let w0 = p.wealth[0];
        assert!(p.wealth.iter().all(|&w| (w - w0).abs() < 1e-12), "equal wealth start");
        // Psychology varies.
        let spread = p.patience.iter().cloned().fold(0.0_f64, f64::max)
            - p.patience.iter().cloned().fold(1.0_f64, f64::min);
        assert!(spread > 0.1, "psychology should be heterogeneous");
        let _ = &mut p;
    }
}
