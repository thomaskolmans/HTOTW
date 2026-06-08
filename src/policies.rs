//! A library of built-in, parameterised policies you can stack.
//!
//! Each policy maps a real-world intervention onto the [`PolicyEffects`] levers.
//! In *governed* mode their effect is additionally gated by the polity's
//! implementation effectiveness (see [`crate::effects::PolicyEffects::scale_effectiveness`]).
//!
//! Policies take a `start_year` plus a primary parameter (`strength` ∈ `[0,1]`,
//! or a `share_of_gdp`). Each also declares an [`Ideology`] position so an
//! endogenous government can judge whether the policy fits its mandate
//! (see [`crate::governance`]).

use crate::effects::PolicyEffects;
use crate::policy::Policy;
use crate::state::{Ideology, WorldState};
use crate::util::clamp01;

macro_rules! strength_policy {
    ($name:ident, $tag:literal, $econ:expr, $eco:expr, $soc:expr, $apply:expr, $desc:expr) => {
        #[doc = $desc]
        pub struct $name {
            start: u32,
            strength: f64,
        }
        impl $name {
            pub fn new(start_year: u32, strength: f64) -> Self {
                Self { start: start_year, strength: clamp01(strength) }
            }
        }
        impl Policy for $name {
            fn name(&self) -> &str { $tag }
            fn start_year(&self) -> u32 { self.start }
            fn position(&self) -> Ideology { Ideology::new($econ, $eco, $soc) }
            fn apply(&self, _year: u32, state: &WorldState, eff: &mut PolicyEffects) {
                let s = self.strength;
                let gdp = state.economy.gdp;
                let _ = gdp;
                ($apply)(s, gdp, eff);
            }
            fn describe(&self) -> String {
                format!("{} (from {}, strength {:.2})", $tag, self.start, self.strength)
            }
        }
    };
}

strength_policy!(CarbonTax, "carbon-tax", -0.2, 0.8, 0.0,
    |s: f64, gdp: f64, eff: &mut PolicyEffects| {
        eff.carbon_intensity_mult *= 1.0 - 0.08 * s;
        eff.pollution_abatement += 0.02 * s;
        eff.spending -= 0.01 * s * gdp; // Pigouvian revenue (negative cost)
        eff.growth_mult *= 1.0 - 0.01 * s;
    },
    "**Carbon tax / carbon pricing.** Accelerates decarbonisation and abates pollution; raises revenue; mild growth drag.");

strength_policy!(UniversalBasicIncome, "universal-basic-income", 0.7, 0.0, 0.8,
    |s: f64, gdp: f64, eff: &mut PolicyEffects| {
        eff.redistribution += 0.6 * s;
        eff.social_support_boost += 0.04 * s;
        eff.livability_boost += 0.03 * s;
        eff.spending += 0.05 * s * gdp;
    },
    "**Universal Basic Income.** Strong redistribution and social security; funded from the budget.");

strength_policy!(Reforestation, "reforestation", 0.0, 0.7, 0.0,
    |s: f64, gdp: f64, eff: &mut PolicyEffects| {
        eff.reforestation += 0.004 * s;
        eff.conservation_effort += 0.02 * s;
        eff.spending += 0.005 * s * gdp;
    },
    "**Reforestation & land restoration.** Grows forest cover (a carbon sink) and supports the biosphere.");

strength_policy!(ConservationProgram, "conservation-program", 0.1, 0.9, 0.1,
    |s: f64, gdp: f64, eff: &mut PolicyEffects| {
        eff.conservation_effort += 0.05 * s;
        eff.spending += 0.004 * s * gdp;
    },
    "**Biodiversity conservation (protected areas, rewilding).** Lifts the biosphere and relieves land pressure.");

strength_policy!(CircularEconomy, "circular-economy", 0.2, 0.6, 0.0,
    |s: f64, _gdp: f64, eff: &mut PolicyEffects| {
        eff.resource_efficiency += 0.6 * s;
        eff.pollution_abatement += 0.015 * s;
    },
    "**Circular-economy / resource-efficiency mandate.** Slows depletion of reserves and cuts pollution.");

strength_policy!(ShorterWorkweek, "shorter-workweek", 0.3, 0.2, 0.6,
    |s: f64, _gdp: f64, eff: &mut PolicyEffects| {
        eff.work_reduction += 0.10 * s;
        eff.social_support_boost += 0.02 * s;
        eff.growth_mult *= 1.0 - 0.02 * s;
    },
    "**Shorter working week.** Trades some measured output for time-affluence and wellbeing.");

