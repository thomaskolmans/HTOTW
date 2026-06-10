//! `worldsim` — drive the planetary simulator from the command line.
//!
//! ```text
//! worldsim run     [--archetype NAME | --file PATH] [--years N] [--seed S]
//! worldsim compare  --archetype A --archetype B [--years N] [--seeds A,B,C]
//! worldsim search  [--years N] [--seeds ..] [--generations G] [--agents N]
//! worldsim list
//! worldsim help
//! ```

use worldsim::config::{Scenario, WorldConfig};
use worldsim::measure::{Measurements, Objective};
use worldsim::search::{self, SearchConfig};
use worldsim::society;
use worldsim::world::World;

fn main() {
    std::process::exit(dispatch(&std::env::args().skip(1).collect::<Vec<_>>()));
}

fn dispatch(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("run") => cmd_run(&args[1..]),
        Some("compare") => cmd_compare(&args[1..]),
        Some("search") => cmd_search(&args[1..]),
        Some("calibrate") => cmd_calibrate(&args[1..]),
        Some("map") => cmd_map(&args[1..]),
        Some("trace") => cmd_trace(&args[1..]),
        Some("whatif") => cmd_whatif(&args[1..]),
        Some("list") => cmd_list(),
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
        "worldsim — a first-principles planetary simulator\n\
         \n\
         Physics, ecology and human psychology drive the world; the build-up of\n\
         laws, structures and institutions is a configurable INPUT; every social\n\
         outcome is MEASURED, never set; and a search finds the best-measured way\n\
         to operate the world.\n\
         \n\
         USAGE:\n\
         \x20 worldsim run     [--archetype NAME | --file PATH] [--years N] [--seed S] [--agents N]\n\
         \x20 worldsim compare  --archetype A --archetype B [--years N] [--seeds 1,2,3]\n\
         \x20 worldsim search  [--years N] [--seeds 1,2,3] [--generations G] [--agents N] [--objective NAME]\n\
         \x20 worldsim calibrate [--samples N] [--refine N] [--years N] [--seeds 1,2,3]\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20 # fit the PRIMITIVES so measured moments match documented reality (MSM)\n\
         \x20 worldsim map   [--archetype NAME | --file PATH] [--layer geo|temp|bio|pop] [--years N]\n\
         \x20 worldsim trace [--archetype NAME | --file PATH] [--years N] [--csv PATH]\n\
         \x20 worldsim whatif [--archetype NAME | --file PATH] --set key=value ... [--years N] [--seeds 1,2,3]\n\
         \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20 # mass-do/undo laws on a society and measure the same-seed delta\n\
         \x20 worldsim list\n\
         \x20 worldsim help\n\
         \n\
         A scenario --file is a .world spec ([world] + [society]/[society N] sections).\n\
         Run 'worldsim list' for the bundled archetypes."
    );
}

fn cmd_list() -> i32 {
    println!("Society archetypes (use with run/compare --archetype NAME):");
    for name in society::ARCHETYPES {
        println!("  {:<22} {}", name, society::describe(name).unwrap_or(""));
    }
    println!("\nScenario files (--file PATH) use sections:");
    println!("  [world]    seed, grid-lon, grid-lat, population, polities, fossil-endowment, patience=a..b, ...");
    println!("  [society]  property, tax-rate, tax-progressivity, transfer, *-share, carbon-price, governance, ...");
    println!("  [society N]  per-polity overrides");
    println!("\nThe hard rule: there is NO key for any social outcome (gdp, gini, temperature...).");
    println!("\nObjectives (the evaluator's VALUES, use with compare/search --objective NAME):");
    for name in Objective::PRESETS {
        println!("  {name}");
    }
    println!("  (balanced weights all four welfare pillars equally; others tilt the value judgement)");
    0
}

/// Resolve an --objective name to its weights (default balanced).
fn resolve_objective(name: &Option<String>) -> Result<Objective, i32> {
    match name {
        None => Ok(Objective::default()),
        Some(n) => Objective::preset(n).ok_or_else(|| {
            eprintln!("unknown objective: {n}\navailable: {}", Objective::PRESETS.join(", "));
            2
        }),
    }
}

