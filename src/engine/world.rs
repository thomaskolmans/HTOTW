//! The agent-based world: a resource **substrate** and a population of **agents**
//! that metabolize, move, harvest, **trade**, reproduce and die.
//!
//! This is the first-principles core (the Sugarscape lineage — Epstein & Axtell,
//! *Growing Artificial Societies*, 1996). **Nothing socioeconomic is an input.**
//! The only inputs are physical/biological/behavioural primitives ([`Primitives`]):
//! a resource landscape, ecological regrowth rate, metabolic costs, perception
//! range, reproduction/mortality biology, and a bargaining rule. Everything a
//! policy analyst cares about — population, the wealth distribution and its
//! **Gini**, life expectancy, carrying capacity, **prices, money, GDP,
//! specialization** — is **measured** from the agent state by the
//! [`crate::engine::instruments`], never assigned.
//!
//! ## Phase 2 — exchange (this layer)
//!
//! There are now **two renewable goods** on the landscape. Each agent holds a
//! small *bundle* of the two goods and a survival `energy` reserve replenished by
//! **consuming** goods (so metabolism still bites and the Phase-1 carrying
//! capacity / inequality emergence still holds). Because the two resource fields
//! peak in different places (geographic **comparative advantage** — Ricardo) and
//! agents harvest with heterogeneous efficiency, agents end up holding lopsided
//! bundles. Adjacent agents then **trade bilaterally**: the trade ratio (a
//! **price**) is the geometric mean of the two parties' marginal rates of
//! substitution (the Sugarscape trade rule), and a trade only happens if it
//! **strictly raises both** agents' satisfaction (Gode & Sunder's
//! zero-intelligence budget-constrained traders converge to the competitive
//! price without any agent knowing it — Hayek: the price *is* the information).
//! Prices, trade volume, money (Menger) and GDP are then **measured**, never set.
//!
//! ## The litmus test
//!
//! `grep` this module for a socioeconomic average (a Gini, a price, a GDP) used
//! as an *input*. There are none. Only biology, geography and a bargaining rule
//! go in; the economy comes out.

use super::rng::Rng;

/// Number of distinct tradeable goods. Two is the smallest set that makes a
/// relative **price** (a ratio) and gains from trade meaningful.
pub const NGOODS: usize = 2;

/// Stefan–Boltzmann constant `σ` (W·m⁻²·K⁻⁴) for the energy-balance model.
pub const STEFAN_BOLTZMANN: f64 = 5.670_374_419e-8;

/// Reference **pre-industrial greenhouse stock** `C₀` (model units). The Myhre
/// log-forcing law is scale-invariant in `C/C₀`, so the absolute unit is free; we
/// fix it so emergent emissions move `C_atm` by a meaningful fraction of `C₀`.
pub const PREINDUSTRIAL_C: f64 = 280.0;

/// Radiative-equilibrium surface temperature (K) of the zero-dimensional
/// energy-balance model with **zero** CO₂ forcing: solving
/// `(1−albedo)·S/4 = ε·σ·T⁴` for `T` (Budyko 1969; Sellers 1969). Used to anchor
/// the pre-industrial steady state and the peak of the productivity response, so
/// the climate subsystem is a genuine no-op until emissions push `C_atm` above
/// `C₀`.
pub fn equilibrium_temperature(albedo: f64, solar_const: f64, emissivity: f64) -> f64 {
    let absorbed = (1.0 - albedo) * solar_const / 4.0;
    (absorbed / (emissivity * STEFAN_BOLTZMANN)).powf(0.25)
}

/// The physical/biological inputs — the "laws and initial conditions" of a run.
/// Every field is a genuine primitive (geography, ecology, metabolism,
/// bargaining), not a social outcome. To "simulate **to**" a real statistic you
/// calibrate *these* until the measured output matches (see `docs/ENGINE.md`),
/// never the output.
#[derive(Debug, Clone)]
pub struct Primitives {
    pub width: usize,
    pub height: usize,
    /// Logistic intrinsic regrowth rate `r` of the renewable resources.
    pub regrowth_rate: f64,
    /// Peak resource capacity `K` of the richest cells (the landscape scale).
    pub peak_capacity: f64,
    /// Number of agents to seed.
    pub n_agents: usize,
    /// Starting energy reserve each agent is seeded with (an *equal* start, so
    /// any inequality that appears is emergent, not built in).
    pub init_energy: f64,
    /// Per-tick metabolic cost range `[min, max]` (heterogeneity is a root fact:
    /// different bodies cost different amounts to run — Kleiber's law). This cost
    /// is paid out of the energy reserve.
    pub metabolism_min: f64,
    pub metabolism_max: f64,
    /// Perception/movement range `[min, max]` in cells (bounded rationality).
    pub vision_min: u32,
    pub vision_max: u32,
    /// Energy reserve above which an agent may reproduce.
    pub birth_threshold: f64,
    /// Fraction of its energy a parent endows to a newborn.
    pub child_endowment_frac: f64,
    /// Hard maximum age (ticks); combined with a rising senescence hazard.
    pub max_age: u32,
    /// Gompertz–Makeham senescence coefficient (per-tick hazard ~ this·e^(α·age)).
    pub senescence: f64,
    /// Trait mutation scale passed to offspring.
    pub mutation: f64,
    /// **Satiation scale** of the need-satisfaction curve `u(s)=s/(s+scale)`.
    /// Diminishing marginal utility is *derived* from this single biological
    /// need parameter — there is no fitted social utility curve.
    pub satiation_scale: f64,
    /// Energy released per unit of good consumed each tick to pay metabolism.
    /// (Goods are the food; energy is the burned reserve.)
    pub energy_per_good: f64,
    /// Whether the bilateral-**exchange** phase runs. Turning it OFF gives an
    /// autarky control run for the gains-from-trade test.
    pub trade_enabled: bool,
    /// **Ecological fragility** (Phase 3). If a cell's *standing* stock of a good
    /// is harvested below `regen_threshold·K`, the resource has been mined past
    /// its regeneration point and the cell's **capacity** itself degrades by
    /// `degrade_rate` (desertification / fishery collapse). This is the physical
    /// reason a commons can be destroyed — and the reason a rule that leaves
    /// enough standing stock can sustain it (Hardin; Ostrom). It is a pure
    /// ecological primitive: no social outcome is set, only the soil's physics.
    pub regen_threshold: f64,
    /// Per-event fractional capacity loss when a cell is mined below threshold.
    pub degrade_rate: f64,
    /// Per-tick fraction of lost capacity that **heals** back toward its pristine
    /// value when a cell is not being over-mined (soil/fishery recovery). Makes
    /// degradation real but reversible, so open access *stresses* the commons
    /// without trivially exterminating every run — and stewardship is rewarded.
    pub recovery_rate: f64,

