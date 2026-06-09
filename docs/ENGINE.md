# ENGINE.md — the society-physics engine (first-principles, agent-based)

> **The rule that defines this engine:** macro quantities (GDP, Gini, life
> expectancy, prices, emissions, state capacity) are **never inputs**. You set
> only physical/biological *primitives*; everything social is **measured** as it
> emerges. To reproduce a real statistic you calibrate the *primitives* until the
> *measured output* matches — you **simulate _to_ the numbers, never from them.**

This supersedes the earlier aggregate system-dynamics model (still in the crate
under `dynamics`/`scenario`/`sim`, now demoted to a "macro twin" / sanity
oracle, and a source of calibration *targets* via `docs/RESEARCH.md`). The new
core lives under [`crate::engine`].

## Why agent-based, not aggregate

An aggregate model *starts at* a Gini of 0.39 and a GDP of 195 and nudges them
with fitted coefficients — but those averages are themselves products of how a
society produces, trades and is governed. Encoding them as inputs bakes in the
very thing we want to explain. The fix is the Sugarscape lineage (Epstein &
Axtell, *Growing Artificial Societies*, 1996): model heterogeneous agents on a
resource landscape under simple, first-principles rules and let the
distributions emerge. Inequality, carrying capacity, prices, money, the tragedy
of the commons, and institutions are then *outputs you observe*.

## Consensus architecture (from the design fan-out)

* **Hand-rolled struct-of-arrays, zero dependencies, fully deterministic.** Not
  an ECS framework — we want guaranteed iteration order, trivial
  snapshot (`World: Clone`), and a dependency-free build. One seeded
  `xoshiro256**` stream ([`engine::rng`]); same seed ⇒ bit-identical history.
* **Explicit, order-stable phased tick.** The phases are the "laws of physics":
  `substrate regrowth → perception → action/production → exchange → institutional
  enforcement → vital events`. Order-sensitive interactions use a
  *claim/resolve* split so results don't depend on who iterates first.
* **Read-only Instruments** ([`engine::instruments`]) compute every macro number
  from raw agent state. They take `&World` — by type they cannot mutate it — so
  a measured aggregate can never feed back as an input. This is the hard rule,
  enforced by the type system.
* **Calibration = inverse modelling.** Tune the primitives (the laws) so the
  emergent moments match empirical targets, via the **Method of Simulated
  Moments** (McFadden 1989; Grazzini & Richiardi 2015) or **Approximate Bayesian
  Computation** (Beaumont 2010) for posteriors over primitives. The targets live
  on the *right-hand side of a loss function only* — never assigned to the world.

## The layers (and their only primitives)

| Layer | Primitives (inputs — laws/biology/geography) | Emergent (measured outputs) |
|---|---|---|
| **Environment / substrate** | solar flux, albedo, emissivity, logistic `r`, capacity `K` field, diffusion, GHG radiative coeff, pollutant decay | biomass, **carrying capacity**, temperature, climate sensitivity, scarcity |
| **Agent** | metabolism (Kleiber), vision, learning rate, Gompertz–Makeham hazard, reproduction cost, mutation | population, **life expectancy**, birth/death rates, skill distribution, diminishing marginal utility |
| **Exchange** | goods (divisibility, durability, mass), recipes + per-agent productivity, search/transport range, bargaining rule | **prices** (realised ratios), **money** (Menger/Kiyotaki–Wright), **GDP**, specialization, **wealth Gini** |
| **Institutions** | reputation memory, interaction radius, sanctioning cost, collective-choice mechanism | cooperation rate, **property regimes**, **state capacity, legitimacy, corruption**, conflict |
| **Engine/measure** | RNG seed, grid size, tick scheduler | the time-series; the calibration loss |

Environment + Agent designs: physical conservation, Verhulst logistic,
Lotka–Volterra, zero-D energy balance (Budyko–Sellers), Kleiber's law, Simon's
bounded rationality, Gompertz–Makeham mortality, Arrow learning-by-doing.
Exchange design: Menger (origin of money), Kiyotaki–Wright, Hayek (prices as
information), Gode–Sunder (zero-intelligence traders), Ricardo (comparative
advantage). Institutions design: Hardin (tragedy of the commons), Ostrom
(governing commons), Axelrod (evolution of cooperation), Demsetz (property),
North (institutions), Olson (collective action), Tilly / Acemoglu–Robinson
(state formation).

## Phased roadmap (runnable at every step)