/// Resolve `--archetype`/`--file` plus planet overrides into a scenario.
fn load_scenario(
    archetype: &Option<String>,
    file: &Option<String>,
    seed: Option<u64>,
    agents: Option<usize>,
    nlon: Option<usize>,
    nlat: Option<usize>,
) -> Result<Scenario, i32> {
    let apply = |mut w: WorldConfig| {
        if let Some(s) = seed {
            w.seed = s;
        }
        if let Some(a) = agents {
            w.n_agents = a;
        }
        if let Some(x) = nlon {
            w.nlon = x;
        }
        if let Some(y) = nlat {
            w.nlat = y;
        }
        w
    };
    match (archetype, file) {
        (Some(name), None) => {
            let sc = society::archetype(name, apply(WorldConfig::default())).ok_or_else(|| {
                eprintln!(
                    "unknown archetype: {name}\navailable: {}",
                    society::ARCHETYPES.join(", ")
                );
                2
            })?;
            Ok(sc)
        }
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path).map_err(|e| {
                eprintln!("cannot read {path}: {e}");
                1
            })?;
            let mut sc = Scenario::parse(&text).map_err(|e| {
                eprintln!("{path}: {e}");
                2
            })?;
            sc.world = apply(sc.world);
            // Re-fit society count if the planet's polity count changed.
            let n = sc.world.n_polities.max(1);
            if sc.societies.len() != n {
                let base = sc.societies.first().cloned().unwrap_or_default();
                sc.societies.resize(n, base);
            }
            Ok(sc)
        }
        (None, None) => {
            // Default: the null baseline planet.
            Ok(Scenario::new("baseline", apply(WorldConfig::default())))
        }
        (Some(_), Some(_)) => {
            eprintln!("pass at most one of --archetype or --file");
            Err(2)
        }
    }
}

fn parse_seeds(spec: &str) -> Result<Vec<u64>, String> {
    let v: Result<Vec<u64>, _> = spec
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().parse())
        .collect();
    match v {
        Ok(s) if !s.is_empty() => Ok(s),
        _ => Err(format!("invalid --seeds '{spec}'")),
    }
}

fn cmd_run(args: &[String]) -> i32 {
    let mut archetype = None;
    let mut file = None;
    let mut years = 200usize;
    let mut seed = None;
    let mut agents = None;
    let (mut nlon, mut nlat) = (None, None);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--archetype" | "-a" => {
                let Some(v) = args.get(i + 1) else { return ae("--archetype needs a value"); };
                archetype = Some(v.clone());
                i += 2;
            }
            "--file" | "-f" => {
                let Some(v) = args.get(i + 1) else { return ae("--file needs a value"); };
                file = Some(v.clone());
                i += 2;
            }
            "--years" | "-y" => {
                let Some(v) = args.get(i + 1) else { return ae("--years needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --years"); };
                years = n;
                i += 2;
            }
            "--seed" => {
                let Some(v) = args.get(i + 1) else { return ae("--seed needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --seed"); };
                seed = Some(n);
                i += 2;
            }
            "--agents" => {
                let Some(v) = args.get(i + 1) else { return ae("--agents needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --agents"); };
                agents = Some(n);
                i += 2;
            }
            "--grid-lon" => {
                let Some(v) = args.get(i + 1) else { return ae("--grid-lon needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --grid-lon"); };
                nlon = Some(n);
                i += 2;
            }
            "--grid-lat" => {
                let Some(v) = args.get(i + 1) else { return ae("--grid-lat needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --grid-lat"); };
                nlat = Some(n);
                i += 2;
            }
            other => return ae(&format!("unknown argument: {other}")),
        }
    }
    let scenario = match load_scenario(&archetype, &file, seed, agents, nlon, nlat) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let mut w = World::from_scenario(&scenario);
    let init = w.measure();
    println!(
        "world '{}': {} cells ({}x{}), {} polities, {} people, seed {}",
        scenario.name,
        w.planet.cells(),
        w.planet.nlon,
        w.planet.nlat,
        w.econ.len(),
        init.population,
        scenario.world.seed
    );
    println!("\n{:>5}  {:>8} {:>8} {:>6} {:>6} {:>6} {:>7} {:>6} {:>6} {:>6}",
        "year", "pop", "gdp/cap", "gini", "life", "wellb", "T+", "clean", "biodiv", "welf");
    for y in 0..years {
        w.step();
        if y % (years / 20).max(1) == 0 || y == years - 1 {
            print_row(&w.measure(), w.welfare());
        }
    }
    println!();
    summarize(&init, &w.measure(), w.welfare());
    println!(
        "  events:      {} pandemics, {} wars ({} war deaths) over {} years",
        w.pandemics_total, w.wars_total, w.war_deaths_total, years
    );
    0
}

fn print_row(m: &Measurements, welfare: f64) {
    let life = if m.life_expectancy.is_finite() { m.life_expectancy } else { 0.0 };
    println!(
        "{:>5}  {:>8} {:>8.2} {:>6.3} {:>6.1} {:>6.3} {:>6.2} {:>6.2} {:>6.2} {:>6.3}",
        m.year, m.population, m.gdp_per_capita, m.wealth_gini, life, m.wellbeing,
        m.temp_anomaly, m.clean_share, m.biodiversity, welfare
    );
}

