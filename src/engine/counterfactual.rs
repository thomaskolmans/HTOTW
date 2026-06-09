//! **Mass do/undo counterfactuals** (Phase 11): take a society (Phase 10), add
//! or repeal laws — one at a time or *every combination at once* — and measure
//! how the world would have gone.
//!
//! This is the project's stated purpose made executable: *"input existing
//! societies and then mass-do or undo laws and structures to see how the world
//! would work."* An [`Edit`] does (`wealth-tax=0.3`) or undoes (`wealth-tax`)
//! one law; [`whatif`] runs the society with and without the edits over the
//! **same seed ensemble** (so the only difference is the law, never the luck);
//! [`sweep`] enumerates **every subset** of the society's law stack and ranks
//! the regimes by the measured welfare functional (prosperity × equity ×
//! sustainability × survival, `calibration::welfare`). Every number in a
//! verdict is an emergent measurement — a counterfactual cannot "decide" its
//! outcome any more than a normal run can.

use super::calibration::{evaluate, Outcome, Verdict};
use super::society::{Law, SocietySpec, LAW_NAMES};

/// One change to a society's law stack.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Enact a law (replacing any existing law of the same name).
    Do(Law),
    /// Repeal the named law. Strict: repealing a law the society does not have
    /// is an error (a typo must not silently run the wrong experiment).
    Undo(String),
}

impl Edit {
    /// Parse a `--do` spec: `name=value` (e.g. `wealth-tax=0.3`) or a bare
    /// parameterless name (`property-rights`).
    pub fn parse_do(spec: &str) -> Result<Edit, String> {
        let (name, value) = match spec.split_once('=') {
            Some((n, v)) => (n.trim(), v.trim()),
            None => (spec.trim(), "on"),
        };
        Ok(Edit::Do(Law::parse(name, value)?))
    }

    /// Parse a `--undo` spec: a law name.
    pub fn parse_undo(spec: &str) -> Result<Edit, String> {
        let name = spec.trim();
        if !LAW_NAMES.contains(&name) {
            return Err(format!(
                "unknown law '{name}'. available: {}",
                LAW_NAMES.join(", ")
            ));
        }
        Ok(Edit::Undo(name.to_string()))
    }
}

/// Apply a list of edits to a law stack, in order. `Do` replaces a same-named
/// law in place (or appends); `Undo` removes it (error if absent).
pub fn apply_edits(laws: &[Law], edits: &[Edit]) -> Result<Vec<Law>, String> {
    let mut out: Vec<Law> = laws.to_vec();
    for edit in edits {
        match edit {
            Edit::Do(law) => {
                if let Some(slot) = out.iter_mut().find(|l| l.name() == law.name()) {
                    *slot = law.clone();
                } else {
                    out.push(law.clone());
                }
            }
            Edit::Undo(name) => {
                let before = out.len();
                out.retain(|l| l.name() != name.as_str());
                if out.len() == before {
                    return Err(format!(
                        "cannot undo '{name}': the society has no such law (has: {})",
                        if laws.is_empty() {
                            "none".to_string()
                        } else {
                            laws.iter().map(Law::name).collect::<Vec<_>>().join(", ")
                        }
                    ));
                }
            }
        }
    }
    Ok(out)
}

/// The result of one counterfactual: the society as specified vs. the society
/// with the edits applied, evaluated on the **same seeds** — plus the welfare
/// verdict. All fields are measured outcome distributions.
#[derive(Debug, Clone)]
pub struct WhatIf {
    /// The law stack as specified.
    pub baseline_laws: Vec<Law>,
    /// The law stack after the edits.
    pub variant_laws: Vec<Law>,
    pub baseline: Outcome,
    pub variant: Outcome,
    /// `First` = the baseline scores higher measured welfare, `Second` = the
    /// edited society does.
    pub verdict: Verdict,
}

/// Run a society with and without a set of law edits, same seeds and ticks, and
/// compare the measured welfare. The geography, biology, psychology and luck
/// are identical across the two arms — the laws are the only difference, so the
/// delta in every emergent moment is attributable to them.
pub fn whatif(
    spec: &SocietySpec,
    edits: &[Edit],
    seeds: &[u64],
    ticks: usize,
) -> Result<WhatIf, String> {
    let variant_laws = apply_edits(&spec.laws, edits)?;
    let mut variant_spec = spec.clone();
    variant_spec.laws = variant_laws.clone();
    variant_spec.name = format!("{} (edited)", spec.name);

    let baseline = evaluate(&spec.scenario(), seeds, ticks);
    let variant = evaluate(&variant_spec.scenario(), seeds, ticks);
    let verdict = if baseline.welfare > variant.welfare {
        Verdict::First
    } else if variant.welfare > baseline.welfare {
        Verdict::Second
    } else {
        Verdict::Tie
    };
    Ok(WhatIf {
        baseline_laws: spec.laws.clone(),
        variant_laws,
        baseline,
        variant,
        verdict,
    })
}

/// One regime in a sweep: a law subset and its measured outcome distribution.
#[derive(Debug, Clone)]
pub struct SweepEntry {
    pub laws: Vec<Law>,
    pub outcome: Outcome,
}

impl SweepEntry {
    /// `law+law+law`, or `(no laws)` for the empty set.
    pub fn label(&self) -> String {
        if self.laws.is_empty() {
            "(no laws)".to_string()
        } else {
            self.laws.iter().map(Law::name).collect::<Vec<_>>().join("+")
        }
    }
}

/// Hard cap on sweepable laws (2^12 = 4096 regimes is already a lot of runs).
pub const SWEEP_MAX_LAWS: usize = 12;

