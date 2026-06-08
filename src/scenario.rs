//! Scenarios: named bundles of initial conditions, calibrated to **real 2024
//! figures** (see `docs/RESEARCH.md` for every number's source).

use crate::dynamics::calibrate_productivity;
use crate::state::{Animal, Economy, Environment, Governance, Human, Ideology, Society, WorldState};

/// A named set of initial conditions for the world.
#[derive(Debug, Clone)]
pub struct Scenario {
    pub name: String,
    pub description: String,
    pub start_year: u32,
    pub human: Human,
    pub society: Society,
    pub economy: Economy,
    pub environment: Environment,
    pub animal: Animal,
    pub governance: Governance,
}

impl Scenario {
    /// Present-day world, calibrated to 2024 data (the "business as usual"
    /// starting point). Sources in `docs/RESEARCH.md`.
    pub fn baseline_2025() -> Scenario {
        Scenario {
            name: "baseline-2025".to_string(),
            description: "Present-day world, calibrated to 2024 figures.".to_string(),
            start_year: 2025,
            human: Human {
                population: 8.2,        // UN WPP 2024 (billions)
                life_expectancy: 73.3,  // UN WPP 2024
                education: 0.63,        // HDI-style attainment index
                health: 0.70,
            },
            society: Society {
                wellbeing: 5.5,         // World Happiness Report (Cantril ladder)
                social_support: 0.70,
                freedom: 0.55,
                livability: 0.65,
                work_intensity: 1.0,
            },
            economy: Economy {
                gdp: 195.0,             // IMF WEO 2024, PPP (trillion int-$)
                capital: 546.0,         // K/Y ≈ 2.8 (Piketty production capital)
                productivity: 1.0,      // recalibrated in initial_state()
                gini: 0.39,             // World Bank median within-country
                tax_rate: 0.30,         // total govt revenue/GDP (IMF)
                unemployment: 0.049,    // ILO 2024
                public_debt: 179.0,     // ≈0.92×GDP (IMF Fiscal Monitor 2024)
            },
            environment: Environment {
                co2_ppm: 422.5,         // Global Carbon Budget 2024
                temp_anomaly: 1.09,     // IPCC AR6 (2011–2020 vs 1850–1900)
                pollution: 0.35,
                resource_reserves: 0.75,
                forest_cover: 0.31,     // FAO FRA 2020 (31% of land)
                carbon_intensity: 1.0,
            },
            animal: Animal {
                biodiversity: 0.79,     // BII (safe boundary 0.90)
                wildlife_index: 0.27,   // LPI: −73% since 1970 (WWF/ZSL 2024)
            },
            governance: Governance {
                state_capacity: 0.55,   // WGI gov. effectiveness (rescaled)
                corruption: 0.57,       // 1 − CPI/100 (CPI global avg 43)
                legitimacy: 0.39,       // OECD trust 2024
                democracy: 0.45,        // V-Dem LDI (declining)
                polarization: 0.45,
                political_capital: 0.50,
                orientation: Ideology::centrist(),
                years_to_election: 4,
                term_length: 4,
            },
        }
    }

    /// A fragile, high-inequality, weak-institutions, resource-stressed world —
    /// to ask "do the same policies still work from a worse starting point?".
    pub fn fragile_world() -> Scenario {
        let mut s = Scenario::baseline_2025();
        s.name = "fragile-world".to_string();
        s.description =
            "High inequality, weak/corrupt institutions, depleted reserves, hotter start.".to_string();
        s.economy.gini = 0.55;
        s.economy.public_debt = 250.0;
        s.environment.resource_reserves = 0.45;
        s.environment.temp_anomaly = 1.3;
        s.environment.co2_ppm = 430.0;
        s.environment.forest_cover = 0.22;
        s.animal.biodiversity = 0.60;
        s.animal.wildlife_index = 0.18;
        s.society.wellbeing = 4.3;
        s.society.social_support = 0.55;
        s.society.freedom = 0.35;
        s.society.livability = 0.45;
        s.governance.state_capacity = 0.35;
        s.governance.corruption = 0.72;
        s.governance.legitimacy = 0.28;
        s.governance.democracy = 0.30;
        s.governance.polarization = 0.65;
        s
    }

    /// A high-capacity, clean-institutions, social-democratic starting point —
    /// "what if we begin with strong institutions?".
    pub fn strong_institutions() -> Scenario {
        let mut s = Scenario::baseline_2025();
        s.name = "strong-institutions".to_string();
        s.description = "High state capacity, low corruption, strong democracy and trust.".to_string();
        s.economy.gini = 0.30;
        s.society.wellbeing = 6.8;
        s.society.social_support = 0.88;
        s.society.freedom = 0.80;
        s.society.livability = 0.80;
        s.governance.state_capacity = 0.85;
        s.governance.corruption = 0.20;
        s.governance.legitimacy = 0.65;
        s.governance.democracy = 0.80;
        s.governance.polarization = 0.25;
        s.governance.orientation = Ideology::new(0.2, 0.3, 0.3);
        s
    }

    pub fn by_name(name: &str) -> Option<Scenario> {
        match name {
            "baseline-2025" | "baseline" => Some(Self::baseline_2025()),
            "fragile-world" | "fragile" => Some(Self::fragile_world()),
            "strong-institutions" | "strong" => Some(Self::strong_institutions()),
            _ => None,
        }
    }

    pub fn all_names() -> &'static [&'static str] {
        &["baseline-2025", "fragile-world", "strong-institutions"]
    }

    /// Materialise the calibrated initial [`WorldState`] (recomputes the
    /// productivity scale so the production function reproduces initial GDP).
    pub fn initial_state(&self) -> WorldState {
        let mut economy = self.economy.clone();
        economy.productivity = calibrate_productivity(&economy, &self.human, &self.environment);
        WorldState {
            year: self.start_year,
            human: self.human.clone(),
            society: self.society.clone(),
            economy,
            environment: self.environment.clone(),
            animal: self.animal.clone(),
            governance: self.governance.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_is_calibrated() {
        let st = Scenario::baseline_2025().initial_state();
        assert_eq!(st.year, 2025);
        assert!(st.economy.productivity > 0.0 && st.economy.productivity.is_finite());
        assert_eq!(st.economy.gdp, 195.0);
    }

    #[test]
    fn lookup_and_aliases() {
        assert!(Scenario::by_name("baseline").is_some());
        assert!(Scenario::by_name("fragile").is_some());
        assert!(Scenario::by_name("strong").is_some());
        assert!(Scenario::by_name("nope").is_none());
    }

    #[test]
    fn fragile_is_worse_strong_is_better_than_baseline() {
        let b = Scenario::baseline_2025().initial_state().planet().overall;
        let f = Scenario::fragile_world().initial_state().planet().overall;
        let s = Scenario::strong_institutions().initial_state().planet().overall;
        assert!(f < b, "fragile should be worse: {f} vs {b}");
        assert!(s > b, "strong should be better: {s} vs {b}");
    }
}
