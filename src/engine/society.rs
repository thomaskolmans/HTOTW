//! **Society-as-input** (Phase 10): describe an existing (or imagined) society
//! in a plain text spec — its physical/biological/psychological *primitives*
//! and the **stack of laws, structures and institutions** it lives under — and
//! load it into the engine.
//!
//! This is the configurable front door the project's vision calls for: nature,
//! environment, physics and human psychology are the driving factors (the
//! engine's primitives), and the *build-up of rules, structures, institutions,
//! policies and laws* is the **input you compose**. The hard rule still holds:
//! a spec may set primitives and laws (mechanisms), **never outcomes** — there
//! is deliberately no key for a Gini, a GDP or a temperature. To make a spec
//! match a real country you calibrate its primitives until the *measured*
//! moments agree (`engine::calibration`); the bundled presets are therefore
//! **archetypes** (an egalitarian green state, a laissez-faire market society,
//! an extractive autocracy…) to be refined against data, not portraits.
//!
//! ## The `.soc` format (dependency-free, line-based)
//!
//! ```text
//! # comment
//! name = egalitarian-green
//! base = warming-world            # demo | fragile-commons | warming-world | human-nature
//!
//! [primitives]                    # physical/biological/psychological knobs only
//! agents = 600
//! psychology = on
//! patience-min = 0.3
//!
//! [governance]                    # optional: endogenous collective choice
//! mechanism = majority            # majority | wealth-weighted
//! period = 25
//! threshold = 0.5
//!
//! [laws]                          # the institutional stack, applied in order
//! wealth-tax = 0.3
//! redistribute = 1.0
//! harvest-quota = 0.35
//! decarbonize = 0.7
//! property-rights = on
//! ```
//!
//! Parsing is strict (unknown keys, sections, laws or values are errors with a
//! line number) so a typo cannot silently change an experiment.

use super::institutions::{
    CorruptOfficial, Decarbonize, HarvestQuota, OpenAccess, PropertyRights, Redistribute, Rule,
    WealthTax,
};
use super::polity::ChoiceMechanism;
use super::world::Primitives;

/// One law/institution in a society's stack — the parsed, comparable form of a
/// `[laws]` entry. Each maps onto one Phase-3/5 [`Rule`] mechanism; a law can
/// therefore mold incentives only, never set an outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum Law {
    /// Progressive wealth tax at this rate into the public pool.
    WealthTax(f64),
    /// Means-tested disbursement of this fraction of the pool per tick.
    Redistribute(f64),
    /// Conservation quota: max fraction of standing stock taken per harvest.
    HarvestQuota(f64),
    /// Homestead property rights over occupied cells.
    PropertyRights,
    /// Clean-production mandate abating this fraction of emissions.
    Decarbonize(f64),
    /// A kleptocrat skimming this fraction of the public pool per tick.
    CorruptOfficial(f64),
}

/// The catalogue of law names a spec may use, in canonical order.
pub const LAW_NAMES: [&str; 6] = [
    "wealth-tax",
    "redistribute",
    "harvest-quota",
    "property-rights",
    "decarbonize",
    "corrupt-official",
];