    // ---- Phase 5: spatial energy-balance CLIMATE coupled to production. ----
    // Climate is OPT-IN. When `climate_enabled` is false the entire subsystem is
    // skipped, so the default `demo()` history is byte-identical to before. When
    // true, emissions emerge from production, accumulate as a greenhouse stock,
    // temperature follows a zero-dimensional energy balance, and warming feeds
    // back MECHANISTICALLY on regrowth (lower net primary productivity → lower
    // carrying capacity → fewer people / less wealth). No damage coefficient is
    // applied to any macro output; the damage is the ecological consequence.
    /// Master switch for the climate coupling (default OFF → no-op).
    pub climate_enabled: bool,
    /// **Emission factor**: greenhouse units released per unit of good harvested
    /// (production *is* the emission source — combustion/land-use proportional to
    /// throughput). 0 ⇒ a clean economy with no climate forcing.
    pub emission_factor: f64,
    /// **Pre-industrial greenhouse stock** `C₀` — the reference concentration in
    /// the Myhre log-forcing law `F = λ·ln(C/C₀)` (so `C = C₀` ⇒ zero forcing).
    pub c_preindustrial: f64,
    /// Initial greenhouse stock `C_atm(0)`. Defaulting it to `c_preindustrial`
    /// starts the planet at radiative balance (a no-op pre-industrial steady
    /// state), so switching climate on without emissions leaves temperature flat.
    pub c_atm0: f64,
    /// **First-order decay** of the greenhouse stock per tick (ocean/biosphere
    /// uptake): `dC = emissions − co2_decay·(C − C₀)`. Relaxes C back toward `C₀`.
    pub co2_decay: f64,
    /// **Radiative-forcing sensitivity** `λ` (W·m⁻²) in the Myhre 1998 log law.
    pub forcing_lambda: f64,
    /// Planetary effective **heat capacity** (thermal inertia): the larger it is,
    /// the more slowly temperature tracks its radiative-equilibrium value.
    pub heat_capacity: f64,
    /// **Albedo** (reflected fraction of incoming solar) — Budyko/Sellers.
    pub albedo: f64,
    /// **Solar constant** `S` (incoming flux); absorbed flux is `(1−albedo)·S/4`.
    pub solar_const: f64,
    /// Surface **emissivity** `ε` for the Stefan–Boltzmann outgoing term `ε·σ·T⁴`.
    pub emissivity: f64,
    /// **Optimal temperature** (K) for net primary productivity — the peak of the
    /// unimodal `temp_response(T)`. Set to the pre-industrial equilibrium so that
    /// at `C = C₀` the regrowth multiplier is exactly 1 (no-op), and warming above
    /// it lowers productivity (Verhulst regrowth scaled by Lindeman-style NPP).
    pub temp_opt: f64,
    /// **Thermal tolerance width** (K) of the productivity response (Gaussian σ).
    pub temp_tolerance: f64,

    /// RNG seed.
    pub seed: u64,
}

impl Primitives {
    /// A reasonable default landscape for experiments (two resource "mountains",
    /// the classic Sugarscape geography — but now each mountain is a *different*
    /// good, giving regional comparative advantage).
    pub fn demo() -> Primitives {
        Primitives {
            width: 50,
            height: 50,
            regrowth_rate: 0.4,
            peak_capacity: 6.0,
            n_agents: 400,
            init_energy: 20.0,
            metabolism_min: 0.5,
            metabolism_max: 2.0,
            vision_min: 1,
            vision_max: 6,
            birth_threshold: 25.0,
            child_endowment_frac: 0.4,
            max_age: 100,
            senescence: 0.0004,
            mutation: 0.1,
            satiation_scale: 4.0,
            energy_per_good: 1.0,
            trade_enabled: true,
            // Ecological fragility is OFF in the default landscape: the Phase-1/2
            // substrate physics (a robust, self-reseeding commons) is preserved
            // and all their emergence tests stand unchanged. Fragility is a
            // Phase-3 primitive an experimenter switches on (see
            // `Primitives::fragile_commons`) to study the tragedy of the commons:
            // only a destructible resource *can* be over-exploited. The threshold
            // is kept defined so `fragile_commons` only needs to flip the rates.
            regen_threshold: 0.15,
            degrade_rate: 0.0,
            recovery_rate: 0.0,
            // Climate OFF by default: the whole subsystem is skipped (so every
            // existing test sees byte-identical behaviour). The parameters below
            // still describe a self-consistent PRE-INDUSTRIAL STEADY STATE — the
            // planet sits at radiative equilibrium and `temp_opt` is exactly that
            // equilibrium temperature — so even forcing `climate_enabled = true`
            // with no emissions leaves temperature and regrowth untouched.
            climate_enabled: false,
            emission_factor: 0.0,
            c_preindustrial: PREINDUSTRIAL_C,
            c_atm0: PREINDUSTRIAL_C,
            co2_decay: 0.02,
            forcing_lambda: 5.35, // Myhre et al. 1998 best-fit CO₂ coefficient
            heat_capacity: 30.0,  // planetary thermal inertia (relaxation time)
            albedo: 0.30,
            solar_const: 1361.0, // W·m⁻² (modern solar constant)
            emissivity: 0.62,    // effective emissivity giving ~288 K equilibrium
            temp_opt: equilibrium_temperature(0.30, 1361.0, 0.62),
            temp_tolerance: 6.0, // K — productivity falls off over a few degrees
            seed: 1,
        }
    }

    /// A **fragile-commons** landscape (Phase 3): the demo world with ecological
    /// fragility switched on, so a cell mined below its regeneration threshold
    /// loses capacity (and heals back only slowly). This is the substrate on
    /// which the **tragedy of the commons** appears under open access and is
    /// resolved by a quota or property regime. Only the ecological primitives
    /// differ from [`Primitives::demo`]; nothing socioeconomic is set.
    pub fn fragile_commons() -> Primitives {
        Primitives {
            degrade_rate: 0.08,
            recovery_rate: 0.02,
            ..Primitives::demo()
        }
    }

    /// A **warming-world** landscape (Phase 5): the demo world with the spatial
    /// energy-balance climate coupling switched ON. Production now emits a
    /// greenhouse gas in proportion to throughput (`emission_factor > 0`); the
    /// stock `C_atm` accumulates with first-order decay; temperature follows the
    /// Budyko–Sellers energy balance driven by Myhre log forcing; and warming
    /// above the productivity optimum mechanistically depresses logistic regrowth
    /// (lower net primary productivity → lower carrying capacity). Nothing
    /// socioeconomic or climatic-as-outcome is set — only the physics is turned
    /// on; the temperature, greenhouse stock and the *climate damage* (the
    /// population/biomass shortfall) all EMERGE. Only the climate primitives
    /// differ from [`Primitives::demo`].
    pub fn warming_world() -> Primitives {
        Primitives {
            climate_enabled: true,
            // Each unit harvested releases a small slug of greenhouse gas. Kept
            // modest so the stock climbs over a multi-century run rather than
            // spiking — emissions are a *flow* proportional to real activity.
            emission_factor: 0.02,
            // Slow uptake so the stock genuinely accumulates from emissions.
            co2_decay: 0.01,
            ..Primitives::demo()
        }
    }

    /// A **continental-scale** landscape (Phase 8 — scale): the same demo physics
    /// on a much larger grid seeded with a large population, sized so the
    /// data-parallel substrate phase (regrowth) crosses the threading threshold.
    /// `cells` is the target number of cells (the grid is the nearest square),
    /// and `n_agents` the seed population — pass `100_000` (or more) to exercise
    /// the scale path. Only geography size and population differ from
    /// [`Primitives::demo`]; no socioeconomic outcome is set, so the same
    /// emergent properties hold, just over a bigger world.
    pub fn large_world(cells: usize, n_agents: usize) -> Primitives {
        let side = (cells as f64).sqrt().ceil() as usize;
        let side = side.max(2);
        Primitives {
            width: side,
            height: side,
            n_agents,
            ..Primitives::demo()
        }
    }
}

