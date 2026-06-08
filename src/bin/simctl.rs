//! `simctl` — run society simulations from the command line.
//!
//! ## Usage
//!
//! ```text
//! simctl run [--scenario NAME] [--years N]
//!            [--government NAME]            # endogenous-government (governed) mode
//!            [--policy SPEC]...             # manually-stacked policies
//!            [--csv PATH]
//! simctl list                              # scenarios, governments, policies
//! simctl help
//! ```
//!
//! A `--policy` SPEC is `name:start=YEAR,param=VALUE`
//! (e.g. `carbon-tax:start=2030,param=0.8`). `start` defaults to the scenario
//! start year; `param` defaults to 0.5. Stack policies by repeating `--policy`.

use society_sim::governance::ArchetypeGovernment;
use society_sim::policies;
use society_sim::scenario::Scenario;
use society_sim::sim::{Simulation, Snapshot};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(dispatch(&args));
}

/// Route a command line to the right subcommand and return its exit code.
/// Separated from `main` so it is unit-testable (main is only the `exit` shim).
fn dispatch(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("run") => cmd_run(&args[1..]),
        Some("list") => cmd_list(),
        Some("calibrate") => cmd_calibrate(),
        Some("experiment") => cmd_experiment(),
        Some("trace") => cmd_trace(&args[1..]),
        Some("render") => cmd_render(&args[1..]),
        Some("bench") => cmd_bench(&args[1..]),
        Some("help") | Some("--help") | Some("-h") | None => {
            print_help();
            0
        }
        Some(other) => {
            eprintln!("unknown command: {other}\n");
            print_help();
            2
        }
    }
}

fn print_help() {
    println!(
        "society-sim :: simctl — a physics-engine-style simulator for society\n\
         \n\
         Stack parameterised policies, or hand control to a simulated government,\n\
         and watch the effects across people, society, economy, environment,\n\
         the biosphere and governance over time. Constants are sourced in\n\
         docs/RESEARCH.md; the model is illustrative, not a forecast.\n\
         \n\
         USAGE:\n\
         \x20 simctl run [--scenario NAME] [--years N] [--government NAME] \\\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20 [--policy SPEC]... [--csv PATH]\n\
         \x20 simctl list\n\
         \x20 simctl calibrate   # Phase-4 agent engine: tune PRIMITIVES to emergent-moment targets (MSM)\n\
         \x20 simctl experiment  # Phase-4: rank two regimes on measured welfare across seeds\n\
         \x20 simctl trace  [--preset NAME] [--ticks N] [--seed S] [--csv PATH]  # record emergent series to CSV\n\
         \x20 simctl render [--preset NAME] [--ticks N] [--seed S]               # ASCII heatmap + sparklines\n\
         \x20 simctl bench  [--agents N] [--cells N] [--ticks N] [--seed S] [--threads N]  # large-population scaling benchmark\n\
         \x20 simctl help\n\
         \n\
         AGENT-ENGINE PRESETS (for trace/render): demo, fragile-commons, warming-world\n\
         \n\
         POLICY SPEC: name:start=YEAR,param=VALUE  (e.g. carbon-tax:start=2030,param=0.8)\n\
         \n\
         EXAMPLES:\n\
         \x20 simctl run --scenario baseline-2025 --government technocracy --years 75\n\
         \x20 simctl run --years 75 \\\n\
         \x20\x20\x20 --policy carbon-tax:param=0.8 \\\n\
         \x20\x20\x20 --policy education-program:param=0.03 \\\n\
         \x20\x20\x20 --policy universal-basic-income:start=2030,param=0.5 --csv run.csv"
    );
}

fn cmd_list() -> i32 {
    println!("Scenarios:");
    for name in Scenario::all_names() {
        let sc = Scenario::by_name(name).unwrap();
        println!("  {:<20} {}", sc.name, sc.description);
    }
    println!("\nGovernments (use with --government NAME):");
    for name in ArchetypeGovernment::all_names() {
        println!("  {name}");
    }
    println!("\nPolicies (use with --policy NAME:start=YEAR,param=VALUE):");
    for name in policies::all_names() {
        if let Some(p) = policies::build(name, 2025, 0.5) {
            println!("  {:<24} {}", name, p.describe());
        }
    }
    println!("\nAgent-engine presets (use with trace/render --preset NAME):");
    for name in ENGINE_PRESETS {
        println!("  {name}");
    }
    0
}