impl Law {
    /// The law's stable spec/CLI name.
    pub fn name(&self) -> &'static str {
        match self {
            Law::WealthTax(_) => "wealth-tax",
            Law::Redistribute(_) => "redistribute",
            Law::HarvestQuota(_) => "harvest-quota",
            Law::PropertyRights => "property-rights",
            Law::Decarbonize(_) => "decarbonize",
            Law::CorruptOfficial(_) => "corrupt-official",
        }
    }

    /// Spec-style rendering, e.g. `wealth-tax = 0.30` / `property-rights = on`.
    pub fn describe(&self) -> String {
        match self {
            Law::WealthTax(v)
            | Law::Redistribute(v)
            | Law::HarvestQuota(v)
            | Law::Decarbonize(v)
            | Law::CorruptOfficial(v) => format!("{} = {v}", self.name()),
            Law::PropertyRights => format!("{} = on", self.name()),
        }
    }

    /// Parse a `name = value` law entry. Parameterised laws need a number in
    /// `[0,1]`; `property-rights` takes `on`/`true`.
    pub fn parse(name: &str, value: &str) -> Result<Law, String> {
        let frac = |what: &str| -> Result<f64, String> {
            let v: f64 = value
                .parse()
                .map_err(|_| format!("law '{name}' needs a numeric {what}, got '{value}'"))?;
            if !(0.0..=1.0).contains(&v) {
                return Err(format!("law '{name}': {what} must be in [0,1], got {v}"));
            }
            Ok(v)
        };
        match name {
            "wealth-tax" => Ok(Law::WealthTax(frac("rate")?)),
            "redistribute" => Ok(Law::Redistribute(frac("fraction")?)),
            "harvest-quota" => Ok(Law::HarvestQuota(frac("fraction")?)),
            "decarbonize" => Ok(Law::Decarbonize(frac("abatement")?)),
            "corrupt-official" => Ok(Law::CorruptOfficial(frac("skim")?)),
            "property-rights" => match value {
                "on" | "true" | "yes" => Ok(Law::PropertyRights),
                other => Err(format!(
                    "law 'property-rights' takes 'on', got '{other}' (omit the line to leave it off)"
                )),
            },
            other => Err(format!(
                "unknown law '{other}'. available: {}",
                LAW_NAMES.join(", ")
            )),
        }
    }

    /// Instantiate the concrete [`Rule`] mechanism this law enacts.
    pub fn rule(&self) -> Box<dyn Rule> {
        match *self {
            Law::WealthTax(rate) => Box::new(WealthTax::new(rate)),
            Law::Redistribute(frac) => Box::new(Redistribute::new(frac)),
            Law::HarvestQuota(frac) => Box::new(HarvestQuota::new(frac)),
            Law::PropertyRights => Box::new(PropertyRights),
            Law::Decarbonize(abate) => Box::new(Decarbonize::new(abate)),
            Law::CorruptOfficial(skim) => Box::new(CorruptOfficial::new(skim)),
        }
    }
}

/// Optional endogenous-government block: how this society aggregates its
/// agents' preferences into the rules in force (Phase 6 collective choice).
#[derive(Debug, Clone, PartialEq)]
pub struct Governance {
    pub mechanism: ChoiceMechanism,
    /// Electoral term length in ticks.
    pub period: u64,
    /// Support share an option needs to be enacted.
    pub threshold: f64,
}

/// A parsed society: a name, the primitives it runs on, the declared law
/// stack, and (optionally) how it governs itself. Built by [`SocietySpec::parse`];
/// consumed by the counterfactual harness and the CLI.
#[derive(Debug, Clone)]
pub struct SocietySpec {
    pub name: String,
    pub primitives: Primitives,
    /// The institutional stack, in declaration order.
    pub laws: Vec<Law>,
    pub governance: Option<Governance>,
}

