# htotw — how to organise the world

> *We've engineered almost everything to perfection, yet we still don't know how
> to live together well. This is a simulator for trying.*

This repository contains **two** simulators:

- **[`worldsim`](worldsim/)** — the full, planetary-scale realisation of the
  vision (start here). A spherical planet with an energy-balance climate, a
  water cycle and ecosystems; individual humans with needs, ageing and heritable
  psychology; a multi-sector economy with emergent prices, learning and an
  energy transition. **Societies are configurable inputs** (laws, taxes,
  property regimes, carbon pricing, governance); **every social outcome is
  measured, never set**; and an **evolutionary search finds the best-measured
  way to operate the world**. See [`worldsim/README.md`](worldsim/README.md) and
  the explicit, cited assumptions in [`docs/ASSUMPTIONS.md`](docs/ASSUMPTIONS.md).

  ```sh
  cargo run --release -p worldsim -- run --archetype social-democracy --years 250
  cargo run --release -p worldsim -- compare --archetype laissez-faire --archetype degrowth-commons
  cargo run --release -p worldsim -- search        # find the best way to operate the world
  ```

- **`society-sim`** (this crate, below) — the original proof-of-concept
  *physics engine for society*: an abstract resource-grid agent model where
  population, inequality, prices, money, GDP, institutions, governance, climate
  and even *which policies a society adopts* all emerge from physical and
  psychological primitives. It remains the conceptual seed and a fast,
  well-tested sandbox.

Both are written in **Rust**, are **dependency-free**, run **deterministically**
(same seed → bit-identical history), and are built test-first.

---

## `society-sim` (the proof of concept)

## The one rule that defines this project

> **Macro quantities are never inputs. They are measured as they emerge, never
> set. To reproduce a real-world number you calibrate the *primitives* until the
> *measured output* matches — you simulate _to_ the numbers, never _from_ them.**

A Gini coefficient of 0.39, a GDP, a life expectancy — these are *consequences*
of how a society produces, trades and is governed, not natural constants. Earlier
iterations of this project plonked such averages in as starting values and nudged
them with invented coefficients; that bakes in the very thing the simulator is
supposed to explain. So the engine instead models the *primitives* (a resource
landscape, ecological regrowth, metabolism, perception, mortality, a bargaining
rule) and lets society fall out. This is enforced *by the type system*:
measurements are computed by read-only "instruments" that take `&World` and
**cannot** mutate it.

It is an **illustrative** model in the agent-based-modelling tradition (the
Sugarscape lineage — Epstein & Axtell, *Growing Artificial Societies*, 1996), not
a forecast. Grounding the *primitives* in real physics/biology makes the emergent
*directions and trade-offs* trustworthy; no single projected number is a
prophecy.

## What emerges (none of it is an input)

| From these primitives… | …this emerges and is measured |
|---|---|
| resource landscape, logistic regrowth `r·S·(1−S/K)`, metabolism (Kleiber), Gompertz–Makeham mortality | population, **carrying capacity**, **life expectancy**, **wealth Gini from an equal start** |
| two goods, heterogeneous productivity, local bilateral bargaining | **prices** (realised ratios), **money** (Menger), **GDP**, specialization, gains-from-trade |
| property/tax/redistribution as composable *rules*, costly imperfect enforcement | **state capacity, legitimacy, corruption**, the **tragedy of the commons** & its resolution |
| agent policy preferences + a voting/aggregation mechanism | **the active policy set itself** (majority vs. wealth-weighted select different rules) |
| production emissions, greenhouse stock, an energy-balance climate | **temperature**, **climate sensitivity**, and **climate damage** (warming throttles carrying capacity — mechanistic, not a fitted curve) |
| heritable **psychology** (patience, risk aversion, fairness, conformity — Fehr–Schmidt, Pratt–Arrow, Cialdini) | a patient culture **sustains its commons with no law at all**, conformists uphold institutions mavericks defy, fair-minded societies **redistribute even under plutocracy**, measured **subjective well-being** |

## Quick start