fn cmd_run(args: &[String]) -> i32 {
    let mut scenario_name = "baseline-2025".to_string();
    let mut years: u32 = 75;
    let mut government: Option<String> = None;
    let mut policy_specs: Vec<String> = Vec::new();
    let mut csv_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--scenario" | "-s" => {
                let Some(v) = args.get(i + 1) else { return arg_err("--scenario needs a value"); };
                scenario_name = v.clone();
                i += 2;
            }
            "--years" | "-y" => {
                let Some(v) = args.get(i + 1) else { return arg_err("--years needs a value"); };
                match v.parse() {
                    Ok(n) => years = n,
                    Err(_) => return arg_err(&format!("invalid --years: {v}")),
                }
                i += 2;
            }
            "--government" | "-g" => {
                let Some(v) = args.get(i + 1) else { return arg_err("--government needs a value"); };
                government = Some(v.clone());
                i += 2;
            }
            "--policy" | "-p" => {
                let Some(v) = args.get(i + 1) else { return arg_err("--policy needs a value"); };
                policy_specs.push(v.clone());
                i += 2;
            }
            "--csv" => {
                let Some(v) = args.get(i + 1) else { return arg_err("--csv needs a value"); };
                csv_path = Some(v.clone());
                i += 2;
            }
            other => return arg_err(&format!("unknown argument: {other}")),
        }
    }

    let Some(scenario) = Scenario::by_name(&scenario_name) else {
        eprintln!("unknown scenario: {scenario_name}\navailable: {}", Scenario::all_names().join(", "));
        return 2;
    };
    let start_year = scenario.start_year;
    let mut sim = Simulation::new(scenario);

    if let Some(g) = &government {
        match ArchetypeGovernment::by_name(g) {
            Some(gov) => {
                sim.set_government(Box::new(gov));
            }
            None => {
                eprintln!("unknown government: {g}\navailable: {}", ArchetypeGovernment::all_names().join(", "));
                return 2;
            }
        }
    }

    for spec in &policy_specs {
        match parse_policy(spec, start_year) {
            Ok(policy) => {
                sim.add_policy(policy);
            }
            Err(e) => {
                eprintln!("bad --policy '{spec}': {e}");
                return 2;
            }
        }
    }

    let history = sim.run(years);

    println!("scenario: {}", sim.scenario_name);
    match sim.government_name() {
        Some(g) => println!("government: {g} (endogenous — enacted {} policies)", sim.enacted_count()),
        None => {
            if sim.policies().is_empty() {
                println!("government: none; policies: (none — business as usual)");
            } else {
                println!("policies:");
                for p in sim.policies().iter() {
                    println!("  - {}", p.describe());
                }
            }
        }
    }

    if let Some(path) = csv_path {
        match write_csv(&path, &history) {
            Ok(()) => println!("wrote {} rows to {path}", history.len()),
            Err(e) => {
                eprintln!("failed to write {path}: {e}");
                return 1;
            }
        }
    } else {
        print_summary(&history);
    }
    0
}

fn arg_err(msg: &str) -> i32 {
    eprintln!("{msg}");
    2
}

fn parse_policy(spec: &str, default_start: u32) -> Result<Box<dyn society_sim::policy::Policy>, String> {
    let (name, rest) = spec.split_once(':').unwrap_or((spec, ""));
    let name = name.trim();
    let mut start = default_start;
    let mut param = 0.5_f64;
    for kv in rest.split(',').filter(|s| !s.trim().is_empty()) {
        let (k, v) = kv.trim().split_once('=').ok_or_else(|| format!("expected key=value, got '{kv}'"))?;
        match k.trim() {
            "start" => start = v.trim().parse().map_err(|_| format!("invalid start: {v}"))?,
            "param" | "strength" | "share" => {
                param = v.trim().parse().map_err(|_| format!("invalid param: {v}"))?
            }
            other => return Err(format!("unknown key '{other}'")),
        }
    }
    policies::build(name, start, param)
        .ok_or_else(|| format!("unknown policy '{name}'. available: {}", policies::all_names().join(", ")))
}

