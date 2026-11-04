//! A GlassBox-style simulation driven by a [`Ruleset`].
//!
//! Each tick:
//! 1. **Production**: every unit produces its resources, scaled by how well
//!    its needs were met (its satisfaction).
//! 2. **Grids**: resources with grid transport (power…) are shared within
//!    each connected network component, proportionally when short.
//! 3. **Agents**: resources with agent transport (workers, goods…) travel
//!    along the road network from senders to the nearest units needing them.
//!    A sample of trips is shown as vehicles.
//! 4. **Treasury**: city resources (taxes, upkeep) are paid in and out.
//! 5. **Layers**: decay, diffuse, and receive emissions (pollution, coverage).
//! 6. **Satisfaction**: share of needs met, halved for each unmet layer requirement.
//! 7. **Demand and growth**: zone demand from unmet and undelivered
//!    resources; plots develop, units level up or down by density.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{
    layer::{BuildingLayer, Rect, Structure},
    rules::{structure_code, DemandSource, Ruleset, StanceDef, TaxDef, TechDef, Transport},
    Direction, Footprints, GraphicCode, RoadKind, StructureCode, Terrain,
};

/// Per-unit state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UnitState {
    /// Produced resources waiting to be sent (by resource index).
    pub stock: Vec<f32>,
    /// Received this tick (by resource index).
    pub received: Vec<f32>,
    /// 0..1, smoothed.
    pub satisfaction: f32,
    /// Ticks since the unit was built or last changed level.
    #[serde(default)]
    pub age: u32,
    /// Consecutive ticks above the level-up threshold.
    #[serde(default)]
    pub thriving: u32,
    /// Consecutive ticks below the level-down threshold.
    #[serde(default)]
    pub struggling: u32,
    /// Asked of this unit by its trips (resource `uses`), needed next tick.
    #[serde(default)]
    pub extra: Vec<f32>,
    /// Everything needed this tick (by resource index): own needs plus `extra`.
    #[serde(default)]
    pub needs: Vec<f32>,
}

/// A plot whose building was abandoned rests before developing again:
/// `growth.grace` ticks, doubling with each failure there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vacancy {
    pub since: u64,
    pub strikes: u8,
}

impl Vacancy {
    fn until(&self, grace: u32) -> u64 {
        self.since + ((grace as u64) << self.strikes.min(6))
    }
}

/// One link of the resource network: a resource going from one unit to
/// another (by origin tile).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Link {
    pub resource: usize,
    pub from: (u32, u32),
    pub to: (u32, u32),
}

/// Share of a link's amount that comes from the latest tick; the rest is
/// carried over, so the network view is steady rather than jittery.
const FLOW_SMOOTHING: f32 = 0.2;

/// City-wide balances of the last tick, by resource index.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Balances {
    pub produced: Vec<f32>,
    pub delivered: Vec<f32>,
    pub unmet: Vec<f32>,
    pub undelivered: Vec<f32>,
}

/// One trip made by agents this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trip {
    pub resource: usize,
    pub from: (u32, u32),
    pub to: (u32, u32),
    pub amount: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlassBox {
    pub rules_id: String,
    /// Unit states by origin tile (linear index).
    pub units: BTreeMap<u32, UnitState>,
    /// Map layers, row-major, by layer index.
    pub layers: Vec<Vec<f32>>,
    pub balances: Balances,
    /// Smoothed demand per zone type.
    pub demand: Vec<f32>,
    /// Fractional treasury income carried between ticks.
    pub treasury: f64,
    /// Ticks run so far (the timestamp of metrics).
    #[serde(default)]
    pub tick: u64,
    /// Metrics logged during the last tick (Extended GlassBox `log`).
    #[serde(default)]
    pub metrics: BTreeMap<String, f32>,
    /// Timestamped metric snapshots, most recent last.
    #[serde(default)]
    pub metric_history: VecDeque<(u64, BTreeMap<String, f32>)>,
    /// Plots whose building was abandoned, by origin tile.
    #[serde(default)]
    pub vacated: BTreeMap<u32, Vacancy>,
    /// Everything above, sampled every tick, for graphs and analysis.
    #[serde(default)]
    pub series: crate::series::Series,
    /// Stocks of pooled resources (ideas, research points), by resource.
    #[serde(default)]
    pub pools: Vec<f32>,
    /// Research projects done.
    #[serde(default)]
    pub researched: BTreeSet<String>,
    /// The project being researched.
    #[serde(default)]
    pub research: Option<String>,
    /// What has been spent on each project so far, by resource.
    #[serde(default)]
    pub progress: BTreeMap<String, BTreeMap<String, f32>>,
    /// Tax rates the city set, by tax id (others are at their default).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tax_rates: BTreeMap<String, f32>,
    /// Stances the city took, by policy id (others are at the first).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub stances: BTreeMap<String, String>,
    /// Picks the next project by itself when none is chosen (interfaces
    /// without a research window).
    #[serde(skip)]
    pub auto_research: bool,
    /// Projects finished during the last tick (for notices).
    #[serde(skip)]
    pub discoveries: Vec<String>,
    /// The resource network: amount per tick on each link, smoothed.
    #[serde(skip)]
    pub flows: HashMap<Link, f32>,
    /// Building changes this tick: built, abandoned, levelled up, levelled down.
    #[serde(skip)]
    churn: [u32; 4],
    #[serde(skip)]
    rules: Option<Arc<Ruleset>>,
    #[serde(skip)]
    grid_cache: Option<(u64, Vec<Vec<u32>>)>,
}

const NO_COMPONENT: u32 = u32::MAX;

/// A grid producer: its origin tile and what it supplied this tick.
type Supplier = ((u32, u32), f32);
const BUCKET: u32 = 8;
/// Units this close (in tiles) to a highway can use it: their trips cost
/// this much to get on and off, then highway speed.
const HIGHWAY_RAMP: u32 = 4;
/// How far (in tiles) people walk to a station.
pub const STATION_WALK: u32 = 8;
/// Time (in street tiles) to board and leave a train.
const STATION_TIME: f32 = 3.0;

impl GlassBox {
    pub fn new(rules: Arc<Ruleset>, width: u32, length: u32) -> Self {
        let n = (width * length) as usize;
        Self {
            rules_id: rules.id.clone(),
            units: BTreeMap::new(),
            layers: vec![vec![0.0; n]; rules.layers.len()],
            balances: Balances::default(),
            demand: vec![0.0; rules.zones.len()],
            treasury: 0.0,
            tick: 0,
            metrics: BTreeMap::new(),
            metric_history: VecDeque::new(),
            vacated: BTreeMap::new(),
            series: Default::default(),
            pools: vec![0.0; rules.resources.len()],
            researched: BTreeSet::new(),
            research: None,
            progress: BTreeMap::new(),
            tax_rates: BTreeMap::new(),
            stances: BTreeMap::new(),
            discoveries: Vec::new(),
            auto_research: false,
            flows: HashMap::new(),
            churn: [0; 4],
            rules: Some(rules),
            grid_cache: None,
        }
    }

    /// Re-attaches the rules after loading a save.
    pub fn attach(&mut self, rules: Arc<Ruleset>, width: u32, length: u32) {
        let n = (width * length) as usize;
        self.layers.resize(rules.layers.len(), vec![0.0; n]);
        self.demand.resize(rules.zones.len(), 0.0);
        self.pools.resize(rules.resources.len(), 0.0);
        self.rules = Some(rules);
        self.grid_cache = None;
    }

    /// Whether research has unlocked a unit or tool id: anything no project
    /// names is open from the start, otherwise any project naming it will do.
    pub fn unlocked(&self, rules: &Ruleset, id: &str) -> bool {
        self.locked_by(rules, id).is_none()
    }

    /// The project still to research for a unit or tool id (one that is not
    /// ruled out, when there is a choice), or None when it is open.
    pub fn locked_by<'a>(&self, rules: &'a Ruleset, id: &'a str) -> Option<&'a TechDef> {
        let mut naming = rules.locked_by(id).peekable();
        naming.peek()?;
        let naming: Vec<&TechDef> = naming.collect();
        if naming.iter().any(|t| self.researched.contains(&t.id)) {
            return None;
        }
        naming.iter().find(|t| !rules.tech_excluded(&t.id, &self.researched)).or(naming.first()).copied()
    }

