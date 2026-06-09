//! The **assumptions registry**: every physical, biological and psychological
//! constant the simulator rests on, in one place, with its source. Nothing in
//! this file is a social outcome — that is the hard rule. If a number here is
//! wrong, fix it *here* and recalibrate; never patch an emergent output.
//!
//! Units: a tick is **1 year**; temperatures are Kelvin; the economic numéraire
//! is one **food unit** (≈ one person-year of calories), so "wealth 3.0" means
//! three person-years of food at market value.

/// Stefan–Boltzmann constant σ (W·m⁻²·K⁻⁴).
pub const STEFAN_BOLTZMANN: f64 = 5.670_374_419e-8;

/// Solar constant S₀ (W·m⁻²) — modern satellite value (Kopp & Lean 2011).
pub const SOLAR_CONSTANT: f64 = 1361.0;

/// Planetary albedo of open ocean / vegetated land / bare land / ice-snow.
/// (Budyko 1969; standard EBM surface albedos.)
pub const ALBEDO_OCEAN: f64 = 0.10;
pub const ALBEDO_LAND: f64 = 0.25;
pub const ALBEDO_ICE: f64 = 0.60;

/// Effective atmospheric emissivity giving a ~288 K modern global mean in the
/// zero-dimensional energy balance (one-layer greenhouse; Sellers 1969).
pub const EMISSIVITY: f64 = 0.612;

/// Meridional heat-transport coefficient (W·m⁻²·K⁻¹) of the diffusive
/// energy-balance model — the value range that reproduces the observed
/// equator-to-pole gradient (North 1975: D ≈ 0.55–0.66).
pub const HEAT_DIFFUSION: f64 = 0.6;

/// CO₂ radiative forcing: F = LAMBDA · ln(C/C₀) (Myhre et al. 1998).
pub const FORCING_LAMBDA: f64 = 5.35;
/// Pre-industrial CO₂ concentration C₀ (ppm) (IPCC AR6).
pub const CO2_PREINDUSTRIAL: f64 = 280.0;
/// Planck feedback parameter (W·m⁻²·K⁻¹): warming per unit forcing is
/// ΔT = F / PLANCK_FEEDBACK ⇒ ≈ 3.0 K per CO₂ doubling with feedbacks
/// (IPCC AR6 central equilibrium climate sensitivity).
pub const PLANCK_FEEDBACK: f64 = 1.23;
/// First-order CO₂ uptake per year toward C₀ (ocean+biosphere; the ~50-100 yr
/// dominant airborne-fraction decay mode of the Bern carbon-cycle model).
pub const CO2_DECAY: f64 = 0.012;
/// Surface temperature relaxation per year toward radiative equilibrium
/// (mixed-layer ocean thermal inertia, ~15-year e-folding).
pub const TEMP_RELAX: f64 = 0.065;
/// Polar amplification: high-latitude warming exceeds the tropics by roughly
/// 2–3× (IPCC AR6 ch.4). Anomaly pattern = 1 + AMP·(sin²lat − ⟨sin²lat⟩).
pub const POLAR_AMP: f64 = 1.6;

/// Net primary productivity, **Miami model** (Lieth 1975): empirical fits of
/// NPP (g dry matter m⁻² yr⁻¹) to temperature and precipitation:
/// NPP_T = 3000 / (1 + e^(1.315 − 0.119·T°C)), NPP_P = 3000·(1 − e^(−0.000664·P_mm)),
/// NPP = min(NPP_T, NPP_P).
pub const NPP_MAX: f64 = 3000.0;

