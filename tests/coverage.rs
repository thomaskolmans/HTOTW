//! Breadth tests that exercise public API surface left uncovered by the
//! behavioural tests — accessors, trait defaults, every policy/rule/archetype —
//! so the suite reflects the whole codebase, not just the headline paths.

use society_sim::engine::{
    agent_support, govern, record, render_agent_density, render_resource_heatmap, render_run,
    render_sparkline, render_trace_sparklines, ChoiceMechanism, CorruptOfficial, Decarbonize,
    HarvestQuota, OpenAccess, Polity, PolicyOption, Primitives, PropertyRights, Redistribute, Rule,
    WealthRanking, WealthTax, World,
};
use society_sim::prelude::*;
use society_sim::state::prosperity_index;

fn agg_state() -> WorldState {
    Scenario::baseline_2025().initial_state()
}

// --- aggregate model: Policy trait defaults + PolicyStack ------------------

struct BarePolicy;
impl Policy for BarePolicy {
    fn name(&self) -> &str {
        "bare"
    }
    fn start_year(&self) -> u32 {
        0
    }
    fn apply(&self, _y: u32, _s: &WorldState, _e: &mut PolicyEffects) {}
}

#[test]
fn policy_trait_defaults_and_stack() {
    let b = BarePolicy;
    // default position(), describe(), is_active().
    assert_eq!(b.position(), Ideology::centrist());
    assert_eq!(b.describe(), "bare");
    assert!(b.is_active(5));

    let mut stack = PolicyStack::new();
    assert!(stack.is_empty());
    stack.push(Box::new(BarePolicy));
    assert_eq!(stack.len(), 1);
    assert_eq!(stack.iter().count(), 1);
    let _ = stack.effects_for(2025, &agg_state());
}

#[test]
fn every_builtin_policy_builds_and_applies() {
    let state = agg_state();
    for name in society_sim::policies::all_names() {
        let p = society_sim::policies::build(name, 2025, 0.03).expect("known policy");
        let _ = p.name();
        let _ = p.describe();
        let _ = p.position();
        assert!(p.is_active(2025));
        let mut e = PolicyEffects::neutral();
        p.apply(2025, &state, &mut e);
    }
    assert!(society_sim::policies::build("ubi", 2025, 0.5).is_some());
    assert!(society_sim::policies::build("does-not-exist", 2025, 0.5).is_none());
}

// --- aggregate model: every government archetype --------------------------

#[test]
fn every_government_archetype_runs() {
    use society_sim::governance::ArchetypeGovernment;
    assert!(!ArchetypeGovernment::all_names().is_empty());
    for name in ArchetypeGovernment::all_names() {
        let gov = ArchetypeGovernment::by_name(name).expect("known archetype");
        let mut sim = Simulation::new(Scenario::baseline_2025());
        sim.set_government(Box::new(gov));
        sim.run(15);
        assert!(sim.government_name().is_some());
        let _ = sim.enacted_count();
    }
    // aliases + unknown
    for alias in ["technocrat", "socdem", "liberal", "populist", "do-nothing"] {
        assert!(ArchetypeGovernment::by_name(alias).is_some());
    }
    assert!(ArchetypeGovernment::by_name("monarchy").is_none());
}

#[test]
fn simulation_accessors_and_scenarios() {
    let sim = Simulation::new(Scenario::baseline_2025());
    assert!(sim.government_name().is_none()); // no government installed
    assert!(sim.policies().is_empty());
    let _ = sim.state();

    for name in Scenario::all_names() {
        let sc = Scenario::by_name(name).expect("known scenario");
        let _ = sc.initial_state().planet();
    }
    for alias in ["baseline", "fragile", "strong"] {
        assert!(Scenario::by_name(alias).is_some());
    }
    assert!(Scenario::by_name("atlantis").is_none());
}

// --- aggregate model: state helpers + sanitize edge branches --------------