fn summarize(init: &Measurements, end: &Measurements, welfare: f64) {
    println!("summary after {} years:", end.year);
    println!("  population   {:>8} -> {:>8}", init.population, end.population);
    println!("  gdp/capita   {:>8.2} -> {:>8.2}", init.gdp_per_capita, end.gdp_per_capita);
    println!("  wealth gini  {:>8.3} -> {:>8.3}", init.wealth_gini, end.wealth_gini);
    println!("  well-being   {:>8.3} -> {:>8.3}", init.wellbeing, end.wellbeing);
    println!("  CO2 (ppm)    {:>8.1} -> {:>8.1}", init.co2, end.co2);
    println!("  warming (K)  {:>8.2} -> {:>8.2}", init.temp_anomaly, end.temp_anomaly);
    println!("  clean share  {:>8.2} -> {:>8.2}", init.clean_share, end.clean_share);
    println!("  biodiversity {:>8.3} -> {:>8.3}", init.biodiversity, end.biodiversity);
    println!("  fossil left  {:>8.2} -> {:>8.2}", init.fossil_remaining, end.fossil_remaining);
    println!("  MEASURED WELFARE: {welfare:.4}  (geomean of well-being x equity x sustainability x survival)");
}

fn cmd_compare(args: &[String]) -> i32 {
    let mut names: Vec<String> = Vec::new();
    let mut years = 200usize;
    let mut seeds = vec![1u64, 2, 3];
    let mut agents = None;
    let mut objective_name: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--archetype" | "-a" => {
                let Some(v) = args.get(i + 1) else { return ae("--archetype needs a value"); };
                names.push(v.clone());
                i += 2;
            }
            "--objective" | "-o" => {
                let Some(v) = args.get(i + 1) else { return ae("--objective needs a value"); };
                objective_name = Some(v.clone());
                i += 2;
            }
            "--years" | "-y" => {
                let Some(v) = args.get(i + 1) else { return ae("--years needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --years"); };
                years = n;
                i += 2;
            }
            "--seeds" => {
                let Some(v) = args.get(i + 1) else { return ae("--seeds needs a value"); };
                match parse_seeds(v) { Ok(s) => seeds = s, Err(e) => return ae(&e) }
                i += 2;
            }
            "--agents" => {
                let Some(v) = args.get(i + 1) else { return ae("--agents needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --agents"); };
                agents = Some(n);
                i += 2;
            }
            other => return ae(&format!("unknown argument: {other}")),
        }
    }
    if names.len() < 2 {
        return ae("compare needs at least two --archetype NAME arguments");
    }
    let objective = match resolve_objective(&objective_name) {
        Ok(o) => o,
        Err(c) => return c,
    };
    println!(
        "comparing {} on the same planet ({} seeds x {years} years), ranked by measured welfare\n         under the '{}' objective:\n",
        names.join(" vs "),
        seeds.len(),
        objective_name.as_deref().unwrap_or("balanced")
    );
    println!("  {:<22} {:>8} {:>7} {:>7} {:>6} {:>6} {:>6}", "archetype", "welfare", "pop", "gini", "wellb", "T+", "biodiv");
    println!("  {}", "-".repeat(70));
    let mut results: Vec<(String, f64, Measurements)> = Vec::new();
    for name in &names {
        let mut welfare_sum = 0.0;
        let mut last = None;
        for &seed in &seeds {
            let mut world = WorldConfig::default();
            if let Some(a) = agents {
                world.n_agents = a;
            }
            world.seed = seed;
            let Some(sc) = society::archetype(name, world) else {
                eprintln!("unknown archetype: {name}");
                return 2;
            };
            let mut w = World::from_scenario(&sc);
            let init = w.initial_population;
            let mut acc = 0.0;
            let window = 30.min(years);
            for _ in 0..years.saturating_sub(window) {
                w.step();
            }
            for _ in 0..window {
                w.step();
                acc += w.measure().welfare_with(init, &objective);
            }
            welfare_sum += acc / window as f64;
            last = Some(w.measure());
        }
        let welfare = welfare_sum / seeds.len() as f64;
        results.push((name.clone(), welfare, last.unwrap()));
    }
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (name, welfare, m) in &results {
        println!(
            "  {:<22} {:>8.4} {:>7} {:>7.3} {:>6.3} {:>6.2} {:>6.3}",
            name, welfare, m.population, m.wealth_gini, m.wellbeing, m.temp_anomaly, m.biodiversity
        );
    }
    println!("\n  winner: {} (highest sustained measured welfare)", results[0].0);
    0
}

