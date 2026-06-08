//! # The society-physics engine (v2 — first-principles, agent-based)
//!
//! A "physics engine for society": macro quantities are **never inputs**. You
//! specify only physical/biological [`Primitives`] (a resource landscape,
//! ecological regrowth, metabolism, perception, reproduction/mortality), run the
//! [`World`] forward, and **measure** what emerges with the read-only
//! [`instruments`] — population, the wealth **Gini**, life expectancy,
//! carrying capacity, production.
//!
//! This is the answer to "I want to simulate *to* those numbers, not start at
//! them": to reproduce a real statistic (say a Gini near 0.4), you calibrate the
//! *primitives* until the *measured* output matches — never by setting the
//! output. See `docs/ENGINE.md` for the full architecture, the phased roadmap
//! (substrate+agents → exchange → institutions → calibration), and citations.
//!
//! ## Minimal run
//!
//! ```
//! use society_sim::engine::{Primitives, World, instruments};
//! let mut world = World::new(Primitives::demo());
//! for _ in 0..200 { world.step(); }
//! let m = instruments::measure(&world);
//! // Inequality EMERGED from an equal start — it was never set:
//! assert!(m.wealth_gini > 0.0);
//! println!("pop {} gini {:.2}", m.population, m.wealth_gini);
//! ```

pub mod calibration;
pub mod institutions;
pub mod instruments;
pub mod parallel;
pub mod polity;
pub mod rng;
pub mod trace;
pub mod world;

pub use calibration::{
    calibrate, compare, default_targets, evaluate, knob_names, loss, welfare, Calibration, Outcome,
    RunSummary, Scenario, Target, Verdict,
};
pub use institutions::{
    CorruptOfficial, Decarbonize, HarvestQuota, OpenAccess, PropertyRights, Redistribute, Rule,
    WealthTax,
};
pub use instruments::{measure, Measurements};
pub use parallel::{max_threads, set_max_threads};
pub use polity::{
    agent_support, govern, ChoiceMechanism, Polity, PolicyOption, WealthRanking,
};
pub use rng::Rng;
pub use trace::{
    record, render_agent_density, render_resource_heatmap, render_run, render_sparkline,
    render_trace_sparklines, Trace, TRACE_CSV_HEADER,
};
pub use world::{
    equilibrium_temperature, Agents, Primitives, Substrate, Trade, World, NGOODS, PREINDUSTRIAL_C,
    STEFAN_BOLTZMANN,
};

#[cfg(test)]
mod emergence_tests {
    use super::instruments::{self, measure, total_welfare};
    use super::world::{Primitives, World};

    /// Mean per-agent need-satisfaction welfare over a run — the quantity the
    /// bilateral trade rule provably raises. Used by the gains-from-trade test.
    fn mean_welfare_per_agent(mut w: World, ticks: usize) -> f64 {
        let mut acc = 0.0;
        let mut n = 0u64;
        for _ in 0..ticks {
            w.step();
            let pop = w.agents.alive_count();
            if pop > 0 {
                acc += total_welfare(&w) / pop as f64;
                n += 1;
            }
        }
        acc / n.max(1) as f64
    }

    /// Volume-weighted median realised price over a whole run (the emergent
    /// market price), used by the scarcity test.
    fn run_price_index(mut w: World, ticks: usize) -> f64 {
        let mut pairs: Vec<(f64, f64)> = Vec::new();
        for _ in 0..ticks {
            w.step();
            for t in &w.trades_this_tick {
                pairs.push((t.price, t.qty_good0));
            }
        }
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let total: f64 = pairs.iter().map(|p| p.1).sum();
        let mut acc = 0.0;
        for (v, wt) in pairs {
            acc += wt;
            if acc >= total / 2.0 {
                return v;
            }
        }
        f64::NAN
    }

    /// The thesis, in one test: starting every agent with **equal** wealth,
    /// inequality EMERGES purely from heterogeneous biology and geography — it is
    /// measured, never assigned. (Sugarscape's classic result.)
    #[test]
    fn wealth_inequality_emerges_from_an_equal_start() {
        let mut p = Primitives::demo();
        p.seed = 42;
        let mut w = World::new(p);

        // Everyone starts identically wealthy.
        let start = measure(&w);
        assert!(start.wealth_gini.abs() < 1e-9, "should start perfectly equal");

        for _ in 0..250 {
            w.step();
        }
        let end = measure(&w);
        // The *magnitude* of inequality is itself emergent (it depends on
        // scarcity, vision, metabolism); we only assert it genuinely arises from
        // a perfectly equal start — never that it hits a back-fitted value.
        assert!(
            end.wealth_gini > 0.05,
            "inequality should EMERGE from an equal start, got Gini {}",
            end.wealth_gini
        );
        assert!(end.population > 0, "society should survive");
    }

    /// Carrying capacity and life expectancy are emergent measurements, present
    /// and finite after the population has turned over.
    #[test]
    fn life_expectancy_and_population_are_measured_outputs() {
        let mut w = World::new(Primitives::demo());
        for _ in 0..300 {
            w.step();
        }
        let m = measure(&w);
        assert!(m.population > 0);
        assert!(m.life_expectancy.is_finite() && m.life_expectancy > 0.0);
        assert!(m.mean_wealth > 0.0);
    }

    // ---------------------------------------------------------------------
    // Phase 2 — EXCHANGE. Prices, money, GDP and specialization EMERGE from
    // two goods, heterogeneous productivity and a bilateral bargaining rule.
    // None of these is ever an input. (Menger, Hayek, Gode & Sunder, Ricardo,
    // Epstein & Axtell.)
    // ---------------------------------------------------------------------

