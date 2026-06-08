//! The simulation engine: wire a [`Scenario`] to a [`PolicyStack`] and/or an
//! endogenous [`Government`], advance year by year, and record the full
//! time-series of [`Snapshot`]s.
//!
//! Two modes, which can be combined:
//! * **Manual** — you stack policies with [`Simulation::add_policy`]. Their
//!   levers are applied directly (you are specifying the intervention exactly).
//! * **Governed** — you install a [`Government`] with
//!   [`Simulation::set_government`]. It legislates each year, and the *combined*
//!   policy effect is gated by the polity's implementation effectiveness, so a
//!   good policy underperforms in a weak or corrupt state.
//!
//! ```
//! use society_sim::prelude::*;
//! let mut sim = Simulation::new(Scenario::baseline_2025());
//! sim.set_government(Box::new(ArchetypeGovernment::technocracy()));
//! let history = sim.run(50);
//! assert_eq!(history.len(), 51);
//! ```

use crate::dynamics::step;
use crate::governance::Government;
use crate::policy::{Policy, PolicyStack};
use crate::scenario::Scenario;
use crate::state::{Animal, Economy, Environment, Governance, Human, Planet, Society, WorldState};

/// A recorded point in the time-series: the full [`WorldState`] split by domain
/// plus the derived [`Planet`] composites.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub year: u32,
    pub human: Human,
    pub society: Society,
    pub economy: Economy,
    pub environment: Environment,
    pub animal: Animal,
    pub governance: Governance,
    pub planet: Planet,
    /// GDP per capita (international-$ PPP), recorded for convenience.
    pub gdp_per_capita: f64,
}

impl Snapshot {
    fn from_state(state: &WorldState) -> Snapshot {
        Snapshot {
            year: state.year,
            human: state.human.clone(),
            society: state.society.clone(),
            economy: state.economy.clone(),
            environment: state.environment.clone(),
            animal: state.animal.clone(),
            governance: state.governance.clone(),
            planet: state.planet(),
            gdp_per_capita: state.gdp_per_capita(),
        }
    }
}

/// Orchestrates a run.
pub struct Simulation {
    state: WorldState,
    /// Manually-stacked policies (applied with full effectiveness).
    policies: PolicyStack,
    /// Optional endogenous government (governed mode).
    government: Option<Box<dyn Government>>,
    /// Count of policies the government has enacted as of the last tick.
    last_enacted: usize,
    pub scenario_name: String,
}

impl Simulation {
    pub fn new(scenario: Scenario) -> Simulation {
        Simulation {
            state: scenario.initial_state(),
            scenario_name: scenario.name.clone(),
            policies: PolicyStack::new(),
            government: None,
            last_enacted: 0,
        }
    }

    /// Add a manually-specified policy (full effectiveness).
    pub fn add_policy(&mut self, policy: Box<dyn Policy>) -> &mut Self {
        self.policies.push(policy);
        self
    }

    /// Install an endogenous government (enables governed mode: its policies are
    /// gated by implementation effectiveness).
    pub fn set_government(&mut self, government: Box<dyn Government>) -> &mut Self {
        self.government = Some(government);
        self
    }

    pub fn state(&self) -> &WorldState {
        &self.state
    }

    pub fn policies(&self) -> &PolicyStack {
        &self.policies
    }

    /// Name of the installed government, if any.
    pub fn government_name(&self) -> Option<&str> {
        self.government.as_ref().map(|g| g.name())
    }

    /// Number of policies the government has enacted as of the last tick
    /// (governed mode).
    pub fn enacted_count(&self) -> usize {
        self.last_enacted
    }

    /// Advance the world by exactly one year.
    pub fn tick(&mut self) {
        let year = self.state.year;
        let mut eff = crate::effects::PolicyEffects::neutral();

        // Manual policies: full effectiveness.
        for p in self.policies.iter() {
            if p.is_active(year) {
                p.apply(year, &self.state, &mut eff);
            }
        }

        // Governed policies: gated by implementation effectiveness.
        if let Some(gov) = self.government.as_mut() {
            let active = gov.legislate(year, &self.state);
            self.last_enacted = active.len();
            let mut gov_eff = crate::effects::PolicyEffects::neutral();
            for p in active {
                if p.is_active(year) {
                    p.apply(year, &self.state, &mut gov_eff);
                }
            }
            gov_eff.scale_effectiveness(self.state.governance.effectiveness());
            merge_into(&mut eff, &gov_eff);
        }

        let mut next = step(&self.state, &eff);
        next.sanitize();
        self.state = next;
    }

    /// Run for `years` years; returns the initial snapshot plus one per year.
    pub fn run(&mut self, years: u32) -> Vec<Snapshot> {
        let mut history = Vec::with_capacity(years as usize + 1);
        history.push(Snapshot::from_state(&self.state));
        for _ in 0..years {
            self.tick();
            history.push(Snapshot::from_state(&self.state));
        }
        history
    }
}

/// Combine governed effects into the accumulator (additive levers add,
/// multiplicative levers multiply) — preserving stacking semantics across the
/// manual and governed sources.
fn merge_into(acc: &mut crate::effects::PolicyEffects, other: &crate::effects::PolicyEffects) {
    acc.carbon_intensity_mult *= other.carbon_intensity_mult;
    acc.growth_mult *= other.growth_mult;
    acc.pollution_abatement += other.pollution_abatement;
    acc.reforestation += other.reforestation;
    acc.resource_efficiency += other.resource_efficiency;
    acc.savings_rate_add += other.savings_rate_add;
    acc.tax_rate_add += other.tax_rate_add;
    acc.redistribution += other.redistribution;
    acc.education_investment += other.education_investment;
    acc.health_investment += other.health_investment;
    acc.social_support_boost += other.social_support_boost;
    acc.freedom_boost += other.freedom_boost;
    acc.livability_boost += other.livability_boost;
    acc.work_reduction += other.work_reduction;
    acc.conservation_effort += other.conservation_effort;
    acc.anti_corruption += other.anti_corruption;
    acc.capacity_building += other.capacity_building;
    acc.democratic_reform += other.democratic_reform;
    acc.spending += other.spending;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policies::CarbonTax;

    #[test]
    fn run_records_initial_plus_one_per_year() {
        let mut sim = Simulation::new(Scenario::baseline_2025());
        let h = sim.run(10);
        assert_eq!(h.len(), 11);
        assert_eq!(h[0].year, 2025);
        assert_eq!(h[10].year, 2035);
    }

    #[test]
    fn carbon_tax_lowers_emissions_versus_baseline() {
        let bau = Simulation::new(Scenario::baseline_2025()).run(40);
        let mut taxed = Simulation::new(Scenario::baseline_2025());
        taxed.add_policy(Box::new(CarbonTax::new(2025, 0.8)));
        let taxed_hist = taxed.run(40);
        assert!(
            taxed_hist.last().unwrap().environment.co2_ppm < bau.last().unwrap().environment.co2_ppm
        );
    }
}
