//! Endogenous governance: a simulated **government that produces policy**.
//!
//! This is the heart of the project's central hypothesis — that *how a society
//! is governed* determines which policies get made, how well they are
//! implemented, and therefore what outcomes are even reachable. Instead of the
//! analyst hand-stacking policies, a [`Government`] actor observes the world and
//! *legislates* each year, constrained by:
//!
//! * **Ideology / mandate** — it favours policies aligned with its governing
//!   [`Ideology`], which elections shift toward salient problems (in democracies
//!   more than autocracies — see [`crate::dynamics`]).
//! * **Time horizon** — short-termist governments discount slow-payoff policies
//!   (climate, biodiversity, education), modelling electoral myopia
//!   (Klomp & de Haan 2013; see `docs/RESEARCH.md` §5).
//! * **Fiscal space & political capital** — costly reforms need capital and a
//!   tolerable debt position.
//!
//! Whatever it enacts is then *implemented* only as well as the state allows:
//! the simulation gates the combined effect by
//! [`Governance::effectiveness`](crate::state::Governance::effectiveness)
//! (IMF 2023: 10–53% of a policy's value is lost to weak capacity/corruption).
//!
//! The same mechanism lets you compare *forms of government* — run the same
//! world under a technocracy, a social democracy, a market-liberal government, a
//! populist one, or do-nothing, and see which produces a liveable society.

use crate::policy::Policy;
use crate::policies;
use crate::state::WorldState;

/// A government actor that decides the active policy set each year.
pub trait Government {
    fn name(&self) -> &str;
    /// Observe `state` and return the full set of policies currently in force.
    /// The government persists what it has enacted across years.
    fn legislate(&mut self, year: u32, state: &WorldState) -> &[Box<dyn Policy>];
}

/// One option on a government's menu: a policy it *could* enact, with the data
/// needed to decide whether to.
struct MenuItem {
    name: &'static str,
    /// Primary parameter (strength or share-of-GDP) used when enacted.
    param: f64,
    /// Rough fiscal cost as a share of GDP (for affordability scoring).
    fiscal_cost: f64,
    /// Payoff horizon: 1.0 = benefits arrive slowly (climate), 0.0 = immediate.
    payoff_horizon: f64,
    /// How salient/needed this policy is *right now*, given the world state.
    demand: fn(&WorldState) -> f64,
}

fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

/// The standard policy menu shared by all archetypes.
fn standard_menu() -> Vec<MenuItem> {
    vec![
        MenuItem { name: "carbon-tax", param: 0.7, fiscal_cost: 0.0, payoff_horizon: 1.0,
            demand: |s| clamp01(0.5 * clamp01((s.environment.temp_anomaly - 1.09) / 1.5) + 0.5 * s.environment.pollution) },
        MenuItem { name: "green-investment", param: 0.03, fiscal_cost: 0.03, payoff_horizon: 0.8,
            demand: |s| clamp01(0.6 * clamp01((s.environment.temp_anomaly - 1.09) / 1.5) + 0.4 * s.environment.pollution) },
        MenuItem { name: "circular-economy", param: 0.6, fiscal_cost: 0.0, payoff_horizon: 0.7,
            demand: |s| clamp01(1.0 - s.environment.resource_reserves) },
        MenuItem { name: "reforestation", param: 0.7, fiscal_cost: 0.005, payoff_horizon: 0.9,
            demand: |s| clamp01(0.5 * (0.31 - s.environment.forest_cover).max(0.0) / 0.31 + 0.5 * (1.0 - s.animal.biodiversity)) },
        MenuItem { name: "conservation-program", param: 0.8, fiscal_cost: 0.004, payoff_horizon: 0.9,
            demand: |s| clamp01(1.0 - s.animal.biodiversity) },
        MenuItem { name: "education-program", param: 0.03, fiscal_cost: 0.03, payoff_horizon: 0.8,
            demand: |s| clamp01(1.0 - s.human.education) },
        MenuItem { name: "healthcare-program", param: 0.03, fiscal_cost: 0.03, payoff_horizon: 0.4,
            demand: |s| clamp01(1.0 - s.human.health) },
        MenuItem { name: "social-housing", param: 0.03, fiscal_cost: 0.03, payoff_horizon: 0.2,
            demand: |s| clamp01(1.0 - s.society.livability) },
        MenuItem { name: "universal-basic-income", param: 0.4, fiscal_cost: 0.05, payoff_horizon: 0.1,
            demand: |s| clamp01(0.5 * s.economy.gini + 0.5 * s.economy.unemployment * 2.0) },
        MenuItem { name: "progressive-tax", param: 0.05, fiscal_cost: 0.0, payoff_horizon: 0.3,
            demand: |s| clamp01(s.economy.gini) },
        MenuItem { name: "shorter-workweek", param: 0.5, fiscal_cost: 0.0, payoff_horizon: 0.1,
            demand: |s| clamp01(1.0 - s.society.wellbeing / 10.0) },
        MenuItem { name: "anti-corruption", param: 0.7, fiscal_cost: 0.002, payoff_horizon: 0.5,
            demand: |s| clamp01(s.governance.corruption) },
        MenuItem { name: "capacity-building", param: 0.03, fiscal_cost: 0.03, payoff_horizon: 0.7,
            demand: |s| clamp01(1.0 - s.governance.state_capacity) },
        MenuItem { name: "democratic-reform", param: 0.6, fiscal_cost: 0.0, payoff_horizon: 0.6,
            demand: |s| clamp01(1.0 - s.governance.democracy) },
        MenuItem { name: "civil-liberties", param: 0.6, fiscal_cost: 0.0, payoff_horizon: 0.2,
            demand: |s| clamp01(1.0 - s.society.freedom) },
    ]
}

