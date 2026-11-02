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

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{
    layer::{BuildingLayer, Rect, Structure},
    rules::{structure_code, DemandSource, Ruleset, Transport},
    Direction, Footprints, GraphicCode, StructureCode, Terrain,
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
}

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
    #[serde(skip)]
    rules: Option<Arc<Ruleset>>,
    #[serde(skip)]
    grid_cache: Option<(u64, Vec<Vec<u32>>)>,
}

const NO_COMPONENT: u32 = u32::MAX;
const BUCKET: u32 = 8;

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
            rules: Some(rules),
            grid_cache: None,
        }
    }

    /// Re-attaches the rules after loading a save.
    pub fn attach(&mut self, rules: Arc<Ruleset>, width: u32, length: u32) {
        let n = (width * length) as usize;
        self.layers.resize(rules.layers.len(), vec![0.0; n]);
        self.demand.resize(rules.zones.len(), 0.0);
        self.rules = Some(rules);
        self.grid_cache = None;
    }

    pub fn rules(&self) -> Option<&Arc<Ruleset>> {
        self.rules.as_ref()
    }

    pub fn layer_value(&self, layer: usize, w: u32, l: u32, width: u32) -> f32 {
        self.layers.get(layer).and_then(|v| v.get((l * width + w) as usize)).copied().unwrap_or(0.0)
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
                    let (sw, sl) = footprints.size(s.graphic);
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
            s.received = vec![0.0; nres];
        }
        self.tick += 1;
        let mut metrics: BTreeMap<String, f32> = BTreeMap::new();
        let mut bal = Balances { produced: vec![0.0; nres], delivered: vec![0.0; nres], unmet: vec![0.0; nres], undelivered: vec![0.0; nres] };

        // 1. Production.
        for (p, u, _) in &units {
            let def = &rules.units[*u];
            let st = self.units.get_mut(&idx(p.0, p.1)).unwrap();
            let eff = st.satisfaction.max(0.25);
            for (m, n) in &def.logs {
                *metrics.entry(m.clone()).or_default() += n * eff;
            }
            for (r, amount) in &def.produce {
                let ri = rules.resource(r).unwrap();
                let made = amount * eff;
                bal.produced[ri] += made;
                match rules.resources[ri].transport {
                    Transport::City => self.treasury += made as f64,
                    Transport::Local => st.stock[ri] = made,
                    Transport::Grid => st.stock[ri] = made,
                    Transport::Agents => st.stock[ri] = (st.stock[ri] + made).min(amount * 3.0),
                }
            }
        }

        // 2. Grids.
        let comps = self.components(&rules, layer, terrain);
        for (ri, res) in rules.resources.iter().enumerate() {
            if res.transport != Transport::Grid {
                continue;
            }
            let net = rules.network(res.network.as_deref().unwrap_or_default()).unwrap();
            let comp_of = |rect: &Rect| unit_component(&comps[net], rect, width, length, terrain);
            let mut supply: HashMap<u32, f32> = HashMap::new();
            let mut demand: HashMap<u32, f32> = HashMap::new();
            for (p, u, r) in &units {
                let Some(c) = comp_of(r) else { continue };
                let st = &self.units[&idx(p.0, p.1)];
                *supply.entry(c).or_default() += st.stock[ri];
                *demand.entry(c).or_default() += rules.units[*u].needs.get(&res.id).copied().unwrap_or(0.0);
            }
            for (p, u, r) in &units {
                let need = rules.units[*u].needs.get(&res.id).copied().unwrap_or(0.0);
                let st = self.units.get_mut(&idx(p.0, p.1)).unwrap();
                let c = comp_of(r);
                if need > 0.0 {
                    let f = c.map_or(0.0, |c| {
                        let d = demand[&c];
                        if d > 0.0 {
                            (supply[&c] / d).min(1.0)
                        } else {
                            0.0
                        }
                    });
                    st.received[ri] += need * f;
                    bal.delivered[ri] += need * f;
                    bal.unmet[ri] += need * (1.0 - f);
                }
                if st.stock[ri] > 0.0 {
                    let used = c.map_or(0.0, |c| (demand[&c] / supply[&c].max(1e-6)).min(1.0));
                    bal.undelivered[ri] += st.stock[ri] * (1.0 - used);
                    st.stock[ri] = 0.0;
                }
            }
        }

        // 3. Agents.
        let mut trips = Vec::new();
        for (ri, res) in rules.resources.iter().enumerate() {
            if res.transport != Transport::Agents {
                continue;
            }
            let net = rules.network(res.network.as_deref().unwrap_or_default()).unwrap();
            let access = |r: &Rect| unit_component(&comps[net], r, width, length, terrain);
            // Senders bucketed by (component, cell).
            let mut buckets: HashMap<(u32, u32, u32), Vec<usize>> = HashMap::new();
            for (i, (p, u, r)) in units.iter().enumerate() {
                if !rules.units[*u].sends.iter().any(|s| s == &res.id) || self.units[&idx(p.0, p.1)].stock[ri] <= 0.0 {
                    continue;
                }
                if let Some(c) = access(r) {
                    buckets.entry((c, p.0 / BUCKET, p.1 / BUCKET)).or_default().push(i);
                }
            }
            let mut receivers: Vec<usize> = (0..units.len()).filter(|&i| rules.units[units[i].1].needs.contains_key(&res.id)).collect();
            receivers.shuffle(rng);
            let reach = res.max_trip.div_ceil(BUCKET) as i64;
            for i in receivers {
                let (p, u, r) = units[i];
                let need = rules.units[u].needs[&res.id];
                let mut missing = need;
                if let Some(c) = access(&r) {
                    // Nearest senders first: rings of buckets.
                    'rings: for ring in 0..=reach {
                        for dz in -ring..=ring {
                            for dx in -ring..=ring {
                                if dx.abs().max(dz.abs()) != ring {
                                    continue;
                                }
                                let (cx, cz) = (p.0 as i64 / BUCKET as i64 + dx, p.1 as i64 / BUCKET as i64 + dz);
                                if cx < 0 || cz < 0 {
                                    continue;
                                }
                                let Some(list) = buckets.get(&(c, cx as u32, cz as u32)) else { continue };
                                for &s in list {
                                    if s == i {
                                        continue;
                                    }
                                    let sp = units[s].0;
                                    if sp.0.abs_diff(p.0) + sp.1.abs_diff(p.1) > res.max_trip {
                                        continue;
                                    }
                                    let stock = &mut self.units.get_mut(&idx(sp.0, sp.1)).unwrap().stock[ri];
                                    let take = stock.min(missing);
                                    if take > 0.0 {
                                        *stock -= take;
                                        missing -= take;
                                        trips.push(Trip { resource: ri, from: sp, to: p, amount: take });
                                        if missing <= 1e-4 {
                                            break 'rings;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                let got = need - missing.max(0.0);
                self.units.get_mut(&idx(p.0, p.1)).unwrap().received[ri] += got;
                bal.delivered[ri] += got;
                bal.unmet[ri] += missing.max(0.0);
            }
            for (p, u, _) in &units {
                if rules.units[*u].sends.iter().any(|s| s == &res.id) {
                    bal.undelivered[ri] += self.units[&idx(p.0, p.1)].stock[ri];
                }
            }
        }

        // 4. Treasury.
        for (p, u, _) in &units {
            for (r, need) in &rules.units[*u].needs {
                let ri = rules.resource(r).unwrap();
                if rules.resources[ri].transport != Transport::City {
                    continue;
                }
                let st = self.units.get_mut(&idx(p.0, p.1)).unwrap();
                if *fund > 0 || self.treasury > *need as f64 {
                    self.treasury -= *need as f64;
                    st.received[ri] = *need;
                    bal.delivered[ri] += need;
                } else {
                    bal.unmet[ri] += need;
                }
            }
        }
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
                let (cx, cz) = ((r.w1 + r.w2) as f32 / 2.0, (r.l1 + r.l2) as f32 / 2.0);
                let rad = e.radius + (r.w2 - r.w1).max(r.l2 - r.l1).div_ceil(2);
                emit(&mut self.layers[li], width, length, cx, cz, e.amount * eff, rad);
            }
        }
        // Agent rules: emissions along the routes of this tick's trips.
        for ar in &rules.agent_rules {
            let wanted = ar.resource.as_deref().and_then(|r| rules.resource(r));
            for t in trips.iter().filter(|t| wanted.is_none_or(|w| w == t.resource)) {
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

        // 6. Satisfaction.
        for (p, u, _) in &units {
            let def = &rules.units[*u];
            let i = idx(p.0, p.1);
            let st = &self.units[&i];
            let mut s = if def.needs.is_empty() {
                1.0
            } else {
                def.needs.iter().map(|(r, n)| (st.received[rules.resource(r).unwrap()] / n.max(1e-6)).min(1.0)).sum::<f32>() / def.needs.len() as f32
            };
            for (lname, b) in &def.requires {
                let v = self.layers[rules.layer(lname).unwrap()][i as usize];
                if b.min.is_some_and(|m| v < m) || b.max.is_some_and(|m| v > m) {
                    s *= 0.5;
                }
            }
            let st = self.units.get_mut(&i).unwrap();
            st.satisfaction = st.satisfaction * 0.7 + s * 0.3;
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
            self.demand[zi] = self.demand[zi] * 0.8 + d * 0.2;
        }
        self.balances = bal;
        self.metric_history.push_back((self.tick, metrics.clone()));
        while self.metric_history.len() > 1000 {
            self.metric_history.pop_front();
        }
        self.metrics = metrics;
        self.grow(&rules, layer, terrain, footprints, rng);
        self.zone_rules(&rules, layer, terrain, footprints, rng);
        trips
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
            let conducts = |w: u32, l: u32| match layer.tile(w, l) {
                None => false,
                Some(_) => {
                    let code = layer.structure_at(w, l).map(|s| s.code);
                    code.is_some_and(|c| codes.contains(&c)) || (net.through_buildings && layer.structure_at(w, l).is_some_and(|s| s.unit.is_some()))
                }
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
                if ok {
                    set_unit(layer, terrain, footprints, rng, w, l, Some(unit), 1, rules);
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
            layer.contains(Rect { w1, l1, w2, l2 }, crate::layer::Query::Code(StructureCode::Road))
        };
        for (zi, z) in rules.zones.iter().enumerate() {
            let Some(code) = structure_code(&z.structure) else { continue };
            // New buildings on empty plots.
            if self.demand[zi] > 0.5 {
                let mut plots: Vec<(u32, u32)> =
                    layer.structures().filter(|(_, s)| s.code == code && s.unit.is_none() && s.level == 0).map(|(p, _)| p).collect();
                plots.shuffle(rng);
                let mut placed = 0;
                for (w, l) in plots {
                    if placed >= g.max_new {
                        break;
                    }
                    let still_plot = layer.tile(w, l).and_then(|t| t.structure()).is_some_and(|s| s.code == code && s.unit.is_none() && s.level == 0);
                    if !still_plot || !road_near(layer, w, l) {
                        continue;
                    }
                    let density = layer.structure_at(w, l).map_or(0, |s| s.density as usize).min(z.densities.len() - 1);
                    let unit = rules.unit(&z.densities[density].units[0]).unwrap();
                    if set_unit(layer, terrain, footprints, rng, w, l, Some(unit), 1, rules) {
                        placed += 1;
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
                let Some(st) = self.units.get(&(p.1 * width + p.0)) else { continue };
                let sat = st.satisfaction;
                let d = &z.densities[(density as usize).min(z.densities.len() - 1)];
                let Some(k) = d.units.iter().position(|id| rules.unit(id) == Some(u as usize)) else { continue };
                if sat > g.level_up && k + 1 < d.units.len() && rng.r#gen::<f32>() < g.chance {
                    let next = rules.unit(&d.units[k + 1]).unwrap();
                    set_unit(layer, terrain, footprints, rng, p.0, p.1, Some(next), k as u8 + 2, rules);
                } else if sat < g.level_down && rng.r#gen::<f32>() < g.chance {
                    if k > 0 {
                        let prev = rules.unit(&d.units[k - 1]).unwrap();
                        set_unit(layer, terrain, footprints, rng, p.0, p.1, Some(prev), k as u8, rules);
                    } else {
                        set_unit(layer, terrain, footprints, rng, p.0, p.1, None, 0, rules);
                        self.units.remove(&(p.1 * width + p.0));
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

/// Network component a unit connects to: any of its tiles or the tiles around it.
fn unit_component(comp: &[u32], r: &Rect, width: u32, length: u32, terrain: &Terrain) -> Option<u32> {
    let _ = terrain;
    let w1 = r.w1.saturating_sub(1);
    let l1 = r.l1.saturating_sub(1);
    let w2 = (r.w2 + 1).min(width - 1);
    let l2 = (r.l2 + 1).min(length - 1);
    (l1..=l2).flat_map(|l| (w1..=w2).map(move |w| (w, l))).map(|(w, l)| comp[(l * width + w) as usize]).find(|&c| c != NO_COMPONENT)
}

impl Structure {
    /// Whether this is a GlassBox unit (developed building or service).
    pub fn is_unit(&self) -> bool {
        self.unit.is_some()
    }
}