    /// Whether a project can be researched now: not done, what it requires
    /// is done, and no rival project is.
    pub fn can_research(&self, rules: &Ruleset, tech: &str) -> bool {
        rules.tech(tech).is_some_and(|t| {
            !self.researched.contains(tech)
                && rules.techs[t].requires.iter().all(|q| self.researched.contains(q))
                && !rules.tech_excluded(tech, &self.researched)
        })
    }

    /// Whether a rival project already done rules this one out for good.
    pub fn ruled_out(&self, rules: &Ruleset, tech: &str) -> bool {
        !self.researched.contains(tech) && rules.tech_excluded(tech, &self.researched)
    }

    /// Extra production from finished projects and policy stances, as a
    /// share per resource.
    pub fn research_bonus(&self, rules: &Ruleset) -> Vec<f32> {
        let mut b = vec![0.0; rules.resources.len()];
        let techs = rules.techs.iter().filter(|t| self.researched.contains(&t.id)).map(|t| &t.bonus);
        for bonus in techs.chain(self.active_stances(rules).map(|s| &s.bonus)) {
            for (r, v) in bonus {
                if let Some(i) = rules.resource(r) {
                    b[i] += v;
                }
            }
        }
        b
    }

    /// A tax's rate, in percent.
    pub fn tax_rate(&self, rules: &Ruleset, tax: &str) -> f32 {
        let Some(t) = rules.tax(tax) else { return 0.0 };
        self.tax_rates.get(tax).copied().unwrap_or(t.rate)
    }

    /// Sets a tax's rate, kept within its range; false for an unknown tax.
    pub fn set_tax_rate(&mut self, tax: &str, rate: f32) -> bool {
        let Some(t) = self.rules.as_ref().and_then(|r| r.tax(tax).cloned()) else { return false };
        let rate = if rate.is_finite() { rate.clamp(t.range[0], t.range[1]) } else { t.rate };
        if (rate - t.rate).abs() < 1e-4 {
            self.tax_rates.remove(tax);
        } else {
            self.tax_rates.insert(tax.to_string(), rate);
        }
        true
    }

