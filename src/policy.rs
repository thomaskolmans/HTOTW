//! The [`Policy`] trait and the [`PolicyStack`] that composes many of them.
//!
//! A *policy* is a parameterised intervention that becomes active at some year
//! and, while active, contributes to the shared [`PolicyEffects`] accumulator.
//! Policies are deliberately *declarative*: they describe their effect, they do
//! not run the simulation. This keeps them small, testable, and trivially
//! stackable (see [`crate::effects`] for why stacking is order-independent).

use crate::effects::PolicyEffects;
use crate::state::{Ideology, WorldState};

/// A single parameterised policy.
///
/// Implementors typically store a `start_year` and a small number of strength
/// parameters, then express their effect in [`Policy::apply`].
pub trait Policy {
    /// Human-readable name, used in reports and CSV headers.
    fn name(&self) -> &str;

    /// First calendar year the policy is in force.
    fn start_year(&self) -> u32;

    /// Whether the policy is active in `year`. Default: active from
    /// `start_year` onward (a permanent policy). Override for sunset clauses.
    fn is_active(&self, year: u32) -> bool {
        year >= self.start_year()
    }

    /// Contribute this policy's levers to `eff`, possibly depending on the
    /// current `state` (e.g. a tax that scales with GDP). Only called when the
    /// policy [`is_active`](Policy::is_active) in `year`.
    ///
    /// Implementations must only *add to* / *multiply into* `eff`, never reset
    /// it — that is what makes stacking well-defined.
    fn apply(&self, year: u32, state: &WorldState, eff: &mut PolicyEffects);

    /// The policy's position on the three ideology axes, used by an endogenous
    /// government to judge whether the policy fits its mandate. Default neutral.
    fn position(&self) -> Ideology {
        Ideology::centrist()
    }

    /// A one-line description of what the policy does and its parameters.
    /// Default is just the name; override to be helpful in `--list` output.
    fn describe(&self) -> String {
        self.name().to_string()
    }
}

/// An ordered collection of policies that are all evaluated each year.
///
/// "Ordered" is for display only — because policies merely accumulate into
/// [`PolicyEffects`], the resulting effect is independent of order.
#[derive(Default)]
pub struct PolicyStack {
    policies: Vec<Box<dyn Policy>>,
}

impl PolicyStack {
    /// An empty stack (the laissez-faire / business-as-usual baseline).
    pub fn new() -> Self {
        PolicyStack { policies: Vec::new() }
    }

    /// Add a policy to the stack.
    pub fn push(&mut self, policy: Box<dyn Policy>) {
        self.policies.push(policy);
    }

    /// Number of policies in the stack.
    pub fn len(&self) -> usize {
        self.policies.len()
    }

    /// Whether the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }

    /// Iterate over the policies (for reporting).
    pub fn iter(&self) -> impl Iterator<Item = &dyn Policy> {
        self.policies.iter().map(|b| b.as_ref())
    }

    /// Evaluate every *active* policy for `year` against `state` and return the
    /// combined [`PolicyEffects`]. This is the only entry point the simulation
    /// needs.
    pub fn effects_for(&self, year: u32, state: &WorldState) -> PolicyEffects {
        let mut eff = PolicyEffects::neutral();
        for p in &self.policies {
            if p.is_active(year) {
                p.apply(year, state, &mut eff);
            }
        }
        eff
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::Scenario;

    /// A tiny test policy: adds a fixed amount of redistribution once active.
    struct FixedRedistribution {
        start: u32,
        amount: f64,
    }
    impl Policy for FixedRedistribution {
        fn name(&self) -> &str {
            "fixed-redistribution"
        }
        fn start_year(&self) -> u32 {
            self.start
        }
        fn apply(&self, _year: u32, _state: &WorldState, eff: &mut PolicyEffects) {
            eff.redistribution += self.amount;
        }
    }

    #[test]
    fn inactive_before_start_year() {
        let mut stack = PolicyStack::new();
        stack.push(Box::new(FixedRedistribution { start: 2030, amount: 0.1 }));
        let state = Scenario::baseline_2025().initial_state();
        let eff = stack.effects_for(2025, &state);
        assert_eq!(eff.redistribution, 0.0, "should be inactive before start");
        let eff = stack.effects_for(2030, &state);
        assert_eq!(eff.redistribution, 0.1, "should be active at start year");
    }

    #[test]
    fn stacking_is_additive_and_order_independent() {
        let state = Scenario::baseline_2025().initial_state();

        let mut a = PolicyStack::new();
        a.push(Box::new(FixedRedistribution { start: 0, amount: 0.1 }));
        a.push(Box::new(FixedRedistribution { start: 0, amount: 0.2 }));

        let mut b = PolicyStack::new();
        // reverse declaration order
        b.push(Box::new(FixedRedistribution { start: 0, amount: 0.2 }));
        b.push(Box::new(FixedRedistribution { start: 0, amount: 0.1 }));

        let ea = a.effects_for(2025, &state);
        let eb = b.effects_for(2025, &state);
        assert!((ea.redistribution - 0.3).abs() < 1e-12);
        assert_eq!(ea, eb, "stacking must be order-independent");
    }
}