#[test]
fn state_helpers_and_sanitize() {
    let s = agg_state();
    let _ = s.human.billions();
    let _ = s.economy.gdp_per_capita(&s.human);
    let _ = s.gdp_per_capita();
    let _ = s.planet();
    let _ = s.governance.is_election_year();

    // zero-population and zero-GDP branches.
    let mut h0 = s.human.clone();
    h0.population = 0.0;
    assert_eq!(s.economy.gdp_per_capita(&h0), 0.0);
    let mut e0 = s.economy.clone();
    e0.gdp = 0.0;
    assert_eq!(e0.debt_ratio(), 0.0);

    // prosperity_index edges.
    assert_eq!(prosperity_index(0.0), 0.0);
    assert!(prosperity_index(50_000.0) > 0.0);

    // Ideology construction (with clamping) + alignment.
    let id = Ideology::new(2.0, -2.0, 0.5); // clamps to [-1,1]
    assert_eq!(id.economic, 1.0);
    assert_eq!(id.ecological, -1.0);
    let _ = id.alignment(&Ideology::centrist());

    // sanitize pulls every out-of-range field back into bounds.
    let mut bad = s.clone();
    bad.human.life_expectancy = 999.0;
    bad.human.health = 9.0;
    bad.society.work_intensity = 9.0;
    bad.society.wellbeing = 99.0;
    bad.economy.gini = -5.0;
    bad.environment.pollution = 9.0;
    bad.governance.corruption = 9.0;
    bad.sanitize();
    assert!(bad.human.life_expectancy <= 100.0);
    assert_eq!(bad.economy.gini, 0.0);
    assert_eq!(bad.environment.pollution, 1.0);
    assert!(bad.society.work_intensity <= 1.2);
}

#[test]
fn policy_effects_default_equals_neutral() {
    assert_eq!(PolicyEffects::default(), PolicyEffects::neutral());
}

// --- engine: every institution Rule (name + enforce) ----------------------

#[test]
fn every_engine_rule_enforces() {
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(OpenAccess),
        Box::new(HarvestQuota::new(0.3)),
        Box::new(PropertyRights),
        Box::new(WealthTax::new(0.1)),
        Box::new(Redistribute::new(0.5)),
        Box::new(CorruptOfficial::new(0.3)),
        Box::new(Decarbonize::new(0.5)),
    ];
    for r in &rules {
        assert!(!r.name().is_empty());
        let mut w = World::new(Primitives::fragile_commons());
        for _ in 0..8 {
            w.step_with_rules(std::slice::from_ref(r));
        }
    }
}

// --- engine: world & agent accessors --------------------------------------

#[test]
fn engine_world_accessors() {
    let w = World::new(Primitives::demo());
    assert_eq!(w.substrate.cells(), w.substrate.width * w.substrate.height);
    assert!(!w.agents.is_empty());
    let _ = w.climate_sensitivity();
    let _ = w.temperature();
    let _ = w.greenhouse_stock();
    let _ = w.emissions_flow();
    let _ = w.total_resource();
}

// --- engine: collective choice (polity) -----------------------------------

#[test]
fn engine_polity_majority_and_wealth_weighted() {
    // Warming world + zero threshold so salient options (incl. decarbonization)
    // get elected — covering the PolicyOption::Decarbonization arms.
    let mut w = World::new(Primitives::warming_world());
    let mut pol = Polity::new(ChoiceMechanism::Majority, 5).with_threshold(0.0);
    govern(&mut w, &mut pol, 30, |_t, _p| {});
    let _ = pol.active_policies();
    let _ = pol.active_rules();
    let _ = pol.turnover();
    let _ = pol.elections();
    for opt in PolicyOption::ALL {
        let _ = opt.name();
        let _ = pol.is_active(opt);
        let _ = pol.vote_share(opt);
    }

    // Wealth-weighted mechanism on the demo world.
    let mut w2 = World::new(Primitives::demo());
    let mut pol2 = Polity::new(ChoiceMechanism::WealthWeighted, 4);
    govern(&mut w2, &mut pol2, 20, |t, p| {
        let _ = (t, p.elections());
    });

    // WealthRanking + agent_support.
    let r = WealthRanking::new(&w);
    let _ = r.mean();
    if let Some(i) = (0..w.agents.len()).find(|&i| w.agents.alive[i]) {
        let _ = r.wealth(i);
        let _ = r.percentile(i);
        let _ = agent_support(&w, i, &r);
    }
}

// --- engine: trace & rendering --------------------------------------------

#[test]
fn engine_trace_and_render() {
    let mut w = World::new(Primitives::demo());
    let t = record(&mut w, &[], 6);
    assert!(!t.is_empty());
    assert_eq!(t.len(), 7); // ticks + 1
    let pops = t.series(|m| m.population as f64);
    assert!(!t.to_csv().is_empty());

    // record under rules (covers the with-rules branch).
    let mut w2 = World::new(Primitives::fragile_commons());
    let rules: Vec<Box<dyn Rule>> = vec![Box::new(HarvestQuota::new(0.3))];
    let _ = record(&mut w2, &rules, 5);

    // every renderer.
    assert!(!render_resource_heatmap(&w).is_empty());
    assert!(!render_agent_density(&w).is_empty());
    assert!(!render_sparkline("pop", &pops).is_empty());
    assert!(!render_trace_sparklines(&t).is_empty());
    assert!(!render_run(&w, &t).is_empty());
}
