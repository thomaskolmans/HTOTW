//! `worldviz` — run the planetary simulator **interactively in your browser**.
//!
//! A dependency-free, std-only HTTP server runs the engine natively (full
//! speed) and serves a self-contained HTML/Canvas page. You get the planet
//! rendered live (geography / temperature / biomass / population layers), a
//! control panel with **every society and planet input** (laws, taxes,
//! transfers, carbon price, governance, psychology ranges, endowments), play /
//! pause / step / reset, and live charts of the emergent measurements. Change
//! a law mid-run and watch the world respond — every number shown is measured,
//! never set.
//!
//! ```sh
//! cargo run --release --bin worldviz            # http://127.0.0.1:8088
//! cargo run --release --bin worldviz -- --port 9000
//! ```

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use worldsim::config::{apply_society_edit, Scenario, SocietyParams, WorldConfig};
use worldsim::measure::Measurements;
use worldsim::society;
use worldsim::world::World;

/// Everything the browser can change, kept alongside the running world.
struct Session {
    world: World,
    scenario: Scenario,
    /// Rolling emergent history (one entry per simulated year).
    history: Vec<Measurements>,
}

impl Session {
    fn new(scenario: Scenario) -> Session {
        let world = World::from_scenario(&scenario);
        let mut s = Session { world, scenario, history: Vec::new() };
        s.history.push(s.world.measure());
        s
    }

    fn reset(&mut self) {
        self.world = World::from_scenario(&self.scenario);
        self.history.clear();
        self.history.push(self.world.measure());
    }

    fn step(&mut self, years: usize) {
        for _ in 0..years {
            self.world.step();
            self.history.push(self.world.measure());
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut port: u16 = 8088;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                if let Some(v) = args.get(i + 1).and_then(|v| v.parse().ok()) {
                    port = v;
                }
                i += 2;
            }
            _ => i += 1,
        }
    }

    let mut cfg = WorldConfig::default();
    cfg.nlon = 72;
    cfg.nlat = 36;
    cfg.n_agents = 3000;
    cfg.n_polities = 6;
    let session = Arc::new(Mutex::new(Session::new(
        society::archetype("social-democracy", cfg).expect("bundled archetype"),
    )));

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot bind 127.0.0.1:{port}: {e}");
            std::process::exit(1);
        }
    };
    println!("worldviz: open http://127.0.0.1:{port} in your browser  (Ctrl-C to stop)");

    for stream in listener.incoming().flatten() {
        let session = Arc::clone(&session);
        // One thread per request: requests are short; the engine lock keeps
        // the simulation consistent.
        std::thread::spawn(move || {
            let _ = handle(stream, &session);
        });
    }
}

fn handle(mut stream: TcpStream, session: &Mutex<Session>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    // Read headers (for Content-Length on POST).
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let l = line.trim();
        if l.is_empty() {
            break;
        }
        if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body = String::from_utf8_lossy(&body).to_string();

    let (path, query) = match path.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path, ""),
    };

    match (method, path) {
        ("GET", "/") => respond(&mut stream, 200, "text/html; charset=utf-8", PAGE),
        ("GET", "/api/state") => {
            let s = session.lock().unwrap_or_else(|e| e.into_inner());
            respond(&mut stream, 200, "application/json", &state_json(&s))
        }
        ("POST", "/api/step") => {
            let years = query_num(query, "years").unwrap_or(1.0) as usize;
            let mut s = session.lock().unwrap_or_else(|e| e.into_inner());
            s.step(years.clamp(1, 100));
            respond(&mut stream, 200, "application/json", &state_json(&s))
        }
        ("POST", "/api/reset") => {
            let mut s = session.lock().unwrap_or_else(|e| e.into_inner());
            s.reset();
            respond(&mut stream, 200, "application/json", &state_json(&s))
        }
        // Apply society edits (key=value lines in the body) to every polity,
        // mid-run: a law changed while the world turns.
        ("POST", "/api/society") => {
            let mut s = session.lock().unwrap_or_else(|e| e.into_inner());
            let mut errors = Vec::new();
            for line in body.lines().filter(|l| !l.trim().is_empty()) {
                if let Some((k, v)) = line.split_once('=') {
                    let (k, v) = (k.trim().to_string(), v.trim().to_string());
                    for soc in &mut s.scenario.societies {
                        if let Err(e) = apply_society_edit(soc, &k, &v) {
                            errors.push(e);
                            break;
                        }
                    }
                    // Live world too (laws change without a reset).
                    let socs: Vec<SocietyParams> = s.scenario.societies.clone();
                    s.world.society = socs;
                } else {
                    errors.push(format!("expected key=value, got '{line}'"));
                }
            }
            if errors.is_empty() {
                respond(&mut stream, 200, "application/json", &state_json(&s))
            } else {
                respond(&mut stream, 400, "application/json", &format!("{{\"error\":{}}}", json_str(&errors.join("; "))))
            }
        }
        // Rebuild the planet from new world primitives (requires reset).
        ("POST", "/api/world") => {
            let mut s = session.lock().unwrap_or_else(|e| e.into_inner());
            let mut cfg = s.scenario.world.clone();
            let mut err = None;
            for line in body.lines().filter(|l| !l.trim().is_empty()) {
                let Some((k, v)) = line.split_once('=') else {
                    err = Some(format!("expected key=value, got '{line}'"));
                    break;
                };
                if let Err(e) = set_world_field(&mut cfg, k.trim(), v.trim()) {
                    err = Some(e);
                    break;
                }
            }
            match err {
                None => {
                    s.scenario.world = cfg;
                    let n = s.scenario.world.n_polities.max(1);
                    let base = s.scenario.societies.first().cloned().unwrap_or_default();
                    s.scenario.societies.resize(n, base);
                    s.reset();
                    respond(&mut stream, 200, "application/json", &state_json(&s))
                }
                Some(e) => respond(&mut stream, 400, "application/json", &format!("{{\"error\":{}}}", json_str(&e))),
            }
        }
        ("POST", "/api/archetype") => {
            let name = body.trim();
            let mut s = session.lock().unwrap_or_else(|e| e.into_inner());
            match society::archetype(name, s.scenario.world.clone()) {
                Some(sc) => {
                    s.scenario = sc;
                    s.reset();
                    respond(&mut stream, 200, "application/json", &state_json(&s))
                }
                None => respond(&mut stream, 400, "application/json", "{\"error\":\"unknown archetype\"}"),
            }
        }
        _ => respond(&mut stream, 404, "text/plain", "not found"),
    }
}