* **Phase 0 — engine skeleton.** ✅ Done. Deterministic RNG, SoA `World`,
  phased `step`, snapshot via `Clone`, instruments + recorder. Tests: determinism.
* **Phase 1 — substrate + metabolizing/harvesting agents.** ✅ Done
  ([`engine::world`]). Logistic landscape, Sugarscape movement, metabolism,
  Gompertz–Makeham death, reproduction. **Already demonstrates the thesis:**
  carrying capacity, life expectancy and a **wealth Gini all emerge from an
  equal start** and are measured, never set (see `engine::emergence_tests`).
* **Phase 2 — exchange.** ✅ Done ([`engine::world`] + [`engine::instruments`]).
  **Two renewable goods** on the landscape (each Gaussian mountain peaks in a
  *different* good → regional comparative advantage), agents hold a tradeable
  *bundle* plus an `energy` reserve refilled by **consuming** goods (so Phase-1
  metabolism/carrying-capacity still bites). Diminishing marginal utility is
  *derived* from a single need-satiation primitive `u(s)=s/(s+scale)`, giving each
  agent a marginal rate of substitution from its **own** holdings. A new
  **exchange phase** in `step` lets adjacent agents trade bilaterally at the
  geometric mean of their two MRSs (the Sugarscape rule), accepting a deal only
  if it **strictly raises both** agents' satisfaction (Gode & Sunder ZI traders).
  Realised trades are recorded in a per-tick ledger (`World::trades_this_tick`)
  and money emerges via per-good acceptance-as-payment (`World::medium_accept`,
  Menger / Kiyotaki–Wright). Instruments measure the **emergent price index**
  (volume-weighted median realised ratio), **trade volume**, **GDP as a flow**,
  per-agent **specialization** (Herfindahl of cumulative harvest), and the
  **dominant medium of exchange** — all read-only, never set. Emergent results:
  gains from trade over autarky, scarcity raising a good's price, heritable
  specialization. Tests: `trade_produces_gains_over_autarky`,
  `scarcity_raises_the_emergent_price`, `prices_money_gdp_specialization_emerge`,
  `exchange_is_deterministic`.
* **Phase 3 — institutions & policy-as-rules.** ✅ Done
  ([`engine::institutions`] + extensions to [`engine::world`]/[`engine::instruments`]).
  A composable [`engine::institutions::Rule`] trait runs in an **institutional
  phase** at the top of `World::step_with_rules`; rules mold *mechanisms and
  payoffs only* — never outcomes. Concrete rules: `OpenAccess`, `HarvestQuota`,
  `PropertyRights` (Demsetz), a progressive `WealthTax` into a `public_pool`,
  means-tested `Redistribute`, and a `CorruptOfficial` that skims the pool. A new
  **ecological fragility** primitive (`degrade_rate`/`recovery_rate`, exposed via
  `Primitives::fragile_commons`) lets a cell mined below its regeneration
  threshold lose capacity — the physical reason a commons *can* be destroyed
  (OFF in `demo()`, so Phase-1/2 physics and tests are unchanged). Compliance is
  voluntary and imperfect: it follows a **conditional-cooperation** response to
  an emergent, reinforcing **legitimacy** belief (Axelrod/Ostrom/Levi), and
  enforcement is **costly and imperfect**, funded from the pool (Olson). New
  read-only instruments measure `commons_health`, `compliance_rate`,
  `state_capacity` (achieved/intended enforcement, Tilly), `legitimacy`,
  `corruption` (diverted/total outflow) and `public_pool`; `instruments::run_under`
  is a same-seed A/B experiment helper. Emergent results: open access collapses a
  fragile commons while a quota or property regime sustains it and carries far
  more population; a tax+transfer lowers the measured Gini; corruption emergently
  lowers state capacity & legitimacy and degrades the commons. Tests:
  `open_access_collapses_commons_that_a_rule_sustains`,
  `redistribution_lowers_measured_gini`,
  `capacity_legitimacy_corruption_emerge_and_corruption_hurts`,
  `legitimacy_and_compliance_emerge_under_a_voluntary_rule`,
  `institutions_are_deterministic`.
