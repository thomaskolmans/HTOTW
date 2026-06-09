//! **Instruments**: read-only observers that *compute* macro quantities from
//! raw agent state. They take `&World` — by type they cannot mutate it — so a
//! measured aggregate can **never** feed back as an input. This is the
//! architectural enforcement of the project's hard rule: *macro is measured,
//! never set.*
//!
//! Every number here (population, the wealth **Gini**, mean wealth, life
//! expectancy, and the Phase-2 **price index, trade volume, GDP and
//! specialization**) is derived from the population the engine produced. None of
//! them appears anywhere as an input. Hayek's point made literal: the price is
//! not assumed, it is *read off* the realised exchanges the agents made.

use super::world::{World, NGOODS};

/// A single emergent measurement taken from a finished or in-progress world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurements {
    pub tick: u64,
    /// Living population (count of alive agents).
    pub population: usize,
    /// Mean agent wealth, valued at the emergent price index (see `wealth`).
    pub mean_wealth: f64,
    /// Gini coefficient of agent wealth (price-valued bundle + energy) — EMERGENT.
    pub wealth_gini: f64,
    /// Mean realised lifespan so far (life expectancy), or NaN before any death.
    pub life_expectancy: f64,
    /// Total resource standing in the landscape (ecological stock).
    pub resource_stock: f64,
    /// Goods harvested this tick across both goods (an emergent production flow).
    pub production: f64,
    /// **Emergent price**: volume-weighted median realised exchange ratio this
    /// tick (units of good 1 per unit of good 0). NaN if nothing traded.
    pub price_index: f64,
    /// Number of bilateral trades executed this tick.
    pub trade_count: usize,
    /// Trade volume this tick = total units of goods that changed hands.
    pub trade_volume: f64,
    /// **GDP as a flow**: value (in good-1 numéraire) of goods traded this tick
    /// at realised prices. A live measure of market activity.
    pub gdp_flow: f64,
    /// Mean per-agent output **specialization** in `[0,1]`: 0 = a perfect
    /// generalist (harvests both goods equally), 1 = produces a single good.
    pub specialization: f64,
    /// Which good (if any) is the dominant **medium of exchange** so far, by
    /// cumulative acceptance-as-payment, or `None` if nothing has traded.
    pub dominant_medium: Option<usize>,

    // ---- Phase 3: governance & commons (all MEASURED, never set) ----
    /// **Commons health** in `[0,1]`: fraction of the landscape's *pristine*
    /// carrying capacity still standing (1 = untouched geography, →0 = degraded
    /// to dust). The tragedy of the commons shows up directly here.
    pub commons_health: f64,
    /// **Compliance rate** in `[0,1]`: share of rule encounters met by voluntary
    /// compliance (defaults to 1.0 when no rule is in force).
    pub compliance_rate: f64,
    /// **State capacity** in `[0,1]`: intended enforcement actually achieved.
    pub state_capacity: f64,
    /// **Legitimacy / tax morale** in `[0,1]`: voluntary compliance weighted by
    /// delivery quality — the feedback that lets institutions earn obedience.
    pub legitimacy: f64,
    /// **Corruption** in `[0,1]`: share of public-pool outflow diverted.
    pub corruption: f64,
    /// Current **public pool** size (good-1 numéraire) — an emergent reservoir.
    pub public_pool: f64,

    // ---- Phase 5: climate (energy-balance physics, all MEASURED) ----
    /// **Surface temperature** (K) from the zero-dimensional energy balance.
    pub temperature: f64,
    /// **Atmospheric greenhouse stock** `C_atm` (model units) — accumulated from
    /// emergent emissions with first-order decay.
    pub greenhouse_stock: f64,
    /// **Emissions flow** this tick (greenhouse units released by production).
    pub emissions: f64,
    /// **Equilibrium climate sensitivity** `ΔT` (K) at a *doubling* of the
    /// greenhouse stock — the Planck response read off this world's own physics.
    pub climate_sensitivity: f64,
}

/// Gini coefficient of a slice of non-negative values, via the sorted
/// cumulative formula `G = (2·Σ i·xᵢ)/(n·Σx) − (n+1)/n` (i 1-indexed, ascending).
/// Returns 0 for an empty set or all-equal/all-zero values.
pub fn gini(values: &[f64]) -> f64 {
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    let mut v: Vec<f64> = values.iter().map(|x| x.max(0.0)).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f64 = v.iter().sum();
    if sum <= 0.0 {
        return 0.0;
    }
    let weighted: f64 = v.iter().enumerate().map(|(i, x)| (i as f64 + 1.0) * x).sum();
    let nf = n as f64;
    ((2.0 * weighted) / (nf * sum) - (nf + 1.0) / nf).clamp(0.0, 1.0)
}

