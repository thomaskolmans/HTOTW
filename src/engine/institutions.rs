//! **Institutions & policy-as-rules** (Phase 3).
//!
//! A [`Rule`] is a *composable mechanism* — never an outcome. Each rule may only
//! reach into the mechanisms and payoffs of the [`World`] (impose a harvest
//! quota, assign property rights, levy a tax into the public pool, redistribute
//! that pool, fund a public good, divert some of it corruptly). It may **not**
//! set any macro quantity. Whether a rule *works* — whether the commons survives,
//! inequality falls, the state has capacity, the regime is seen as legitimate, or
//! officials skim the pool — is then **measured** by the read-only
//! [`crate::engine::instruments`], exactly like every other emergent number.
//!
//! Rules run in a dedicated **institutional phase** at the top of
//! [`World::step_with_rules`]: they configure this tick's harvest mechanism and
//! act on accumulated holdings (taxation/redistribution) *before* production and
//! exchange respond to those incentives. Stacking rules is order-stable: each
//! rule only adds to the shared mechanism state.
//!
//! ## Citations
//! - Hardin (1968), *The Tragedy of the Commons* — open access destroys a shared
//!   resource because each user's private optimum ignores the externality.
//! - Ostrom (1990), *Governing the Commons* — locally-enforced rules (quotas,
//!   monitoring, graduated sanctions) can sustain a commons without privatising
//!   or nationalising it.
//! - Demsetz (1967), *Toward a Theory of Property Rights* — property rights
//!   emerge to internalise externalities; an owner conserves future value.
//! - Axelrod (1984), *The Evolution of Cooperation* — cooperation is sustainable
//!   when defection is detectable and punishable.
//! - Olson (1965), *The Logic of Collective Action* — enforcement is a costly
//!   public good; without funding it is under-provided.

use super::rng::Rng;
use super::world::World;

/// A composable institution: a mechanism the world runs each tick. Implementors
/// mold **mechanisms and payoffs only** (quotas, property, taxes, the public
/// pool) — never macro outcomes, which stay measured by the instruments.
pub trait Rule {
    /// Human-readable name (for experiment reports).
    fn name(&self) -> &str;
    /// Apply the mechanism to the world for this tick. Receives the world's RNG
    /// so any stochastic enforcement/diversion stays on the single deterministic
    /// stream (same seed ⇒ identical history).
    fn enforce(&self, world: &mut World, rng: &mut Rng);
}

/// **Open access** (the null institution / Hardin baseline): no quota, no owners.
/// Every agent strips its cell — the commons is mined past regeneration and the
/// land degrades. Used as the control against which a conservation rule is scored.
#[derive(Debug, Clone, Default)]
pub struct OpenAccess;

impl Rule for OpenAccess {
    fn name(&self) -> &str {
        "open-access"
    }
    fn enforce(&self, world: &mut World, _rng: &mut Rng) {
        // Explicitly clear any standing mechanism so the commons is unmanaged.
        world.harvest_quota = None;
    }
}

/// **Harvest quota** (an Ostrom-style appropriation rule): cap the fraction of a
/// cell's standing stock any agent may take this tick. Compliance is voluntary
/// and imperfect inside the engine (it rises with emergent legitimacy and is
/// otherwise only as binding as funded enforcement reaches), so the *measured*
/// sustainability is genuinely emergent, not decreed. The quota is a mechanism;
/// the resulting resource level is read off the substrate.
#[derive(Debug, Clone)]
pub struct HarvestQuota {
    /// Fraction of standing stock allowed per harvest (e.g. 0.5 = take half).
    pub fraction: f64,
}

impl HarvestQuota {
    pub fn new(fraction: f64) -> Self {
        HarvestQuota { fraction: fraction.clamp(0.0, 1.0) }
    }
}

impl Rule for HarvestQuota {
    fn name(&self) -> &str {
        "harvest-quota"
    }
    fn enforce(&self, world: &mut World, _rng: &mut Rng) {
        world.harvest_quota = Some(self.fraction);
    }
}