    /// **Gains from trade** (the central Phase-2 result): with the exchange phase
    /// ON, average per-agent need-satisfaction welfare exceeds the *same-seed*
    /// autarky run with trade OFF. Trade is voluntary and each deal strictly
    /// Pareto-improving, so welfare can only rise — emergent, not assumed.
    #[test]
    fn trade_produces_gains_over_autarky() {
        for seed in [1u64, 7, 42] {
            let mut on = Primitives::demo();
            on.seed = seed;
            on.trade_enabled = true;
            let mut off = on.clone();
            off.trade_enabled = false;

            let w_on = mean_welfare_per_agent(World::new(on), 250);
            let w_off = mean_welfare_per_agent(World::new(off), 250);
            assert!(
                w_on > w_off,
                "trade should raise welfare over autarky (seed {seed}): {w_on} vs {w_off}"
            );
        }
    }

    /// **Scarcity raises the emergent price** of a good (Hayek: the price carries
    /// the scarcity information). Halving good-0's stock & capacity raises the
    /// volume-weighted median realised ratio (good-1 paid per unit good-0).
    #[test]
    fn scarcity_raises_the_emergent_price() {
        for seed in [1u64, 7, 42] {
            let p = {
                let mut p = Primitives::demo();
                p.seed = seed;
                p
            };
            let normal = World::new(p.clone());
            let mut scarce = World::new(p);
            for cell in scarce.substrate.resource.iter_mut() {
                cell[0] *= 0.5;
            }
            for cell in scarce.substrate.capacity.iter_mut() {
                cell[0] *= 0.5;
            }
            let price_normal = run_price_index(normal, 250);
            let price_scarce = run_price_index(scarce, 250);
            assert!(
                price_scarce > price_normal,
                "scarcer good-0 should command a higher price (seed {seed}): \
                 {price_scarce} vs {price_normal}"
            );
        }
    }

    /// Prices, money, GDP and specialization are all **measured, never set**:
    /// after running, a realised price exists, a dominant medium of exchange has
    /// emerged (Menger/Kiyotaki–Wright), GDP flows, and agents specialize
    /// (Ricardo) beyond a pure generalist baseline.
    #[test]
    fn prices_money_gdp_specialization_emerge() {
        let mut p = Primitives::demo();
        p.seed = 42;
        let mut w = World::new(p);
        let mut any_trade = false;
        let mut any_gdp = false;
        for _ in 0..250 {
            w.step();
            let m = measure(&w);
            if m.trade_count > 0 {
                any_trade = true;
                assert!(m.price_index.is_finite() && m.price_index > 0.0);
            }
            if m.gdp_flow > 0.0 {
                any_gdp = true;
            }
        }
        assert!(any_trade, "bilateral trade should occur");
        assert!(any_gdp, "GDP flow should be measured");
        let m = measure(&w);
        assert!(m.dominant_medium.is_some(), "a medium of exchange should emerge");
        assert!(
            instruments::mean_specialization(&w) > 0.0,
            "agents should specialize beyond a pure generalist"
        );
    }

    /// Determinism is preserved with the exchange phase ON: same seed ⇒
    /// bit-identical trade ledgers and price series.
    #[test]
    fn exchange_is_deterministic() {
        let mut p = Primitives::demo();
        p.seed = 9;
        let mut a = World::new(p.clone());
        let mut b = World::new(p);
        for _ in 0..200 {
            a.step();
            b.step();
            assert_eq!(a.trades_this_tick.len(), b.trades_this_tick.len());
            let ma = measure(&a);
            let mb = measure(&b);
            assert_eq!(ma.gdp_flow.to_bits(), mb.gdp_flow.to_bits());
            assert_eq!(ma.price_index.to_bits(), mb.price_index.to_bits());
        }
    }
}

#[cfg(test)]
mod phase3_institution_tests {
    //! Phase 3 — INSTITUTIONS. Property regimes, the tragedy of the commons and
    //! its resolution, redistribution, and *emergent* state capacity / legitimacy
    //! / corruption. Every governance quality is **measured** by an instrument
    //! from raw agent/substrate state; rules only mold mechanisms and payoffs.
    //! (Hardin, Ostrom, Demsetz, Axelrod, Olson.)
    use super::institutions::{
        CorruptOfficial, HarvestQuota, OpenAccess, PropertyRights, Redistribute, Rule, WealthTax,
    };
    use super::instruments::{commons_health, measure, run_under};
    use super::rng::Rng;
    use super::world::{Primitives, World};

    fn boxed(rules: Vec<Box<dyn Rule>>) -> Vec<Box<dyn Rule>> {
        rules
    }

    /// **The tragedy of the commons and its resolution** (Hardin → Ostrom /
    /// Demsetz), as a same-seed contrast. Under **open access** every agent strips
    /// its cell; summed over the population the standing stock is mined past its
    /// regeneration threshold and the landscape's *capacity itself* degrades,
    /// collapsing both the resource and the population. A **harvest quota** and a
    /// **property regime** that leave enough standing stock keep the commons far
    /// healthier and support a much larger population — none of it set, all
    /// measured from `capacity/capacity0` and the living count.
    #[test]
    fn open_access_collapses_commons_that_a_rule_sustains() {
        for seed in [1u64, 7, 42, 100] {
            let mut p = Primitives::fragile_commons();
            p.seed = seed;
            let open = run_under(p.clone(), &boxed(vec![Box::new(OpenAccess)]), 300);
            let quota =
                run_under(p.clone(), &boxed(vec![Box::new(HarvestQuota::new(0.3))]), 300);
            let prop = run_under(p.clone(), &boxed(vec![Box::new(PropertyRights)]), 300);

            assert!(
                quota.commons_health > open.commons_health,
                "quota should sustain the commons better than open access (seed {seed}): \
                 {} vs {}",
                quota.commons_health,
                open.commons_health
            );
            assert!(
                prop.commons_health > open.commons_health,
                "property rights should sustain the commons better than open access (seed {seed}): \
                 {} vs {}",
                prop.commons_health,
                open.commons_health
            );
            // The healthier commons also carries more people (the welfare payoff).
            assert!(
                quota.population > open.population && prop.population > open.population,
                "a sustained commons should support more population (seed {seed})"
            );
        }
    }