/// Volume-weighted median of `(value, weight)` pairs. The weighted median is
/// robust to the occasional extreme trade ratio, which is why we report it as
/// the price index rather than a mean. Returns NaN if there is no weight.
fn weighted_median(mut pairs: Vec<(f64, f64)>) -> f64 {
    if pairs.is_empty() {
        return f64::NAN;
    }
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = pairs.iter().map(|&(_, w)| w).sum();
    if total <= 0.0 {
        return f64::NAN;
    }
    let mut acc = 0.0;
    for &(v, w) in &pairs {
        acc += w;
        if acc >= total / 2.0 {
            return v;
        }
    }
    pairs.last().unwrap().0
}

/// The **emergent price index**: volume-weighted median of this tick's realised
/// trade ratios (good-1 per good-0). It is *read off* the ledger of trades the
/// agents actually made — never assumed (Hayek; Gode & Sunder).
pub fn price_index(world: &World) -> f64 {
    let pairs: Vec<(f64, f64)> = world
        .trades_this_tick
        .iter()
        .map(|t| (t.price, t.qty_good0))
        .collect();
    weighted_median(pairs)
}

/// Per-agent output **specialization** (Herfindahl-style concentration of an
/// agent's cumulative harvest across goods, rescaled to `[0,1]`), averaged over
/// living agents that have ever produced. Comparative advantage shows up here:
/// agents drift toward the good they (or their patch) are better at.
pub fn mean_specialization(world: &World) -> f64 {
    let mut sum = 0.0;
    let mut n = 0usize;
    for i in 0..world.agents.len() {
        if !world.agents.alive[i] {
            continue;
        }
        let h = &world.agents.harvested[i];
        let total: f64 = h.iter().sum();
        if total <= 0.0 {
            continue;
        }
        // Herfindahl H = Σ pᵢ². Rescale (H − 1/G)/(1 − 1/G) → 0=even, 1=single.
        let hhi: f64 = h.iter().map(|&x| (x / total).powi(2)).sum();
        let g = NGOODS as f64;
        let spec = ((hhi - 1.0 / g) / (1.0 - 1.0 / g)).clamp(0.0, 1.0);
        sum += spec;
        n += 1;
    }
    if n == 0 {
        0.0
    } else {
        sum / n as f64
    }
}

/// Total **need-satisfaction welfare**: the sum over living agents of their
/// bundle's satiation utility `Σ_g s_g/(s_g+scale)`. This is the quantity the
/// bilateral trade rule provably increases (each accepted trade strictly raises
/// both parties' satisfaction), so it is the right lens on *gains from trade* —
/// a read-only measure derived from holdings and the single need primitive.
pub fn total_welfare(world: &World) -> f64 {
    let scale = world.params().satiation_scale;
    let mut sum = 0.0;
    for i in 0..world.agents.len() {
        if !world.agents.alive[i] {
            continue;
        }
        for g in 0..NGOODS {
            let s = world.agents.good[i][g];
            sum += s / (s + scale);
        }
    }
    sum
}

/// Mean **subjective well-being** of the living population in `[0,1]` (Phase
/// 9): the average of each agent's slow EMA of realised need satisfaction
/// (Diener 1984; Kahneman & Krueger 2006 on measured life satisfaction). A
/// read-only readout of the `wellbeing` ledger — it starts neutral at 0.5 and
/// moves only as agents' lived satisfaction does; it is never set and never
/// feeds back. 0.5 (the neutral start) when no one is alive.
pub fn mean_wellbeing(world: &World) -> f64 {
    let mut sum = 0.0;
    let mut n = 0usize;
    for i in 0..world.agents.len() {
        if world.agents.alive[i] {
            sum += world.agents.wellbeing[i];
            n += 1;
        }
    }
    if n == 0 {
        0.5
    } else {
        sum / n as f64
    }
}

/// **Commons health**: the fraction of the landscape's pristine carrying
/// capacity that still stands. Open access mines cells past their regeneration
/// threshold and degrades capacity, dragging this toward 0 (Hardin); a quota or
/// property regime that leaves enough standing stock keeps it near 1 (Ostrom).
/// Read-only, computed from `capacity / capacity0` — never set.
pub fn commons_health(world: &World) -> f64 {
    let mut k = 0.0;
    let mut k0 = 0.0;
    for (cap, cap0) in world.substrate.capacity.iter().zip(world.substrate.capacity0.iter()) {
        for g in 0..NGOODS {
            k += cap[g];
            k0 += cap0[g];
        }
    }
    if k0 <= 0.0 {
        1.0
    } else {
        (k / k0).clamp(0.0, 1.0)
    }
}

/// **Compliance rate**: share of rule encounters met by voluntary compliance.
/// 1.0 when no rule has been encountered yet (nothing to comply with).
pub fn compliance_rate(world: &World) -> f64 {
    let total = world.compliance_events + world.defiance_events;
    if total == 0 {
        1.0
    } else {
        world.compliance_events as f64 / total as f64
    }
}

/// Per-agent total wealth = energy reserve + bundle valued at `price` (good-1
/// numéraire). Falls back to summing physical units when nothing has traded yet
/// (no emergent price exists), so wealth is always defined.
fn agent_wealth(world: &World, i: usize, price: f64) -> f64 {
    let g = &world.agents.good[i];
    let bundle_value = if price.is_finite() && price > 0.0 {
        g[0] * price + g[1]
    } else {
        g[0] + g[1]
    };
    world.agents.energy[i] + bundle_value
}