/// A configurable government archetype. The *behavioural traits* (horizon,
/// reformism, cost-sensitivity, ideology-weighting) are fixed per archetype;
/// the *ideology* it acts on is read live from the world's governance state,
/// which elections move toward whatever the electorate currently demands.
pub struct ArchetypeGovernment {
    label: String,
    /// Weight on long-run benefits, `[0,1]`. Low = myopic/short-termist.
    long_termism: f64,
    /// How many reforms it is willing to enact per year (× a 0–2 base).
    reformism: f64,
    /// Sensitivity to fiscal cost, `[0,1]`. High = austere.
    cost_sensitivity: f64,
    /// Weight on ideological alignment vs. evidence of need, `[0,1]`.
    align_weight: f64,
    /// Minimum score to enact a policy.
    threshold: f64,
    menu: Vec<MenuItem>,
    enacted_names: Vec<&'static str>,
    enacted: Vec<Box<dyn Policy>>,
}

impl ArchetypeGovernment {
    fn new(
        label: &str,
        long_termism: f64,
        reformism: f64,
        cost_sensitivity: f64,
        align_weight: f64,
        threshold: f64,
    ) -> Self {
        ArchetypeGovernment {
            label: label.to_string(),
            long_termism,
            reformism,
            cost_sensitivity,
            align_weight,
            threshold,
            menu: standard_menu(),
            enacted_names: Vec::new(),
            enacted: Vec::new(),
        }
    }

    /// **Technocracy.** Long horizon, evidence-led (ignores ideology), reformist
    /// but cost-aware. Enacts what the data says is needed.
    pub fn technocracy() -> Self {
        Self::new("technocracy", 1.0, 0.9, 0.3, 0.10, 0.45)
    }
    /// **Social democracy.** Balanced horizon, reformist, redistribution-friendly.
    pub fn social_democracy() -> Self {
        Self::new("social-democracy", 0.7, 0.7, 0.4, 0.50, 0.48)
    }
    /// **Market-liberal.** Cautious, austere, ideology-weighted, fewer reforms.
    pub fn market_liberal() -> Self {
        Self::new("market-liberal", 0.6, 0.3, 0.9, 0.60, 0.58)
    }
    /// **Populist.** Myopic (discounts slow-payoff policy), reformist on visible
    /// wins, heavily ideology-driven.
    pub fn populist() -> Self {
        Self::new("populist", 0.2, 0.8, 0.5, 0.70, 0.45)
    }
    /// **Status quo / do-nothing.** Barely legislates.
    pub fn status_quo() -> Self {
        Self::new("status-quo", 0.5, 0.05, 0.5, 0.5, 0.95)
    }

    pub fn by_name(name: &str) -> Option<ArchetypeGovernment> {
        Some(match name {
            "technocracy" | "technocrat" => Self::technocracy(),
            "social-democracy" | "socdem" => Self::social_democracy(),
            "market-liberal" | "liberal" => Self::market_liberal(),
            "populist" => Self::populist(),
            "status-quo" | "do-nothing" => Self::status_quo(),
            _ => return None,
        })
    }

