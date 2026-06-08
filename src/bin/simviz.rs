//! `simviz` — an interactive, cross-platform visualizer for the society-physics
//! engine.
//!
//! It runs the agent-based engine **natively** (full speed, big populations) and
//! serves an interactive UI in your **browser**, so it works identically on
//! Linux, Windows and macOS with nothing to install beyond the binary itself.
//! The whole thing is **dependency-free** — a tiny std-only HTTP server plus a
//! self-contained HTML/Canvas page embedded below.
//!
//! ## Run it
//!
//! ```sh
//! cargo run --bin simviz            # then open the printed http://127.0.0.1:8080
//! cargo run --bin simviz -- --port 9000 --no-open
//! ```
//!
//! The browser drives the simulation: it asks the server to advance a few ticks
//! at a time and renders each returned frame. The server holds one live `World`
//! session. Controls let you pick a preset, reseed, nudge a few **primitives**,
//! toggle **policy rules**, and watch the **emergent** measurements respond live
//! — nothing macro is ever set, only measured (the engine's hard rule).
//!
//! Protocol (all GET, responses are hand-built JSON; no external crates):
//! * `GET /`                              → the UI page
//! * `GET /reset?preset=..&seed=..&...`   → rebuild the world, return a frame
//! * `GET /step?n=K`                      → advance K ticks, return a frame
//! * `GET /frame`                         → current frame, no advance

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use society_sim::engine::institutions::{
    CorruptOfficial, Decarbonize, HarvestQuota, PropertyRights, Redistribute, Rule, WealthTax,
};
use society_sim::engine::instruments::measure;
use society_sim::engine::world::{Primitives, World, NGOODS};

fn main() {
    let mut port: u16 = 8080;
    let mut open = true;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" | "-p" => {
                if let Some(v) = args.next() {
                    port = v.parse().unwrap_or(8080);
                }
            }
            "--no-open" => open = false,
            "--help" | "-h" => {
                println!("simviz — interactive browser visualizer for the society-physics engine\n\nUSAGE:\n  simviz [--port N] [--no-open]\n\nThen open the printed URL in any browser (Linux/Windows/macOS).");
                return;
            }
            _ => {}
        }
    }

    // Bind, trying a few ports if the first is taken.
    let listener = (port..port + 20)
        .find_map(|p| TcpListener::bind(("127.0.0.1", p)).ok().map(|l| (l, p)));
    let (listener, port) = match listener {
        Some(x) => x,
        None => {
            eprintln!("could not bind any port in {port}..{}", port + 20);
            std::process::exit(1);
        }
    };
    let url = format!("http://127.0.0.1:{port}");
    println!("society-sim :: simviz");
    println!("  serving the interactive visualizer at {url}");
    println!("  open that URL in any browser (Ctrl-C to stop)");
    if open {
        open_browser(&url);
    }

    // One live session, mutated in the single-threaded accept loop. The browser
    // makes sequential requests, so no locking is needed.
    let mut session = Session::new("demo", 1);

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if let Err(e) = handle(stream, &mut session) {
            // Broken pipe / browser closed a connection — not fatal.
            let _ = e;
        }
    }
}

/// A live simulation session: the world plus the rules currently in force.
struct Session {
    world: World,
    rules: Vec<Box<dyn Rule>>,
    preset: String,
}

impl Session {
    fn new(preset: &str, seed: u64) -> Session {
        let mut p = preset_primitives(preset);
        p.seed = seed;
        Session {
            world: World::new(p),
            rules: Vec::new(),
            preset: preset.to_string(),
        }
    }
}

fn preset_primitives(name: &str) -> Primitives {
    match name {
        "fragile-commons" | "fragile" => Primitives::fragile_commons(),
        "warming-world" | "warming" => Primitives::warming_world(),
        _ => Primitives::demo(),
    }
}

// ---------------------------------------------------------------------------
// HTTP handling (std-only)
// ---------------------------------------------------------------------------

