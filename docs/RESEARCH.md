# RESEARCH.md — sourced real-world figures (calibration targets)

> In the first-principles engine these numbers are **not inputs**. They are
> **targets**: the right-hand side of the calibration loss. The simulator tunes
> physical/biological *primitives* until the *emergent, measured* aggregates land
> on these values (see `docs/ENGINE.md` and `engine::calibration`). You simulate
> **to** these numbers, never from them.
>
> Every figure cites a real, authoritative source. Where the literature gives a
> range, the central estimate *and* the range are recorded. These are
> illustrative calibration anchors, not a claim of precision.

## Demographics & population

| Quantity | Value (range) | Source |
|---|---|---|
| World population (2024) | 8.2 billion | UN World Population Prospects 2024 |
| Projected peak | ~10.3 bn around 2084, then slow decline | UN WPP 2024 (medium variant) |
| Crude birth rate (2023) | 16.3 per 1000/yr | World Bank SP.DYN.CBRT.IN |
| Crude death rate (2023) | 7.6 per 1000/yr | World Bank SP.DYN.CDRT.IN |
| Population growth (2024) | ~0.96 %/yr | World Bank SP.POP.GROW |
| Total fertility rate | 2.2–2.3 (replacement 2.1) | UN World Fertility 2024 |
| Life expectancy at birth (2024) | 73.3 years | UN WPP 2024 |
| Under-5 mortality (2023) | 37 per 1000 live births | UN IGME 2024 |

Education ⟶ fertility: strong negative, nonlinear (demographic transition), no
single elasticity — Cleland; Doepke et al. 2022; Galor (NBER w17057).
Preston curve: life expectancy ≈ `a + b·ln(GDP per capita)`, concave; the log form
explains ~70% of cross-country variance — Preston 1975.

## Economy (Solow / DICE-style anchors)

| Quantity | Value (range) | Source |
|---|---|---|
| World GDP (2024), nominal | $110.1 trillion | IMF WEO Oct 2024 |
| World GDP (2024), PPP | $194.6 trillion int-$ | IMF WEO Oct 2024 |
| Gross capital formation | ~26 % of GDP | World Bank WDI NE.GDI.TOTL.ZS (2023) |
| Production capital/output K/Y | ~2.8 (2.5–3.5) | Piketty & Zucman 2014 (≠ total-wealth β≈5–6) |
| Capital share α | 0.30 | Nordhaus 2017 DICE-2016R |
| Depreciation δ | 0.10 (DICE); 0.035–0.05 (PWT aggregate) | Nordhaus 2017; Penn World Table 10 |
| Long-run TFP growth | ~1.0 %/yr (1.0–2.0) | World Bank Global Productivity 2021; Gordon 2016 |
| Within-country Gini (median) | **0.39** (0.25–0.65) | World Bank 2024; WID 2022 |
| Global Gini (between+within) | ~0.62 | World Inequality Database 2022 |
| Global unemployment (2024) | 4.9 % | ILO WESO May 2024 |
| Govt total revenue | ~30 % of GDP | IMF Fiscal Monitor 2024 |
| Tax revenue (cap by capacity) | ~17.5 % (LIC 10–15 → HIC 35–45) | IMF; World Bank 15% threshold 2024; OECD 2025 |
| Public debt (2024) | ~92 % of GDP | IMF Fiscal Monitor Oct 2024 |
| Okun coefficient | output gap ≈ 2×Δunemployment (inverse β≈−0.45) | Ball, Leigh & Loungani 2017 |

## Climate & carbon

| Quantity | Value (range) | Source |
|---|---|---|
| Equilibrium climate sensitivity | 3.0 °C/CO₂ doubling (likely 2.5–4.0) | IPCC AR6 WG1 SPM A.4.4 |
| Pre-industrial CO₂ (1750) | 278.3 ppm | IPCC AR6 WG1 |
| Current CO₂ (2024) | 422.5 ppm | Global Carbon Budget 2024 |
| Temp anomaly (2011–2020) | 1.09 °C vs 1850–1900 (0.95–1.20) | IPCC AR6 WG1 SPM A.1.2 |
| CO₂ radiative forcing | ΔF = 5.35·ln(C/C₀) W/m² (AR6 ERF₂ₓ = 3.93) | Myhre et al. 1998; IPCC AR6 |
| Mass per ppm | 7.81 Gt CO₂ = 2.13 GtC | NOAA / IPCC |
| Airborne fraction | ~0.46 (0.44–0.48) | Global Carbon Budget 2024 |
| Current CO₂ emissions (2024) | 41.6 Gt CO₂/yr (fossil 37.4 + LUC 4.2) | Global Carbon Budget 2024 |
| DICE damage coefficient | Ω = 1/(1+ψ₂T²), ψ₂ = 0.00236 ⇒ 2.1% at 3 °C | Nordhaus 2017 PNAS |

