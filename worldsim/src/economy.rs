//! The **economy**: a multi-sector production-and-exchange system per polity.
//! Sectors are **food** (farming, drawing on land/soil/water/climate),
//! **water** (provisioning), **fuel** (fossil — finite and emitting — vs.
//! **clean** — learning-curve-driven and carbon-free), **materials** (mining
//! finite deposits) and **manufactured goods**. Each sector has a stock of
//! **capital** that depreciates and is rebuilt from investment, and a
//! **productivity** that rises with cumulative output (learning-by-doing,
//! Wright/Arrow). Output uses a Cobb–Douglas capital–labour function scaled by
//! resource access and human capital.
//!
//! **Prices, GDP, the energy mix and the carbon intensity of growth are all
//! emergent** — read off realised scarcity (need vs. supply) each year. Nothing
//! here sets a macro outcome; the society parameters mold only mechanisms
//! (taxes, investment shares, a carbon price, conservation quotas).

use crate::constants::*;

/// The sectors, in a fixed order (determinism of every tally).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sector {
    Food,
    Water,
    FossilFuel,
    CleanEnergy,
    Materials,
    Goods,
}

impl Sector {
    pub const ALL: [Sector; 6] = [
        Sector::Food,
        Sector::Water,
        Sector::FossilFuel,
        Sector::CleanEnergy,
        Sector::Materials,
        Sector::Goods,
    ];
    pub const N: usize = 6;
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            Sector::Food => "food",
            Sector::Water => "water",
            Sector::FossilFuel => "fossil",
            Sector::CleanEnergy => "clean",
            Sector::Materials => "materials",
            Sector::Goods => "goods",
        }
    }
    /// Learning rate (clean energy learns fastest; Way et al. 2022).
    fn learning_rate(self) -> f64 {
        match self {
            Sector::CleanEnergy => LEARNING_RATE_CLEAN,
            _ => LEARNING_RATE,
        }
    }
}

/// One polity's productive economy.
#[derive(Debug, Clone)]
pub struct Economy {
    /// Capital stock per sector.
    pub capital: [f64; Sector::N],
    /// Productivity (total-factor) per sector; rises via learning-by-doing.
    pub productivity: [f64; Sector::N],
    /// Cumulative output per sector (the learning-curve experience base).
    pub cumulative: [f64; Sector::N],
    /// Output produced this year per sector (a flow; measured).
    pub output: [f64; Sector::N],
    /// Emergent price per sector (numéraire per unit; food = 1 by definition).
    pub price: [f64; Sector::N],
    /// Treasury: collected revenue awaiting allocation (numéraire).
    pub treasury: f64,
}

/// Base labour productivity: numéraire units one effective worker-year yields
/// at unit capital and unit resource access (one farmer feeds several people;
/// FAO labour-productivity scale). Output is constant-returns in labour at the
/// margin — the *diminishing* returns and the carrying-capacity ceiling come
/// from the finite resource each sector draws on (handled in `World`), not from
/// a sublinear labour exponent, which would make per-capita output vanish as a
/// population grows.
pub const BASE_YIELD: f64 = 12.0;

impl Default for Economy {
    fn default() -> Economy {
        Economy {
            // A modest initial capital and unit productivity in every sector,
            // with clean energy starting expensive-and-immature (low cumulative
            // experience) so its rise has to be *earned* by deployment.
            capital: [1.0; Sector::N],
            productivity: [1.0; Sector::N],
            cumulative: [1.0; Sector::N],
            output: [0.0; Sector::N],
            price: [1.0; Sector::N],
            treasury: 0.0,
        }
    }
}

impl Economy {
    /// The labour–capital **effort** a sector brings to bear: constant-returns
    /// in effective labour (so a populous society can feed itself), with
    /// capital and learned productivity as multipliers and a resource-access
    /// factor. `effort = A · BASE_YIELD · K^α · L · access`. The realised output
    /// is this effort passed through a saturating utilization against the
    /// sector's finite resource ceiling (in `World`), which is where the
    /// diminishing returns and the carrying capacity actually live.
    pub fn potential(&self, s: Sector, labour: f64, access: f64) -> f64 {
        let i = s.index();
        let k = self.capital[i].max(1e-9);
        let l = labour.max(0.0) * access.max(0.0);
        self.productivity[i] * BASE_YIELD * k.powf(CAPITAL_ELASTICITY) * l
    }

    /// Record realised output and bank the learning: productivity rises with
    /// the log of cumulative output (a progress ratio per doubling).
    pub fn record_output(&mut self, s: Sector, produced: f64) {
        let i = s.index();
        self.output[i] = produced;
        let before = self.cumulative[i];
        self.cumulative[i] += produced.max(0.0);
        if self.cumulative[i] > before && before > 0.0 {
            let doublings = (self.cumulative[i] / before).log2();
            self.productivity[i] *= (1.0 + s.learning_rate()).powf(doublings.max(0.0));
        }
    }