impl SocietySpec {
    /// Parse a `.soc` text. Strict: unknown sections/keys/laws/values are
    /// errors carrying the line number.
    pub fn parse(text: &str) -> Result<SocietySpec, String> {
        #[derive(PartialEq)]
        enum Section {
            Top,
            Primitives,
            Governance,
            Laws,
        }
        let mut section = Section::Top;
        let mut name: Option<String> = None;
        let mut primitives = Primitives::demo();
        let mut laws: Vec<Law> = Vec::new();
        let mut gov_mechanism: Option<ChoiceMechanism> = None;
        let mut gov_period: u64 = 25;
        let mut gov_threshold: f64 = 0.5;
        let mut saw_governance = false;

        for (ln, raw) in text.lines().enumerate() {
            let at = |msg: String| format!("line {}: {msg}", ln + 1);
            // Strip comments and whitespace.
            let line = match raw.find('#') {
                Some(i) => &raw[..i],
                None => raw,
            }
            .trim();
            if line.is_empty() {
                continue;
            }

            if let Some(header) = line.strip_prefix('[') {
                let Some(header) = header.strip_suffix(']') else {
                    return Err(at(format!("malformed section header '{line}'")));
                };
                section = match header.trim() {
                    "primitives" | "psychology" => Section::Primitives,
                    "governance" => {
                        saw_governance = true;
                        Section::Governance
                    }
                    "laws" => Section::Laws,
                    other => {
                        return Err(at(format!(
                            "unknown section '[{other}]' (expected [primitives], [governance] or [laws])"
                        )))
                    }
                };
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                return Err(at(format!("expected 'key = value', got '{line}'")));
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');

            match section {
                Section::Top => match key {
                    "name" => name = Some(value.to_string()),
                    "base" => {
                        // The base preset the primitives start from; section
                        // keys then override individual knobs.
                        primitives = match value {
                            "demo" => Primitives::demo(),
                            "fragile-commons" => Primitives::fragile_commons(),
                            "warming-world" => Primitives::warming_world(),
                            "human-nature" => Primitives::human_nature(),
                            other => {
                                return Err(at(format!(
                                    "unknown base '{other}' (demo | fragile-commons | warming-world | human-nature)"
                                )))
                            }
                        };
                    }
                    other => {
                        return Err(at(format!(
                            "unknown top-level key '{other}' (expected name or base)"
                        )))
                    }
                },
                Section::Primitives => {
                    set_primitive(&mut primitives, key, value).map_err(at)?;
                }
                Section::Governance => match key {
                    "mechanism" => {
                        gov_mechanism = Some(match value {
                            "majority" => ChoiceMechanism::Majority,
                            "wealth-weighted" => ChoiceMechanism::WealthWeighted,
                            other => {
                                return Err(at(format!(
                                    "unknown mechanism '{other}' (majority | wealth-weighted)"
                                )))
                            }
                        });
                    }
                    "period" => {
                        gov_period = value
                            .parse()
                            .map_err(|_| at(format!("invalid period '{value}'")))?;
                    }
                    "threshold" => {
                        gov_threshold = value
                            .parse()
                            .map_err(|_| at(format!("invalid threshold '{value}'")))?;
                    }
                    other => return Err(at(format!("unknown governance key '{other}'"))),
                },
                Section::Laws => {
                    let law = Law::parse(key, value).map_err(at)?;
                    if laws.iter().any(|l| l.name() == law.name()) {
                        return Err(at(format!("duplicate law '{}'", law.name())));
                    }
                    laws.push(law);
                }
            }
        }

        let governance = if saw_governance {
            let mechanism = gov_mechanism
                .ok_or_else(|| "[governance] section needs a 'mechanism' key".to_string())?;
            Some(Governance {
                mechanism,
                period: gov_period.max(1),
                threshold: gov_threshold.clamp(0.0, 1.0),
            })
        } else {
            None
        };

        Ok(SocietySpec {
            name: name.unwrap_or_else(|| "unnamed-society".to_string()),
            primitives,
            laws,
            governance,
        })
    }

    /// The concrete [`Rule`] stack this society's laws enact, in declaration
    /// order. Always begins with [`OpenAccess`] so the harvest mechanism is
    /// explicitly reset each tick before the laws layer on (the same convention
    /// the polity uses).
    pub fn rules(&self) -> Vec<Box<dyn Rule>> {
        let mut rules: Vec<Box<dyn Rule>> = vec![Box::new(OpenAccess)];
        for law in &self.laws {
            rules.push(law.rule());
        }
        rules
    }

    /// This society as an experiment [`Scenario`](super::calibration::Scenario)
    /// (primitives + rule stack), ready for `evaluate`/`compare`.
    pub fn scenario(&self) -> super::calibration::Scenario {
        super::calibration::Scenario::new(self.name.clone(), self.primitives.clone(), self.rules())
    }

    /// Load a bundled preset archetype by name.
    pub fn preset(name: &str) -> Option<SocietySpec> {
        presets()
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, text)| SocietySpec::parse(text).expect("bundled preset must parse"))
    }
}

