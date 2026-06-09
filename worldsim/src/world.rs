//! The **World**: the annual tick that couples planet, people, economy and the
//! configured societies into one closed loop, and the read-only
//! [`World::measure`] that reads every macro quantity back out.
//!
//! ## The yearly order of operations (the "laws of physics" of a year)
//!
//! 1. **Production & exchange** (per polity): assign the year's effective
//!    labour across sectors by emergent demand and price, produce against the
//!    planet's resource access (farmland, deposits, fisheries), bank
//!    learning-by-doing, set emergent prices, and write physical pressures
//!    (harvest, land use, emissions) onto the planet.
//! 2. **Fiscal mechanism**: collect taxes and the carbon price into the
//!    treasury; spend it on schooling, infrastructure, research, enforcement
//!    and transfers — *mechanisms set by [`SocietyParams`]*, never outcomes.
//! 3. **Consumption**: each person meets food/water/fuel/goods needs as far as
//!    availability and budget allow; shortfalls become deprivation; well-being
//!    tracks realised satisfaction.
//! 4. **Vital events**: mortality (Gompertz–Makeham + deprivation + heat) and
//!    fertility (an individual decision gated by wealth buffer and risk
//!    aversion), with heritable psychology.
//! 5. **Migration**: the deprived move toward opportunity, subject to border
//!    openness.
//! 6. **Planet**: integrate carbon, temperature, the water cycle and ecosystem
//!    renewal.
//! 7. **Governance**: periodic referenda may move a polity's dials.
//!
//! Determinism is total: same config + seed ⇒ bit-identical history.

use crate::config::*;
use crate::constants::*;
use crate::economy::{Economy, Sector};
use crate::measure::{gini, Measurements};
use crate::people::People;
use crate::planet::Planet;
use crate::rng::Rng;

pub struct World {
    pub year: u64,
    pub cfg: WorldConfig,
    pub planet: Planet,
    pub people: People,
    /// One economy and one parameter set per polity.
    pub econ: Vec<Economy>,
    pub society: Vec<SocietyParams>,
    pub rng: Rng,
    /// Polity owning each land cell (`u16::MAX` for ocean / unowned).
    pub polity_of_cell: Vec<u16>,
    /// Land cells belonging to each polity.
    pub polity_cells: Vec<Vec<usize>>,
    /// Per-polity institutional legitimacy in [0,1] (drives quota compliance).
    pub legitimacy: Vec<f64>,

    pub initial_population: usize,
    pub death_age_sum: f64,
    pub death_count: u64,
    /// People whose survival need went unmet this year (for the instrument).
    deprived_this_year: usize,
    /// Pristine total fossil endowment (for the remaining-fraction instrument).
    fossil0: f64,
}

const UNOWNED: u16 = u16::MAX;

impl World {
    /// Build a world from a [`WorldConfig`] with the **null society** in every
    /// polity (the baseline). Use [`World::from_scenario`] to load configured
    /// societies.
    pub fn new(cfg: &WorldConfig) -> World {
        let scenario = Scenario::new("default", cfg.clone());
        World::from_scenario(&scenario)
    }

    /// Build a world from a full scenario (planet + one society per polity).
    pub fn from_scenario(scenario: &Scenario) -> World {
        let cfg = &scenario.world;
        let mut rng = Rng::seed(cfg.seed);
        let planet = Planet::generate(cfg, &mut rng);
        let n_polities = cfg.n_polities.max(1);

        // Partition land among polities: pick seed cells, assign each land cell
        // to the nearest seed (a deterministic Voronoi over the grid).
        let land: Vec<usize> = (0..planet.cells()).filter(|&i| planet.is_land[i]).collect();
        let mut polity_of_cell = vec![UNOWNED; planet.cells()];
        let mut polity_cells = vec![Vec::new(); n_polities];
        if !land.is_empty() {
            let mut seeds = Vec::with_capacity(n_polities);
            for _ in 0..n_polities {
                seeds.push(land[rng.below(land.len())]);
            }
            for &c in &land {
                let (cx, cy) = (c % planet.nlon, c / planet.nlon);
                let mut best = (f64::INFINITY, 0usize);
                for (p, &s) in seeds.iter().enumerate() {
                    let (sx, sy) = (s % planet.nlon, s / planet.nlon);
                    let dx = {
                        let d = (cx as f64 - sx as f64).abs();
                        d.min(planet.nlon as f64 - d)
                    };
                    let dy = cy as f64 - sy as f64;
                    let d2 = dx * dx + dy * dy;
                    if d2 < best.0 {
                        best = (d2, p);
                    }
                }
                polity_of_cell[c] = best.1 as u16;
                polity_cells[best.1].push(c);
            }
        }

        // Habitable cells: land with positive productivity. People seed there.
        let habitable: Vec<usize> =
            land.iter().copied().filter(|&i| planet.npp0[i] > 0.05).collect();
        let habitable = if habitable.is_empty() { land.clone() } else { habitable };
        let people = People::seed(cfg, &habitable, &polity_of_cell, &mut rng);

        let fossil0: f64 = planet.fossil.iter().sum();
        let initial_population = people.alive_count();

        World {
            year: 0,
            cfg: cfg.clone(),
            planet,
            people,
            econ: vec![Economy::default(); n_polities],
            society: scenario.societies.clone(),
            rng,
            polity_of_cell,
            polity_cells,
            legitimacy: vec![0.5; n_polities],
            initial_population,
            death_age_sum: 0.0,
            death_count: 0,
            deprived_this_year: 0,
            fossil0,
        }
    }

