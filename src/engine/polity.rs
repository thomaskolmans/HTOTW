//! **Emergent collective choice** (Phase 6): the *rules themselves* emerge from
//! agent preferences instead of being hand-picked by the experimenter.
//!
//! In Phases 3–5 the analyst chose which [`Rule`]s were active. Here that choice
//! is **endogenised**. Each agent has a **policy preference** read purely off its
//! own measured situation — its wealth percentile, the local resource scarcity it
//! faces, and (under a warming world) its exposure to a rising temperature. A
//! **collective-choice mechanism** then aggregates those preferences into the
//! active rule set for the next political term. *Which policies a society adopts
//! is therefore measured out of the population, never set.*
//!
//! Two mechanisms are provided as **structural options** to be compared:
//!
//! - [`ChoiceMechanism::Majority`] — one-person-one-vote. Each agent casts a unit
//!   ballot for the options it supports; the winners are the options with majority
//!   support. This is the **median-voter** logic (Downs 1957): the decisive voter
//!   is the median of the preference distribution, so a society whose median agent
//!   is poor adopts redistribution.
//! - [`ChoiceMechanism::WealthWeighted`] — votes weighted by wealth (a plutocracy
//!   / elite-capture rule). The decisive voter is now the *wealth-weighted* median,
//!   so the same population can select a **different** rule set: the rich, who
//!   oppose taxes and favour property rights, carry more weight. (Acemoglu &
//!   Robinson 2006: institutions reflect who holds *de facto* power.)
//!
//! Both must clear a per-option **support threshold** to be enacted — a coarse
//! model of the collective-action cost of organising to change the rules (Olson
//! 1965: diffuse interests under-provide the public good of institutional change).
//!
//! Preferences are derived by [`agent_support`], which takes `&World` and an agent
//! index and returns each option's support — it cannot mutate the world, so a
//! measured aggregate (the wealth percentile, the scarcity) can never be fed back
//! as an input. The polity reads support, tallies votes, and selects rules; the
//! *consequences* (does the Gini fall? does temperature stabilise?) are then
//! MEASURED by the [`crate::engine::instruments`] exactly like every other
//! emergent number.
//!
//! ## Citations
//! - Downs (1957), *An Economic Theory of Democracy* — the median-voter theorem:
//!   majority rule converges on the policy preferred by the median voter.
//! - Olson (1965), *The Logic of Collective Action* — organising to change the
//!   rules is itself a costly public good, so change needs a threshold of support.
//! - Acemoglu & Robinson (2006), *Economic Origins of Dictatorship and Democracy*
//!   — institutions are chosen by whoever holds power; weighting votes by wealth
//!   (or handing them to an elite) selects different, self-serving rules.

use super::institutions::{
    Decarbonize, HarvestQuota, OpenAccess, PropertyRights, Redistribute, Rule, WealthTax,
};
use super::world::{World, NGOODS};

/// The menu of mutually-composable policies a society can adopt. Each maps to one
/// or more Phase-3/5 [`Rule`]s from the existing catalogue — the polity selects
/// from this menu; it never invents an outcome. `OpenAccess` is the null option
/// (no managing rule), always implicitly available so a society can choose to do
/// nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolicyOption {
    /// Progressive wealth tax funding a means-tested transfer (the redistributive
    /// bundle). Favoured by below-median-wealth agents.
    Redistribution,
    /// A conservation harvest quota. Favoured by agents facing local scarcity.
    Conservation,
    /// Property rights (homestead the occupied cell). Favoured by the wealthy, who
    /// gain most from securing what they hold.
    PropertyRights,
    /// A decarbonisation mandate. Favoured by agents exposed to warming.
    Decarbonization,
}

impl PolicyOption {
    /// All selectable options, in a fixed order (determinism: vote-tally and
    /// rule-application order are stable).
    pub const ALL: [PolicyOption; 4] = [
        PolicyOption::Redistribution,
        PolicyOption::Conservation,
        PolicyOption::PropertyRights,
        PolicyOption::Decarbonization,
    ];

    /// A short stable name (for the active-rule timeline / reports).
    pub fn name(self) -> &'static str {
        match self {
            PolicyOption::Redistribution => "redistribution",
            PolicyOption::Conservation => "conservation",
            PolicyOption::PropertyRights => "property-rights",
            PolicyOption::Decarbonization => "decarbonization",
        }
    }

    /// Instantiate the concrete Phase-3/5 [`Rule`]s this option enacts. Strengths
    /// are fixed, sensible mechanism settings — the *choice* of whether to enact is
    /// what emerges, not a tuned magnitude.
    fn rules(self) -> Vec<Box<dyn Rule>> {
        match self {
            PolicyOption::Redistribution => vec![
                Box::new(WealthTax::new(0.2)),
                Box::new(Redistribute::new(1.0)),
            ],
            PolicyOption::Conservation => vec![Box::new(HarvestQuota::new(0.3))],
            PolicyOption::PropertyRights => vec![Box::new(PropertyRights)],
            PolicyOption::Decarbonization => vec![Box::new(Decarbonize::new(0.8))],
        }
    }
}

