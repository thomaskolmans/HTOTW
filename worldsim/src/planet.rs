//! The **planet**: a spherical lat–lon grid carrying geography (fractal
//! continents, mountains, coasts), a diffusive **energy-balance climate** with
//! latitudinal insolation, ice–albedo feedback and greenhouse forcing
//! (Budyko 1969; Sellers 1969; North 1975; Myhre 1998), a simple hydrological
//! cycle (zonal precipitation belts, continentality, orographic enhancement,
//! Clausius–Clapeyron scaling), **ecosystems** (Miami-model net primary
//! productivity → biomass and fishery stocks, soils, a species–area
//! biodiversity index), and finite, geographically clustered **fossil and
//! mineral deposits**.
//!
//! Everything here is physics/ecology: the planet neither knows nor sets any
//! social quantity. The economy reads its stocks and writes back only physical
//! pressures (harvest, land use, emissions); the climate trajectory that
//! results is measured, never scripted.

use crate::config::WorldConfig;
use crate::constants::*;
use crate::rng::Rng;

/// Pre-industrial global-mean precipitation scale (mm/yr) the zonal pattern is
/// normalised to (observed ≈ 1000 mm/yr).
const PRECIP_MEAN: f64 = 1000.0;
/// Atmospheric lapse rate (K per km of elevation).
const LAPSE_RATE: f64 = 6.5;
/// Standing forest biomass ≈ this many years of NPP (stock/flow ratio of
/// mature ecosystems).
const BIOMASS_STOCK_YEARS: f64 = 25.0;

#[derive(Debug, Clone)]
pub struct Planet {
    pub nlon: usize,
    pub nlat: usize,
    /// Latitude (radians) of each cell's row centre.
    pub lat: Vec<f64>,
    /// Normalised cell area weights (cos-latitude; sums to 1 over the globe).
    pub area: Vec<f64>,
    pub elevation: Vec<f64>,
    pub is_land: Vec<bool>,
    /// Grid distance to the nearest ocean cell (0 on the ocean itself).
    pub dist_to_ocean: Vec<f64>,

    // --- climate state ---
    /// Pre-industrial equilibrium temperature per cell (K), solved once from
    /// the diffusive energy balance at generation.
    pub temp0: Vec<f64>,
    /// Current temperature per cell (K) = temp0 + anomaly x amplification.
    pub temp: Vec<f64>,
    /// Current precipitation per cell (mm/yr).
    pub precip: Vec<f64>,
    /// Polar-amplification pattern (area-weighted mean = 1).
    amp: Vec<f64>,
    /// Pre-industrial area-weighted global mean temperature (K).
    pub t_global0: f64,
    /// Realised global-mean warming anomaly (K) — MEASURED state.
    pub t_anomaly: f64,
    /// Atmospheric CO₂ (ppm).
    pub co2: f64,

    // --- ecology ---
    /// Potential net primary productivity per cell, 0..1 (Miami model, current
    /// climate).
    pub npp: Vec<f64>,
    /// Pristine NPP at the pre-industrial climate (the ecological baseline).
    pub npp0: Vec<f64>,
    /// Renewable fresh-water availability index (1 ≈ comfortable).
    pub water: Vec<f64>,
    /// Soil fertility 0..1 (degrades under over-intensive cultivation).
    pub soil: Vec<f64>,
    /// Standing biomass stock and its pristine capacity (numéraire units).
    pub biomass: Vec<f64>,
    pub biomass_k0: Vec<f64>,
    /// Coastal fishery stock and capacity (on ocean cells bordering land).
    pub fish: Vec<f64>,
    pub fish_k: Vec<f64>,
    /// Finite deposits (numéraire units of fuel/material at unit extraction).
    pub fossil: Vec<f64>,
    pub mineral: Vec<f64>,
    /// Biodiversity index 0..1 per land cell (species–area response to intact
    /// habitat).
    pub biodiversity: Vec<f64>,