fn cmd_search(args: &[String]) -> i32 {
    let mut cfg = SearchConfig::default();
    let mut objective_name = "balanced".to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--years" | "-y" => {
                let Some(v) = args.get(i + 1) else { return ae("--years needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --years"); };
                cfg.years = n;
                i += 2;
            }
            "--seeds" => {
                let Some(v) = args.get(i + 1) else { return ae("--seeds needs a value"); };
                match parse_seeds(v) { Ok(s) => cfg.seeds = s, Err(e) => return ae(&e) }
                i += 2;
            }
            "--generations" | "-g" => {
                let Some(v) = args.get(i + 1) else { return ae("--generations needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --generations"); };
                cfg.generations = n;
                i += 2;
            }
            "--agents" => {
                let Some(v) = args.get(i + 1) else { return ae("--agents needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --agents"); };
                cfg.world.n_agents = n;
                i += 2;
            }
            "--mu" => {
                let Some(v) = args.get(i + 1) else { return ae("--mu needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --mu"); };
                cfg.mu = n;
                i += 2;
            }
            "--lambda" => {
                let Some(v) = args.get(i + 1) else { return ae("--lambda needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --lambda"); };
                cfg.lambda = n;
                i += 2;
            }
            "--objective" | "-o" => {
                let Some(v) = args.get(i + 1) else { return ae("--objective needs a value"); };
                match Objective::preset(v) {
                    Some(o) => cfg.objective = o,
                    None => return ae(&format!("unknown objective '{v}' (one of: {})", Objective::PRESETS.join(", "))),
                }
                objective_name = v.clone();
                i += 2;
            }
            other => return ae(&format!("unknown argument: {other}")),
        }
    }
    let baseline = search::evaluate(&worldsim::config::SocietyParams::default(), &cfg);
    println!(
        "searching the society-parameter space for the best way to operate the world\n\
         (planet {}x{}, {} people, {} seeds x {} years; {} generations of {}+{})\n",
        cfg.world.nlon, cfg.world.nlat, cfg.world.n_agents,
        cfg.seeds.len(), cfg.years, cfg.generations, cfg.mu, cfg.lambda
    );
    println!("  objective: {objective_name} (the evaluator's values)");
    println!("  null-society (do-nothing) baseline welfare: {baseline:.4}\n");
    let pop = search::search(&cfg);
    let best = &pop[0];
    println!("  BEST FOUND: welfare {:.4}  (+{:.4} over baseline)\n", best.welfare, best.welfare - baseline);
    let s = &best.params;
    println!("  property            {:?}", s.property);
    println!("  conservation-quota  {:.2}", s.conservation_quota);
    println!("  tax-rate            {:.2}  (progressivity {:.2})", s.tax_rate, s.tax_progressivity);
    println!("  transfer            {:?}", s.transfer);
    println!("  education-share     {:.2}", s.education_share);
    println!("  infrastructure      {:.2}", s.infrastructure_share);
    println!("  research-share      {:.2}", s.research_share);
    println!("  enforcement-share   {:.2}", s.enforcement_share);
    println!("  carbon-price        {:.2}", s.carbon_price);
    println!("  migration-openness  {:.2}", s.migration_openness);
    println!("  governance          {:?} (period {})", s.governance, s.vote_period);
    println!("\n  runners-up:");
    for c in pop.iter().take(4).skip(1) {
        println!(
            "    welfare {:.4}  tax {:.2}  carbon {:.2}  edu {:.2}  {:?}/{:?}",
            c.welfare, c.params.tax_rate, c.params.carbon_price, c.params.education_share,
            c.params.property, c.params.transfer
        );
    }
    0
}

