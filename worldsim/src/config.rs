//! Run configuration: the **planetary scenario** ([`WorldConfig`] — geography,
//! resource endowment, initial population, psychology ranges) and the **way the
//! world is operated** ([`SocietyParams`] — laws, structures, institutions,
//! policies). Both are inputs you compose; *neither contains any social
//! outcome*. A plain-text config format (`[world]` / `[society]` sections,
//! strict parsing) makes existing or imagined societies loadable files.

use crate::rng::Rng;

/// How land and renewable stocks may be used — the property regime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyRegime {
    /// No restraint: anyone may strip any cell (Hardin's open access).
    OpenAccess,
    /// A community quota caps the harvested fraction of standing stocks
    /// (Ostrom-style appropriation rule); compliance is voluntary/enforced.
    CommonsQuota,
    /// Occupant-ownership: users self-limit on land they hold (Demsetz).
    Private,
}

/// How collected revenue is returned to people.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferRegime {
    None,
    /// Means-tested floor: payments fill the worst shortfalls first.
    Floor,
    /// Unconditional uniform dividend to every person.
    UniversalDividend,
}

/// Who can change the dials while the world runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceRegime {
    /// The parameters stay exactly as configured (a constitution of stone).
    Fixed,
    /// One-person-one-vote referenda each period nudge the fiscal/ecological
    /// dials toward the median voter's measured interest (Downs 1957).
    Majority,
    /// Votes weighted by wealth (elite capture; Acemoglu & Robinson 2006).
    WealthWeighted,
}

/// **The society parameters** — the complete, testable description of how a
/// polity is operated. This is the vector the counterfactual and search layers
/// explore. Every field is a *mechanism setting*; outcomes stay measured.
#[derive(Debug, Clone, PartialEq)]
pub struct SocietyParams {
    pub property: PropertyRegime,
    /// Cap on the fraction of a standing renewable stock one harvester may
    /// take per year (active under `CommonsQuota`; 1.0 = no cap).
    pub conservation_quota: f64,
    /// Income tax rate on the year's earnings.
    pub tax_rate: f64,
    /// 0 = flat tax; 1 = levied only on income above the mean (progressive).
    pub tax_progressivity: f64,
    pub transfer: TransferRegime,
    /// Budget shares (of collected revenue) for public schooling, productive
    /// infrastructure, research, and rule enforcement. The remainder funds the
    /// transfer regime. Shares are renormalised if they exceed 1.
    pub education_share: f64,
    pub infrastructure_share: f64,
    pub research_share: f64,
    pub enforcement_share: f64,
    /// Price per unit of CO₂ emitted (a Pigouvian carbon tax in numéraire).
    pub carbon_price: f64,
    /// 0 = closed borders (no in-migration), 1 = fully open.
    pub migration_openness: f64,
    pub governance: GovernanceRegime,
    /// Years between referenda when governance is not `Fixed`.
    pub vote_period: u32,
}

impl Default for SocietyParams {
    /// The **null society**: no laws, no taxes, no transfers, no quotas, open
    /// access, closed-by-default nothing — the Hardin/laissez-faire baseline
    /// every configured society is compared against.
    fn default() -> SocietyParams {
        SocietyParams {
            property: PropertyRegime::OpenAccess,
            conservation_quota: 1.0,
            tax_rate: 0.0,
            tax_progressivity: 0.0,
            transfer: TransferRegime::None,
            education_share: 0.0,
            infrastructure_share: 0.0,
            research_share: 0.0,
            enforcement_share: 0.0,
            carbon_price: 0.0,
            migration_openness: 1.0,
            governance: GovernanceRegime::Fixed,
            vote_period: 20,
        }
    }
}

/// A `[min, max]` range a heterogeneous trait is drawn from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range(pub f64, pub f64);