Requires a recent stable Rust toolchain (`rustc`/`cargo`).

```sh
cargo build            # build (release: cargo build --release)
cargo test             # run the full suite (lib + integration + doctests)
cargo run --bin simctl -- help
```

### The `simctl` CLI

```text
simctl society [--file PATH | --preset NAME] [--ticks N] [--seeds A,B,C]
                                                           # run a society spec (.soc) and report its emergent outcome
simctl whatif  [--file PATH | --preset NAME] [--do LAW[=VAL]]... [--undo LAW]... [--sweep]
                                                           # mass do/undo laws and measure the counterfactual
simctl render  [--preset NAME] [--ticks N] [--seed S]      # watch a run: ASCII landscape + sparklines
simctl trace   [--preset NAME] [--ticks N] [--csv PATH]    # record the emergent time-series to CSV
simctl calibrate                                           # tune PRIMITIVES to hit real target moments (MSM)
simctl experiment                                          # rank ways of organising society by emergent welfare
simctl bench   [--agents N] [--cells N] [--ticks N] [--threads N]  # large-population scaling benchmark
simctl list                                                # presets / societies / laws / scenarios / policies
```

Agent-engine presets for `render`/`trace`: `demo`, `fragile-commons`,
`warming-world`, `human-nature`.

### Societies as input, laws as the experiment

The point of the project is to **input a society and mass-do or undo its laws
and structures** to see how the world would work. A society is a plain-text
`.soc` spec (see [`societies/`](societies)): the *primitives* (geography,
ecology, biology, **psychology**), the **stack of laws** it lives under, and
optionally how it governs itself. Five archetypes ship built in —
`open-frontier`, `stewardship-commons`, `egalitarian-green`, `laissez-faire`,
`extractive-autocracy` — as starting points to calibrate toward real societies.

```sh
# Run an archetype and read its emergent outcome (nothing in it was set):
cargo run --release --bin simctl -- society --preset egalitarian-green

# Counterfactual: repeal two laws, same seeds in both arms — the laws are the
# only difference, so every delta is attributable to them:
cargo run --release --bin simctl -- whatif --preset egalitarian-green \
    --undo decarbonize --undo wealth-tax

# Mass do/undo: evaluate EVERY subset of the society's law stack and rank the
# regimes by measured welfare (prosperity × equity × sustainability × survival):
cargo run --release --bin simctl -- whatif --preset egalitarian-green --sweep
```

The sweep is the "engineer the best way to organise the world" loop in one
command — and it bites: it found that a 0.3-per-tick wealth tax collapses every
regime containing it (per-tick taxes on a wealth *stock* must be a few percent,
exactly as in reality), which is why the bundled archetype levies 0.04.

### Interactive visualizer (`simviz`) — runs in your browser, on any OS

```sh
cargo run --bin simviz          # opens http://127.0.0.1:8080 in your browser
cargo run --bin simviz -- --port 9000 --no-open
```

`simviz` runs the engine **natively** (full speed) and serves a self-contained
interactive UI to your **browser**, so it works identically on Linux, Windows and
macOS with nothing to install. It's **dependency-free** — a tiny std-only HTTP
server plus an embedded HTML/Canvas page. You get:

- a live **landscape map** (good 0 = red, good 1 = green, agents as dots that
  brighten with wealth),
- **play / pause / step** and a speed control,
- a **preset** picker (`demo` / `fragile-commons` / `warming-world`), a seed, and
  sliders for key **primitives** (agent count, resource capacity, regrowth,
  metabolism, birth threshold),
- **policy-rule toggles** (harvest quota, property rights, wealth tax,
  redistribute, decarbonize, corrupt official),
- and **live charts** of the emergent measurements (population, Gini, wealth,
  commons health, temperature…) responding as the society runs.

Every number shown is *measured* from agent state, never set.

**See a society run** (resource map + emergent series in your terminal):

```sh
cargo run --bin simctl -- render --preset warming-world --ticks 250 --seed 7
```