fn handle(mut stream: TcpStream, session: &mut Session) -> std::io::Result<()> {
    let mut buf = [0u8; 16384];
    let n = stream.read(&mut buf)?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let first = req.lines().next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let _method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));

    match path {
        "/" => respond(&mut stream, "200 OK", "text/html; charset=utf-8", INDEX_HTML.as_bytes()),
        "/reset" => {
            let q = parse_query(query);
            let preset = get(&q, "preset").unwrap_or("demo").to_string();
            let seed = getu(&q, "seed").unwrap_or(1);
            let mut p = preset_primitives(&preset);
            p.seed = seed;
            // A handful of primitives are exposed as sliders.
            if let Some(v) = getf(&q, "n_agents") {
                p.n_agents = v as usize;
            }
            if let Some(v) = getf(&q, "peak_capacity") {
                p.peak_capacity = v;
            }
            if let Some(v) = getf(&q, "regrowth_rate") {
                p.regrowth_rate = v;
            }
            if let Some(v) = getf(&q, "metabolism_max") {
                p.metabolism_max = v.max(p.metabolism_min);
            }
            if let Some(v) = getf(&q, "birth_threshold") {
                p.birth_threshold = v;
            }
            session.world = World::new(p);
            session.rules = build_rules(get(&q, "rules").unwrap_or(""));
            session.preset = preset;
            respond_json(&mut stream, &frame_json(session))
        }
        "/step" => {
            let q = parse_query(query);
            let steps = getf(&q, "n").map(|v| v as usize).unwrap_or(1).clamp(1, 100);
            for _ in 0..steps {
                session.world.step_with_rules(&session.rules);
            }
            respond_json(&mut stream, &frame_json(session))
        }
        "/frame" => respond_json(&mut stream, &frame_json(session)),
        _ => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
    }
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn respond_json(stream: &mut TcpStream, body: &str) -> std::io::Result<()> {
    respond(stream, "200 OK", "application/json", body.as_bytes())
}

// ---------------------------------------------------------------------------
// Query parsing
// ---------------------------------------------------------------------------

fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|s| !s.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (k.to_string(), percent_decode(v))
        })
        .collect()
}

fn get<'a>(q: &'a [(String, String)], key: &str) -> Option<&'a str> {
    q.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}
fn getf(q: &[(String, String)], key: &str) -> Option<f64> {
    get(q, key).and_then(|v| v.parse().ok())
}
fn getu(q: &[(String, String)], key: &str) -> Option<u64> {
    get(q, key).and_then(|v| v.parse().ok())
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(byte as char);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { ' ' } else { b[i] as char });
        i += 1;
    }
    out
}

fn build_rules(spec: &str) -> Vec<Box<dyn Rule>> {
    let mut rules: Vec<Box<dyn Rule>> = Vec::new();
    for name in spec.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        match name {
            "quota" => rules.push(Box::new(HarvestQuota::new(0.3))),
            "property" => rules.push(Box::new(PropertyRights)),
            "wealth-tax" => rules.push(Box::new(WealthTax::new(0.1))),
            "redistribute" => rules.push(Box::new(Redistribute::new(0.5))),
            "decarbonize" => rules.push(Box::new(Decarbonize::new(0.5))),
            "corrupt" => rules.push(Box::new(CorruptOfficial::new(0.3))),
            _ => {}
        }
    }
    rules
}

// ---------------------------------------------------------------------------
// Frame serialization (hand-built JSON — no serde)
// ---------------------------------------------------------------------------

/// Format an f64 as JSON, emitting `null` for non-finite values (e.g. life
/// expectancy before the first death) so the front-end can leave a gap.
fn jnum(x: f64) -> String {
    if x.is_finite() {
        format!("{x:.5}")
    } else {
        "null".to_string()
    }
}