strength_policy!(AntiCorruption, "anti-corruption", 0.1, 0.0, 0.2,
    |s: f64, gdp: f64, eff: &mut PolicyEffects| {
        eff.anti_corruption += 0.08 * s;
        eff.spending += 0.002 * s * gdp;
    },
    "**Anti-corruption drive (transparency, audit, prosecution).** Lowers corruption, raising effectiveness.");

strength_policy!(DemocraticReform, "democratic-reform", 0.0, 0.0, 0.4,
    |s: f64, _gdp: f64, eff: &mut PolicyEffects| {
        eff.democratic_reform += 0.08 * s;
        eff.freedom_boost += 0.03 * s;
    },
    "**Democratic / institutional reform.** Raises accountability and civic freedom.");

strength_policy!(CivilLiberties, "civil-liberties", 0.0, 0.0, 0.3,
    |s: f64, _gdp: f64, eff: &mut PolicyEffects| {
        eff.freedom_boost += 0.06 * s;
    },
    "**Civil-liberties expansion.** Directly raises freedom to make life choices.");

// --- Share-of-GDP policies (parameter = fraction of GDP spent per year) ------

macro_rules! share_policy {
    ($name:ident, $tag:literal, $cap:expr, $econ:expr, $eco:expr, $soc:expr, $apply:expr, $desc:expr) => {
        #[doc = $desc]
        pub struct $name {
            start: u32,
            share: f64,
        }
        impl $name {
            pub fn new(start_year: u32, share_of_gdp: f64) -> Self {
                Self { start: start_year, share: share_of_gdp.clamp(0.0, $cap) }
            }
        }
        impl Policy for $name {
            fn name(&self) -> &str { $tag }
            fn start_year(&self) -> u32 { self.start }
            fn position(&self) -> Ideology { Ideology::new($econ, $eco, $soc) }
            fn apply(&self, _year: u32, state: &WorldState, eff: &mut PolicyEffects) {
                ($apply)(self.share, state.economy.gdp, eff);
            }
            fn describe(&self) -> String {
                format!("{} (from {}, {:.1}% of GDP/yr)", $tag, self.start, self.share * 100.0)
            }
        }
    };
}

share_policy!(GreenInvestment, "green-investment", 0.10, 0.3, 0.8, 0.0,
    |s: f64, gdp: f64, eff: &mut PolicyEffects| {
        eff.carbon_intensity_mult *= 1.0 - 2.0 * s;
        eff.pollution_abatement += 0.5 * s;
        eff.savings_rate_add += 0.5 * s;
        eff.spending += s * gdp;
    },
    "**Public clean-energy & green R&D investment.** Faster decarbonisation, less growth drag than a tax, costs the budget.");

share_policy!(EducationProgram, "education-program", 0.10, 0.4, 0.1, 0.5,
    |s: f64, gdp: f64, eff: &mut PolicyEffects| {
        eff.education_investment += s;
        eff.spending += s * gdp;
    },
    "**Public education investment.** Raises human capital → productivity, health, lower fertility, lower inequality.");

share_policy!(HealthcareProgram, "healthcare-program", 0.15, 0.4, 0.0, 0.5,
    |s: f64, gdp: f64, eff: &mut PolicyEffects| {
        eff.health_investment += s;
        eff.spending += s * gdp;
    },
    "**Universal healthcare investment.** Raises population health and life expectancy.");

share_policy!(SocialHousing, "social-housing", 0.06, 0.5, 0.0, 0.5,
    |s: f64, gdp: f64, eff: &mut PolicyEffects| {
        eff.livability_boost += 4.0 * s;
        eff.social_support_boost += 1.0 * s;
        eff.spending += s * gdp;
    },
    "**Affordable-housing & public-services programme.** Directly raises livability.");

share_policy!(CapacityBuilding, "capacity-building", 0.05, 0.5, 0.0, 0.2,
    |s: f64, gdp: f64, eff: &mut PolicyEffects| {
        eff.capacity_building += 2.0 * s;
        eff.spending += s * gdp;
    },
    "**State-capacity building (civil service, digital govt, PFM).** Raises the effectiveness of *all* policy.");