    /// **Redistribution lowers the measured Gini** (the mechanism-level result):
    /// run a world forward with no institutions until inequality has emerged, then
    /// apply *one* round of a progressive wealth tax + means-tested transfer to a
    /// clone of that exact state. Comparing the two same-tick measurements
    /// isolates the rule's effect from demographic churn: the transfer compresses
    /// the wealth distribution, so the Gini falls. Emergent compression, not a set
    /// target.
    #[test]
    fn redistribution_lowers_measured_gini() {
        for seed in [1u64, 7, 42, 100, 5] {
            let mut p = Primitives::demo();
            p.seed = seed;
            let mut w = World::new(p);
            for _ in 0..200 {
                w.step();
            }
            let before = measure(&w).wealth_gini;
            assert!(before > 0.0, "inequality should have emerged first (seed {seed})");

            // Apply tax + redistribution once to a snapshot of this exact world.
            let mut c = w.clone();
            let mut rng = Rng::seed(0xD15EA5E ^ seed);
            WealthTax::new(0.5).enforce(&mut c, &mut rng);
            Redistribute::new(1.0).enforce(&mut c, &mut rng);
            let after = measure(&c).wealth_gini;

            assert!(
                after < before,
                "redistribution should lower the measured Gini (seed {seed}): {after} vs {before}"
            );
        }
    }

    /// **State capacity, legitimacy and corruption EMERGE** and respond to the
    /// mechanism — they are never set. A clean institution (tax-funded quota
    /// enforcement) is compared, same seed, against one with a kleptocrat skimming
    /// the public pool. Corruption is measured > 0 only when the diversion rule is
    /// present; the looted pool funds less enforcement (lower **state capacity**)
    /// and erodes **legitimacy**, which in turn degrades the commons. All four are
    /// read off ledgers, not assigned. (Tilly, Levi, Olson, North.)
    #[test]
    fn capacity_legitimacy_corruption_emerge_and_corruption_hurts() {
        for seed in [1u64, 7, 42] {
            let mut p = Primitives::fragile_commons();
            p.seed = seed;
            let clean = boxed(vec![
                Box::new(WealthTax::new(0.05)),
                Box::new(HarvestQuota::new(0.4)),
            ]);
            let corrupt = boxed(vec![
                Box::new(WealthTax::new(0.05)),
                Box::new(CorruptOfficial::new(0.6)),
                Box::new(HarvestQuota::new(0.4)),
            ]);

            let mut wc = World::new(p.clone());
            for _ in 0..250 {
                wc.step_with_rules(&clean);
            }
            let mut wk = World::new(p.clone());
            for _ in 0..250 {
                wk.step_with_rules(&corrupt);
            }

            // Corruption is measured, and only the corrupt regime has any.
            assert!(wc.corruption() == 0.0, "clean regime has no corruption (seed {seed})");
            assert!(
                wk.corruption() > 0.0,
                "corruption should be measured under a diverting rule (seed {seed})"
            );
            // Emergent governance qualities live in [0,1].
            for w in [&wc, &wk] {
                assert!((0.0..=1.0).contains(&w.state_capacity()));
                assert!((0.0..=1.0).contains(&w.legitimacy()));
                assert!((0.0..=1.0).contains(&w.corruption()));
            }
            // The looted pool funds less enforcement and erodes morale...
            assert!(
                wk.state_capacity() < wc.state_capacity(),
                "corruption should lower state capacity (seed {seed}): {} vs {}",
                wk.state_capacity(),
                wc.state_capacity()
            );
            assert!(
                wk.legitimacy() < wc.legitimacy(),
                "corruption should erode legitimacy (seed {seed}): {} vs {}",
                wk.legitimacy(),
                wc.legitimacy()
            );
            // ...and the weakened institution lets the commons degrade further.
            assert!(
                commons_health(&wk) < commons_health(&wc),
                "corruption should degrade the commons (seed {seed}): {} vs {}",
                commons_health(&wk),
                commons_health(&wc)
            );
        }
    }

    /// **Legitimacy / compliance bootstraps emergently.** Under a bare quota
    /// (voluntarily enforced, no funded coercion) the reinforcing legitimacy
    /// belief climbs above its neutral 0.5 start as a cooperative norm takes hold
    /// (Axelrod reciprocity; Ostrom): a positive compliance rate and a legitimacy
    /// above one-half are *measured*, never assigned.
    #[test]
    fn legitimacy_and_compliance_emerge_under_a_voluntary_rule() {
        let mut p = Primitives::demo();
        p.seed = 42;
        let mut w = World::new(p);
        let rules = boxed(vec![Box::new(HarvestQuota::new(0.3))]);
        for _ in 0..200 {
            w.step_with_rules(&rules);
        }
        let m = measure(&w);
        assert!(m.compliance_rate > 0.0, "a compliance rate should emerge");
        assert!(
            w.legitimacy() > 0.5,
            "a working voluntary institution should earn legitimacy above neutral, got {}",
            w.legitimacy()
        );
    }

    /// Determinism is preserved with the institutional phase ON: the same seed and
    /// same rule stack ⇒ bit-identical governance ledgers and measurements.
    #[test]
    fn institutions_are_deterministic() {
        let mut p = Primitives::demo();
        p.seed = 9;
        let make = || {
            boxed(vec![
                Box::new(WealthTax::new(0.1)) as Box<dyn Rule>,
                Box::new(Redistribute::new(0.7)),
                Box::new(HarvestQuota::new(0.4)),
            ])
        };
        let ra = make();
        let rb = make();
        let mut a = World::new(p.clone());
        let mut b = World::new(p);
        for _ in 0..200 {
            a.step_with_rules(&ra);
            b.step_with_rules(&rb);
            let ma = measure(&a);
            let mb = measure(&b);
            assert_eq!(ma.public_pool.to_bits(), mb.public_pool.to_bits());
            assert_eq!(ma.commons_health.to_bits(), mb.commons_health.to_bits());
            assert_eq!(ma.legitimacy.to_bits(), mb.legitimacy.to_bits());
            assert_eq!(ma.state_capacity.to_bits(), mb.state_capacity.to_bits());
            assert_eq!(a.agents.alive_count(), b.agents.alive_count());
        }
    }
}