/// Per-agent wealth for **every slot** (alive or not), written by absolute index.
///
/// At scale this per-agent valuation is the bulk of `measure`'s cost, and it is
/// **order-independent and read-only** (each `out[i]` depends only on agent `i`
/// and the shared price), so it is computed data-parallel over disjoint index
/// chunks. Writing by absolute index keeps the result **bit-identical** to the
/// sequential map for any thread count (no cross-thread reduction); the caller
/// then filters the living and runs the order-sensitive sum/sort sequentially.
fn agent_wealth_all(world: &World, price: f64) -> Vec<f64> {
    let n = world.agents.len();
    let mut out = vec![0.0_f64; n];
    super::parallel::for_each_chunk_mut(&mut out, |start, chunk| {
        for (local, w) in chunk.iter_mut().enumerate() {
            let i = start + local;
            if world.agents.alive[i] {
                *w = agent_wealth(world, i, price);
            }
        }
    });
    out
}

/// Take a full set of measurements from the current world state.
pub fn measure(world: &World) -> Measurements {
    let price = price_index(world);

    let wealth_all = agent_wealth_all(world, price);
    let wealth: Vec<f64> = (0..world.agents.len())
        .filter(|&i| world.agents.alive[i])
        .map(|i| wealth_all[i])
        .collect();

    let population = wealth.len();
    let mean_wealth = if population == 0 {
        0.0
    } else {
        wealth.iter().sum::<f64>() / population as f64
    };

    let trade_count = world.trades_this_tick.len();
    let trade_volume: f64 = world
        .trades_this_tick
        .iter()
        .map(|t| t.qty_good0 + t.qty_good1)
        .sum();
    // GDP flow in good-1 numéraire: value of goods exchanged at realised prices.
    let gdp_flow: f64 = world
        .trades_this_tick
        .iter()
        .map(|t| t.qty_good0 * t.price + t.qty_good1)
        .sum();

    let dominant_medium = {
        let m = &world.medium_accept;
        let total: u64 = m.iter().sum();
        if total == 0 {
            None
        } else {
            let mut best = 0;
            for g in 1..NGOODS {
                if m[g] > m[best] {
                    best = g;
                }
            }
            Some(best)
        }
    };

    Measurements {
        tick: world.tick,
        population,
        mean_wealth,
        wealth_gini: gini(&wealth),
        life_expectancy: world.life_expectancy().unwrap_or(f64::NAN),
        resource_stock: world.total_resource(),
        production: world.harvested_this_tick.iter().sum(),
        price_index: price,
        trade_count,
        trade_volume,
        gdp_flow,
        specialization: mean_specialization(world),
        dominant_medium,
        commons_health: commons_health(world),
        compliance_rate: compliance_rate(world),
        state_capacity: world.state_capacity(),
        legitimacy: world.legitimacy(),
        corruption: world.corruption(),
        public_pool: world.public_pool,
        temperature: world.temperature(),
        greenhouse_stock: world.greenhouse_stock(),
        emissions: world.emissions_flow(),
        climate_sensitivity: world.climate_sensitivity(),
    }
}

/// A tiny **experiment helper**: run the *same seed* forward `ticks` ticks under a
/// stack of rules and return the final measurements. Pairing two calls (one with
/// a rule, one without) is the canonical Phase-3 A/B test — same biology and
/// geography, only the institution differs, so any difference in the measured
/// outcome is attributable to the rule (Ostrom's comparative method).
pub fn run_under(
    primitives: super::world::Primitives,
    rules: &[Box<dyn super::institutions::Rule>],
    ticks: usize,
) -> Measurements {
    let mut w = World::new(primitives);
    for _ in 0..ticks {
        w.step_with_rules(rules);
    }
    measure(&w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gini_of_equal_distribution_is_zero() {
        assert!(gini(&[5.0, 5.0, 5.0, 5.0]).abs() < 1e-9);
    }

    #[test]
    fn gini_of_maximal_inequality_approaches_one() {
        let mut v = vec![0.0; 999];
        v.push(1000.0);
        let g = gini(&v);
        assert!(g > 0.99, "near-maximal inequality should give Gini≈1, got {g}");
    }

    #[test]
    fn gini_known_value() {
        assert!((gini(&[1.0, 2.0, 3.0, 4.0]) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn gini_degenerate_inputs() {
        assert_eq!(gini(&[]), 0.0); // empty
        assert_eq!(gini(&[0.0, 0.0, 0.0]), 0.0); // all-zero (sum<=0 guard)
        assert_eq!(gini(&[5.0]), 0.0); // single value
    }

    #[test]
    fn weighted_median_picks_the_high_volume_ratio() {
        // Two cheap tiny trades, one big trade at 5.0 → median follows volume.
        let v = weighted_median(vec![(1.0, 0.1), (2.0, 0.1), (5.0, 10.0)]);
        assert!((v - 5.0).abs() < 1e-9, "got {v}");
    }
}