/// `worldsim calibrate` — Method of Simulated Moments: tune the scale-model
/// PRIMITIVES (labour yield, birth ceiling, fossil endowment) until the
/// world's MEASURED moments (life expectancy, wealth Gini, population
/// stationarity, deprivation) land on documented pre-industrial reality. The
/// targets live only inside the loss — never written into the world.
fn cmd_calibrate(args: &[String]) -> i32 {
    use worldsim::calibrate as cal;
    let mut samples = 16usize;
    let mut refine = 8usize;
    let mut years = 80usize;
    let mut seeds = vec![1u64, 2];
    let mut agents = 1000usize;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--samples" => {
                let Some(v) = args.get(i + 1) else { return ae("--samples needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --samples"); };
                samples = n;
                i += 2;
            }
            "--refine" => {
                let Some(v) = args.get(i + 1) else { return ae("--refine needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --refine"); };
                refine = n;
                i += 2;
            }
            "--years" | "-y" => {
                let Some(v) = args.get(i + 1) else { return ae("--years needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --years"); };
                years = n;
                i += 2;
            }
            "--seeds" => {
                let Some(v) = args.get(i + 1) else { return ae("--seeds needs a value"); };
                match parse_seeds(v) { Ok(s) => seeds = s, Err(e) => return ae(&e) }
                i += 2;
            }
            "--agents" => {
                let Some(v) = args.get(i + 1) else { return ae("--agents needs a value"); };
                let Ok(n) = v.parse() else { return ae("invalid --agents"); };
                agents = n;
                i += 2;
            }
            other => return ae(&format!("unknown argument: {other}")),
        }
    }
    let mut base = WorldConfig::default();
    base.nlon = 36;
    base.nlat = 18;
    base.n_agents = agents;
    let targets = cal::preindustrial_targets();
    println!("calibrating PRIMITIVES to documented pre-industrial moments (MSM)");
    println!("  targets (right-hand side of the loss ONLY):");
    for t in &targets {
        println!("    {:<18} = {:>7.2}  (weight {})", t.name, t.value, t.weight);
    }
    println!(
        "  searching {} knobs: {} LHS samples + {} refinement passes, {} seeds x {} years\n",
        cal::KNOBS.len(), samples, refine, seeds.len(), years
    );
    let result = cal::calibrate(&base, &targets, &seeds, years, samples, refine);
    println!("  loss: start {:.4}  ->  fitted {:.4}", result.initial_loss, result.loss);
    println!("  fitted primitives:");
    for ((name, _, _), v) in cal::KNOBS.iter().zip(result.theta.iter()) {
        println!("    {name:<18} = {v:.3}");
    }
    println!("\n  EMERGENT moments at the fitted primitives (measured, never set):");
    println!("    life expectancy   = {:.1}", result.moments.life_expectancy);
    println!("    wealth gini       = {:.3}", result.moments.wealth_gini);
    println!("    population ratio  = {:.2}", result.moments.pop_ratio);
    println!("    deprivation rate  = {:.3}", result.moments.deprivation);
    0
}

/// `worldsim map` — run the world, then paint an ASCII world map of a chosen
/// per-cell layer (geography, temperature, biomass, or population density) so
/// the emergent planet is legible. Read-only.
fn cmd_map(args: &[String]) -> i32 {
    use worldsim::render::{render_map, MapLayer};
    let mut archetype = None;
    let mut file = None;
    let mut years = 200usize;
    let mut layer = MapLayer::Population;
    let mut png: Option<String> = None;
    let mut scale = 12usize;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--archetype" | "-a" => { let Some(v) = args.get(i+1) else { return ae("--archetype needs a value"); }; archetype = Some(v.clone()); i += 2; }
            "--file" | "-f" => { let Some(v) = args.get(i+1) else { return ae("--file needs a value"); }; file = Some(v.clone()); i += 2; }
            "--years" | "-y" => { let Some(v) = args.get(i+1) else { return ae("--years needs a value"); }; let Ok(n) = v.parse() else { return ae("invalid --years"); }; years = n; i += 2; }
            "--layer" | "-l" => {
                let Some(v) = args.get(i+1) else { return ae("--layer needs a value"); };
                match MapLayer::parse(v) { Some(l) => layer = l, None => return ae("layer must be geo|temp|bio|pop") }
                i += 2;
            }
            "--png" => { let Some(v) = args.get(i+1) else { return ae("--png needs a path"); }; png = Some(v.clone()); i += 2; }
            "--scale" => { let Some(v) = args.get(i+1) else { return ae("--scale needs a value"); }; let Ok(n) = v.parse() else { return ae("invalid --scale"); }; scale = n; i += 2; }
            other => return ae(&format!("unknown argument: {other}")),
        }
    }
    let scenario = match load_scenario(&archetype, &file, None, None, None, None) {
        Ok(s) => s, Err(c) => return c,
    };
    let mut w = World::from_scenario(&scenario);
    for _ in 0..years { w.step(); }
    let m = w.measure();
    println!(
        "world '{}' after {years} years — layer: {:?}\n  pop {}  warming {:.2}K  biodiversity {:.2}\n",
        scenario.name, layer, m.population, m.temp_anomaly, m.biodiversity
    );
    if let Some(path) = png {
        let bytes = worldsim::render::render_png(&w, layer, scale);
        match std::fs::write(&path, &bytes) {
            Ok(()) => { println!("wrote {}x{} px image to {path}", w.planet.nlon*scale, w.planet.nlat*scale); 0 }
            Err(e) => { eprintln!("failed to write {path}: {e}"); 1 }
        }
    } else {
        print!("{}", render_map(&w, layer));
        println!("\n  (~ ocean; ramp ' .:-=+*#%@' low→high; map is north-up)");
        0
    }
}