**"Simulate _to_ the numbers"** — the calibrator tunes primitives until the
*measured* Gini and life expectancy land on real-world targets:

```sh
cargo run --bin simctl -- calibrate
#   loss: start 0.308  ->  fitted 0.0014
#   EMERGENT at fitted primitives: wealth_gini = 0.387 (target 0.39), life_expectancy = 67.4 (target 70)
```

**Rank ways of organising society** by emergent welfare (a same-seed A/B over regimes):

```sh
cargo run --bin simctl -- experiment
#   open-access        welfare 0.622 | commons 0.635
#   harvest-quota(0.3) welfare 0.676 | commons 0.999   <- Ostrom beats the tragedy of the commons
```

## Using it as a library

```rust
use society_sim::engine::{Primitives, World, instruments};

let mut world = World::new(Primitives::demo());
for _ in 0..200 { world.step(); }
let m = instruments::measure(&world);

// Inequality EMERGED from a perfectly equal start — it was never set:
assert!(m.wealth_gini > 0.0);
println!("pop {}  gini {:.2}  life expectancy {:.0}", m.population, m.wealth_gini, m.life_expectancy);
```

## Architecture

The engine is the `society_sim::engine` module — a hand-rolled struct-of-arrays
world with an explicit, order-stable phased tick (substrate regrowth →
perception → production → exchange → institutional enforcement → climate → vital
events). No ECS framework: determinism, snapshotting (`World: Clone`), and a
dependency-free build come first.

| Module | Layer |
|---|---|
| `engine::rng` | deterministic PRNG (SplitMix64 + xoshiro256**) |
| `engine::world` | substrate (logistic resources, energy-balance climate) + agents (metabolism, movement, trade, reproduction, death, **psychology**) |
| `engine::institutions` | composable policy **`Rule`s** (open access, quota, property rights, wealth tax, redistribution, decarbonize…) |
| `engine::polity` | agent policy preferences + collective-choice mechanisms → the active rule set emerges |
| `engine::society` | the **`.soc` society spec**: primitives + law stack + governance as input (never an outcome) |
| `engine::counterfactual` | **mass do/undo**: `whatif` law edits & `sweep` every law subset, same-seed, ranked by measured welfare |
| `engine::instruments` | **read-only** measurement of every macro quantity |
| `engine::calibration` | Method-of-Simulated-Moments calibration + experiment harness |
| `engine::trace` | CSV time-series + ASCII visualisation |
| `engine::parallel` | std-only, bit-deterministic parallelism for large populations |

Full architecture, the phased roadmap, and citations are in
[`docs/ENGINE.md`](docs/ENGINE.md). Real-world figures used as **calibration
targets** (with sources) are in [`docs/RESEARCH.md`](docs/RESEARCH.md).

> An earlier aggregate system-dynamics model (`dynamics`, `scenario`, `sim`,
> `state`, `policies`, `governance`) remains in the crate as a "macro twin" /
> sanity oracle. The agent-based `engine` is the real thing.

## Testing philosophy

Tests assert **emergence**, not back-fitted magnitudes: seed-stable *bands*
("inequality arises from an equal start"), *distributional* checks, and *regime
comparisons* under a shared seed (an open-access commons collapses; a quota
sustains it; redistribution lowers the measured Gini). Determinism makes these
stable rather than flaky, and is itself tested (including identical results
across thread counts).

## Status

Phases 1–11 implemented and green: substrate+agents, exchange, institutions,
calibration, climate, collective choice, visualisation, deterministic scaling,
**human psychology** (patience / risk aversion / fairness / conformity as
heritable behavioural primitives, well-being measured), **society-as-input**
(the `.soc` spec + archetype presets) and **mass do/undo counterfactuals**
(`simctl whatif` / `--sweep`). Possible next steps: calibrating `.soc` specs
against real-country target moments end-to-end, a spatial (per-cell) climate
field, a deeper multi-good/credit economy, migration and conflict, and exposing
collective choice and societies in `simviz`.

## License

MIT. See `Cargo.toml`.