/// How the population's preferences are aggregated into the active rule set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChoiceMechanism {
    /// One person, one vote (the median-voter rule, Downs).
    Majority,
    /// Votes weighted by agent wealth (plutocracy / elite capture; Acemoglu &
    /// Robinson). The same population can select a different rule set.
    WealthWeighted,
}

/// The endogenous government: a mechanism, an electoral period, and the result of
/// the most recent election (the active rule set + the vote share each option
/// received + a turnover count). The active rules are *measured out* of agent
/// preferences each term — never set by the experimenter.
pub struct Polity {
    /// The aggregation rule (a structural option, compared in tests).
    pub mechanism: ChoiceMechanism,
    /// Length of an electoral term in ticks: the rules are fixed for this many
    /// steps, then re-decided. (Institutions are sticky between elections.)
    pub period: u64,
    /// The fraction of (weighted) votes an option needs to be enacted — the
    /// collective-action threshold (Olson). 0.5 = a simple majority.
    pub threshold: f64,
    /// The options enacted by the most recent election (the active rule set).
    active: Vec<PolicyOption>,
    /// Vote share `[0,1]` each option received at the most recent election, in
    /// [`PolicyOption::ALL`] order (a MEASURED aggregate of preferences).
    vote_share: [f64; PolicyOption::ALL.len()],
    /// Cumulative count of options that *changed* status (enacted↔repealed)
    /// across elections — an emergent **policy turnover** measure.
    turnover: u64,
    /// Number of elections held so far.
    elections: u64,
    /// Whether an election has been held yet (so the first `vote_share` read is
    /// meaningful).
    seeded: bool,
}

impl Polity {
    /// A new polity with the given mechanism and electoral period, a simple
    /// majority threshold, and an empty (do-nothing) initial rule set.
    pub fn new(mechanism: ChoiceMechanism, period: u64) -> Polity {
        Polity {
            mechanism,
            period: period.max(1),
            threshold: 0.5,
            active: Vec::new(),
            vote_share: [0.0; PolicyOption::ALL.len()],
            turnover: 0,
            elections: 0,
            seeded: false,
        }
    }

