//! **Visualisation** (dependency-free): make the emergent planet legible. An
//! ASCII world map shades any per-cell field (geography, temperature, biomass,
//! population density), and a CSV/sparkline trace records the global emergent
//! time-series. Strictly read-only consumers of the world and its instruments —
//! they never mutate or set anything.

use crate::measure::Measurements;
use crate::world::World;

/// Which per-cell field to paint on the world map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapLayer {
    /// Land/ocean/ice geography.
    Geography,
    /// Surface temperature (cold → hot).
    Temperature,
    /// Standing biomass as a fraction of pristine (the living biosphere).
    Biomass,
    /// Human settlement density.
    Population,
}

impl MapLayer {
    pub fn parse(name: &str) -> Option<MapLayer> {
        Some(match name {
            "geography" | "geo" => MapLayer::Geography,
            "temperature" | "temp" => MapLayer::Temperature,
            "biomass" | "bio" => MapLayer::Biomass,
            "population" | "pop" => MapLayer::Population,
            _ => return None,
        })
    }
}

/// Render the world as an ASCII map of the chosen layer. Rows are latitude
/// bands (north at top), columns longitude; oceans are shown as `.`/`~`.
pub fn render_map(world: &World, layer: MapLayer) -> String {
    let p = &world.planet;
    let (nlon, nlat) = (p.nlon, p.nlat);

    // Per-cell population density (people per cell) for the population layer.
    let mut pop = vec![0u32; p.cells()];
    if layer == MapLayer::Population {
        for i in 0..world.people.len() {
            if world.people.alive[i] {
                pop[world.people.cell[i]] += 1;
            }
        }
    }
    let pop_max = pop.iter().copied().max().unwrap_or(1).max(1) as f64;

    // Temperature range over land, for contrast.
    let (mut tmin, mut tmax) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..p.cells() {
        tmin = tmin.min(p.temp[i]);
        tmax = tmax.max(p.temp[i]);
    }
    let trange = (tmax - tmin).max(1e-6);

    // Shading ramps (dark → bright).
    const RAMP: &[u8] = b" .:-=+*#%@";
    let shade = |frac: f64| -> char {
        let f = frac.clamp(0.0, 1.0);
        RAMP[((f * (RAMP.len() - 1) as f64).round() as usize).min(RAMP.len() - 1)] as char
    };

    let mut out = String::with_capacity((nlon + 1) * nlat);
    for y in 0..nlat {
        for x in 0..nlon {
            let i = y * nlon + x;
            let ch = match layer {
                MapLayer::Geography => {
                    if !p.is_land[i] {
                        '~'
                    } else if p.temp[i] < 263.0 {
                        '*' // ice cap
                    } else if p.elevation[i] > 1.5 {
                        '^' // mountains
                    } else {
                        '#'
                    }
                }
                MapLayer::Temperature => {
                    if !p.is_land[i] {
                        '~'
                    } else {
                        shade((p.temp[i] - tmin) / trange)
                    }
                }
                MapLayer::Biomass => {
                    if !p.is_land[i] {
                        '~'
                    } else {
                        let k0 = p.biomass_k0[i].max(1e-9);
                        shade(p.biomass[i] / k0)
                    }
                }
                MapLayer::Population => {
                    if pop[i] == 0 {
                        if p.is_land[i] {
                            '.'
                        } else {
                            '~'
                        }
                    } else {
                        shade(pop[i] as f64 / pop_max)
                    }
                }
            };
            out.push(ch);
        }
        out.push('\n');
    }
    out
}

/// A single sparkline of a series in `[min,max]`, drawn with block glyphs.
pub fn sparkline(series: &[f64]) -> String {
    const BARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let finite: Vec<f64> = series.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return String::new();
    }
    let lo = finite.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = finite.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = (hi - lo).max(1e-9);
    series
        .iter()
        .map(|&v| {
            if !v.is_finite() {
                ' '
            } else {
                let idx = ((v - lo) / range * (BARS.len() - 1) as f64).round() as usize;
                BARS[idx.min(BARS.len() - 1)]
            }
        })
        .collect()
}

/// The CSV header for a global time-series trace (stable column order).
pub const TRACE_HEADER: &str = "year,population,gdp_per_capita,wealth_gini,life_expectancy,\
wellbeing,deprivation,co2_ppm,warming_K,clean_share,biodiversity,commons_health,fossil_remaining";

/// One CSV row from a measurements snapshot (columns match `TRACE_HEADER`).
pub fn trace_row(m: &Measurements) -> String {
    let life = if m.life_expectancy.is_finite() { m.life_expectancy } else { 0.0 };
    format!(
        "{},{},{:.4},{:.4},{:.2},{:.4},{:.4},{:.2},{:.3},{:.4},{:.4},{:.4},{:.4}",
        m.year,
        m.population,
        m.gdp_per_capita,
        m.wealth_gini,
        life,
        m.wellbeing,
        m.deprivation_rate,
        m.co2,
        m.temp_anomaly,
        m.clean_share,
        m.biodiversity,
        m.commons_health,
        m.fossil_remaining,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorldConfig;

    fn small() -> World {
        let mut cfg = WorldConfig::default();
        cfg.nlon = 24;
        cfg.nlat = 12;
        cfg.n_agents = 600;
        World::new(&cfg)
    }

    #[test]
    fn map_layers_render_with_right_shape() {
        let mut w = small();
        for _ in 0..30 {
            w.step();
        }
        for layer in [
            MapLayer::Geography,
            MapLayer::Temperature,
            MapLayer::Biomass,
            MapLayer::Population,
        ] {
            let m = render_map(&w, layer);
            let lines: Vec<&str> = m.lines().collect();
            assert_eq!(lines.len(), w.planet.nlat, "one row per latitude");
            assert!(lines.iter().all(|l| l.chars().count() == w.planet.nlon));
            // Oceans appear on every layer.
            assert!(m.contains('~'), "{layer:?} map should show ocean");
        }
        assert!(MapLayer::parse("temp").is_some());
        assert!(MapLayer::parse("nonsense").is_none());
    }

    #[test]
    fn map_reflects_state_population_concentrates() {
        let mut w = small();
        for _ in 0..40 {
            w.step();
        }
        let map = render_map(&w, MapLayer::Population);
        // Some cells are inhabited (non-blank, non-ocean glyphs present).
        assert!(map.chars().any(|c| "▁:-=+*#%@".contains(c) || c.is_alphanumeric() || "=+*#%@".contains(c) || c == '@' ));
        // Determinism of the render.
        assert_eq!(render_map(&w, MapLayer::Population), map);
    }

    #[test]
    fn sparkline_and_trace_are_sane() {
        let s = sparkline(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(s.chars().count(), 4);
        assert!(sparkline(&[]).is_empty());
        // Flat and non-finite series don't panic.
        let _ = sparkline(&[5.0, 5.0, 5.0]);
        let _ = sparkline(&[f64::NAN, 1.0]);

        let mut w = small();
        for _ in 0..5 {
            w.step();
        }
        let row = trace_row(&w.measure());
        assert_eq!(row.split(',').count(), TRACE_HEADER.split(',').count());
    }
}