impl Range {
    pub fn sample(&self, rng: &mut Rng) -> f64 {
        rng.range(self.0.min(self.1), self.0.max(self.1))
    }
    pub fn clamp(&self, v: f64) -> f64 {
        v.clamp(self.0.min(self.1), self.0.max(self.1))
    }
}

/// The planetary scenario: geography, endowment, demography seed, psychology.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldConfig {
    pub seed: u64,
    /// Grid resolution (longitude x latitude). Cell area is weighted by
    /// cos(latitude), so the grid covers a sphere honestly.
    pub nlon: usize,
    pub nlat: usize,
    /// Fraction of the surface that is land (Earth: 0.29).
    pub land_fraction: f64,
    /// Initial human population (seeded in habitable cells).
    pub n_agents: usize,
    /// Number of polities the land is partitioned into (1 = world government).
    pub n_polities: usize,
    /// Total recoverable fossil energy and mineral stock, in numéraire units
    /// per capita of the initial population (the endowment scale).
    pub fossil_endowment: f64,
    pub mineral_endowment: f64,
    /// Heritable psychology ranges (Phase-9 traits, now core).
    pub patience: Range,
    pub risk_aversion: Range,
    pub fairness: Range,
    pub conformity: Range,
}

impl Default for WorldConfig {
    fn default() -> WorldConfig {
        WorldConfig {
            seed: 1,
            nlon: 72,
            nlat: 36,
            land_fraction: 0.29,
            n_agents: 4000,
            n_polities: 6,
            fossil_endowment: 60.0,
            mineral_endowment: 60.0,
            patience: Range(0.2, 0.8),
            risk_aversion: Range(0.2, 0.8),
            fairness: Range(0.2, 0.8),
            conformity: Range(0.2, 0.8),
        }
    }
}

/// A full scenario file: the planet plus one [`SocietyParams`] per polity (a
/// single `[society]` section applies to all polities; `[society N]` overrides
/// polity N).
#[derive(Debug, Clone)]
pub struct Scenario {
    pub name: String,
    pub world: WorldConfig,
    /// `societies[p]` is polity p's parameter set.
    pub societies: Vec<SocietyParams>,
}

impl Scenario {
    pub fn new(name: impl Into<String>, world: WorldConfig) -> Scenario {
        let n = world.n_polities.max(1);
        Scenario {
            name: name.into(),
            world,
            societies: vec![SocietyParams::default(); n],
        }
    }

    /// Apply one society parameter set to every polity.
    pub fn with_uniform_society(mut self, s: SocietyParams) -> Scenario {
        for slot in &mut self.societies {
            *slot = s.clone();
        }
        self
    }