* **Phase 4 — calibration & experiment harness.** ✅ Done
  ([`engine::calibration`]). The formal "simulate **to** the numbers". Empirical
  **targets** (a within-country wealth Gini ~0.39, a plausible life expectancy)
  are defined as `{name, extract: fn(&RunSummary)->f64, target, weight}` and live
  **only on the right-hand side of a loss** — never assigned to a world. The
  **Method of Simulated Moments** loss `L(θ)=Σ w_k (m_k(θ)−m̂_k)²` builds a world
  from a *primitive* vector `θ` (via `decode`, which writes only physical knobs:
  `peak_capacity`, `metabolism_max`, `senescence`, `vision_max`,
  `birth_threshold`, clamped to physical bounds), runs it over a **seed ensemble**
  to damp Monte-Carlo noise, and **measures** the emergent moments with the
  instruments. A dependency-free optimiser minimises it: a **Latin-Hypercube**
  global search followed by a hand-rolled **Nelder–Mead** simplex refinement
  (`calibrate` returns the fitted θ/primitives, the achieved loss and the
  starting loss). The **experiment harness** defines a `Scenario` (primitives +
  a Phase-3 `Rule` stack), runs it across seeds (`evaluate` → an `Outcome`
  distribution) and `compare`s two regimes on a MEASURED **welfare functional**:
  the geometric mean of prosperity × equity × sustainability × survival (a
  no-substitutes Sen/Stiglitz/HDI-style composite — collapsing any pillar,
  including a population crash, collapses the score, which is what stops a
  per-capita metric from "winning" by letting most agents die). Emergent results:
  calibration measurably *reduces* the loss vs a random/neutral start and pulls
  the emergent Gini toward 0.39 (CLI: loss 0.31→0.001, Gini→0.387); the harness
  ranks a sustaining quota above open access on a fragile commons, deterministically
  across the seed ensemble. `simctl calibrate` and `simctl experiment` run these
  from the CLI. Tests: `calibration_reduces_the_loss_versus_a_random_start`,
  `fitted_primitives_move_an_emergent_moment_toward_target`,
  `harness_ranks_a_sustainable_regime_above_open_access`,
  `calibration_and_harness_are_deterministic`,
  `welfare_is_a_no_substitutes_composite`. (McFadden 1989; Grazzini & Richiardi
  2015; Beaumont 2010 for ABC; Nelder–Mead 1965; McKay–Beckman–Conover 1979.)