/// Minimal world-primitive setter for the UI (a safe subset; the .world file
/// format remains the full interface). Only primitives — no outcome exists.
fn set_world_field(cfg: &mut WorldConfig, key: &str, value: &str) -> Result<(), String> {
    let num = |v: &str| v.parse::<f64>().map_err(|_| format!("invalid number '{v}' for '{key}'"));
    match key {
        "seed" => cfg.seed = num(value)? as u64,
        "population" => cfg.n_agents = num(value)? as usize,
        "polities" => cfg.n_polities = (num(value)? as usize).max(1),
        "fossil-endowment" => cfg.fossil_endowment = num(value)?,
        "base-yield" => cfg.base_yield = num(value)?.clamp(2.0, 12.0),
        "birth-ceiling" => cfg.birth_ceiling = num(value)?.clamp(0.15, 0.5),
        "patience-min" => cfg.patience.0 = num(value)?.clamp(0.0, 1.0),
        "patience-max" => cfg.patience.1 = num(value)?.clamp(0.0, 1.0),
        "fairness-min" => cfg.fairness.0 = num(value)?.clamp(0.0, 1.0),
        "fairness-max" => cfg.fairness.1 = num(value)?.clamp(0.0, 1.0),
        other => return Err(format!("unknown world key '{other}'")),
    }
    Ok(())
}