fn csv_header() -> String {
    [
        "year", "population_bn", "gdp_tn", "gdp_per_capita", "gini", "unemployment", "debt_ratio",
        "life_expectancy", "education", "health", "wellbeing", "social_support", "freedom", "livability",
        "co2_ppm", "temp_anomaly", "pollution", "forest_cover", "resource_reserves",
        "biodiversity", "wildlife_lpi", "state_capacity", "corruption", "legitimacy", "democracy",
        "polarization", "eco_score", "social_score", "prosperity_score", "governance_score", "overall_score",
    ]
    .join(",")
}

fn csv_row(s: &Snapshot) -> String {
    format!(
        "{},{:.3},{:.1},{:.0},{:.4},{:.4},{:.3},{:.1},{:.4},{:.4},{:.3},{:.4},{:.4},{:.4},\
         {:.2},{:.3},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},\
         {:.4},{:.4},{:.4},{:.4},{:.4}",
        s.year, s.human.population, s.economy.gdp, s.gdp_per_capita, s.economy.gini,
        s.economy.unemployment, s.economy.debt_ratio(), s.human.life_expectancy, s.human.education,
        s.human.health, s.society.wellbeing, s.society.social_support, s.society.freedom,
        s.society.livability, s.environment.co2_ppm, s.environment.temp_anomaly, s.environment.pollution,
        s.environment.forest_cover, s.environment.resource_reserves, s.animal.biodiversity,
        s.animal.wildlife_index, s.governance.state_capacity, s.governance.corruption,
        s.governance.legitimacy, s.governance.democracy, s.governance.polarization,
        s.planet.ecological, s.planet.social, s.planet.prosperity, s.planet.governance, s.planet.overall,
    )
}

fn write_csv(path: &str, history: &[Snapshot]) -> std::io::Result<()> {
    use std::io::Write;
    let mut out = String::with_capacity(history.len() * 220);
    out.push_str(&csv_header());
    out.push('\n');
    for s in history {
        out.push_str(&csv_row(s));
        out.push('\n');
    }
    std::fs::File::create(path)?.write_all(out.as_bytes())
}

/// `simctl calibrate` — run the agent engine's Phase-4 Method-of-Simulated-Moments
/// calibration: tune PRIMITIVES so the EMERGENT moments (wealth Gini, life
/// expectancy) approach the empirical targets. The targets live only inside the
/// loss; the world is built from primitives and the moments are measured out.
fn cmd_calibrate() -> i32 {
    use society_sim::engine::calibration as cal;
    use society_sim::engine::Primitives;

    let base = Primitives::demo();
    let targets = cal::default_targets();
    let seeds = [1u64, 7, 42];
    let ticks = 200;

    println!("Phase-4 calibration (Method of Simulated Moments)");
    println!("  targets (RHS of the loss only):");
    for t in &targets {
        println!("    {:<18} = {:.3}", t.name, t.target);
    }
    println!("  searching {} primitives over {} seeds × {} ticks ...", cal::dim(), seeds.len(), ticks);

    let result = cal::calibrate(&base, &targets, &seeds, ticks, 40, 60);

    println!("\n  loss: start {:.5}  ->  fitted {:.5}", result.initial_loss, result.loss);
    println!("  fitted primitives:");
    for (name, v) in cal::knob_names().iter().zip(result.theta.iter()) {
        println!("    {name:<18} = {v:.4}");
    }
    let m = cal::ensemble_summary(&result.primitives, &seeds, &[], ticks);
    println!("\n  EMERGENT moments at the fitted primitives (measured, never set):");
    println!("    wealth_gini       = {:.3}", m.gini);
    println!("    life_expectancy   = {:.3}", m.life_expectancy);
    println!("    population        = {:.0}", m.population);
    println!("    commons_health    = {:.3}", m.commons_health);
    0
}