* **Phase 5 — spatial energy-balance climate coupled to production.** ✅ Done
  ([`engine::world`] climate methods + [`engine::institutions::Decarbonize`] +
  [`engine::instruments`] climate fields). Climate damage is made to **emerge**
  from physics rather than be a fitted curve. **Emissions are a flow** that
  emerges from activity: each unit harvested releases `emission_factor` of
  greenhouse gas (combustion/land-use proportional to throughput). The gas
  accumulates in a global **greenhouse stock** `C_atm` with first-order decay
  toward a pre-industrial reference `C₀` (`dC = emissions − co2_decay·(C−C₀)`).
  **Temperature** follows the zero-dimensional **energy balance** (Budyko 1969;
  Sellers 1969) `heat_cap·dT/dt = (1−albedo)·S/4 − ε·σ·T⁴ + F`, with CO₂-style
  **Myhre 1998 log forcing** `F = λ·ln(C_atm/C₀)`, so T relaxes toward radiative
  equilibrium with the planet's thermal inertia. The **feedback is mechanistic**:
  a unimodal `temp_response(T)` (a Gaussian peaked at the productivity optimum,
  Lindeman 1942 energetics) scales the **logistic regrowth rate** (Verhulst), so
  warming above the optimum lowers net primary productivity → carrying capacity →
  population/wealth. *No damage coefficient touches any macro output* — the damage
  is the ecological consequence. The whole subsystem is **OPT-IN**: `demo()` keeps
  `climate_enabled = false` (the code path is skipped, so all earlier tests are
  byte-identical), and `Primitives::warming_world()` switches the coupling on at a
  self-consistent pre-industrial steady state (`temp_opt` = the zero-forcing
  equilibrium temperature, so even enabling climate with zero emissions is a
  no-op). New read-only instruments: `temperature`, `greenhouse_stock`,
  `emissions` (flow) and an emergent `climate_sensitivity` (ΔT for a doubling of
  `C_atm`, the Planck response read off this world's own physics). A Phase-3-style
  `Decarbonize` rule abates the carbon intensity of output (`emission_scale`),
  lowering the *emergent* temperature. The temperature/greenhouse state lives on
  `World` so the next (collective-choice) phase can let agents perceive and react
  to it. Emergent results: more production→more emissions→higher `C_atm`→higher T;
  warming lowers the carrying capacity (population) vs a clean same-seed control
  (emergent climate damage); a decarbonising rule lowers emergent T and carries
  more people. Tests: `default_world_is_a_climate_no_op`,
  `emissions_raise_the_greenhouse_stock_and_temperature`,
  `warming_lowers_carrying_capacity_versus_a_clean_control`,
  `a_decarbonising_rule_lowers_emergent_temperature`, `climate_is_deterministic`,
  `climate_instruments_report_measured_values`. (Budyko 1969; Sellers 1969; Myhre
  et al. 1998; Verhulst; Lindeman 1942; Pigou 1920 for the abatement mandate.)
* **Phase 6 — emergent collective choice.** ✅ Done ([`engine::polity`]). The
  active rule set is no longer hand-picked by the experimenter: it **emerges** from
  agent preferences. Each agent has a **policy preference** read purely off its own
  *measured* situation by the read-only `agent_support(&World, i, &WealthRanking)`
  — its wealth relative to the population mean (`WealthRanking`, the same wealth the
  Gini measures), the local resource scarcity at its cell, and (under a warming
  world) its exposure to temperature above the productivity optimum. *No party
  label or assumed ideology is an input.* A `Polity` then aggregates those
  preferences into the rules in force each electoral term via one of two
  **structural mechanisms** — `ChoiceMechanism::Majority` (one-person-one-vote /
  median-voter, Downs 1957) or `ChoiceMechanism::WealthWeighted` (votes weighted by
  wealth: a plutocracy / elite-capture rule, Acemoglu & Robinson 2006) — selecting
  from the Phase-3/5 [`Rule`] catalogue: a redistribution bundle
  (`WealthTax`+`Redistribute`), a conservation `HarvestQuota`, `PropertyRights`, or
  `Decarbonize`. Enactment needs a support **threshold** (the collective-action
  cost of organising to change the rules, Olson 1965). The `govern(&mut World,
  &mut Polity, ticks, observer)` driver holds an election every `period` ticks and
  applies the elected set through the existing `step_with_rules` machinery for the
  rest of the term; the `observer(tick, &Polity)` hook records the **active-rule
  timeline** for visualisation. New read-only instruments on `Polity`:
  `active_policies`, `is_active`, `vote_share` (per option), `turnover` (policy
  churn across terms) and `elections` — all MEASURED out of the population.
  Emergent results: a high-inequality population under majority rule **adopts
  redistribution and the measured Gini falls** (right-skewed wealth ⇒ a below-mean
  majority, Meltzer–Richard 1981) — the policy emerged from preferences, was never
  set; the **same** population elects a **different** rule set under wealth-weighting
  (the rich block the tax and entrench property rights); and on the climate preset
  agents **adopt decarbonisation once warming bites** (an election at the cold
  steady state enacts none). Tests: `majority_adopts_redistribution_and_gini_falls`,
  `wealth_weighting_selects_a_different_rule_set_than_majority`,
  `warming_makes_agents_adopt_decarbonization`,
  `govern_runs_terms_and_records_a_timeline`, `collective_choice_is_deterministic`
  (+ `active_rules_always_reset_harvest_mechanism`, `percentiles_span_the_population`).
  (Downs 1957; Olson 1965; Acemoglu & Robinson 2006; Meltzer & Richard 1981.)
* **Phase 7 — visualisation & trace.** ✅ Done ([`engine::trace`]). Makes the
  emergence **visible**, dependency-free. A `Trace` recorder runs a `World` forward
  and records a full read-only `instruments::measure` snapshot at the initial state
  and after each step (`record(&mut World, &[Rule], ticks)` ⇒ `ticks + 1` frames);
  `Trace::to_csv()` emits a **stable-header** CSV (`TRACE_CSV_HEADER`, one row per
  tick) covering the headline emergent metrics across *all* phases — population,
  wealth Gini, mean wealth, life expectancy, the emergent price index, GDP flow,
  production, specialization, commons health, temperature / greenhouse stock /
  emissions / climate sensitivity, and state capacity / legitimacy / corruption /
  public pool. An ASCII renderer draws a shaded **resource heatmap**
  (`render_resource_heatmap`), an **agent-density map** (`render_agent_density`) and
  **sparklines** of the headline series (`render_trace_sparklines`), combined by
  `render_run(&World, &Trace)` — so the tragedy of the commons (commons-health and
  population fall while the Gini rises) is plainly visible in a plain terminal. Two
  new `simctl` subcommands wire it up: `simctl trace [--preset NAME] [--ticks N]
  [--seed S] [--csv PATH]` and `simctl render [--preset NAME] [--ticks N]
  [--seed S]` (presets: `demo`, `fragile-commons`, `warming-world`). Everything is a
  strictly read-only consumer of the instruments — no world mutation beyond the
  engine's own `step` — and deterministic: the same seed yields a byte-identical
  CSV. Tests: `csv_has_stable_header_row_count_and_is_deterministic`,
  `csv_covers_climate_columns_when_climate_is_on`,
  `renderer_produces_non_empty_output`, `sparkline_handles_flat_and_nonfinite_series`,
  `render_is_deterministic_and_read_only`.
* **Phase 8 — deterministic parallel scaling.** ✅ Done ([`engine::parallel`]).
  The engine now scales toward 10⁵–10⁶ agents while staying **dependency-free**
  and **bit-deterministic** — parallelism is std-only (`std::thread::scope`, no
  new crate) and *never changes results*. Only **order-independent, RNG-free**
  phases are parallelised: per-cell **substrate regrowth** (each cell's logistic
  update depends solely on its own prior `resource`/`capacity`/`capacity0`) and
  the per-agent **wealth valuation** inside `measure` (each agent's wealth is a
  pure read-only function of its own state). Both are partitioned into
  contiguous, disjoint index ranges with `parallel::for_each_chunk_mut`, so the
  per-element float arithmetic is identical to the sequential loop for **any**
  thread count — no cross-thread reduction, no float atomics, nothing whose
  result can depend on interleaving. Every **order-dependent** phase (movement,
  bilateral trade, reproduction, enforcement, and the RNG-consuming
  vital-events loop) stays strictly **sequential** by design; the agents'
  `occupant` array already serves as an O(1) spatial index, so neighbour queries
  are near-linear without a separate bucket structure. The single-threaded path
  is the canonical golden oracle, and `parallel == sequential` is asserted
  bit-for-bit. The worker cap (`engine::set_max_threads` / `max_threads`,
  defaulting to the machine's cores) is a pure performance knob; a
  `PARALLEL_THRESHOLD` keeps small worlds (every prior test) on the sequential
  path byte-for-byte. `Primitives::large_world(cells, n_agents)` builds a
  continental-scale landscape, and `simctl bench` runs ~10⁵ agents for a few
  ticks and reports ticks/sec and agent-ticks/sec (≈3× speedup on
  regrowth-dominated worlds; agent-dominated runs are bounded by the sequential
  phases, as the determinism rule requires). **Emergent property:** the very same
  emergent statistics (inequality from an equal start, carrying capacity) hold at
  six-figure scale, and the parallel path reproduces them to the bit. Tests:
  `parallel::chunked_matches_whole_slice_for_any_thread_count`,
  `parallel::small_slices_take_the_sequential_path`,
  `parallel_matches_sequential_bit_for_bit`, `parallel_runs_are_deterministic`,
  `large_population_runs_without_panic`, `thread_cap_does_not_change_results`.

* **Phase 9 — human psychology as a driving factor.** ✅ Done (extensions to
  [`engine::world`] + [`engine::polity`] + [`engine::instruments`]). Psychology
  enters the engine exactly the way biology does: as heterogeneous, **heritable
  per-agent primitives** drawn from configurable ranges — **patience** (time
  preference, Frederick/Loewenstein/O'Donoghue 2002), **risk aversion**
  (Pratt 1964; Arrow 1965), **fairness / inequity aversion** (Fehr & Schmidt
  1999) and **conformity / norm sensitivity** (Cialdini & Goldstein 2004;
  Henrich 2015) — which drive *behaviour mechanisms only*: a patient agent
  self-limits its harvest even with no rule in force (discounting is the
  psychological root of the tragedy of the commons, foresight its
  non-institutional resolution); compliance willingness blends the
  institution's legitimacy with the agent's own patience, weighted by its
  conformity; risk-averse agents hold a larger precautionary buffer before
  reproducing; and fair-minded above-mean agents support a redistributive
  floor (the Fehr–Schmidt β), so a fair-minded culture redistributes **even
  under wealth-weighted voting**. A per-agent **subjective well-being** ledger
  (a slow EMA of realised need satisfaction; Diener 1984, Kahneman & Krueger
  2006) is measured by `instruments::mean_wellbeing` and never feeds back. The
  whole coupling is **OPT-IN** (`psyche_enabled`, OFF in `demo()`; preset
  `Primitives::human_nature()`): disabled, every trait is an inert neutral, no
  extra RNG is consumed, and the engine is byte-identical to before. Emergent
  results: a patient culture sustains a fragile commons *without any law*;
  conformists comply where mavericks defy; risk aversion slows reproduction;
  fairness broadens the redistribution coalition across mechanisms. Tests:
  `psychology_is_off_and_neutral_by_default`,
  `patient_culture_sustains_a_commons_without_any_law`,
  `risk_aversion_slows_reproduction`,
  `conformists_comply_where_nonconformists_defy`,
  `fairness_broadens_the_redistribution_coalition`,
  `wellbeing_is_measured_never_set`, `psychology_is_deterministic`.
* **Phase 10 — society-as-input.** ✅ Done ([`engine::society`]). The
  configurable front door the project's vision calls for: describe an existing
  (or imagined) society in a plain-text **`.soc` spec** — its
  physical/biological/psychological *primitives*, its **stack of laws and
  institutions** (`[laws]`: `wealth-tax`, `redistribute`, `harvest-quota`,
  `property-rights`, `decarbonize`, `corrupt-official`), and optionally how it
  governs itself (`[governance]`: `majority` / `wealth-weighted`, period,
  threshold) — and load it with `SocietySpec::parse`. Parsing is **strict**
  (unknown keys/laws/sections are errors with line numbers) and the hard rule
  holds at the input layer: *there is deliberately no key for any macro
  outcome* (`no_outcome_key_exists` pins this) — to make a spec match a real
  country you calibrate its primitives until the measured moments agree.
  Five **archetype presets** ship embedded in the binary (`societies/*.soc`):
  `open-frontier` (Hardin's ungoverned baseline), `stewardship-commons`
  (Ostrom), `egalitarian-green` (a redistributive green state),
  `laissez-faire` (Demsetz property and nothing else) and
  `extractive-autocracy` (Acemoglu–Robinson extraction) — each runs to a
  distinct, archetype-true emergent profile, none of it set. CLI:
  `simctl society [--file PATH | --preset NAME] [--ticks N] [--seeds A,B,C]`.
  Tests: `parses_a_full_spec`, `minimal_spec_and_defaults`,
  `strict_errors_carry_line_numbers`, `no_outcome_key_exists`,
  `all_bundled_presets_parse_and_run`, `law_parse_and_describe_round_trip`.
* **Phase 11 — mass do/undo counterfactuals.** ✅ Done
  ([`engine::counterfactual`]). The project's stated purpose made executable:
  *input a society, then mass-do or undo laws and structures and see how the
  world would work*. An [`Edit`] enacts (`Do(law)` — replacing a same-named
  law) or repeals (`Undo(name)` — strict: repealing an absent law is an error)
  one law; `whatif(spec, edits, seeds, ticks)` evaluates the society **with
  and without the edits on the same seed ensemble** (geography, biology,
  psychology and luck identical — the laws are the only difference, so every
  emergent delta is attributable to them) and returns both outcome
  distributions plus the welfare verdict; `sweep(spec, seeds, ticks)`
  enumerates **every subset of the law stack** (2ⁿ regimes, capped at 12 laws)
  and ranks them by the measured welfare functional, best first. A
  counterfactual cannot "decide" its outcome any more than a normal run can —
  the sweep immediately proved its worth in development by showing that a 0.3
  per-tick wealth tax collapses every regime that contains it (per-tick taxes
  on the wealth *stock* must be a few percent, as in reality). CLI:
  `simctl whatif [--file|--preset] [--do LAW[=VAL]]... [--undo LAW]...
  [--sweep] [--top N] [--ticks N] [--seeds A,B,C]`. Tests:
  `edits_do_undo_and_replace`,
  `undoing_conservation_makes_a_fragile_world_worse`,
  `doing_a_conservation_law_improves_the_frontier`,
  `sweep_enumerates_and_ranks_every_law_subset`,
  `sweep_guards_against_combinatorial_explosion`.

## Testing emergence (TDD)

Assert *emergent properties and regimes*, not back-fitted magnitudes:
seed-stable **bands** ("Gini emerges > 0 from an equal start"), **distributional**
tests (heavy-tailed wealth), and **regime comparisons** under a shared seed
(open-access commons collapses; regrowth sustains population; a policy rule
shifts an outcome). Determinism makes these stable rather than flaky.

## Status

Phases 0–**11** are implemented and green: the emergent slice spans
environment → agents → exchange → institutions → calibration → climate →
collective choice → visualisation → scale → **psychology** (Phase 9), and on
top of it the two layers the project's vision asks for — **society-as-input**
(Phase 10: a `.soc` spec composes primitives + the law/institution stack +
governance, never an outcome) and **mass do/undo counterfactuals** (Phase 11:
`whatif` and `sweep` re-run the same seeds under edited law stacks and rank
regimes on measured welfare).

Run a `World` forward
(optionally under a stack of `Rule`s via `step_with_rules`) and read
`engine::instruments::measure` — population, life expectancy, inequality, prices,
money, GDP, specialization, commons health, compliance, state capacity,
legitimacy, corruption, and the **climate** (temperature, greenhouse stock,
emissions flow, climate sensitivity) — all computed from raw agent/substrate
state, with no socioeconomic or climatic-as-outcome number ever supplied as an
input. `engine::calibration` then tunes the *primitives* (Method
of Simulated Moments) until those *measured* moments match empirical targets
(targets used only inside the loss), and ranks ways of organising society on a
measured welfare functional across a seed ensemble. **The hard rule holds
end-to-end: we simulate _to_ the numbers, never from them.**

**Public interface the psychology / society / counterfactual phases left
(Phases 9–11):**
- Psychology primitives on [`Primitives`]: `psyche_enabled` plus the
  `patience/risk_aversion/fairness/conformity` `_min`/`_max` ranges;
  `Primitives::human_nature()` switches the coupling on. Per-agent traits live
  on `Agents` (`patience`, `risk_aversion`, `fairness`, `conformity`,
  `wellbeing`); `instruments::mean_wellbeing(&World)` is the measured
  subjective well-being. Add a new behavioural channel by gating it on
  `psyche_enabled` (the disabled path must stay byte-identical and consume no
  RNG).
- `engine::society::{SocietySpec, Law, Governance, presets, LAW_NAMES}` — the
  `.soc` parser and the embedded archetypes. `SocietySpec::{parse, preset,
  rules, scenario}`; a `Law` knows its `name`, `describe` rendering and the
  concrete `rule()` mechanism it enacts. Extend the law catalogue by adding a
  `Law` variant + `Law::parse` arm (every law must map to a Phase-3/5
  mechanism, never an outcome).
- `engine::counterfactual::{Edit, apply_edits, whatif, sweep, WhatIf,
  SweepEntry, SWEEP_MAX_LAWS}` — the mass do/undo harness, built on
  `calibration::{evaluate, welfare}` so every verdict is a measured-welfare
  comparison over a shared seed ensemble.
- CLI: `simctl society` and `simctl whatif` (see `simctl help`); `simctl list`
  names the archetypes and the law catalogue.

**Public interface the collective-choice phase left (Phase 6):**
- `engine::polity::{Polity, ChoiceMechanism, PolicyOption, WealthRanking,
  agent_support, govern}`. A `Polity::new(mechanism, period)` (builder
  `.with_threshold`) holds elections; `govern(&mut World, &mut Polity, ticks,
  observer)` runs full electoral terms and calls `observer(tick, &Polity)` after
  each step — the **active-rule timeline** hook visualisation records.
- Read-only emergent measurements on `Polity`: `active_policies()`,
  `is_active(option)`, `vote_share(option)`, `turnover()`, `elections()`, plus
  `active_rules()` (the concrete `Rule` stack the elected options expand to).
- `agent_support(&World, i, &WealthRanking)` is the preference extractor (read-only
  over `&World`); `WealthRanking::new(&World)` exposes per-agent `wealth`,
  `percentile` and the population `mean` — extend either to add new preference
  channels or new `PolicyOption`s without touching the hard rule.

**Public interface the scaling phase left (Phase 8):**
- `engine::parallel::for_each_chunk_mut(&mut [T], f)` — the deterministic
  data-parallel primitive. Route a *new* order-independent, RNG-free per-element
  phase through it (closure receives the chunk's absolute start index so it can
  address parallel companion arrays); the result is guaranteed bit-identical to
  the sequential loop for any thread count. **Do not** route an order-dependent
  or RNG-consuming phase through it.
- `engine::{set_max_threads(n), max_threads()}` — the worker-thread cap
  (performance only; `set_max_threads(1)` forces the canonical sequential path,
  which is what the determinism oracle and the `parallel == sequential` tests
  pin against). The cap never changes what the engine computes.
- `Primitives::large_world(cells, n_agents)` — a continental-scale landscape for
  six-figure runs; `simctl bench [--agents N] [--cells N] [--ticks N] [--seed S]
  [--threads N]` reports ticks/sec and agent-ticks/sec.

**Public interface the climate phase left (Phase 5):**
- Climate state lives on `World`: `temperature`, `c_atm` (greenhouse stock),
  `emissions_this_tick`, plus the per-tick `emission_scale` lever. Read-only
  accessors `World::{temperature, greenhouse_stock, emissions_flow,
  climate_sensitivity}` and the matching `Measurements` fields are the emergent
  climate observables — agents in the next phase can perceive `temperature` /
  `greenhouse_stock` and vote/act on them.
- `Primitives::warming_world()` turns the coupling on; the climate primitives
  (`emission_factor`, `c_preindustrial`, `c_atm0`, `co2_decay`, `forcing_lambda`,
  `heat_capacity`, `albedo`, `solar_const`, `emissivity`, `temp_opt`,
  `temp_tolerance`, `climate_enabled`) are the only inputs. `equilibrium_temperature`
  and `PREINDUSTRIAL_C`/`STEFAN_BOLTZMANN` are exposed for setting up steady states.
- `institutions::Decarbonize { abatement }` is the template for emission-side
  policies: it molds only `emission_scale`; the temperature path stays measured.

**Public interface a Phase 6 (scale / further inference) can build on:**
- `engine::calibration::{calibrate, loss, loss_at, run_summary, ensemble_summary,
  decode, dim, knob_names}` — the MSM pipeline. `Target {name, extract, target,
  weight}` + `default_targets()` are the empirical moments (RHS of the loss only);
  `decode(base, θ)` proves world construction is primitive-only. Swap the optimiser
  inner loop for ABC by accepting θ whose `loss_at` is below a tolerance.
- `engine::calibration::{Scenario, evaluate, compare, welfare, Outcome, Verdict}`
  — the experiment harness: a `Scenario` is `Primitives` + a `Rule` stack;
  `evaluate` returns the per-seed `Outcome` distribution; `compare` ranks two
  regimes on the measured `welfare` functional. `Outcome::mean(extractor)` reads
  any emergent moment back out of the ensemble.
- `simctl calibrate` / `simctl experiment` — CLI front-ends for both.

**Public interface inherited from Phase 3:**
- `World::step_with_rules(&[Box<dyn Rule>])` — the institutional phase; `step()`
  is the no-rules case. The `Rule` trait (`name`, `enforce(&mut World, &mut Rng)`)
  is the extension point for new policies, and rules stack order-stably.
- `instruments::run_under(primitives, &rules, ticks) -> Measurements` — the
  canonical same-seed A/B experiment helper (run with vs without a rule, compare
  the emergent moments). Pair it with multi-seed loops to rank institutions.
- `Primitives::fragile_commons()` (and the `degrade_rate`/`recovery_rate`/
  `regen_threshold` primitives) for over-exploitable substrates; `trade_enabled`
  toggles autarky.
- Read-only observers a policy can be *scored* against without mutating the world:
  `measure`, `price_index`, `total_welfare`, `mean_specialization`,
  `commons_health`, `compliance_rate`, and `World::{state_capacity, legitimacy,
  corruption, public_pool}`. All emergent — the right-hand side of a calibration
  loss, never an input.

## Visualizing a run

Phase 7 makes a run *visible* with no plotting dependency — a read-only consumer of
the instruments. Two `simctl` subcommands drive it (presets: `demo`,
`fragile-commons`, `warming-world`):

```text
# Record the per-tick emergent series to a CSV (stable header, ticks+1 rows):
simctl trace --preset fragile-commons --ticks 300 --seed 1 --csv run.csv
simctl trace --preset warming-world --ticks 200 --seed 7        # CSV to stdout

# Draw the run in ASCII: resource heatmap + agent-density map + sparklines:
simctl render --preset fragile-commons --ticks 200 --seed 1
```

The CSV header is `TRACE_CSV_HEADER` — `tick,population,gini,mean_wealth,
life_expectancy,price_index,gdp_flow,production,specialization,commons_health,
temperature,greenhouse_stock,emissions,climate_sensitivity,state_capacity,
legitimacy,corruption,public_pool` — covering the headline emergent metrics of every
phase; undefined values (e.g. the price before the first trade) are blank fields.
The same seed yields a **byte-identical** CSV. On a fragile commons the sparklines
show the tragedy directly: `commons_health` and `population` fall while `gini`
climbs.

Library entry points (all read-only): `engine::trace::{record, Trace, to_csv,
TRACE_CSV_HEADER, render_resource_heatmap, render_agent_density,
render_trace_sparklines, render_sparkline, render_run}`. `Trace::series(|m| ...)`
projects any `Measurements` field to a `Vec<f64>` column for custom plots. The
recorder calls only the engine's own `step` / `step_with_rules`, so it never
mutates a world beyond stepping it and never sets a macro quantity.