/// The renewable resource landscape. Each cell holds a stock of **each good**
/// that regrows logistically toward its per-good capacity.
#[derive(Debug, Clone)]
pub struct Substrate {
    pub width: usize,
    pub height: usize,
    /// Per-cell per-good carrying capacity `K` (the geography). **Mutable**: it
    /// degrades when a cell is mined past its regeneration threshold (Phase 3),
    /// which is how a commons can be physically destroyed.
    pub capacity: Vec<[f64; NGOODS]>,
    /// Per-cell per-good *pristine* capacity (the geography as originally laid
    /// down). Never mutated after construction; the sustainability instrument
    /// reads `capacity / capacity0` to measure how much of the commons survives.
    pub capacity0: Vec<[f64; NGOODS]>,
    /// Per-cell per-good current resource stock `S` (state).
    pub resource: Vec<[f64; NGOODS]>,
    /// Index of the occupying agent, or `usize::MAX` for empty (one agent/cell).
    pub occupant: Vec<usize>,
    regrowth_rate: f64,
    recovery_rate: f64,
    /// **Climate productivity multiplier** applied to logistic regrowth this tick
    /// (Phase 5). 1.0 = no climate effect (the default and the pre-industrial
    /// state); warming above the optimum drives it below 1, lowering net primary
    /// productivity. Set by [`World`] from the temperature each tick; the regrowth
    /// loop reads it. This is the MECHANISTIC channel for emergent climate damage.
    temp_growth_factor: f64,
}

const EMPTY: usize = usize::MAX;

impl Substrate {
    #[inline]
    pub fn idx(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }
    #[inline]
    pub fn xy(&self, i: usize) -> (usize, usize) {
        (i % self.width, i / self.width)
    }
    pub fn cells(&self) -> usize {
        self.width * self.height
    }

    /// Logistic regrowth (per good) with a small seed-bank term so fully
    /// stripped cells can still recover (real ecosystems reseed from
    /// neighbours/dormancy): `S += r·max(S, 0.15K)·(1 − S/K)`, capped at `K`.
    ///
    /// **This phase is data-parallel (Phase 8).** Each cell's regrowth depends
    /// only on *its own* prior `resource`/`capacity` and its pristine
    /// `capacity0`, with no RNG and no cross-cell coupling — so it is partitioned
    /// into contiguous, disjoint index ranges and run across worker threads. The
    /// arithmetic per cell is identical to the sequential loop, so the result is
    /// **bit-identical** for any thread count (the engine pins this with a
    /// `parallel == sequential` test). The `capacity`/`capacity0` arrays are
    /// addressed by absolute index inside each chunk; they are read-only
    /// (`capacity0`) or written only at the chunk's own indices (`capacity`).
    fn regrow(&mut self) {
        // Snapshot the scalar laws so the closure captures plain `f64`s (Sync),
        // not a borrow of `self`, while the slices are split disjointly.
        let recovery_rate = self.recovery_rate;
        let regrowth_rate = self.regrowth_rate;
        let temp_growth_factor = self.temp_growth_factor;

        // `capacity` is mutated at this chunk's own indices and `capacity0` is
        // read-only; we expose both to the per-chunk closure as raw pointers
        // bounded to the chunk's absolute indices. Safety: each chunk owns a
        // disjoint, contiguous index range (guaranteed by `for_each_chunk_mut`),
        // so the `capacity` writes never alias across threads, and `capacity0`
        // is only ever read. `resource` is the partitioned slice itself.
        let cap_ptr = self.capacity.as_mut_ptr() as usize;
        let cap0_ptr = self.capacity0.as_ptr() as usize;

        super::parallel::for_each_chunk_mut(&mut self.resource, move |start, chunk| {
            let cap = cap_ptr as *mut [f64; NGOODS];
            let cap0 = cap0_ptr as *const [f64; NGOODS];
            for (local, res) in chunk.iter_mut().enumerate() {
                let i = start + local;
                // SAFETY: `i` is within the chunk's disjoint index range, so this
                // thread is the sole writer of `capacity[i]`; `capacity0[i]` is
                // read-only. Both arrays are the same length as `resource`.
                let capacity_i = unsafe { &mut *cap.add(i) };
                let capacity0_i = unsafe { &*cap0.add(i) };
                for g in 0..NGOODS {
                    // Capacity recovery: degraded land heals slowly back toward
                    // its pristine value (soil/fishery recovery) — degradation is
                    // real but reversible, so stewardship pays and open access
                    // merely *stresses* the commons rather than exterminating it.
                    let k0 = capacity0_i[g];
                    if recovery_rate > 0.0 && capacity_i[g] < k0 {
                        capacity_i[g] =
                            (capacity_i[g] + recovery_rate * (k0 - capacity_i[g])).min(k0);
                    }
                    let k = capacity_i[g];
                    if k <= 0.0 {
                        continue;
                    }
                    let s = res[g].max(0.15 * k);
                    // Logistic regrowth (Verhulst) with the climate productivity
                    // multiplier scaling the intrinsic rate — warming below the
                    // optimum slows net primary productivity (Lindeman
                    // energetics), shrinking the realised carrying capacity.
                    let r = regrowth_rate * temp_growth_factor;
                    let next = res[g] + r * s * (1.0 - res[g] / k);
                    res[g] = next.clamp(0.0, k);
                }
            }
        });
    }
}

/// The agent population in struct-of-arrays layout (cache-friendly hot loops).
/// `alive[i] == false` marks a tombstone; slots are not reused within a run.
#[derive(Debug, Clone, Default)]
pub struct Agents {
    pub alive: Vec<bool>,
    pub age: Vec<u32>,
    pub cell: Vec<usize>,
    /// Survival energy reserve. Metabolism is paid from this; it is refilled by
    /// **consuming** held goods. This is the Phase-1 "wealth" the carrying
    /// capacity / mortality dynamics act on.
    pub energy: Vec<f64>,
    /// Per-good holdings (the tradeable **bundle**). The wealth instrument values
    /// these at emergent prices; the Gini measures the spread of total wealth.
    pub good: Vec<[f64; NGOODS]>,
    pub metabolism: Vec<f64>,
    pub vision: Vec<u32>,
    /// Per-good harvest efficiency multiplier (root productivity heterogeneity →
    /// **comparative advantage**, Ricardo). Inherited with mutation.
    pub skill: Vec<[f64; NGOODS]>,
    /// Cumulative units harvested per good (for the specialization instrument).
    pub harvested: Vec<[f64; NGOODS]>,
}

impl Agents {
    pub fn len(&self) -> usize {
        self.alive.len()
    }
    pub fn is_empty(&self) -> bool {
        self.alive.is_empty()
    }
    pub fn alive_count(&self) -> usize {
        self.alive.iter().filter(|&&a| a).count()
    }
    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        cell: usize,
        energy: f64,
        good: [f64; NGOODS],
        metabolism: f64,
        vision: u32,
        skill: [f64; NGOODS],
    ) -> usize {
        let id = self.alive.len();
        self.alive.push(true);
        self.age.push(0);
        self.cell.push(cell);
        self.energy.push(energy);
        self.good.push(good);
        self.metabolism.push(metabolism);
        self.vision.push(vision);
        self.skill.push(skill);
        self.harvested.push([0.0; NGOODS]);
        id
    }
}

/// A single realised bilateral trade, recorded in the per-tick ledger. The
/// `price` is the realised exchange ratio (units of good 1 per unit of good 0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trade {
    pub seller_good0: usize,
    pub buyer_good0: usize,
    /// Quantity of good 0 that moved.
    pub qty_good0: f64,
    /// Quantity of good 1 that moved (the other way).
    pub qty_good1: f64,
    /// Realised price = units of good 1 per unit of good 0.
    pub price: f64,
}