#[cfg(test)]
mod phase4_calibration_tests {
    //! Phase 4 — CALIBRATION & EXPERIMENT HARNESS: the formal "simulate **to** the
    //! numbers". Empirical targets (a within-country Gini ~0.39, a plausible life
    //! expectancy) enter **only** as the right-hand side of an MSM loss; worlds are
    //! built from primitives and the moments are MEASURED. The optimiser tunes the
    //! primitives; the harness ranks regimes on a measured welfare functional.
    //! (McFadden 1989; Grazzini & Richiardi 2015; Beaumont 2010.)
    use super::calibration::{
        calibrate, compare, default_targets, evaluate, loss, loss_at, welfare, Scenario, Verdict,
    };
    use super::institutions::{HarvestQuota, OpenAccess, Rule};
    use super::rng::Rng;
    use super::world::Primitives;

    fn boxed(rules: Vec<Box<dyn Rule>>) -> Vec<Box<dyn Rule>> {
        rules
    }

    /// **Calibration measurably reduces the loss** (the central Phase-4 result):
    /// Method-of-Simulated-Moments search over the *primitive* vector lowers the
    /// weighted distance between the model's EMERGENT moments (Gini, life
    /// expectancy) and the empirical targets, versus a random starting primitive
    /// vector. We assert *improvement*, never a back-fitted loss value — the
    /// targets only ever appear inside the loss, never assigned to a world.
    #[test]
    fn calibration_reduces_the_loss_versus_a_random_start() {
        let base = Primitives::demo();
        let targets = default_targets();
        let seeds = [1u64, 7];
        let ticks = 80;

        // A *random* starting primitive vector (the naive baseline to beat).
        let mut rng = Rng::seed(0xDEAD_BEEF_u64);
        let dim = super::calibration::dim();
        let random_start: Vec<f64> = (0..dim).map(|_| rng.f64() * 10.0 - 1.0).collect();
        let random_loss = loss_at(&base, &random_start, &seeds, ticks, &targets);

        // Calibrate (small budget so the test is fast).
        let cal = calibrate(&base, &targets, &seeds, ticks, 12, 12);

        assert!(
            cal.loss < cal.initial_loss,
            "calibration should beat its neutral start: {} vs {}",
            cal.loss,
            cal.initial_loss
        );
        assert!(
            cal.loss < random_loss,
            "calibration should beat a random primitive vector: {} vs {}",
            cal.loss,
            random_loss
        );
        // The fitted thing is a *primitive* set, not an outcome: sanity-check the
        // decoded world is physically sensible.
        assert!(cal.primitives.peak_capacity > 0.0);
        assert!(cal.primitives.metabolism_min < cal.primitives.metabolism_max);
    }

    /// The fitted primitives **bring an emergent moment toward its target**: after
    /// calibration the measured Gini sits closer to the 0.39 target than the
    /// neutral start does. The number 0.39 is only ever compared against, never
    /// set — the world still produces its Gini from biology and geography.
    #[test]
    fn fitted_primitives_move_an_emergent_moment_toward_target() {
        let base = Primitives::demo();
        let targets = default_targets();
        let seeds = [1u64, 7];
        let ticks = 90;

        let cal = calibrate(&base, &targets, &seeds, ticks, 14, 14);
        let fitted = super::calibration::ensemble_summary(&cal.primitives, &seeds, &[], ticks);

        // The untouched demo() world is a legitimate "uncalibrated" reference:
        // it, too, produces its Gini from biology and geography, never set.
        let baseline = super::calibration::ensemble_summary(&base, &seeds, &[], ticks);

        let d_fitted = (fitted.gini - 0.39).abs();
        let d_base = (baseline.gini - 0.39).abs();
        assert!(
            d_fitted <= d_base + 1e-9,
            "fitted Gini should be at least as close to target: |{}-0.39|={} vs |{}-0.39|={}",
            fitted.gini,
            d_fitted,
            baseline.gini,
            d_base
        );
    }

    /// **The harness ranks two regimes deterministically** on a MEASURED welfare
    /// functional (geometric mean of prosperity × equity × sustainability). On a
    /// fragile commons, a conservation **quota** sustains the resource and the
    /// population while **open access** collapses it (Hardin → Ostrom), so the
    /// quota scenario scores strictly higher welfare. The verdict is reproducible
    /// across the same seed ensemble — the welfare comes only from instruments.
    #[test]
    fn harness_ranks_a_sustainable_regime_above_open_access() {
        let p = Primitives::fragile_commons();
        let seeds = [1u64, 7, 42];
        let ticks = 180;

        let open = Scenario::new("open-access", p.clone(), boxed(vec![Box::new(OpenAccess)]));
        let quota = Scenario::new(
            "quota",
            p.clone(),
            boxed(vec![Box::new(HarvestQuota::new(0.3))]),
        );

        let (o_open, o_quota, verdict) = compare(&open, &quota, &seeds, ticks);
        assert_eq!(
            verdict,
            Verdict::Second,
            "quota should out-score open access on welfare: open={} quota={}",
            o_open.welfare,
            o_quota.welfare
        );
        // Welfare is a measured composite, strictly higher for the sustained one.
        assert!(o_quota.welfare > o_open.welfare);
        // And the underlying pillars are themselves measured, never set.
        assert!(o_quota.mean(|s| s.commons_health) > o_open.mean(|s| s.commons_health));
    }

    /// **Determinism** of the whole Phase-4 layer: the same seeds give a
    /// bit-identical calibration (loss + θ) and bit-identical harness welfare.
    #[test]
    fn calibration_and_harness_are_deterministic() {
        let base = Primitives::demo();
        let targets = default_targets();
        let seeds = [3u64, 11];
        let ticks = 70;

        let a = calibrate(&base, &targets, &seeds, ticks, 10, 10);
        let b = calibrate(&base, &targets, &seeds, ticks, 10, 10);
        assert_eq!(a.loss.to_bits(), b.loss.to_bits());
        assert_eq!(a.theta.len(), b.theta.len());
        for (x, y) in a.theta.iter().zip(b.theta.iter()) {
            assert_eq!(x.to_bits(), y.to_bits());
        }

        let sc = || {
            Scenario::new(
                "quota",
                Primitives::fragile_commons(),
                boxed(vec![Box::new(HarvestQuota::new(0.3)) as Box<dyn Rule>]),
            )
        };
        let oa = evaluate(&sc(), &seeds, ticks);
        let ob = evaluate(&sc(), &seeds, ticks);
        assert_eq!(oa.welfare.to_bits(), ob.welfare.to_bits());
    }

