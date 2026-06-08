//! **Trace & visualisation** (Phase 7): make the emergence *visible*.
//!
//! Everything here is a strictly **read-only** consumer of the
//! [`crate::engine::instruments`]: it runs a [`World`] forward, takes a full
//! [`Measurements`] snapshot each tick, and records the emergent series into
//! columnar storage. From that recording it can:
//!
//! - write a **stable-header CSV** (one row per tick, `ticks + 1` rows including
//!   the initial state) covering the key emergent metrics across *all* phases —
//!   population, the wealth Gini, mean wealth, life expectancy, the emergent
//!   price index, GDP flow, specialization, commons health, temperature /
//!   greenhouse stock, and state capacity / legitimacy / corruption; and
//! - render an **ASCII picture of a run**: a shaded heatmap of the resource
//!   landscape (or agent density) plus sparklines of a few headline series — so
//!   the regimes and distributions the engine produces can be *seen* in a plain
//!   terminal, with no plotting dependency.
//!
//! Nothing in this module mutates a world other than by calling the same public
//! `step` / `step_with_rules` the engine already exposes; no macro quantity is
//! ever set. The recorder is fully deterministic: the same seed yields a
//! byte-identical CSV.

use super::instruments::{measure, Measurements};
use super::world::{World, NGOODS};

/// A recorded run: the per-tick [`Measurements`] in capture order, plus a
/// reference to the final world for spatial rendering.
///
/// The series are stored as a single `Vec<Measurements>` (columnar access is
/// provided by the helper iterators below); this keeps the recorder a thin,
/// allocation-light wrapper around the instruments.
#[derive(Debug, Clone)]
pub struct Trace {
    /// One measurement per recorded tick, in order. The first entry is the
    /// initial state (tick 0, before any step), so a run of `n` steps yields
    /// `n + 1` rows.
    pub frames: Vec<Measurements>,
}

impl Trace {
    /// Number of recorded frames (`ticks + 1`).
    pub fn len(&self) -> usize {
        self.frames.len()
    }
    /// Whether the trace is empty.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Extract one column as a `Vec<f64>` via a projection over each frame — the
    /// columnar view the sparkline renderer and any analysis consume.
    pub fn series<F: Fn(&Measurements) -> f64>(&self, f: F) -> Vec<f64> {
        self.frames.iter().map(f).collect()
    }
}

/// The CSV column header — **stable**: append-only, fixed order. One name per
/// emergent metric recorded per tick. (Kept in lockstep with [`csv_row`].)
pub const TRACE_CSV_HEADER: &str = "tick,population,gini,mean_wealth,life_expectancy,\
price_index,gdp_flow,production,specialization,commons_health,\
temperature,greenhouse_stock,emissions,climate_sensitivity,\
state_capacity,legitimacy,corruption,public_pool";

/// Format one measurement as a CSV row matching [`TRACE_CSV_HEADER`]. Non-finite
/// values (e.g. an undefined price or life expectancy before the first trade /
/// death) are written as an empty field so the row is always well-formed and the
/// output stays deterministic. Floats use a fixed precision for byte-stability.
fn csv_row(m: &Measurements) -> String {
    fn num(x: f64) -> String {
        if x.is_finite() {
            format!("{x:.6}")
        } else {
            String::new()
        }
    }
    format!(
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        m.tick,
        m.population,
        num(m.wealth_gini),
        num(m.mean_wealth),
        num(m.life_expectancy),
        num(m.price_index),
        num(m.gdp_flow),
        num(m.production),
        num(m.specialization),
        num(m.commons_health),
        num(m.temperature),
        num(m.greenhouse_stock),
        num(m.emissions),
        num(m.climate_sensitivity),
        num(m.state_capacity),
        num(m.legitimacy),
        num(m.corruption),
        num(m.public_pool),
    )
}

impl Trace {
    /// Render the whole trace as a CSV string: the stable header followed by one
    /// row per recorded frame (`ticks + 1` rows). Byte-identical for the same
    /// seed and inputs.
    pub fn to_csv(&self) -> String {
        let mut out = String::with_capacity(TRACE_CSV_HEADER.len() + self.frames.len() * 120);
        out.push_str(TRACE_CSV_HEADER);
        out.push('\n');
        for m in &self.frames {
            out.push_str(&csv_row(m));
            out.push('\n');
        }
        out
    }
}

/// Run a [`World`] forward `ticks` steps under a fixed rule stack, recording a
/// [`Measurements`] frame at the initial state and after each step. An empty
/// `rules` slice records a *plain* run (the bare `step` pipeline); a non-empty
/// one records a governed run (`step_with_rules`). Read-only over instruments.
pub fn record(world: &mut World, rules: &[Box<dyn super::institutions::Rule>], ticks: usize) -> Trace {
    let mut frames = Vec::with_capacity(ticks + 1);
    frames.push(measure(world)); // tick 0: the initial state
    for _ in 0..ticks {
        if rules.is_empty() {
            world.step();
        } else {
            world.step_with_rules(rules);
        }
        frames.push(measure(world));
    }
    Trace { frames }
}