    // --- per-year pressure ledgers the economy writes, the planet consumes ---
    /// Fraction of each land cell under cultivation this year.
    pub land_use: Vec<f64>,
    /// CO₂ emitted this year (ppm-equivalent), from fossil burning + land use.
    pub emissions_this_year: f64,

    /// Food-yield scale: numéraire units one fully-cultivated, fully-fertile
    /// cell of unit NPP yields per year. Fixed at generation so the pristine
    /// planet could feed a documented multiple of the seed population — a unit
    /// choice (the grid is a scale model), not an outcome.
    pub yield_scale: f64,
}

impl Planet {
    #[inline]
    pub fn idx(&self, x: usize, y: usize) -> usize {
        y * self.nlon + x
    }
    #[inline]
    pub fn cells(&self) -> usize {
        self.nlon * self.nlat
    }
    /// 4-neighbourhood with longitude wrap-around and latitude clamping.
    pub fn neighbors(&self, i: usize) -> [usize; 4] {
        let (x, y) = (i % self.nlon, i / self.nlon);
        let xl = (x + self.nlon - 1) % self.nlon;
        let xr = (x + 1) % self.nlon;
        let yu = y.saturating_sub(1);
        let yd = (y + 1).min(self.nlat - 1);
        [
            self.idx(xl, y),
            self.idx(xr, y),
            self.idx(x, yu),
            self.idx(x, yd),
        ]
    }

    /// Generate a planet from the scenario config. Deterministic per seed.
    pub fn generate(cfg: &WorldConfig, rng: &mut Rng) -> Planet {
        let (nlon, nlat) = (cfg.nlon.max(8), cfg.nlat.max(4));
        let n = nlon * nlat;

        // Latitudes and honest spherical area weights.
        let mut lat = vec![0.0; n];
        let mut area = vec![0.0; n];
        let mut wsum = 0.0;
        for y in 0..nlat {
            let phi = std::f64::consts::PI * ((y as f64 + 0.5) / nlat as f64 - 0.5);
            let w = phi.cos();
            for x in 0..nlon {
                lat[y * nlon + x] = phi;
                area[y * nlon + x] = w;
            }
            wsum += w * nlon as f64;
        }
        for a in &mut area {
            *a /= wsum;
        }

        // --- Geography: tileable fractal noise -> elevation; the sea level is
        // the area-weighted quantile that yields the configured land fraction.
        let noise_seed = rng.next_u64();
        let mut elevation = vec![0.0; n];
        for y in 0..nlat {
            for x in 0..nlon {
                elevation[y * nlon + x] = fractal_noise(noise_seed, nlon, x, y, 5);
            }
        }
        let sea_level = weighted_quantile(&elevation, &area, 1.0 - cfg.land_fraction);
        let mut is_land = vec![false; n];
        for i in 0..n {
            is_land[i] = elevation[i] > sea_level;
            // Re-zero elevation at the coast; land rises to a few km.
            elevation[i] = if is_land[i] {
                (elevation[i] - sea_level) * 6.0 // km, peaks ~3-4 km
            } else {
                0.0
            };
        }

        // Distance to ocean (BFS over the grid).
        let mut dist = vec![f64::INFINITY; n];
        let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&i| !is_land[i]).collect();
        for &i in &queue {
            dist[i] = 0.0;
        }
        let tmp = Planet {
            nlon,
            nlat,
            lat: lat.clone(),
            area: area.clone(),
            elevation: vec![],
            is_land: vec![],
            dist_to_ocean: vec![],
            temp0: vec![],
            temp: vec![],
            precip: vec![],
            amp: vec![],
            t_global0: 0.0,
            t_anomaly: 0.0,
            co2: CO2_PREINDUSTRIAL,
            npp: vec![],
            npp0: vec![],
            water: vec![],
            soil: vec![],
            biomass: vec![],
            biomass_k0: vec![],
            fish: vec![],
            fish_k: vec![],
            fossil: vec![],
            mineral: vec![],
            biodiversity: vec![],
            land_use: vec![],
            emissions_this_year: 0.0,
            yield_scale: 0.0,
        };
        while let Some(i) = queue.pop_front() {
            for nb in tmp.neighbors(i) {
                if dist[nb].is_infinite() {
                    dist[nb] = dist[i] + 1.0;
                    queue.push_back(nb);
                }
            }
        }