/// `worldsim trace` — run the world and emit the global emergent time-series as
/// CSV (stable header), or print headline sparklines if no --csv is given.
fn cmd_trace(args: &[String]) -> i32 {
    use worldsim::render::{sparkline, trace_row, TRACE_HEADER};
    let mut archetype = None;
    let mut file = None;
    let mut years = 250usize;
    let mut csv: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--archetype" | "-a" => { let Some(v) = args.get(i+1) else { return ae("--archetype needs a value"); }; archetype = Some(v.clone()); i += 2; }
            "--file" | "-f" => { let Some(v) = args.get(i+1) else { return ae("--file needs a value"); }; file = Some(v.clone()); i += 2; }
            "--years" | "-y" => { let Some(v) = args.get(i+1) else { return ae("--years needs a value"); }; let Ok(n) = v.parse() else { return ae("invalid --years"); }; years = n; i += 2; }
            "--csv" => { let Some(v) = args.get(i+1) else { return ae("--csv needs a value"); }; csv = Some(v.clone()); i += 2; }
            other => return ae(&format!("unknown argument: {other}")),
        }
    }
    let scenario = match load_scenario(&archetype, &file, None, None, None, None) {
        Ok(s) => s, Err(c) => return c,
    };
    let mut w = World::from_scenario(&scenario);
    let mut rows = vec![trace_row(&w.measure())];
    let mut series: Vec<Measurements> = vec![w.measure()];
    for _ in 0..years {
        w.step();
        rows.push(trace_row(&w.measure()));
        series.push(w.measure());
    }
    match csv {
        Some(path) => {
            let body = format!("{TRACE_HEADER}\n{}\n", rows.join("\n"));
            match std::fs::write(&path, body) {
                Ok(()) => { println!("wrote {} rows to {path}", rows.len()); 0 }
                Err(e) => { eprintln!("failed to write {path}: {e}"); 1 }
            }
        }
        None => {
            let col = |f: fn(&Measurements) -> f64| series.iter().map(f).collect::<Vec<_>>();
            println!("world '{}' — {years} years of emergent history:\n", scenario.name);
            println!("  population     {}", sparkline(&col(|m| m.population as f64)));
            println!("  gdp/capita     {}", sparkline(&col(|m| m.gdp_per_capita)));
            println!("  wealth gini    {}", sparkline(&col(|m| m.wealth_gini)));
            println!("  wellbeing      {}", sparkline(&col(|m| m.wellbeing)));
            println!("  warming (K)    {}", sparkline(&col(|m| m.temp_anomaly)));
            println!("  clean share    {}", sparkline(&col(|m| m.clean_share)));
            println!("  biodiversity   {}", sparkline(&col(|m| m.biodiversity)));
            let last = series.last().unwrap();
            println!(
                "\n  final: pop {}  gini {:.2}  warming {:.2}K  biodiversity {:.2}",
                last.population, last.wealth_gini, last.temp_anomaly, last.biodiversity
            );
            0
        }
    }
}