    /// The stance taken on a policy (its first one unless chosen).
    pub fn stance<'a>(&self, rules: &'a Ruleset, policy: &str) -> Option<&'a StanceDef> {
        let p = rules.policy(policy)?;
        self.stances.get(policy).and_then(|id| p.stances.iter().find(|s| &s.id == id)).or(p.stances.first())
    }

    /// Every policy's current stance.
    pub fn active_stances<'a>(&'a self, rules: &'a Ruleset) -> impl Iterator<Item = &'a StanceDef> + 'a {
        rules.policies.iter().filter_map(move |p| self.stance(rules, &p.id))
    }

    /// Whether a stance can be taken: its research is done.
    pub fn stance_open(&self, stance: &StanceDef) -> bool {
        stance.requires.as_ref().is_none_or(|q| self.researched.contains(q))
    }

    /// Takes a stance on a policy; false when unknown or still locked.
    pub fn set_stance(&mut self, policy: &str, stance: &str) -> bool {
        let Some(rules) = self.rules.clone() else { return false };
        let Some(p) = rules.policy(policy) else { return false };
        let Some(st) = p.stances.iter().find(|s| s.id == stance) else { return false };
        if !self.stance_open(st) {
            return false;
        }
        if p.stances.first().is_some_and(|f| f.id == stance) {
            self.stances.remove(policy);
        } else {
            self.stances.insert(policy.to_string(), stance.to_string());
        }
        true
    }

    /// Share of the written amount a unit of `zone` makes of `resource`
    /// under the tax rates (1 when no tax applies).
    fn tax_factor(&self, rules: &Ruleset, zone: Option<usize>, resource: &str) -> f32 {
        let mut f = 1.0;
        for t in rules.taxes.iter().filter(|t| t.resource == resource && taxes_zone(t, rules, zone)) {
            f *= self.tax_rate(rules, &t.id) / t.rate;
        }
        f
    }

    /// Picks the project to research (or none); false if it cannot be researched.
    pub fn set_research(&mut self, tech: Option<&str>) -> bool {
        let Some(rules) = self.rules.clone() else { return false };
        match tech {
            Some(t) if !self.can_research(&rules, t) => false,
            _ => {
                self.research = tech.map(str::to_string);
                true
            }
        }
    }

    /// Share of a project paid so far (0..1).
    pub fn research_progress(&self, rules: &Ruleset, tech: &str) -> f32 {
        let Some(t) = rules.tech(tech).map(|t| &rules.techs[t]) else { return 0.0 };
        if self.researched.contains(tech) {
            return 1.0;
        }
        let total: f32 = t.cost.values().sum();
        let paid: f32 = self.progress.get(tech).map_or(0.0, |p| t.cost.iter().map(|(r, c)| p.get(r).copied().unwrap_or(0.0).min(*c)).sum());
        paid / total.max(1e-6)
    }

    /// Marks every project done (demo towns, tests, sandbox play).
    pub fn research_all(&mut self) {
        if let Some(rules) = &self.rules {
            // In file order, skipping rivals of what is already chosen.
            for t in &rules.techs {
                if !rules.tech_excluded(&t.id, &self.researched) {
                    self.researched.insert(t.id.clone());
                }
            }
        }
        self.research = None;
    }

    /// Spends the pools on the current project, finishing it when paid.
    fn advance_research(&mut self, rules: &Ruleset) {
        self.discoveries.clear();
        if self.research.is_none() && self.auto_research {
            self.research = rules.techs.iter().find(|t| self.can_research(rules, &t.id)).map(|t| t.id.clone());
        }
        let Some(id) = self.research.clone() else { return };
        let Some(t) = rules.tech(&id).map(|t| &rules.techs[t]) else {
            self.research = None;
            return;
        };
        let paid = self.progress.entry(id.clone()).or_default();
        let mut done = true;
        for (r, cost) in &t.cost {
            let Some(ri) = rules.resource(r) else { continue };
            let have = paid.entry(r.clone()).or_default();
            let take = (cost - *have).max(0.0).min(self.pools[ri].max(0.0));
            self.pools[ri] -= take;
            *have += take;
            done &= *have >= cost - 1e-3;
        }
        if done {
            self.progress.remove(&id);
            self.researched.insert(id.clone());
            self.discoveries.push(id);
            self.research = None;
        }
    }

    pub fn rules(&self) -> Option<&Arc<Ruleset>> {
        self.rules.as_ref()
    }

    pub fn layer_value(&self, layer: usize, w: u32, l: u32, width: u32) -> f32 {
        self.layers.get(layer).and_then(|v| v.get((l * width + w) as usize)).copied().unwrap_or(0.0)
    }

    /// What a unit lacks: (resource, missing amount per tick) for each need
    /// met by less than 95%, its trips' needs included.
    pub fn shortfall(&self, rules: &Ruleset, unit: usize, w: u32, l: u32, width: u32) -> Vec<(usize, f32)> {
        let Some(st) = self.unit_state(w, l, width) else { return Vec::new() };
        let own = rules.units[unit].needs.iter().filter_map(|(r, n)| Some((rules.resource(r)?, *n)));
        let wanted: Vec<(usize, f32)> =
            if st.needs.is_empty() { own.collect() } else { st.needs.iter().copied().enumerate().filter(|(_, v)| *v > 0.0).collect() };
        wanted
            .into_iter()
            .filter_map(|(ri, need)| {
                let missing = need - st.received.get(ri).copied().unwrap_or(0.0);
                (missing > need * 0.05).then_some((ri, missing))
            })
            .collect()
    }

    pub fn unit_state(&self, w: u32, l: u32, width: u32) -> Option<&UnitState> {
        self.units.get(&(l * width + w))
    }

    /// Total of a local resource (e.g. residents) over all units.
    pub fn local_total(&self, resource: usize) -> f32 {
        self.units.values().filter_map(|u| u.stock.get(resource)).sum()
    }

    /// Summary for the status bar: R/C/I as unit levels, power as the first grid resource.
    pub fn values(&self, layer: &BuildingLayer) -> crate::sim::SimValues {
        let mut v = crate::sim::SimValues::default();
        for (_, s) in layer.structures() {
            if s.unit.is_none() {
                continue;
            }
            match s.code {
                StructureCode::Residential => v.residential += s.level as u32,
                StructureCode::Commercial => v.commercial += s.level as u32,
                StructureCode::Industrial => v.industrial += s.level as u32,
                _ => {}
            }
        }
        if let Some(rules) = &self.rules {
            if let Some(ri) = rules.resources.iter().position(|r| r.transport == Transport::Grid) {
                let b = &self.balances;
                let (made, wanted) =
                    (b.produced.get(ri).copied().unwrap_or(0.0), b.delivered.get(ri).copied().unwrap_or(0.0) + b.unmet.get(ri).copied().unwrap_or(0.0));
                v.power_capacity = made as i32;
                v.power = (made - wanted) as i32;
            }
        }
        v
    }

    /// Runs one tick. Returns the trips made, for traffic display.
    pub fn tick(&mut self, layer: &mut BuildingLayer, terrain: &Terrain, footprints: &Footprints, fund: &mut i64, rng: &mut impl Rng) -> Vec<Trip> {
        let Some(rules) = self.rules.clone() else { return Vec::new() };
        let nres = rules.resources.len();
        let (width, length) = (layer.width(), layer.length());
        let idx = |w: u32, l: u32| l * width + w;

        // Units present on the map.
        let mut units: Vec<((u32, u32), usize, Rect)> = layer
            .structures()
            .filter_map(|(p, s)| {
                let u = s.unit? as usize;
                (u < rules.units.len()).then(|| {
                    let (sw, sl) = s.size(footprints);
                    (p, u, Rect::sized(p.0, p.1, sw, sl))
                })
            })
            .collect();
        units.retain(|(_, _, r)| layer.in_bounds(*r));
        let present: std::collections::BTreeSet<u32> = units.iter().map(|(p, _, _)| idx(p.0, p.1)).collect();
        self.units.retain(|k, _| present.contains(k));
        for (p, _, _) in &units {
            let s = self.units.entry(idx(p.0, p.1)).or_insert_with(|| UnitState { satisfaction: 0.5, ..Default::default() });
            s.stock.resize(nres, 0.0);
            s.extra.resize(nres, 0.0);
            s.received = vec![0.0; nres];
        }
        // What each unit needs this tick: its own needs, plus what its trips
        // of the last tick asked of other networks (resource `uses`).
        let need: Vec<Vec<f32>> = units
            .iter()
            .map(|(p, u, _)| {
                let mut n = vec![0.0; nres];
                for (r, v) in &rules.units[*u].needs {
                    n[rules.resource(r).unwrap()] += v;
                }
                let st = self.units.get_mut(&idx(p.0, p.1)).unwrap();
                for (ri, e) in st.extra.iter_mut().enumerate() {
                    n[ri] += *e;
                    *e = 0.0;
                }
                st.needs = n.clone();
                n
            })
            .collect();
        self.tick += 1;
        let mut metrics: BTreeMap<String, f32> = BTreeMap::new();
        let mut bal = Balances { produced: vec![0.0; nres], delivered: vec![0.0; nres], unmet: vec![0.0; nres], undelivered: vec![0.0; nres] };

        // 1. Production. A resource with kinds is made as its kinds, by share.
        // Finished research adds its bonus to a resource and to its kinds.
        let boost = self.research_bonus(&rules);
        let unit_zone: Vec<Option<usize>> = rules.units.iter().map(|u| rules.zone_of_unit(&u.id)).collect();
        // Tax rates scale what each kind of unit makes, by resource.
        let unit_tax: Vec<BTreeMap<String, f32>> = rules
            .units
            .iter()
            .enumerate()
            .map(|(u, def)| def.produce.keys().map(|r| (r.clone(), self.tax_factor(&rules, unit_zone[u], r))).collect())
            .collect();
        for (p, u, _) in &units {
            let def = &rules.units[*u];
            let st = self.units.get_mut(&idx(p.0, p.1)).unwrap();
            let eff = st.satisfaction.max(0.25);
            for (m, n) in &def.logs {
                *metrics.entry(m.clone()).or_default() += n * eff;
            }
            for (r, amount) in &def.produce {
                let ri = rules.resource(r).unwrap();
                let up = (1.0 + boost[ri] + if rules.root(ri) != ri { boost[rules.root(ri)] } else { 0.0 }) * unit_tax[*u].get(r).copied().unwrap_or(1.0);
                let made = amount * eff * up;
                add(&mut bal.produced, ri, rules.root(ri), made);
                for (k, f) in split(&rules, ri) {
                    if k != ri {
                        bal.produced[k] += made * f;
                    }
                    let m = made * f;
                    match rules.resources[k].transport {
                        Transport::City => self.treasury += m as f64,
                        Transport::Pool => self.pools[k] += m,
                        Transport::Local | Transport::Grid => st.stock[k] = m,
                        Transport::Agents => st.stock[k] = (st.stock[k] + m).min(amount * f * 3.0),
                    }
                }
            }
        }

        // 2. Grids. Kinds serve needs for that kind first; what is left fills
        // needs for the resource (slots any kind fills), shared out evenly.
        let mut tick_flows: HashMap<Link, f32> = HashMap::new();
        let comps = self.components(&rules, layer, terrain);
        for root in 0..nres {
            let res = &rules.resources[root];
            if res.transport != Transport::Grid || res.parent.is_some() {
                continue;
            }
            let fam = rules.family(root);
            let net = rules.network(res.network.as_deref().unwrap_or_default()).unwrap();
            let comp: Vec<Option<u32>> = units.iter().map(|(_, _, r)| unit_component(&comps[net], r, width, length, 1, |_, _| true).map(|a| a.0)).collect();
            let mut pools: HashMap<u32, Pool> = HashMap::new();
            let mut producers: HashMap<(u32, usize), Vec<Supplier>> = HashMap::new();
            for (i, (p, _, _)) in units.iter().enumerate() {
                let Some(c) = comp[i] else { continue };
                let pool = pools.entry(c).or_insert_with(|| Pool::new(fam.len()));
                let st = &self.units[&idx(p.0, p.1)];
                for (j, &k) in fam.iter().enumerate() {
                    if st.stock[k] > 0.0 {
                        pool.supply[j] += st.stock[k];
                        producers.entry((c, j)).or_default().push((*p, st.stock[k]));
                    }
                    if j > 0 {
                        pool.want[j] += need[i][k];
                    }
                }
                pool.slot += need[i][root];
            }
            pools.values_mut().for_each(Pool::settle);
            for (i, (p, _, _)) in units.iter().enumerate() {
                let pool = comp[i].and_then(|c| pools.get(&c).map(|pl| (c, pl)));
                for (j, &k) in fam.iter().enumerate() {
                    let n = need[i][k];
                    if n <= 0.0 {
                        continue;
                    }
                    let mut got = 0.0;
                    if let Some((c, pl)) = pool {
                        // (kind, amount) that served this need.
                        let parts: Vec<(usize, f32)> = if j > 0 {
                            vec![(j, n * pl.kind_share[j])]
                        } else {
                            let slot_total: f32 = pl.slot_take.iter().sum();
                            (0..fam.len()).filter(|&q| pl.slot_take[q] > 0.0).map(|q| (q, n * pl.slot_share * pl.slot_take[q] / slot_total)).collect()
                        };
                        for (q, amount) in parts {
                            got += amount;
                            if fam[q] != root {
                                bal.delivered[fam[q]] += amount;
                            }
                            for &(from, made) in producers.get(&(c, q)).into_iter().flatten() {
                                if from != *p && amount > 0.0 {
                                    *tick_flows.entry(Link { resource: fam[q], from, to: *p }).or_default() += amount * made / pl.supply[q];
                                }
                            }
                        }
                    }
                    self.units.get_mut(&idx(p.0, p.1)).unwrap().received[k] += got;
                    bal.delivered[root] += got;
                    add(&mut bal.unmet, k, root, (n - got).max(0.0));
                }
                let st = self.units.get_mut(&idx(p.0, p.1)).unwrap();
                for (j, &k) in fam.iter().enumerate() {
                    if st.stock[k] > 0.0 {
                        let used = pool.map_or(0.0, |(_, pl)| pl.used(j));
                        add(&mut bal.undelivered, k, root, st.stock[k] * (1.0 - used));
                        st.stock[k] = 0.0;
                    }
                }
            }
        }

        // 3. Agents. A need for a kind takes only that kind; a need for the
        // resource is a slot filled by any kind, from the nearest senders.
        // Trips are measured in street tiles: slower roads make them longer,
        // and highways shorten them between units near a highway.
        let road_at = |w: u32, l: u32| layer.structure_at(w, l).filter(|s| s.code == StructureCode::Road).map(|s| s.road);
        let mut fastest = 0f32;
        let mut near_highway = Vec::new();
        for ((w, l), s) in layer.structures().filter(|(_, s)| s.code == StructureCode::Road) {
            fastest = fastest.max(s.road.speed());
            if s.road == RoadKind::Highway {
                near_highway.resize((width * length) as usize, false);
                let (w1, l1, w2, l2) = terrain.around(w, l, HIGHWAY_RAMP);
                for (x, y) in (Rect { w1, l1, w2, l2 }).tiles() {
                    near_highway[idx(x, y) as usize] = true;
                }
            }
        }
        // Railways: places near stations on one line are linked, whatever
        // the streets between them (see `rail`).
        let rail = crate::rail::Railways::find(layer, terrain, footprints);
        if !rail.stations.is_empty() {
            fastest = fastest.max(RoadKind::Rail.speed());
        }
        let fastest = fastest.max(0.1);
        let mut trips = Vec::new();
        for root in 0..nres {
            let res = &rules.resources[root];
            if res.transport != Transport::Agents || res.parent.is_some() {
                continue;
            }
            let fam = rules.family(root);
            let net = rules.network(res.network.as_deref().unwrap_or_default()).unwrap();
            // Agents walk from the road to any plot that was allowed to develop.
            let reach = rules.growth.road_access.max(1);
            // Network component, road speed and highway access of each unit,
            // through the nearest road it can open onto (not a highway).
            let access: Vec<Option<(u32, (u32, u32))>> =
                units.iter().map(|(_, _, r)| unit_component(&comps[net], r, width, length, reach, |w, l| road_at(w, l).is_none_or(|k| k.access()))).collect();
            // Street networks joined by a railway count as one: merge their
            // components through the stations on each line.
            let mut merged: HashMap<u32, u32> = HashMap::new();
            let station_comp: Vec<Option<u32>> = rail
                .stations
                .iter()
                .map(|st| {
                    let r = Rect::sized(st.origin.0, st.origin.1, st.size.0, st.size.1);
                    unit_component(&comps[net], &r, width, length, 2, |w, l| road_at(w, l).is_none_or(|k| k.access())).map(|(c, _)| c)
                })
                .collect();
            {
                let mut root_of_line: HashMap<u32, u32> = HashMap::new();
                for (st, c) in rail.stations.iter().zip(&station_comp) {
                    let Some(c) = *c else { continue };
                    let r = *root_of_line.entry(st.line).or_insert(c);
                    let (ra, rb) = (find_root(&merged, r), find_root(&merged, c));
                    if ra != rb {
                        merged.insert(rb, ra);
                    }
                }
            }
            // Component (merged and own), road speed, highway access and the
            // nearest station (index, walk) of each unit.
            #[allow(clippy::type_complexity)]
            let acc: Vec<Option<(u32, u32, f32, bool, Option<(usize, u32)>, (u32, u32))>> = access
                .iter()
                .map(|a| {
                    let (c, (w, l)) = (*a)?;
                    let station = rail.nearest_station((w, l), STATION_WALK).filter(|&(i, _)| station_comp[i].is_some());
                    Some((
                        find_root(&merged, c),
                        c,
                        road_at(w, l).map_or(1.0, |k| k.speed()),
                        near_highway.get(idx(w, l) as usize).copied().unwrap_or(false),
                        station,
                        (w, l),
                    ))
                })
                .collect();
            // The vehicles carrying the resource cap the speed (people on
            // foot are no quicker on a paved street) and may be kept off
            // highways.
            let vehicles = rules.vehicles_for(root);
            let road_vehicles: Vec<_> = vehicles.iter().filter(|v| !v.ways().allows(RoadKind::Rail)).collect();
            let (top, highway) = if vehicles.is_empty() {
                (f32::INFINITY, true)
            } else {
                (road_vehicles.iter().map(|v| v.top_speed()).fold(0.0, f32::max), road_vehicles.iter().any(|v| v.ways().allows(RoadKind::Highway)))
            };
            // Trains carry the resource when the network lists a rail
            // vehicle for it, or lists no vehicles at all.
            let by_rail = vehicles.is_empty() || vehicles.iter().any(|v| v.ways().allows(RoadKind::Rail));
            let trip_length = |s: usize, i: usize, dist: u32| -> f32 {
                let (Some(a), Some(b)) = (acc[s], acc[i]) else { return f32::INFINITY };
                // By road, only within one street network.
                let mut best = if a.1 == b.1 { dist as f32 / a.2.min(b.2).min(top) } else { f32::INFINITY };
                if highway && a.3 && b.3 && a.1 == b.1 {
                    best = best.min(HIGHWAY_RAMP as f32 + dist as f32 / RoadKind::Highway.speed().min(top));
                }
                if let (true, Some((sa, wa)), Some((sb, wb))) = (by_rail, a.4, b.4) {
                    let (x, y) = (&rail.stations[sa], &rail.stations[sb]);
                    if sa != sb && x.line == y.line {
                        let ride = x.platform.0.abs_diff(y.platform.0) + x.platform.1.abs_diff(y.platform.1);
                        let walk = (wa + wb) as f32 / a.2.min(b.2).min(top).min(1.0);
                        best = best.min(STATION_TIME + walk + ride as f32 / RoadKind::Rail.speed());
                    }
                }
                best
            };
            // Which unit types send which members of the family.
            let sends_table: Vec<Vec<bool>> = rules
                .units
                .iter()
                .map(|def| (0..nres).map(|k| fam.contains(&k) && def.sends.iter().any(|s| *s == rules.resources[k].id || *s == res.id)).collect())
                .collect();
            let sends = |u: usize, k: usize| sends_table[u][k];
            let uses: Vec<Vec<(usize, f32)>> =
                (0..nres).map(|k| rules.resources[k].uses.iter().filter_map(|(r, per)| Some((rules.resource(r)?, *per))).collect()).collect();
            // Stocks of the family's members, by unit and member position,
            // and what is left of each member over all senders.
            let mut stock: Vec<Vec<f32>> = units
                .iter()
                .map(|(p, u, _)| {
                    let st = &self.units[&idx(p.0, p.1)];
                    fam.iter().map(|&k| if sends(*u, k) { st.stock[k] } else { 0.0 }).collect()
                })
                .collect();
            let mut left: Vec<f32> = (0..fam.len()).map(|j| stock.iter().map(|s| s[j]).sum()).collect();
            // Senders bucketed by (component, cell), with what each bucket has left.
            let mut buckets: HashMap<(u32, u32, u32), Bucket> = HashMap::new();
            for (i, (p, _, _)) in units.iter().enumerate() {
                if stock[i].iter().all(|&x| x <= 0.0) {
                    continue;
                }
                if let Some((c, ..)) = acc[i] {
                    let b = buckets.entry((c, p.0 / BUCKET, p.1 / BUCKET)).or_insert_with(|| Bucket { senders: Vec::new(), left: vec![0.0; fam.len()] });
                    b.senders.push(i);
                    b.left.iter_mut().zip(&stock[i]).for_each(|(l, x)| *l += x);
                }
            }
            let mut receivers: Vec<usize> = (0..units.len()).filter(|&i| fam.iter().any(|&k| need[i][k] > 0.0)).collect();
            // Established units are served first, in a stable order: who gets
            // the scarce workers or shoppers must not change at random every
            // tick, or buildings flicker between thriving and failing.
            receivers.sort_by_key(|&i| (std::cmp::Reverse(self.units[&idx(units[i].0 .0, units[i].0 .1)].age), i));
            let max_trip = fam.iter().map(|&k| rules.resources[k].max_trip).max().unwrap_or(0);
            let rings = ((max_trip as f32 * fastest).ceil() as u32).div_ceil(BUCKET) as i64;
            let all: Vec<usize> = (0..fam.len()).collect();
            for i in receivers {
                let (p, _, _) = units[i];
                // Needs for a kind first, then the slot any kind fills
                // (members by position in `fam`).
                let keys: Vec<(usize, &[usize])> =
                    (1..fam.len()).filter(|&j| need[i][fam[j]] > 0.0).map(|j| (j, &all[j..=j])).chain((need[i][root] > 0.0).then_some((0, &all[..]))).collect();
                for (key, accept) in keys {
                    let n = need[i][fam[key]];
                    let mut missing = n;
                    let available = |left: &[f32]| accept.iter().any(|&j| left[j] > 0.0);
                    if let Some(c) = acc[i].map(|a| a.0).filter(|_| available(&left)) {
                        // Nearest senders first: rings of buckets.
                        'rings: for ring in 0..=rings {
                            for dz in -ring..=ring {
                                for dx in -ring..=ring {
                                    if dx.abs().max(dz.abs()) != ring {
                                        continue;
                                    }
                                    let (cx, cz) = (p.0 as i64 / BUCKET as i64 + dx, p.1 as i64 / BUCKET as i64 + dz);
                                    if cx < 0 || cz < 0 {
                                        continue;
                                    }
                                    let Some(bucket) = buckets.get_mut(&(c, cx as u32, cz as u32)) else { continue };
                                    if !accept.iter().any(|&j| bucket.left[j] > 0.0) {
                                        continue;
                                    }
                                    for &s in &bucket.senders {
                                        if s == i {
                                            continue;
                                        }
                                        let sp = units[s].0;
                                        let dist = trip_length(s, i, sp.0.abs_diff(p.0) + sp.1.abs_diff(p.1));
                                        for &j in accept {
                                            let a = fam[j];
                                            let take = stock[s][j].min(missing);
                                            if take <= 0.0 || dist > rules.resources[a].max_trip as f32 {
                                                continue;
                                            }
                                            stock[s][j] -= take;
                                            left[j] -= take;
                                            bucket.left[j] -= take;
                                            missing -= take;
                                            // Secondary impact: the trip asks other networks of the sender.
                                            if !uses[a].is_empty() {
                                                let sender = self.units.get_mut(&idx(sp.0, sp.1)).unwrap();
                                                for &(ur, per) in &uses[a] {
                                                    sender.extra[ur] += take * per;
                                                }
                                            }
                                            if a != root {
                                                bal.delivered[a] += take;
                                            }
                                            trips.push(Trip { resource: a, from: sp, to: p, amount: take });
                                            *tick_flows.entry(Link { resource: a, from: sp, to: p }).or_default() += take;
                                            if missing <= 1e-4 || !available(&left) {
                                                break 'rings;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let got = n - missing.max(0.0);
                    self.units.get_mut(&idx(p.0, p.1)).unwrap().received[fam[key]] += got;
                    bal.delivered[root] += got;
                    add(&mut bal.unmet, fam[key], root, missing.max(0.0));
                }
            }
            for (i, (p, u, _)) in units.iter().enumerate() {
                let st = self.units.get_mut(&idx(p.0, p.1)).unwrap();
                for (j, &k) in fam.iter().enumerate() {
                    if sends(*u, k) {
                        st.stock[k] = stock[i][j];
                        add(&mut bal.undelivered, k, root, stock[i][j]);
                    }
                }
            }
        }

        // The network view: links fade unless used again.
        for v in self.flows.values_mut() {
            *v *= 1.0 - FLOW_SMOOTHING;
        }
        for (k, v) in tick_flows {
            *self.flows.entry(k).or_default() += v * FLOW_SMOOTHING;
        }
        self.flows.retain(|k, v| *v > 0.01 && present.contains(&idx(k.from.0, k.from.1)) && present.contains(&idx(k.to.0, k.to.1)));

        // 4. Treasury.
        for (i, (p, _, _)) in units.iter().enumerate() {
            for ri in 0..nres {
                let n = need[i][ri];
                if n <= 0.0 || rules.resources[ri].transport != Transport::City {
                    continue;
                }
                let st = self.units.get_mut(&idx(p.0, p.1)).unwrap();
                if *fund > 0 || self.treasury > n as f64 {
                    self.treasury -= n as f64;
                    st.received[ri] = n;
                    bal.delivered[ri] += n;
                } else {
                    bal.unmet[ri] += n;
                }
            }
        }
        // Pooled resources: needs draw on the city-wide stock.
        for (i, (p, _, _)) in units.iter().enumerate() {
            for ri in 0..nres {
                let n = need[i][ri];
                if n <= 0.0 || rules.resources[ri].transport != Transport::Pool {
                    continue;
                }
                let got = n.min(self.pools[ri].max(0.0));
                self.pools[ri] -= got;
                self.units.get_mut(&idx(p.0, p.1)).unwrap().received[ri] = got;
                bal.delivered[ri] += got;
                bal.unmet[ri] += n - got;
            }
        }
        self.advance_research(&rules);
        // Running costs of policies (negative ones earn).
        self.treasury -= self.active_stances(&rules).map(|st| st.cost as f64).sum::<f64>();
        let whole = self.treasury.trunc();
        *fund += whole as i64;
        self.treasury -= whole;

        // 5. Layers.
        for (li, def) in rules.layers.iter().enumerate() {
            let v = &mut self.layers[li];
            if def.diffusion > 0.0 {
                let old = v.clone();
                for l in 0..length {
                    for w in 0..width {
                        let mut sum = 0.0f32;
                        let mut n = 0.0f32;
                        for d in Direction::ALL {
                            if let Some((nw, nl)) = terrain.neighbour(w, l, d) {
                                sum += old[idx(nw, nl) as usize];
                                n += 1.0;
                            }
                        }
                        let i = idx(w, l) as usize;
                        v[i] = old[i] * (1.0 - def.diffusion) + def.diffusion * sum / n.max(1.0);
                    }
                }
            }
            if def.decay > 0.0 {
                v.iter_mut().for_each(|x| *x *= 1.0 - def.decay);
            }
        }
        for (p, u, r) in &units {
            let eff = self.units[&idx(p.0, p.1)].satisfaction.max(0.25);
            for (lname, e) in &rules.units[*u].emits {
                let li = rules.layer(lname).unwrap();
                if e.by_road {
                    emit_by_road(&mut self.layers[li], layer, terrain, r, e.amount * eff, e.radius);
                    continue;
                }
                let (cx, cz) = ((r.w1 + r.w2) as f32 / 2.0, (r.l1 + r.l2) as f32 / 2.0);
                let rad = e.radius + (r.w2 - r.w1).max(r.l2 - r.l1).div_ceil(2);
                emit(&mut self.layers[li], width, length, cx, cz, e.amount * eff, rad);
            }
        }
        // Agent rules: emissions along the routes of this tick's trips.
        for ar in &rules.agent_rules {
            let wanted: Vec<usize> = ar.resources.iter().filter_map(|r| rules.resource(r)).collect();
            let follows = |r: usize| ar.resources.is_empty() || wanted.contains(&r) || wanted.contains(&rules.root(r));
            for t in trips.iter().filter(|t| follows(t.resource)) {
                let (mut x, mut z) = (t.from.0 as i64, t.from.1 as i64);
                let (tx, tz) = (t.to.0 as i64, t.to.1 as i64);
                let mut steps = 0u32;
                while (x, z) != (tx, tz) {
                    if x != tx {
                        x += (tx - x).signum()
                    } else {
                        z += (tz - z).signum()
                    }
                    steps += 1;
                    if steps % ar.every.max(1) == 0 {
                        for a in &ar.areas {
                            let li = rules.layer(&a.layer).unwrap();
                            emit(&mut self.layers[li], width, length, x as f32, z as f32, a.amount, a.radius);
                        }
                    }
                }
            }
        }
        // Resource trails: layers along the routes of this tick's trips.
        for t in &trips {
            let trail = &rules.resources[t.resource].trail;
            if trail.is_empty() {
                continue;
            }
            let lis: Vec<(usize, f32)> = trail.iter().map(|(l, a)| (rules.layer(l).unwrap(), a * t.amount)).collect();
            let (mut x, mut z) = (t.from.0 as i64, t.from.1 as i64);
            let (tx, tz) = (t.to.0 as i64, t.to.1 as i64);
            while (x, z) != (tx, tz) {
                if x != tx {
                    x += (tx - x).signum()
                } else {
                    z += (tz - z).signum()
                }
                for &(li, a) in &lis {
                    self.layers[li][(z as u32 * width + x as u32) as usize] += a;
                }
            }
        }
        // Map rules, tile by tile.
        for mr in &rules.map_rules {
            if self.tick % mr.every.max(1) as u64 != 0 {
                continue;
            }
            let ins: Vec<(usize, f32)> = mr.inputs.iter().map(|(l, v)| (rules.layer(l).unwrap(), *v)).collect();
            let outs: Vec<(usize, f32)> = mr.outputs.iter().map(|(l, v)| (rules.layer(l).unwrap(), *v)).collect();
            let conds: Vec<(usize, &crate::rules::Test)> = mr.conditions.iter().map(|c| (rules.layer(&c.id).unwrap(), c)).collect();
            for i in 0..(width * length) as usize {
                if conds.iter().all(|(li, c)| c.cmp.holds(self.layers[*li][i], c.value)) && ins.iter().all(|(li, v)| self.layers[*li][i] >= *v) {
                    ins.iter().for_each(|(li, v)| self.layers[*li][i] -= v);
                    outs.iter().for_each(|(li, v)| self.layers[*li][i] += v);
                }
            }
        }
        // Consuming rules cannot take a map below zero.
        self.layers.iter_mut().flatten().for_each(|v| *v = v.max(0.0));

        // 6. Satisfaction: the share of needs met (own needs and those of trips).
        let mood: f32 = self.active_stances(&rules).map(|st| st.satisfaction).sum();
        for (n, (p, u, _)) in units.iter().enumerate() {
            let def = &rules.units[*u];
            let i = idx(p.0, p.1);
            let st = &self.units[&i];
            let wanted: Vec<(usize, f32)> = need[n].iter().copied().enumerate().filter(|(_, v)| *v > 0.0).collect();
            let mut s =
                if wanted.is_empty() { 1.0 } else { wanted.iter().map(|(ri, v)| (st.received[*ri] / v.max(1e-6)).min(1.0)).sum::<f32>() / wanted.len() as f32 };
            for (lname, b) in &def.requires {
                let v = self.layers[rules.layer(lname).unwrap()][i as usize];
                if b.min.is_some_and(|m| v < m) || b.max.is_some_and(|m| v > m) {
                    s *= 0.5;
                }
            }
            // Policies and taxes above (or below) their default rate.
            s += mood;
            for t in rules.taxes.iter().filter(|t| taxes_zone(t, &rules, unit_zone[*u]) && def.produce.contains_key(&t.resource)) {
                s += t.satisfaction * (self.tax_rate(&rules, &t.id) - t.rate);
            }
            let s = s.clamp(0.0, 1.0);
            let g = &rules.growth;
            let st = self.units.get_mut(&i).unwrap();
            st.satisfaction = st.satisfaction * 0.7 + s * 0.3;
            st.age = st.age.saturating_add(1);
            st.thriving = if st.satisfaction > g.level_up { st.thriving.saturating_add(1) } else { 0 };
            st.struggling = if st.satisfaction < g.level_down { st.struggling.saturating_add(1) } else { 0 };
        }

        // 7. Demand and growth.
        for (zi, z) in rules.zones.iter().enumerate() {
            let mut d = z.base_demand;
            for t in &z.demand {
                let ri = rules.resource(&t.resource).unwrap();
                d += t.weight
                    * match t.from {
                        DemandSource::Unmet => bal.unmet[ri],
                        DemandSource::Undelivered => bal.undelivered[ri],
                    };
            }
            for st in self.active_stances(&rules) {
                d += st.demand.get(&z.id).copied().unwrap_or(0.0);
            }
            for t in rules.taxes.iter().filter(|t| taxes_zone(t, &rules, Some(zi))) {
                d += t.demand * (self.tax_rate(&rules, &t.id) - t.rate);
            }
            self.demand[zi] = self.demand[zi] * 0.8 + d * 0.2;
        }
        self.balances = bal;
        self.metric_history.push_back((self.tick, metrics.clone()));
        while self.metric_history.len() > 1000 {
            self.metric_history.pop_front();
        }
        self.metrics = metrics;
        self.churn = [0; 4];
        self.grow(&rules, layer, terrain, footprints, rng);
        self.zone_rules(&rules, layer, terrain, footprints, rng);
        self.record(&rules, layer, *fund);
        trips
    }

    /// Samples this tick into [`Self::series`].
    fn record(&mut self, rules: &Ruleset, layer: &BuildingLayer, fund: i64) {
        let mut v: Vec<(String, f32)> = Vec::new();
        for (z, d) in rules.zones.iter().zip(&self.demand) {
            v.push((format!("Demand/{}", z.name), *d));
        }
        let b = &self.balances;
        for (i, r) in rules.resources.iter().enumerate() {
            for (group, x) in [("Produced", &b.produced), ("Delivered", &b.delivered), ("Unmet", &b.unmet), ("Undelivered", &b.undelivered)] {
                v.push((format!("{group}/{}", r.name), x[i]));
            }
        }
        let width = layer.width();
        let mut per_zone = vec![(0u32, 0f32); rules.zones.len()];
        let mut services = 0u32;
        for (p, s) in layer.structures() {
            if s.unit.is_none() {
                continue;
            }
            let sat = self.units.get(&(p.1 * width + p.0)).map_or(0.0, |st| st.satisfaction);
            match rules.zones.iter().position(|z| structure_code(&z.structure) == Some(s.code)) {
                Some(zi) => {
                    per_zone[zi].0 += 1;
                    per_zone[zi].1 += sat;
                }
                None => services += 1,
            }
        }
        v.push(("Buildings/Services".into(), services as f32));
        for (z, (n, sat)) in rules.zones.iter().zip(&per_zone) {
            v.push((format!("Buildings/{}", z.name), *n as f32));
            if *n > 0 {
                v.push((format!("Satisfaction/{}", z.name), sat / *n as f32));
            }
        }
        for (def, values) in rules.layers.iter().zip(&self.layers) {
            let n = values.len().max(1) as f32;
            v.push((format!("Layer mean/{}", def.name), values.iter().sum::<f32>() / n));
            v.push((format!("Layer max/{}", def.name), values.iter().cloned().fold(0.0, f32::max)));
        }
        for (name, m) in &self.metrics {
            v.push((format!("Metric/{name}"), *m));
        }
        v.push(("City/Funds".into(), fund as f32));
        for (label, n) in ["Built", "Abandoned", "Levelled up", "Levelled down"].iter().zip(self.churn) {
            v.push((format!("Growth/{label}"), n as f32));
        }
        self.series.push(self.tick, v.iter().map(|(k, x)| (k.as_str(), *x)));
    }

    /// Network components: per network, a component id per tile.
    fn components(&mut self, rules: &Ruleset, layer: &BuildingLayer, terrain: &Terrain) -> Vec<Vec<u32>> {
        if let Some((rev, c)) = &self.grid_cache {
            if *rev == layer.revision() {
                return c.clone();
            }
        }
        let (width, length) = (layer.width(), layer.length());
        let mut all = Vec::new();
        for net in &rules.networks {
            let codes: Vec<StructureCode> = net.structures.iter().filter_map(|s| structure_code(s)).collect();
            let rail = net.structures.iter().any(|s| s == "rail");
            let conducts = |w: u32, l: u32| match layer.structure_at(w, l) {
                None => false,
                // Track belongs to rail networks, not to the streets.
                Some(s) if s.code == StructureCode::Road && s.road.is_rail() => rail || (codes.contains(&StructureCode::Road) && layer.is_level_crossing(w, l)),
                Some(s) => codes.contains(&s.code) || (net.through_buildings && s.unit.is_some()),
            };
            let mut comp = vec![NO_COMPONENT; (width * length) as usize];
            let mut next = 0;
            for l in 0..length {
                for w in 0..width {
                    let i = (l * width + w) as usize;
                    if comp[i] != NO_COMPONENT || !conducts(w, l) {
                        continue;
                    }
                    comp[i] = next;
                    let mut q = VecDeque::from([(w, l)]);
                    while let Some((x, y)) = q.pop_front() {
                        for d in Direction::ALL {
                            if let Some((nx, ny)) = terrain.neighbour(x, y, d) {
                                let j = (ny * width + nx) as usize;
                                if comp[j] == NO_COMPONENT && conducts(nx, ny) {
                                    comp[j] = next;
                                    q.push_back((nx, ny));
                                }
                            }
                        }
                    }
                    next += 1;
                }
            }
            all.push(comp);
        }
        self.grid_cache = Some((layer.revision(), all.clone()));
        all
    }

    /// GlassBox `zoneRule`s: sample empty plots and build where the tests hold.
    fn zone_rules(&mut self, rules: &Ruleset, layer: &mut BuildingLayer, terrain: &Terrain, footprints: &Footprints, rng: &mut impl Rng) {
        let width = layer.width();
        for zr in &rules.zone_rules {
            if self.tick % zr.every.max(1) as u64 != 0 {
                continue;
            }
            let code = zr.zone.as_deref().and_then(|z| rules.zones.iter().find(|x| x.id == z)).and_then(|z| structure_code(&z.structure));
            let mut plots: Vec<(u32, u32)> = layer
                .structures()
                .filter(|(_, s)| s.code.is_zone() && s.unit.is_none() && s.level == 0 && code.is_none_or(|c| c == s.code))
                .map(|(p, _)| p)
                .collect();
            plots.shuffle(rng);
            let unit = rules.unit(&zr.create).unwrap();
            if !self.unlocked(rules, &zr.create) {
                continue;
            }
            for (w, l) in plots.into_iter().take(zr.sample as usize) {
                if !layer.tile(w, l).and_then(|t| t.structure()).is_some_and(|s| s.unit.is_none() && s.level == 0) {
                    continue;
                }
                let ok = zr.tests.iter().all(|t| {
                    let v = match t.scope {
                        crate::rules::Scope::Map => rules.layer(&t.id).map_or(0.0, |li| self.layers[li][(l * width + w) as usize]),
                        crate::rules::Scope::Global => {
                            rules.resource(&t.id).map(|ri| self.balances.produced[ri]).or_else(|| self.metrics.get(&t.id).copied()).unwrap_or(0.0)
                        }
                    };
                    t.cmp.holds(v, t.value)
                });
                if ok && set_unit(layer, terrain, footprints, rng, w, l, Some(unit), 1, rules) {
                    self.churn[0] += 1;
                }
            }
        }
    }

    /// Places new buildings on plots and changes unit levels.
    fn grow(&mut self, rules: &Ruleset, layer: &mut BuildingLayer, terrain: &Terrain, footprints: &Footprints, rng: &mut impl Rng) {
        let g = &rules.growth;
        let width = layer.width();
        let road_near = |layer: &BuildingLayer, w: u32, l: u32| {
            let (w1, l1, w2, l2) = terrain.around(w, l, g.road_access);
            layer.contains(Rect { w1, l1, w2, l2 }, crate::layer::Query::RoadAccess)
        };
        for (zi, z) in rules.zones.iter().enumerate() {
            let Some(code) = structure_code(&z.structure) else { continue };
            // Oversupply: while many established units of this zone are failing,
            // more of them would only be abandoned in turn.
            let (mut settled, mut failing) = (0u32, 0u32);
            for (p, s) in layer.structures() {
                if s.code != code || s.unit.is_none() {
                    continue;
                }
                if let Some(u) = self.units.get(&(p.1 * width + p.0)).filter(|u| u.age >= g.patience) {
                    settled += 1;
                    failing += (u.struggling > 0) as u32;
                }
            }
            let saturated = failing as f32 > settled as f32 * g.saturation;
            // New buildings on empty plots.
            if self.demand[zi] > 0.5 && !saturated {
                let mut plots: Vec<(u32, u32)> =
                    layer.structures().filter(|(_, s)| s.code == code && s.unit.is_none() && s.level == 0).map(|(p, _)| p).collect();
                plots.shuffle(rng);
                let mut placed = 0;
                for (w, l) in plots {
                    if placed >= g.max_new {
                        break;
                    }
                    let still_plot = layer.tile(w, l).and_then(|t| t.structure()).is_some_and(|s| s.code == code && s.unit.is_none() && s.level == 0);
                    let resting = self.vacated.get(&(l * width + w)).is_some_and(|v| self.tick < v.until(g.grace));
                    if !still_plot || resting || !road_near(layer, w, l) {
                        continue;
                    }
                    let density = layer.structure_at(w, l).map_or(0, |s| s.density as usize).min(z.densities.len() - 1);
                    let unit = rules.unit(&z.densities[density].units[0]).unwrap();
                    if !self.unlocked(rules, &rules.units[unit].id) {
                        continue;
                    }
                    if set_unit(layer, terrain, footprints, rng, w, l, Some(unit), 1, rules) {
                        placed += 1;
                        self.churn[0] += 1;
                    }
                }
            }
            // Level changes.
            let units: Vec<((u32, u32), u16, u8)> =
                layer.structures().filter(|(_, s)| s.code == code).filter_map(|(p, s)| Some((p, s.unit?, s.density))).collect();
            for (p, u, density) in units {
                if layer.tile(p.0, p.1).and_then(|t| t.structure()).and_then(|s| s.unit) != Some(u) {
                    continue; // swallowed or changed earlier in this pass
                }
                let key = p.1 * width + p.0;
                let Some(st) = self.units.get(&key) else { continue };
                if st.age < g.grace {
                    continue;
                }
                if st.age > 2 * g.grace && st.struggling == 0 {
                    self.vacated.remove(&key); // the plot works now
                }
                let (up, down) = (st.thriving >= g.patience, st.struggling >= g.patience);
                let d = &z.densities[(density as usize).min(z.densities.len() - 1)];
                let Some(k) = d.units.iter().position(|id| rules.unit(id) == Some(u as usize)) else { continue };
                // Never grow into a unit whose map requirements already fail
                // here: it would only shrink back, and flicker.
                let fits = |unit: usize| {
                    rules.units[unit].requires.iter().all(|(l, b)| {
                        let v = rules.layer(l).map_or(0.0, |li| self.layers[li][key as usize]);
                        !(b.min.is_some_and(|m| v < m) || b.max.is_some_and(|m| v > m))
                    })
                };
                // Nor into one research has not unlocked yet.
                let next_fits = k + 1 < d.units.len() && fits(rules.unit(&d.units[k + 1]).unwrap()) && self.unlocked(rules, &d.units[k + 1]);
                let changed = if up && next_fits && rng.r#gen::<f32>() < g.chance {
                    let next = rules.unit(&d.units[k + 1]).unwrap();
                    let ok = set_unit(layer, terrain, footprints, rng, p.0, p.1, Some(next), k as u8 + 2, rules);
                    self.churn[2] += ok as u32;
                    ok
                } else if down && rng.r#gen::<f32>() < g.chance {
                    if k > 0 {
                        let prev = rules.unit(&d.units[k - 1]).unwrap();
                        let ok = set_unit(layer, terrain, footprints, rng, p.0, p.1, Some(prev), k as u8, rules);
                        self.churn[3] += ok as u32;
                        ok
                    } else {
                        set_unit(layer, terrain, footprints, rng, p.0, p.1, None, 0, rules);
                        self.units.remove(&key);
                        let strikes = self.vacated.get(&key).map_or(0, |v| v.strikes.saturating_add(1));
                        self.vacated.insert(key, Vacancy { since: self.tick, strikes });
                        self.churn[1] += 1;
                        false
                    }
                } else {
                    false
                };
                if changed {
                    // A new level starts afresh: no flip-flopping straight back.
                    if let Some(st) = self.units.get_mut(&key) {
                        st.age = 0;
                        st.thriving = 0;
                        st.struggling = 0;
                    }
                }
            }
        }
    }
}

/// Adds `amount` around a point, fading linearly to zero at `radius + 1`.
fn emit(values: &mut [f32], width: u32, length: u32, cx: f32, cz: f32, amount: f32, radius: u32) {
    let rad = radius as i64;
    for l in (cz as i64 - rad).max(0)..=(cz as i64 + rad).min(length as i64 - 1) {
        for w in (cx as i64 - rad).max(0)..=(cx as i64 + rad).min(width as i64 - 1) {
            let d = ((w as f32 - cx).powi(2) + (l as f32 - cz).powi(2)).sqrt();
            let k = (1.0 - d / (rad as f32 + 1.0)).max(0.0);
            values[(l as u32 * width + w as u32) as usize] += amount * k;
        }
    }
}

/// Places a building can drive to: travel time (in street tiles) to every
/// road tile reached within `max` from the roads around `r`. Track is not
/// road, but level crossings are.
pub fn road_reach(layer: &BuildingLayer, terrain: &Terrain, r: &Rect, max: f32) -> HashMap<(u32, u32), f32> {
    let road = |w: u32, l: u32| {
        layer.structure_at(w, l).filter(|s| s.code == StructureCode::Road).and_then(|s| {
            if s.road.is_rail() {
                layer.is_level_crossing(w, l).then_some(1.0)
            } else {
                Some(1.0 / s.road.speed())
            }
        })
    };
    let mut best: HashMap<(u32, u32), f32> = HashMap::new();
    let mut heap = std::collections::BinaryHeap::new();
    let (w1, l1) = (r.w1.saturating_sub(2), r.l1.saturating_sub(2));
    let (w2, l2) = ((r.w2 + 2).min(layer.width() - 1), (r.l2 + 2).min(layer.length() - 1));
    for (w, l) in (Rect { w1, l1, w2, l2 }).tiles() {
        if road(w, l).is_some() {
            best.insert((w, l), 0.0);
            heap.push((std::cmp::Reverse(0u32), (w, l)));
        }
    }
    while let Some((std::cmp::Reverse(c), (w, l))) = heap.pop() {
        let c = c as f32 / 100.0;
        if best.get(&(w, l)).is_some_and(|&b| b < c) {
            continue;
        }
        for d in Direction::ALL {
            let Some((nw, nl)) = terrain.neighbour(w, l, d) else { continue };
            let Some(step) = road(nw, nl) else { continue };
            let nc = c + step;
            if nc <= max && best.get(&(nw, nl)).is_none_or(|&b| nc < b) {
                best.insert((nw, nl), nc);
                heap.push((std::cmp::Reverse((nc * 100.0) as u32), (nw, nl)));
            }
        }
    }
    best
}

/// Coverage along the streets (see `Emission::by_road`): each place takes
/// the best of the road tiles within 2 tiles of it, with linear falloff
/// over travel time plus the last steps on foot.
fn emit_by_road(values: &mut [f32], layer: &BuildingLayer, terrain: &Terrain, r: &Rect, amount: f32, radius: u32) {
    let (width, length) = (layer.width(), layer.length());
    let rad = radius as f32 + 1.0;
    let mut got: HashMap<usize, f32> = HashMap::new();
    for ((w, l), c) in road_reach(layer, terrain, r, radius as f32) {
        for y in l.saturating_sub(2)..=(l + 2).min(length - 1) {
            for x in w.saturating_sub(2)..=(w + 2).min(width - 1) {
                let d = (x.abs_diff(w) + y.abs_diff(l)) as f32;
                let k = 1.0 - (c + d) / rad;
                if k > 0.0 {
                    let v = got.entry((y * width + x) as usize).or_default();
                    *v = v.max(amount * k);
                }
            }
        }
    }
    for (i, v) in got {
        values[i] += v;
    }
}

/// Changes the unit (and model) of the structure at `(w, l)`; `None` empties the plot.
/// Returns false when the new model does not fit.
#[allow(clippy::too_many_arguments)]
fn set_unit(
    layer: &mut BuildingLayer,
    terrain: &Terrain,
    footprints: &Footprints,
    rng: &mut impl Rng,
    w: u32,
    l: u32,
    unit: Option<usize>,
    level: u8,
    rules: &Ruleset,
) -> bool {
    // Only a building's origin tile carries it; a tile swallowed by a growing
    // neighbour earlier in the same pass is a part and is left alone.
    let Some(s) = layer.tile(w, l).and_then(|t| t.structure()).cloned() else { return false };
    let graphic = match unit {
        Some(u) => GraphicCode(rules.units[u].graphic),
        None => s.code.base_graphic(),
    };
    if graphic != s.graphic && !crate::sim::can_take_shape(layer, terrain, footprints, w, l, &s, graphic) {
        return false;
    }
    let old = s.graphic;
    let st = layer.structure_mut(w, l).unwrap();
    st.graphic = graphic;
    st.unit = unit.map(|u| u as u16);
    st.level = level;
    if graphic != old {
        layer.resize(w, l, old, terrain, footprints, rng);
    }
    true
}

/// Adds to a resource's balance, and to its parent's when it is a kind.
fn add(v: &mut [f32], k: usize, root: usize, x: f32) {
    v[k] += x;
    if k != root {
        v[root] += x;
    }
}

/// What producing `resource` makes: itself, or its kinds by share.
fn split(rules: &Ruleset, resource: usize) -> Vec<(usize, f32)> {
    let kinds = rules.kinds(resource);
    let total: f32 = kinds.iter().map(|&k| rules.resources[k].share).sum();
    if kinds.is_empty() || total <= 0.0 {
        vec![(resource, 1.0)]
    } else {
        kinds.into_iter().map(|k| (k, rules.resources[k].share / total)).collect()
    }
}

/// Senders of an agent resource family in one cell of a network component.
struct Bucket {
    senders: Vec<usize>,
    /// Stock left per family member.
    left: Vec<f32>,
}

/// A grid resource family within one network component. Index 0 is the
/// resource itself, then its kinds.
struct Pool {
    supply: Vec<f32>,
    /// Needs for each kind.
    want: Vec<f32>,
    /// Needs for the resource, which any kind fills.
    slot: f32,
    /// Share of a kind's needs met.
    kind_share: Vec<f32>,
    /// Share of the slot needs met.
    slot_share: f32,
    /// Supply of each kind going to slots.
    slot_take: Vec<f32>,
}

impl Pool {
    fn new(n: usize) -> Self {
        Self { supply: vec![0.0; n], want: vec![0.0; n], slot: 0.0, kind_share: vec![0.0; n], slot_share: 0.0, slot_take: vec![0.0; n] }
    }

    fn settle(&mut self) {
        let left: Vec<f32> = (0..self.supply.len()).map(|j| self.supply[j] - self.supply[j].min(self.want[j])).collect();
        for j in 0..self.supply.len() {
            self.kind_share[j] = if self.want[j] > 0.0 { (self.supply[j] / self.want[j]).min(1.0) } else { 0.0 };
        }
        let spare: f32 = left.iter().sum();
        let filled = spare.min(self.slot);
        self.slot_share = if self.slot > 0.0 { filled / self.slot } else { 0.0 };
        for (take, l) in self.slot_take.iter_mut().zip(&left) {
            *take = if spare > 0.0 { filled * l / spare } else { 0.0 };
        }
    }

    /// Share of kind `j`'s supply that was used.
    fn used(&self, j: usize) -> f32 {
        if self.supply[j] <= 0.0 {
            return 0.0;
        }
        ((self.supply[j].min(self.want[j]) + self.slot_take[j]) / self.supply[j]).min(1.0)
    }
}

/// Network component a unit connects to: any of its tiles or the tiles up
/// to `reach` around it, nearest rings first.
/// Also returns the tile it connects through; `usable` filters the tiles
/// it may connect through.
fn unit_component(comp: &[u32], r: &Rect, width: u32, length: u32, reach: u32, usable: impl Fn(u32, u32) -> bool) -> Option<(u32, (u32, u32))> {
    (0..=reach).find_map(|d| {
        let w1 = r.w1.saturating_sub(d);
        let l1 = r.l1.saturating_sub(d);
        let w2 = (r.w2 + d).min(width - 1);
        let l2 = (r.l2 + d).min(length - 1);
        (l1..=l2)
            .flat_map(|l| (w1..=w2).map(move |w| (w, l)))
            .find(|&(w, l)| comp[(l * width + w) as usize] != NO_COMPONENT && usable(w, l))
            .map(|(w, l)| (comp[(l * width + w) as usize], (w, l)))
    })
}

impl Structure {
    /// Whether this is a GlassBox unit (developed building or service).
    pub fn is_unit(&self) -> bool {
        self.unit.is_some()
    }
}

/// Whether a tax applies to units of `zone` (a tax naming no zones applies
/// to every unit).
fn taxes_zone(t: &TaxDef, rules: &Ruleset, zone: Option<usize>) -> bool {
    t.zones.is_empty() || zone.is_some_and(|z| t.zones.iter().any(|id| id == &rules.zones[z].id))
}

/// Root of a component in a union-find stored as a parent map.
fn find_root(parent: &HashMap<u32, u32>, mut c: u32) -> u32 {
    while let Some(&p) = parent.get(&c) {
        if p == c {
            break;
        }
        c = p;
    }
    c
}