    /// Advance the world by one year.
    pub fn step(&mut self) {
        let n_polities = self.econ.len();
        self.deprived_this_year = 0;

        // Group living people by polity (one pass; reused by every phase).
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); n_polities];
        for i in 0..self.people.len() {
            if self.people.alive[i] {
                let p = self.people.polity[i] as usize;
                if p < n_polities {
                    members[p].push(i);
                }
            }
        }

        // Scratch: this year's market income per person.
        let mut income = vec![0.0_f64; self.people.len()];

        for p in 0..n_polities {
            self.run_economy(p, &members[p], &mut income);
        }

        // Consumption, well-being and deprivation (needs the local temperature).
        for p in 0..n_polities {
            self.consume_polity(p, &members[p], &income);
        }

        // Vital events and migration draw on the shared RNG stream.
        self.vital_events(&members);
        self.migration(&members);

        // Integrate the physical planet under this year's pressures.
        self.planet.step();

        // Governance: referenda on the period boundary.
        self.governance(&members);

        self.year += 1;
    }

    /// One polity's production-exchange-fiscal year.
    fn run_economy(&mut self, p: usize, members: &[usize], income: &mut [f64]) {
        if members.is_empty() {
            return;
        }
        // Effective labour (skill-weighted working-age people) and aggregate
        // needs across the polity.
        let mut labour = 0.0;
        let mut need_food = 0.0;
        let mut need_water = 0.0;
        let mut need_fuel = 0.0;
        let mut need_goods = 0.0;
        for &i in members {
            let t = self.planet.temp[self.people.cell[i]];
            let nd = self.people.need(i, t);
            need_food += nd.food;
            need_water += nd.water;
            need_fuel += nd.fuel;
            need_goods += nd.goods;
            if (15..70).contains(&self.people.age[i]) {
                labour += self.people.skill[i];
            }
        }
        labour = labour.max(1e-6);

        // Resource access per sector (the planet's gift to production).
        let mut farm_capacity = 0.0;
        let mut fossil_avail = 0.0;
        let mut mineral_avail = 0.0;
        let mut fish_avail = 0.0;
        let cells = self.polity_cells[p].clone();
        for &c in &cells {
            farm_capacity += self.planet.npp[c] * self.planet.soil[c] * self.planet.water[c].min(1.0);
            fossil_avail += self.planet.fossil[c];
            mineral_avail += self.planet.mineral[c];
            for nb in self.planet.neighbors(c) {
                if !self.planet.is_land[nb] {
                    fish_avail += self.planet.fish[nb];
                }
            }
        }
        farm_capacity *= self.planet.yield_scale;

        // Energy demand = heating fuel + an industrial component proportional to
        // material/goods ambition. Carbon pricing tilts energy toward clean.
        let energy_need = need_fuel + 0.5 * (need_goods + need_water);
        let demand = [
            need_food.max(1e-6),
            need_water.max(1e-6),
            energy_need.max(1e-6), // split into fossil/clean below
            need_goods.max(1e-6),  // materials feed goods
            need_goods.max(1e-6),
        ];
        // Labour shares across {food, water, energy, materials, goods}.
        let dsum: f64 = demand.iter().sum();
        let share = |d: f64| labour * d / dsum;

        let soc = self.society[p].clone();
        let econ = &mut self.econ[p];

        // Production is grounded in resource CAPACITY with a saturating
        // utilization: `output = min(capacity, target · (1 − e^(−input/target)))`,
        // where `input = A · K^α · L^(1−α)` is the labour-capital effort and
        // `target` is demand. This makes output (a) bounded by the physical
        // resource ceiling (farmland, deposits) — so a land-limited carrying
        // capacity emerges Malthusianly — and (b) responsive to effort,
        // capital and learning (more productivity ⇒ the same need is met with
        // less labour). A pure Cobb–Douglas in labour alone gives vanishing
        // per-capita output and is the wrong ground truth for a populous world.
        let produce = |econ: &Economy, s: Sector, l: f64, access: f64, target: f64, cap: f64| {
            let input = econ.potential(s, l, access);
            let t = target.max(1e-9);
            let util = 1.0 - (-input / t).exp();
            (t * util).min(cap)
        };

        // FOOD — ceiling is the farmland; cultivation sets land use & erodes soil.
        let food = produce(econ, Sector::Food, share(demand[0]), 1.0, need_food, farm_capacity);
        econ.record_output(Sector::Food, food);
        econ.update_price(Sector::Food, need_food, food);

        // WATER — renewable and abundant where it rains (a generous ceiling).
        let water = produce(econ, Sector::Water, share(demand[1]), 1.0, need_water, need_water * 8.0);
        econ.record_output(Sector::Water, water);
        econ.update_price(Sector::Water, need_water, water);

        // ENERGY — split between fossil (finite, emitting, carbon-priced) and
        // clean (learning-curve, carbon-free). Allocate to the cheaper effective
        // unit cost; fossil is also ceilinged by the remaining deposit.
        let fossil_unit = econ.price[Sector::FossilFuel.index()]
            + soc.carbon_price * EMISSION_FACTOR_FOSSIL;
        let clean_unit = econ.price[Sector::CleanEnergy.index()];
        let clean_pull = (fossil_unit / (clean_unit + 1e-6)).clamp(0.05, 20.0);
        let clean_frac = (clean_pull / (1.0 + clean_pull)).clamp(0.0, 1.0);
        let energy_labour = share(demand[2]);
        let fossil = produce(
            econ,
            Sector::FossilFuel,
            energy_labour * (1.0 - clean_frac),
            1.0,
            energy_need * (1.0 - clean_frac),
            fossil_avail,
        );
        let clean = produce(
            econ,
            Sector::CleanEnergy,
            energy_labour * clean_frac,
            1.0,
            energy_need * clean_frac,
            energy_need * 4.0,
        );
        econ.record_output(Sector::FossilFuel, fossil);
        econ.record_output(Sector::CleanEnergy, clean);
        econ.update_price(Sector::FossilFuel, energy_need * (1.0 - clean_frac), fossil + 1e-6);
        econ.update_price(Sector::CleanEnergy, energy_need * clean_frac, clean + 1e-6);

        // MATERIALS — ceiling is the remaining mineral deposit.
        let materials =
            produce(econ, Sector::Materials, share(demand[3]), 1.0, need_goods, mineral_avail);
        econ.record_output(Sector::Materials, materials);
        econ.update_price(Sector::Materials, need_goods, materials + 1e-6);

        // GOODS — manufacturing needs materials as an input (access scales with
        // the material supply available per unit of goods need).
        let mat_access = (materials / need_goods.max(1e-6)).clamp(0.1, 1.5);
        let goods = produce(econ, Sector::Goods, share(demand[4]), mat_access, need_goods, need_goods * 4.0);
        econ.record_output(Sector::Goods, goods);
        econ.update_price(Sector::Goods, need_goods, goods);

        // Physical pressures onto the planet (deposits drawn down; cultivation
        // intensity; emissions = fossil burning + land-use change).
        Self::draw_down(&mut self.planet.fossil, &cells, fossil);
        Self::draw_down(&mut self.planet.mineral, &cells, materials);
        // Cultivation intensity from realised food vs. farmland capacity.
        let cult = if farm_capacity > 0.0 { (food / farm_capacity).clamp(0.0, 1.0) } else { 0.0 };
        for &c in &cells {
            if self.planet.is_land[c] {
                self.planet.land_use[c] = cult;
                // Over-intensive cultivation erodes soil.
                if cult > 0.5 {
                    self.planet.soil[c] =
                        (self.planet.soil[c] - SOIL_DEGRADE * (cult - 0.5)).max(0.1);
                }
            }
        }
        // Fisheries harvested toward the food need shortfall on the coast.
        let fish_take = (need_food - food).max(0.0).min(fish_avail);
        if fish_avail > 0.0 && fish_take > 0.0 {
            for &c in &cells {
                for nb in self.planet.neighbors(c) {
                    if !self.planet.is_land[nb] && self.planet.fish[nb] > 0.0 {
                        let frac = self.planet.fish[nb] / fish_avail;
                        self.planet.fish[nb] = (self.planet.fish[nb] - fish_take * frac).max(0.0);
                    }
                }
            }
        }
        let emissions = fossil * EMISSION_FACTOR_FOSSIL + food * cult * EMISSION_FACTOR_LANDUSE;
        // ppm conversion: scale emissions to a per-capita-of-Earth magnitude.
        self.planet.emissions_this_year += emissions / (self.initial_population as f64 / 200.0).max(1.0);

        // --- Income distribution, **survival-first**. A society feeds its
        // people before it rewards anyone: GDP first covers every member's
        // *survival* need (food/water/fuel), distributed by need and rationed
        // proportionally only in a genuine shortfall (famine). This is the
        // family/community sharing that feeds dependents — children and the
        // elderly do no wage work but are not left to starve. Only the
        // **surplus** above subsistence is distributed by human capital (the
        // labour/merit channel) and by wealth (the capital channel) — which is
        // where inequality emerges. The split is a structural fact of sharing,
        // never a social outcome. ---
        let gdp = econ.gdp();
        let mut total_survival = 0.0;
        for &i in members {
            total_survival += self.people.need(i, self.planet.temp[self.people.cell[i]]).survival();
        }
        let total_survival = total_survival.max(1e-9);
        let for_survival = gdp.min(total_survival);
        let surplus = (gdp - for_survival).max(0.0);
        let wage_pool = 0.6 * surplus;
        let capital_pool = 0.4 * surplus;
        let total_wealth: f64 =
            members.iter().map(|&i| self.people.wealth[i]).sum::<f64>().max(1e-6);
        for &i in members {
            let surv = self.people.need(i, self.planet.temp[self.people.cell[i]]).survival();
            let mut inc = for_survival * surv / total_survival;
            if (15..70).contains(&self.people.age[i]) {
                inc += wage_pool * self.people.skill[i] / labour;
            }
            inc += capital_pool * self.people.wealth[i] / total_wealth;
            income[i] = inc;
        }

        // --- Fiscal mechanism: income tax + carbon revenue → treasury. The tax
        // falls only on income **above the taxpayer's own survival need** — a
        // state cannot tax away the food in people's mouths without killing its
        // own tax base (and historically even extractive regimes leave bare
        // subsistence). Progressivity additionally exempts a share of the
        // below-mean surplus, so a flat regime taxes all surplus equally and a
        // progressive one spares the modestly-above-subsistence. ---
        let mean_surplus = {
            let mut s = 0.0;
            for &i in members {
                let surv = self.people.need(i, self.planet.temp[self.people.cell[i]]).survival();
                s += (income[i] - surv).max(0.0);
            }
            s / members.len() as f64
        };
        let mut collected = 0.0;
        for &i in members {
            let surv = self.people.need(i, self.planet.temp[self.people.cell[i]]).survival();
            let taxable_surplus = (income[i] - surv).max(0.0);
            // Progressive carve-out: spare a fraction of the first `mean_surplus`.
            let taxable = if soc.tax_progressivity > 0.0 {
                (taxable_surplus - soc.tax_progressivity * mean_surplus).max(0.0)
                    + (1.0 - soc.tax_progressivity) * taxable_surplus.min(mean_surplus)
            } else {
                taxable_surplus
            };
            let tax = (taxable * soc.tax_rate).max(0.0);
            income[i] -= tax;
            collected += tax;
        }
        let carbon_revenue = soc.carbon_price * emissions;
        let mut treasury = self.econ[p].treasury + collected + carbon_revenue;

        // Allocate the treasury (shares renormalised if they exceed 1).
        let mut shares = [
            soc.education_share,
            soc.infrastructure_share,
            soc.research_share,
            soc.enforcement_share,
        ];
        let ssum: f64 = shares.iter().sum();
        if ssum > 1.0 {
            for s in &mut shares {
                *s /= ssum;
            }
        }
        let edu = treasury * shares[0];
        let infra = treasury * shares[1];
        let research = treasury * shares[2];
        let enforce = treasury * shares[3];
        let transfer_budget = treasury - edu - infra - research - enforce;

        // Education raises human capital (children benefit most).
        if edu > 0.0 {
            let per = edu / members.len() as f64;
            for &i in members {
                let gain = per * if self.people.age[i] < 20 { 0.15 } else { 0.05 };
                self.people.skill[i] = (self.people.skill[i] + gain).min(5.0);
            }
        }
        // Infrastructure → capital across sectors; research → broad productivity.
        if infra > 0.0 {
            for s in Sector::ALL {
                self.econ[p].invest(s, infra / Sector::N as f64);
            }
        }
        if research > 0.0 {
            let bump = 1.0 + (research / (gdp + 1.0)).min(0.1);
            for s in Sector::ALL {
                self.econ[p].productivity[s.index()] *= bump;
            }
        }
        // Enforcement raises legitimacy/compliance (capacity bought with money).
        let enforce_strength = (enforce / (members.len() as f64 + 1.0)).clamp(0.0, 1.0);
        self.legitimacy[p] = (self.legitimacy[p] + 0.1 * (enforce_strength - 0.0)).clamp(0.0, 1.0);

        // Transfers: floor (means-tested) or universal dividend.
        treasury = transfer_budget.max(0.0);
        match soc.transfer {
            TransferRegime::None => {}
            TransferRegime::UniversalDividend => {
                let per = treasury / members.len() as f64;
                for &i in members {
                    income[i] += per;
                }
                treasury = 0.0;
            }
            TransferRegime::Floor => {
                // Fill the largest income shortfalls below the mean first.
                let mean: f64 = members.iter().map(|&i| income[i]).sum::<f64>()
                    / members.len() as f64;
                let total_short: f64 = members
                    .iter()
                    .map(|&i| (mean - income[i]).max(0.0))
                    .sum();
                if total_short > 0.0 {
                    let disburse = treasury.min(total_short);
                    for &i in members {
                        let short = (mean - income[i]).max(0.0);
                        if short > 0.0 {
                            income[i] += disburse * short / total_short;
                        }
                    }
                    treasury -= disburse;
                }
            }
        }
        // Unspent treasury carries over (a fiscal reserve).
        self.econ[p].treasury = treasury;
    }

    /// Consume to meet needs from income + savings, set deprivation and
    /// well-being. Availability rations physical goods; budget rations the rest.
    fn consume_polity(&mut self, p: usize, members: &[usize], income: &[f64]) {
        if members.is_empty() {
            return;
        }
        let econ = &self.econ[p];
        // Per-need availability ratio across the polity (supply / demand).
        let supply_food = econ.output[Sector::Food.index()]
            + (0..self.planet.cells())
                .filter(|&c| self.polity_of_cell[c] == p as u16)
                .count() as f64
                * 0.0; // food has no carry-over stock in this model
        // Recompute demand to ration.
        let mut dem_food = 0.0;
        let mut dem_water = 0.0;
        let mut dem_fuel = 0.0;
        let mut dem_goods = 0.0;
        for &i in members {
            let nd = self.people.need(i, self.planet.temp[self.people.cell[i]]);
            dem_food += nd.food;
            dem_water += nd.water;
            dem_fuel += nd.fuel;
            dem_goods += nd.goods;
        }
        let avail_food = ratio(supply_food, dem_food);
        let avail_water = ratio(econ.output[Sector::Water.index()], dem_water);
        let energy_supply =
            econ.output[Sector::FossilFuel.index()] + econ.output[Sector::CleanEnergy.index()];
        let avail_fuel = ratio(energy_supply, dem_fuel + 0.5 * (dem_goods + dem_water));
        let avail_goods = ratio(econ.output[Sector::Goods.index()], dem_goods);

        let price = econ.price;
        for &i in members {
            let nd = self.people.need(i, self.planet.temp[self.people.cell[i]]);
            // What's physically obtainable (availability-rationed).
            let get_food = nd.food * avail_food.min(1.0);
            let get_water = nd.water * avail_water.min(1.0);
            let get_fuel = nd.fuel * avail_fuel.min(1.0);
            let get_goods = nd.goods * avail_goods.min(1.0);
            // Cost at emergent prices.
            let cost_survival = get_food * price[Sector::Food.index()]
                + get_water * price[Sector::Water.index()]
                + get_fuel * price[Sector::FossilFuel.index()].min(price[Sector::CleanEnergy.index()]);
            let cost_goods = get_goods * price[Sector::Goods.index()];
            let budget = self.people.wealth[i] + income[i];

            // Buy survival first, then goods; the rest is saved.
            let (survival_frac, spent_survival) = if budget >= cost_survival {
                (1.0, cost_survival)
            } else if cost_survival > 0.0 {
                (budget / cost_survival, budget)
            } else {
                (1.0, 0.0)
            };
            let remaining = (budget - spent_survival).max(0.0);
            let goods_frac = if cost_goods > 0.0 {
                (remaining / cost_goods).min(1.0)
            } else {
                1.0
            };
            let spent_goods = goods_frac * cost_goods;
            self.people.wealth[i] = (budget - spent_survival - spent_goods).max(0.0);

            // Deprivation: shortfall in the *survival* basket (availability x
            // affordability). Drives mortality and migration.
            let met_survival = survival_frac
                * ((get_food + get_water + get_fuel) / nd.survival().max(1e-9)).min(1.0);
            let short = (1.0 - met_survival).clamp(0.0, 1.0);
            if short > 0.05 {
                self.people.deprivation[i] = (self.people.deprivation[i] + short).min(3.0);
                self.deprived_this_year += 1;
            } else {
                self.people.deprivation[i] *= 0.5; // recovery when fed
            }

            // Well-being EMA: realised satisfaction across all needs.
            let total_need = nd.total().max(1e-9);
            let met = (get_food * survival_frac
                + get_water * survival_frac
                + get_fuel * survival_frac
                + get_goods * goods_frac)
                / total_need;
            let sat = met.clamp(0.0, 1.0);
            self.people.wellbeing[i] += 0.1 * (sat - self.people.wellbeing[i]);
        }
    }

    /// Mortality and fertility for the whole population (uses the RNG stream).
    fn vital_events(&mut self, members: &[Vec<usize>]) {
        // Deaths.
        for i in 0..self.people.len() {
            if !self.people.alive[i] {
                continue;
            }
            self.people.age[i] += 1;
            let t = self.planet.temp[self.people.cell[i]];
            let h = self.people.mortality_hazard(i, t);
            if self.rng.f64() < h {
                self.people.alive[i] = false;
                self.death_age_sum += self.people.age[i] as f64;
                self.death_count += 1;
            }
        }

        // Births: each fertile person decides. The realised rate is the
        // physiological ceiling discounted by (a) how well-provisioned the
        // person is — well-being is the measured signal, so a society pressed
        // against its food/resource limit slows down (the Malthusian feedback
        // that produces an emergent carrying capacity) — and (b) the person's
        // risk aversion (the precautionary/quantity–quality motive: cautious
        // people have fewer children). No wealth gate: the poor reproduce too,
        // exactly as real demographies show. A child inherits cell, polity and
        // (mutated) psychology, so population growth is an emergent decision.
        let n_polities = members.len();
        for p in 0..n_polities {
            for idx in 0..members[p].len() {
                let i = members[p][idx];
                if !self.people.alive[i] || !self.people.is_fertile(i) {
                    continue;
                }
                let provision = self.people.wellbeing[i].clamp(0.0, 1.0);
                let caution = 1.0 - 0.5 * self.people.risk_aversion[i];
                let rate = MAX_BIRTH_RATE * provision * caution;
                if self.rng.f64() < rate {
                    self.spawn_child(i);
                }
            }
        }
    }

    fn spawn_child(&mut self, parent: usize) {
        let cell = self.people.cell[parent];
        let polity = self.people.polity[parent];
        // Endowment: a quarter of the parent's savings (possibly little — birth
        // is not gated on wealth, so a child may start near-empty and rely on
        // the subsistence-sharing income channel until it can work).
        let endow = self.people.wealth[parent] * 0.25;
        self.people.wealth[parent] -= endow;
        let mu = 0.05;
        let mutate = |v: f64, rng: &mut Rng, r: Range| r.clamp(v + rng.range(-mu, mu));
        let psyche = [
            mutate(self.people.patience[parent], &mut self.rng, self.cfg.patience),
            mutate(self.people.risk_aversion[parent], &mut self.rng, self.cfg.risk_aversion),
            mutate(self.people.fairness[parent], &mut self.rng, self.cfg.fairness),
            mutate(self.people.conformity[parent], &mut self.rng, self.cfg.conformity),
        ];
        // Children inherit a fraction of parental human capital.
        let skill = (0.5 + 0.5 * self.people.skill[parent]).max(0.5);
        self.people.push(cell, polity, endow, skill, psyche, 0);
    }

    /// The deprived migrate toward better-provisioned neighbouring cells,
    /// possibly crossing into another polity if its borders are open.
    fn migration(&mut self, _members: &[Vec<usize>]) {
        for i in 0..self.people.len() {
            if !self.people.alive[i] || self.people.deprivation[i] < 1.0 {
                continue;
            }
            let from = self.people.cell[i];
            // Best neighbouring land cell by productivity x soil x water.
            let mut best = from;
            let mut best_score = self.cell_score(from);
            for nb in self.planet.neighbors(from) {
                if !self.planet.is_land[nb] {
                    continue;
                }
                let target_polity = self.polity_of_cell[nb];
                if target_polity != self.people.polity[i] {
                    // Crossing a border: gated by the *destination* openness.
                    let openness = self.society[target_polity as usize].migration_openness;
                    if self.rng.f64() > openness {
                        continue;
                    }
                }
                let s = self.cell_score(nb);
                if s > best_score {
                    best_score = s;
                    best = nb;
                }
            }
            if best != from {
                self.people.cell[i] = best;
                self.people.polity[i] = self.polity_of_cell[best];
                self.people.deprivation[i] *= 0.5; // moving relieves some stress
            }
        }
    }

    fn cell_score(&self, c: usize) -> f64 {
        self.planet.npp[c] * self.planet.soil[c] * self.planet.water[c].min(1.5)
    }

    /// Periodic referenda nudge a polity's fiscal/ecological dials toward the
    /// measured interest of the decisive voter (median under one-person-one-vote,
    /// wealth-weighted under elite capture). The *direction* emerges from who is
    /// poor, who faces scarcity, and who is exposed to warming — never scripted.
    fn governance(&mut self, members: &[Vec<usize>]) {
        for p in 0..self.society.len() {
            let mech = self.society[p].governance;
            if mech == GovernanceRegime::Fixed {
                continue;
            }
            if self.society[p].vote_period == 0 || self.year % self.society[p].vote_period as u64 != 0 {
                continue;
            }
            let mem = &members[p];
            if mem.is_empty() {
                continue;
            }
            // Decisive-voter wealth: median (majority) or wealth-weighted mean.
            let mean_wealth: f64 =
                mem.iter().map(|&i| self.people.wealth[i]).sum::<f64>() / mem.len() as f64;
            let (support_redistribution, support_conservation) = match mech {
                GovernanceRegime::Majority => {
                    let below = mem.iter().filter(|&&i| self.people.wealth[i] < mean_wealth).count();
                    let deprived =
                        mem.iter().filter(|&&i| self.people.deprivation[i] > 0.5).count();
                    (
                        below as f64 / mem.len() as f64 > 0.5,
                        deprived as f64 / mem.len() as f64 > 0.25,
                    )
                }
                GovernanceRegime::WealthWeighted => {
                    // The wealthy decide: they oppose taxes, and back conservation
                    // only if scarcity threatens their holdings.
                    let scarcity = 1.0 - self.planet_polity_health(p);
                    (false, scarcity > 0.4)
                }
                GovernanceRegime::Fixed => (false, false),
            };
            let soc = &mut self.society[p];
            if support_redistribution {
                soc.tax_rate = (soc.tax_rate + 0.02).min(0.4);
                soc.tax_progressivity = (soc.tax_progressivity + 0.1).min(1.0);
                soc.transfer = TransferRegime::Floor;
            } else {
                soc.tax_rate = (soc.tax_rate - 0.01).max(0.0);
            }
            if support_conservation {
                soc.property = PropertyRegime::CommonsQuota;
                soc.conservation_quota = (soc.conservation_quota - 0.05).max(0.2);
            }
        }
    }

    /// Mean biomass health over a polity's land (governance reads it).
    fn planet_polity_health(&self, p: usize) -> f64 {
        let cells = &self.polity_cells[p];
        if cells.is_empty() {
            return 1.0;
        }
        let mut k = 0.0;
        let mut k0 = 0.0;
        for &c in cells {
            k += self.planet.biomass[c];
            k0 += self.planet.biomass_k0[c];
        }
        if k0 <= 0.0 {
            1.0
        } else {
            (k / k0).clamp(0.0, 1.0)
        }
    }

    fn draw_down(field: &mut [f64], cells: &[usize], amount: f64) {
        if amount <= 0.0 {
            return;
        }
        let total: f64 = cells.iter().map(|&c| field[c]).sum();
        if total <= 0.0 {
            return;
        }
        let take = amount.min(total);
        for &c in cells {
            field[c] -= take * field[c] / total;
            if field[c] < 0.0 {
                field[c] = 0.0;
            }
        }
    }

    /// **Read every macro quantity** off the current state — all emergent.
    pub fn measure(&self) -> Measurements {
        let pop = self.people.alive_count();
        let mut wealths = Vec::with_capacity(pop);
        let mut wealth_sum = 0.0;
        let mut skill_sum = 0.0;
        let mut wb_sum = 0.0;
        for i in 0..self.people.len() {
            if self.people.alive[i] {
                wealths.push(self.people.wealth[i]);
                wealth_sum += self.people.wealth[i];
                skill_sum += self.people.skill[i];
                wb_sum += self.people.wellbeing[i];
            }
        }
        let gdp: f64 = self.econ.iter().map(|e| e.gdp()).sum();
        let (clean, energy): (f64, f64) = self.econ.iter().fold((0.0, 0.0), |(c, e), ec| {
            let cl = ec.output[Sector::CleanEnergy.index()];
            let fo = ec.output[Sector::FossilFuel.index()];
            (c + cl, e + cl + fo)
        });
        let fossil_now: f64 = self.planet.fossil.iter().sum();

        Measurements {
            year: self.year,
            population: pop,
            gdp,
            gdp_per_capita: if pop > 0 { gdp / pop as f64 } else { 0.0 },
            mean_wealth: if pop > 0 { wealth_sum / pop as f64 } else { 0.0 },
            wealth_gini: gini(&wealths),
            life_expectancy: if self.death_count > 0 {
                self.death_age_sum / self.death_count as f64
            } else {
                f64::NAN
            },
            wellbeing: if pop > 0 { wb_sum / pop as f64 } else { 0.5 },
            deprivation_rate: if pop > 0 { self.deprived_this_year as f64 / pop as f64 } else { 0.0 },
            mean_skill: if pop > 0 { skill_sum / pop as f64 } else { 0.0 },
            temp_anomaly: self.planet.t_anomaly,
            co2: self.planet.co2,
            clean_share: if energy > 0.0 { clean / energy } else { 0.0 },
            commons_health: self.planet.commons_health(),
            biodiversity: self.planet.mean_biodiversity(),
            fossil_remaining: if self.fossil0 > 0.0 { (fossil_now / self.fossil0).clamp(0.0, 1.0) } else { 0.0 },
        }
    }

    /// Convenience: the measured long-run welfare of this world right now.
    pub fn welfare(&self) -> f64 {
        self.measure().welfare(self.initial_population)
    }
}

