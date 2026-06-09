# ASSUMPTIONS.md — what `worldsim` assumes, and what it refuses to

Every simulator assumes; the discipline that makes one trustworthy is keeping
the assumptions **explicit, physical, and free of the outcomes you want to
study**. `worldsim` follows two rules.

## Rule 1 — no social outcome is ever an input

There is no field, key or knob anywhere for GDP, the Gini coefficient, life
expectancy, well-being, a price, an unemployment rate, or a temperature
trajectory. These are computed by the read-only instruments in
`worldsim::measure` from raw state. The type system enforces it: instruments
take `&World` and cannot mutate it. To reproduce a real statistic you calibrate
the *primitives* until the *measured* output matches — you simulate **to** the
numbers, never **from** them. The `.world` parser rejects any such key.

## Rule 2 — every physical/biological assumption is centralised and cited

All of them live in `worldsim/src/constants.rs`, each with its source. The load-
bearing ones:

| Domain | Assumption | Source |
|---|---|---|
| Climate | Zero-/one-layer energy balance; σT⁴ outgoing; effective emissivity ≈ 0.61 → ~288 K mean | Budyko 1969; Sellers 1969 |
| Climate | Annual-mean insolation Q(φ)=S₀/4·(1−0.477·P₂(sinφ)); diffusion D≈0.6 | North 1975 |
| Climate | CO₂ forcing F = 5.35·ln(C/C₀); ECS ~3 K per doubling via the Planck+feedback slope | Myhre 1998; IPCC AR6 |
| Climate | Polar amplification ≈ 1.6×; mixed-layer thermal inertia (~15 yr) | IPCC AR6 ch.4 |
| Ecology | Net primary productivity from temperature & precipitation (Miami model), **plus a high-temperature decline** for heat stress | Lieth 1975; Huang 2019; Duffy 2021 |
| Ecology | Logistic biomass regrowth; Schaefer surplus-production fisheries; species–area biodiversity (z≈0.25) | Verhulst; Schaefer 1954; MacArthur & Wilson 1967 |
| Ecology | Soil erodes ~10× faster than it forms under intensive use | Montgomery 2007 |
| Demography | Gompertz–Makeham mortality + infant penalty + deprivation + heat stress | Gompertz 1825; Sherwood & Huber 2010 |
| Demography | Physiological fertility ceiling; realised rate is an agent decision | Bongaarts 1978 |
| Economy | Constant returns to labour at the margin, capital Cobb–Douglas α≈0.3, output ceilinged by the finite resource (Malthusian carrying capacity) | Solow; Malthus |
| Economy | Learning-by-doing: productivity rises with cumulative output; clean energy learns fastest | Wright 1936; Arrow 1962; Way 2022 |
| Economy | Emissions ∝ fossil throughput + land-use change | IPCC inventories |

Numbers are chosen to put the *pre-industrial* world in documented bands (an
Earth-like land fraction, a ~288 K mean, a pre-modern life-expectancy regime).
They are **scale-model** units, not predictions; the realised macro quantities
emerge and are measured.

## Rule 3 — social structure emerges or is configured; it is never hard-coded

Mechanisms that earlier hid as fixed rules are now either **emergent from
individual choice** or **explicit configurable inputs**:

| Structure | How it is produced now |
|---|---|
| Labour allocation across sectors | each worker **chooses** by following observed wages (cobweb dynamics + switching friction; Ezekiel 1938, Artuç 2010) — no planner |
| Provisioning of dependants | **kin provisioning** (parents pay their children's bills; Kaplan 1996) + **voluntary charity** from the fair-minded (Fehr–Schmidt) — not a fiat transfer |
| Investment / capital formation | individual **saving** out of surplus, scaled by patience (time preference; Frederick 2002) |
| Fertility | an individual decision with a Becker **quantity–quality** opportunity cost, so the demographic transition emerges from human capital (Becker 1960; Galor & Weil 2000) |
| Policy change over time | **referenda** in which each person votes its measured self-interest; the mechanism only sets whose vote counts (Downs; Meltzer–Richard; Acemoglu–Robinson) |
| The welfare objective | an explicit **`Objective`** input — the evaluator's *values*, never a property of the world |

The remaining behavioural constants (in `constants.rs`, "Behavioural structure")
are *response forms* — how strongly a trait maps to a choice — not directions or
targets. Their **magnitudes are calibration knobs**, cited to the empirical
ranges they come from; the *direction* of every decision comes from the agent's
own psychology and measured situation.

## What this is and isn't

It is an **illustrative**, first-principles, agent-based model. Grounding the
primitives in real physics and biology makes the emergent *directions and
trade-offs* (overshoot, the commons tragedy, the equity/sustainability tension,
the value of a carbon price) trustworthy. It is **not** a forecast: no single
projected number is a prophecy, and the right use of the `search` and `compare`
tools is to study how outcomes *respond* to how a society is organised, not to
read a welfare score as gospel.