fn frame_json(session: &Session) -> String {
    let w = &session.world;
    let sub = &w.substrate;
    let (gw, gh) = (sub.width, sub.height);
    let m = measure(w);

    // Per-cell resource fill (0..100) for each good, relative to pristine capacity.
    let mut res0 = String::from("[");
    let mut res1 = String::from("[");
    for i in 0..sub.resource.len() {
        if i > 0 {
            res0.push(',');
            res1.push(',');
        }
        let fill = |g: usize| -> u32 {
            let cap0 = sub.capacity0[i][g];
            if cap0 <= 1e-9 {
                0
            } else {
                ((sub.resource[i][g] / cap0) * 100.0).clamp(0.0, 100.0) as u32
            }
        };
        res0.push_str(&fill(0).to_string());
        res1.push_str(&fill(1).to_string());
    }
    res0.push(']');
    res1.push(']');

    // Living agents: [cell, wealthBucket 0..100], bucketed by energy reserve.
    let max_e = w
        .agents
        .energy
        .iter()
        .zip(&w.agents.alive)
        .filter(|(_, &a)| a)
        .map(|(&e, _)| e)
        .fold(1e-9_f64, f64::max);
    let mut agents = String::from("[");
    let mut first = true;
    for i in 0..w.agents.len() {
        if !w.agents.alive[i] {
            continue;
        }
        if !first {
            agents.push(',');
        }
        first = false;
        let bucket = ((w.agents.energy[i] / max_e) * 100.0).clamp(0.0, 100.0) as u32;
        agents.push_str(&format!("[{},{}]", w.agents.cell[i], bucket));
    }
    agents.push(']');

    let medium = match m.dominant_medium {
        Some(g) => g.to_string(),
        None => "null".to_string(),
    };

    format!(
        "{{\"tick\":{},\"w\":{},\"h\":{},\"ngoods\":{},\"preset\":\"{}\",\
         \"res0\":{},\"res1\":{},\"agents\":{},\
         \"m\":{{\"population\":{},\"gini\":{},\"mean_wealth\":{},\"life_expectancy\":{},\
         \"price_index\":{},\"gdp_flow\":{},\"specialization\":{},\"trade_count\":{},\
         \"commons_health\":{},\"state_capacity\":{},\"legitimacy\":{},\"corruption\":{},\
         \"public_pool\":{},\"temperature\":{},\"greenhouse\":{},\"emissions\":{},\
         \"climate_sensitivity\":{},\"resource_stock\":{},\"dominant_medium\":{}}}}}",
        w.tick,
        gw,
        gh,
        NGOODS,
        session.preset,
        res0,
        res1,
        agents,
        m.population,
        jnum(m.wealth_gini),
        jnum(m.mean_wealth),
        jnum(m.life_expectancy),
        jnum(m.price_index),
        jnum(m.gdp_flow),
        jnum(m.specialization),
        m.trade_count,
        jnum(m.commons_health),
        jnum(m.state_capacity),
        jnum(m.legitimacy),
        jnum(m.corruption),
        jnum(m.public_pool),
        jnum(m.temperature),
        jnum(m.greenhouse_stock),
        jnum(m.emissions),
        jnum(m.climate_sensitivity),
        jnum(m.resource_stock),
        medium,
    )
}

// ---------------------------------------------------------------------------
// Best-effort browser launch
// ---------------------------------------------------------------------------

fn open_browser(url: &str) {
    use std::process::Command;
    let _ = if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", "start", "", url]).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
}

// ---------------------------------------------------------------------------
// The embedded UI (self-contained HTML + CSS + JS; no external assets)
// ---------------------------------------------------------------------------