    /// Set the support threshold (clamped to `[0,1]`). Builder-style.
    pub fn with_threshold(mut self, threshold: f64) -> Polity {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// The options currently in force (the active rule set) — MEASURED.
    pub fn active_policies(&self) -> &[PolicyOption] {
        &self.active
    }

    /// Whether a given option is currently enacted.
    pub fn is_active(&self, option: PolicyOption) -> bool {
        self.active.contains(&option)
    }

    /// Vote share `[0,1]` an option received at the most recent election.
    pub fn vote_share(&self, option: PolicyOption) -> f64 {
        let i = PolicyOption::ALL.iter().position(|&o| o == option).unwrap();
        self.vote_share[i]
    }

    /// Cumulative **policy turnover**: how many option enact/repeal flips have
    /// happened across all elections — an emergent measure of institutional churn.
    pub fn turnover(&self) -> u64 {
        self.turnover
    }

    /// Number of elections held.
    pub fn elections(&self) -> u64 {
        self.elections
    }

    /// Build the concrete [`Rule`] stack for the active policies. Always begins
    /// with [`OpenAccess`] so the harvest mechanism is explicitly reset each tick
    /// (the null institution), then layers the enacted options in fixed order.
    pub fn active_rules(&self) -> Vec<Box<dyn Rule>> {
        let mut rules: Vec<Box<dyn Rule>> = vec![Box::new(OpenAccess)];
        // Apply in ALL order for determinism, regardless of enactment order.
        for &opt in &PolicyOption::ALL {
            if self.active.contains(&opt) {
                rules.extend(opt.rules());
            }
        }
        rules
    }

    /// Hold an election: read every living agent's preferences off the world,
    /// tally them under the mechanism, and update the active rule set, vote shares
    /// and turnover. Read-only over agents — only the polity's own ledgers mutate.
    pub fn hold_election(&mut self, world: &World) {
        let n = world.agents.len();
        let opts = PolicyOption::ALL;

        // Accumulate, per option, the weighted votes FOR and the total weight, so
        // the share is votes_for / total_weight (turnout-normalised support).
        let mut votes_for = [0.0f64; PolicyOption::ALL.len()];
        let mut total_weight = 0.0f64;

        // Precompute the wealth ranking once (read-only) so each agent's wealth
        // percentile — the heart of its preferences — is a measured aggregate.
        let ranking = WealthRanking::new(world);

        for i in 0..n {
            if !world.agents.alive[i] {
                continue;
            }
            let weight = match self.mechanism {
                ChoiceMechanism::Majority => 1.0,
                // Wealth-weighted: an agent's clout is its wealth (a small floor so
                // even a pauper has an infinitesimal voice; the rich dominate).
                ChoiceMechanism::WealthWeighted => ranking.wealth(i).max(1e-6),
            };
            total_weight += weight;
            let support = agent_support(world, i, &ranking);
            for (k, s) in support.iter().enumerate() {
                if *s {
                    votes_for[k] += weight;
                }
            }
        }

        // Decide each option: enacted iff its weighted support share clears the
        // threshold. Record the share for the instrument.
        let mut new_active: Vec<PolicyOption> = Vec::new();
        for (k, &opt) in opts.iter().enumerate() {
            let share = if total_weight > 0.0 {
                votes_for[k] / total_weight
            } else {
                0.0
            };
            self.vote_share[k] = share;
            if share > self.threshold {
                new_active.push(opt);
            }
        }

        // Turnover: count options whose enacted-status flipped vs the prior term
        // (only after the first election, so the initial enactment isn't churn).
        if self.seeded {
            for &opt in &opts {
                let was = self.active.contains(&opt);
                let now = new_active.contains(&opt);
                if was != now {
                    self.turnover += 1;
                }
            }
        }

        self.active = new_active;
        self.elections += 1;
        self.seeded = true;
    }
}

/// A read-only **wealth ranking** of the living population, computed once per
/// election. Holds each agent's measured wealth and its percentile in `[0,1]`
/// (0 = poorest, 1 = richest). Wealth is the energy reserve plus the physical
/// good bundle — the same quantity the Gini instrument measures. Purely derived
/// from agent state; nothing is set.
pub struct WealthRanking {
    wealth: Vec<f64>,
    percentile: Vec<f64>,
    mean: f64,
}

impl WealthRanking {
    /// Build from the world (read-only).
    pub fn new(world: &World) -> WealthRanking {
        let n = world.agents.len();
        let mut wealth = vec![0.0; n];
        for (i, w) in wealth.iter_mut().enumerate() {
            if world.agents.alive[i] {
                let g = &world.agents.good[i];
                *w = world.agents.energy[i] + g[0] + g[1];
            }
        }
        // Percentile = fraction of living agents with strictly less wealth.
        // Computed by sorting the living wealths once (deterministic).
        let mut living: Vec<f64> = (0..n)
            .filter(|&i| world.agents.alive[i])
            .map(|i| wealth[i])
            .collect();
        living.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let m = living.len();
        let mean = if m > 0 {
            living.iter().sum::<f64>() / m as f64
        } else {
            0.0
        };
        let mut percentile = vec![0.0; n];
        for i in 0..n {
            if !world.agents.alive[i] {
                continue;
            }
            if m <= 1 {
                percentile[i] = 0.5;
                continue;
            }
            // Count strictly-less via binary search on the sorted living wealths.
            let w = wealth[i];
            let lo = living.partition_point(|&x| x < w);
            percentile[i] = lo as f64 / (m - 1) as f64;
        }
        WealthRanking { wealth, percentile, mean }
    }