    /// The welfare functional behaves as a *no-substitutes* aggregator: zeroing
    /// any single pillar (equity, sustainability, prosperity) zeroes welfare, and
    /// the loss/welfare helpers stay finite. A small guard that the composite
    /// can't be gamed by one dimension. All inputs MEASURED.
    #[test]
    fn welfare_is_a_no_substitutes_composite() {
        use super::calibration::RunSummary;
        let good = RunSummary {
            population: 100.0,
            gini: 0.3,
            life_expectancy: 60.0,
            mean_wealth: 10.0,
            welfare_per_capita: 1.0,
            commons_health: 0.9,
            initial_population: 100.0,
        };
        assert!(welfare(&good) > 0.0);
        // Collapse sustainability → welfare collapses to 0.
        let dead_commons = RunSummary { commons_health: 0.0, ..good };
        assert!(welfare(&dead_commons).abs() < 1e-12);
        // Total inequality (Gini=1) → equity 0 → welfare 0.
        let unequal = RunSummary { gini: 1.0, ..good };
        assert!(welfare(&unequal).abs() < 1e-12);
        // Extinct society → 0.
        let extinct = RunSummary { population: 0.0, ..good };
        assert!(welfare(&extinct).abs() < 1e-12);
        // loss stays finite on a normal summary.
        assert!(loss(&good, &default_targets()).is_finite());
    }
}

#[cfg(test)]
mod phase5_climate_tests {
    //! Phase 5 — SPATIAL ENERGY-BALANCE CLIMATE coupled to production. Emissions
    //! EMERGE from agent production; they accumulate as a greenhouse stock with
    //! first-order decay; temperature follows the zero-dimensional energy balance
    //! (Budyko 1969; Sellers 1969) driven by Myhre 1998 log forcing; and warming
    //! above the productivity optimum MECHANISTICALLY throttles logistic regrowth
    //! (Verhulst) via a unimodal temperature response (Lindeman 1942) — so
    //! "climate damage" is the ecological consequence (lower carrying capacity →
    //! fewer people / less biomass), never a fitted curve on GDP. The climate is
    //! OPT-IN (`Primitives::warming_world`); `demo()` is byte-identical to before.
    use super::institutions::{Decarbonize, Rule};
    use super::instruments::measure;
    use super::world::{Primitives, World};

    fn boxed(r: Vec<Box<dyn Rule>>) -> Vec<Box<dyn Rule>> {
        r
    }

    /// The default world is **unchanged** by the climate code: with the coupling
    /// off, temperature sits exactly at the radiative equilibrium and the
    /// greenhouse stock at its pre-industrial reference for the whole run, and the
    /// emergent population matches the climate-disabled clone bit-for-bit (the
    /// no-op guarantee that keeps every pre-Phase-5 test valid).
    #[test]
    fn default_world_is_a_climate_no_op() {
        let p = Primitives::demo();
        assert!(!p.climate_enabled, "climate must be OFF in demo()");
        let mut w = World::new(p.clone());
        let t0 = w.temperature();
        let c0 = w.greenhouse_stock();
        for _ in 0..200 {
            w.step();
            // Nothing moves: the subsystem is skipped entirely.
            assert_eq!(w.temperature().to_bits(), t0.to_bits());
            assert_eq!(w.greenhouse_stock().to_bits(), c0.to_bits());
            assert_eq!(w.emissions_flow(), 0.0);
        }
    }

    /// (a) **More production → more emissions → higher greenhouse stock → higher
    /// temperature.** A `warming_world` run drives the greenhouse stock and the
    /// temperature strictly above their pre-industrial start; a same-seed clone
    /// with the emission factor zeroed (clean production) holds both pinned at the
    /// pre-industrial steady state. The warming is read off the energy balance,
    /// never set. Across several seeds.
    #[test]
    fn emissions_raise_the_greenhouse_stock_and_temperature() {
        for seed in [1u64, 7, 42] {
            let mut p = Primitives::warming_world();
            p.seed = seed;
            let mut dirty = World::new(p.clone());
            let t0 = dirty.temperature();
            let c0 = dirty.greenhouse_stock();

            let mut clean_p = p.clone();
            clean_p.emission_factor = 0.0; // a clean economy: production, no carbon
            let mut clean = World::new(clean_p);

            for _ in 0..300 {
                dirty.step();
                clean.step();
            }

            // Dirty production accumulated carbon and warmed the planet...
            assert!(
                dirty.greenhouse_stock() > c0,
                "emissions should raise the greenhouse stock (seed {seed}): {} vs {}",
                dirty.greenhouse_stock(),
                c0
            );
            assert!(
                dirty.temperature() > t0 + 0.1,
                "a higher greenhouse stock should warm the planet (seed {seed}): {} vs {}",
                dirty.temperature(),
                t0
            );
            // ...while the clean economy stayed at the pre-industrial steady state.
            assert!(
                (clean.greenhouse_stock() - c0).abs() < 1e-9,
                "no emissions ⇒ greenhouse stock unchanged (seed {seed})"
            );
            assert!(
                (clean.temperature() - t0).abs() < 1e-9,
                "no emissions ⇒ temperature unchanged (seed {seed})"
            );
            // The climate-sensitivity readout is a positive physical quantity.
            assert!(dirty.climate_sensitivity() > 0.0);
        }
    }

