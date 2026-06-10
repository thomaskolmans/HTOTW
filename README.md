# worldsim — a first-principles planetary simulator

> Simulate the whole world to find the best way to operate it.

**worldsim** ("how to organise the world") simulates a whole planet from first
principles — nature, environment, physics and human psychology drive it; the
build-up of rules, structures and institutions is a configurable input;
everything social is measured, never set; and a search layer finds the
best-measured way to operate the world. Four commitments:

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

   Critically, **no central planner runs the economy**: each working-age person
   *chooses its own sector* by following last year's observed wages (cobweb
   dynamics with switching friction); investment is individual saving driven by
   patience; children are fed by **kin provisioning** and the deprived by
   **voluntary charity** from the fair-minded; fertility is an individual
   decision with a Becker quantity–quality opportunity cost (so the demographic
   transition *emerges* from rising human capital). The only collective levers
   are the configured institutions.

2. **Societies are configurable inputs** (`config`, `society`). The build-up of
   rules, structures and institutions is a `SocietyParams`: property regime,
   taxation and progressivity, transfers, public spending on schooling /
   infrastructure / research / enforcement, a carbon price, border openness, and
   a governance mechanism. Under `majority` or `wealth-weighted` governance the
   fiscal/ecological dials are not scripted — they move each period by
   **referendum**, where every person votes its *measured self-interest* (the
   below-mean back redistribution, the climate-harmed back a carbon price, but
   fossil-sector workers vote it down — the just-transition conflict, emergent).
   You describe an existing or imagined society — in code or a strict `.world`
   text file — and run it.

3. **Everything social is measured, never set** (`measure`). There is no input
   anywhere for GDP, the Gini, life expectancy, well-being or a temperature
   trajectory. Instruments take `&World` and cannot mutate it; you calibrate
   *primitives* until measured reality matches — you simulate **to** the
   numbers, never **from** them. The assumptions the model does make are
   physical/biological/behavioural, centralised and cited in `src/constants.rs`.

4. **Search for the best way to operate the world** (`search`). A deterministic
   (μ+λ) evolution strategy explores the society-parameter space, scoring each
   candidate on the measured long-run welfare functional — a weighted geometric
   mean of well-being × equity × sustainability × survival — averaged over a
   seed ensemble and the run's final years (sustained, not lucky, welfare). The
   **weights are an explicit `Objective` input** (the evaluator's *values*), not
   the simulator's opinion: a `headcount` objective and a `green` objective
   rightly crown different societies. Values are where the judgement is made,
   out in the open — never smuggled into the world.

Everything is **dependency-free** and **bit-deterministic** (same config + seed
⇒ identical history).

## Usage

```sh
cargo run --release -- run --archetype social-democracy --years 250
cargo run --release -- compare --archetype laissez-faire \
    --archetype degrowth-commons --objective green --years 250
cargo run --release -- search --objective balanced --years 150
cargo run --release -- calibrate          # fit primitives to documented reality
cargo run --release -- whatif --archetype laissez-faire --set carbon-price=8
cargo run --release -- map --layer biomass --archetype degrowth-commons
cargo run --release -- list
```

### Calibration — simulate *to* reality

`worldsim calibrate` is the deepest answer to "make no assumptions": rather than
trusting the scale-model primitives (labour yield, the fertility ceiling, the
fossil endowment), it **fits them by the Method of Simulated Moments** so the
world's *measured, emergent* moments — life expectancy, the wealth Gini,
population stationarity, deprivation — land on documented pre-industrial values
(Riley 2005; Scheidel 2017; McEvedy & Jones 1978). The targets live only inside
a loss function; the fitted output is a *primitive* vector. A typical run drops
the loss ~60× and reproduces a ~30-year life expectancy, a ~0.7 wealth Gini and
a near-stationary population — none of it ever set. Calibrate first, then trust
the counterfactuals.

### Trade, disease and war

Polities now **trade** (value-balanced multilateral exchange with iceberg
transport costs — comparative advantage emerges, and a `trade-openness` dial is
a real policy lever); **disease** is endemic and density-driven (McNeill),
worsened by malnutrition and tempered by knowledge (McKeown), with **pandemics**
that travel the trade network; and **war** is scarcity-driven (Homer-Dixon) and
trade-tempered (the capitalist peace, Gartzke) between neighbouring polities. A
fed, freely-trading world rolls no wars; a fragmented, scarce, autarkic one
descends into recurring conflict — all emergent, none scripted.

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