    /// Depreciate capital and rebuild it from this year's sector investment.
    pub fn invest(&mut self, s: Sector, amount: f64) {
        let i = s.index();
        self.capital[i] = self.capital[i] * (1.0 - DEPRECIATION) + amount.max(0.0);
    }

    /// Update the emergent price of a sector from realised scarcity: the ratio
    /// of demand (need) to supply (output + carry-over), passed through a
    /// bounded elasticity so a shortfall raises the price and a glut lowers it.
    /// Food is the numéraire and stays at 1. Hayek: the price *is* the
    /// scarcity signal, not an input.
    pub fn update_price(&mut self, s: Sector, demand: f64, supply: f64) {
        let i = s.index();
        if s == Sector::Food {
            self.price[i] = 1.0;
            return;
        }
        let scarcity = (demand + 1e-6) / (supply + 1e-6);
        // Smooth toward the scarcity ratio (clamped to a sane band).
        let target = scarcity.clamp(0.1, 10.0);
        self.price[i] += 0.3 * (target - self.price[i]);
        self.price[i] = self.price[i].clamp(0.05, 20.0);
    }

    /// Total GDP this year = value of all sector output at emergent prices.
    pub fn gdp(&self) -> f64 {
        Sector::ALL
            .iter()
            .map(|&s| self.output[s.index()] * self.price[s.index()])
            .sum()
    }

    /// Clean-energy share of total energy produced this year (the decarbonisation
    /// instrument input): clean / (clean + fossil).
    pub fn clean_share(&self) -> f64 {
        let clean = self.output[Sector::CleanEnergy.index()];
        let fossil = self.output[Sector::FossilFuel.index()];
        if clean + fossil <= 0.0 {
            0.0
        } else {
            clean / (clean + fossil)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effort_scales_with_labour_capital_and_access() {
        let mut e = Economy::default();
        // Constant-returns in labour at the margin (so a populous society can
        // feed itself; the diminishing returns live in the resource ceiling).
        let y1 = e.potential(Sector::Goods, 1.0, 1.0);
        let y2 = e.potential(Sector::Goods, 2.0, 1.0);
        assert!((y2 - 2.0 * y1).abs() < 1e-9, "effort is linear in labour");
        // Resource access scales effort.
        assert!(e.potential(Sector::Food, 1.0, 0.5) < e.potential(Sector::Food, 1.0, 1.0));
        // Capital deepening helps with diminishing returns (α < 1).
        let k_low = e.potential(Sector::Goods, 1.0, 1.0);
        e.capital[Sector::Goods.index()] = 4.0;
        let k_hi = e.potential(Sector::Goods, 1.0, 1.0);
        assert!(k_hi > k_low, "more capital, more effort");
        assert!(k_hi < 4.0 * k_low, "but capital has diminishing returns (α<1)");
    }

    #[test]
    fn learning_by_doing_lowers_the_cost_of_scale() {
        let mut e = Economy::default();
        let p0 = e.productivity[Sector::CleanEnergy.index()];
        for _ in 0..20 {
            e.record_output(Sector::CleanEnergy, 5.0);
        }
        let p1 = e.productivity[Sector::CleanEnergy.index()];
        assert!(p1 > p0 * 1.5, "deploying clean energy should make it much cheaper");
        // Clean learns faster than a conventional sector at equal cumulative
        // growth.
        let mut f = Economy::default();
        for _ in 0..20 {
            f.record_output(Sector::Materials, 5.0);
        }
        assert!(
            e.productivity[Sector::CleanEnergy.index()]
                > f.productivity[Sector::Materials.index()],
            "clean energy should have the steeper learning curve"
        );
    }

    #[test]
    fn prices_track_scarcity_food_is_numeraire() {
        let mut e = Economy::default();
        for _ in 0..20 {
            e.update_price(Sector::Goods, 10.0, 2.0); // chronic shortage
        }
        assert!(e.price[Sector::Goods.index()] > 1.5, "scarcity raises price");
        for _ in 0..40 {
            e.update_price(Sector::Goods, 2.0, 10.0); // glut
        }
        assert!(e.price[Sector::Goods.index()] < 1.0, "a glut lowers price");
        e.update_price(Sector::Food, 99.0, 1.0);
        assert_eq!(e.price[Sector::Food.index()], 1.0, "food is the numéraire");
    }

    #[test]
    fn capital_depreciates_and_rebuilds() {
        let mut e = Economy::default();
        let k0 = e.capital[Sector::Goods.index()];
        e.invest(Sector::Goods, 0.0);
        assert!(e.capital[Sector::Goods.index()] < k0, "no investment ⇒ decay");
        for _ in 0..50 {
            e.invest(Sector::Goods, 1.0);
        }
        assert!(e.capital[Sector::Goods.index()] > k0, "sustained investment grows capital");
    }
}