fn query_num(query: &str, key: &str) -> Option<f64> {
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == key)
        .and_then(|(_, v)| v.parse().ok())
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Serialise the full UI state: planet fields for the map, the society dials,
/// the emergent history. Hand-rolled JSON (dependency-free).
fn state_json(s: &Session) -> String {
    let w = &s.world;
    let p = &w.planet;

    // Per-cell layers, quantised to keep the payload small.
    let n = p.cells();
    let mut land = String::with_capacity(n);
    for i in 0..n {
        land.push(if p.is_land[i] { '1' } else { '0' });
    }
    let q = |f: &dyn Fn(usize) -> f64| -> String {
        // 0..9 quantisation as a digit string.
        (0..n)
            .map(|i| {
                let v = f(i).clamp(0.0, 1.0);
                char::from_digit((v * 9.0).round() as u32, 10).unwrap_or('0')
            })
            .collect()
    };
    let (mut tmin, mut tmax) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..n {
        tmin = tmin.min(p.temp[i]);
        tmax = tmax.max(p.temp[i]);
    }
    let trange = (tmax - tmin).max(1e-6);
    let temp_layer = q(&|i: usize| (p.temp[i] - tmin) / trange);
    let bio_layer = q(&|i: usize| {
        if p.biomass_k0[i] > 0.0 { p.biomass[i] / p.biomass_k0[i] } else { 0.0 }
    });
    let mut pop_cells = vec![0u32; n];
    for i in 0..w.people.len() {
        if w.people.alive[i] {
            pop_cells[w.people.cell[i]] += 1;
        }
    }
    let pop_max = pop_cells.iter().copied().max().unwrap_or(1).max(1) as f64;
    let pop_layer = q(&|i: usize| pop_cells[i] as f64 / pop_max);
    let elev_layer = q(&|i: usize| (p.elevation[i] / 4.0).min(1.0));
    let mut polity_layer = String::with_capacity(n);
    for i in 0..n {
        let v = w.polity_of_cell[i];
        polity_layer.push(if v == u16::MAX {
            '~'
        } else {
            char::from_digit((v % 10) as u32, 10).unwrap()
        });
    }

    // Society dials of polity 0 (the UI edits all polities uniformly).
    let soc = &w.society[0];
    let society = format!(
        "{{\"property\":{},\"conservation-quota\":{:.2},\"tax-rate\":{:.2},\"tax-progressivity\":{:.2},\
          \"transfer\":{},\"education-share\":{:.2},\"infrastructure-share\":{:.2},\"research-share\":{:.2},\
          \"enforcement-share\":{:.2},\"carbon-price\":{:.2},\"migration-openness\":{:.2},\"trade-openness\":{:.2},\
          \"governance\":{},\"vote-period\":{}}}",
        json_str(match soc.property {
            worldsim::config::PropertyRegime::OpenAccess => "open-access",
            worldsim::config::PropertyRegime::CommonsQuota => "commons-quota",
            worldsim::config::PropertyRegime::Private => "private",
        }),
        soc.conservation_quota,
        soc.tax_rate,
        soc.tax_progressivity,
        json_str(match soc.transfer {
            worldsim::config::TransferRegime::None => "none",
            worldsim::config::TransferRegime::Floor => "floor",
            worldsim::config::TransferRegime::UniversalDividend => "universal-dividend",
        }),
        soc.education_share,
        soc.infrastructure_share,
        soc.research_share,
        soc.enforcement_share,
        soc.carbon_price,
        soc.migration_openness,
        soc.trade_openness,
        json_str(match soc.governance {
            worldsim::config::GovernanceRegime::Fixed => "fixed",
            worldsim::config::GovernanceRegime::Majority => "majority",
            worldsim::config::GovernanceRegime::WealthWeighted => "wealth-weighted",
        }),
        soc.vote_period,
    );

    let cfg = &s.scenario.world;
    let worldcfg = format!(
        "{{\"seed\":{},\"population\":{},\"polities\":{},\"fossil-endowment\":{:.1},\
          \"base-yield\":{:.2},\"birth-ceiling\":{:.2},\"patience-min\":{:.2},\"patience-max\":{:.2},\
          \"fairness-min\":{:.2},\"fairness-max\":{:.2}}}",
        cfg.seed, cfg.n_agents, cfg.n_polities, cfg.fossil_endowment, cfg.base_yield,
        cfg.birth_ceiling, cfg.patience.0, cfg.patience.1, cfg.fairness.0, cfg.fairness.1,
    );

    // History series (one row per year).
    let m_row = |m: &Measurements| {
        let life = if m.life_expectancy.is_finite() { m.life_expectancy } else { 0.0 };
        format!(
            "[{},{},{:.4},{:.4},{:.2},{:.4},{:.4},{:.2},{:.3},{:.4},{:.4},{:.4}]",
            m.year, m.population, m.gdp_per_capita, m.wealth_gini, life, m.wellbeing,
            m.deprivation_rate, m.co2, m.temp_anomaly, m.clean_share, m.biodiversity,
            m.welfare(w.initial_population),
        )
    };
    let history: Vec<String> = s.history.iter().map(m_row).collect();

    format!(
        "{{\"nlon\":{},\"nlat\":{},\"year\":{},\"name\":{},\
          \"land\":{},\"temp\":{},\"bio\":{},\"pop\":{},\"elev\":{},\"polity\":{},\
          \"society\":{society},\"world\":{worldcfg},\
          \"pandemics\":{},\"wars\":{},\"war_deaths\":{},\
          \"history\":[{}],\
          \"cols\":[\"year\",\"population\",\"gdp/capita\",\"gini\",\"life expectancy\",\"wellbeing\",\"deprivation\",\"co2 ppm\",\"warming K\",\"clean share\",\"biodiversity\",\"welfare\"]}}",
        p.nlon,
        p.nlat,
        w.year,
        json_str(&s.scenario.name),
        json_str(&land),
        json_str(&temp_layer),
        json_str(&bio_layer),
        json_str(&pop_layer),
        json_str(&elev_layer),
        json_str(&polity_layer),
        w.pandemics_total,
        w.wars_total,
        w.war_deaths_total,
        history.join(","),
    )
}

fn respond(stream: &mut TcpStream, code: u16, ctype: &str, body: &str) -> std::io::Result<()> {
    let status = match code {
        200 => "200 OK",
        400 => "400 Bad Request",
        _ => "404 Not Found",
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body.as_bytes())
}

/// The whole UI, embedded: HTML + CSS + Canvas JS, no external assets.
const PAGE: &str = include_str!("worldviz.html");