/// **Mass do/undo**: evaluate *every subset* of the society's declared law
/// stack (2^n regimes, same seeds and ticks for all), and return them ranked by
/// measured welfare, best first. The top entry is the engine's answer to "which
/// combination of this society's laws serves it best?" — an answer read off
/// emergent outcomes, never assumed. Deterministic: ties break toward fewer
/// laws, then by label.
pub fn sweep(spec: &SocietySpec, seeds: &[u64], ticks: usize) -> Result<Vec<SweepEntry>, String> {
    let n = spec.laws.len();
    if n > SWEEP_MAX_LAWS {
        return Err(format!(
            "sweep over {n} laws would mean {} regimes (max {SWEEP_MAX_LAWS} laws)",
            1u64 << n
        ));
    }
    let mut entries = Vec::with_capacity(1 << n);
    for mask in 0u64..(1u64 << n) {
        let laws: Vec<Law> = spec
            .laws
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, l)| l.clone())
            .collect();
        let mut sub = spec.clone();
        sub.laws = laws.clone();
        let entry = SweepEntry {
            outcome: evaluate(&sub.scenario(), seeds, ticks),
            laws,
        };
        entries.push(entry);
    }
    entries.sort_by(|a, b| {
        b.outcome
            .welfare
            .partial_cmp(&a.outcome.welfare)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.laws.len().cmp(&b.laws.len()))
            .then(a.label().cmp(&b.label()))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str) -> SocietySpec {
        SocietySpec::parse(text).unwrap()
    }

    #[test]
    fn edits_do_undo_and_replace() {
        let laws = vec![Law::WealthTax(0.2), Law::HarvestQuota(0.3)];

        // Undo removes exactly the named law.
        let undone = apply_edits(&laws, &[Edit::parse_undo("wealth-tax").unwrap()]).unwrap();
        assert_eq!(undone, vec![Law::HarvestQuota(0.3)]);

        // Do replaces a same-named law in place, or appends a new one.
        let edited = apply_edits(
            &laws,
            &[
                Edit::parse_do("wealth-tax=0.5").unwrap(),
                Edit::parse_do("property-rights").unwrap(),
            ],
        )
        .unwrap();
        assert_eq!(
            edited,
            vec![Law::WealthTax(0.5), Law::HarvestQuota(0.3), Law::PropertyRights]
        );

        // Strictness: undoing an absent law, or a typo, is an error.
        assert!(apply_edits(&laws, &[Edit::Undo("decarbonize".into())]).is_err());
        assert!(Edit::parse_undo("wealth-taxx").is_err());
        assert!(Edit::parse_do("teleport=1").is_err());
    }

    /// The headline counterfactual: undoing the conservation law of a
    /// stewardship society on a fragile commons makes the measured world worse
    /// — the baseline wins the welfare verdict, and the commons-health delta
    /// shows exactly why. Same seeds in both arms: the law is the only
    /// difference.
    #[test]
    fn undoing_conservation_makes_a_fragile_world_worse() {
        let s = spec(
            "name = steward\nbase = fragile-commons\n[laws]\nharvest-quota = 0.3\n",
        );
        let w = whatif(
            &s,
            &[Edit::parse_undo("harvest-quota").unwrap()],
            &[1, 7],
            250,
        )
        .unwrap();
        assert_eq!(w.verdict, Verdict::First, "the quota society should out-score its repeal");
        assert!(w.baseline.welfare > w.variant.welfare);
        assert!(
            w.baseline.mean(|r| r.commons_health) > w.variant.mean(|r| r.commons_health),
            "repealing the quota should degrade the measured commons"
        );
        assert!(w.variant_laws.is_empty());
        assert_eq!(w.baseline_laws.len(), 1);
    }

    /// And the inverse "do": giving the lawless frontier a quota improves it.
    #[test]
    fn doing_a_conservation_law_improves_the_frontier() {
        let s = spec("name = frontier\nbase = fragile-commons\n");
        let w = whatif(
            &s,
            &[Edit::parse_do("harvest-quota=0.3").unwrap()],
            &[1, 7],
            250,
        )
        .unwrap();
        assert_eq!(w.verdict, Verdict::Second, "enacting the quota should win");
    }

    /// The mass do/undo: a sweep enumerates every law subset, ranks regimes by
    /// measured welfare, and its best entry is at least as good as both the
    /// full stack and the empty stack (it considered both).
    #[test]
    fn sweep_enumerates_and_ranks_every_law_subset() {
        let s = spec(
            "name = mixed\nbase = fragile-commons\n[laws]\nwealth-tax = 0.2\nredistribute = 1.0\nharvest-quota = 0.3\n",
        );
        let entries = sweep(&s, &[1, 7], 150).unwrap();
        assert_eq!(entries.len(), 8, "2^3 regimes");
        // Ranked best-first.
        for pair in entries.windows(2) {
            assert!(pair[0].outcome.welfare >= pair[1].outcome.welfare);
        }
        // Both extremes were considered.
        assert!(entries.iter().any(|e| e.laws.is_empty()));
        assert!(entries.iter().any(|e| e.laws.len() == 3));
        // Labels are stable and readable.
        assert!(entries.iter().any(|e| e.label() == "(no laws)"));
        // Determinism: a second sweep is bit-identical.
        let again = sweep(&s, &[1, 7], 150).unwrap();
        for (a, b) in entries.iter().zip(again.iter()) {
            assert_eq!(a.label(), b.label());
            assert_eq!(a.outcome.welfare.to_bits(), b.outcome.welfare.to_bits());
        }
    }

    #[test]
    fn sweep_guards_against_combinatorial_explosion() {
        let mut s = spec("name = x\n");
        s.laws = (0..13).map(|i| Law::HarvestQuota(i as f64 / 13.0)).collect();
        assert!(sweep(&s, &[1], 10).is_err());
    }
}