const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>society-physics — simviz</title>
<style>
  :root { --bg:#0e1116; --panel:#161b22; --ink:#e6edf3; --muted:#8b949e; --acc:#58a6ff; --good:#2ea043; }
  * { box-sizing: border-box; }
  body { margin:0; background:var(--bg); color:var(--ink); font:14px/1.5 -apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif; }
  header { padding:10px 16px; border-bottom:1px solid #30363d; display:flex; align-items:baseline; gap:12px; }
  header h1 { font-size:16px; margin:0; }
  header .sub { color:var(--muted); font-size:12px; }
  .wrap { display:grid; grid-template-columns: 280px 1fr 320px; gap:14px; padding:14px; align-items:start; }
  .panel { background:var(--panel); border:1px solid #30363d; border-radius:10px; padding:14px; }
  .panel h2 { font-size:12px; text-transform:uppercase; letter-spacing:.06em; color:var(--muted); margin:0 0 10px; }
  label { display:block; font-size:12px; color:var(--muted); margin:10px 0 2px; }
  .row { display:flex; align-items:center; gap:8px; }
  input[type=range] { width:100%; }
  input[type=number], select { width:100%; background:#0d1117; color:var(--ink); border:1px solid #30363d; border-radius:6px; padding:5px 8px; }
  .val { font-variant-numeric:tabular-nums; color:var(--ink); min-width:48px; text-align:right; font-size:12px; }
  .checks label { display:flex; align-items:center; gap:8px; color:var(--ink); margin:6px 0; cursor:pointer; }
  .btns { display:flex; gap:8px; margin-top:14px; flex-wrap:wrap; }
  button { background:#21262d; color:var(--ink); border:1px solid #30363d; border-radius:6px; padding:7px 12px; cursor:pointer; font-size:13px; }
  button:hover { border-color:var(--acc); }
  button.primary { background:var(--acc); color:#06121f; border-color:var(--acc); font-weight:600; }
  button.go { background:var(--good); border-color:var(--good); color:#04140a; font-weight:600; }
  #stage { display:flex; flex-direction:column; align-items:center; gap:8px; }
  #land { image-rendering: pixelated; width:100%; max-width:560px; aspect-ratio:1/1; background:#05070a; border-radius:8px; border:1px solid #30363d; }
  .legend { color:var(--muted); font-size:11px; display:flex; gap:14px; }
  .stat-grid { display:grid; grid-template-columns:1fr auto; gap:3px 10px; font-variant-numeric:tabular-nums; }
  .stat-grid .k { color:var(--muted); font-size:12px; }
  .stat-grid .v { text-align:right; font-size:13px; }
  .chart { margin-top:8px; }
  .chart .cap { font-size:11px; color:var(--muted); display:flex; justify-content:space-between; }
  canvas.spark { width:100%; height:46px; display:block; background:#0d1117; border-radius:6px; border:1px solid #21262d; }
  .note { color:var(--muted); font-size:11px; margin-top:12px; }
</style>
</head>
<body>
<header>
  <h1>society-physics · <span style="color:var(--acc)">simviz</span></h1>
  <span class="sub">primitives in → society out. every number here is <b>measured</b>, never set.</span>
</header>

<div class="wrap">
  <!-- CONTROLS -->
  <div class="panel" id="controls">
    <h2>Setup</h2>
    <label>Preset (landscape & physics)</label>
    <select id="preset">
      <option value="demo">demo — robust commons</option>
      <option value="fragile-commons">fragile-commons — destructible</option>
      <option value="warming-world">warming-world — climate on</option>
    </select>
    <label>Seed</label>
    <input type="number" id="seed" value="7" min="0" step="1">

    <h2 style="margin-top:16px">Primitives</h2>
    <div id="sliders"></div>

    <h2 style="margin-top:16px">Policy rules</h2>
    <div class="checks" id="rules">
      <label><input type="checkbox" value="quota"> Harvest quota (Ostrom)</label>
      <label><input type="checkbox" value="property"> Property rights (Demsetz)</label>
      <label><input type="checkbox" value="wealth-tax"> Wealth tax</label>
      <label><input type="checkbox" value="redistribute"> Redistribute</label>
      <label><input type="checkbox" value="decarbonize"> Decarbonize</label>
      <label><input type="checkbox" value="corrupt"> Corrupt official</label>
    </div>

    <div class="btns">
      <button class="primary" id="reset">Reset</button>
      <button class="go" id="play">▶ Play</button>
      <button id="step">Step</button>
    </div>
    <label style="margin-top:14px">Speed (ticks / frame)</label>
    <div class="row"><input type="range" id="speed" min="1" max="20" value="3"><span class="val" id="speedv">3</span></div>
    <div class="note">Toggling rules or moving a slider takes effect on the next <b>Reset</b>.</div>
  </div>

  <!-- STAGE -->
  <div id="stage">
    <canvas id="land" width="400" height="400"></canvas>
    <div class="legend">
      <span>🟥 good&nbsp;0 stock</span><span>🟩 good&nbsp;1 stock</span><span>⚪ agents (bright = wealthier)</span>
    </div>
    <div class="legend" id="tickline">tick 0</div>
  </div>

  <!-- READOUTS -->
  <div class="panel">
    <h2>Emergent measurements</h2>
    <div class="stat-grid" id="stats"></div>
    <div id="charts"></div>
    <div class="note">All values are computed from raw agent state by read-only instruments — the engine's hard rule.</div>
  </div>
</div>

<script>
const SLIDERS = [
  {k:'n_agents',      label:'Agents (seed count)', min:50,  max:1500, step:10,   val:400},
  {k:'peak_capacity', label:'Peak resource (K)',   min:1,   max:12,   step:0.5,  val:6},
  {k:'regrowth_rate', label:'Regrowth rate (r)',   min:0.05,max:0.8,  step:0.05, val:0.4},
  {k:'metabolism_max',label:'Max metabolism',      min:0.5, max:4,    step:0.1,  val:2},
  {k:'birth_threshold',label:'Birth threshold',    min:10,  max:60,   step:1,    val:25},
];
const CHARTS = [
  {k:'population', color:'#58a6ff'},
  {k:'gini',       color:'#f0883e'},
  {k:'mean_wealth',color:'#2ea043'},
  {k:'commons_health', color:'#3fb950'},
  {k:'temperature',color:'#db6d28'},
];
const STAT_ORDER = [
  ['population','pop'],['gini','Gini (inequality)'],['mean_wealth','mean wealth'],
  ['life_expectancy','life expectancy'],['price_index','price index'],['gdp_flow','GDP flow'],
  ['specialization','specialization'],['commons_health','commons health'],
  ['state_capacity','state capacity'],['legitimacy','legitimacy'],['corruption','corruption'],
  ['temperature','temperature (K)'],['climate_sensitivity','climate sensitivity'],
];

const series = {};
let playing = false, timer = null, lastFrame = null;

// build sliders
const slidersEl = document.getElementById('sliders');
for (const s of SLIDERS) {
  const wrap = document.createElement('div');
  wrap.innerHTML = `<label>${s.label}</label><div class="row"><input type="range" id="s_${s.k}" min="${s.min}" max="${s.max}" step="${s.step}" value="${s.val}"><span class="val" id="v_${s.k}">${s.val}</span></div>`;
  slidersEl.appendChild(wrap);
  const inp = wrap.querySelector('input');
  inp.addEventListener('input', () => document.getElementById('v_'+s.k).textContent = inp.value);
}
// build stat rows + charts
const statsEl = document.getElementById('stats');
for (const [k,label] of STAT_ORDER) {
  statsEl.insertAdjacentHTML('beforeend', `<div class="k">${label}</div><div class="v" id="st_${k}">–</div>`);
}
const chartsEl = document.getElementById('charts');
for (const c of CHARTS) {
  chartsEl.insertAdjacentHTML('beforeend',
    `<div class="chart"><div class="cap"><span>${c.k}</span><span id="ch_${c.k}_last"></span></div><canvas class="spark" id="ch_${c.k}"></canvas></div>`);
  series[c.k] = [];
}

document.getElementById('speed').addEventListener('input', e => document.getElementById('speedv').textContent = e.target.value);

function resetUrl() {
  const p = new URLSearchParams();
  p.set('preset', document.getElementById('preset').value);
  p.set('seed', document.getElementById('seed').value);
  for (const s of SLIDERS) p.set(s.k, document.getElementById('s_'+s.k).value);
  const rules = [...document.querySelectorAll('#rules input:checked')].map(x=>x.value).join(',');
  if (rules) p.set('rules', rules);
  return '/reset?' + p.toString();
}

async function doReset() {
  stop();
  for (const k in series) series[k] = [];
  const f = await (await fetch(resetUrl())).json();
  apply(f);
}
async function doStep() {
  const n = document.getElementById('speed').value;
  const f = await (await fetch('/step?n='+n)).json();
  apply(f);
}
function play() { if (playing) return; playing = true; document.getElementById('play').textContent='⏸ Pause';
  const loop = async () => { if (!playing) return; await doStep(); timer = setTimeout(loop, 60); }; loop(); }
function stop() { playing = false; document.getElementById('play').textContent='▶ Play'; if (timer) clearTimeout(timer); }

document.getElementById('reset').onclick = doReset;
document.getElementById('step').onclick = () => { stop(); doStep(); };
document.getElementById('play').onclick = () => playing ? stop() : play();

function apply(f) {
  lastFrame = f;
  drawLand(f);
  document.getElementById('tickline').textContent = `tick ${f.tick} · preset ${f.preset} · ${f.m.population} alive`;
  for (const [k] of STAT_ORDER) {
    const v = f.m[k];
    document.getElementById('st_'+k).textContent = (v===null||v===undefined) ? '–' : fmt(v);
  }
  for (const c of CHARTS) {
    const v = f.m[c.k];
    if (v !== null && v !== undefined) series[c.k].push(v);
    if (series[c.k].length > 600) series[c.k].shift();
    drawSpark('ch_'+c.k, series[c.k], c.color);
    const lastEl = document.getElementById('ch_'+c.k+'_last');
    if (lastEl) lastEl.textContent = (v===null||v===undefined)?'':fmt(v);
  }
}
function fmt(v){ if (Math.abs(v)>=1000) return v.toFixed(0); if (Math.abs(v)>=10) return v.toFixed(1); return v.toFixed(3); }

const land = document.getElementById('land');
const lctx = land.getContext('2d');
function drawLand(f) {
  if (land.width !== f.w || land.height !== f.h) { land.width = f.w; land.height = f.h; }
  const img = lctx.createImageData(f.w, f.h);
  for (let i=0;i<f.res0.length;i++){
    const r = Math.round(f.res0[i]*2.2);      // good 0 -> red
    const g = Math.round(f.res1[i]*2.2);      // good 1 -> green
    const o = i*4;
    img.data[o]=Math.min(255,r); img.data[o+1]=Math.min(255,g); img.data[o+2]=30; img.data[o+3]=255;
  }
  lctx.putImageData(img, 0, 0);
  // agents as bright dots, brightness by wealth bucket
  for (const [cell,wb] of f.agents){
    const x = cell % f.w, y = (cell / f.w)|0;
    const b = 120 + Math.round(wb*1.35);
    lctx.fillStyle = `rgb(${b},${b},255)`;
    lctx.fillRect(x, y, 1, 1);
  }
}

function drawSpark(id, data, color) {
  const c = document.getElementById(id);
  const dpr = window.devicePixelRatio || 1;
  const w = c.clientWidth, h = c.clientHeight;
  if (c.width !== w*dpr || c.height !== h*dpr) { c.width=w*dpr; c.height=h*dpr; }
  const ctx = c.getContext('2d'); ctx.setTransform(dpr,0,0,dpr,0,0); ctx.clearRect(0,0,w,h);
  if (data.length < 2) return;
  let lo=Math.min(...data), hi=Math.max(...data); if (hi-lo<1e-9){hi=lo+1;}
  ctx.strokeStyle=color; ctx.lineWidth=1.5; ctx.beginPath();
  for (let i=0;i<data.length;i++){
    const x = (i/(data.length-1))*(w-2)+1;
    const y = h-2 - ((data[i]-lo)/(hi-lo))*(h-4);
    i?ctx.lineTo(x,y):ctx.moveTo(x,y);
  }
  ctx.stroke();
}

// initial world
doReset();
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    #[test]
    fn percent_decode_handles_escapes_and_plus() {
        assert_eq!(percent_decode("a%20b+c"), "a b c");
        assert_eq!(percent_decode("quota%2Cproperty"), "quota,property");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("bad%zz"), "bad%zz"); // not a valid escape -> literal
    }

    #[test]
    fn query_parsing_and_getters() {
        let q = parse_query("preset=demo&seed=7&peak_capacity=6.5&flag");
        assert_eq!(get(&q, "preset"), Some("demo"));
        assert_eq!(getu(&q, "seed"), Some(7));
        assert_eq!(getf(&q, "peak_capacity"), Some(6.5));
        assert_eq!(get(&q, "flag"), Some("")); // key with no '='
        assert_eq!(get(&q, "missing"), None);
        assert_eq!(getf(&q, "preset"), None); // not a number
        assert!(parse_query("").is_empty());
    }

    #[test]
    fn presets_and_rules_map() {
        // every preset arm
        assert!(preset_primitives("demo").n_agents > 0);
        assert!(preset_primitives("fragile-commons").degrade_rate >= 0.0);
        assert!(preset_primitives("warming-world").n_agents > 0);
        assert!(preset_primitives("unknown-falls-back-to-demo").n_agents > 0);
        // every rule arm + an unknown (ignored)
        let rules = build_rules("quota,property,wealth-tax,redistribute,decarbonize,corrupt,bogus");
        assert_eq!(rules.len(), 6);
        assert!(build_rules("").is_empty());
    }

    #[test]
    fn jnum_finite_and_nonfinite() {
        assert_eq!(jnum(f64::NAN), "null");
        assert_eq!(jnum(f64::INFINITY), "null");
        assert!(jnum(1.5).starts_with("1.5"));
    }

    #[test]
    fn frame_json_is_wellformed_and_evolves() {
        let mut s = Session::new("warming-world", 3);
        let f0 = frame_json(&s);
        assert!(f0.contains("\"tick\":0"));
        assert!(f0.contains("\"preset\":\"warming-world\""));
        assert!(f0.contains("\"res0\":[") && f0.contains("\"res1\":["));
        // life expectancy is NaN before any death -> the null branch of jnum.
        assert!(f0.contains("\"life_expectancy\":null"));
        // dominant_medium is None at tick 0 -> "null".
        assert!(f0.contains("\"dominant_medium\":null"));
        // advance with some rules and confirm the tick moves.
        s.rules = build_rules("quota,redistribute,decarbonize");
        for _ in 0..30 {
            s.world.step_with_rules(&s.rules);
        }
        let f1 = frame_json(&s);
        assert!(f1.contains(&format!("\"tick\":{}", s.world.tick)));
        assert!(!f1.contains("\"tick\":0"));
    }

    /// End-to-end: a real local server handling real HTTP requests, covering
    /// `handle`, routing, `respond`/`respond_json`, and `frame_json`.
    #[test]
    fn http_server_round_trip() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let mut session = Session::new("demo", 1);
            for _ in 0..5 {
                let (stream, _) = listener.accept().unwrap();
                let _ = handle(stream, &mut session);
            }
        });

        let request = |path: &str| -> String {
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).unwrap();
            s.flush().unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        };

        assert!(request("/").contains("<!DOCTYPE html>"));
        let reset = request("/reset?preset=warming-world&seed=2&n_agents=200&peak_capacity=6&regrowth_rate=0.4&metabolism_max=2&birth_threshold=25&rules=quota%2Cproperty%2Cwealth-tax%2Credistribute%2Cdecarbonize%2Ccorrupt");
        assert!(reset.contains("200 OK") && reset.contains("\"tick\":0"));
        assert!(request("/step?n=5").contains("\"tick\":5"));
        assert!(request("/frame").contains("\"tick\":5"));
        assert!(request("/nope").contains("404 Not Found"));

        server.join().unwrap();
    }
}