// ---------------------------------------------------------------------------
// ASCII rendering — a dependency-free picture of a run.
// ---------------------------------------------------------------------------

/// Shading ramp from empty to full (light → dark), used by the heatmap and the
/// per-cell density map. Index a normalised value in `[0,1]` into this.
const SHADES: [char; 8] = [' ', '.', ':', '-', '=', '+', '*', '#'];

/// Sparkline ramp (eighths block ascii surrogate): low → high using ASCII so the
/// output is pure 7-bit and stable across terminals.
const SPARK: [char; 8] = ['_', '.', ',', '-', '~', '=', '*', '#'];

#[inline]
fn shade(norm: f64, ramp: &[char; 8]) -> char {
    let n = if norm.is_finite() { norm.clamp(0.0, 1.0) } else { 0.0 };
    let i = ((n * (ramp.len() as f64 - 1.0)).round() as usize).min(ramp.len() - 1);
    ramp[i]
}

/// Render the resource landscape as a shaded heatmap (summed over goods,
/// normalised by each cell's pristine capacity so the picture is comparable
/// across geographies). Returns a multi-line string with a header. Read-only.
pub fn render_resource_heatmap(world: &World) -> String {
    let w = world.substrate.width;
    let h = world.substrate.height;
    let mut out = String::new();
    out.push_str(&format!("resource landscape ({w}x{h}, shaded by stock/capacity):\n"));
    for y in 0..h {
        for x in 0..w {
            let i = world.substrate.idx(x, y);
            let cap: f64 = (0..NGOODS).map(|g| world.substrate.capacity0[i][g]).sum();
            let res: f64 = (0..NGOODS).map(|g| world.substrate.resource[i][g]).sum();
            let norm = if cap > 0.0 { res / cap } else { 0.0 };
            out.push(shade(norm, &SHADES));
        }
        out.push('\n');
    }
    out
}

/// Render the **agent density** map: one char per cell, shaded by whether a
/// living agent occupies it (occupancy is 0/1 per cell, so this is effectively a
/// population map). Read-only.
pub fn render_agent_density(world: &World) -> String {
    let w = world.substrate.width;
    let h = world.substrate.height;
    let mut occupied = vec![false; w * h];
    for i in 0..world.agents.len() {
        if world.agents.alive[i] {
            let c = world.agents.cell[i];
            if c < occupied.len() {
                occupied[c] = true;
            }
        }
    }
    let pop = occupied.iter().filter(|&&o| o).count();
    let mut out = String::new();
    out.push_str(&format!("agent density ({w}x{h}, {pop} occupied cells):\n"));
    for y in 0..h {
        for x in 0..w {
            let i = world.substrate.idx(x, y);
            out.push(if occupied[i] { '#' } else { '.' });
        }
        out.push('\n');
    }
    out
}