/// Set one primitive by its spec key. Only physical/biological/psychological
/// knobs are reachable — there is deliberately no key for any macro outcome.
fn set_primitive(p: &mut Primitives, key: &str, value: &str) -> Result<(), String> {
    fn num<T: std::str::FromStr>(key: &str, value: &str) -> Result<T, String> {
        value
            .parse()
            .map_err(|_| format!("invalid value '{value}' for '{key}'"))
    }
    fn boolean(key: &str, value: &str) -> Result<bool, String> {
        match value {
            "on" | "true" | "yes" => Ok(true),
            "off" | "false" | "no" => Ok(false),
            other => Err(format!("invalid value '{other}' for '{key}' (on/off)")),
        }
    }
    match key {
        "width" => p.width = num(key, value)?,
        "height" => p.height = num(key, value)?,
        "agents" => p.n_agents = num(key, value)?,
        "seed" => p.seed = num(key, value)?,
        "regrowth-rate" => p.regrowth_rate = num(key, value)?,
        "peak-capacity" => p.peak_capacity = num(key, value)?,
        "init-energy" => p.init_energy = num(key, value)?,
        "metabolism-min" => p.metabolism_min = num(key, value)?,
        "metabolism-max" => p.metabolism_max = num(key, value)?,
        "vision-min" => p.vision_min = num(key, value)?,
        "vision-max" => p.vision_max = num(key, value)?,
        "birth-threshold" => p.birth_threshold = num(key, value)?,
        "child-endowment" => p.child_endowment_frac = num(key, value)?,
        "max-age" => p.max_age = num(key, value)?,
        "senescence" => p.senescence = num(key, value)?,
        "mutation" => p.mutation = num(key, value)?,
        "satiation-scale" => p.satiation_scale = num(key, value)?,
        "energy-per-good" => p.energy_per_good = num(key, value)?,
        "trade" => p.trade_enabled = boolean(key, value)?,
        "regen-threshold" => p.regen_threshold = num(key, value)?,
        "degrade-rate" => p.degrade_rate = num(key, value)?,
        "recovery-rate" => p.recovery_rate = num(key, value)?,
        "climate" => p.climate_enabled = boolean(key, value)?,
        "emission-factor" => p.emission_factor = num(key, value)?,
        "co2-decay" => p.co2_decay = num(key, value)?,
        "psychology" => p.psyche_enabled = boolean(key, value)?,
        "patience-min" => p.patience_min = num(key, value)?,
        "patience-max" => p.patience_max = num(key, value)?,
        "risk-aversion-min" => p.risk_aversion_min = num(key, value)?,
        "risk-aversion-max" => p.risk_aversion_max = num(key, value)?,
        "fairness-min" => p.fairness_min = num(key, value)?,
        "fairness-max" => p.fairness_max = num(key, value)?,
        "conformity-min" => p.conformity_min = num(key, value)?,
        "conformity-max" => p.conformity_max = num(key, value)?,
        other => return Err(format!("unknown primitive '{other}'")),
    }
    Ok(())
}