/// `worldsim whatif` — the planetary counterfactual: take a society, apply one
/// or more `--set key=value` law edits (mass-do or undo any institution), and
/// run the BASE and the EDITED society on the *same seeds and the same planet*,
/// so every measured difference is attributable to the edits and nothing else.
/// This is "input an existing society and see how the world would work if you
/// changed its laws", at full scale.
fn cmd_whatif(args: &[String]) -> i32 {
    use worldsim::config::apply_society_edit;
    let mut archetype = None;
    let mut file = None;
    let mut years = 250usize;
    let mut seeds = vec![1u64, 2, 3];
    let mut edits: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--archetype" | "-a" => { let Some(v) = args.get(i+1) else { return ae("--archetype needs a value"); }; archetype = Some(v.clone()); i += 2; }
            "--file" | "-f" => { let Some(v) = args.get(i+1) else { return ae("--file needs a value"); }; file = Some(v.clone()); i += 2; }
            "--years" | "-y" => { let Some(v) = args.get(i+1) else { return ae("--years needs a value"); }; let Ok(n) = v.parse() else { return ae("invalid --years"); }; years = n; i += 2; }
            "--seeds" => { let Some(v) = args.get(i+1) else { return ae("--seeds needs a value"); }; match parse_seeds(v) { Ok(x) => seeds = x, Err(e) => return ae(&e) } i += 2; }
            "--set" => {
                let Some(v) = args.get(i+1) else { return ae("--set needs key=value"); };
                let Some((k, val)) = v.split_once('=') else { return ae(&format!("--set wants key=value, got '{v}'")); };
                edits.push((k.trim().to_string(), val.trim().to_string()));
                i += 2;
            }
            other => return ae(&format!("unknown argument: {other}")),
        }
    }
    if edits.is_empty() {
        return ae("whatif needs at least one --set key=value edit");
    }
    let base = match load_scenario(&archetype, &file, None, None, None, None) {
        Ok(s) => s, Err(c) => return c,
    };
    // Build the edited scenario by applying every edit to every polity.
    let mut edited = base.clone();
    edited.name = format!("{} (edited)", base.name);
    for soc in &mut edited.societies {
        for (k, v) in &edits {
            if let Err(e) = apply_society_edit(soc, k, v) {
                return ae(&format!("bad --set {k}={v}: {e}"));
            }
        }
    }

    // Run both on the same seeds; average the final-window measurements.
    let eval = |scenario: &Scenario| -> (Measurements, f64) {
        let window = 30.min(years);
        let mut sums = [0.0_f64; 7];
        let mut welfare = 0.0;
        let mut last = None;
        for &seed in &seeds {
            let mut sc = scenario.clone();
            sc.world.seed = seed;
            let mut w = World::from_scenario(&sc);
            let init = w.initial_population;
            for _ in 0..years.saturating_sub(window) { w.step(); }
            let mut acc = [0.0_f64; 7];
            let mut wf = 0.0;
            for _ in 0..window {
                w.step();
                let m = w.measure();
                acc[0] += m.population as f64;
                acc[1] += m.wealth_gini;
                acc[2] += m.wellbeing;
                acc[3] += m.temp_anomaly;
                acc[4] += m.biodiversity;
                acc[5] += m.gdp_per_capita;
                acc[6] += m.deprivation_rate;
                wf += m.welfare(init);
            }
            for k in 0..7 { sums[k] += acc[k] / window as f64; }
            welfare += wf / window as f64;
            last = Some(w.measure());
        }
        let n = seeds.len() as f64;
        let mut m = last.unwrap();
        m.population = (sums[0] / n) as usize;
        m.wealth_gini = sums[1] / n;
        m.wellbeing = sums[2] / n;
        m.temp_anomaly = sums[3] / n;
        m.biodiversity = sums[4] / n;
        m.gdp_per_capita = sums[5] / n;
        m.deprivation_rate = sums[6] / n;
        (m, welfare / n)
    };

    let (mb, wb) = eval(&base);
    let (me, we) = eval(&edited);
    println!("what-if on '{}' ({} seeds x {years} years, same planet in both arms):", base.name, seeds.len());
    println!("  edits: {}", edits.iter().map(|(k,v)| format!("{k}={v}")).collect::<Vec<_>>().join(", "));
    println!("\n  {:<16} {:>10} {:>10} {:>10}", "metric", "base", "edited", "delta");
    println!("  {}", "-".repeat(50));
    let row = |label: &str, a: f64, b: f64| println!("  {label:<16} {a:>10.3} {b:>10.3} {:>+10.3}", b - a);
    row("welfare", wb, we);
    row("population", mb.population as f64, me.population as f64);
    row("gdp/capita", mb.gdp_per_capita, me.gdp_per_capita);
    row("wealth gini", mb.wealth_gini, me.wealth_gini);
    row("wellbeing", mb.wellbeing, me.wellbeing);
    row("warming (K)", mb.temp_anomaly, me.temp_anomaly);
    row("biodiversity", mb.biodiversity, me.biodiversity);
    row("deprivation", mb.deprivation_rate, me.deprivation_rate);
    let verdict = if we > wb { "the EDITED society scores higher measured welfare" }
        else if wb > we { "the society as-is scores higher measured welfare" }
        else { "a tie on measured welfare" };
    println!("\n  verdict: {verdict}");
    0
}

