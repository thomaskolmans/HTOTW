# worldsim — a first-principles planetary simulator

> Simulate the whole world to find the best way to operate it.

`worldsim` is the full realisation of the HTOTW vision. The original
`society-sim` engine (in the repository root) was a proof of concept — an
abstract resource-grid agent model. `worldsim` rebuilds the idea at planetary
scale and with real subsystems, around four commitments:

1. **Nature, environment, physics and human psychology drive the world.**
   - A **spherical lat–lon planet** (`planet`): fractal continents and mountains;
     a diffusive **energy-balance climate** with latitudinal insolation, the
     ice–albedo feedback, greenhouse forcing and polar amplification (Budyko,
     Sellers, North, Myhre); a **water cycle** (ITCZ + storm-track rainfall
     belts, continentality, orographic lift, Clausius–Clapeyron scaling);
     **ecosystems** (Miami-model net primary productivity with a thermal
     optimum, logistic biomass and Schaefer fisheries, soils, a species–area
     biodiversity index); and finite, geographically clustered **fossil and
     mineral deposits**.
   - **Individual humans** (`people`): location, age, savings, human capital,
     and heritable **psychology** — patience, risk aversion, fairness,
     conformity — with biological mortality (Gompertz–Makeham + deprivation +
     heat) and an emergent fertility decision.
   - A **multi-sector economy** (`economy`): food, water, fossil and clean
     energy, materials and manufactured goods, each with capital, learning-by-
     doing (Wright/Arrow), and emergent prices that read off realised scarcity.

2. **Societies are configurable inputs** (`config`, `society`). The build-up of
   rules, structures and institutions is a `SocietyParams`: property regime,
   taxation and progressivity, transfers, public spending on schooling /
   infrastructure / research / enforcement, a carbon price, border openness, and
   a governance mechanism (fixed, majority, or wealth-weighted referenda). You
   describe an existing or imagined society — in code or a strict `.world`
   text file — and run it.

3. **Everything social is measured, never set** (`measure`). There is no input
   anywhere for GDP, the Gini, life expectancy, well-being or a temperature
   trajectory. Instruments take `&World` and cannot mutate it; you calibrate
   *primitives* until measured reality matches — you simulate **to** the
   numbers, never **from** them. The assumptions the model does make are
   physical/biological, centralised and cited in `src/constants.rs`.

4. **Search for the best way to operate the world** (`search`). A deterministic
   (μ+λ) evolution strategy explores the society-parameter space, scoring each
   candidate on the measured long-run welfare functional — the geometric mean of
   well-being × equity × sustainability × survival — averaged over a seed
   ensemble and the run's final years (sustained, not lucky, welfare).

Everything is **dependency-free** and **bit-deterministic** (same config + seed
⇒ identical history), like the original engine.

## Usage

```sh
cargo run --release -p worldsim -- run --archetype social-democracy --years 250
cargo run --release -p worldsim -- compare --archetype laissez-faire \
    --archetype social-democracy --archetype degrowth-commons --years 250
cargo run --release -p worldsim -- search --years 150 --generations 8
cargo run --release -p worldsim -- list
```

`run --file PATH` loads a `.world` scenario:

```text
name = my-world
[world]
seed = 1
grid-lon = 72
grid-lat = 36
population = 4000
polities = 6
patience = 0.2..0.8

[society]          # applies to every polity
property = commons-quota
tax-rate = 0.25
tax-progressivity = 0.8
transfer = floor
education-share = 0.3
carbon-price = 5
governance = majority

[society 2]        # override one polity
property = open-access
```

## What emerges (none of it is an input)

Run the ungoverned baseline and you watch a **planetary overshoot**: cheap
fossil energy lets the population boom past the land's carrying capacity, the
climate warms several degrees, biodiversity collapses, and the population
crashes back to a poorer, hotter equilibrium — the tragedy of the commons at
planetary scale, entirely emergent. Add a carbon price and the energy mix tilts
to clean and the warming eases; add a progressive tax and a floor and the
measured Gini falls; fund schooling and human capital and output per head rise.
The `search` then finds, rather than assumes, the combination that scores best.

## Status

A complete, working first version of the full simulator: planet, people,
economy, configurable societies, measured outcomes, and the optimiser, all
green under test. It is an **illustrative** model in the agent-based tradition —
grounding the primitives in real physics and biology makes the emergent
directions and trade-offs trustworthy; no single projected number is a
prophecy. Natural next steps: calibrate the primitives to historical data
(method of simulated moments, as the original engine does), richer trade and
migration between polities, disease and conflict, and a spatial climate-impact
field feeding agriculture directly.