/// `simctl experiment` — rank two ways of organising a fragile commons on the
/// MEASURED welfare functional (geometric mean of prosperity × equity ×
/// sustainability), across a seed ensemble.
fn cmd_experiment() -> i32 {
    use society_sim::engine::calibration as cal;
    use society_sim::engine::{HarvestQuota, OpenAccess, Primitives, Rule};

    let p = Primitives::fragile_commons();
    let seeds = [1u64, 7, 42, 100];
    let ticks = 300;

    let open = cal::Scenario::new(
        "open-access",
        p.clone(),
        vec![Box::new(OpenAccess) as Box<dyn Rule>],
    );
    let quota = cal::Scenario::new(
        "harvest-quota(0.3)",
        p.clone(),
        vec![Box::new(HarvestQuota::new(0.3)) as Box<dyn Rule>],
    );

    let (a, b, verdict) = cal::compare(&open, &quota, &seeds, ticks);
    println!("Phase-4 experiment: ranking regimes on emergent welfare");
    println!("  ({} seeds × {} ticks; welfare = geomean(prosperity·equity·sustainability·survival))\n", seeds.len(), ticks);
    for o in [&a, &b] {
        println!(
            "  {:<20} welfare {:.4}  | gini {:.3}  commons {:.3}  pop {:.0}",
            o.name,
            o.welfare,
            o.mean(|s| s.gini),
            o.mean(|s| s.commons_health),
            o.mean(|s| s.population),
        );
    }
    let winner = match verdict {
        cal::Verdict::First => a.name.as_str(),
        cal::Verdict::Second => b.name.as_str(),
        cal::Verdict::Tie => "tie",
    };
    println!("\n  verdict: {winner} has higher measured welfare");
    0
}

/// Resolve an agent-engine preset name to its [`Primitives`]. These are the
/// first-principles primitive sets the Phase-1–6 engine runs on (distinct from
/// the legacy system-dynamics `--scenario`s used by `simctl run`).
fn engine_preset(name: &str) -> Option<society_sim::engine::Primitives> {
    use society_sim::engine::Primitives;
    match name {
        "demo" => Some(Primitives::demo()),
        "fragile-commons" | "fragile_commons" => Some(Primitives::fragile_commons()),
        "warming-world" | "warming_world" => Some(Primitives::warming_world()),
        _ => None,
    }
}

const ENGINE_PRESETS: &[&str] = &["demo", "fragile-commons", "warming-world"];

/// Shared parse for the trace/render subcommands: `--preset`, `--ticks`, `--seed`
/// and (trace only) `--csv`. Returns `(primitives, ticks, csv_path)` or an exit
/// code on a bad argument.
fn parse_viz_args(args: &[String]) -> Result<(society_sim::engine::Primitives, usize, Option<String>), i32> {
    let mut preset = "demo".to_string();
    let mut ticks: usize = 200;
    let mut seed: Option<u64> = None;
    let mut csv_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--preset" | "--scenario" | "-s" => {
                let Some(v) = args.get(i + 1) else { return Err(arg_err("--preset needs a value")); };
                preset = v.clone();
                i += 2;
            }
            "--ticks" | "-t" | "--years" | "-y" => {
                let Some(v) = args.get(i + 1) else { return Err(arg_err("--ticks needs a value")); };
                ticks = v.parse().map_err(|_| arg_err(&format!("invalid --ticks: {v}")))?;
                i += 2;
            }
            "--seed" => {
                let Some(v) = args.get(i + 1) else { return Err(arg_err("--seed needs a value")); };
                seed = Some(v.parse().map_err(|_| arg_err(&format!("invalid --seed: {v}")))?);
                i += 2;
            }
            "--csv" => {
                let Some(v) = args.get(i + 1) else { return Err(arg_err("--csv needs a value")); };
                csv_path = Some(v.clone());
                i += 2;
            }
            other => return Err(arg_err(&format!("unknown argument: {other}"))),
        }
    }

    let Some(mut p) = engine_preset(&preset) else {
        eprintln!("unknown preset: {preset}\navailable: {}", ENGINE_PRESETS.join(", "));
        return Err(2);
    };
    if let Some(s) = seed {
        p.seed = s;
    }
    Ok((p, ticks, csv_path))
}

/// `simctl trace` — run the agent engine forward and record the per-tick EMERGENT
/// [`Measurements`] into a columnar [`Trace`], then print a CSV (or write it to a
/// file). Read-only consumer of the instruments; deterministic per seed.
fn cmd_trace(args: &[String]) -> i32 {
    use society_sim::engine::{trace, World};
    let (p, ticks, csv_path) = match parse_viz_args(args) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let seed = p.seed;
    let mut w = World::new(p);
    let tr = trace::record(&mut w, &[], ticks);
    let csv = tr.to_csv();
    match csv_path {
        Some(path) => match std::fs::write(&path, csv.as_bytes()) {
            Ok(()) => {
                println!("wrote {} rows (seed {seed}) to {path}", tr.len());
                0
            }
            Err(e) => {
                eprintln!("failed to write {path}: {e}");
                1
            }
        },
        None => {
            print!("{csv}");
            0
        }
    }
}