        // --- Pre-industrial climate: diffusive EBM solved by relaxation.
        // Annual-mean insolation Q(phi) = S0/4 * (1 - 0.477 * P2(sin phi))
        // (North 1975); albedo by surface + ice feedback; elevation lapse;
        // meridional diffusion smooths the gradient.
        let mut temp0 = vec![288.0_f64; n];
        for _ in 0..600 {
            let snapshot = temp0.clone();
            for i in 0..n {
                let s = lat[i].sin();
                let p2 = 0.5 * (3.0 * s * s - 1.0);
                let q = SOLAR_CONSTANT / 4.0 * (1.0 - 0.477 * p2);
                // Smooth ice fraction from the *surface* temperature.
                let ice = ((268.0 - snapshot[i]) / 10.0).clamp(0.0, 1.0);
                let base_albedo = if is_land[i] { ALBEDO_LAND } else { ALBEDO_OCEAN };
                let albedo = base_albedo * (1.0 - ice) + ALBEDO_ICE * ice;
                let absorbed = q * (1.0 - albedo);
                // Outgoing longwave from the sea-level-equivalent temperature
                // (the lapse-rate cooling is applied as a surface offset below).
                let t = snapshot[i] + LAPSE_RATE * elevation[i] * if is_land[i] { 1.0 } else { 0.0 };
                let outgoing = EMISSIVITY * STEFAN_BOLTZMANN * t.powi(4);
                let nb = tmp.neighbors(i);
                let lap: f64 =
                    nb.iter().map(|&j| snapshot[j]).sum::<f64>() / 4.0 - snapshot[i];
                let flux = absorbed - outgoing + HEAT_DIFFUSION * 6.0 * lap;
                // Pseudo-time relaxation toward equilibrium.
                temp0[i] = snapshot[i] + flux * 0.05;
            }
        }
        let t_global0: f64 = (0..n).map(|i| temp0[i] * area[i]).sum();

        // Polar-amplification pattern, normalised to an area-weighted mean of 1.
        let mut amp: Vec<f64> = lat
            .iter()
            .map(|&phi| 1.0 + POLAR_AMP * (phi.sin().powi(2) - 0.5))
            .collect();
        let mean_amp: f64 = (0..n).map(|i| amp[i] * area[i]).sum();
        for a in &mut amp {
            *a /= mean_amp;
        }

        // --- Hydrology: zonal precipitation belts (ITCZ + mid-latitude storm
        // tracks), continentality decay, orographic enhancement; normalised to
        // the observed global mean.
        let mut precip = vec![0.0; n];
        for i in 0..n {
            precip[i] = zonal_precip(lat[i])
                * (-(dist[i] / (0.10 * nlon as f64))).exp().max(0.05)
                * (1.0 + 0.25 * elevation[i].min(2.0));
        }
        let pmean: f64 = (0..n).map(|i| precip[i] * area[i]).sum();
        for p in &mut precip {
            *p *= PRECIP_MEAN / pmean;
        }

        // --- Ecology from climate: Miami-model NPP, water index, soils,
        // biomass/fishery stocks at their pristine capacity, biodiversity 1.
        let mut npp0 = vec![0.0; n];
        for i in 0..n {
            if is_land[i] {
                npp0[i] = miami_npp(temp0[i] - LAPSE_RATE * elevation[i], precip[i]);
            }
        }
        let water: Vec<f64> = precip.iter().map(|&p| (p / 800.0).min(2.5)).collect();
        let soil = vec![1.0_f64; n];

        // Yield unit: the pristine land, fully cultivated by primitive labour,
        // could feed ~8x the seed population (a scale-model unit choice; the
        // realised carrying capacity is emergent).
        let total_npp: f64 = (0..n).map(|i| npp0[i]).sum();
        let yield_scale = 8.0 * cfg.n_agents as f64 / total_npp.max(1e-9);