fn ratio(supply: f64, demand: f64) -> f64 {
    if demand <= 1e-9 {
        1.0
    } else {
        supply / demand
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_cfg() -> WorldConfig {
        let mut c = WorldConfig::default();
        c.nlon = 36;
        c.nlat = 18;
        c.n_agents = 1500;
        c.n_polities = 4;
        c
    }

    #[test]
    fn a_world_runs_and_sustains_life() {
        let mut w = World::new(&small_cfg());
        assert!(w.people.alive_count() > 500, "should seed a population");
        for _ in 0..120 {
            w.step();
        }
        let m = w.measure();
        assert!(m.population > 0, "society should not go extinct in the baseline");
        assert!(m.gdp > 0.0, "an economy should be producing");
        assert!(m.life_expectancy.is_finite() && m.life_expectancy > 0.0);
    }

    #[test]
    fn inequality_emerges_from_an_equal_start() {
        let mut w = World::new(&small_cfg());
        assert!(w.measure().wealth_gini.abs() < 1e-9, "equal-wealth start");
        for _ in 0..150 {
            w.step();
        }
        assert!(
            w.measure().wealth_gini > 0.05,
            "inequality should emerge, got {}",
            w.measure().wealth_gini
        );
    }

    #[test]
    fn determinism_same_seed_same_history() {
        let cfg = small_cfg();
        let mut a = World::new(&cfg);
        let mut b = World::new(&cfg);
        for _ in 0..80 {
            a.step();
            b.step();
        }
        let (ma, mb) = (a.measure(), b.measure());
        assert_eq!(ma.population, mb.population);
        assert_eq!(ma.gdp.to_bits(), mb.gdp.to_bits());
        assert_eq!(ma.co2.to_bits(), mb.co2.to_bits());
        assert_eq!(ma.wealth_gini.to_bits(), mb.wealth_gini.to_bits());
    }

    /// A carbon price shifts the *emergent* energy mix toward clean and lowers
    /// CO₂ and warming — same planet and seed, only the policy differs.
    #[test]
    fn a_carbon_price_decarbonises_and_cools() {
        let cfg = small_cfg();
        let dirty = {
            let mut w = World::new(&cfg);
            for _ in 0..150 {
                w.step();
            }
            w.measure()
        };
        let priced = {
            let mut s = SocietyParams::default();
            s.carbon_price = 5.0;
            s.research_share = 0.2;
            let sc = Scenario::new("priced", cfg.clone()).with_uniform_society(s);
            let mut w = World::from_scenario(&sc);
            for _ in 0..150 {
                w.step();
            }
            w.measure()
        };
        assert!(
            priced.clean_share > dirty.clean_share,
            "a carbon price should raise the clean-energy share: {} vs {}",
            priced.clean_share,
            dirty.clean_share
        );
        assert!(
            priced.co2 < dirty.co2,
            "a carbon price should lower emergent CO2: {} vs {}",
            priced.co2,
            dirty.co2
        );
    }

    /// Redistribution (a progressive tax funding a floor) lowers the measured
    /// Gini relative to the laissez-faire baseline on the same planet.
    #[test]
    fn redistribution_lowers_measured_inequality() {
        let cfg = small_cfg();
        let base = {
            let mut w = World::new(&cfg);
            for _ in 0..150 {
                w.step();
            }
            w.measure().wealth_gini
        };
        let redist = {
            let mut s = SocietyParams::default();
            s.tax_rate = 0.25;
            s.tax_progressivity = 1.0;
            s.transfer = TransferRegime::Floor;
            let sc = Scenario::new("redist", cfg.clone()).with_uniform_society(s);
            let mut w = World::from_scenario(&sc);
            for _ in 0..150 {
                w.step();
            }
            w.measure().wealth_gini
        };
        assert!(
            redist < base,
            "redistribution should lower the measured Gini: {redist} vs {base}"
        );
    }

    /// Education spending raises emergent human capital and output per capita.
    #[test]
    fn public_education_raises_human_capital() {
        let cfg = small_cfg();
        let schooled = {
            let mut s = SocietyParams::default();
            s.tax_rate = 0.2;
            s.education_share = 0.6;
            let sc = Scenario::new("schooled", cfg.clone()).with_uniform_society(s);
            let mut w = World::from_scenario(&sc);
            for _ in 0..150 {
                w.step();
            }
            w.measure()
        };
        let base = {
            let mut w = World::new(&cfg);
            for _ in 0..150 {
                w.step();
            }
            w.measure()
        };
        assert!(
            schooled.mean_skill > base.mean_skill,
            "schooling should raise mean human capital: {} vs {}",
            schooled.mean_skill,
            base.mean_skill
        );
    }
}
