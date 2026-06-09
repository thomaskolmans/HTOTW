//! # worldsim — a first-principles planetary simulator
//!
//! The complete loop the project exists for:
//!
//! 1. **Nature, environment, physics and human psychology drive the world.**
//!    A spherical-grid planet with latitudinal insolation, an energy-balance
//!    climate, a hydrological cycle, ecosystems (primary productivity, soils,
//!    forests, fisheries, biodiversity) and finite mineral/fossil deposits
//!    ([`planet`]); a demographically real population of individual humans with
//!    needs, ageing, and heritable psychology ([`people`]); and a multi-sector
//!    economy (food, water, fuel, materials, manufactured goods) with emergent
//!    prices, capital, technological learning and an energy transition
//!    ([`economy`]).
//! 2. **Societies are configurable inputs.** The build-up of rules, structures,
//!    institutions, policies and laws is a [`config::SocietyParams`] — property
//!    regimes, taxation and transfers, public investment, conservation quotas,
//!    carbon pricing, border openness, governance mechanisms ([`society`]).
//!    You can describe an existing society and mass-do or undo its choices.
//! 3. **Everything social is measured, never set** ([`measure`]). There is no
//!    input anywhere for a GDP, a Gini, a life expectancy or a temperature
//!    trajectory; they are read off the simulated world. This is the project's
//!    hard rule, carried over from the original engine and enforced the same
//!    way: instruments take `&World` and cannot mutate it.
//! 4. **Search for the best way to operate the world** ([`search`]): a
//!    deterministic evolutionary optimiser over the society-parameter space,
//!    scored on a measured long-run welfare functional (well-being x equity x
//!    sustainability x survival), across seed ensembles.
//!
//! ## On "no assumptions"
//!
//! Every model assumes; pretending otherwise is how assumptions hide. The
//! discipline here is: **no social outcome is ever an input**, and every
//! physical/biological assumption is explicit, centralised and cited in
//! [`constants`] (and `docs/ASSUMPTIONS.md`). Calibrate primitives to match
//! measured reality — simulate *to* the numbers, never *from* them.
//!
//! ## Minimal run
//!
//! ```
//! use worldsim::{config::WorldConfig, world::World};
//! let mut cfg = WorldConfig::default();
//! cfg.nlon = 24; cfg.nlat = 12; cfg.n_agents = 300; // tiny doc-test world
//! let mut w = World::new(&cfg);
//! for _ in 0..20 { w.step(); }
//! let m = w.measure();
//! assert!(m.population > 0);
//! ```

pub mod calibrate;
pub mod config;
pub mod constants;
pub mod economy;
pub mod measure;
pub mod people;
pub mod planet;
pub mod rng;
pub mod search;
pub mod society;
pub mod world;

pub use config::{SocietyParams, WorldConfig};
pub use measure::Measurements;
pub use rng::Rng;
pub use world::World;