        let biomass_k0: Vec<f64> = npp0
            .iter()
            .map(|&v| v * yield_scale * BIOMASS_STOCK_YEARS / 8.0)
            .collect();
        let biomass = biomass_k0.clone();

        // Coastal fisheries: ocean cells adjacent to land, scaled so the sea
        // contributes roughly a sixth of pristine food potential (the marine
        // share of human calories is small; FAO).
        let mut fish_k = vec![0.0; n];
        let mut n_coast = 0usize;
        for i in 0..n {
            if !is_land[i] && tmp.neighbors(i).iter().any(|&j| is_land[j]) {
                n_coast += 1;
            }
        }
        if n_coast > 0 {
            let per = 8.0 * cfg.n_agents as f64 / 6.0 / n_coast as f64 * BIOMASS_STOCK_YEARS / 8.0;
            for i in 0..n {
                if !is_land[i] && tmp.neighbors(i).iter().any(|&j| is_land[j]) {
                    fish_k[i] = per;
                }
            }
        }
        let fish = fish_k.clone();

        // --- Finite deposits: clustered Gaussian blobs over land.
        let fossil = deposits(rng, &is_land, nlon, nlat, cfg.fossil_endowment * cfg.n_agents as f64);
        let mineral =
            deposits(rng, &is_land, nlon, nlat, cfg.mineral_endowment * cfg.n_agents as f64);

        let biodiversity: Vec<f64> = is_land.iter().map(|&l| if l { 1.0 } else { 0.0 }).collect();

