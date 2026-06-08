//! The yearly update equations — the model's "laws of motion".
//!
//! [`step`] takes the current [`WorldState`] plus the [`PolicyEffects`] in force
//! and returns the state one year later. **Every constant is sourced in
//! `docs/RESEARCH.md`** and cited inline in [`params`]; the structural choices
//! (Cobb–Douglas growth, DICE damage, airborne-fraction carbon accounting,
//! Preston-curve life expectancy, WHR wellbeing, IMF effectiveness gating,
//! economic-voting elections) are explained in `docs/MODEL.md`.
//!
//! ## Integrator discipline (the "physics engine" rule)
//!
//! Every quantity for year *t+1* is computed from the year-*t* state (`prev`).
//! Within a block we may reuse a value computed earlier in the *same* block
//! (a local), but we never read another domain's freshly-updated value. This
//! makes the step a clean explicit-Euler integrator: the order of the domain
//! blocks does not change the result. Indices are clamped; `ln`/`powf`
//! arguments are kept strictly positive so no NaNs can appear.

use crate::effects::PolicyEffects;
use crate::state::{
    prosperity_index, Animal, Economy, Environment, Governance, Human, Ideology, Society, WorldState,
};
use crate::util::{clamp, clamp01, log2, relax};

/// All tunable constants, each with its source. See `docs/RESEARCH.md`.
pub mod params {
    // --- Climate (RESEARCH.md §1) -------------------------------------------
    /// Pre-industrial CO₂ (ppm). IPCC AR6 WG1 (1750 ref).
    pub const CO2_PREINDUSTRIAL: f64 = 278.3;
    /// Equilibrium Climate Sensitivity (°C per CO₂ doubling). IPCC AR6 best est.
    pub const CLIMATE_SENSITIVITY: f64 = 3.0;
    /// Temperature relaxation toward equilibrium (~40yr ocean lag). Calibrated
    /// so transient warming < equilibrium, consistent with TCR<ECS (AR6).
    pub const TEMP_ADJUST_RATE: f64 = 0.025;
    /// Gt CO₂ per ppm. NOAA/IPCC (1 ppm ≈ 2.13 GtC = 7.81 Gt CO₂).
    pub const GTCO2_PER_PPM: f64 = 7.81;
    /// Baseline airborne fraction of emissions. Global Carbon Budget 2024.
    pub const AIRBORNE_FRACTION: f64 = 0.46;
    /// Emission intensity: Gt CO₂ per trillion int-$ at carbon_intensity=1.
    /// Calibrated: 41.6 Gt CO₂ / 195 T$ = 0.213 (Global Carbon Budget 2024).
    pub const EMISSION_INTENSITY: f64 = 0.2133;
    /// Autonomous annual decline in carbon intensity (clean-tech progress).
    pub const AUTONOMOUS_DECARB: f64 = 0.99;
    /// Floor on carbon intensity (no literally zero-carbon economy).
    pub const CARBON_INTENSITY_FLOOR: f64 = 0.05;
    /// DICE-2016R damage coefficient ψ₂ (Ω = 1/(1+ψ₂·T²)). Nordhaus 2017 PNAS:
    /// 2.1% loss at 3 °C. (High-damage alternatives are 3–10× this.)
    pub const DICE_DAMAGE_PSI2: f64 = 0.00236;