    /// (b) **Emergent climate damage**: at the same seed, the warming economy
    /// supports a strictly *smaller* population than the no-emissions control —
    /// purely because warming above the productivity optimum lowers net primary
    /// productivity (the regrowth rate) and hence the realised carrying capacity.
    /// No damage coefficient touches population or wealth; the loss is the
    /// ecological consequence, measured. (Standing biomass is *not* asserted: with
    /// fewer survivors there is less harvest pressure, so the stock can sit higher
    /// even as its productive capacity is throttled — population is the clean,
    /// monotone damage signal.)
    #[test]
    fn warming_lowers_carrying_capacity_versus_a_clean_control() {
        for seed in [1u64, 7, 42] {
            let mut p = Primitives::warming_world();
            p.seed = seed;
            let mut dirty = World::new(p.clone());

            let mut clean_p = p.clone();
            clean_p.emission_factor = 0.0;
            let mut clean = World::new(clean_p);

            for _ in 0..400 {
                dirty.step();
                clean.step();
            }
            let md = measure(&dirty);
            let mc = measure(&clean);

            assert!(
                md.population < mc.population,
                "warming should lower the carrying capacity (seed {seed}): dirty {} vs clean {}",
                md.population,
                mc.population
            );
        }
    }

    /// (c) **A decarbonising rule lowers the emergent temperature.** Same seed,
    /// same biology and geography: a `Decarbonize` mandate (abating most of the
    /// carbon intensity of output) yields a strictly cooler planet than the
    /// unmitigated baseline, and — via the regrowth feedback — a larger surviving
    /// population. The policy molds only the emission mechanism; the temperature
    /// it produces is measured. (Pigou; Budyko–Sellers.)
    #[test]
    fn a_decarbonising_rule_lowers_emergent_temperature() {
        for seed in [1u64, 7, 42] {
            let mut p = Primitives::warming_world();
            p.seed = seed;

            let mut baseline = World::new(p.clone());
            for _ in 0..400 {
                baseline.step();
            }

            let mut green = World::new(p.clone());
            let rules = boxed(vec![Box::new(Decarbonize::new(0.8)) as Box<dyn Rule>]);
            for _ in 0..400 {
                green.step_with_rules(&rules);
            }

            assert!(
                green.temperature() < baseline.temperature(),
                "decarbonising should lower the emergent temperature (seed {seed}): {} vs {}",
                green.temperature(),
                baseline.temperature()
            );
            assert!(
                green.greenhouse_stock() < baseline.greenhouse_stock(),
                "decarbonising should lower the greenhouse stock (seed {seed})"
            );
            assert!(
                green.agents.alive_count() > baseline.agents.alive_count(),
                "a cooler planet should carry more people (seed {seed}): {} vs {}",
                green.agents.alive_count(),
                baseline.agents.alive_count()
            );
        }
    }

    /// (d) **Determinism preserved** with the climate subsystem on: same seed ⇒
    /// bit-identical temperature, greenhouse stock and population every tick.
    #[test]
    fn climate_is_deterministic() {
        let mut p = Primitives::warming_world();
        p.seed = 9;
        let mut a = World::new(p.clone());
        let mut b = World::new(p);
        for _ in 0..250 {
            a.step();
            b.step();
            assert_eq!(a.temperature().to_bits(), b.temperature().to_bits());
            assert_eq!(a.greenhouse_stock().to_bits(), b.greenhouse_stock().to_bits());
            assert_eq!(a.emissions_flow().to_bits(), b.emissions_flow().to_bits());
            assert_eq!(a.agents.alive_count(), b.agents.alive_count());
        }
    }

    /// The climate **instruments** are wired into the measurement struct and
    /// report sane physical values on a warmed world (temperature in Kelvin, a
    /// positive greenhouse stock and emissions flow, a positive sensitivity).
    #[test]
    fn climate_instruments_report_measured_values() {
        let mut p = Primitives::warming_world();
        p.seed = 42;
        let mut w = World::new(p);
        for _ in 0..200 {
            w.step();
        }
        let m = measure(&w);
        assert!(m.temperature > 200.0 && m.temperature < 400.0, "T in K: {}", m.temperature);
        assert!(m.greenhouse_stock > 0.0);
        assert!(m.emissions > 0.0, "a producing economy should emit");
        assert!(m.climate_sensitivity > 0.0);
    }
}

#[cfg(test)]
mod phase6_collective_choice_tests {
    //! Phase 6 — EMERGENT COLLECTIVE CHOICE. The active rule set is no longer
    //! hand-picked: each agent has a policy **preference** read off its own
    //! measured situation (wealth relative to the mean, local scarcity, warming
    //! exposure), and a **collective-choice mechanism** aggregates those into the
    //! rules in force each term. Which policies a society adopts therefore EMERGES.
    //! Two mechanisms are compared as structural options: one-person-one-vote
    //! (median-voter, Downs) and wealth-weighted voting (elite capture, Acemoglu &
    //! Robinson); enactment needs a support threshold (collective-action cost,
    //! Olson). All preferences are derived read-only over `&World`; the
    //! consequences (does the Gini fall? does decarbonisation win?) are MEASURED.
    use super::instruments::measure;
    use super::polity::{govern, ChoiceMechanism, Polity, PolicyOption};
    use super::world::{Primitives, World};

    /// Run a world far enough that an unequal wealth distribution has emerged
    /// (the substrate for a redistributive coalition).
    fn unequal_world(seed: u64) -> World {
        let mut p = Primitives::demo();
        p.seed = seed;
        p.n_agents = 800;
        let mut w = World::new(p);
        for _ in 0..150 {
            w.step();
        }
        w
    }