/// The bundled society **archetypes** as `(name, spec text)` pairs, embedded in
/// the binary so `simctl society --preset NAME` works anywhere. They are
/// starting points for calibration, not portraits of real countries.
pub fn presets() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "open-frontier",
            include_str!("../../societies/open-frontier.soc"),
        ),
        (
            "stewardship-commons",
            include_str!("../../societies/stewardship-commons.soc"),
        ),
        (
            "egalitarian-green",
            include_str!("../../societies/egalitarian-green.soc"),
        ),
        (
            "laissez-faire",
            include_str!("../../societies/laissez-faire.soc"),
        ),
        (
            "extractive-autocracy",
            include_str!("../../societies/extractive-autocracy.soc"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"
# A complete spec exercising every section.
name = "test-society"
base = fragile-commons

[primitives]
agents = 500
seed = 11
psychology = on
patience-min = 0.6
patience-max = 0.9

[governance]
mechanism = majority
period = 30
threshold = 0.55

[laws]
wealth-tax = 0.25
redistribute = 1.0
harvest-quota = 0.3
property-rights = on
"#;

    #[test]
    fn parses_a_full_spec() {
        let s = SocietySpec::parse(FULL).unwrap();
        assert_eq!(s.name, "test-society");
        // Base + overrides land in the primitives.
        assert!(s.primitives.degrade_rate > 0.0, "fragile-commons base");
        assert_eq!(s.primitives.n_agents, 500);
        assert_eq!(s.primitives.seed, 11);
        assert!(s.primitives.psyche_enabled);
        assert_eq!(s.primitives.patience_min, 0.6);
        // Laws kept in declaration order.
        let names: Vec<&str> = s.laws.iter().map(|l| l.name()).collect();
        assert_eq!(
            names,
            ["wealth-tax", "redistribute", "harvest-quota", "property-rights"]
        );
        assert_eq!(s.laws[0], Law::WealthTax(0.25));
        // Governance block.
        let g = s.governance.clone().unwrap();
        assert_eq!(g.mechanism, ChoiceMechanism::Majority);
        assert_eq!(g.period, 30);
        assert_eq!(g.threshold, 0.55);
        // The rule stack resets the harvest mechanism first.
        let rules = s.rules();
        assert_eq!(rules[0].name(), "open-access");
        assert_eq!(rules.len(), 1 + 4);
    }

    #[test]
    fn minimal_spec_and_defaults() {
        let s = SocietySpec::parse("").unwrap();
        assert_eq!(s.name, "unnamed-society");
        assert!(s.laws.is_empty());
        assert!(s.governance.is_none());
        // No laws ⇒ just the open-access reset.
        assert_eq!(s.rules().len(), 1);
    }

    #[test]
    fn strict_errors_carry_line_numbers() {
        for (bad, needle) in [
            ("base = mars", "unknown base"),
            ("[weather]", "unknown section"),
            ("[primitives]\ngini = 0.3", "unknown primitive"),
            ("[primitives]\nagents = lots", "invalid value"),
            ("[laws]\nteleport = 1", "unknown law"),
            ("[laws]\nwealth-tax = high", "numeric"),
            ("[laws]\nwealth-tax = 1.5", "[0,1]"),
            ("[laws]\nproperty-rights = 0.5", "takes 'on'"),
            ("[laws]\nwealth-tax = 0.2\nwealth-tax = 0.3", "duplicate law"),
            ("[governance]\nmechanism = monarchy", "unknown mechanism"),
            ("[governance]\nperiod = 25", "needs a 'mechanism'"),
            ("just words", "key = value"),
            ("[laws\nx = 1", "malformed section"),
            ("flavour = sweet", "unknown top-level key"),
            ("[governance]\nmechanism = majority\ncolour = red", "unknown governance key"),
            ("[governance]\nmechanism = majority\nperiod = soon", "invalid period"),
            ("[governance]\nmechanism = majority\nthreshold = half", "invalid threshold"),
            ("[primitives]\ntrade = maybe", "on/off"),
        ] {
            let err = SocietySpec::parse(bad).unwrap_err();
            assert!(
                err.contains(needle),
                "spec {bad:?} should fail mentioning {needle:?}, got: {err}"
            );
        }
    }

    /// The hard rule, at the input layer: no macro outcome is settable.
    #[test]
    fn no_outcome_key_exists() {
        for key in ["gini", "gdp", "temperature", "life-expectancy", "population"] {
            let text = format!("[primitives]\n{key} = 1.0");
            assert!(
                SocietySpec::parse(&text).is_err(),
                "'{key}' must not be a settable primitive"
            );
        }
    }

    #[test]
    fn all_bundled_presets_parse_and_run() {
        assert!(!presets().is_empty());
        for (name, text) in presets() {
            let s = SocietySpec::parse(text)
                .unwrap_or_else(|e| panic!("preset '{name}' must parse: {e}"));
            assert_eq!(&s.name, name, "preset name must match its registry key");
            // Each preset builds a world and survives a short run under its laws.
            let rules = s.rules();
            let mut w = crate::engine::World::new(s.primitives.clone());
            for _ in 0..10 {
                w.step_with_rules(&rules);
            }
            assert!(w.agents.alive_count() > 0, "preset '{name}' should sustain life");
        }
        assert!(SocietySpec::preset("egalitarian-green").is_some());
        assert!(SocietySpec::preset("atlantis").is_none());
    }

    #[test]
    fn law_parse_and_describe_round_trip() {
        for (name, value) in [
            ("wealth-tax", "0.3"),
            ("redistribute", "1"),
            ("harvest-quota", "0.35"),
            ("decarbonize", "0.7"),
            ("corrupt-official", "0.6"),
            ("property-rights", "on"),
        ] {
            let law = Law::parse(name, value).unwrap();
            assert_eq!(law.name(), name);
            assert!(law.describe().starts_with(name));
            let _ = law.rule(); // instantiates a concrete mechanism
        }
    }
}