fn ae(msg: &str) -> i32 {
    eprintln!("{msg}");
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn help_and_list() {
        assert_eq!(dispatch(&mk(&["help"])), 0);
        assert_eq!(dispatch(&mk(&["list"])), 0);
        assert_eq!(dispatch(&[]), 0);
        assert_eq!(dispatch(&mk(&["bogus"])), 2);
    }

    #[test]
    fn run_archetype_and_baseline() {
        assert_eq!(cmd_run(&mk(&["--years", "20", "--agents", "500", "--grid-lon", "24", "--grid-lat", "12"])), 0);
        assert_eq!(
            cmd_run(&mk(&["--archetype", "social-democracy", "--years", "20", "--agents", "500", "--grid-lon", "24", "--grid-lat", "12"])),
            0
        );
        assert_eq!(cmd_run(&mk(&["--archetype", "atlantis"])), 2);
        assert_eq!(cmd_run(&mk(&["--years", "x"])), 2);
        assert_eq!(cmd_run(&mk(&["--bogus"])), 2);
    }

    #[test]
    fn run_from_file() {
        let p = std::env::temp_dir().join("worldsim_test.world");
        std::fs::write(
            &p,
            "name = t\n[world]\ngrid-lon = 24\ngrid-lat = 12\npopulation = 500\n[society]\ntax-rate = 0.1\n",
        )
        .unwrap();
        assert_eq!(cmd_run(&mk(&["--file", p.to_str().unwrap(), "--years", "15"])), 0);
        let _ = std::fs::remove_file(&p);
        assert_eq!(cmd_run(&mk(&["--file", "/no/such/file.world"])), 1);
        assert_eq!(cmd_run(&mk(&["--archetype", "laissez-faire", "--file", "x"])), 2);
    }

    #[test]
    fn compare_archetypes() {
        assert_eq!(
            cmd_compare(&mk(&[
                "--archetype", "laissez-faire", "--archetype", "social-democracy",
                "--years", "25", "--seeds", "1", "--agents", "500",
            ])),
            0
        );
        assert_eq!(cmd_compare(&mk(&["--archetype", "laissez-faire"])), 2); // needs two
        assert_eq!(cmd_compare(&mk(&["--bogus"])), 2);
    }

    #[test]
    fn search_runs_small() {
        assert_eq!(
            cmd_search(&mk(&[
                "--years", "40", "--seeds", "1", "--generations", "2",
                "--agents", "400", "--mu", "3", "--lambda", "5",
            ])),
            0
        );
    }

    #[test]
    fn calibrate_runs_small() {
        assert_eq!(
            cmd_calibrate(&mk(&["--samples", "4", "--refine", "2", "--years", "30", "--seeds", "1", "--agents", "400"])),
            0
        );
        assert_eq!(cmd_calibrate(&mk(&["--samples", "x"])), 2);
    }

    #[test]
    fn map_and_trace_commands() {
        for layer in ["geo", "temp", "bio", "pop"] {
            assert_eq!(cmd_map(&mk(&["--file", &tiny_world_file(), "--layer", layer, "--years", "10"])), 0);
        }
        assert_eq!(cmd_map(&mk(&["--file", &tiny_world_file(), "--layer", "bogus"])), 2);
        // Trace to stdout (sparklines) and to CSV.
        assert_eq!(cmd_trace(&mk(&["--file", &tiny_world_file(), "--years", "15"])), 0);
        let p = std::env::temp_dir().join("worldsim_trace.csv");
        assert_eq!(cmd_trace(&mk(&["--file", &tiny_world_file(), "--years", "10", "--csv", p.to_str().unwrap()])), 0);
        let body = std::fs::read_to_string(&p).unwrap();
        assert!(body.starts_with("year,population,"));
        assert_eq!(body.lines().count(), 12); // header + 11 rows (0..=10)
        let _ = std::fs::remove_file(&p);
        assert_eq!(cmd_trace(&mk(&["--csv", "/no/such/dir/x.csv", "--file", &tiny_world_file(), "--years", "3"])), 1);
    }

    fn tiny_world_file() -> String {
        let p = std::env::temp_dir().join("worldsim_tiny.world");
        std::fs::write(&p, "name = tiny\n[world]\ngrid-lon = 24\ngrid-lat = 12\npopulation = 500\n").unwrap();
        p.to_str().unwrap().to_string()
    }

    #[test]
    fn whatif_counterfactual() {
        // Apply a redistribution edit to a society and measure the delta.
        assert_eq!(
            cmd_whatif(&mk(&[
                "--file", &tiny_world_file(), "--set", "tax-rate=0.3",
                "--set", "transfer=floor", "--years", "40", "--seeds", "1,2",
            ])),
            0
        );
        // Errors.
        assert_eq!(cmd_whatif(&mk(&["--file", &tiny_world_file()])), 2); // no edits
        assert_eq!(cmd_whatif(&mk(&["--file", &tiny_world_file(), "--set", "tax-rate"])), 2); // no =
        assert_eq!(cmd_whatif(&mk(&["--file", &tiny_world_file(), "--set", "gdp=5"])), 2); // outcome key
        assert_eq!(cmd_whatif(&mk(&["--set", "tax-rate=0.1", "--archetype", "atlantis"])), 2);
    }

    #[test]
    fn dispatch_routes() {
        assert_eq!(dispatch(&mk(&["run", "--years", "10", "--agents", "400", "--grid-lon", "24", "--grid-lat", "12"])), 0);
        assert_eq!(dispatch(&mk(&["map", "--file", &tiny_world_file(), "--years", "5"])), 0);
        assert_eq!(dispatch(&mk(&["trace", "--file", &tiny_world_file(), "--years", "5"])), 0);
        assert_eq!(dispatch(&mk(&["whatif", "--file", &tiny_world_file(), "--set", "carbon-price=3", "--years", "20", "--seeds", "1"])), 0);
    }
}