    /// (a) **A majority adopts redistribution and the measured Gini falls — the
    /// policy EMERGED from preferences, it was never set.** On an unequal
    /// population, because wealth is right-skewed a majority sits *below the mean*
    /// (Meltzer–Richard), so under one-person-one-vote the redistribution option
    /// clears the threshold. Applying the elected rule set for a stretch then
    /// compresses the wealth distribution: the Gini drops below where it started.
    #[test]
    fn majority_adopts_redistribution_and_gini_falls() {
        for seed in [1u64, 7, 42, 100, 5] {
            let base = unequal_world(seed);
            let g0 = measure(&base).wealth_gini;
            assert!(g0 > 0.0, "inequality should have emerged first (seed {seed})");

            // Hold ONE election on the unequal population, then live under its
            // verdict (a long term, so we read its consequence, not churn).
            let mut polity = Polity::new(ChoiceMechanism::Majority, u64::MAX);
            polity.hold_election(&base);
            assert!(
                polity.is_active(PolicyOption::Redistribution),
                "a below-mean majority should ELECT redistribution (seed {seed}); active = {:?}",
                polity.active_policies()
            );

            let mut w = base.clone();
            let rules = polity.active_rules();
            for _ in 0..120 {
                w.step_with_rules(&rules);
            }
            let g1 = measure(&w).wealth_gini;
            assert!(
                g1 < g0,
                "the elected redistribution should lower the measured Gini (seed {seed}): {g1} vs {g0}"
            );
        }
    }

    /// (b) **Institutions shape outcomes: the SAME population elects DIFFERENT
    /// rule sets under different mechanisms.** One-person-one-vote hands power to
    /// the below-mean majority (redistribution wins); weighting votes by wealth
    /// hands it to the rich, who oppose the tax and instead secure property rights
    /// (Acemoglu & Robinson). The two active rule sets are not equal — the choice
    /// of mechanism, not just the population, determines the institutions.
    #[test]
    fn wealth_weighting_selects_a_different_rule_set_than_majority() {
        for seed in [1u64, 7, 42, 100, 5] {
            let base = unequal_world(seed);

            let mut maj = Polity::new(ChoiceMechanism::Majority, u64::MAX);
            maj.hold_election(&base);
            let mut elite = Polity::new(ChoiceMechanism::WealthWeighted, u64::MAX);
            elite.hold_election(&base);

            let mut a = maj.active_policies().to_vec();
            let mut b = elite.active_policies().to_vec();
            a.sort_by_key(|o| o.name());
            b.sort_by_key(|o| o.name());
            assert_ne!(
                a, b,
                "mechanisms should select different rule sets (seed {seed}): maj {a:?} vs elite {b:?}"
            );
            // Specifically: the majority redistributes, the plutocracy does not but
            // entrenches property rights instead.
            assert!(maj.is_active(PolicyOption::Redistribution));
            assert!(!elite.is_active(PolicyOption::Redistribution));
            assert!(elite.is_active(PolicyOption::PropertyRights));
        }
    }

    /// (c) **As warming bites, agents adopt a decarbonising rule.** On the climate
    /// preset, an election held while the planet sits at its pre-industrial steady
    /// state enacts no decarbonisation (no one is exposed to warming yet). After an
    /// ungoverned warming run heats the planet above the productivity optimum, the
    /// same population — now exposed — votes the decarbonisation option in. The
    /// policy emerges from the measured climate signal, not a script.
    #[test]
    fn warming_makes_agents_adopt_decarbonization() {
        for seed in [1u64, 7, 42] {
            let mut p = Primitives::warming_world();
            p.seed = seed;

            // Cold start: at the steady state, no warming exposure → no mandate.
            let cold = World::new(p.clone());
            let mut cold_pol = Polity::new(ChoiceMechanism::Majority, 25);
            cold_pol.hold_election(&cold);
            assert!(
                !cold_pol.is_active(PolicyOption::Decarbonization),
                "a cold (pre-industrial) world should not elect decarbonisation (seed {seed})"
            );

            // Let the planet warm with no policy, then re-poll the population.
            let mut hot = World::new(p.clone());
            for _ in 0..400 {
                hot.step();
            }
            assert!(
                hot.temperature() > hot.params().temp_opt,
                "the ungoverned world should warm above the optimum (seed {seed})"
            );
            let mut hot_pol = Polity::new(ChoiceMechanism::Majority, 25);
            hot_pol.hold_election(&hot);
            assert!(
                hot_pol.is_active(PolicyOption::Decarbonization),
                "a warmed population should ELECT decarbonisation (seed {seed}); share = {}",
                hot_pol.vote_share(PolicyOption::Decarbonization)
            );
        }
    }

    /// The `govern` driver runs full electoral terms and exposes an active-rule
    /// **timeline** via its observer hook (the interface visualisation builds on),
    /// records vote shares and a turnover count, and keeps the society alive.
    #[test]
    fn govern_runs_terms_and_records_a_timeline() {
        let mut p = Primitives::demo();
        p.seed = 42;
        p.n_agents = 600;
        let mut w = World::new(p);
        let mut polity = Polity::new(ChoiceMechanism::Majority, 20);

        let mut timeline: Vec<(u64, Vec<PolicyOption>)> = Vec::new();
        govern(&mut w, &mut polity, 200, |tick, pol| {
            timeline.push((tick, pol.active_policies().to_vec()));
        });

        assert_eq!(timeline.len(), 200, "one timeline entry per tick");
        assert!(w.agents.alive_count() > 0, "society should survive self-government");
        assert!(polity.elections() >= 10, "≈ ticks/period elections held");
        // Vote shares are measured fractions in [0,1].
        for opt in PolicyOption::ALL {
            let s = polity.vote_share(opt);
            assert!((0.0..=1.0).contains(&s), "{} share out of range: {s}", opt.name());
        }
        // Turnover is a non-negative measured count.
        let _ = polity.turnover();
    }