    /// Agent `i`'s measured wealth.
    pub fn wealth(&self, i: usize) -> f64 {
        self.wealth[i]
    }
    /// Agent `i`'s wealth percentile in `[0,1]` (0 poorest, 1 richest).
    pub fn percentile(&self, i: usize) -> f64 {
        self.percentile[i]
    }
    /// Mean wealth of the living population (a measured aggregate).
    pub fn mean(&self) -> f64 {
        self.mean
    }
}

/// Read agent `i`'s **policy preferences** off its measured situation, returning a
/// boolean support flag per [`PolicyOption`] (in `ALL` order). Each preference is
/// derived from a primitive measured fact about the agent — its wealth percentile,
/// the local resource scarcity at its cell, and (when climate is on) its exposure
/// to warming. *No party label or assumed ideology is used.* Read-only over the
/// world: the function takes `&World` and cannot mutate it, so the measured
/// position can never be fed back as a control input.
pub fn agent_support(
    world: &World,
    i: usize,
    ranking: &WealthRanking,
) -> [bool; PolicyOption::ALL.len()] {
    let wealth = ranking.wealth(i);
    let mean = ranking.mean();
    // (The percentile is available via `ranking.percentile(i)` for callers that
    // want a rank-based preference; this default keys redistribution off the
    // measured mean per Meltzer–Richard.)
    // Below-mean wealth is the redistribution constituency (Meltzer–Richard 1981):
    // because wealth is right-skewed, MORE than half the population sits below the
    // mean, so in an unequal society a majority can form for a transfer — the size
    // of that coalition EMERGES from the measured distribution, never set.
    let below_mean = wealth < mean;

    // Local scarcity: how depleted the agent's own cell is relative to its
    // carrying capacity (1 = stripped bare, 0 = full). A genuinely measured
    // ecological signal the agent perceives where it stands.
    let cell = world.agents.cell[i];
    let scarcity = {
        let cap = &world.substrate.capacity[cell];
        let res = &world.substrate.resource[cell];
        let mut k = 0.0;
        let mut s = 0.0;
        for g in 0..NGOODS {
            k += cap[g];
            s += res[g];
        }
        if k > 0.0 {
            (1.0 - (s / k)).clamp(0.0, 1.0)
        } else {
            1.0
        }
    };

    // Warming exposure: how far the planet has warmed above the productivity
    // optimum, normalised by the thermal tolerance. Zero when climate is off (the
    // pre-industrial steady state), so decarbonisation only attracts support once
    // warming actually bites. A measured climate signal.
    let warming = if world.params().climate_enabled {
        let t = world.temperature();
        let opt = world.params().temp_opt;
        ((t - opt) / world.params().temp_tolerance).max(0.0)
    } else {
        0.0
    };

    // Fair-mindedness (Phase 9, only when psychology is on): a strongly
    // inequity-averse agent dislikes *advantageous* inequality too (the
    // Fehr–Schmidt β), so even an above-mean agent supports a redistributive
    // floor if its measured fairness trait is high. With psychology off this
    // channel is inert and the preference is exactly the Meltzer–Richard one.
    let fair_minded = world.params().psyche_enabled && world.agents.fairness[i] > 0.65;

    let mut support = [false; PolicyOption::ALL.len()];
    for (k, &opt) in PolicyOption::ALL.iter().enumerate() {
        support[k] = match opt {
            // The below-mean favour redistribution & the tax that funds it; the
            // poorer you are, the more you stand to gain from a transfer — and
            // the fair-minded support the floor regardless of their own rank.
            PolicyOption::Redistribution => below_mean || fair_minded,
            // Anyone facing material local scarcity wants the commons conserved.
            PolicyOption::Conservation => scarcity > 0.5,
            // The wealthy favour securing what they hold (property rights) and
            // oppose redistribution: above-mean wealth supports it.
            PolicyOption::PropertyRights => !below_mean,
            // Agents exposed to real warming want it abated.
            PolicyOption::Decarbonization => warming > 0.25,
        };
    }
    support
}

/// Drive a [`World`] forward for `ticks` steps under an **endogenous government**:
/// every `polity.period` ticks an election is held (preferences read off the
/// population, aggregated by the mechanism), the winning policies become the
/// active rule set, and that set is applied via [`World::step_with_rules`] for the
/// rest of the term. The active-rule timeline can be recorded by the optional
/// `observer`, called once per tick with `(tick, &Polity)` *after* the step — a
/// clean hook for visualisation of which rules a society held when. Deterministic:
/// elections read state read-only and rule application uses the world's own RNG
/// stream.
pub fn govern<F: FnMut(u64, &Polity)>(
    world: &mut World,
    polity: &mut Polity,
    ticks: u64,
    mut observer: F,
) {
    for _ in 0..ticks {
        // Election at the start of each term (and at t=0): the rules for the term
        // are decided from the population's current measured preferences.
        if world.tick % polity.period == 0 {
            polity.hold_election(world);
        }
        let rules = polity.active_rules();
        world.step_with_rules(&rules);
        observer(world.tick, polity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::world::Primitives;

    #[test]
    fn active_rules_always_reset_harvest_mechanism() {
        let p = Polity::new(ChoiceMechanism::Majority, 20);
        let rules = p.active_rules();
        // Even with no enacted policy, OpenAccess is present to reset the quota.
        assert_eq!(rules[0].name(), "open-access");
    }

    #[test]
    fn percentiles_span_the_population() {
        let mut w = World::new(Primitives::demo());
        for _ in 0..120 {
            w.step();
        }
        let r = WealthRanking::new(&w);
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for i in 0..w.agents.len() {
            if w.agents.alive[i] {
                min = min.min(r.percentile(i));
                max = max.max(r.percentile(i));
            }
        }
        assert!(min < 0.1 && max > 0.9, "percentiles should span [0,1]: {min}..{max}");
    }
}