    /// Parse a `.world` scenario file. Strict: unknown sections/keys/values
    /// are errors with line numbers, and **no outcome key exists**.
    pub fn parse(text: &str) -> Result<Scenario, String> {
        enum Section {
            Top,
            World,
            /// `None` = all polities, `Some(p)` = a single polity override.
            Society(Option<usize>),
        }
        let mut name = "unnamed-world".to_string();
        let mut world = WorldConfig::default();
        let mut all = SocietyParams::default();
        let mut overrides: Vec<(usize, Vec<(String, String, usize)>)> = Vec::new();
        let mut all_kv: Vec<(String, String, usize)> = Vec::new();
        let mut section = Section::Top;

        for (ln, raw) in text.lines().enumerate() {
            let at = |msg: String| format!("line {}: {msg}", ln + 1);
            let line = match raw.find('#') {
                Some(i) => &raw[..i],
                None => raw,
            }
            .trim();
            if line.is_empty() {
                continue;
            }
            if let Some(h) = line.strip_prefix('[') {
                let Some(h) = h.strip_suffix(']') else {
                    return Err(at(format!("malformed section header '{line}'")));
                };
                let h = h.trim();
                section = if h == "world" {
                    Section::World
                } else if h == "society" {
                    Section::Society(None)
                } else if let Some(num) = h.strip_prefix("society ") {
                    let p: usize = num
                        .trim()
                        .parse()
                        .map_err(|_| at(format!("bad polity index '{num}'")))?;
                    overrides.push((p, Vec::new()));
                    Section::Society(Some(overrides.len() - 1))
                } else {
                    return Err(at(format!(
                        "unknown section '[{h}]' (expected [world], [society] or [society N])"
                    )));
                };
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                return Err(at(format!("expected 'key = value', got '{line}'")));
            };
            let (k, v) = (k.trim().to_string(), v.trim().trim_matches('"').to_string());
            match &section {
                Section::Top => {
                    if k == "name" {
                        name = v;
                    } else {
                        return Err(at(format!("unknown top-level key '{k}' (expected name)")));
                    }
                }
                Section::World => set_world_key(&mut world, &k, &v).map_err(at)?,
                Section::Society(None) => all_kv.push((k, v, ln + 1)),
                Section::Society(Some(idx)) => overrides[*idx].1.push((k, v, ln + 1)),
            }
        }

        for (k, v, ln) in &all_kv {
            set_society_key(&mut all, k, v).map_err(|e| format!("line {ln}: {e}"))?;
        }
        let n = world.n_polities.max(1);
        let mut societies = vec![all; n];
        for (p, kvs) in overrides {
            if p >= n {
                return Err(format!(
                    "[society {p}]: polity index out of range (n-polities = {n})"
                ));
            }
            for (k, v, ln) in kvs {
                set_society_key(&mut societies[p], &k, &v)
                    .map_err(|e| format!("line {ln}: {e}"))?;
            }
        }
        Ok(Scenario { name, world, societies })
    }
}

fn num<T: std::str::FromStr>(key: &str, value: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("invalid value '{value}' for '{key}'"))
}

fn set_world_key(w: &mut WorldConfig, key: &str, value: &str) -> Result<(), String> {
    // Trait ranges are written `a..b`.
    let range = |key: &str, value: &str| -> Result<Range, String> {
        let (a, b) = value
            .split_once("..")
            .ok_or_else(|| format!("'{key}' wants a range like 0.2..0.8, got '{value}'"))?;
        Ok(Range(num(key, a.trim())?, num(key, b.trim())?))
    };
    match key {
        "seed" => w.seed = num(key, value)?,
        "grid-lon" => w.nlon = num(key, value)?,
        "grid-lat" => w.nlat = num(key, value)?,
        "land-fraction" => w.land_fraction = num(key, value)?,
        "population" => w.n_agents = num(key, value)?,
        "polities" => w.n_polities = num(key, value)?,
        "fossil-endowment" => w.fossil_endowment = num(key, value)?,
        "mineral-endowment" => w.mineral_endowment = num(key, value)?,
        "patience" => w.patience = range(key, value)?,
        "risk-aversion" => w.risk_aversion = range(key, value)?,
        "fairness" => w.fairness = range(key, value)?,
        "conformity" => w.conformity = range(key, value)?,
        other => return Err(format!("unknown world key '{other}'")),
    }
    Ok(())
}