    // --- Economy (RESEARCH.md §2) -------------------------------------------
    /// Capital share α (Cobb–Douglas). DICE-2016R / standard.
    pub const CAPITAL_SHARE: f64 = 0.30;
    /// Baseline savings/investment rate. World Bank 2023 (~26% of GDP).
    pub const BASE_SAVINGS_RATE: f64 = 0.26;
    /// Capital depreciation /yr. DICE-2016R (0.10).
    pub const DEPRECIATION: f64 = 0.10;
    /// Long-run TFP growth /yr. World Bank Global Productivity 2021.
    pub const BASE_TFP_GROWTH: f64 = 0.010;
    /// Extra TFP growth per unit education-investment (share of GDP).
    pub const TFP_PER_EDU_INVEST: f64 = 0.40;
    /// TFP contribution from the education level.
    pub const TFP_PER_EDU_LEVEL: f64 = 0.010;
    /// TFP contribution from state capacity (Vu 2025: +6–7% GDP/SD capacity).
    pub const TFP_PER_CAPACITY: f64 = 0.006;
    /// Deadweight drag per unit tax rate above 0.35.
    pub const TAX_DRAG: f64 = 0.05;
    /// Baseline government primary spending, share of GDP (IMF; ~small deficit).
    pub const BASE_GOV_SPEND: f64 = 0.31;
    /// Interest rate on public debt.
    pub const DEBT_INTEREST: f64 = 0.03;
    /// Okun's-law sensitivity of unemployment to the growth gap (inverse form
    /// β≈−0.45). Ball, Leigh & Loungani 2017.
    pub const OKUN: f64 = 0.40;
    /// Reference (full-employment) real growth rate. IMF WEO.
    pub const REFERENCE_GROWTH: f64 = 0.03;
    /// Annual upward drift of inequality from skill-biased growth.
    pub const GINI_DRIFT: f64 = 0.0010;
    /// Minimum and capacity-scaled maximum tax/GDP a state can actually collect.
    /// World Bank 15% threshold; OECD high ≈ 45%.
    pub const TAX_FLOOR: f64 = 0.15;
    pub const TAX_CAPACITY_SLOPE: f64 = 0.40;

    // --- Resources & pollution (RESEARCH.md §1, §4) ------------------------
    /// Reserve depletion per trillion-$ output (before efficiency). Calibrated
    /// to ~0.5%/yr at baseline (UNEP IRP material-extraction scale).
    pub const RESOURCE_INTENSITY: f64 = 0.000026;
    /// Pollution generated per unit carbon-weighted output. Calibrated so source
    /// ≈ decay at baseline (slow drift, not instant saturation).
    pub const POLLUTION_PER_OUTPUT: f64 = 0.0001;
    /// Natural pollution decay /yr.
    pub const POLLUTION_DECAY: f64 = 0.02;
    /// Baseline forest-cover loss /yr. FAO FRA 2020 (net 4.7 Mha/yr scale).
    pub const DEFOREST_PRESSURE: f64 = 0.0006;

    // --- Demographics & health (RESEARCH.md §3) ----------------------------
    /// Crude birth rate (fraction/yr). World Bank 2023 (16.3/1000).
    pub const BASE_BIRTH_RATE: f64 = 0.0163;
    /// Fertility decline per unit education above baseline (demographic
    /// transition). Calibrated; Cleland/Galor (strong negative).
    pub const FERTILITY_SLOPE: f64 = 0.020;
    /// Crude death rate (fraction/yr). World Bank 2023 (7.6/1000).
    pub const BASE_DEATH_RATE: f64 = 0.0076;
    /// Excess mortality multiplier from pollution and heat stress.
    pub const MORTALITY_POLLUTION: f64 = 0.40;
    pub const MORTALITY_HEAT: f64 = 0.05;
    /// Preston-curve life-expectancy fit: LE ≈ A + B·ln(GDPpc). Calibrated to
    /// 73.3 yr at $23.8k (UN WPP 2024; Preston 1975 functional form).
    pub const PRESTON_A: f64 = 14.5;
    pub const PRESTON_B: f64 = 6.0;

    // --- Relaxation speeds for slow indices ---------------------------------
    pub const LIFE_EXP_RELAX: f64 = 0.15;
    pub const HEALTH_RELAX: f64 = 0.20;
    pub const EDUCATION_RELAX: f64 = 0.08;
    pub const WELLBEING_RELAX: f64 = 0.20;
    pub const SOCIETY_RELAX: f64 = 0.12;
    pub const LIVABILITY_RELAX: f64 = 0.15;

    // --- Biosphere (RESEARCH.md §4) ----------------------------------------
    /// Multiplicative biodiversity decay per unit pressure (decay slows as less
    /// remains — partial irreversibility). Species–area logic (z≈0.25).
    pub const BIODIVERSITY_DECAY: f64 = 0.020;
    /// Multiplicative wildlife (LPI) decay per unit pressure.
    pub const WILDLIFE_DECAY: f64 = 0.030;
    /// Biosphere recovery per unit conservation effort.
    pub const BIOSPHERE_RECOVERY: f64 = 0.5;

    // --- Governance (RESEARCH.md §5) ---------------------------------------
    pub const GOV_RELAX: f64 = 0.10;
    pub const CAPACITY_RELAX: f64 = 0.05;
    pub const CORRUPTION_RELAX: f64 = 0.05;
    pub const POLARIZATION_RELAX: f64 = 0.08;
    /// Economic-voting: orientation responsiveness at elections scales with
    /// democracy (accountable polities track public demand; autocracies don't).
    pub const ELECTION_RESPONSIVENESS: f64 = 0.5;
}

