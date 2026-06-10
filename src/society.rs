//! **Society archetypes**: named, ready-to-run [`Scenario`]s that compose the
//! planet with a recognisable way of operating it — the starting points you
//! load, then mass-do or undo with the counterfactual/search layers. They are
//! archetypes to be calibrated against real data, not portraits of countries.
//!
//! Every archetype sets only *mechanisms* ([`SocietyParams`]); what each one
//! produces (its inequality, its temperature path, its well-being) is measured.

use crate::config::*;

/// The registry of archetype names.
pub const ARCHETYPES: [&str; 5] = [
    "laissez-faire",
    "social-democracy",
    "eco-technocracy",
    "extractive-autocracy",
    "degrowth-commons",
];

/// A short description for `list`.
pub fn describe(name: &str) -> Option<&'static str> {
    Some(match name {
        "laissez-faire" => "minimal state: private property, no tax, open borders, no climate policy",
        "social-democracy" => "progressive tax funds schooling, infrastructure and a redistributive floor",
        "eco-technocracy" => "fixed expert policy: strong carbon price, research, conservation quota",
        "extractive-autocracy" => "wealth-weighted rule extracts and entrenches, little reinvested",
        "degrowth-commons" => "commons quotas, universal dividend, heavy carbon price, closed-ish borders",
        _ => return None,
    })
}

/// Build an archetype scenario on the given planet config.
pub fn archetype(name: &str, world: WorldConfig) -> Option<Scenario> {
    let s = match name {
        "laissez-faire" => SocietyParams {
            property: PropertyRegime::Private,
            governance: GovernanceRegime::WealthWeighted,
            migration_openness: 1.0,
            ..SocietyParams::default()
        },
        "social-democracy" => SocietyParams {
            property: PropertyRegime::Private,
            tax_rate: 0.28,
            tax_progressivity: 0.8,
            transfer: TransferRegime::Floor,
            education_share: 0.3,
            infrastructure_share: 0.25,
            research_share: 0.15,
            enforcement_share: 0.1,
            carbon_price: 3.0,
            migration_openness: 0.7,
            governance: GovernanceRegime::Majority,
            vote_period: 15,
            ..SocietyParams::default()
        },
        "eco-technocracy" => SocietyParams {
            property: PropertyRegime::CommonsQuota,
            conservation_quota: 0.3,
            tax_rate: 0.22,
            tax_progressivity: 0.5,
            transfer: TransferRegime::Floor,
            education_share: 0.25,
            infrastructure_share: 0.2,
            research_share: 0.35,
            enforcement_share: 0.15,
            carbon_price: 7.0,
            migration_openness: 0.5,
            governance: GovernanceRegime::Fixed,
            ..SocietyParams::default()
        },
        "extractive-autocracy" => SocietyParams {
            property: PropertyRegime::Private,
            tax_rate: 0.35,
            tax_progressivity: 0.0,
            transfer: TransferRegime::None,
            enforcement_share: 0.3,
            migration_openness: 0.2,
            governance: GovernanceRegime::WealthWeighted,
            vote_period: 40,
            ..SocietyParams::default()
        },
        "degrowth-commons" => SocietyParams {
            property: PropertyRegime::CommonsQuota,
            conservation_quota: 0.25,
            tax_rate: 0.3,
            tax_progressivity: 0.7,
            transfer: TransferRegime::UniversalDividend,
            education_share: 0.3,
            infrastructure_share: 0.1,
            research_share: 0.2,
            enforcement_share: 0.15,
            carbon_price: 10.0,
            migration_openness: 0.3,
            governance: GovernanceRegime::Majority,
            vote_period: 20,
            ..SocietyParams::default()
        },
        _ => return None,
    };
    Some(Scenario::new(name, world).with_uniform_society(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    #[test]
    fn every_archetype_builds_and_runs() {
        let mut world = WorldConfig::default();
        world.nlon = 24;
        world.nlat = 12;
        world.n_agents = 800;
        for name in ARCHETYPES {
            assert!(describe(name).is_some(), "{name} needs a description");
            let sc = archetype(name, world.clone()).expect("archetype builds");
            let mut w = World::from_scenario(&sc);
            for _ in 0..40 {
                w.step();
            }
            // The archetypes are deliberately survivable starting points.
            assert!(w.people.alive_count() > 0, "{name} should sustain life");
        }
        assert!(archetype("atlantis", world).is_none());
    }
}