/// The complete simulated world: substrate + agents + RNG + ledgers.
#[derive(Debug, Clone)]
pub struct World {
    pub tick: u64,
    pub substrate: Substrate,
    pub agents: Agents,
    pub rng: Rng,
    params: Primitives,
    // Death ledger for the (emergent) life-expectancy instrument.
    pub death_age_sum: f64,
    pub death_count: u64,
    /// Goods harvested this tick, per good (an emergent "production" flow).
    pub harvested_this_tick: [f64; NGOODS],
    /// Realised trades this tick — the price/volume ledger (cleared each step).
    pub trades_this_tick: Vec<Trade>,
    /// Per-good count of times that good was *given up* (offered as payment) in a
    /// completed trade. The good most often used to pay tends to become the
    /// medium of exchange (Menger / Kiyotaki–Wright money emergence).
    pub medium_accept: [u64; NGOODS],

    // ---- Phase 3: institutions (mechanism state set by RULES, never outcomes) ----
    /// **Property rights** on cells (Demsetz). `cell_owner[c]` is the agent that
    /// owns cell `c`, or `EMPTY` for open access (no owner). Owners harvest with
    /// restraint on their own land; this is the mechanism a property rule molds.
    pub cell_owner: Vec<usize>,
    /// Active **harvest quota**: the maximum fraction of a cell's *standing*
    /// stock an agent may take this tick (a conservation mechanism). `None` =
    /// unrestricted (open access — strip the cell). Set by a rule each tick.
    pub harvest_quota: Option<f64>,
    /// **Public pool**: a stock of value (good-1 numéraire) accumulated by taxes
    /// and spent on redistribution / enforcement / public goods. Emergent in
    /// size; it is a physical reservoir, not a macro target.
    pub public_pool: f64,
    /// Productivity multiplier delivered by **public-good provision** funded from
    /// the pool (e.g. irrigation/roads raising regrowth). Read by regrowth/act.
    pub public_good_level: f64,

    // ---- Phase 3: emergent governance ledgers (MEASURED by instruments) ----
    /// Enforcement actions a rule *intended* to carry out this run (cumulative).
    pub enforce_intended: u64,
    /// Enforcement actions actually *achieved* (funded & not failed) — the ratio
    /// achieved/intended is the emergent **state capacity** (Tilly).
    pub enforce_achieved: u64,
    /// Value diverted from the public pool by corrupt officials (cumulative).
    /// corruption = diverted / (diverted + delivered) — emergent, never set.
    pub pool_diverted: f64,
    /// Value actually delivered from the pool to its intended public use.
    pub pool_delivered: f64,
    /// Times an agent *voluntarily complied* with the active rule (cumulative).
    pub compliance_events: u64,
    /// Times an agent *defied* the rule (evaded the quota / tax). The voluntary
    /// compliance share is the emergent **legitimacy / tax morale** (Levi).
    pub defiance_events: u64,
    /// **Legitimacy belief** in `[0,1]`, a slow EMA the population holds about
    /// the institution (Levi's tax morale; Axelrod/Ostrom reciprocity). It is
    /// *reinforcing*: it rises when this tick's realised compliance beats the
    /// neutral 50% norm and the pool is delivered cleanly, and falls when
    /// defiance dominates or corruption diverts the pool. That makes 0.5 an
    /// **unstable** fixed point — a quota that tips early compliance above half
    /// climbs into a high-trust regime; a looted institution slides into a
    /// low-trust one. Emergent, never set.
    legit_level: f64,
    /// Legitimacy as perceived by agents *this* tick — a snapshot of `legit_level`
    /// taken at tick start. Snapshotting (rather than recomputing mid-tick) stops
    /// a within-tick cascade where one agent's defiance instantly drives everyone
    /// else to defy; agents respond to the institution's *track record*.
    legit_snapshot: f64,
    // Per-tick compliance/defiance tallies, reset each tick, that drive the EMA.
    tick_complied: u64,
    tick_defied: u64,

    // ---- Phase 5: climate state (PHYSICS state, read back by instruments). ----
    /// **Atmospheric greenhouse stock** `C_atm` (model units): fed by emissions
    /// from production, removed by first-order decay toward `C_preindustrial`.
    /// Left on the world so the next phase (collective choice) can let agents
    /// perceive and respond to it. Emergent in size; never set as an outcome.
    pub c_atm: f64,
    /// **Surface temperature** `T` (K) from the zero-dimensional energy balance.
    /// Relaxes toward radiative equilibrium with the planet's thermal inertia.
    pub temperature: f64,
    /// Greenhouse emissions produced **this tick** (a flow), summed over all
    /// agents' harvesting. Cleared and recomputed each step; measured, not set.
    pub emissions_this_tick: f64,
    /// **Emission-intensity multiplier** a decarbonising institution may set for
    /// this tick (Phase 5, a mechanism — like a clean-tech mandate that lowers the
    /// carbon per unit of output). 1.0 = unmitigated (the default); a rule can
    /// push it toward 0 to abate emissions. Reset to 1.0 each tick before rules
    /// run, so it is purely a per-tick lever, never a stored outcome. The
    /// resulting temperature path is still MEASURED, not set.
    pub emission_scale: f64,
}

impl World {
    /// Build a world from primitives. Agents start at random cells with EQUAL
    /// energy, empty bundles and heterogeneous metabolism/vision/skill — so any
    /// later inequality, price or specialization is emergent.
    pub fn new(params: Primitives) -> World {
        let mut rng = Rng::seed(params.seed);
        let (w, h) = (params.width, params.height);
        let n = w * h;

        // Landscape: two Gaussian "mountains", but each peak is a *different*
        // good. Cells near peak 0 are rich in good 0, cells near peak 1 in good
        // 1 → regional comparative advantage (Ricardo) that drives trade.
        let mut capacity = vec![[0.0; NGOODS]; n];
        let peaks = [
            (w as f64 * 0.3, h as f64 * 0.3),
            (w as f64 * 0.7, h as f64 * 0.7),
        ];
        let sigma = (w.min(h) as f64) * 0.18;
        for y in 0..h {
            for x in 0..w {
                for (g, &(px, py)) in peaks.iter().enumerate() {
                    let d2 = (x as f64 - px).powi(2) + (y as f64 - py).powi(2);
                    let v = (-d2 / (2.0 * sigma * sigma)).exp();
                    capacity[y * w + x][g] = params.peak_capacity * v.min(1.0);
                }
            }
        }
        let resource = capacity.clone(); // start full
        let capacity0 = capacity.clone(); // pristine geography (never mutated)

        let mut substrate = Substrate {
            width: w,
            height: h,
            capacity,
            capacity0,
            resource,
            occupant: vec![EMPTY; n],
            regrowth_rate: params.regrowth_rate,
            recovery_rate: params.recovery_rate,
            temp_growth_factor: 1.0,
        };

        // Seed agents at distinct random cells.
        let mut agents = Agents::default();
        let mut placed = 0;
        let mut guard = 0;
        while placed < params.n_agents && guard < params.n_agents * 50 {
            guard += 1;
            let c = rng.below(n);
            if substrate.occupant[c] != EMPTY {
                continue;
            }
            let met = rng.range(params.metabolism_min, params.metabolism_max);
            let vis = (rng.below((params.vision_max - params.vision_min + 1) as usize) as u32)
                + params.vision_min;
            // Heterogeneous skill: each agent is innately better at one good
            // (around 1.0±), the seed of comparative advantage.
            let s0 = rng.range(0.5, 1.5);
            let s1 = rng.range(0.5, 1.5);
            let id = agents.push(
                c,
                params.init_energy,
                [0.0; NGOODS],
                met,
                vis,
                [s0, s1],
            );
            substrate.occupant[c] = id;
            placed += 1;
        }

        // Pre-industrial climate steady state (computed before `params` moves in).
        let c_atm0 = params.c_atm0;
        let t_eq =
            equilibrium_temperature(params.albedo, params.solar_const, params.emissivity);

        World {
            tick: 0,
            substrate,
            agents,
            rng,
            params,
            death_age_sum: 0.0,
            death_count: 0,
            harvested_this_tick: [0.0; NGOODS],
            trades_this_tick: Vec::new(),
            medium_accept: [0; NGOODS],
            cell_owner: vec![EMPTY; n],
            harvest_quota: None,
            public_pool: 0.0,
            public_good_level: 0.0,
            enforce_intended: 0,
            enforce_achieved: 0,
            pool_diverted: 0.0,
            pool_delivered: 0.0,
            compliance_events: 0,
            defiance_events: 0,
            legit_level: 0.5,
            legit_snapshot: 0.5,
            tick_complied: 0,
            tick_defied: 0,
            // Start the climate at its pre-industrial steady state: the stock at
            // C₀ and the temperature at the radiative equilibrium of the
            // energy-balance model (zero CO₂ forcing). With climate disabled
            // these are inert; with it enabled but no emissions they stay put.
            c_atm: c_atm0,
            temperature: t_eq,
            emissions_this_tick: 0.0,
            emission_scale: 1.0,
        }
    }