        Planet {
            nlon,
            nlat,
            lat,
            area,
            elevation,
            is_land,
            dist_to_ocean: dist,
            temp: temp0.clone(),
            temp0,
            precip,
            amp,
            t_global0,
            t_anomaly: 0.0,
            co2: CO2_PREINDUSTRIAL,
            npp: npp0.clone(),
            npp0,
            water,
            soil,
            biomass,
            biomass_k0,
            fish,
            fish_k,
            fossil,
            mineral,
            biodiversity,
            land_use: vec![0.0; n],
            emissions_this_year: 0.0,
            yield_scale,
        }
    }

    /// Advance the physical planet by one year. The economy has already
    /// written this year's pressures (harvests subtracted from stocks,
    /// `land_use`, `emissions_this_year`); this integrates carbon, temperature,
    /// the water cycle and ecosystem renewal, then clears the pressure ledgers.
    pub fn step(&mut self) {
        // Carbon: accumulate emissions, first-order uptake toward C0.
        self.co2 += self.emissions_this_year - CO2_DECAY * (self.co2 - CO2_PREINDUSTRIAL);
        self.co2 = self.co2.max(150.0);

        // Greenhouse forcing -> equilibrium anomaly -> thermal-inertia
        // relaxation (Myhre 1998 log forcing over the Planck+feedbacks slope).
        let forcing = FORCING_LAMBDA * (self.co2 / CO2_PREINDUSTRIAL).ln();
        let eq_anomaly = forcing / PLANCK_FEEDBACK;
        self.t_anomaly += TEMP_RELAX * (eq_anomaly - self.t_anomaly);

        // Pattern-scaled temperature field (polar amplification) and a
        // Clausius–Clapeyron-scaled water cycle (~7%/K wet-gets-wetter, with
        // continental drying expressed through the NPP water limitation).
        let cc = 1.0 + 0.04 * self.t_anomaly;
        for i in 0..self.cells() {
            self.temp[i] = self.temp0[i] + self.t_anomaly * self.amp[i];
            self.water[i] = (self.precip[i] * cc / 800.0).min(2.5);
        }

        // Ecology under the new climate.
        for i in 0..self.cells() {
            if !self.is_land[i] {
                // Fishery: Schaefer logistic renewal (harvest was subtracted).
                if self.fish_k[i] > 0.0 {
                    let s = self.fish[i].max(0.02 * self.fish_k[i]);
                    self.fish[i] =
                        (self.fish[i] + FISH_REGROWTH * s * (1.0 - self.fish[i] / self.fish_k[i]))
                            .clamp(0.0, self.fish_k[i]);
                }
                continue;
            }
            // NPP under current temperature & rainfall (the climate-damage
            // channel: warming past the local optimum and drying both bite).
            self.npp[i] = miami_npp(
                self.temp[i] - LAPSE_RATE * self.elevation[i],
                self.precip[i] * cc,
            );

            // Biomass regrows logistically toward a capacity set by *current*
            // productivity and intact habitat; degraded climate lowers K.
            let k = self.biomass_k0[i]
                * if self.npp0[i] > 0.0 { self.npp[i] / self.npp0[i] } else { 0.0 }
                * (1.0 - 0.5 * self.land_use[i]);
            if k > 0.0 {
                let s = self.biomass[i].max(0.02 * k);
                self.biomass[i] = (self.biomass[i]
                    + BIOMASS_REGROWTH * s * (1.0 - self.biomass[i] / k))
                    .clamp(0.0, k);
            } else {
                self.biomass[i] *= 1.0 - BIOMASS_REGROWTH;
            }

            // Soil: recovers when lightly used (degradation is applied by the
            // farming that causes it, in the economy).
            if self.land_use[i] < 0.5 {
                self.soil[i] = (self.soil[i] + SOIL_RECOVER * (1.0 - self.soil[i])).min(1.0);
            }

            // Biodiversity: species–area response to intact habitat (z = 0.25),
            // fast to lose, slow to recover.
            let habitat = ((self.biomass[i] / self.biomass_k0[i].max(1e-9)).clamp(0.0, 1.0)
                * (1.0 - self.land_use[i]))
                .powf(0.25);
            let rate = if habitat < self.biodiversity[i] { BIODIV_DECLINE } else { BIODIV_RECOVER };
            self.biodiversity[i] += rate * (habitat - self.biodiversity[i]);
        }

        // Clear the pressure ledgers for the next year's economy.
        self.emissions_this_year = 0.0;
        for u in &mut self.land_use {
            *u = 0.0;
        }
    }

    /// Area-weighted global mean of a per-cell field — the measuring stick the
    /// instruments use.
    pub fn global_mean(&self, field: &[f64]) -> f64 {
        (0..self.cells()).map(|i| field[i] * self.area[i]).sum()
    }

    /// Mean biodiversity over land (area-weighted) — an instrument input.
    pub fn mean_biodiversity(&self) -> f64 {
        let mut s = 0.0;
        let mut w = 0.0;
        for i in 0..self.cells() {
            if self.is_land[i] {
                s += self.biodiversity[i] * self.area[i];
                w += self.area[i];
            }
        }
        if w > 0.0 {
            s / w
        } else {
            0.0
        }
    }

    /// Fraction of pristine standing biomass remaining (the commons-health
    /// instrument input).
    pub fn commons_health(&self) -> f64 {
        let k0: f64 = self.biomass_k0.iter().sum();
        if k0 <= 0.0 {
            1.0
        } else {
            (self.biomass.iter().sum::<f64>() / k0).clamp(0.0, 1.0)
        }
    }
}

/// Miami-model NPP (Lieth 1975), normalised to 0..1 by `NPP_MAX`, with a
/// **high-temperature decline** added. The bare Miami temperature term rises
/// monotonically with warmth (it never falls), which would make warming
/// *raise* productivity everywhere — physically wrong: photosynthesis and
/// Rubisco efficiency fall above an optimum, and heat/drought stress depress
/// NPP at high temperatures (Huang et al. 2019; Duffy et al. 2021). We multiply
/// the Liebig minimum of the temperature- and precipitation-limited terms by a
/// thermal-stress factor that is 1 up to the optimum (~24 °C) and declines as a
/// Gaussian beyond it — so warming past the optimum lowers NPP, the mechanistic
/// root of emergent climate damage.
pub fn miami_npp(temp_k: f64, precip_mm: f64) -> f64 {
    let t = temp_k - 273.15;
    let npp_t = NPP_MAX / (1.0 + (1.315 - 0.119 * t).exp());
    let npp_p = NPP_MAX * (1.0 - (-0.000664 * precip_mm.max(0.0)).exp());
    let optimum = 24.0_f64;
    let heat_stress = if t > optimum {
        (-((t - optimum) / 12.0).powi(2)).exp()
    } else {
        1.0
    };
    (npp_t.min(npp_p) * heat_stress / NPP_MAX).clamp(0.0, 1.0)
}

