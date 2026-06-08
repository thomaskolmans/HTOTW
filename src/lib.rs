//! # society_sim
//!
//! A system-dynamics simulator for exploring **how to best organise society**.
//!
//! The world is modelled as five coupled domains — [`state::Human`],
//! [`state::Economy`], [`state::Environment`], [`state::Animal`] and a set of
//! derived [`state::Planet`] composites — that evolve one year at a time.
//!
//! On top of the raw dynamics you can stack *parameterised policies*
//! ([`policy::Policy`]). Policies never mutate the world directly; instead they
//! contribute to a [`effects::PolicyEffects`] accumulator (a bundle of "levers"
//! such as carbon-pricing strength, redistribution, or conservation effort).
//! Because every policy only *adds* to this accumulator, stacking many policies
//! is well-defined and independent of the order in which they were declared.
//!
//! ## A minimal run
//!
//! ```
//! use society_sim::prelude::*;
//!
//! // Start from a present-day baseline world.
//! let scenario = Scenario::baseline_2025();
//!
//! // Stack two policies on top of each other.
//! let mut sim = Simulation::new(scenario);
//! sim.add_policy(Box::new(CarbonTax::new(2025, 0.6)));
//! sim.add_policy(Box::new(UniversalBasicIncome::new(2030, 0.4)));
//!
//! // Advance 50 years and inspect the recorded time-series.
//! let history = sim.run(50);
//! assert_eq!(history.len(), 51); // initial state + 50 steps
//! let last = history.last().unwrap();
//! println!("Year {}: wellbeing {:.2}/10", last.year, last.society.wellbeing);
//! ```
//!
//! Or hand control to a simulated government and watch it produce policy:
//!
//! ```
//! use society_sim::prelude::*;
//! let mut sim = Simulation::new(Scenario::baseline_2025());
//! sim.set_government(Box::new(ArchetypeGovernment::technocracy()));
//! let history = sim.run(75);
//! println!("overall score: {:.2}", history.last().unwrap().planet.overall);
//! ```
//!
//! ## Scientific honesty
//!
//! This is an **illustrative** model, not a forecast. The equations
//! (documented in `docs/MODEL.md`) are deliberately simple, directionally
//! reasonable couplings — chosen so that policy *trade-offs* become visible,
//! not so that any single number predicts the real future. Treat outputs as
//! "what tends to happen if these assumptions hold", never as prophecy.

pub mod dynamics;
pub mod effects;
pub mod engine;
pub mod governance;
pub mod policies;
pub mod policy;
pub mod scenario;
pub mod sim;
pub mod state;
pub mod util;

/// Convenient single-import surface for typical users.
pub mod prelude {
    pub use crate::effects::PolicyEffects;
    pub use crate::governance::{ArchetypeGovernment, Government};
    pub use crate::policies::*;
    pub use crate::policy::{Policy, PolicyStack};
    pub use crate::scenario::Scenario;
    pub use crate::sim::{Simulation, Snapshot};
    pub use crate::state::{
        Animal, Economy, Environment, Governance, Human, Ideology, Planet, Society, WorldState,
    };
}