    pub fn all_names() -> &'static [&'static str] {
        &["technocracy", "social-democracy", "market-liberal", "populist", "status-quo"]
    }

    /// Score a menu item in the current world. Higher = more worth enacting.
    fn score(&self, item: &MenuItem, state: &WorldState) -> f64 {
        let demand = (item.demand)(state);
        // Short-termist governments discount slow-payoff policies.
        let horizon_discount = 1.0 - item.payoff_horizon * (1.0 - self.long_termism);
        let effective_demand = demand * horizon_discount;

        // Ideological fit with the *current* governing orientation.
        let position = policies::build(item.name, state.year, item.param)
            .map(|p| p.position())
            .unwrap_or_else(crate::state::Ideology::centrist);
        let alignment = state.governance.orientation.alignment(&position); // [-1,1]
        let align01 = (alignment + 1.0) / 2.0;

        let cost_norm = (item.fiscal_cost / 0.05).min(1.5);

        (1.0 - self.align_weight) * effective_demand + self.align_weight * align01
            - self.cost_sensitivity * cost_norm * 0.4
    }
}

impl Government for ArchetypeGovernment {
    fn name(&self) -> &str {
        &self.label
    }

    fn legislate(&mut self, year: u32, state: &WorldState) -> &[Box<dyn Policy>] {
        // Reforms are gated by political capital and fiscal room.
        let capital = state.governance.political_capital;
        let debt_ok = state.economy.debt_ratio() < 2.5;
        let max_new = if capital > 0.25 {
            (self.reformism * 2.0).round() as usize
        } else {
            0
        };

        if max_new > 0 {
            // Score everything not already enacted.
            let mut candidates: Vec<(f64, usize)> = self
                .menu
                .iter()
                .enumerate()
                .filter(|(_, it)| !self.enacted_names.contains(&it.name))
                .map(|(i, it)| (self.score(it, state), i))
                .filter(|(sc, i)| *sc > self.threshold && (debt_ok || self.menu[*i].fiscal_cost == 0.0))
                .collect();
            // Highest score first.
            candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

            for (_, idx) in candidates.into_iter().take(max_new) {
                let item = &self.menu[idx];
                if let Some(policy) = policies::build(item.name, year, item.param) {
                    self.enacted_names.push(item.name);
                    self.enacted.push(policy);
                }
            }
        }
        &self.enacted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::Scenario;
    use crate::sim::Simulation;

    #[test]
    fn technocracy_enacts_more_than_status_quo() {
        let mut tech = Simulation::new(Scenario::baseline_2025());
        tech.set_government(Box::new(ArchetypeGovernment::technocracy()));
        tech.run(30);
        let mut sq = Simulation::new(Scenario::baseline_2025());
        sq.set_government(Box::new(ArchetypeGovernment::status_quo()));
        sq.run(30);
        assert!(
            tech.enacted_count() > sq.enacted_count(),
            "technocracy ({}) should enact more than status-quo ({})",
            tech.enacted_count(),
            sq.enacted_count()
        );
    }

    #[test]
    fn governed_world_beats_do_nothing_overall() {
        let mut nothing = Simulation::new(Scenario::baseline_2025());
        nothing.set_government(Box::new(ArchetypeGovernment::status_quo()));
        let n = nothing.run(75).last().unwrap().planet.overall;

        let mut tech = Simulation::new(Scenario::baseline_2025());
        tech.set_government(Box::new(ArchetypeGovernment::technocracy()));
        let t = tech.run(75).last().unwrap().planet.overall;

        assert!(t > n, "technocracy should beat do-nothing overall: {t} vs {n}");
    }

    #[test]
    fn strong_institutions_lead_in_the_near_term() {
        // Over a near-term horizon (before the model's long-run convergence),
        // the same government delivers more from strong institutions than from a
        // weak, corrupt state — the implementation-effectiveness gate at work.
        // (The long-run over-convergence is a known limitation; see docs/MODEL.md
        // and the v2 "emergent engine" redesign in docs/PLAN.md.)
        let mut weak = Simulation::new(Scenario::fragile_world());
        weak.set_government(Box::new(ArchetypeGovernment::technocracy()));
        let w = weak.run(20).last().unwrap().planet.overall;

        let mut strong = Simulation::new(Scenario::strong_institutions());
        strong.set_government(Box::new(ArchetypeGovernment::technocracy()));
        let s = strong.run(20).last().unwrap().planet.overall;

        assert!(s > w, "strong institutions should lead near-term: {s} vs {w}");
    }
}