/// `simctl render` — run the agent engine forward and print an ASCII picture of
/// the run: a shaded resource heatmap, an agent-density map, and sparklines of
/// the headline emergent series. Read-only; deterministic per seed.
fn cmd_render(args: &[String]) -> i32 {
    use society_sim::engine::{trace, World};
    let (p, ticks, _csv) = match parse_viz_args(args) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let seed = p.seed;
    let mut w = World::new(p);
    let tr = trace::record(&mut w, &[], ticks);
    println!("agent-engine run: seed {seed}, {ticks} ticks\n");
    print!("{}", trace::render_run(&w, &tr));
    0
}

/// `simctl bench` — a large-population scaling benchmark (Phase 8). Builds a
/// continental-scale world (`--agents`, sized grid via `--cells`), runs it
/// forward `--ticks` ticks and reports ticks/sec and agent-ticks/sec. The
/// data-parallel substrate phase (regrowth) uses up to `--threads` workers; the
/// engine stays bit-deterministic regardless of the thread count. With
/// `--threads 1` you get the canonical single-threaded path for comparison.
fn cmd_bench(args: &[String]) -> i32 {
    use society_sim::engine::{measure, set_max_threads, Primitives, World};
    use std::time::Instant;

    let mut agents: usize = 100_000;
    let mut cells: usize = 0; // 0 → auto (≈ 4× agents, so the grid isn't packed)
    let mut ticks: usize = 20;
    let mut seed: u64 = 1;
    let mut threads: Option<usize> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--agents" | "-a" => {
                let Some(v) = args.get(i + 1) else { return arg_err("--agents needs a value"); };
                match v.parse() { Ok(n) => agents = n, Err(_) => return arg_err(&format!("invalid --agents: {v}")) }
                i += 2;
            }
            "--cells" | "-c" => {
                let Some(v) = args.get(i + 1) else { return arg_err("--cells needs a value"); };
                match v.parse() { Ok(n) => cells = n, Err(_) => return arg_err(&format!("invalid --cells: {v}")) }
                i += 2;
            }
            "--ticks" | "-t" => {
                let Some(v) = args.get(i + 1) else { return arg_err("--ticks needs a value"); };
                match v.parse() { Ok(n) => ticks = n, Err(_) => return arg_err(&format!("invalid --ticks: {v}")) }
                i += 2;
            }
            "--seed" => {
                let Some(v) = args.get(i + 1) else { return arg_err("--seed needs a value"); };
                match v.parse() { Ok(n) => seed = n, Err(_) => return arg_err(&format!("invalid --seed: {v}")) }
                i += 2;
            }
            "--threads" => {
                let Some(v) = args.get(i + 1) else { return arg_err("--threads needs a value"); };
                match v.parse() { Ok(n) => threads = Some(n), Err(_) => return arg_err(&format!("invalid --threads: {v}")) }
                i += 2;
            }
            other => return arg_err(&format!("unknown argument: {other}")),
        }
    }

    let used_threads = match threads {
        Some(n) => set_max_threads(n),
        None => society_sim::engine::max_threads(),
    };
    let cells = if cells == 0 { agents.saturating_mul(4).max(16) } else { cells };

    let mut p = Primitives::large_world(cells, agents);
    p.seed = seed;
    let (w, h) = (p.width, p.height);

    println!(
        "bench: {agents} agents on {w}x{h} = {} cells, {ticks} ticks, seed {seed}, {used_threads} worker thread(s)",
        w * h
    );

    let build_start = Instant::now();
    let mut world = World::new(p);
    let build_secs = build_start.elapsed().as_secs_f64();
    let seeded = world.agents.alive_count();
    println!("  built world in {build_secs:.3}s (seeded {seeded} agents)");

    let run_start = Instant::now();
    for _ in 0..ticks {
        world.step();
    }
    let run_secs = run_start.elapsed().as_secs_f64();

    let m = measure(&world);
    let tps = ticks as f64 / run_secs.max(1e-9);
    // Agent-ticks use the seeded population as a stable scale denominator.
    let atps = seeded as f64 * ticks as f64 / run_secs.max(1e-9);
    println!("  ran {ticks} ticks in {run_secs:.3}s");
    println!("  {tps:.2} ticks/sec, {atps:.3e} agent-ticks/sec");
    println!("  final: pop {} gini {:.3} mean_wealth {:.2}", m.population, m.wealth_gini, m.mean_wealth);
    0
}