    pub fn params(&self) -> &Primitives {
        &self.params
    }

    /// Per-tick senescence hazard (Gompertz–Makeham): small constant + rising
    /// exponential with age. Returns a probability in `[0,1)`.
    fn senescence_hazard(&self, age: u32) -> f64 {
        let a = age as f64;
        (self.params.senescence * (0.05 * a).exp()).min(1.0)
    }

    /// Marginal utility of one more unit of good `g` for agent `i`, derived from
    /// the need-satisfaction curve `u(s)=s/(s+scale)` ⇒ `u'(s)=scale/(s+scale)²`.
    /// Diminishing returns are a *consequence* of satiation, not a fitted curve.
    #[inline]
    fn marginal_utility(&self, stock: f64, _g: usize) -> f64 {
        let scale = self.params.satiation_scale;
        scale / (stock + scale).powi(2)
    }

    /// Total satisfaction (need utility) of agent `i`'s current bundle — used
    /// only to verify that a candidate trade is strictly Pareto-improving.
    #[inline]
    fn satisfaction(&self, bundle: &[f64; NGOODS]) -> f64 {
        let scale = self.params.satiation_scale;
        let mut u = 0.0;
        for &s in bundle {
            u += s / (s + scale);
        }
        u
    }

    /// Marginal rate of substitution: units of good 1 the agent would give for
    /// one more unit of good 0 = MU₀ / MU₁ (its private valuation / price).
    #[inline]
    fn mrs(&self, bundle: &[f64; NGOODS]) -> f64 {
        let mu0 = self.marginal_utility(bundle[0], 0);
        let mu1 = self.marginal_utility(bundle[1], 1);
        mu0 / mu1
    }

    /// Advance one year with **no institutions** (the Phase-1/2 behaviour).
    /// Equivalent to `step_with_rules(&[])`.
    pub fn step(&mut self) {
        self.step_with_rules(&[]);
    }

    /// Advance one year under a stack of composable [`crate::engine::institutions::Rule`]s. Phases (the "laws
    /// of physics", order-stable): **institutional enforcement** (rules mold this
    /// tick's mechanisms / tax / redistribute) → substrate regrowth →
    /// (shuffled) move/harvest → **exchange** → consume/metabolize → vital events
    /// (death, birth). Rules run first so production and exchange respond to the
    /// incentives they set; they may only touch mechanisms and the public pool,
    /// never macro outcomes (those stay measured by the instruments).
    pub fn step_with_rules(&mut self, rules: &[Box<dyn crate::engine::institutions::Rule>]) {
        self.harvested_this_tick = [0.0; NGOODS];
        self.emissions_this_tick = 0.0;
        // Reset the per-tick emission-intensity lever before rules run; a
        // decarbonising rule may lower it this tick, abating emergent emissions.
        self.emission_scale = 1.0;
        self.trades_this_tick.clear();
        // Freeze this tick's perceived legitimacy and reset the per-tick tallies
        // that will feed the legitimacy EMA. Agents respond to the institution's
        // track record (the snapshot), not to each other within the same tick.
        self.legit_snapshot = self.legit_level;
        self.tick_complied = 0;
        self.tick_defied = 0;

        // --- Institutional phase: apply each rule in declared order. The RNG is
        // moved out and back so rules share the single deterministic stream. ---
        if !rules.is_empty() {
            let mut rng = std::mem::replace(&mut self.rng, Rng::seed(0));
            for rule in rules {
                rule.enforce(self, &mut rng);
            }
            self.rng = rng;
        }

        // Climate → ecology feedback: warming above the productivity optimum
        // throttles this tick's net primary productivity (the regrowth rate).
        // Read from the *current* temperature (last tick's climate), so the
        // feedback is causal and order-stable. Disabled ⇒ factor stays 1.0.
        if self.params.climate_enabled {
            self.substrate.temp_growth_factor = self.temp_response(self.temperature);
        }

        self.substrate.regrow();

        // Deterministic but fair processing order.
        let mut order: Vec<usize> =
            (0..self.agents.len()).filter(|&i| self.agents.alive[i]).collect();
        self.rng.shuffle(&mut order);

        // --- Production phase: move and harvest both goods. ---
        for &i in &order {
            if !self.agents.alive[i] {
                continue;
            }
            self.act(i);
        }

        // --- Exchange phase: adjacent agents trade bilaterally (Sugarscape). ---
        if self.params.trade_enabled {
            self.exchange(&order);
        }

        // --- Climate phase: this tick's emergent emissions feed the greenhouse
        // stock; temperature relaxes toward radiative equilibrium under the new
        // forcing (Budyko–Sellers + Myhre). Updated *after* production so the
        // damage it causes is felt next tick (the regrowth throttle above). ---
        if self.params.climate_enabled {
            self.update_climate();
        }

        // --- Metabolism / vital events. ---
        for &i in &order {
            if !self.agents.alive[i] {
                continue;
            }
            // Consume goods to refuel the energy reserve, then pay metabolism.
            self.consume(i);
            self.agents.energy[i] -= self.agents.metabolism[i];
            self.agents.age[i] += 1;

            let starved = self.agents.energy[i] <= 0.0;
            let aged = self.agents.age[i] >= self.params.max_age
                || self.rng.f64() < self.senescence_hazard(self.agents.age[i]);
            if starved || aged {
                self.kill(i);
                continue;
            }

            if self.agents.energy[i] >= self.params.birth_threshold {
                self.try_reproduce(i);
            }
        }

        // --- Legitimacy update: nudge the belief toward this tick's realised
        // compliance share, anchored by clean delivery of the public pool. The
        // realised share enters relative to the 0.5 norm, so a tick where most
        // agents cooperated *raises* legitimacy (reinforcing) and a tick of mass
        // evasion or heavy corruption *lowers* it (Levi; Axelrod reciprocity). ---
        let encounters = self.tick_complied + self.tick_defied;
        if encounters > 0 {
            const ALPHA: f64 = 0.1; // EMA inertia
            let share = self.tick_complied as f64 / encounters as f64;
            // Clean-delivery factor in [0,1]: 1 when nothing diverted.
            let outflow = self.pool_delivered + self.pool_diverted;
            let clean = if outflow > 0.0 {
                self.pool_delivered / outflow
            } else {
                1.0
            };
            // Target morale: realised compliance discounted by corruption.
            let target = (share * clean).clamp(0.0, 1.0);
            self.legit_level += ALPHA * (target - self.legit_level);
            self.legit_level = self.legit_level.clamp(0.0, 1.0);
        } else {
            // No rule encountered this tick: morale relaxes gently toward neutral.
            self.legit_level += 0.02 * (0.5 - self.legit_level);
        }

        self.tick += 1;
    }