/// A progressive tax is special: it adjusts the tax rate (revenue) directly.
pub struct ProgressiveTax {
    start: u32,
    extra_rate: f64,
}
impl ProgressiveTax {
    pub fn new(start_year: u32, extra_rate: f64) -> Self {
        Self { start: start_year, extra_rate: extra_rate.clamp(0.0, 0.4) }
    }
}
impl Policy for ProgressiveTax {
    fn name(&self) -> &str { "progressive-tax" }
    fn start_year(&self) -> u32 { self.start }
    fn position(&self) -> Ideology { Ideology::new(0.8, 0.1, 0.5) }
    fn apply(&self, _year: u32, _state: &WorldState, eff: &mut PolicyEffects) {
        eff.tax_rate_add += self.extra_rate;
        eff.redistribution += 0.4 * self.extra_rate / 0.05;
    }
    fn describe(&self) -> String {
        format!("progressive-tax (from {}, +{:.0}pp)", self.start, self.extra_rate * 100.0)
    }
}

/// Construct a built-in policy by `name` with one primary parameter.
pub fn build(name: &str, start_year: u32, param: f64) -> Option<Box<dyn Policy>> {
    let p: Box<dyn Policy> = match name {
        "carbon-tax" => Box::new(CarbonTax::new(start_year, param)),
        "green-investment" => Box::new(GreenInvestment::new(start_year, param)),
        "universal-basic-income" | "ubi" => Box::new(UniversalBasicIncome::new(start_year, param)),
        "progressive-tax" => Box::new(ProgressiveTax::new(start_year, param)),
        "education-program" => Box::new(EducationProgram::new(start_year, param)),
        "healthcare-program" => Box::new(HealthcareProgram::new(start_year, param)),
        "social-housing" => Box::new(SocialHousing::new(start_year, param)),
        "reforestation" => Box::new(Reforestation::new(start_year, param)),
        "conservation-program" => Box::new(ConservationProgram::new(start_year, param)),
        "circular-economy" => Box::new(CircularEconomy::new(start_year, param)),
        "shorter-workweek" => Box::new(ShorterWorkweek::new(start_year, param)),
        "anti-corruption" => Box::new(AntiCorruption::new(start_year, param)),
        "capacity-building" => Box::new(CapacityBuilding::new(start_year, param)),
        "democratic-reform" => Box::new(DemocraticReform::new(start_year, param)),
        "civil-liberties" => Box::new(CivilLiberties::new(start_year, param)),
        _ => return None,
    };
    Some(p)
}

pub fn all_names() -> &'static [&'static str] {
    &[
        "carbon-tax", "green-investment", "universal-basic-income", "progressive-tax",
        "education-program", "healthcare-program", "social-housing", "reforestation",
        "conservation-program", "circular-economy", "shorter-workweek", "anti-corruption",
        "capacity-building", "democratic-reform", "civil-liberties",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::Scenario;
    use crate::sim::Simulation;

    fn state() -> WorldState {
        Scenario::baseline_2025().initial_state()
    }

    #[test]
    fn carbon_tax_pushes_carbon_intensity_down() {
        let mut eff = PolicyEffects::neutral();
        CarbonTax::new(2025, 1.0).apply(2025, &state(), &mut eff);
        assert!(eff.carbon_intensity_mult < 1.0);
    }

    #[test]
    fn ubi_reduces_inequality() {
        let g_bau = Simulation::new(Scenario::baseline_2025()).run(30).last().unwrap().economy.gini;
        let mut sim = Simulation::new(Scenario::baseline_2025());
        sim.add_policy(Box::new(UniversalBasicIncome::new(2025, 1.0)));
        let g_ubi = sim.run(30).last().unwrap().economy.gini;
        assert!(g_ubi < g_bau, "UBI should lower Gini: {g_ubi} vs {g_bau}");
    }

    #[test]
    fn anti_corruption_raises_effectiveness_over_time() {
        let e_bau = Simulation::new(Scenario::fragile_world()).run(40).last().unwrap().governance.corruption;
        let mut sim = Simulation::new(Scenario::fragile_world());
        sim.add_policy(Box::new(AntiCorruption::new(2025, 1.0)));
        let e_anti = sim.run(40).last().unwrap().governance.corruption;
        assert!(e_anti < e_bau, "anti-corruption should lower corruption: {e_anti} vs {e_bau}");
    }

    #[test]
    fn build_resolves_known_and_rejects_unknown() {
        assert!(build("carbon-tax", 2030, 0.5).is_some());
        assert!(build("capacity-building", 2030, 0.03).is_some());
        assert!(build("teleportation", 2030, 0.5).is_none());
    }
}