/// Logistic regrowth rate of standing biomass toward its NPP-set capacity
/// (forest/grassland recovery timescales, decades: r ≈ 0.08/yr).
pub const BIOMASS_REGROWTH: f64 = 0.08;
/// Fishery intrinsic growth rate (Schaefer surplus-production, r ≈ 0.3/yr).
pub const FISH_REGROWTH: f64 = 0.3;
/// Soil fertility loss per unit of over-intensive cultivation, and its slow
/// natural recovery (soil formation is ~10× slower than erosion under
/// intensive use; Montgomery 2007).
pub const SOIL_DEGRADE: f64 = 0.02;
pub const SOIL_RECOVER: f64 = 0.002;
/// Biodiversity responds to habitat: it declines toward the intact-habitat
/// fraction (species–area relation, exponent z≈0.25; MacArthur & Wilson 1967)
/// and recovers an order of magnitude more slowly.
pub const BIODIV_DECLINE: f64 = 0.05;
pub const BIODIV_RECOVER: f64 = 0.005;

/// Human energy need: one adult-year of food defines the numéraire (≈ 0.9 M
/// kcal/yr; FAO). Children/elderly need less — scaled by `people::need_scale`.
pub const FOOD_NEED: f64 = 1.0;
/// Domestic water need relative to food in numéraire terms (drinking,
/// cooking, hygiene — small next to agricultural water, which is inside the
/// farming production function).
pub const WATER_NEED: f64 = 0.2;
/// Heating-fuel need per degree-year below the comfort temperature (K), in
/// numéraire units — zero in the tropics, material in high latitudes.
pub const FUEL_NEED_PER_K: f64 = 0.012;
pub const COMFORT_TEMP: f64 = 288.0;
/// Manufactured-goods need (clothing, shelter upkeep, tools) per adult-year.
pub const GOODS_NEED: f64 = 0.15;

/// Gompertz–Makeham mortality: hazard = MAKEHAM + GOMPERTZ_A·e^(GOMPERTZ_B·age).
/// Fit so untreated-world life expectancy lands in the documented pre-modern
/// 30–40 yr band with high infant mortality (Gompertz 1825; CDC life tables
/// for the shape).
pub const MAKEHAM: f64 = 0.006;
pub const GOMPERTZ_A: f64 = 0.00012;
pub const GOMPERTZ_B: f64 = 0.092;
/// Extra infant (age 0–4) hazard in the absence of adequate nutrition/care.
pub const INFANT_HAZARD: f64 = 0.05;
/// Mortality hazard per unit of unmet survival need (starvation/thirst/cold).
pub const DEPRIVATION_HAZARD: f64 = 0.55;
/// Heat-stress mortality threshold: sustained local temperature above this
/// adds hazard (wet-bulb survivability literature; Sherwood & Huber 2010).
pub const HEAT_STRESS_TEMP: f64 = 303.0;
pub const HEAT_STRESS_HAZARD: f64 = 0.04;

/// Female fertile window and a physiological ceiling on births per fertile
/// year (population-level: ~0.35 births per fertile woman-year ⇒ total
/// fertility ~8 at the biological maximum; Bongaarts 1978 proximate
/// determinants). The *realised* rate is an agent decision, never set.
pub const FERTILE_AGE: (u32, u32) = (15, 45);
pub const MAX_BIRTH_RATE: f64 = 0.35;

/// Learning-by-doing: sector productivity rises with the log of cumulative
/// output (Wright 1936; Arrow 1962). Per-doubling progress ratios of 10–25%
/// are documented across industries; clean energy sits at the high end
/// (Way et al. 2022).
pub const LEARNING_RATE: f64 = 0.12;
pub const LEARNING_RATE_CLEAN: f64 = 0.22;
/// Capital depreciation per year (standard macro 4–6%).
pub const DEPRECIATION: f64 = 0.05;
/// Output elasticity of capital (Cobb–Douglas α ≈ 0.3; Solow growth
/// accounting).
pub const CAPITAL_ELASTICITY: f64 = 0.3;

/// CO₂ emitted per numéraire unit of fossil fuel produced (scaling chosen so
/// an industrialised run moves CO₂ by hundreds of ppm over centuries — the
/// magnitude of the historical record).
pub const EMISSION_FACTOR_FOSSIL: f64 = 0.04;
/// CO₂ from converting high-biomass land to agriculture (land-use change,
/// ~10–15% of historical emissions; IPCC).
pub const EMISSION_FACTOR_LANDUSE: f64 = 0.01;