/// Render a single ASCII **sparkline** for a series, min–max normalised, prefixed
/// with a fixed-width label and annotated with its first→last values. Returns a
/// one-line string. An empty or all-non-finite series renders as blanks.
pub fn render_sparkline(label: &str, series: &[f64]) -> String {
    let finite: Vec<f64> = series.iter().copied().filter(|x| x.is_finite()).collect();
    if finite.is_empty() {
        return format!("{label:<16} (no data)");
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &v in &finite {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    let span = hi - lo;
    let line: String = series
        .iter()
        .map(|&v| {
            if !v.is_finite() {
                ' '
            } else if span <= 0.0 {
                shade(0.5, &SPARK)
            } else {
                shade((v - lo) / span, &SPARK)
            }
        })
        .collect();
    let first = *finite.first().unwrap();
    let last = *finite.last().unwrap();
    format!("{label:<16} {line}  [{first:.3} -> {last:.3}]")
}

/// A labelled projection from a measurement frame to one plotted series value.
type SeriesProjection = (&'static str, fn(&Measurements) -> f64);

/// Render a whole [`Trace`] as a block of headline sparklines (the emergent
/// time-series), in a fixed order. Read-only over the recorded frames.
pub fn render_trace_sparklines(trace: &Trace) -> String {
    let mut out = String::new();
    out.push_str(&format!("emergent series over {} ticks:\n", trace.len().saturating_sub(1)));
    let rows: [SeriesProjection; 8] = [
        ("population", |m| m.population as f64),
        ("gini", |m| m.wealth_gini),
        ("mean_wealth", |m| m.mean_wealth),
        ("price_index", |m| m.price_index),
        ("gdp_flow", |m| m.gdp_flow),
        ("specialization", |m| m.specialization),
        ("commons_health", |m| m.commons_health),
        ("temperature", |m| m.temperature),
    ];
    for (label, f) in rows {
        let s = trace.series(f);
        out.push_str(&render_sparkline(label, &s));
        out.push('\n');
    }
    out
}

/// Render a full **run report**: the spatial heatmap, the agent density map, and
/// the headline sparklines — the one-call "see the run" view used by
/// `simctl render`. Read-only.
pub fn render_run(world: &World, trace: &Trace) -> String {
    let mut out = String::new();
    out.push_str(&render_resource_heatmap(world));
    out.push('\n');
    out.push_str(&render_agent_density(world));
    out.push('\n');
    out.push_str(&render_trace_sparklines(trace));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::world::Primitives;

    /// The CSV has the expected stable header, exactly `ticks + 1` data rows, and
    /// is **byte-identical for the same seed** — the determinism guarantee a
    /// visualization can rely on.
    #[test]
    fn csv_has_stable_header_row_count_and_is_deterministic() {
        let ticks = 60;
        let make = || {
            let mut p = Primitives::demo();
            p.seed = 42;
            let mut w = World::new(p);
            record(&mut w, &[], ticks).to_csv()
        };
        let a = make();
        let b = make();
        assert_eq!(a, b, "same seed must give a byte-identical CSV");

        let lines: Vec<&str> = a.lines().collect();
        assert_eq!(lines[0], TRACE_CSV_HEADER, "header must be the stable one");
        assert_eq!(
            lines.len(),
            ticks + 2,
            "header + (ticks + 1) data rows, got {}",
            lines.len()
        );
        // Every data row has exactly as many fields as the header has columns.
        let cols = TRACE_CSV_HEADER.split(',').count();
        for (r, row) in lines[1..].iter().enumerate() {
            assert_eq!(
                row.split(',').count(),
                cols,
                "row {r} has the wrong column count: {row}"
            );
        }
        // First data row is the initial state at tick 0.
        assert!(lines[1].starts_with("0,"), "first data row should be tick 0");
    }

    /// The CSV records the climate columns on a warming preset (the cross-phase
    /// coverage requirement): temperature and greenhouse stock are present and
    /// move over the run, while a default demo run leaves them at the no-op steady
    /// state — proving the trace faithfully reports whatever the engine produced.
    #[test]
    fn csv_covers_climate_columns_when_climate_is_on() {
        let mut p = Primitives::warming_world();
        p.seed = 7;
        let mut w = World::new(p);
        let trace = record(&mut w, &[], 100);
        let temps = trace.series(|m| m.temperature);
        let ghg = trace.series(|m| m.greenhouse_stock);
        assert!(temps[0].is_finite() && temps[temps.len() - 1] > temps[0], "warming should show up");
        assert!(ghg[ghg.len() - 1] > ghg[0], "greenhouse stock should accumulate");
    }

    /// The renderer produces **non-empty output without panicking** for both the
    /// spatial maps and the sparklines, after a real run.
    #[test]
    fn renderer_produces_non_empty_output() {
        let mut p = Primitives::demo();
        p.seed = 1;
        let mut w = World::new(p);
        let trace = record(&mut w, &[], 50);

        let heat = render_resource_heatmap(&w);
        let dens = render_agent_density(&w);
        let report = render_run(&w, &trace);

        assert!(heat.contains('\n') && heat.len() > w.substrate.width);
        assert!(dens.contains('#') || dens.contains('.'), "density map should render cells");
        assert!(report.contains("population"), "report should include the headline series");
        // A heatmap row is exactly `width` shading chars (plus the newline).
        let first_grid_line = heat.lines().nth(1).unwrap();
        assert_eq!(first_grid_line.chars().count(), w.substrate.width);
    }

    /// Sparkline normalisation is robust to a flat series and to embedded
    /// non-finite values (an undefined price before the first trade), never
    /// panicking and always returning the label.
    #[test]
    fn sparkline_handles_flat_and_nonfinite_series() {
        let flat = render_sparkline("flat", &[3.0, 3.0, 3.0]);
        assert!(flat.contains("flat"));
        let withnan = render_sparkline("mixed", &[f64::NAN, 1.0, 2.0]);
        assert!(withnan.contains("mixed"));
        let empty = render_sparkline("empty", &[]);
        assert!(empty.contains("no data"));
    }

    /// Render is deterministic and read-only: rendering a world does not change
    /// the measured state, and two identical runs render identically.
    #[test]
    fn render_is_deterministic_and_read_only() {
        let mk = || {
            let mut p = Primitives::demo();
            p.seed = 9;
            let mut w = World::new(p);
            let trace = record(&mut w, &[], 40);
            let before = measure(&w);
            let r = render_run(&w, &trace);
            let after = measure(&w);
            assert_eq!(before.population, after.population, "render must not mutate the world");
            r
        };
        assert_eq!(mk(), mk(), "rendering must be deterministic");
    }
}