    /// (d) **Determinism preserved**: same seed and mechanism ⇒ a bit-identical
    /// governed history — the active-rule timeline, the redistribution vote-share
    /// series (to the bit), and the final population all match across two runs.
    #[test]
    fn collective_choice_is_deterministic() {
        let mut p = Primitives::demo();
        p.seed = 9;
        p.n_agents = 600;

        let run = || {
            let mut w = World::new(p.clone());
            let mut pol = Polity::new(ChoiceMechanism::WealthWeighted, 15);
            let mut trace: Vec<(u64, u64, Vec<&'static str>)> = Vec::new();
            govern(&mut w, &mut pol, 180, |tick, polity| {
                let active: Vec<&'static str> =
                    polity.active_policies().iter().map(|o| o.name()).collect();
                // Compare the vote share to the bit, alongside the active set.
                let bits = polity.vote_share(PolicyOption::Redistribution).to_bits();
                trace.push((tick, bits, active));
            });
            (trace, w.agents.alive_count())
        };
        let (ta, pa) = run();
        let (tb, pb) = run();
        assert_eq!(ta, tb, "governed traces must be bit-identical");
        assert_eq!(pa, pb, "final populations must match");
    }
}

#[cfg(test)]
mod phase8_scaling_tests {
    //! Phase 8 — DETERMINISTIC PARALLEL SCALING. The order-independent phases
    //! (per-cell substrate regrowth; the per-agent wealth valuation inside
    //! `measure`) are partitioned across worker threads, while every
    //! order-dependent phase (movement, bilateral trade, reproduction,
    //! enforcement, the RNG-consuming vital-events loop) stays strictly
    //! sequential. Parallelism here is a *speed* choice only: the engine remains
    //! **bit-deterministic**, so the parallel path must reproduce the
    //! single-threaded golden oracle exactly. We pin that down on a world large
    //! enough to actually cross the threading threshold, then smoke-test a
    //! six-figure population for a few ticks.
    use super::instruments::{measure, Measurements};
    use super::parallel::set_max_threads;
    use super::world::{Primitives, World};

    /// Project a `Measurements` to a bit-comparable fingerprint (every field as
    /// raw bits, so any floating-point divergence is caught — not just the
    /// integer counts).
    fn fingerprint(m: &Measurements) -> Vec<u64> {
        vec![
            m.tick,
            m.population as u64,
            m.mean_wealth.to_bits(),
            m.wealth_gini.to_bits(),
            m.life_expectancy.to_bits(),
            m.resource_stock.to_bits(),
            m.production.to_bits(),
            m.price_index.to_bits(),
            m.trade_count as u64,
            m.trade_volume.to_bits(),
            m.gdp_flow.to_bits(),
            m.specialization.to_bits(),
            m.commons_health.to_bits(),
            m.state_capacity.to_bits(),
            m.legitimacy.to_bits(),
            m.corruption.to_bits(),
            m.public_pool.to_bits(),
            m.temperature.to_bits(),
            m.greenhouse_stock.to_bits(),
            m.emissions.to_bits(),
            m.climate_sensitivity.to_bits(),
        ]
    }

    /// Record a full bit-fingerprint series over a run, under a given worker cap.
    /// A grid above the threading threshold (≈16k cells) ensures regrowth and the
    /// `measure` wealth-map genuinely run multi-threaded when the cap allows it.
    fn fingerprint_series(threads: usize, seed: u64, ticks: usize) -> Vec<Vec<u64>> {
        set_max_threads(threads);
        // A grid above the threading threshold seeded with thousands of agents,
        // so trade, reproduction and degradation all fire *and* both
        // data-parallel phases (regrowth; the `measure` wealth-map) genuinely run
        // multi-threaded when the cap allows it.
        let mut p = Primitives::large_world(90_000, 8_000);
        p.seed = seed;
        let mut w = World::new(p);
        let mut series = Vec::with_capacity(ticks + 1);
        series.push(fingerprint(&measure(&w)));
        for _ in 0..ticks {
            w.step();
            series.push(fingerprint(&measure(&w)));
        }
        series
    }

    /// (a) **Parallel ≡ sequential.** The same seed produces a bit-identical
    /// `Measurements` *series* on 1 worker (the canonical oracle) and on several
    /// multi-thread configurations — proving the data-parallel regrowth and the
    /// parallel wealth valuation change nothing the instruments can see.
    #[test]
    fn parallel_matches_sequential_bit_for_bit() {
        let _g = super::parallel::THREAD_CAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let seq = fingerprint_series(1, 7, 40);
        for threads in [2usize, 4, 8] {
            let par = fingerprint_series(threads, 7, 40);
            assert_eq!(
                seq, par,
                "parallel ({threads} threads) must reproduce the sequential series bit-for-bit"
            );
        }
        set_max_threads(1);
    }

    /// (a′) Determinism under a fixed (multi-thread) config: two runs at the same
    /// seed and the same worker cap are identical (no thread-interleaving leak).
    #[test]
    fn parallel_runs_are_deterministic() {
        let _g = super::parallel::THREAD_CAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = fingerprint_series(4, 13, 30);
        let b = fingerprint_series(4, 13, 30);
        assert_eq!(a, b, "same seed + same worker cap ⇒ bit-identical series");
        set_max_threads(1);
    }

    /// (b) **Scale smoke test.** A six-figure population on a large grid runs a
    /// few ticks without panicking, the population stays positive, and inequality
    /// still EMERGES from the equal start (the Phase-1 thesis holds at scale).
    #[test]
    fn large_population_runs_without_panic() {
        let _g = super::parallel::THREAD_CAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Default worker cap (machine cores) — exercises the real parallel path.
        set_max_threads(super::parallel::max_threads());
        let mut p = Primitives::large_world(400_000, 100_000);
        p.seed = 1;
        let mut w = World::new(p);
        let seeded = w.agents.alive_count();
        assert!(seeded > 50_000, "should seed a six-figure population, got {seeded}");
        let start = measure(&w);
        assert!(start.wealth_gini.abs() < 1e-9, "equal start");
        for _ in 0..3 {
            w.step();
        }
        let end = measure(&w);
        assert!(end.population > 0, "population must survive the smoke run");
        assert!(end.wealth_gini > 0.0, "inequality must emerge at scale");
        set_max_threads(1);
    }

    /// The thread cap is purely a performance knob: setting it never changes what
    /// the engine computes (guarded explicitly so a future refactor can't quietly
    /// make a result depend on the worker count).
    #[test]
    fn thread_cap_does_not_change_results() {
        let _g = super::parallel::THREAD_CAP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let one = fingerprint_series(1, 99, 25);
        let many = fingerprint_series(6, 99, 25);
        assert_eq!(one, many, "the worker cap must not affect results");
        set_max_threads(1);
    }
}