fn set_society_key(s: &mut SocietyParams, key: &str, value: &str) -> Result<(), String> {
    let frac = |key: &str, value: &str| -> Result<f64, String> {
        let v: f64 = num(key, value)?;
        if !(0.0..=1.0).contains(&v) {
            return Err(format!("'{key}' must be in [0,1], got {v}"));
        }
        Ok(v)
    };
    match key {
        "property" => {
            s.property = match value {
                "open-access" => PropertyRegime::OpenAccess,
                "commons-quota" => PropertyRegime::CommonsQuota,
                "private" => PropertyRegime::Private,
                other => {
                    return Err(format!(
                        "unknown property regime '{other}' (open-access | commons-quota | private)"
                    ))
                }
            }
        }
        "conservation-quota" => s.conservation_quota = frac(key, value)?,
        "tax-rate" => s.tax_rate = frac(key, value)?,
        "tax-progressivity" => s.tax_progressivity = frac(key, value)?,
        "transfer" => {
            s.transfer = match value {
                "none" => TransferRegime::None,
                "floor" => TransferRegime::Floor,
                "universal-dividend" => TransferRegime::UniversalDividend,
                other => {
                    return Err(format!(
                        "unknown transfer regime '{other}' (none | floor | universal-dividend)"
                    ))
                }
            }
        }
        "education-share" => s.education_share = frac(key, value)?,
        "infrastructure-share" => s.infrastructure_share = frac(key, value)?,
        "research-share" => s.research_share = frac(key, value)?,
        "enforcement-share" => s.enforcement_share = frac(key, value)?,
        "carbon-price" => s.carbon_price = num::<f64>(key, value)?.max(0.0),
        "migration-openness" => s.migration_openness = frac(key, value)?,
        "governance" => {
            s.governance = match value {
                "fixed" => GovernanceRegime::Fixed,
                "majority" => GovernanceRegime::Majority,
                "wealth-weighted" => GovernanceRegime::WealthWeighted,
                other => {
                    return Err(format!(
                        "unknown governance '{other}' (fixed | majority | wealth-weighted)"
                    ))
                }
            }
        }
        "vote-period" => s.vote_period = num(key, value)?,
        other => return Err(format!("unknown society key '{other}'")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_scenario_with_overrides() {
        let text = r#"
name = test
[world]
seed = 7
grid-lon = 24
grid-lat = 12
population = 500
polities = 3
patience = 0.4..0.9

[society]
tax-rate = 0.1
transfer = floor
property = commons-quota
conservation-quota = 0.3

[society 2]
property = open-access
tax-rate = 0
"#;
        let sc = Scenario::parse(text).unwrap();
        assert_eq!(sc.name, "test");
        assert_eq!(sc.world.seed, 7);
        assert_eq!(sc.world.n_polities, 3);
        assert_eq!(sc.world.patience, Range(0.4, 0.9));
        assert_eq!(sc.societies.len(), 3);
        assert_eq!(sc.societies[0].property, PropertyRegime::CommonsQuota);
        assert_eq!(sc.societies[0].tax_rate, 0.1);
        assert_eq!(sc.societies[1].transfer, TransferRegime::Floor);
        assert_eq!(sc.societies[2].property, PropertyRegime::OpenAccess);
        assert_eq!(sc.societies[2].tax_rate, 0.0);
    }

    #[test]
    fn strict_errors_and_no_outcome_keys() {
        for (bad, needle) in [
            ("[weather]", "unknown section"),
            ("[world]\ngdp = 5", "unknown world key"),
            ("[world]\ngini = 0.3", "unknown world key"),
            ("[society]\nlife-expectancy = 80", "unknown society key"),
            ("[society]\ntemperature = 288", "unknown society key"),
            ("[society]\ntax-rate = 1.5", "[0,1]"),
            ("[society]\nproperty = feudal", "unknown property regime"),
            ("[society]\ntransfer = manna", "unknown transfer regime"),
            ("[society]\ngovernance = monarchy", "unknown governance"),
            ("[world]\npatience = 0.5", "range"),
            ("[society 9]\ntax-rate = 0.1", "out of range"),
            ("words", "key = value"),
            ("[world\nx = 1", "malformed"),
            ("flavour = sweet", "unknown top-level key"),
        ] {
            let err = Scenario::parse(bad).unwrap_err();
            assert!(err.contains(needle), "{bad:?} should mention {needle:?}, got {err}");
        }
    }

    #[test]
    fn default_society_is_the_null_baseline() {
        let s = SocietyParams::default();
        assert_eq!(s.property, PropertyRegime::OpenAccess);
        assert_eq!(s.tax_rate, 0.0);
        assert_eq!(s.transfer, TransferRegime::None);
        assert_eq!(s.carbon_price, 0.0);
    }
}