/// Calibrate the productivity scale so the full production function (including
/// damage, scarcity and tax drag) reproduces the scenario's initial GDP.
pub fn calibrate_productivity(economy: &Economy, human: &Human, environment: &Environment) -> f64 {
    let basis = production_basis(economy.capital, effective_labor(human, economy));
    let mult = output_multipliers(
        environment.temp_anomaly,
        environment.resource_reserves,
        economy.tax_rate,
    );
    let denom = basis * mult;
    if denom.abs() < f64::EPSILON {
        1.0
    } else {
        economy.gdp / denom
    }
}

/// Effective labour: employed people × human-capital quality.
fn effective_labor(human: &Human, economy: &Economy) -> f64 {
    let employed = human.population * (1.0 - economy.unemployment);
    employed * (0.5 + 0.5 * human.education)
}

/// Cobb–Douglas core `K^α · L^(1-α)`.
fn production_basis(capital: f64, labor: f64) -> f64 {
    capital.max(0.0).powf(params::CAPITAL_SHARE) * labor.max(0.0).powf(1.0 - params::CAPITAL_SHARE)
}

/// DICE damage factor Ω = 1/(1+ψ₂·T²): the fraction of potential output that
/// *survives* climate damage at temperature anomaly `t` (°C above pre-industrial).
fn climate_survival(t: f64) -> f64 {
    1.0 / (1.0 + params::DICE_DAMAGE_PSI2 * t * t)
}

/// Combined damage/scarcity/tax-drag multiplier on potential output.
fn output_multipliers(temp_anomaly: f64, resource_reserves: f64, tax_rate: f64) -> f64 {
    let resource_factor = 0.7 + 0.3 * resource_reserves; // scarcity drag
    let tax_drag = 1.0 - params::TAX_DRAG * (tax_rate - 0.35).max(0.0);
    climate_survival(temp_anomaly) * resource_factor * tax_drag
}