/// **Progressive wealth tax → public pool** funding enforcement and public goods.
/// Levies a fraction of each living agent's **energy reserve** (the survival
/// wealth the Gini is measured on) *above a per-capita allowance*, into the
/// shared [`World::public_pool`]. Taxing only the surplus above the mean makes it
/// progressive, so a flat redistribution then compresses the distribution. Some
/// agents *evade* with a probability that falls as legitimacy rises (tax morale,
/// Levi); evasion is defiance and is only caught as far as state capacity
/// reaches, so realised revenue, tax morale and capacity all emerge. The tax is
/// the mechanism; the pool size, legitimacy and capacity are measured.
#[derive(Debug, Clone)]
pub struct WealthTax {
    /// Fraction of the *above-allowance* energy reserve taken each tick.
    pub rate: f64,
}

impl WealthTax {
    pub fn new(rate: f64) -> Self {
        WealthTax { rate: rate.clamp(0.0, 1.0) }
    }
}

impl Rule for WealthTax {
    fn name(&self) -> &str {
        "wealth-tax"
    }
    fn enforce(&self, world: &mut World, rng: &mut Rng) {
        let legit = world.perceived_legitimacy();
        let n = world.agents.len();
        // Per-capita allowance = mean reserve; only surplus above it is taxed
        // (progressivity). Computed read-only before any mutation.
        let pop = world.agents.alive_count();
        if pop == 0 {
            return;
        }
        let mean: f64 = {
            let mut s = 0.0;
            for i in 0..n {
                if world.agents.alive[i] {
                    s += world.agents.energy[i];
                }
            }
            s / pop as f64
        };
        for i in 0..n {
            if !world.agents.alive[i] {
                continue;
            }
            let surplus = (world.agents.energy[i] - mean).max(0.0);
            let due = surplus * self.rate;
            if due <= 0.0 {
                continue;
            }
            // Voluntary compliance with prob rising faster than legitimacy
            // (conditional cooperation / tax morale, Levi & Axelrod).
            let voluntary = rng.f64() < legit.sqrt();
            if voluntary {
                world.record_compliance(true);
                world.agents.energy[i] -= due;
                world.public_pool += due;
            } else {
                world.record_compliance(false);
                world.enforce_intended += 1;
                // Caught only as far as funded enforcement reaches.
                if world.try_enforce_public() {
                    world.enforce_achieved += 1;
                    world.agents.energy[i] -= due;
                    world.public_pool += due;
                }
                // else: evaded; revenue lost (capacity gap shows up emergently).
            }
        }
    }
}

/// **Redistribution** as a means-tested safety net (negative income tax): pay
/// the public pool out to the agents *below the per-capita mean reserve*, in
/// proportion to how far each falls short. Topping up the bottom of the
/// distribution from a pool filled by the top both **compresses the measured
/// Gini** and reduces starvation deaths — a doubly stabilising, well-motivated
/// transfer. The compression is an emergent consequence of the mechanism, never
/// a set target. (A flat demogrant works too but is noisier near the survival
/// margin; the floor is the robust version.)
#[derive(Debug, Clone)]
pub struct Redistribute {
    /// Fraction of the current pool to disburse this tick.
    pub fraction: f64,
}

impl Redistribute {
    pub fn new(fraction: f64) -> Self {
        Redistribute { fraction: fraction.clamp(0.0, 1.0) }
    }
}