fn print_summary(history: &[Snapshot]) {
    let (first, last) = match (history.first(), history.last()) {
        (Some(a), Some(b)) => (a, b),
        _ => return,
    };
    println!("\n{} -> {}  ({} years)\n", first.year, last.year, last.year - first.year);

    let rows: [(&str, f64, f64, bool); 16] = [
        ("overall", first.planet.overall, last.planet.overall, true),
        ("ecological", first.planet.ecological, last.planet.ecological, true),
        ("social", first.planet.social, last.planet.social, true),
        ("prosperity", first.planet.prosperity, last.planet.prosperity, true),
        ("governance", first.planet.governance, last.planet.governance, true),
        ("wellbeing /10", first.society.wellbeing, last.society.wellbeing, true),
        ("life exp (yr)", first.human.life_expectancy, last.human.life_expectancy, true),
        ("education", first.human.education, last.human.education, true),
        ("GDP/capita", first.gdp_per_capita, last.gdp_per_capita, true),
        ("gini (ineq.)", first.economy.gini, last.economy.gini, false),
        ("debt ratio", first.economy.debt_ratio(), last.economy.debt_ratio(), false),
        ("temp anom C", first.environment.temp_anomaly, last.environment.temp_anomaly, false),
        ("forest cover", first.environment.forest_cover, last.environment.forest_cover, true),
        ("biodiversity", first.animal.biodiversity, last.animal.biodiversity, true),
        ("state cap.", first.governance.state_capacity, last.governance.state_capacity, true),
        ("corruption", first.governance.corruption, last.governance.corruption, false),
    ];

    println!("{:<16} {:>12} {:>12}  trend", "metric", "start", "end");
    println!("{}", "-".repeat(56));
    for (label, a, b, higher_better) in rows {
        let improved = if higher_better { b > a } else { b < a };
        let arrow = if (b - a).abs() < 1e-6 { "  =" } else if improved { " ^+" } else { " v-" };
        println!("{label:<16} {a:>12.3} {b:>12.3} {arrow}");
    }
    println!("\n(^+ = better, v- = worse; scores in [0,1], wellbeing 0-10)");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }
    fn history(years: u32) -> Vec<Snapshot> {
        Simulation::new(Scenario::baseline_2025()).run(years)
    }
    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn help_and_list() {
        print_help();
        assert_eq!(cmd_list(), 0);
    }

    #[test]
    fn run_defaults_and_options() {
        assert_eq!(cmd_run(&[]), 0);
        let args = mk(&[
            "--scenario", "baseline-2025", "--years", "3", "--government", "technocracy",
            "--policy", "carbon-tax:param=0.5", "--policy", "education-program:start=2025,param=0.03",
        ]);
        assert_eq!(cmd_run(&args), 0);
    }

    #[test]
    fn run_csv_and_bad_path() {
        let p = tmp("simctl_run.csv");
        assert_eq!(cmd_run(&mk(&["--years", "2", "--csv", p.to_str().unwrap()])), 0);
        assert!(std::fs::read_to_string(&p).unwrap().contains("year"));
        let _ = std::fs::remove_file(&p);
        assert_eq!(cmd_run(&mk(&["--years", "2", "--csv", "/no/such/dir/x.csv"])), 1);
    }

    #[test]
    fn run_arg_errors() {
        assert_eq!(cmd_run(&mk(&["--scenario"])), 2);
        assert_eq!(cmd_run(&mk(&["--years", "x"])), 2);
        assert_eq!(cmd_run(&mk(&["--years"])), 2);
        assert_eq!(cmd_run(&mk(&["--government"])), 2);
        assert_eq!(cmd_run(&mk(&["--policy"])), 2);
        assert_eq!(cmd_run(&mk(&["--csv"])), 2);
        assert_eq!(cmd_run(&mk(&["--bogus"])), 2);
        assert_eq!(cmd_run(&mk(&["--scenario", "nope"])), 2);
        assert_eq!(cmd_run(&mk(&["--government", "nope"])), 2);
        assert_eq!(cmd_run(&mk(&["--policy", "teleport:param=1"])), 2);
    }

    #[test]
    fn parse_policy_paths() {
        assert!(parse_policy("carbon-tax:start=2030,param=0.8", 2025).is_ok());
        assert!(parse_policy("carbon-tax", 2025).is_ok());
        assert!(parse_policy("carbon-tax:strength=0.4", 2025).is_ok());
        assert!(parse_policy("teleport", 2025).is_err());
        assert!(parse_policy("carbon-tax:bad", 2025).is_err());
        assert!(parse_policy("carbon-tax:zzz=1", 2025).is_err());
        assert!(parse_policy("carbon-tax:start=xx", 2025).is_err());
        assert!(parse_policy("carbon-tax:param=xx", 2025).is_err());
    }

    #[test]
    fn csv_and_summary_helpers() {
        let h = history(3);
        let header = csv_header();
        assert!(header.starts_with("year,"));
        assert_eq!(csv_row(&h[0]).split(',').count(), header.split(',').count());
        print_summary(&h);
        print_summary(&[]);
        let p = tmp("simctl_wc.csv");
        assert!(write_csv(p.to_str().unwrap(), &h).is_ok());
        let _ = std::fs::remove_file(&p);
        assert!(write_csv("/no/such/dir/x.csv", &h).is_err());
        assert_eq!(arg_err("x"), 2);
    }

    #[test]
    fn calibrate_and_experiment() {
        assert_eq!(cmd_calibrate(), 0);
        assert_eq!(cmd_experiment(), 0);
    }

    #[test]
    fn viz_subcommands_and_errors() {
        assert_eq!(cmd_trace(&mk(&["--preset", "demo", "--ticks", "5"])), 0);
        let p = tmp("simctl_trace.csv");
        assert_eq!(
            cmd_trace(&mk(&["--preset", "warming-world", "--ticks", "5", "--seed", "3", "--csv", p.to_str().unwrap()])),
            0
        );
        let _ = std::fs::remove_file(&p);
        assert_eq!(cmd_trace(&mk(&["--ticks", "3", "--csv", "/no/such/dir/x.csv"])), 1);
        assert_eq!(cmd_render(&mk(&["--preset", "fragile-commons", "--ticks", "5"])), 0);
        assert_eq!(cmd_trace(&mk(&["--preset"])), 2);
        assert_eq!(cmd_trace(&mk(&["--ticks", "x"])), 2);
        assert_eq!(cmd_trace(&mk(&["--seed", "x"])), 2);
        assert_eq!(cmd_trace(&mk(&["--csv"])), 2);
        assert_eq!(cmd_trace(&mk(&["--bogus"])), 2);
        assert_eq!(cmd_trace(&mk(&["--preset", "nope"])), 2);
        assert!(engine_preset("nope").is_none());
        assert!(engine_preset("fragile_commons").is_some());
        assert!(engine_preset("warming_world").is_some());
    }

    #[test]
    fn bench_and_errors() {
        assert_eq!(cmd_bench(&mk(&["--agents", "300", "--cells", "1600", "--ticks", "3", "--seed", "2", "--threads", "1"])), 0);
        assert_eq!(cmd_bench(&mk(&["--agents", "200", "--ticks", "2"])), 0);
        assert_eq!(cmd_bench(&mk(&["--agents", "x"])), 2);
        assert_eq!(cmd_bench(&mk(&["--cells", "x"])), 2);
        assert_eq!(cmd_bench(&mk(&["--ticks", "x"])), 2);
        assert_eq!(cmd_bench(&mk(&["--seed", "x"])), 2);
        assert_eq!(cmd_bench(&mk(&["--threads", "x"])), 2);
        assert_eq!(cmd_bench(&mk(&["--agents"])), 2);
        assert_eq!(cmd_bench(&mk(&["--bogus"])), 2);
    }

    #[test]
    fn dispatch_routes_commands() {
        assert_eq!(dispatch(&[]), 0); // none -> help
        assert_eq!(dispatch(&mk(&["help"])), 0);
        assert_eq!(dispatch(&mk(&["list"])), 0);
        assert_eq!(dispatch(&mk(&["bogus"])), 2);
        assert_eq!(dispatch(&mk(&["run", "--years", "2"])), 0);
        assert_eq!(dispatch(&mk(&["trace", "--ticks", "3"])), 0);
        assert_eq!(dispatch(&mk(&["render", "--ticks", "3"])), 0);
        assert_eq!(dispatch(&mk(&["bench", "--agents", "200", "--ticks", "2"])), 0);
    }
}