/// Zonal annual-mean precipitation shape: an ITCZ peak at the equator,
/// mid-latitude storm tracks near 45°, subtropical and polar dry zones
/// (the observed zonal climatology, e.g. GPCP).
fn zonal_precip(lat: f64) -> f64 {
    let deg = lat.to_degrees();
    let itcz = 1.6 * (-(deg / 10.0).powi(2)).exp();
    let storms = 0.8 * (-(((deg.abs()) - 45.0) / 12.0).powi(2)).exp();
    itcz + storms + 0.10
}

/// Deterministic tileable value noise in [0,1]: hashed lattice values,
/// smoothstep-interpolated, several octaves (longitude wraps).
fn fractal_noise(seed: u64, nlon: usize, x: usize, y: usize, octaves: u32) -> f64 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    for o in 0..octaves {
        let freq = 1 << o; // lattice cells per (small) base period
        let period = (nlon / 4).max(2) / freq as usize;
        let period = period.max(1);
        let gx = x as f64 / period as f64;
        let gy = y as f64 / period as f64;
        let x0 = gx.floor() as i64;
        let y0 = gy.floor() as i64;
        let fx = smooth(gx - x0 as f64);
        let fy = smooth(gy - y0 as f64);
        let wrap = ((nlon + period - 1) / period) as i64; // lattice width
        let h = |ix: i64, iy: i64| -> f64 {
            let ix = ix.rem_euclid(wrap);
            hash01(seed ^ (o as u64) << 56, ix, iy)
        };
        let v = lerp(
            lerp(h(x0, y0), h(x0 + 1, y0), fx),
            lerp(h(x0, y0 + 1), h(x0 + 1, y0 + 1), fx),
            fy,
        );
        sum += v * amp;
        norm += amp;
        amp *= 0.5;
    }
    sum / norm
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
/// SplitMix-style integer hash -> [0,1].
fn hash01(seed: u64, x: i64, y: i64) -> f64 {
    let mut z = seed
        .wrapping_add((x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add((y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// Area-weighted quantile of a field (used to set the sea level).
fn weighted_quantile(values: &[f64], weights: &[f64], q: f64) -> f64 {
    let mut pairs: Vec<(f64, f64)> = values.iter().copied().zip(weights.iter().copied()).collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = pairs.iter().map(|p| p.1).sum();
    let mut acc = 0.0;
    for (v, w) in &pairs {
        acc += w;
        if acc >= q * total {
            return *v;
        }
    }
    pairs.last().map(|p| p.0).unwrap_or(0.0)
}

/// Scatter a finite endowment over land as a few Gaussian deposit blobs.
fn deposits(rng: &mut Rng, is_land: &[bool], nlon: usize, nlat: usize, total: f64) -> Vec<f64> {
    let n = nlon * nlat;
    let mut field = vec![0.0; n];
    let land: Vec<usize> = (0..n).filter(|&i| is_land[i]).collect();
    if land.is_empty() || total <= 0.0 {
        return field;
    }
    let blobs = 8;
    for _ in 0..blobs {
        let c = land[rng.below(land.len())];
        let (cx, cy) = ((c % nlon) as f64, (c / nlon) as f64);
        let sigma = (nlon.min(nlat) as f64) * rng.range(0.03, 0.08);
        for &i in &land {
            let (x, y) = ((i % nlon) as f64, (i / nlon) as f64);
            // Longitude wraps.
            let dx = (x - cx).abs().min(nlon as f64 - (x - cx).abs());
            let d2 = dx * dx + (y - cy) * (y - cy);
            field[i] += (-d2 / (2.0 * sigma * sigma)).exp();
        }
    }
    let sum: f64 = field.iter().sum();
    for v in &mut field {
        *v *= total / sum;
    }
    field
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> Planet {
        let mut cfg = WorldConfig::default();
        cfg.nlon = 36;
        cfg.nlat = 18;
        cfg.n_agents = 1000;
        Planet::generate(&cfg, &mut Rng::seed(1))
    }

    /// The generated planet is physically sane: the configured land fraction,
    /// a modern-Earth-band global mean temperature, and a temperature field
    /// where the tropics are warmer than the poles.
    #[test]
    fn geography_and_climate_are_sane() {
        let p = small();
        let land_area: f64 = (0..p.cells()).filter(|&i| p.is_land[i]).map(|i| p.area[i]).sum();
        assert!(
            (land_area - 0.29).abs() < 0.06,
            "land fraction should be ~0.29 by area, got {land_area}"
        );
        assert!(
            (270.0..300.0).contains(&p.t_global0),
            "global mean temperature should be Earth-like, got {} K",
            p.t_global0
        );
        // Tropics warmer than poles (area-weighted band means).
        let band = |lo: f64, hi: f64| {
            let mut s = 0.0;
            let mut w = 0.0;
            for i in 0..p.cells() {
                let d = p.lat[i].to_degrees().abs();
                if d >= lo && d < hi {
                    s += p.temp0[i] * p.area[i];
                    w += p.area[i];
                }
            }
            s / w
        };
        let tropics = band(0.0, 20.0);
        let poles = band(60.0, 90.0);
        assert!(
            tropics > poles + 10.0,
            "tropics should be much warmer than poles: {tropics} vs {poles}"
        );
        // Rain features exist: a wet equator relative to the subtropics.
        let p_eq = {
            let mut s = 0.0;
            let mut w = 0.0;
            for i in 0..p.cells() {
                if p.lat[i].to_degrees().abs() < 10.0 {
                    s += p.precip[i] * p.area[i];
                    w += p.area[i];
                }
            }
            s / w
        };
        assert!(p_eq > PRECIP_MEAN, "the ITCZ should be wetter than the mean");
    }

    /// Emissions raise CO₂ and temperature; the warming pattern is polar
    /// amplified; stopping emissions lets CO₂ decay back toward C₀. All
    /// integrated physics, nothing scripted.
    #[test]
    fn greenhouse_physics_works() {
        let mut p = small();
        let t0 = p.t_global0;
        for _ in 0..80 {
            p.emissions_this_year = 3.0; // ppm/yr of fossil burning
            p.step();
        }
        assert!(p.co2 > 400.0, "CO2 should accumulate: {}", p.co2);
        assert!(p.t_anomaly > 0.5, "the planet should warm: {}", p.t_anomaly);
        let warming_global = p.global_mean(&p.temp) - t0;
        assert!(warming_global > 0.3);
        // Polar amplification: high-latitude anomaly beats the tropics.
        let anomaly_at = |deg: f64| {
            let mut best = (f64::INFINITY, 0.0);
            for i in 0..p.cells() {
                let d = (p.lat[i].to_degrees().abs() - deg).abs();
                if d < best.0 {
                    best = (d, p.temp[i] - p.temp0[i]);
                }
            }
            best.1
        };
        assert!(anomaly_at(75.0) > anomaly_at(5.0), "warming should be polar-amplified");
        // Decay after emissions stop.
        let c_peak = p.co2;
        for _ in 0..100 {
            p.step();
        }
        assert!(p.co2 < c_peak, "CO2 should decay once emissions stop");
    }

    /// The climate-damage mechanism: productivity peaks at an optimum and
    /// **declines past it** (heat stress), so warming hurts already-warm land
    /// even where it helps cold land. We assert the mechanism directly (the
    /// Miami+heat-stress curve falls at high temperature) and that on the planet
    /// strong warming both degrades the warmest (tropical) cells' NPP and
    /// collapses biodiversity — the real damage, even on a net-greening cold
    /// world.
    #[test]
    fn warming_damages_warm_regions_and_biodiversity() {
        // Mechanism: NPP at the optimum exceeds NPP when much hotter.
        let opt = miami_npp(273.15 + 24.0, 1500.0);
        let hot = miami_npp(273.15 + 40.0, 1500.0);
        assert!(hot < opt, "NPP must fall above the thermal optimum: {hot} vs {opt}");

        // On the planet: the warmest land cells lose productivity under strong
        // warming, and biodiversity collapses.
        let mut p = small();
        let warm_cell = (0..p.cells())
            .filter(|&i| p.is_land[i])
            .max_by(|&a, &b| p.temp0[a].partial_cmp(&p.temp0[b]).unwrap())
            .unwrap();
        let npp_warm_before = p.npp[warm_cell];
        let biodiv_before = p.mean_biodiversity();
        for _ in 0..200 {
            p.emissions_this_year = 8.0;
            p.step();
        }
        assert!(p.t_anomaly > 3.0, "should be a strongly warmed world: {}", p.t_anomaly);
        assert!(
            p.npp[warm_cell] < npp_warm_before,
            "strong warming should lower NPP in the warmest regions: {} vs {}",
            p.npp[warm_cell],
            npp_warm_before
        );
        assert!(
            p.mean_biodiversity() < biodiv_before,
            "strong warming should collapse biodiversity: {} vs {}",
            p.mean_biodiversity(),
            biodiv_before
        );
    }

    /// Harvest pressure draws stocks down; relief lets them regrow; over-use
    /// of land erodes biodiversity (species–area), and habitat recovery brings
    /// it back only slowly.
    #[test]
    fn ecology_responds_to_pressure() {
        let mut p = small();
        let cell = (0..p.cells()).find(|&i| p.is_land[i] && p.biomass_k0[i] > 0.0).unwrap();
        let b0 = p.biomass[cell];
        // Heavy harvest + full cultivation for 30 years.
        for _ in 0..30 {
            p.biomass[cell] *= 0.4;
            p.land_use[cell] = 0.9;
            p.step();
            p.land_use[cell] = 0.9; // economy would re-write it each year
        }
        assert!(p.biomass[cell] < b0, "pressure should deplete the stock");
        let bd_low = p.biodiversity[cell];
        assert!(bd_low < 0.95, "habitat loss should erode biodiversity: {bd_low}");
        let b_low = p.biomass[cell];
        // Release the pressure; the stock regrows, biodiversity recovers slower.
        for _ in 0..80 {
            p.land_use[cell] = 0.0;
            p.step();
        }
        assert!(p.biomass[cell] > b_low, "stock should regrow after relief");
        assert!(p.biodiversity[cell] > bd_low, "biodiversity should begin to recover");
    }

    /// Determinism: the same seed generates a bit-identical planet and a
    /// bit-identical century under pressure.
    #[test]
    fn planet_is_deterministic() {
        let mut cfg = WorldConfig::default();
        cfg.nlon = 24;
        cfg.nlat = 12;
        let mut a = Planet::generate(&cfg, &mut Rng::seed(9));
        let mut b = Planet::generate(&cfg, &mut Rng::seed(9));
        for year in 0..100 {
            a.emissions_this_year = 1.0 + (year % 7) as f64;
            b.emissions_this_year = 1.0 + (year % 7) as f64;
            a.step();
            b.step();
        }
        assert_eq!(a.co2.to_bits(), b.co2.to_bits());
        for i in 0..a.cells() {
            assert_eq!(a.temp[i].to_bits(), b.temp[i].to_bits());
            assert_eq!(a.biomass[i].to_bits(), b.biomass[i].to_bits());
        }
    }
}