impl Rule for Redistribute {
    fn name(&self) -> &str {
        "redistribute"
    }
    fn enforce(&self, world: &mut World, _rng: &mut Rng) {
        let pop = world.agents.alive_count();
        if pop == 0 || world.public_pool <= 0.0 {
            return;
        }
        let n = world.agents.len();
        // Per-capita mean reserve = the means-test threshold (read-only first).
        let mean: f64 = {
            let mut s = 0.0;
            for i in 0..n {
                if world.agents.alive[i] {
                    s += world.agents.energy[i];
                }
            }
            s / pop as f64
        };
        // Total shortfall of the below-mean agents → weights for the transfer.
        let total_short: f64 = (0..n)
            .filter(|&i| world.agents.alive[i])
            .map(|i| (mean - world.agents.energy[i]).max(0.0))
            .sum();
        if total_short <= 0.0 {
            return;
        }
        let disburse = (world.public_pool * self.fraction).min(total_short);
        world.public_pool -= disburse;
        world.pool_delivered += disburse;
        for i in 0..n {
            if world.agents.alive[i] {
                let short = (mean - world.agents.energy[i]).max(0.0);
                if short > 0.0 {
                    world.agents.energy[i] += disburse * (short / total_short);
                }
            }
        }
    }
}

/// **Corrupt official**: a kleptocratic mechanism that skims a fraction of the
/// public pool into private waste each tick (the value leaves the system). The
/// diversion is logged so the corruption instrument and the legitimacy feedback
/// (delivery quality) both move — emergently shrinking state capacity, since a
/// looted pool can fund less enforcement. Models the failure mode Ostrom and
/// North warn of.
#[derive(Debug, Clone)]
pub struct CorruptOfficial {
    /// Fraction of the pool diverted each tick.
    pub skim: f64,
}

impl CorruptOfficial {
    pub fn new(skim: f64) -> Self {
        CorruptOfficial { skim: skim.clamp(0.0, 1.0) }
    }
}

impl Rule for CorruptOfficial {
    fn name(&self) -> &str {
        "corrupt-official"
    }
    fn enforce(&self, world: &mut World, _rng: &mut Rng) {
        let take = world.public_pool * self.skim;
        if take <= 0.0 {
            return;
        }
        world.public_pool -= take;
        world.pool_diverted += take;
    }
}

/// **Decarbonisation mandate** (Phase 5): a clean-production mechanism that
/// lowers the carbon **intensity** of output this tick (a mandated shift to
/// cleaner techniques / abatement effort, in the spirit of a Pigouvian standard).
/// It sets [`World::emission_scale`] to `1 − abatement`, so the same physical
/// production emits less greenhouse gas. The rule molds only the emission
/// mechanism; the resulting greenhouse stock and temperature stay **measured** by
/// the instruments. Pairing this against an unmitigated baseline at the same seed
/// shows the policy lowers the *emergent* temperature (and thus the climate
/// damage), never a set figure. (Pigou 1920; Nordhaus DICE-style abatement.)
#[derive(Debug, Clone)]
pub struct Decarbonize {
    /// Fraction of emissions abated this tick (0 = none, 1 = fully clean).
    pub abatement: f64,
}

impl Decarbonize {
    pub fn new(abatement: f64) -> Self {
        Decarbonize { abatement: abatement.clamp(0.0, 1.0) }
    }
}

impl Rule for Decarbonize {
    fn name(&self) -> &str {
        "decarbonize"
    }
    fn enforce(&self, world: &mut World, _rng: &mut Rng) {
        world.emission_scale = (1.0 - self.abatement).clamp(0.0, 1.0);
    }
}

/// **Property rights** (Demsetz): assign each occupied cell to its occupant as
/// owner. An owner self-limits its take (it internalises the resource's future
/// value), so privatised land is conserved without a central quota. Open access
/// (no owners) is the contrast. Ownership is re-asserted each tick from current
/// occupancy (a simple homestead rule).
#[derive(Debug, Clone, Default)]
pub struct PropertyRights;

impl Rule for PropertyRights {
    fn name(&self) -> &str {
        "property-rights"
    }
    fn enforce(&self, world: &mut World, _rng: &mut Rng) {
        // No central quota under a pure property regime; owners self-limit.
        world.harvest_quota = None;
        let n = world.agents.len();
        for i in 0..n {
            if world.agents.alive[i] {
                let c = world.agents.cell[i];
                world.cell_owner[c] = i;
            }
        }
    }
}
