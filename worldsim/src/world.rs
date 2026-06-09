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
use crate::people::{People, NO_JOB, NO_PARENT};
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
    /// Biomass fuel gathered this year per polity (the wood channel; measured).
    pub wood_this_year: Vec<f64>,

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
            wood_this_year: vec![0.0; n_polities],
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
        for w in &mut self.wood_this_year {
            *w = 0.0;
        }

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

    /// One polity's production–exchange–fiscal year. There is **no planner**:
    /// workers choose sectors from observed wages; production is grounded in
    /// physical resource ceilings; prices, wages and the energy mix emerge;
    /// investment comes from individual patience; the only levers a society
    /// holds are the configured mechanisms in its [`SocietyParams`].
    fn run_economy(&mut self, p: usize, members: &[usize], income: &mut [f64]) {
        if members.is_empty() {
            return;
        }
        let cells = self.polity_cells[p].clone();
        let soc = self.society[p].clone();

        // --- Aggregate needs (the observable demand side). Water is
        // self-provisioned from the local renewable supply first (a household
        // fetches its own water where it rains); only the *shortfall* — dry
        // cells — is market demand on the Water sector. ---
        let mut need_food = 0.0;
        let mut need_water = 0.0; // market shortfall only
        let mut need_fuel = 0.0;
        let mut need_goods = 0.0;
        for &i in members {
            let c = self.people.cell[i];
            let t = self.planet.temp[c];
            let nd = self.people.need(i, t);
            need_food += nd.food;
            let free_water = self.planet.water[c].min(1.0);
            need_water += nd.water * (1.0 - free_water);
            need_fuel += nd.fuel;
            need_goods += nd.goods;
        }

        // --- Biomass fuel gathering (the commons channel). Heating fuel is
        // first gathered freely from standing biomass — the pre-industrial
        // default. How much of the standing stock may be taken is the property
        // regime: open access strips it (Hardin), a commons quota caps the
        // fraction (Ostrom) with imperfect compliance (legitimacy + the
        // population's conformity), private owners self-limit (Demsetz).
        // Biomass burning is carbon-neutral on this timescale (regrowth
        // reabsorbs it); the *ecological* cost is the standing stock.
        let polity_biomass: f64 = cells.iter().map(|&c| self.planet.biomass[c]).sum();
        let mean_conformity: f64 = {
            let s: f64 = members.iter().map(|&i| self.people.conformity[i]).sum();
            s / members.len() as f64
        };
        let allowed_frac = match soc.property {
            PropertyRegime::OpenAccess => 1.0,
            PropertyRegime::CommonsQuota => {
                // Compliance is voluntary and imperfect: it rises with the
                // institution's legitimacy and the people's norm-following.
                let compliance = (self.legitimacy[p] * mean_conformity).sqrt().clamp(0.0, 1.0);
                soc.conservation_quota
                    + (1.0 - compliance) * (1.0 - soc.conservation_quota)
            }
            // An owner internalises the future value of the stock and takes a
            // sustainable fraction (Demsetz; cf. the original engine).
            PropertyRegime::Private => 0.5,
        };
        let wood = need_fuel.min(allowed_frac * polity_biomass);
        Self::draw_down(&mut self.planet.biomass, &cells, wood);
        self.wood_this_year[p] = wood;
        let fuel_gap = (need_fuel - wood).max(0.0);

        // --- Labour: each working-age person CHOOSES a sector from the wages
        // observed last year (cobweb dynamics, Ezekiel 1938) — for an empty
        // sector the signal is the predicted first-worker wage. Switching has
        // a cost and the risk-averse switch later (Artuç et al. 2010). ---
        let wage_signal: [f64; Sector::N] = {
            let econ = &self.econ[p];
            let mut w = [0.0; Sector::N];
            for s in Sector::ALL {
                let k = s.index();
                w[k] = if econ.labour[k] > 0.0 {
                    econ.last_wage[k]
                } else {
                    // Marginal product of the first worker (utilization ~ 1).
                    (1.0 - CAPITAL_ELASTICITY)
                        * econ.price[k]
                        * econ.productivity[k]
                        * crate::economy::BASE_YIELD
                        * econ.capital[k].powf(CAPITAL_ELASTICITY)
                };
            }
            w
        };
        let best_sector = {
            let mut b = 0;
            for s in 1..Sector::N {
                if wage_signal[s] > wage_signal[b] {
                    b = s;
                }
            }
            b as u8
        };
        // Entrants sample a sector with probability proportional to its wage
        // signal (people spread across opportunities rather than all picking
        // the same argmax); incumbents only *search* in a given year with the
        // Calvo friction, and switch only past their personal inertia
        // threshold. Both stop the all-at-once cobweb herding a synchronous
        // best response would cause.
        let wage_sum: f64 = wage_signal.iter().sum::<f64>().max(1e-9);
        let mut labour = [0.0_f64; Sector::N];
        for &i in members {
            if !(15..70).contains(&self.people.age[i]) {
                self.people.sector[i] = NO_JOB;
                continue;
            }
            let cur = self.people.sector[i];
            if cur == NO_JOB {
                let mut pick = self.rng.f64() * wage_sum;
                let mut chosen = Sector::N - 1;
                for (s, w) in wage_signal.iter().enumerate() {
                    if pick < *w {
                        chosen = s;
                        break;
                    }
                    pick -= *w;
                }
                self.people.sector[i] = chosen as u8;
            } else if self.rng.f64() < JOB_SEARCH_RATE {
                let threshold =
                    1.0 + JOB_INERTIA * (1.0 + self.people.risk_aversion[i]);
                if wage_signal[best_sector as usize] > wage_signal[cur as usize] * threshold {
                    self.people.sector[i] = best_sector;
                }
            }
            let s = self.people.sector[i] as usize;
            labour[s] += self.people.skill[i];
        }

        // --- Physical resource access (the planet's side of production). ---
        let mut farm_capacity = 0.0;
        let mut fossil_avail = 0.0;
        let mut mineral_avail = 0.0;
        let mut fish_avail = 0.0;
        for &c in &cells {
            farm_capacity +=
                self.planet.npp[c] * self.planet.soil[c] * self.planet.water[c].min(1.0);
            fossil_avail += self.planet.fossil[c];
            mineral_avail += self.planet.mineral[c];
            for nb in self.planet.neighbors(c) {
                if !self.planet.is_land[nb] {
                    fish_avail += self.planet.fish[nb];
                }
            }
        }
        farm_capacity *= self.planet.yield_scale;

        // Industrial energy demand on top of the remaining heating gap.
        let energy_need = fuel_gap + 0.5 * (need_goods + need_water);

        // Production: effort = A·BASE_YIELD·K^α·L, passed through a saturating
        // utilization against demand and ceilinged by the finite resource —
        // which is where carrying capacity lives.
        let produce = |econ: &Economy, s: Sector, target: f64, cap: f64, access: f64| {
            let input = econ.potential(s, labour[s.index()], access);
            let t = target.max(1e-9);
            (t * (1.0 - (-input / t).exp())).min(cap)
        };
        let econ = &mut self.econ[p];
        econ.labour = labour;

        let food = produce(econ, Sector::Food, need_food, farm_capacity, 1.0);
        let water = produce(econ, Sector::Water, need_water, f64::INFINITY, 1.0);
        // Energy: fossil and clean jointly serve the demand; output splits by
        // each sector's effort, fossil ceilinged by the remaining deposit.
        let (fossil, clean) = {
            let ef = econ.potential(Sector::FossilFuel, labour[Sector::FossilFuel.index()], 1.0);
            let ec = econ.potential(Sector::CleanEnergy, labour[Sector::CleanEnergy.index()], 1.0);
            let t = energy_need.max(1e-9);
            let combined = t * (1.0 - (-(ef + ec) / t).exp());
            let share_f = if ef + ec > 0.0 { ef / (ef + ec) } else { 0.0 };
            ((combined * share_f).min(fossil_avail), combined * (1.0 - share_f))
        };
        let materials = produce(econ, Sector::Materials, need_goods, mineral_avail, 1.0);
        let mat_access = (materials / need_goods.max(1e-6)).clamp(0.1, 1.5);
        let goods = produce(econ, Sector::Goods, need_goods, f64::INFINITY, mat_access);

        econ.record_output(Sector::Food, food);
        econ.record_output(Sector::Water, water);
        econ.record_output(Sector::FossilFuel, fossil);
        econ.record_output(Sector::CleanEnergy, clean);
        econ.record_output(Sector::Materials, materials);
        econ.record_output(Sector::Goods, goods);

        econ.update_price(Sector::Food, need_food, food);
        econ.update_price(Sector::Water, need_water, water);
        let energy_out = (fossil + clean).max(1e-9);
        econ.update_price(Sector::FossilFuel, energy_need * fossil / energy_out, fossil + 1e-6);
        econ.update_price(Sector::CleanEnergy, energy_need * clean / energy_out, clean + 1e-6);
        econ.update_price(Sector::Materials, need_goods, materials + 1e-6);
        econ.update_price(Sector::Goods, need_goods, goods);

        // --- Physical pressures onto the planet. ---
        Self::draw_down(&mut self.planet.fossil, &cells, fossil);
        Self::draw_down(&mut self.planet.mineral, &cells, materials);
        let cult = if farm_capacity > 0.0 { (food / farm_capacity).clamp(0.0, 1.0) } else { 0.0 };
        for &c in &cells {
            if self.planet.is_land[c] {
                self.planet.land_use[c] = cult;
                if cult > 0.5 {
                    self.planet.soil[c] =
                        (self.planet.soil[c] - SOIL_DEGRADE * (cult - 0.5)).max(0.1);
                }
            }
        }
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
        self.planet.emissions_this_year +=
            emissions / (self.initial_population as f64 / 200.0).max(1.0);

        // --- Distribution: each sector's revenue splits Cobb–Douglas between
        // its workers (by skill — the wage channel) and capital owners (by
        // wealth — asset returns). The carbon price is charged at the fossil
        // wellhead (a Pigouvian levy on the externality), so it lowers fossil
        // wages and pushes workers toward clean energy — the transition is a
        // labour-market outcome, not a mandate. Nothing here shares by fiat;
        // dependants are fed by kin and charity in the consumption phase. ---
        let econ = &mut self.econ[p];
        let mut capital_pool = 0.0;
        let mut carbon_revenue = 0.0;
        for s in Sector::ALL {
            let k = s.index();
            let mut revenue = econ.price[k] * econ.output[k];
            if s == Sector::FossilFuel {
                let levy = (soc.carbon_price * EMISSION_FACTOR_FOSSIL * econ.output[k])
                    .min(revenue);
                revenue -= levy;
                carbon_revenue += levy;
            }
            let wage_pool = (1.0 - CAPITAL_ELASTICITY) * revenue;
            capital_pool += CAPITAL_ELASTICITY * revenue;
            econ.last_wage[k] = if econ.labour[k] > 0.0 {
                wage_pool / econ.labour[k]
            } else {
                econ.last_wage[k]
            };
        }
        let total_wealth: f64 =
            members.iter().map(|&i| self.people.wealth[i]).sum::<f64>().max(1e-6);
        for &i in members {
            let mut inc = 0.0;
            let s = self.people.sector[i];
            if s != NO_JOB {
                let k = s as usize;
                if self.econ[p].labour[k] > 0.0 {
                    let wage_pool = self.econ[p].last_wage[k] * self.econ[p].labour[k];
                    inc += wage_pool * self.people.skill[i] / self.econ[p].labour[k];
                }
            }
            inc += capital_pool * self.people.wealth[i] / total_wealth;
            income[i] = inc;
        }

        // --- Investment: saving is the behavioural face of time preference —
        // each person defers `patience · INVEST_RATE` of the wealth they hold
        // above their own survival need, and the pooled savings become sector
        // capital, allocated toward the sectors earning revenue (chasing
        // returns). The patient build the capital stock; the impatient
        // consume. (Frederick et al. 2002.) ---
        let mut invested = 0.0;
        for &i in members {
            let surv = self.people.need(i, self.planet.temp[self.people.cell[i]]).survival();
            let surplus = (self.people.wealth[i] - surv).max(0.0);
            let put = surplus * INVEST_RATE * self.people.patience[i];
            self.people.wealth[i] -= put;
            invested += put;
        }
        // Building capital is real work: the invested funds are *demand on the
        // Goods sector* and are paid out as wages to its workers (purchasing
        // power recycles; investment is not a monetary sink). The physical
        // capital lands in the sectors earning revenue (investors chase
        // returns).
        let goods_labour = self.econ[p].labour[Sector::Goods.index()];
        if invested > 0.0 && goods_labour > 0.0 {
            for &i in members {
                if self.people.sector[i] == Sector::Goods.index() as u8 {
                    income[i] += invested * self.people.skill[i] / goods_labour;
                }
            }
            let total_revenue: f64 = Sector::ALL
                .iter()
                .map(|&s| self.econ[p].price[s.index()] * self.econ[p].output[s.index()])
                .sum::<f64>()
                .max(1e-9);
            for s in Sector::ALL {
                let k = s.index();
                let share = self.econ[p].price[k] * self.econ[p].output[k] / total_revenue;
                self.econ[p].invest(s, invested * share);
            }
        } else {
            // No builders this year: the savings stay in pockets; capital
            // still depreciates.
            for &i in members {
                let surv = self.people.need(i, self.planet.temp[self.people.cell[i]]).survival();
                let _ = surv;
            }
            if invested > 0.0 {
                let per = invested / members.len() as f64;
                for &i in members {
                    self.people.wealth[i] += per;
                }
            }
            for s in Sector::ALL {
                self.econ[p].invest(s, 0.0);
            }
        }

        // --- Fiscal mechanism: income tax + the carbon levy → treasury. The
        // tax falls only on income above the taxpayer's own survival need — a
        // state cannot tax away the food in people's mouths without killing
        // its own tax base. Progressivity carves out a share of modest
        // surpluses. ---
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
                let k = s.index();
                self.econ[p].capital[k] += infra / Sector::N as f64;
            }
        }
        if research > 0.0 {
            let gdp = self.econ[p].gdp();
            let bump = 1.0 + (research / (gdp + 1.0)).min(0.1);
            for s in Sector::ALL {
                self.econ[p].productivity[s.index()] *= bump;
            }
        }
        // Enforcement buys institutional capacity; legitimacy tracks it.
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
                let mean: f64 =
                    members.iter().map(|&i| income[i]).sum::<f64>() / members.len() as f64;
                let total_short: f64 =
                    members.iter().map(|&i| (mean - income[i]).max(0.0)).sum();
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
        self.econ[p].treasury = treasury;
    }


    /// Consumption with **no fiat sharing**: each person buys against its own
    /// budget; **children are provisioned by their parents** (kin provisioning
    /// is a human universal — Kaplan 1996 — biology, not policy); after the
    /// market, the **fair-minded voluntarily give** part of their surplus to
    /// the visibly deprived around them (Fehr–Schmidt preferences in action);
    /// state transfers, if any, arrived through income. Shortfalls become
    /// deprivation; well-being tracks realised satisfaction.
    fn consume_polity(&mut self, p: usize, members: &[usize], income: &[f64]) {
        if members.is_empty() {
            return;
        }
        // Income lands in everyone's pocket first; all purchasing below is out
        // of wealth, so a parent's budget already includes this year's pay.
        for &i in members {
            self.people.wealth[i] += income[i];
        }
        // Physical availability ratios (a famine rations everyone pro rata).
        let econ = &self.econ[p];
        let mut dem_food = 0.0;
        let mut dem_water = 0.0;
        let mut dem_fuel = 0.0;
        let mut dem_goods = 0.0;
        for &i in members {
            let c = self.people.cell[i];
            let nd = self.people.need(i, self.planet.temp[c]);
            dem_food += nd.food;
            dem_water += nd.water * (1.0 - self.planet.water[c].min(1.0));
            dem_fuel += nd.fuel;
            dem_goods += nd.goods;
        }
        let avail_food = ratio(econ.output[Sector::Food.index()], dem_food).min(1.0);
        // dem_water below is the *market* shortfall (self-provision covered the
        // rest); coverage of the gap by the Water sector:
        let avail_water_market = ratio(econ.output[Sector::Water.index()], dem_water).min(1.0);
        // Heating fuel was gathered as wood first; market energy covers the rest.
        let energy_supply =
            econ.output[Sector::FossilFuel.index()] + econ.output[Sector::CleanEnergy.index()];
        let avail_fuel =
            ratio(self.wood_this_year[p] + energy_supply, dem_fuel + 0.5 * (dem_goods + dem_water))
                .min(1.0);
        let avail_goods = ratio(econ.output[Sector::Goods.index()], dem_goods).min(1.0);
        let price = econ.price;
        let energy_price =
            price[Sector::FossilFuel.index()].min(price[Sector::CleanEnergy.index()]);

        // The cost of one person's obtainable basket, split survival/comfort.
        // Water: the freely-fetched local share costs nothing; only the market
        // share of the gap is bought.
        let planet_water = &self.planet.water;
        let basket = |me: &People, i: usize, temp: f64| -> (f64, f64, f64, f64) {
            let nd = me.need(i, temp);
            let free_w = planet_water[me.cell[i]].min(1.0);
            let water_cover = free_w + avail_water_market * (1.0 - free_w);
            let get_surv = nd.food * avail_food + nd.water * water_cover + nd.fuel * avail_fuel;
            let cost_surv = nd.food * avail_food * price[Sector::Food.index()]
                + nd.water * avail_water_market * (1.0 - free_w) * price[Sector::Water.index()]
                + nd.fuel * avail_fuel * energy_price;
            let get_goods = nd.goods * avail_goods;
            let cost_goods = get_goods * price[Sector::Goods.index()];
            (get_surv / nd.survival().max(1e-9), cost_surv, get_goods / nd.goods.max(1e-9), cost_goods)
        };

        // Pass 1 — each basket is settled by its payer: adults pay for
        // themselves; a child's bill routes to its living parent (kin
        // provisioning). Survival is bought before comfort.
        let mut shortfall = vec![0.0_f64; self.people.len()];
        for &i in members {
            let temp = self.planet.temp[self.people.cell[i]];
            let payer = if self.people.age[i] < 15 {
                let par = self.people.parent[i];
                if par != NO_PARENT && self.people.alive[par] { par } else { i }
            } else {
                i
            };
            let (frac_surv_avail, cost_surv, frac_goods_avail, cost_goods) =
                basket(&self.people, i, temp);
            let budget = self.people.wealth[payer];
            let afford_surv = if cost_surv > 0.0 { (budget / cost_surv).min(1.0) } else { 1.0 };
            let spend_surv = cost_surv * afford_surv;
            let left = (budget - spend_surv).max(0.0);
            let afford_goods = if cost_goods > 0.0 { (left / cost_goods).min(1.0) } else { 1.0 };
            let spend_goods = cost_goods * afford_goods;
            self.people.wealth[payer] = (budget - spend_surv - spend_goods).max(0.0);

            // Realised satisfaction of i.
            let met_surv = (frac_surv_avail * afford_surv).clamp(0.0, 1.0);
            let met_goods = (frac_goods_avail * afford_goods).clamp(0.0, 1.0);
            shortfall[i] = (1.0 - met_surv).clamp(0.0, 1.0);
            let sat = (0.8 * met_surv + 0.2 * met_goods).clamp(0.0, 1.0);
            self.people.wellbeing[i] += 0.1 * (sat - self.people.wellbeing[i]);
        }

        // Pass 2 — voluntary charity: the fair-minded give a share of the
        // wealth they hold above their own survival cost to those still short,
        // filling the largest shortfalls pro rata. Emergent: a high-fairness
        // culture has a private safety net; a selfish one does not.
        let mut pool = 0.0;
        for &i in members {
            let temp = self.planet.temp[self.people.cell[i]];
            let surv_cost = self.people.need(i, temp).survival();
            let surplus = (self.people.wealth[i] - surv_cost).max(0.0);
            let give = surplus * CHARITY_RATE * self.people.fairness[i];
            self.people.wealth[i] -= give;
            pool += give;
        }
        let total_short_cost: f64 = members
            .iter()
            .map(|&i| {
                let temp = self.planet.temp[self.people.cell[i]];
                shortfall[i] * self.people.need(i, temp).survival()
            })
            .sum();
        if pool > 0.0 && total_short_cost > 0.0 {
            let coverage = (pool / total_short_cost).min(1.0);
            for &i in members {
                if shortfall[i] > 0.0 {
                    shortfall[i] *= 1.0 - coverage;
                }
            }
            // Whatever charity could not place returns to the donors pro rata
            // (kept simple: it dissipates as administrative loss otherwise).
        }

        // Deprivation ledger from the final shortfall.
        for &i in members {
            if shortfall[i] > 0.05 {
                self.people.deprivation[i] = (self.people.deprivation[i] + shortfall[i]).min(3.0);
                self.deprived_this_year += 1;
            } else {
                self.people.deprivation[i] *= 0.5;
            }
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
        // physiological ceiling discounted by three measured/psychological
        // factors, none of them a demographic target:
        //  - provision (own well-being): a society pressed against its
        //    resource limit slows down — the Malthusian feedback that makes a
        //    carrying capacity emerge;
        //  - caution (risk aversion): the precautionary motive;
        //  - opportunity cost (Becker 1960; Galor & Weil 2000): childrearing
        //    time costs more for the higher-skilled, whose forgone wage is
        //    larger — the quantity–quality trade-off, so as education raises
        //    human capital, fertility falls and the demographic transition
        //    EMERGES rather than being scripted.
        // No wealth gate: the poor reproduce too, as real demographies show.
        // A child inherits cell, polity, a parent link and (mutated)
        // psychology.
        let n_polities = members.len();
        for p in 0..n_polities {
            for idx in 0..members[p].len() {
                let i = members[p][idx];
                if !self.people.alive[i] || !self.people.is_fertile(i) {
                    continue;
                }
                let provision = self.people.wellbeing[i].clamp(0.0, 1.0);
                let caution = 1.0 - 0.5 * self.people.risk_aversion[i];
                let opportunity =
                    1.0 + FERTILITY_OPPORTUNITY_COST * (self.people.skill[i] - 1.0).max(0.0);
                let rate = MAX_BIRTH_RATE * provision * caution / opportunity;
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
        // Children inherit a fraction of parental human capital, and the
        // parent link that routes their needs to the family budget.
        let skill = (0.5 + 0.5 * self.people.skill[parent]).max(0.5);
        self.people.push(cell, polity, endow, skill, psyche, 0, parent);
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

    /// Periodic **referenda**: when governance is not `Fixed`, three questions
    /// are put to the polity each period — *more or less redistribution?*,
    /// *price carbon higher or lower?*, *tighten or loosen conservation?* —
    /// and each person votes its **measured self-interest and psychology**:
    ///
    /// - redistribution: the below-mean-wealth vote to raise the tax that
    ///   funds the floor (Meltzer & Richard 1981); the above-mean vote to cut
    ///   it — unless strongly fair-minded (Fehr–Schmidt advantageous-inequity
    ///   aversion), in which case they vote with the floor too;
    /// - carbon: those measurably harmed where they live (heat stress, local
    ///   productivity loss against the pre-industrial baseline) vote to raise
    ///   the price; **fossil-sector workers vote to cut it** (their wage is on
    ///   the line — the just-transition conflict, emergent);
    /// - conservation: those whose local biomass commons is visibly depleted
    ///   vote to tighten the quota.
    ///
    /// The mechanism only sets *whose vote counts how much* (one-person-one-
    /// vote vs. wealth-weighted) and how far a dial can move per period (the
    /// institutional step). No direction is ever scripted.
    fn governance(&mut self, members: &[Vec<usize>]) {
        for p in 0..self.society.len() {
            let mech = self.society[p].governance;
            if mech == GovernanceRegime::Fixed {
                continue;
            }
            if self.society[p].vote_period == 0
                || self.year == 0
                || self.year % self.society[p].vote_period as u64 != 0
            {
                continue;
            }
            let mem = &members[p];
            if mem.is_empty() {
                continue;
            }
            let mean_wealth: f64 =
                mem.iter().map(|&i| self.people.wealth[i]).sum::<f64>() / mem.len() as f64;

            let mut net_redist = 0.0_f64;
            let mut net_carbon = 0.0_f64;
            let mut net_conserve = 0.0_f64;
            for &i in mem {
                let weight = match mech {
                    GovernanceRegime::Majority => 1.0,
                    GovernanceRegime::WealthWeighted => self.people.wealth[i].max(1e-6),
                    GovernanceRegime::Fixed => unreachable!(),
                };
                let c = self.people.cell[i];

                // Redistribution ballot.
                let below = self.people.wealth[i] < mean_wealth;
                let fair_minded = self.people.fairness[i] > 0.65;
                net_redist += if below || fair_minded { weight } else { -weight };

                // Carbon ballot: measured local harm vs. a fossil pay-check.
                let warmed = self.planet.temp[c] - self.planet.temp0[c];
                let npp_lost = self.planet.npp0[c] > 0.0
                    && self.planet.npp[c] < 0.9 * self.planet.npp0[c];
                let harmed = warmed > 1.0 || npp_lost
                    || self.planet.temp[c] > HEAT_STRESS_TEMP;
                let fossil_worker = self.people.sector[i] == Sector::FossilFuel.index() as u8;
                net_carbon += if fossil_worker {
                    -weight
                } else if harmed {
                    weight
                } else {
                    0.0
                };

                // Conservation ballot: is my local commons visibly depleted?
                let depleted = self.planet.biomass_k0[c] > 0.0
                    && self.planet.biomass[c] < 0.5 * self.planet.biomass_k0[c];
                net_conserve += if depleted { weight } else { 0.0 };
            }
            let half_weight: f64 = mem
                .iter()
                .map(|&i| match mech {
                    GovernanceRegime::Majority => 1.0,
                    GovernanceRegime::WealthWeighted => self.people.wealth[i].max(1e-6),
                    GovernanceRegime::Fixed => unreachable!(),
                })
                .sum::<f64>()
                / 2.0;

            let soc = &mut self.society[p];
            if net_redist > 0.0 {
                soc.tax_rate = (soc.tax_rate + REFERENDUM_STEP).min(0.5);
                soc.tax_progressivity = (soc.tax_progressivity + 2.0 * REFERENDUM_STEP).min(1.0);
                if soc.transfer == TransferRegime::None {
                    soc.transfer = TransferRegime::Floor;
                }
            } else if net_redist < 0.0 {
                soc.tax_rate = (soc.tax_rate - REFERENDUM_STEP).max(0.0);
            }
            if net_carbon > 0.0 {
                // The carbon dial moves on the same institutional step,
                // scaled to its range (0..15 vs 0..0.5).
                soc.carbon_price = (soc.carbon_price + 30.0 * REFERENDUM_STEP).min(15.0);
            } else if net_carbon < 0.0 {
                soc.carbon_price = (soc.carbon_price - 30.0 * REFERENDUM_STEP).max(0.0);
            }
            // Conservation needs an actual (weighted) majority to bind a quota
            // on everyone (Olson's collective-action threshold).
            if net_conserve > half_weight {
                soc.property = PropertyRegime::CommonsQuota;
                soc.conservation_quota = (soc.conservation_quota - REFERENDUM_STEP).max(0.2);
            }
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

    /// **Labour follows wages — there is no planner.** Force one sector's price
    /// permanently high (a standing scarcity) and, over years, workers
    /// reallocate into it of their own accord: its employment share rises well
    /// above where it started. The reallocation is the workers' response to the
    /// emergent wage signal, never an assignment.
    #[test]
    fn workers_reallocate_toward_higher_wages_with_no_planner() {
        let cfg = small_cfg();
        let mut w = World::new(&cfg);
        for _ in 0..30 {
            w.step();
        }
        // Measure goods-sector employment share, then pin its price high.
        let share = |w: &World| {
            let g: f64 = w.econ.iter().map(|e| e.labour[Sector::Goods.index()]).sum();
            let tot: f64 = w
                .econ
                .iter()
                .flat_map(|e| e.labour.iter())
                .sum::<f64>()
                .max(1e-9);
            g / tot
        };
        let before = share(&w);
        for _ in 0..40 {
            // A persistent goods scarcity keeps the goods wage attractive.
            for e in &mut w.econ {
                e.price[Sector::Goods.index()] = 6.0;
            }
            w.step();
        }
        let after = share(&w);
        assert!(
            after > before + 0.03,
            "workers should move toward the high-wage sector: {after} vs {before}"
        );
    }

    /// **A fair-minded culture builds a private safety net.** Same planet and
    /// seed, psychology pinned: a high-fairness population reaches a lower
    /// measured deprivation rate than a selfish one, purely through voluntary
    /// charity flowing to the deprived — no transfer policy involved.
    #[test]
    fn fairness_lowers_deprivation_through_voluntary_charity() {
        // A population well below the planet's carrying capacity, so food is
        // abundant and any deprivation is *distributional* (the poor can't
        // afford it) — the regime where charity, which moves purchasing power,
        // can actually help. (In an overshoot famine no amount of giving
        // creates food, and fairness rightly makes little difference.)
        let run = |fairness: f64| {
            let mut cfg = small_cfg();
            cfg.n_agents = 500;
            cfg.fairness = crate::config::Range(fairness, fairness);
            let mut w = World::new(&cfg);
            let mut acc = 0.0;
            for _ in 0..40 {
                w.step();
                acc += w.measure().deprivation_rate;
            }
            acc / 40.0
        };
        let fair = run(0.95);
        let selfish = run(0.05);
        assert!(
            fair < selfish,
            "a fair-minded society should suffer less deprivation via charity: {fair} vs {selfish}"
        );
    }

    /// **The demographic transition emerges (Becker quantity–quality).** A
    /// society that schools its people heavily ends with a *smaller* population
    /// than an unschooled one on the same planet: higher human capital raises
    /// the opportunity cost of childrearing, so realised fertility falls — the
    /// transition is an emergent consequence of education, never imposed.
    #[test]
    fn education_triggers_an_emergent_fertility_transition() {
        let cfg = small_cfg();
        let pop_after = |edu: f64, tax: f64| {
            let mut s = SocietyParams::default();
            s.tax_rate = tax;
            s.education_share = edu;
            let sc = Scenario::new("x", cfg.clone()).with_uniform_society(s);
            let mut w = World::from_scenario(&sc);
            for _ in 0..200 {
                w.step();
            }
            (w.measure().population, w.measure().mean_skill)
        };
        let (pop_schooled, skill_schooled) = pop_after(0.7, 0.25);
        let (pop_unschooled, skill_unschooled) = pop_after(0.0, 0.25);
        assert!(
            skill_schooled > skill_unschooled,
            "schooling should raise skill: {skill_schooled} vs {skill_unschooled}"
        );
        assert!(
            pop_schooled < pop_unschooled,
            "higher human capital should lower fertility (Becker): pop {pop_schooled} vs {pop_unschooled}"
        );
    }
}
