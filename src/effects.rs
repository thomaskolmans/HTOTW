//! The [`PolicyEffects`] accumulator: the single, well-defined interface between
//! *policies* and the *dynamics*.
//!
//! Policies never mutate the world directly; each only *adds to* (or *multiplies
//! into*) this neutral accumulator. The dynamics then read it once. This makes
//! stacking many policies order-independent and predictable (sums and
//! independent factors don't care about order). See `docs/ARCHITECTURE.md`.
//!
//! In *governed* mode every benefit lever is additionally scaled by the polity's
//! [implementation effectiveness](crate::state::Governance::effectiveness) via
//! [`PolicyEffects::scale_effectiveness`] — a "perfect" policy underperforms in
//! a weak or corrupt state, while its fiscal cost is still paid in full.
//!
//! Each lever notes whether it is *additive* (neutral `0.0`) or *multiplicative*
//! (neutral `1.0`).

/// Levers contributed by the active policies during a single year.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyEffects {
    // --- Environment ---------------------------------------------------------
    /// Multiplicative extra cut to carbon intensity this year (×, neutral 1.0).
    pub carbon_intensity_mult: f64,
    /// Additive pollution abatement (neutral 0.0).
    pub pollution_abatement: f64,
    /// Additive reforestation, in forest-cover fraction/yr (0.0).
    pub reforestation: f64,
    /// Additive resource-efficiency effort, slows depletion (0.0).
    pub resource_efficiency: f64,

    // --- Economy -------------------------------------------------------------
    /// Additive change to the savings/investment rate (0.0).
    pub savings_rate_add: f64,
    /// Additive change to the tax rate, share of GDP (0.0).
    pub tax_rate_add: f64,
    /// Redistribution strength — pulls Gini down (0.0).
    pub redistribution: f64,
    /// Direct multiplicative drag/boost on output (×, neutral 1.0).
    pub growth_mult: f64,

    // --- Human / society -----------------------------------------------------
    /// Public education investment, share of GDP (0.0).
    pub education_investment: f64,
    /// Public health investment, share of GDP (0.0).
    pub health_investment: f64,
    /// Boost to social support / community (0.0).
    pub social_support_boost: f64,
    /// Boost to freedom / civic agency (0.0).
    pub freedom_boost: f64,
    /// Boost to livability (housing, services, safety) (0.0).
    pub livability_boost: f64,
    /// Reduction in work intensity (more leisure/time) (0.0).
    pub work_reduction: f64,

    // --- Biosphere -----------------------------------------------------------
    /// Conservation effort — biosphere recovery & land relief (0.0).
    pub conservation_effort: f64,

    // --- Governance ----------------------------------------------------------
    /// Anti-corruption effort — lowers corruption (0.0).
    pub anti_corruption: f64,
    /// State-capacity building — raises capacity (0.0).
    pub capacity_building: f64,
    /// Democratic / institutional reform — raises accountability (0.0).
    pub democratic_reform: f64,

    // --- Budget --------------------------------------------------------------
    /// Net public spending demanded this year, in trillions int-$ (negative =
    /// net revenue). Flows into the budget/debt dynamics (0.0).
    pub spending: f64,
}

impl Default for PolicyEffects {
    fn default() -> Self {
        Self::neutral()
    }
}

impl PolicyEffects {
    /// A neutral accumulator that, applied alone, changes nothing.
    pub fn neutral() -> Self {
        PolicyEffects {
            carbon_intensity_mult: 1.0,
            pollution_abatement: 0.0,
            reforestation: 0.0,
            resource_efficiency: 0.0,
            savings_rate_add: 0.0,
            tax_rate_add: 0.0,
            redistribution: 0.0,
            growth_mult: 1.0,
            education_investment: 0.0,
            health_investment: 0.0,
            social_support_boost: 0.0,
            freedom_boost: 0.0,
            livability_boost: 0.0,
            work_reduction: 0.0,
            conservation_effort: 0.0,
            anti_corruption: 0.0,
            capacity_building: 0.0,
            democratic_reform: 0.0,
            spending: 0.0,
        }
    }

    /// Scale every *benefit* lever by implementation effectiveness `e ∈ [0,1]`,
    /// modelling that a weak/corrupt state delivers only a fraction of a
    /// policy's intended effect. The fiscal **cost (`spending`) and the tax
    /// lever are deliberately left unscaled** — you still pay for botched
    /// policy (and corruption wastes the rest). Multiplicative levers have their
    /// *deviation from 1* scaled. Grounded in IMF (2023); see `docs/RESEARCH.md`.
    pub fn scale_effectiveness(&mut self, e: f64) {
        let e = e.clamp(0.0, 1.0);
        // Multiplicative levers: shrink their deviation from the neutral 1.0.
        self.carbon_intensity_mult = 1.0 - (1.0 - self.carbon_intensity_mult) * e;
        self.growth_mult = 1.0 + (self.growth_mult - 1.0) * e;
        // Additive benefit levers.
        for lever in [
            &mut self.pollution_abatement,
            &mut self.reforestation,
            &mut self.resource_efficiency,
            &mut self.savings_rate_add,
            &mut self.redistribution,
            &mut self.education_investment,
            &mut self.health_investment,
            &mut self.social_support_boost,
            &mut self.freedom_boost,
            &mut self.livability_boost,
            &mut self.work_reduction,
            &mut self.conservation_effort,
            &mut self.anti_corruption,
            &mut self.capacity_building,
            &mut self.democratic_reform,
        ] {
            *lever *= e;
        }
        // `spending` and `tax_rate_add` intentionally NOT scaled.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_has_identity_values() {
        let e = PolicyEffects::neutral();
        assert_eq!(e.carbon_intensity_mult, 1.0);
        assert_eq!(e.growth_mult, 1.0);
        assert_eq!(e.redistribution, 0.0);
        assert_eq!(e.spending, 0.0);
    }

    #[test]
    fn scaling_reduces_benefits_but_not_cost() {
        let mut e = PolicyEffects::neutral();
        e.redistribution = 1.0;
        e.carbon_intensity_mult = 0.9; // a 10% cut
        e.spending = 5.0;
        e.scale_effectiveness(0.5);
        assert!((e.redistribution - 0.5).abs() < 1e-12, "benefit should halve");
        assert!((e.carbon_intensity_mult - 0.95).abs() < 1e-12, "cut should halve");
        assert_eq!(e.spending, 5.0, "cost must NOT be scaled");
    }

    #[test]
    fn full_effectiveness_is_identity() {
        let mut e = PolicyEffects::neutral();
        e.education_investment = 0.03;
        e.carbon_intensity_mult = 0.8;
        let before = e.clone();
        e.scale_effectiveness(1.0);
        assert_eq!(e, before);
    }
}
