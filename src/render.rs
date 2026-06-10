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

/// Render the world to a **PNG** image (dependency-free, universally
/// viewable) at `scale` pixels per cell, painting the chosen layer with a
/// natural palette. Read-only.
pub fn render_png(world: &World, layer: MapLayer, scale: usize) -> Vec<u8> {
    let p = &world.planet;
    let (nlon, nlat) = (p.nlon, p.nlat);
    let scale = scale.max(1);
    let (w, h) = (nlon * scale, nlat * scale);

    // Per-cell population for the population layer.
    let mut pop = vec![0u32; p.cells()];
    if layer == MapLayer::Population {
        for i in 0..world.people.len() {
            if world.people.alive[i] {
                pop[world.people.cell[i]] += 1;
            }
        }
    }
    let pop_max = pop.iter().copied().max().unwrap_or(1).max(1) as f64;
    let (mut tmin, mut tmax) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..p.cells() {
        tmin = tmin.min(p.temp[i]);
        tmax = tmax.max(p.temp[i]);
    }
    let trange = (tmax - tmin).max(1e-6);
    let ramp = |v: f64, a: [u8; 3], b: [u8; 3]| -> [u8; 3] {
        let v = v.clamp(0.0, 1.0);
        [0, 1, 2].map(|k| (a[k] as f64 + (b[k] as f64 - a[k] as f64) * v) as u8)
    };
    let color = |i: usize| -> [u8; 3] {
        let land = p.is_land[i];
        let ocean = [16u8, 42, 74];
        match layer {
            MapLayer::Geography => {
                if !land { ocean }
                else if p.temp[i] < 263.0 { [232, 240, 247] }
                else if p.elevation[i] > 1.5 { [120, 110, 96] }
                else {
                    let b = if p.biomass_k0[i] > 0.0 { p.biomass[i] / p.biomass_k0[i] } else { 0.0 };
                    [(70.0 + (1.0 - b) * 120.0) as u8, (110.0 + b * 60.0) as u8, (60.0 + (1.0 - b) * 40.0) as u8]
                }
            }
            MapLayer::Temperature => if !land { ocean } else { ramp((p.temp[i] - tmin) / trange, [60, 90, 170], [230, 90, 60]) },
            MapLayer::Biomass => if !land { ocean } else {
                let b = if p.biomass_k0[i] > 0.0 { p.biomass[i] / p.biomass_k0[i] } else { 0.0 };
                ramp(b, [40, 40, 30], [80, 200, 90])
            },
            MapLayer::Population => {
                if !land { ocean }
                else if pop[i] == 0 { [30, 38, 48] }
                else { ramp(pop[i] as f64 / pop_max, [40, 30, 60], [250, 230, 120]) }
            }
        }
    };

    // Build the raw RGB raster (top-down), then PNG-encode it.
    let mut raw = Vec::with_capacity(h * (1 + w * 3));
    for py in 0..h {
        let cy = py / scale;
        raw.push(0u8); // filter type 0 (none) per scanline
        for px in 0..w {
            let cx = px / scale;
            let c = color(cy * nlon + cx);
            raw.extend_from_slice(&c);
        }
    }
    encode_png(w, h, &raw)
}

/// CRC-32 (IEEE) over a byte slice.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Adler-32 over a byte slice (the zlib trailer).
fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b): (u32, u32) = (1, 0);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Minimal PNG encoder: 8-bit RGB, a zlib stream of **stored** (uncompressed)
/// DEFLATE blocks — no compression library needed, and the files stay modest
/// at map resolutions. Produces a standard, universally-viewable PNG.
fn encode_png(w: usize, h: usize, raw_rgb_with_filters: &[u8]) -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    // IHDR.
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, truecolor RGB
    chunk(&mut out, b"IHDR", &ihdr);
    // IDAT: zlib(stored deflate).
    let mut zlib = vec![0x78, 0x01]; // CMF/FLG
    let data = raw_rgb_with_filters;
    let mut i = 0;
    while i < data.len() {
        let n = (data.len() - i).min(0xFFFF);
        let last = if i + n >= data.len() { 1u8 } else { 0u8 };
        zlib.push(last); // BFINAL, BTYPE=00 (stored)
        zlib.extend_from_slice(&(n as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(n as u16)).to_le_bytes());
        zlib.extend_from_slice(&data[i..i + n]);
        i += n;
    }
    zlib.extend_from_slice(&adler32(data).to_be_bytes());
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
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
    fn png_export_has_a_valid_header() {
        let mut w = small();
        for _ in 0..20 { w.step(); }
        let png = render_png(&w, MapLayer::Biomass, 4);
        assert_eq!(&png[0..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A], "PNG signature");
        // IHDR length (13) then "IHDR", then width/height big-endian.
        assert_eq!(&png[12..16], b"IHDR");
        let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
        assert_eq!(width as usize, w.planet.nlon * 4);
        assert_eq!(height as usize, w.planet.nlat * 4);
        // Ends with an IEND chunk.
        assert_eq!(&png[png.len()-8..png.len()-4], b"IEND");
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