/// Advance the world by exactly one year.
pub fn step(prev: &WorldState, eff: &PolicyEffects) -> WorldState {
    use params::*;

    let h = &prev.human;
    let so = &prev.society;
    let ec = &prev.economy;
    let en = &prev.environment;
    let an = &prev.animal;
    let gv = &prev.governance;

    let gdp_pc = ec.gdp_per_capita(h);
    let prosperity = prosperity_index(gdp_pc);
    let temp_excess = (en.temp_anomaly - 1.09).max(0.0); // warming beyond today

    // =======================================================================
    // ENVIRONMENT — carbon, climate, pollution, resources, forests
    // =======================================================================
    let carbon_intensity =
        (en.carbon_intensity * AUTONOMOUS_DECARB * eff.carbon_intensity_mult).max(CARBON_INTENSITY_FLOOR);

    // Emissions (Gt CO₂) → ppm via the airborne fraction (which already nets out
    // ocean/land sinks); the fraction worsens as forests decline.
    let emissions = ec.gdp * carbon_intensity * EMISSION_INTENSITY;
    let airborne = clamp(AIRBORNE_FRACTION + 0.3 * (0.31 - en.forest_cover), 0.40, 0.65);
    let co2_ppm = (en.co2_ppm + airborne * emissions / GTCO2_PER_PPM).max(0.0);

    // Temperature relaxes toward its CO₂-implied equilibrium (ECS·log₂(C/C₀)).
    let equilibrium_temp = CLIMATE_SENSITIVITY * log2(co2_ppm.max(1.0) / CO2_PREINDUSTRIAL);
    let temp_anomaly = relax(en.temp_anomaly, equilibrium_temp, TEMP_ADJUST_RATE);

    let pollution_source = ec.gdp * carbon_intensity * POLLUTION_PER_OUTPUT;
    let pollution = clamp01(en.pollution + pollution_source - POLLUTION_DECAY - eff.pollution_abatement);

    let depletion = ec.gdp * RESOURCE_INTENSITY * (1.0 - clamp01(eff.resource_efficiency));
    let resource_reserves = clamp01(en.resource_reserves - depletion);

    let deforestation = DEFOREST_PRESSURE * (1.0 + prosperity);
    let forest_cover =
        clamp01(en.forest_cover - deforestation + eff.reforestation + 0.5 * eff.conservation_effort);

    // =======================================================================
    // ECONOMY — capital, productivity, output, jobs, inequality, budget
    // =======================================================================
    let savings_rate = clamp01(BASE_SAVINGS_RATE + eff.savings_rate_add);
    let capital = (ec.capital * (1.0 - DEPRECIATION) + savings_rate * ec.gdp).max(0.0);

    let tfp_growth = BASE_TFP_GROWTH
        + TFP_PER_EDU_INVEST * eff.education_investment
        + TFP_PER_EDU_LEVEL * h.education
        + TFP_PER_CAPACITY * gv.state_capacity;
    let productivity = ec.productivity * (1.0 + tfp_growth);

    let tax_rate = clamp01(ec.tax_rate + eff.tax_rate_add);
    let labor = effective_labor(h, ec);
    let basis = production_basis(capital, labor);
    let gdp = (productivity
        * basis
        * output_multipliers(temp_anomaly, resource_reserves, tax_rate)
        * eff.growth_mult)
        .max(0.0);

    let growth_rate = if ec.gdp.abs() < f64::EPSILON { 0.0 } else { gdp / ec.gdp - 1.0 };
    let unemployment = clamp(ec.unemployment - OKUN * (growth_rate - REFERENCE_GROWTH), 0.01, 0.60);
    let gini = clamp01(ec.gini + GINI_DRIFT - eff.redistribution * 0.05 - eff.education_investment * 0.01);

    // Budget: revenue capped by what the state can actually collect.
    let collectible = TAX_FLOOR + TAX_CAPACITY_SLOPE * gv.state_capacity;
    let revenue = tax_rate.min(collectible) * gdp;
    let interest = ec.public_debt * DEBT_INTEREST;
    let spending = BASE_GOV_SPEND * gdp + eff.spending + interest;
    let public_debt = ec.public_debt + (spending - revenue);

    // =======================================================================
    // HUMAN — population, life expectancy, education, health
    // =======================================================================
    let birth_rate = (BASE_BIRTH_RATE - FERTILITY_SLOPE * (h.education - 0.63)).max(0.0);
    let death_rate = BASE_DEATH_RATE
        * (1.0 + MORTALITY_POLLUTION * (en.pollution - 0.35).max(0.0) + MORTALITY_HEAT * temp_excess)
        * (1.0 - 0.2 * (h.health - 0.70));
    let population = (h.population * (1.0 + birth_rate - death_rate)).max(0.0);

    // Preston curve + environmental burden + health investment.
    let le_target = PRESTON_A + PRESTON_B * (gdp_pc.max(1.0)).ln()
        - 4.0 * en.pollution
        - 2.0 * temp_excess
        + 20.0 * eff.health_investment;
    let life_expectancy = clamp(relax(h.life_expectancy, le_target, LIFE_EXP_RELAX), 20.0, 100.0);

    let health_target = clamp01((life_expectancy - 45.0) / 40.0 - 0.2 * (en.pollution - 0.35).max(0.0));
    let health = clamp01(relax(h.health, health_target, HEALTH_RELAX));

    let education_target = clamp01(0.25 + 0.65 * prosperity + 3.0 * eff.education_investment);
    let education = clamp01(relax(h.education, education_target, EDUCATION_RELAX));

    // =======================================================================
    // SOCIETY — social support, freedom, livability, work, wellbeing (WHR)
    // =======================================================================
    let social_target =
        clamp01(0.70 + 0.20 * gv.legitimacy - 0.20 * gv.polarization + eff.social_support_boost);
    let social_support = clamp01(relax(so.social_support, social_target, SOCIETY_RELAX));

    let freedom_target = clamp01(0.30 + 0.45 * gv.democracy - 0.15 * gv.polarization + eff.freedom_boost);
    let freedom = clamp01(relax(so.freedom, freedom_target, SOCIETY_RELAX));

    let work_intensity = clamp(relax(so.work_intensity, 1.0 - eff.work_reduction, 0.2), 0.5, 1.2);

    let livability_target = clamp01(
        0.30 * prosperity
            + 0.25 * (1.0 - en.pollution)
            + 0.15 * h.health
            + 0.15 * (1.0 - ec.gini)
            + 0.15 * clamp01(en.forest_cover / 0.31)
            + eff.livability_boost,
    );
    let livability = clamp01(relax(so.livability, livability_target, LIVABILITY_RELAX));

    // Wellbeing on the Cantril ladder (0–10), weights informed by the World
    // Happiness Report's driver coefficients (social support & freedom large,
    // corruption negative), with an inequality penalty and leisure bonus.
    let wellbeing_core = 0.03
        + 0.18 * prosperity
        + 0.26 * so.social_support
        + 0.16 * h.health
        + 0.14 * so.freedom
        + 0.10 * (1.0 - gv.corruption)
        + 0.16 * so.livability
        - 0.25 * ec.gini
        + 0.05 * (1.0 - so.work_intensity);
    let wellbeing_target = 10.0 * clamp01(wellbeing_core);
    let wellbeing = clamp(relax(so.wellbeing, wellbeing_target, WELLBEING_RELAX), 0.0, 10.0);

    // =======================================================================
    // ANIMAL — biodiversity & wildlife (multiplicative decay = partial
    // irreversibility; recovery only via active conservation)
    // =======================================================================
    let land_pressure = clamp01(0.40 + 0.40 * prosperity + 0.30 * (0.31 - forest_cover) / 0.31);
    let env_pressure =
        clamp01(0.30 * pollution + 0.30 * clamp01(temp_excess / 2.0) + 0.40 * land_pressure);
    let recovery = BIOSPHERE_RECOVERY * eff.conservation_effort;
    let biodiversity = clamp01(an.biodiversity * (1.0 - BIODIVERSITY_DECAY * env_pressure) + recovery);
    let wildlife_index = clamp01(an.wildlife_index * (1.0 - WILDLIFE_DECAY * env_pressure) + recovery);

    // =======================================================================
    // GOVERNANCE — capacity, corruption, legitimacy, democracy, polarization,
    // political capital, and elections (economic voting)
    // =======================================================================
    let capacity_target =
        clamp01(0.30 + 0.45 * h.education + 0.15 * gv.legitimacy - 0.15 * gv.corruption + 2.0 * eff.capacity_building);
    let state_capacity = clamp01(relax(gv.state_capacity, capacity_target, CAPACITY_RELAX));

    let corruption_target =
        clamp01(0.80 - 0.30 * gv.state_capacity - 0.15 * gv.democracy - 2.0 * eff.anti_corruption);
    let corruption = clamp01(relax(gv.corruption, corruption_target, CORRUPTION_RELAX));

    let legitimacy_target = clamp01(
        0.10 + 0.30 * (so.wellbeing / 10.0) + 0.20 * (1.0 - gv.corruption) + 0.10 * gv.democracy,
    );
    let legitimacy = clamp01(relax(gv.legitimacy, legitimacy_target, GOV_RELAX));

    // Democracy: persistent, drifts with backsliding pressure & reform.
    let democracy = clamp01(
        gv.democracy + 0.5 * eff.democratic_reform - 0.01 * (gv.polarization - 0.40).max(0.0)
            + 0.01 * (gv.legitimacy - 0.50),
    );

    let polarization_target = clamp01(0.20 + 0.60 * ec.gini + 0.20 * (1.0 - gv.legitimacy));
    let polarization = clamp01(relax(gv.polarization, polarization_target, POLARIZATION_RELAX));

    let mut political_capital = clamp01(gv.political_capital + 0.10 * gv.legitimacy - 0.05);

    // Elections: every term, the electorate re-orients policy toward salient
    // problems, scaled by how accountable the system is (democracy). Economic
    // voting (growth ↑ / unemployment ↓ ⇒ satisfaction) sets the mandate.
    let mut orientation = gv.orientation;
    let mut years_to_election = gv.years_to_election;
    if years_to_election == 0 {
        let satisfaction = clamp01(
            0.5 + 1.0 * (growth_rate - REFERENCE_GROWTH) - 0.30 * (unemployment - 0.05) + 0.2 * (so.wellbeing / 10.0 - 0.5),
        );
        // What the electorate currently wants given conditions.
        let eco_concern = clamp01(0.5 * clamp01(temp_excess / 2.0) + 0.5 * pollution);
        let redistribution_demand = clamp01(gini + 2.0 * (unemployment - 0.05).max(0.0));
        let solidarity_demand = clamp01(1.0 - so.wellbeing / 10.0);
        let desired = Ideology::new(
            2.0 * redistribution_demand - 1.0,
            2.0 * eco_concern - 1.0,
            2.0 * solidarity_demand - 1.0,
        );
        let responsiveness = ELECTION_RESPONSIVENESS * democracy;
        orientation = Ideology::new(
            relax(orientation.economic, desired.economic, responsiveness),
            relax(orientation.ecological, desired.ecological, responsiveness),
            relax(orientation.social, desired.social, responsiveness),
        );
        // A fresh mandate restores political capital (more so if satisfied).
        political_capital = clamp01(0.4 + 0.4 * satisfaction);
        years_to_election = gv.term_length.max(1);
    } else {
        years_to_election -= 1;
    }

    WorldState {
        year: prev.year + 1,
        human: Human { population, life_expectancy, education, health },
        society: Society { wellbeing, social_support, freedom, livability, work_intensity },
        economy: Economy {
            gdp,
            capital,
            productivity,
            gini,
            tax_rate,
            unemployment,
            public_debt,
        },
        environment: Environment {
            co2_ppm,
            temp_anomaly,
            pollution,
            resource_reserves,
            forest_cover,
            carbon_intensity,
        },
        animal: Animal { biodiversity, wildlife_index },
        governance: Governance {
            state_capacity,
            corruption,
            legitimacy,
            democracy,
            polarization,
            political_capital,
            orientation,
            years_to_election,
            term_length: gv.term_length,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::PolicyStack;
    use crate::scenario::Scenario;

    fn baseline() -> WorldState {
        Scenario::baseline_2025().initial_state()
    }

    #[test]
    fn step_advances_year_by_one() {
        let s0 = baseline();
        assert_eq!(step(&s0, &PolicyEffects::neutral()).year, s0.year + 1);
    }

    #[test]
    fn all_indices_stay_in_range_over_long_run() {
        let mut s = baseline();
        let stack = PolicyStack::new();
        for _ in 0..300 {
            let eff = stack.effects_for(s.year, &s);
            s = step(&s, &eff);
            s.sanitize();
            for v in [
                s.human.education, s.human.health, s.economy.gini, s.economy.tax_rate,
                s.economy.unemployment, s.environment.pollution, s.environment.resource_reserves,
                s.environment.forest_cover, s.animal.biodiversity, s.animal.wildlife_index,
                s.society.social_support, s.society.freedom, s.society.livability,
                s.governance.state_capacity, s.governance.corruption, s.governance.legitimacy,
                s.governance.democracy, s.governance.polarization,
            ] {
                assert!((0.0..=1.0).contains(&v), "index left [0,1]: {v} in year {}", s.year);
                assert!(v.is_finite());
            }
            assert!(s.economy.gdp.is_finite() && s.economy.gdp >= 0.0);
            assert!((0.0..=10.0).contains(&s.society.wellbeing));
        }
    }

    #[test]
    fn calibration_reproduces_initial_gdp() {
        let s = baseline();
        let s1 = step(&s, &PolicyEffects::neutral());
        let growth = s1.economy.gdp / s.economy.gdp - 1.0;
        // Year-0 growth should be the modest endogenous rate, not a jump.
        assert!(growth.abs() < 0.08, "year-0 growth implausible: {growth}");
    }

    #[test]
    fn baseline_co2_growth_is_realistic() {
        // ~+2.4 ppm/yr at present (Global Carbon Budget).
        let s = baseline();
        let s1 = step(&s, &PolicyEffects::neutral());
        let d = s1.environment.co2_ppm - s.environment.co2_ppm;
        assert!((1.5..3.5).contains(&d), "CO₂ growth off: {d} ppm/yr");
    }

    #[test]
    fn business_as_usual_warms_the_planet() {
        let mut s = baseline();
        let t0 = s.environment.temp_anomaly;
        for _ in 0..50 {
            s = step(&s, &PolicyEffects::neutral());
        }
        assert!(s.environment.temp_anomaly > t0, "BAU should warm: {t0} -> {}", s.environment.temp_anomaly);
    }

    #[test]
    fn damage_is_monotonic_and_bounded() {
        assert!(climate_survival(0.0) >= climate_survival(2.0));
        assert!(climate_survival(2.0) >= climate_survival(4.0));
        assert!(climate_survival(100.0) > 0.0);
        // Nordhaus calibration check: ~2.1% loss at 3 °C.
        assert!(((1.0 - climate_survival(3.0)) - 0.021).abs() < 0.003);
    }
}
