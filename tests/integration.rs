//! End-to-end integration tests exercising the public API the way a user would.
//! These assert the *direction* of trade-offs, not exact values — the model is
//! illustrative (see `docs/MODEL.md`, `docs/RESEARCH.md`).

use society_sim::prelude::*;

fn run_with(scenario: Scenario, policies: Vec<Box<dyn Policy>>, years: u32) -> Snapshot {
    let mut sim = Simulation::new(scenario);
    for p in policies {
        sim.add_policy(p);
    }
    sim.run(years).into_iter().last().unwrap()
}

#[test]
fn business_as_usual_degrades_the_biosphere() {
    let start = Scenario::baseline_2025().initial_state();
    let end = run_with(Scenario::baseline_2025(), vec![], 75);
    assert!(
        end.animal.biodiversity < start.animal.biodiversity,
        "BAU should reduce biodiversity: {} -> {}",
        start.animal.biodiversity,
        end.animal.biodiversity
    );
}

#[test]
fn a_green_and_social_package_beats_business_as_usual_overall() {
    let bau = run_with(Scenario::baseline_2025(), vec![], 75);
    let package: Vec<Box<dyn Policy>> = vec![
        Box::new(CarbonTax::new(2025, 0.7)),
        Box::new(GreenInvestment::new(2025, 0.03)),
        Box::new(Reforestation::new(2025, 0.8)),
        Box::new(ConservationProgram::new(2025, 0.8)),
        Box::new(EducationProgram::new(2025, 0.03)),
        Box::new(HealthcareProgram::new(2025, 0.03)),
        Box::new(SocialHousing::new(2025, 0.03)),
        Box::new(UniversalBasicIncome::new(2030, 0.4)),
        Box::new(CircularEconomy::new(2025, 0.6)),
    ];
    let mixed = run_with(Scenario::baseline_2025(), package, 75);

    assert!(
        mixed.planet.overall > bau.planet.overall,
        "package should beat BAU overall: {} vs {}",
        mixed.planet.overall,
        bau.planet.overall
    );
    assert!(mixed.environment.temp_anomaly < bau.environment.temp_anomaly);
    assert!(mixed.planet.ecological > bau.planet.ecological);
    assert!(mixed.society.wellbeing > bau.society.wellbeing);
}

#[test]
fn there_are_trade_offs_unfunded_ubi_costs_the_budget() {
    let bau = run_with(Scenario::baseline_2025(), vec![], 40);
    let ubi = run_with(
        Scenario::baseline_2025(),
        vec![Box::new(UniversalBasicIncome::new(2025, 1.0))],
        40,
    );
    assert!(
        ubi.economy.debt_ratio() > bau.economy.debt_ratio(),
        "unfunded UBI should raise debt: {} vs {}",
        ubi.economy.debt_ratio(),
        bau.economy.debt_ratio()
    );
    assert!(ubi.economy.gini < bau.economy.gini, "but it should lower inequality");
}

#[test]
fn an_endogenous_technocracy_beats_do_nothing() {
    let mut nothing = Simulation::new(Scenario::baseline_2025());
    nothing.set_government(Box::new(ArchetypeGovernment::status_quo()));
    let n = nothing.run(75).last().unwrap().planet.overall;

    let mut tech = Simulation::new(Scenario::baseline_2025());
    tech.set_government(Box::new(ArchetypeGovernment::technocracy()));
    let t = tech.run(75).last().unwrap().planet.overall;

    assert!(t > n, "technocracy should beat do-nothing: {t} vs {n}");
}

#[test]
fn history_length_and_years_are_consistent() {
    let mut sim = Simulation::new(Scenario::baseline_2025());
    let h = sim.run(100);
    assert_eq!(h.len(), 101);
    assert_eq!(h.first().unwrap().year, 2025);
    assert_eq!(h.last().unwrap().year, 2125);
    for w in h.windows(2) {
        assert_eq!(w[1].year, w[0].year + 1);
    }
}