    /// Sugarscape movement rule (generalised to two goods): among visible empty
    /// cells (four axes + current), move to the one with the most **total**
    /// resource (good0+good1), ties → nearest → lowest index, then harvest both
    /// goods, scaled by the agent's per-good skill.
    fn act(&mut self, i: usize) {
        let from = self.agents.cell[i];
        let (x, y) = self.substrate.xy(from);
        let vision = self.agents.vision[i] as usize;
        let (w, h) = (self.substrate.width, self.substrate.height);

        let total = |r: &[f64; NGOODS]| r[0] + r[1];
        let mut best_cell = from;
        let mut best_res = total(&self.substrate.resource[from]);
        let mut best_dist = 0usize;

        let dirs: [(isize, isize); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        for &(dx, dy) in &dirs {
            for d in 1..=vision {
                let nx = (x as isize + dx * d as isize).rem_euclid(w as isize) as usize;
                let ny = (y as isize + dy * d as isize).rem_euclid(h as isize) as usize;
                let c = self.substrate.idx(nx, ny);
                if self.substrate.occupant[c] != EMPTY && c != from {
                    continue;
                }
                let r = total(&self.substrate.resource[c]);
                let better = r > best_res
                    || (r == best_res && d < best_dist)
                    || (r == best_res && d == best_dist && c < best_cell);
                if better {
                    best_res = r;
                    best_cell = c;
                    best_dist = d;
                }
            }
        }

        if best_cell != from {
            self.substrate.occupant[from] = EMPTY;
            self.substrate.occupant[best_cell] = i;
            self.agents.cell[i] = best_cell;
        }
        self.harvest_cell(i, best_cell);
    }

    /// Harvest agent `i`'s current cell, subject to whatever **harvest mechanism**
    /// the active institution imposes (a conservation quota and/or property
    /// rights). Open access (no quota, no owner) strips the cell to zero — the
    /// individually-rational move that, summed over agents, mines the commons
    /// below its regeneration threshold and degrades the land (Hardin). A quota
    /// caps the *fraction* taken so standing stock stays above threshold — but
    /// compliance is **voluntary and imperfect**: an agent obeys with a
    /// probability that rises with the institution's emergent **legitimacy**
    /// (delivery / tax morale, Levi) and is otherwise (probabilistically) caught
    /// and forced to comply only as far as the **state capacity** reaches. None
    /// of compliance, capacity or legitimacy is set; they are read back off the
    /// ledgers the rules and agents write.
    fn harvest_cell(&mut self, i: usize, cell: usize) {
        // The fraction this agent is *allowed* to take under the current rule.
        // No quota → 1.0 (take everything). An owner farming its own land also
        // restrains itself (Demsetz: ownership internalises the future value).
        let owns = self.cell_owner[cell] == i;
        let owner_set = self.cell_owner[cell] != EMPTY;
        let allowed = match self.harvest_quota {
            Some(q) => q,
            None if owner_set => {
                // Property regime: an owner self-limits to a sustainable take;
                // a trespasser on owned land is also held to it by the owner.
                self.params.regen_threshold.max(0.0) + 0.5 * (1.0 - self.params.regen_threshold)
            }
            None => 1.0,
        };

        // Voluntary compliance: probability rises with measured legitimacy. With
        // no rule in force (allowed >= 1) there is nothing to comply with.
        let take_frac = if allowed >= 1.0 {
            1.0
        } else {
            // Conditional cooperation (Axelrod / Ostrom reciprocity): the
            // willingness to comply rises *faster* than legitimacy itself (a
            // concave response), so when a cooperative norm gains a foothold it
            // reinforces — 0.5 becomes an unstable tipping point between a
            // high-trust and a low-trust regime, rather than a flat random walk.
            let legit = self.legit_snapshot;
            let willingness = legit.sqrt();
            // An agent complies voluntarily with prob = willingness. Owners on
            // their own land always comply (their own future is at stake).
            let voluntary = owns || self.rng.f64() < willingness;
            if voluntary {
                self.compliance_events += 1;
                self.tick_complied += 1;
                allowed
            } else {
                // Defiance attempt. The state *intends* to catch every evader,
                // but only succeeds as far as funded enforcement (state
                // capacity) reaches — checked here against the pool.
                self.defiance_events += 1;
                self.tick_defied += 1;
                self.enforce_intended += 1;
                if self.try_enforce() {
                    self.enforce_achieved += 1;
                    allowed // caught → forced down to the quota
                } else {
                    1.0 // evaded → strips the cell anyway (the tragedy)
                }
            }
        };

        for g in 0..NGOODS {
            let standing = self.substrate.resource[cell][g];
            let removed = standing * take_frac;
            let take = removed * self.agents.skill[i][g];
            self.substrate.resource[cell][g] = standing - removed;
            self.agents.good[i][g] += take;
            self.agents.harvested[i][g] += take;
            self.harvested_this_tick[g] += take;
            // Emissions emerge from activity: production releases a greenhouse gas
            // in proportion to throughput (combustion / land-use change scale with
            // what is harvested). A clean economy (emission_factor = 0) emits none.
            if self.params.climate_enabled {
                self.emissions_this_tick +=
                    take * self.params.emission_factor * self.emission_scale;
            }

            // Ecology: if the *standing* stock was mined below its regeneration
            // threshold, the cell's capacity itself degrades (desertification).
            let k = self.substrate.capacity[cell][g];
            if k > 0.0 && self.substrate.resource[cell][g] < self.params.regen_threshold * k {
                self.substrate.capacity[cell][g] = k * (1.0 - self.params.degrade_rate);
                if self.substrate.resource[cell][g] > self.substrate.capacity[cell][g] {
                    self.substrate.resource[cell][g] = self.substrate.capacity[cell][g];
                }
            }
        }
    }

    /// Attempt one funded enforcement action. Enforcement is **costly** (Olson:
    /// collective action needs resources) and **imperfect**. It draws a fixed
    /// fee from the public pool; if the pool can't pay, the action fails (no
    /// capacity without funding). Corrupt diversion (set by a rule via
    /// `pool_diverted`) shrinks the effective pool, lowering capacity emergently.
    /// Returns whether the action succeeded.
    fn try_enforce(&mut self) -> bool {
        const FEE: f64 = 0.5;
        // Base technical failure rate even when funded (imperfect monitoring).
        const RELIABILITY: f64 = 0.85;
        if self.public_pool < FEE {
            return false;
        }
        self.public_pool -= FEE;
        self.pool_delivered += FEE;
        self.rng.f64() < RELIABILITY
    }

    /// Funded, imperfect enforcement attempt for use by [`crate::engine::institutions::Rule`]s (e.g. catching
    /// a tax evader). Same costly/imperfect mechanism as in-engine enforcement.
    pub(crate) fn try_enforce_public(&mut self) -> bool {
        self.try_enforce()
    }

    /// This tick's perceived legitimacy snapshot (for [`crate::engine::institutions::Rule`]s, e.g. tax morale).
    pub(crate) fn perceived_legitimacy(&self) -> f64 {
        self.legit_snapshot
    }

    /// Record a rule encounter (for [`crate::engine::institutions::Rule`]s that police compliance, e.g. tax):
    /// updates both the cumulative ledger and this tick's tallies that drive the
    /// legitimacy EMA.
    pub(crate) fn record_compliance(&mut self, complied: bool) {
        if complied {
            self.compliance_events += 1;
            self.tick_complied += 1;
        } else {
            self.defiance_events += 1;
            self.tick_defied += 1;
        }
    }

    /// Bilateral local exchange. Each agent (in the shuffled order) trades with
    /// the highest-resource adjacent neighbour. The price is the **geometric
    /// mean** of the two MRSs — the Sugarscape trade rule (Epstein & Axtell):
    /// it sits strictly inside the mutual-gain interval. We move one unit-step of
    /// the cheaper good and accept the trade only if it **strictly raises both**
    /// agents' satisfaction (Gode & Sunder budget-constrained ZI traders). With
    /// the price fixed and trade limited to mutually beneficial steps, repeated
    /// meetings drive MRSs toward a common ratio — the emergent market price.
    fn exchange(&mut self, order: &[usize]) {
        let (w, h) = (self.substrate.width, self.substrate.height);
        let dirs: [(isize, isize); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        for &i in order {
            if !self.agents.alive[i] {
                continue;
            }
            let (x, y) = self.substrate.xy(self.agents.cell[i]);
            // Find adjacent trading partner: alive neighbour in the lowest
            // cell-index direction (deterministic). Each ordered pass lets pairs
            // trade once per tick; repeated ticks/meetings converge the price.
            let mut partner = EMPTY;
            let mut partner_cell = usize::MAX;
            for &(dx, dy) in &dirs {
                let nx = (x as isize + dx).rem_euclid(w as isize) as usize;
                let ny = (y as isize + dy).rem_euclid(h as isize) as usize;
                let c = self.substrate.idx(nx, ny);
                let occ = self.substrate.occupant[c];
                if occ != EMPTY && self.agents.alive[occ] && c < partner_cell {
                    partner = occ;
                    partner_cell = c;
                }
            }
            if partner == EMPTY {
                continue;
            }
            self.try_trade(i, partner);
        }
    }

    /// Attempt a single mutually-beneficial unit trade between `a` and `b`.
    fn try_trade(&mut self, a: usize, b: usize) {
        let ba = self.agents.good[a];
        let bb = self.agents.good[b];
        let mrs_a = self.mrs(&ba); // a's value of good0 in units of good1
        let mrs_b = self.mrs(&bb);
        if (mrs_a - mrs_b).abs() < 1e-12 {
            return; // identical valuations → no gain
        }
        // The party with the *higher* MRS values good0 more → it buys good0 and
        // pays good1. Price = geometric mean of the two MRSs (Sugarscape).
        let price = (mrs_a * mrs_b).sqrt();
        let (buyer, seller) = if mrs_a > mrs_b { (a, b) } else { (b, a) };

        // Unit trade: buyer gets `q0` of good0, pays `price*q0` of good1.
        let q0 = 1.0_f64;
        let pay1 = price * q0;
        if self.agents.good[seller][0] < q0 || self.agents.good[buyer][1] < pay1 {
            return; // not enough to settle
        }

        // Candidate post-trade bundles.
        let mut nb_buyer = self.agents.good[buyer];
        let mut nb_seller = self.agents.good[seller];
        nb_buyer[0] += q0;
        nb_buyer[1] -= pay1;
        nb_seller[0] -= q0;
        nb_seller[1] += pay1;

        // Strict Pareto improvement in need-satisfaction, or no deal.
        let gain_buyer = self.satisfaction(&nb_buyer) - self.satisfaction(&self.agents.good[buyer]);
        let gain_seller =
            self.satisfaction(&nb_seller) - self.satisfaction(&self.agents.good[seller]);
        if gain_buyer <= 0.0 || gain_seller <= 0.0 {
            return;
        }

        self.agents.good[buyer] = nb_buyer;
        self.agents.good[seller] = nb_seller;

        // Record in the ledger. price = units of good1 per unit of good0.
        self.trades_this_tick.push(Trade {
            seller_good0: seller,
            buyer_good0: buyer,
            qty_good0: q0,
            qty_good1: pay1,
            price,
        });
        // Good1 was the means of payment here → it accrues "medium" acceptance.
        self.medium_accept[1] += 1;
    }

    /// Consume goods to refill the energy reserve enough to cover this tick's
    /// metabolism (plus a little buffer toward reproduction), consuming from the
    /// larger holding first (you eat what you have most of — satiation again).
    fn consume(&mut self, i: usize) {
        let need_units = (self.agents.metabolism[i] * 1.5) / self.params.energy_per_good.max(1e-9);
        let mut remaining = need_units;
        // Consume from whichever good the agent holds more of, alternating until
        // the need is met or holdings are exhausted.
        for _ in 0..(2 * NGOODS) {
            if remaining <= 1e-12 {
                break;
            }
            // pick richest good
            let mut g_best = 0;
            for g in 1..NGOODS {
                if self.agents.good[i][g] > self.agents.good[i][g_best] {
                    g_best = g;
                }
            }
            let avail = self.agents.good[i][g_best];
            if avail <= 1e-12 {
                break;
            }
            let take = avail.min(remaining);
            self.agents.good[i][g_best] -= take;
            self.agents.energy[i] += take * self.params.energy_per_good;
            remaining -= take;
        }
    }

    fn kill(&mut self, i: usize) {
        self.agents.alive[i] = false;
        self.substrate.occupant[self.agents.cell[i]] = EMPTY;
        self.death_age_sum += self.agents.age[i] as f64;
        self.death_count += 1;
    }

    fn try_reproduce(&mut self, parent: usize) {
        let from = self.agents.cell[parent];
        let (x, y) = self.substrate.xy(from);
        let (w, h) = (self.substrate.width, self.substrate.height);
        let dirs: [(isize, isize); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        for &(dx, dy) in &dirs {
            let nx = (x as isize + dx).rem_euclid(w as isize) as usize;
            let ny = (y as isize + dy).rem_euclid(h as isize) as usize;
            let c = self.substrate.idx(nx, ny);
            if self.substrate.occupant[c] == EMPTY {
                let endow = self.agents.energy[parent] * self.params.child_endowment_frac;
                self.agents.energy[parent] -= endow;
                // Endow the child with a share of each held good too.
                let mut child_goods = [0.0; NGOODS];
                let frac = self.params.child_endowment_frac;
                for (held, child_g) in
                    self.agents.good[parent].iter_mut().zip(child_goods.iter_mut())
                {
                    let give = *held * frac;
                    *held -= give;
                    *child_g = give;
                }
                let m = (self.agents.metabolism[parent]
                    + self.rng.range(-self.params.mutation, self.params.mutation))
                    .clamp(self.params.metabolism_min, self.params.metabolism_max);
                let vmin = self.params.vision_min as i64;
                let vmax = self.params.vision_max as i64;
                let dv = if self.rng.f64() < self.params.mutation { 1 } else { 0 };
                let sign: i64 = if self.rng.f64() < 0.5 { -1 } else { 1 };
                let v = (self.agents.vision[parent] as i64 + sign * dv).clamp(vmin, vmax) as u32;
                // Inherit skill with mutation (heritable comparative advantage).
                let mut skill = self.agents.skill[parent];
                for s in &mut skill {
                    *s = (*s + self.rng.range(-self.params.mutation, self.params.mutation))
                        .clamp(0.25, 2.0);
                }
                let child = self.agents.push(c, endow, child_goods, m, v, skill);
                self.substrate.occupant[c] = child;
                return;
            }
        }
    }

    /// Mean realised lifespan so far — the emergent life expectancy (returns
    /// `None` until at least one agent has died).
    pub fn life_expectancy(&self) -> Option<f64> {
        if self.death_count == 0 {
            None
        } else {
            Some(self.death_age_sum / self.death_count as f64)
        }
    }

    /// Total resource (both goods) currently standing in the landscape.
    pub fn total_resource(&self) -> f64 {
        self.substrate.resource.iter().map(|r| r[0] + r[1]).sum()
    }

    /// **Emergent state capacity** in `[0,1]`: the fraction of *intended*
    /// enforcement actions actually achieved (funded and not failed). With no
    /// enforcement attempted yet it is 1.0 (vacuously full). Tilly: a state's
    /// reach is what it can fund and execute, not what it decrees.
    pub fn state_capacity(&self) -> f64 {
        if self.enforce_intended == 0 {
            1.0
        } else {
            self.enforce_achieved as f64 / self.enforce_intended as f64
        }
    }

    /// **Emergent legitimacy / tax morale** in `[0,1]`: the population's
    /// reinforcing belief in the institution (see the `legit_level` field). Starts
    /// neutral at 0.5 and is updated each tick from realised compliance and clean
    /// delivery — it is read here, never assigned (Levi, *Of Rule and Revenue*).
    pub fn legitimacy(&self) -> f64 {
        self.legit_level
    }

    /// **Emergent corruption** in `[0,1]`: the share of public-pool outflow that
    /// was diverted rather than delivered to its intended use. 0 if nothing has
    /// flowed. Measured from the diversion ledger a rule writes — never set.
    pub fn corruption(&self) -> f64 {
        let total = self.pool_diverted + self.pool_delivered;
        if total <= 0.0 {
            0.0
        } else {
            self.pool_diverted / total
        }
    }

    // ===================== Phase 5: climate physics =====================

    /// **Unimodal temperature response** of net primary productivity, a Gaussian
    /// peaked at the productivity optimum `temp_opt` with width `temp_tolerance`:
    /// `exp(−((T−T_opt)/σ)²)`. It equals 1 at the optimum (the pre-industrial
    /// equilibrium, so the subsystem is a no-op there) and falls below 1 as the
    /// planet warms past it — growth peaks then declines, the mechanistic root of
    /// emergent climate damage (warming → lower NPP → lower carrying capacity).
    /// (Lindeman 1942 trophic energetics; productivity–temperature unimodality.)
    fn temp_response(&self, temperature: f64) -> f64 {
        let z = (temperature - self.params.temp_opt) / self.params.temp_tolerance;
        (-z * z).exp()
    }

    /// **CO₂-style radiative forcing** `F = λ·ln(C_atm / C_preindustrial)`
    /// (Myhre et al. 1998 logarithmic law). Zero at the pre-industrial stock and
    /// rising with the log of accumulated greenhouse gas.
    fn radiative_forcing(&self) -> f64 {
        let ratio = (self.c_atm / self.params.c_preindustrial).max(1e-9);
        self.params.forcing_lambda * ratio.ln()
    }

    /// Advance the greenhouse stock and temperature by one tick.
    ///
    /// Greenhouse stock: `dC = emissions − co2_decay·(C − C₀)` (first-order decay
    /// toward the pre-industrial reference). Temperature: the **zero-dimensional
    /// energy balance** (Budyko 1969; Sellers 1969)
    /// `heat_cap·dT/dt = (1−albedo)·S/4 − ε·σ·T⁴ + F`, with `F` the Myhre log
    /// forcing — so warming follows from the physics, never a fitted curve, and
    /// relaxes with the planet's thermal inertia.
    fn update_climate(&mut self) {
        // Greenhouse stock with first-order uptake back toward the reference.
        self.c_atm += self.emissions_this_tick
            - self.params.co2_decay * (self.c_atm - self.params.c_preindustrial);
        if self.c_atm < 0.0 {
            self.c_atm = 0.0;
        }

        // Energy balance: absorbed shortwave − outgoing longwave + GHG forcing.
        let absorbed = (1.0 - self.params.albedo) * self.params.solar_const / 4.0;
        let outgoing = self.params.emissivity * STEFAN_BOLTZMANN * self.temperature.powi(4);
        let forcing = self.radiative_forcing();
        let dt = (absorbed - outgoing + forcing) / self.params.heat_capacity;
        self.temperature += dt;
    }

    /// **Emergent surface temperature** (K) — read off the energy-balance state.
    pub fn temperature(&self) -> f64 {
        self.temperature
    }

    /// **Emergent atmospheric greenhouse stock** `C_atm` (model units).
    pub fn greenhouse_stock(&self) -> f64 {
        self.c_atm
    }

    /// **Emergent emissions flow** this tick (greenhouse units from production).
    pub fn emissions_flow(&self) -> f64 {
        self.emissions_this_tick
    }

    /// **Emergent equilibrium climate sensitivity**: the steady-state warming
    /// `ΔT` for a *doubling* of the greenhouse stock above pre-industrial, read
    /// off this world's own physics (not assumed). Linearising the energy balance
    /// about the current temperature, the outgoing-longwave feedback is
    /// `dR/dT = 4·ε·σ·T³`, and a doubling adds `F₂ₓ = λ·ln 2` of forcing, so
    /// `ΔT₂ₓ = λ·ln 2 / (4·ε·σ·T³)` (the standard no-feedback Planck response).
    /// A pure physics readout, like every other instrument.
    pub fn climate_sensitivity(&self) -> f64 {
        let f2x = self.params.forcing_lambda * std::f64::consts::LN_2;
        let planck = 4.0 * self.params.emissivity * STEFAN_BOLTZMANN * self.temperature.powi(3);
        if planck <= 0.0 {
            0.0
        } else {
            f2x / planck
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinism_same_seed_same_history() {
        let p = Primitives::demo();
        let mut a = World::new(p.clone());
        let mut b = World::new(p);
        for _ in 0..200 {
            a.step();
            b.step();
            assert_eq!(a.agents.alive_count(), b.agents.alive_count());
            assert_eq!(a.total_resource().to_bits(), b.total_resource().to_bits());
            assert_eq!(a.trades_this_tick.len(), b.trades_this_tick.len());
        }
    }

    #[test]
    fn population_finds_a_carrying_capacity() {
        let mut p = Primitives::demo();
        p.n_agents = 1200;
        let mut w = World::new(p);
        for _ in 0..150 {
            w.step();
        }
        let mid = w.agents.alive_count();
        for _ in 0..150 {
            w.step();
        }
        let end = w.agents.alive_count();
        assert!(end > 0, "population should not go extinct");
        let ratio = end as f64 / mid.max(1) as f64;
        assert!((0.5..2.0).contains(&ratio), "population not stationary: {mid} -> {end}");
    }

    #[test]
    fn regrowth_sustains_population_depletion_collapses_it() {
        let base = {
            let mut p = Primitives::demo();
            p.n_agents = 600;
            p
        };
        let alive_regrow = {
            let mut w = World::new(base.clone());
            for _ in 0..120 {
                w.step();
            }
            w.agents.alive_count()
        };
        let mut zero = base.clone();
        zero.regrowth_rate = 0.0;
        let alive_norigrow = {
            let mut w = World::new(zero);
            for _ in 0..120 {
                w.step();
            }
            w.agents.alive_count()
        };
        assert!(
            alive_regrow > alive_norigrow,
            "regrowth should sustain more life than depletion: {alive_regrow} vs {alive_norigrow}"
        );
    }
}