Higher empirical damage estimates (exposed as alternatives): Burke, Hsiang &
Miguel 2015 (~23% income loss by 2100); Howard & Sterner 2017 (~7–10% at 3 °C).

## Ecology, land & resources

| Quantity | Value (range) | Source |
|---|---|---|
| Living Planet Index decline 1970–2020 | −73 % (−67 to −78) | WWF/ZSL Living Planet Report 2024 |
| Biodiversity Intactness Index (current) | ~79 % (safe boundary 90 %) | Stockholm Resilience; Newbold et al. 2016 |
| Species threatened | ~1,000,000 (IPBES); >47,000 assessed (IUCN) | IPBES 2019; IUCN Red List 2024-2 |
| Forest area (2020) | 4.06 bn ha = 31 % of land | FAO FRA 2020 |
| Net forest loss (2010–20) | 4.7 Mha/yr | FAO FRA 2020 |
| Planetary boundaries transgressed | 6 of 9 (2023); 7 of 9 (2025) | Richardson et al. 2023 Sci. Adv. |
| Material extraction | 106 bn tonnes/yr (1970: 30) | UNEP IRP Global Resources Outlook 2024 |
| Species–area exponent z | 0.25 (0.20–0.35) | Arrhenius 1921; Rosenzweig 1995 |
| Overfished stocks (2021) | 37.7 % | FAO SOFIA 2024 |

## Governance & politics

| Quantity / finding | Value (range) | Source |
|---|---|---|
| WGI dimensions (6) | z-scores −2.5..+2.5, mean 0 | World Bank WGI |
| Corruption Perceptions Index global avg (2024) | 43/100 (>⅔ of countries <50) | Transparency International CPI 2024 |
| V-Dem Liberal Democracy Index | declining to ~1996 levels; ~72% in autocracies | V-Dem 2024/25 |
| State capacity ⟶ GDP/capita | +6–7 % per +1 SD | Vu 2025 OBES |
| Public-investment efficiency loss | 34 % avg (clean ~10–15% → LIDC ~53%) | IMF PFM 2023 |
| Growth ⟶ incumbent vote | +1 pp growth ⟶ ~+1 pp vote | NBER w21899; econ-voting lit |
| Unemployment ⟶ incumbent vote | +1 pp ⟶ −0.23 to −0.36 pp | econ-voting lit (IFAU 2025) |
| Trust in government (OECD 2024) | 39% trust / 44% distrust | OECD Trust survey 2024 |

State-capacity dimensions (extractive / coercive / administrative): Hanson &
Sigman 2021. Implementation effectiveness mapping used by the engine:
`efficiency_loss` ≈ 10% (clean, high-capacity) → 53% (corrupt, low-capacity) —
IMF PFM 2023.

## Foundational methods the engine builds on

- **Agent-based emergence:** Epstein & Axtell 1996 (Sugarscape).
- **Ecology:** Verhulst logistic; Lotka–Volterra; Lindeman trophic efficiency.
- **Climate physics:** Budyko 1969 & Sellers 1969 (energy balance); Myhre 1998 (log forcing).
- **Biology / behaviour:** Kleiber's law; Simon (bounded rationality); Gompertz–Makeham mortality; Arrow (learning-by-doing).
- **Exchange:** Menger (origin of money); Kiyotaki–Wright; Hayek (prices as information); Gode & Sunder (zero-intelligence traders); Ricardo (comparative advantage).
- **Institutions:** Hardin (tragedy of the commons); Ostrom (governing the commons); Demsetz (property); Axelrod (cooperation); Olson (collective action); Tilly / Acemoglu & Robinson (state formation); Downs (median voter); Meltzer–Richard (redistribution demand).
- **Calibration:** McFadden 1989 (Method of Simulated Moments); Grazzini & Richiardi 2015; Beaumont 2010 (ABC).

## Caveats

1. **Direction, not prophecy.** Calibrated primitives make trade-offs credible; long projections are scenario explorations.
2. **These are targets, not initial conditions.** World construction is from primitives only.
3. **Damage functions and governance effect sizes are deeply uncertain** — exposed as tunable targets/parameters, not pinned.
