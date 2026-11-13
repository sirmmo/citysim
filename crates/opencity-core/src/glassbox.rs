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
    rules::{structure_code, DemandSource, DisasterEffect, Ruleset, StanceDef, TaxDef, TechDef, Transport},
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
    /// Consecutive ticks with a need or an outlet met by less than half.
    #[serde(default)]
    pub starved: u32,
    /// Its outlets this tick: (resource, made, sent).
    #[serde(skip)]
    pub outlets: Vec<(usize, f32, f32)>,
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
    /// Bought from and sold to neighbouring cities (regions).
    #[serde(default)]
    pub imported: Vec<f32>,
    #[serde(default)]
    pub exported: Vec<f32>,
}

/// A border crossing working as a node of the network (regions): it sends
/// into the city what is bought from the neighbour, by agents from the edge
/// or into the power grid, and takes out what is sold, from the units it
/// can reach along the crossing's network. Amounts are per tick, by
/// resource id.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Gate {
    /// The crossing's edge tile.
    pub at: (u32, u32),
    /// What crosses there: a road kind's id, `power_line` or `metro`.
    pub link: String,
    /// The neighbour's slot in the region, or [`Gate::WORLD`].
    pub partner: [u32; 2],
    /// The airport or seaport (its origin tile) a world gate belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<(u32, u32)>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub import: BTreeMap<String, f32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub export: BTreeMap<String, f32>,
    /// What actually went through last tick, in and out.
    #[serde(skip)]
    pub carried_in: BTreeMap<String, f32>,
    #[serde(skip)]
    pub carried_out: BTreeMap<String, f32>,
}

impl Gate {
    /// The partner of a gate to the rest of the world (an airport or seaport).
    pub const WORLD: [u32; 2] = [u32::MAX; 2];

    pub fn is_world(&self) -> bool {
        self.partner == Self::WORLD
    }
}

/// A disaster that struck: where, and how many buildings it hit.
#[derive(Clone, Debug, PartialEq)]
pub struct DisasterEvent {
    pub id: String,
    pub name: String,
    pub at: (u32, u32),
    pub hit: u32,
    /// The buildings hit burn on (and may yet be saved) instead of being
    /// lost at once.
    pub burning: bool,
}

/// A building in trouble for a while: on fire, under water, its people ill.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affliction {
    /// The disaster's id in the rules.
    pub disaster: String,
    /// The tick it began, and the tick it ends by itself.
    pub since: u64,
    pub until: u64,
}

/// One trip made by agents this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trip {
    pub resource: usize,
    pub from: (u32, u32),
    pub to: (u32, u32),
    pub amount: f32,
    /// The share of `amount` on its way out (`from` → `to`) and on its way
    /// back right now. Resources with a tide follow the clock (over a day
    /// the two add up to 1 on average); the others are 1 and 0.
    pub out: f32,
    pub back: f32,
    /// The share that goes by road vehicle: 1 by road, a little by bus,
    /// 0 by rail, tram, metro or boat.
    pub road: f32,
}

impl Trip {
    /// What is on the move right now, both ways.
    pub fn flow(&self) -> f32 {
        self.amount * (self.out + self.back)
    }
}

/// A probe the city left at a tile: every tick it records what passes and
/// what the maps hold there (see [`GlassBox::readings`]), as series named
/// `Sensor/<name>/…`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sensor {
    pub name: String,
    pub at: (u32, u32),
}

/// The road tiles a trip starts and ends on.
type Ends = ((u32, u32), (u32, u32));

/// Sensors a city can hold.
pub const MAX_SENSORS: usize = 32;

/// Which road sensors each pair of road tiles' route passes (a bit each),
/// kept while the roads and the sensors stay as they are.
#[derive(Clone, Debug, Default)]
struct SensorRoutes {
    revision: u64,
    tiles: Vec<(u32, u32)>,
    passes: HashMap<(u32, u32), u32>,
    doors: HashMap<u32, Option<(u32, u32)>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlassBox {
    pub rules_id: String,
    /// The rules' unit ids in the order the buildings' unit numbers refer
    /// to: a save made before the rules gained, lost or reordered units is
    /// renumbered when the rules are attached (see `City::use_rules`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unit_ids: Vec<String>,
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
    /// Running averages of what is left over (undelivered) and what is
    /// lacking (unmet) per tick, by resource, before any trade: what a
    /// region offers to and asks from the neighbours.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surplus: Vec<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deficit: Vec<f32>,
    /// Trade agreed with the neighbours, through the border crossings:
    /// each is a node that sends what is bought and takes what is sold.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gates: Vec<Gate>,
    /// Traffic on the roads of each cell of `BUCKET` tiles (a running
    /// average): the share of their capacity this tick's road trips take.
    /// 1 is full; trips across busy cells take longer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub congestion: Vec<f32>,
    /// The same at this hour, when traffic has a tide: what the roads show.
    /// How far trips get goes by `congestion`, the day's traffic (a tick
    /// exchanges a day's worth, and people caught in the rush still arrive).
    #[serde(skip)]
    pub rush: Vec<f32>,
    /// Culture flowing in from the rest of the region, per tick and
    /// resource (shared pools).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub culture_in: Vec<f32>,
    /// Money this month by budget line ("Taxes: Property tax", "Upkeep:
    /// School"…): income positive, spending negative.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ledger: BTreeMap<String, f64>,
    /// Funding of each service by unit id (1 = as the rules say; 0.5 half):
    /// it scales the service's upkeep and its coverage.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub funding: BTreeMap<String, f32>,
    /// Disasters switched off by the player.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disasters_off: bool,
    /// Season and weather factors on the odds of disasters that destroy,
    /// flood (near water) and sicken (set by the city each day).
    #[serde(skip)]
    pub climate: Option<[f32; 3]>,
    /// Disasters that struck during the last tick (for notices).
    #[serde(skip)]
    pub events: Vec<DisasterEvent>,
    /// Buildings in trouble (burning, flooded, ill) by tile key, while a
    /// disaster with `lasts` goes on.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub afflictions: BTreeMap<u32, Affliction>,
    /// Buildings lost and saved since the last fire began: (lost, saved).
    /// The city reports and clears it when nothing burns any more.
    #[serde(skip)]
    pub fire_tally: (u32, u32),
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
    /// What last tick left unused of each grid and agent resource, by
    /// (resource, network component): what a building levelling up can
    /// count on.
    #[serde(skip)]
    spare: HashMap<(usize, u32), f32>,
    /// The same anywhere in the city: what the gates could still bring in.
    #[serde(skip)]
    spare_any: Vec<f32>,
    /// Network component of each unit (by origin tile) per resource.
    #[serde(skip)]
    unit_net: HashMap<u32, Vec<u32>>,
    /// The street network (joined by its lines) each road component belongs
    /// to, by (resource, component): the key of `spare` for a plot there.
    #[serde(skip)]
    comp_root: HashMap<(usize, u32), u32>,
    /// For buildings about to level up: what senders within reach had left
    /// of an agent resource, by (origin tile, resource).
    #[serde(skip)]
    within_reach: HashMap<(u32, usize), f32>,
    /// The city could not pay its administration last tick.
    #[serde(skip)]
    pub administration_unpaid: bool,
    /// The hour of the day (0 to 24), set by the city before each tick:
    /// trips of resources with a tide gather around their hours. None: no
    /// clock, traffic is the same at any hour.
    #[serde(skip)]
    pub hour: Option<f32>,
    /// Probes left on the map.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensors: Vec<Sensor>,
    /// What each sensor read last tick, in the order of `sensors`.
    #[serde(skip)]
    pub readings: Vec<Vec<(String, f32)>>,
    #[serde(skip)]
    sensor_routes: SensorRoutes,
}

const NO_COMPONENT: u32 = u32::MAX;
/// Satisfaction left to a unit that gets nothing at all of one of its needs
/// (as a factor; it rises to 1 when half the need is met).
const STARVED: f32 = 0.25;

/// A grid producer: its origin tile and what it supplied this tick.
type Supplier = ((u32, u32), f32);
const BUCKET: u32 = 8;
/// Units this close (in tiles) to a highway can use it: their trips cost
/// this much to get on and off, then highway speed.
const HIGHWAY_RAMP: u32 = 4;
/// How far (in tiles) people walk to a station.
pub const STATION_WALK: u32 = 8;
/// Time (in street tiles) to board and leave a train or a boat.
const STATION_TIME: f32 = 3.0;
/// Boats cover this many tiles in the time a street takes for one.
pub const WATER_SPEED: f32 = 2.0;
/// Trams, in road speed units: their own tracks, right of way at lights.
pub const TRAM_SPEED: f32 = 2.0;
/// Buses, in road speed units: bus lanes, but stops on the way.
pub const BUS_SPEED: f32 = 1.5;
/// Metro trains underground, in road speed units.
pub const METRO_SPEED: f32 = 3.0;
/// Road trips a road tile of capacity 1 carries in a tick before it is full.
pub const ROAD_TRIPS: f32 = 20.0;
/// How much longer a trip across full roads takes: × (1 + this × congestion).
pub const CONGESTION_DELAY: f32 = 1.0;
/// Congestion counted at most (gridlock).
const CONGESTION_MAX: f32 = 2.0;
/// Buses feel this share of the congestion (bus lanes); trams, trains and
/// boats none.
const BUS_CONGESTION: f32 = 0.5;
/// What a bus passenger adds to the traffic, against a car trip.
const BUS_LOAD: f32 = 0.1;

/// Calls `f` with each cell (index) on the straight line between two
/// tiles, once per sample; the cells are `BUCKET` tiles a side, `cw` across.
fn line_cells(a: (u32, u32), b: (u32, u32), cw: u32, mut f: impl FnMut(usize)) {
    let (dx, dz) = (b.0 as f32 - a.0 as f32, b.1 as f32 - a.1 as f32);
    let n = (dx.abs().max(dz.abs()) / BUCKET as f32).ceil().max(1.0) as u32;
    for k in 0..=n {
        let t = k as f32 / n as f32;
        let (x, z) = ((a.0 as f32 + dx * t) as u32, (a.1 as f32 + dz * t) as u32);
        f(((z / BUCKET) * cw + x / BUCKET) as usize);
    }
}

impl GlassBox {
    pub fn new(rules: Arc<Ruleset>, width: u32, length: u32) -> Self {
        let n = (width * length) as usize;
        Self {
            rules_id: rules.id.clone(),
            unit_ids: rules.units.iter().map(|u| u.id.clone()).collect(),
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
            surplus: Vec::new(),
            deficit: Vec::new(),
            gates: Vec::new(),
            congestion: Vec::new(),
            rush: Vec::new(),
            culture_in: Vec::new(),
            tax_rates: BTreeMap::new(),
            disasters_off: false,
            ledger: BTreeMap::new(),
            funding: BTreeMap::new(),
            events: Vec::new(),
            afflictions: BTreeMap::new(),
            fire_tally: (0, 0),
            climate: None,
            stances: BTreeMap::new(),
            discoveries: Vec::new(),
            auto_research: false,
            flows: HashMap::new(),
            churn: [0; 4],
            rules: Some(rules),
            grid_cache: None,
            spare: HashMap::new(),
            spare_any: Vec::new(),
            unit_net: HashMap::new(),
            within_reach: HashMap::new(),
            comp_root: HashMap::new(),
            administration_unpaid: false,
            hour: None,
            sensors: Vec::new(),
            readings: Vec::new(),
            sensor_routes: SensorRoutes::default(),
        }
    }

    /// Re-attaches the rules after loading a save. Returns the unit ids
    /// the save was written with when they differ from the rules' (empty
    /// otherwise, or when the save does not say): the caller renumbers the
    /// buildings.
    pub fn attach(&mut self, rules: Arc<Ruleset>, width: u32, length: u32) -> Vec<String> {
        let ids: Vec<String> = rules.units.iter().map(|u| u.id.clone()).collect();
        let old = std::mem::replace(&mut self.unit_ids, ids);
        let old = if old == self.unit_ids { Vec::new() } else { old };
        let n = (width * length) as usize;
        self.layers.resize(rules.layers.len(), vec![0.0; n]);
        self.demand.resize(rules.zones.len(), 0.0);
        self.pools.resize(rules.resources.len(), 0.0);
        self.rules = Some(rules);
        self.grid_cache = None;
        old
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

    /// What every unit needs of each resource under the policy stances, as
    /// a factor on the written amount (1 unless a stance changes it).
    pub fn need_factors(&self, rules: &Ruleset) -> Vec<f32> {
        let mut f = vec![1.0f32; rules.resources.len()];
        for st in self.active_stances(rules) {
            for (r, v) in &st.needs {
                if let Some(i) = rules.resource(r) {
                    for k in rules.family(i) {
                        f[k] += v;
                    }
                }
            }
        }
        f.iter_mut().for_each(|x| *x = x.max(0.0));
        f
    }

    /// What levelling up from `unit` to `next` at a tile would need more
    /// of: (resource, extra amount, whether last tick left that much spare
    /// on the building's network). Grid and agent resources only.
    pub fn growth_needs(&self, rules: &Ruleset, unit: usize, next: usize, key: u32) -> Vec<(usize, f32, bool)> {
        let factor = self.need_factors(rules);
        let nets = self.unit_net.get(&key);
        rules.units[next]
            .needs
            .iter()
            .filter_map(|(r, n)| {
                let ri = rules.resource(r)?;
                if !matches!(rules.resources[ri].transport, Transport::Grid | Transport::Agents) {
                    return None;
                }
                let extra = (n - rules.units[unit].needs.get(r).copied().unwrap_or(0.0)) * factor[ri];
                let root = rules.root(ri);
                let net = nets.and_then(|v| v.get(root)).and_then(|c| self.spare.get(&(root, *c))).copied().unwrap_or(0.0);
                // Agents only bring what is within a trip of the building.
                let here = self.within_reach.get(&(key, root)).map_or(net, |r| r.min(net));
                let spare = here + self.spare_any.get(root).copied().unwrap_or(0.0);
                (extra > 1e-4).then_some((root, extra, spare >= extra))
            })
            .collect()
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
    /// A service's funding (1 unless the player changed it).
    pub fn funding_of(&self, unit: &str) -> f32 {
        self.funding.get(unit).copied().unwrap_or(1.0)
    }

    /// Sets a service's funding, between 0 and 1.5.
    pub fn set_funding(&mut self, unit: &str, f: f32) {
        let f = if f.is_finite() { f.clamp(0.0, 1.5) } else { 1.0 };
        if (f - 1.0).abs() < 1e-3 {
            self.funding.remove(unit);
        } else {
            self.funding.insert(unit.to_string(), f);
        }
    }

    fn book(&mut self, line: &str, amount: f64) {
        if amount != 0.0 {
            *self.ledger.entry(line.to_string()).or_default() += amount;
        }
    }

    /// Sets off a disaster at a spot (or a likely one). `None` when the
    /// rules have no such disaster or it finds nowhere to strike.
    #[allow(clippy::too_many_arguments)]
    pub fn strike(
        &mut self,
        id: &str,
        at: Option<(u32, u32)>,
        layer: &mut BuildingLayer,
        terrain: &Terrain,
        footprints: &Footprints,
        rng: &mut impl Rng,
    ) -> Option<DisasterEvent> {
        let rules = self.rules.clone()?;
        let d = rules.disaster(id)?.clone();
        let width = layer.width();
        // Developed zone buildings it may strike.
        let low_by_water = |w: u32, l: u32| {
            let (w1, l1, w2, l2) = terrain.around(w, l, 3);
            terrain.tile_max(w, l) <= 1 && (Rect { w1, l1, w2, l2 }).tiles().any(|(x, y)| terrain.is_water(x, y))
        };
        let candidates: Vec<(u32, u32)> =
            layer.structures().filter(|(p, s)| s.unit.is_some() && s.code.is_zone() && (!d.near_water || low_by_water(p.0, p.1))).map(|(p, _)| p).collect();
        let at = match at {
            Some(p) => p,
            None if candidates.is_empty() => return None,
            None => candidates[rng.gen_range(0..candidates.len())],
        };
        let protection = d.protection.as_deref().and_then(|l| rules.layer(l));
        let mut hit = 0;
        for (w, l) in candidates {
            let dist = w.abs_diff(at.0) + l.abs_diff(at.1);
            if dist > d.radius {
                continue;
            }
            let cover = protection.map_or(0.0, |li| self.layers[li][(l * width + w) as usize]);
            let risk = (1.0 - cover / d.full).clamp(0.05, 1.0) * if dist == 0 { 1.0 } else { 0.7 };
            if rng.r#gen::<f32>() >= risk {
                continue;
            }
            let key = l * width + w;
            let Some(s) = layer.tile(w, l).and_then(|t| t.structure()).cloned() else { continue };
            let mark = Affliction { disaster: d.id.clone(), since: self.tick, until: self.tick + d.lasts as u64 };
            match d.effect {
                // A fire that lasts: the building burns, and is lost only if
                // nobody puts it out in time (see `afflictions_tick`).
                DisasterEffect::Destroy if d.lasts > 0 => {
                    if let std::collections::btree_map::Entry::Vacant(e) = self.afflictions.entry(key) {
                        e.insert(mark);
                        hit += 1;
                    }
                }
                DisasterEffect::Destroy => {
                    if self.burn_down(&rules, layer, terrain, footprints, rng, w, l) {
                        hit += 1;
                    }
                }
                DisasterEffect::Damage => {
                    let Some(u) = s.unit else { continue };
                    let zone = rules.zone_for(s.code).and_then(|z| rules.zones[z].densities.get(s.density as usize));
                    let k = zone.and_then(|z| z.units.iter().position(|x| rules.unit(x) == Some(u as usize)));
                    let ok = match k {
                        Some(k) if k > 0 => set_unit(layer, terrain, footprints, rng, w, l, rules.unit(&zone.unwrap().units[k - 1]), k as u8, &rules),
                        _ => set_unit(layer, terrain, footprints, rng, w, l, None, 0, &rules),
                    };
                    if ok {
                        if k.is_none_or(|k| k == 0) {
                            self.units.remove(&key);
                        } else if d.lasts > 0 {
                            self.afflictions.insert(key, mark);
                        }
                        self.churn[3] += 1;
                        hit += 1;
                    }
                }
                DisasterEffect::Sicken => {
                    if let Some(st) = self.units.get_mut(&key) {
                        st.satisfaction = st.satisfaction.min(0.1);
                        if d.lasts > 0 {
                            self.afflictions.insert(key, mark);
                        }
                        hit += 1;
                    }
                }
            }
        }
        let event = DisasterEvent { id: d.id.clone(), name: d.name.clone(), at, hit, burning: d.effect == DisasterEffect::Destroy && d.lasts > 0 };
        self.events.push(event.clone());
        Some(event)
    }

    /// A building lost to a disaster: its plot is empty and rests.
    #[allow(clippy::too_many_arguments)]
    fn burn_down(
        &mut self,
        rules: &Ruleset,
        layer: &mut BuildingLayer,
        terrain: &Terrain,
        footprints: &Footprints,
        rng: &mut impl Rng,
        w: u32,
        l: u32,
    ) -> bool {
        let key = l * layer.width() + w;
        if !set_unit(layer, terrain, footprints, rng, w, l, None, 0, rules) {
            return false;
        }
        self.units.remove(&key);
        let strikes = self.vacated.get(&key).map_or(1, |v| v.strikes.saturating_add(2));
        self.vacated.insert(key, Vacancy { since: self.tick, strikes });
        self.churn[1] += 1;
        true
    }

    /// The disaster a building is in, if any (by its origin tile).
    pub fn affliction(&self, w: u32, l: u32, width: u32) -> Option<&Affliction> {
        self.afflictions.get(&(l * width + w))
    }

    /// Buildings in trouble, one tick on: protection may end it (a fire put
    /// out), it may pass to the buildings next door, and a fire nobody put
    /// out in time takes the building.
    fn afflictions_tick(&mut self, rules: &Ruleset, layer: &mut BuildingLayer, terrain: &Terrain, footprints: &Footprints, rng: &mut impl Rng) {
        if self.afflictions.is_empty() {
            return;
        }
        let width = layer.width();
        let burning: Vec<(u32, Affliction)> = self.afflictions.iter().map(|(k, a)| (*k, a.clone())).collect();
        for (key, a) in burning {
            let (w, l) = (key % width, key / width);
            let standing = layer.tile(w, l).and_then(|t| t.structure()).filter(|s| s.unit.is_some()).cloned();
            let Some(d) = rules.disaster(&a.disaster) else {
                self.afflictions.remove(&key);
                continue;
            };
            let destroys = d.effect == DisasterEffect::Destroy;
            // Gone meanwhile (abandoned, pulled down): a burning one is lost.
            let Some(s) = standing else {
                self.afflictions.remove(&key);
                if destroys {
                    self.fire_tally.0 += 1;
                }
                continue;
            };
            let protection = d.protection.as_deref().and_then(|p| rules.layer(p));
            let cover = |gb: &Self, w: u32, l: u32| protection.map_or(0.0, |li| gb.layers[li][(l * width + w) as usize] / d.full).clamp(0.0, 1.0);
            if d.relief > 0.0 && rng.r#gen::<f32>() < d.relief * cover(self, w, l) {
                self.afflictions.remove(&key);
                if destroys {
                    self.fire_tally.1 += 1;
                }
                continue;
            }
            if self.tick >= a.until {
                self.afflictions.remove(&key);
                if destroys && self.burn_down(rules, layer, terrain, footprints, rng, w, l) {
                    self.fire_tally.0 += 1;
                }
                continue;
            }
            if d.spread <= 0.0 {
                continue;
            }
            // The buildings next door: those touching this one's footprint.
            let (sw, sl) = s.size(footprints);
            let (w1, l1) = (w.saturating_sub(1), l.saturating_sub(1));
            let (w2, l2) = ((w + sw).min(width - 1), (l + sl).min(layer.length() - 1));
            let mut next: Vec<(u32, u32)> = Rect { w1, l1, w2, l2 }.tiles().filter_map(|(x, y)| layer.origin_of(x, y)).filter(|p| *p != (w, l)).collect();
            next.sort_unstable();
            next.dedup();
            for (x, y) in next {
                let k = y * width + x;
                let fuel = layer.tile(x, y).and_then(|t| t.structure()).is_some_and(|n| n.unit.is_some() && n.code.is_zone());
                if !fuel || self.afflictions.contains_key(&k) {
                    continue;
                }
                if rng.r#gen::<f32>() < d.spread * (1.0 - cover(self, x, y)).max(0.05) {
                    if d.effect == DisasterEffect::Sicken {
                        if let Some(st) = self.units.get_mut(&k) {
                            st.satisfaction = st.satisfaction.min(0.1);
                        }
                    }
                    self.afflictions.insert(k, Affliction { disaster: d.id.clone(), since: self.tick, until: self.tick + d.lasts as u64 });
                }
            }
        }
    }

    /// Disasters striking by chance this tick.
    fn chance_disasters(&mut self, rules: &Ruleset, layer: &mut BuildingLayer, terrain: &Terrain, footprints: &Footprints, rng: &mut impl Rng) {
        if self.disasters_off {
            return;
        }
        let climate = self.climate.unwrap_or([1.0; 3]);
        for d in rules.disasters.iter().filter(|d| d.every > 0) {
            let factor = if d.near_water {
                climate[1]
            } else {
                match d.effect {
                    DisasterEffect::Destroy => climate[0],
                    DisasterEffect::Sicken => climate[2],
                    DisasterEffect::Damage => 1.0,
                }
            };
            if rng.r#gen::<f32>() * (d.every as f32) < factor {
                self.strike(&d.id, None, layer, terrain, footprints, rng);
            }
        }
    }

    pub fn tick(&mut self, layer: &mut BuildingLayer, terrain: &Terrain, footprints: &Footprints, fund: &mut i64, rng: &mut impl Rng) -> Vec<Trip> {
        let Some(rules) = self.rules.clone() else { return Vec::new() };
        self.events.clear();
        self.afflictions_tick(&rules, layer, terrain, footprints, rng);
        self.chance_disasters(&rules, layer, terrain, footprints, rng);
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
            s.outlets.clear();
        }
        // Policy stances change what units need (less parking in walkable centres).
        let need_factor = self.need_factors(&rules);
        // What each unit needs this tick: its own needs, plus what its trips
        // of the last tick asked of other networks (resource `uses`).
        let need: Vec<Vec<f32>> = units
            .iter()
            .map(|(p, u, _)| {
                let mut n = vec![0.0; nres];
                // Funding scales what a service costs (its city resources).
                let fund = self.funding_of(&rules.units[*u].id);
                for (r, v) in &rules.units[*u].needs {
                    let ri = rules.resource(r).unwrap();
                    n[ri] += if rules.resources[ri].transport == Transport::City { v * fund } else { *v * need_factor[ri] };
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
        let mut bal =
            Balances { produced: vec![0.0; nres], delivered: vec![0.0; nres], unmet: vec![0.0; nres], undelivered: vec![0.0; nres], ..Default::default() };

        // 1. Production. A resource with kinds is made as its kinds, by share.
        // Finished research adds its bonus to a resource and to its kinds.
        let boost = self.research_bonus(&rules);
        // Budget line of each city resource a unit makes: its tax, or the resource.
        let income_line = |u: usize, k: usize| -> String {
            let zone = rules.zone_of_unit(&rules.units[u].id);
            let r = &rules.resources[k];
            match rules.taxes.iter().find(|t| t.resource == r.id && taxes_zone(t, &rules, zone)) {
                Some(t) => format!("Taxes: {}", t.name),
                None => format!("Income: {}", r.name),
            }
        };
        let mut income: BTreeMap<String, f64> = BTreeMap::new();
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
                        Transport::City => {
                            self.treasury += m as f64;
                            *income.entry(income_line(*u, k)).or_default() += m as f64;
                        }
                        Transport::Pool => self.pools[k] += m,
                        Transport::Local | Transport::Grid => st.stock[k] = m,
                        Transport::Agents => st.stock[k] = (st.stock[k] + m).min(amount * f * 3.0),
                    }
                }
                // An outlet: what was made, and (for now) what is in stock to send.
                if def.outlets.contains_key(r) {
                    let stocked: f32 = rules.family(ri).iter().map(|&k| st.stock[k]).sum();
                    st.outlets.push((ri, made, stocked));
                }
            }
        }

        for (line, v) in income {
            self.book(&line, v);
        }
        // Culture from the rest of the region joins the shared pools.
        for (k, v) in self.culture_in.iter().enumerate() {
            if k < self.pools.len() && *v > 0.0 {
                self.pools[k] += v;
            }
        }

        // 2. Grids. Kinds serve needs for that kind first; what is left fills
        // needs for the resource (slots any kind fills), shared out evenly.
        let mut tick_flows: HashMap<Link, f32> = HashMap::new();
        let comps = self.components(&rules, layer, terrain);
        // What is left unused this tick by (resource, network component),
        // and each unit's component: room for buildings to level up.
        let mut spare: HashMap<(usize, u32), f32> = HashMap::new();
        let mut unit_net: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut within_reach: HashMap<(u32, usize), f32> = HashMap::new();
        let mut comp_root: HashMap<(usize, u32), u32> = HashMap::new();
        // Buildings about to level up, with the unit they would become.
        let g = &rules.growth;
        let candidates: Vec<(usize, usize)> = units
            .iter()
            .enumerate()
            .filter_map(|(i, (p, u, _))| {
                let st = &self.units[&idx(p.0, p.1)];
                (st.thriving + 1 >= g.patience && st.age + 1 >= g.grace).then_some(())?;
                Some((i, rules.next_unit(*u, layer.structure_at(p.0, p.1)?.density)?))
            })
            .collect();
        // What each unit's grid supply left unused (unit, resource): what a
        // gate can sell to the neighbours.
        let mut grid_spare: HashMap<(usize, usize), f32> = HashMap::new();
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
            for (c, pl) in &pools {
                let left: f32 = (0..fam.len()).map(|j| pl.supply[j] * (1.0 - pl.used(j))).sum();
                spare.insert((root, *c), left);
            }
            for (i, (p, _, _)) in units.iter().enumerate() {
                if let Some(c) = comp[i] {
                    unit_net.entry(idx(p.0, p.1)).or_insert_with(|| vec![NO_COMPONENT; nres])[root] = c;
                }
            }
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
                        *grid_spare.entry((i, root)).or_default() += st.stock[k] * (1.0 - used);
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
        // Waterways: harbours on one body of water, the same way.
        let water = crate::rail::Railways::water(layer, terrain, footprints);
        if !water.stations.is_empty() {
            fastest = fastest.max(WATER_SPEED);
        }
        // Tram and bus lines: stops on one tram line, or on one street
        // network, the same way again.
        let tram = crate::rail::Railways::trams(layer, terrain, footprints);
        let bus = crate::rail::Railways::buses(layer, terrain, footprints);
        // Metro lines: stations on one run of tunnels.
        let metro = crate::rail::Railways::metro(layer, terrain, footprints);
        for (l, speed) in [(&tram, TRAM_SPEED), (&bus, BUS_SPEED), (&metro, METRO_SPEED)] {
            if !l.stations.is_empty() {
                fastest = fastest.max(speed);
            }
        }
        let lines = [(&rail, RoadKind::Rail.speed()), (&water, WATER_SPEED), (&tram, TRAM_SPEED), (&bus, BUS_SPEED), (&metro, METRO_SPEED)];
        let fastest = fastest.max(0.1);
        // Congestion: road capacity per cell, last ticks' traffic, and this
        // tick's load from the trips made by road.
        let cw = width.div_ceil(BUCKET);
        let cells = (cw * length.div_ceil(BUCKET)) as usize;
        let mut road_cap = vec![0f32; cells];
        for ((w, l), s) in layer.structures().filter(|(_, s)| s.code == StructureCode::Road && !s.road.is_rail()) {
            road_cap[((l / BUCKET) * cw + w / BUCKET) as usize] += s.road.capacity();
        }
        if self.congestion.len() != cells {
            self.congestion = vec![0.0; cells];
        }
        let congestion = self.congestion.clone();
        let delay = |a: (u32, u32), b: (u32, u32)| -> f32 {
            let (mut sum, mut n) = (0.0, 0);
            line_cells(a, b, cw, |c| {
                sum += congestion[c].min(CONGESTION_MAX);
                n += 1;
            });
            1.0 + CONGESTION_DELAY * sum / n.max(1) as f32
        };
        let mut load = vec![0f32; cells];
        let mut load_now = vec![0f32; cells];
        let mut trips = Vec::new();
        // Tides: by the clock, the share of a resource's trips to a unit
        // that is on its way out and back right now. Staff of units that
        // work round the clock come and go at the shift changes.
        let hour = self.hour;
        let clock: Vec<bool> = units.iter().map(|(p, u, _)| rules.round_the_clock(*u, idx(p.0, p.1))).collect();
        let shifts = rules.shifts.clone().unwrap_or_default();
        let tide = |a: usize, taker: Option<usize>| -> (f32, f32) {
            match (hour, rules.tide(a)) {
                (Some(h), Some(t)) if t.shifts && taker.is_some_and(|i| clock[i]) => shifts.flow(h),
                (Some(h), Some(t)) => t.flow(h),
                _ => (1.0, 0.0),
            }
        };
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
            let station_comp: [Vec<Option<u32>>; 5] = lines.map(|(net_lines, _)| {
                net_lines
                    .stations
                    .iter()
                    .map(|st| {
                        let r = Rect::sized(st.origin.0, st.origin.1, st.size.0, st.size.1);
                        unit_component(&comps[net], &r, width, length, 2, |w, l| road_at(w, l).is_none_or(|k| k.access())).map(|(c, _)| c)
                    })
                    .collect()
            });
            for (k, (net_lines, _)) in lines.iter().enumerate() {
                let mut root_of_line: HashMap<u32, u32> = HashMap::new();
                for (st, c) in net_lines.stations.iter().zip(&station_comp[k]) {
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
            let acc: Vec<Option<(u32, u32, f32, bool, [Option<(usize, u32)>; 5], (u32, u32))>> = access
                .iter()
                .map(|a| {
                    let (c, (w, l)) = (*a)?;
                    let station = [0, 1, 2, 3, 4].map(|k| lines[k].0.nearest_station((w, l), STATION_WALK).filter(|&(i, _)| station_comp[k][i].is_some()));
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
            let road_vehicles: Vec<_> = vehicles.iter().filter(|v| v.door_to_door()).collect();
            let (top, highway) = if vehicles.is_empty() {
                (f32::INFINITY, true)
            } else {
                (road_vehicles.iter().map(|v| v.top_speed()).fold(0.0, f32::max), road_vehicles.iter().any(|v| v.ways().allows(RoadKind::Highway)))
            };
            // Trains carry the resource when the network lists a rail
            // vehicle for it, or lists no vehicles at all.
            let by_rail = vehicles.is_empty() || vehicles.iter().any(|v| v.ways().allows(RoadKind::Rail));
            let by_water = vehicles.is_empty() || vehicles.iter().any(|v| v.ways().sails());
            let on_line = |id: &str| vehicles.is_empty() || vehicles.iter().any(|v| v.line.as_deref() == Some(id));
            let carried = [by_rail, by_water, on_line("tram"), on_line("bus"), on_line("metro")];
            // A trip's length in street tiles and how it goes: `ROAD`,
            // `HIGHWAY` or `LINE + k` (the line kind in `lines`).
            const ROAD: usize = 0;
            const HIGHWAY: usize = 1;
            const LINE: usize = 2;
            let trip_length = |s: usize, i: usize, dist: u32| -> (f32, usize) {
                let (Some(a), Some(b)) = (acc[s], acc[i]) else { return (f32::INFINITY, ROAD) };
                // By road, only within one street network, slowed by traffic.
                let mut best = if a.1 == b.1 { (dist as f32 / a.2.min(b.2).min(top) * delay(a.5, b.5), ROAD) } else { (f32::INFINITY, ROAD) };
                if highway && a.3 && b.3 && a.1 == b.1 {
                    let t = HIGHWAY_RAMP as f32 + dist as f32 / RoadKind::Highway.speed().min(top);
                    if t < best.0 {
                        best = (t, HIGHWAY);
                    }
                }
                for (k, (net_lines, speed)) in lines.iter().enumerate() {
                    if let (true, Some((sa, wa)), Some((sb, wb))) = (carried[k], a.4[k], b.4[k]) {
                        let (x, y) = (&net_lines.stations[sa], &net_lines.stations[sb]);
                        if sa != sb && x.line == y.line {
                            let ride = x.platform.0.abs_diff(y.platform.0) + x.platform.1.abs_diff(y.platform.1);
                            let walk = (wa + wb) as f32 / a.2.min(b.2).min(top).min(1.0);
                            // Buses are held up by part of the traffic.
                            let slow = if k == 3 { 1.0 + (delay(x.platform, y.platform) - 1.0) * BUS_CONGESTION } else { 1.0 };
                            let t = STATION_TIME + walk + ride as f32 / speed * slow;
                            if t < best.0 {
                                best = (t, LINE + k);
                            }
                        }
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
                                        let (dist, how) = trip_length(s, i, sp.0.abs_diff(p.0) + sp.1.abs_diff(p.1));
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
                                            // Road trips load the roads on their way.
                                            let weight = match how {
                                                ROAD => 1.0,
                                                h if h == LINE + 3 => BUS_LOAD,
                                                _ => 0.0,
                                            };
                                            let (out, back) = tide(a, Some(i));
                                            if weight > 0.0 {
                                                line_cells(sp, p, cw, |c| {
                                                    load[c] += take * weight;
                                                    load_now[c] += take * weight * (out + back);
                                                });
                                            }
                                            let road = if how == HIGHWAY { 1.0 } else { weight };
                                            trips.push(Trip { resource: a, from: sp, to: p, amount: take, out, back, road });
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
            // Room to grow. What a sender has to spare is what it has left,
            // up to a tick's production (not what piled up unsent).
            let shares = split(&rules, root);
            let nominal: Vec<Vec<f32>> = rules
                .units
                .iter()
                .map(|def| {
                    let made = |k: usize| def.produce.get(&rules.resources[k].id).copied().unwrap_or(0.0);
                    fam.iter().map(|&k| made(k) + if k != root { made(root) * shares.iter().find(|s| s.0 == k).map_or(0.0, |s| s.1) } else { 0.0 }).collect()
                })
                .collect();
            let spare_of = |stock: &[Vec<f32>], s: usize, j: usize| stock[s][j].min(nominal[units[s].1][j]);
            // For buildings about to level up: what is left within a trip.
            for &(i, next) in &candidates {
                let (p, u, _) = units[i];
                let more = |k: usize| {
                    let id = &rules.resources[k].id;
                    rules.units[next].needs.get(id).copied().unwrap_or(0.0) - rules.units[u].needs.get(id).copied().unwrap_or(0.0)
                };
                let extra: f32 = fam.iter().map(|&k| more(k).max(0.0) * need_factor[k]).sum();
                let Some(c) = acc[i].map(|a| a.0).filter(|_| extra > 1e-4) else { continue };
                let mut found = 0.0;
                if left.iter().any(|&x| x > 0.0) {
                    'probe: for ring in 0..=rings {
                        for dz in -ring..=ring {
                            for dx in -ring..=ring {
                                let (cx, cz) = (p.0 as i64 / BUCKET as i64 + dx, p.1 as i64 / BUCKET as i64 + dz);
                                if dx.abs().max(dz.abs()) != ring || cx < 0 || cz < 0 {
                                    continue;
                                }
                                let Some(bucket) = buckets.get(&(c, cx as u32, cz as u32)).filter(|b| b.left.iter().any(|&x| x > 0.0)) else { continue };
                                for &s in bucket.senders.iter().filter(|&&s| s != i) {
                                    let sp = units[s].0;
                                    let (dist, _) = trip_length(s, i, sp.0.abs_diff(p.0) + sp.1.abs_diff(p.1));
                                    for (j, &k) in fam.iter().enumerate() {
                                        if dist <= rules.resources[k].max_trip as f32 {
                                            found += spare_of(&stock, s, j);
                                        }
                                    }
                                    if found >= extra {
                                        break 'probe;
                                    }
                                }
                            }
                        }
                    }
                }
                within_reach.insert((idx(p.0, p.1), root), found);
            }
            for (i, (p, u, _)) in units.iter().enumerate() {
                let st = self.units.get_mut(&idx(p.0, p.1)).unwrap();
                for (j, &k) in fam.iter().enumerate() {
                    if sends(*u, k) {
                        st.stock[k] = stock[i][j];
                        add(&mut bal.undelivered, k, root, stock[i][j]);
                    }
                }
                if let Some(a) = acc[i] {
                    *spare.entry((root, a.0)).or_default() += (0..fam.len()).map(|j| spare_of(&stock, i, j)).sum::<f32>();
                    unit_net.entry(idx(p.0, p.1)).or_insert_with(|| vec![NO_COMPONENT; nres])[root] = a.0;
                    comp_root.insert((root, a.1), a.0);
                }
            }
        }

        // Trade with neighbouring cities (regions): running averages of what
        // is left over and lacking, then the gates (border crossings) bring
        // in what was bought and take out what was sold, as far as their
        // networks reach.
        for v in [&mut self.surplus, &mut self.deficit] {
            v.resize(nres, 0.0);
        }
        bal.imported = vec![0.0; nres];
        bal.exported = vec![0.0; nres];
        for ri in 0..nres {
            if rules.resources[ri].price > 0.0 {
                self.surplus[ri] = self.surplus[ri] * 0.95 + bal.undelivered[ri] * 0.05;
                self.deficit[ri] = self.deficit[ri] * 0.95 + bal.unmet[ri] * 0.05;
            }
        }
        // Airports and seaports trade with the rest of the world: a gate at
        // each one's door, open to what its rules list (the gate only
        // carries what is lacking or left over).
        let mut gates: Vec<Gate> = std::mem::take(&mut self.gates).into_iter().filter(|g| !g.is_world()).collect();
        for (p, u, r) in &units {
            let def = &rules.units[*u];
            if def.trade.is_empty() {
                continue;
            }
            let mut gate = Gate { at: *p, link: "world".into(), partner: Gate::WORLD, owner: Some(*p), ..Default::default() };
            for (rid, &cap) in &def.trade {
                let Some(ri) = rules.resource(rid) else { continue };
                let Some(net) = rules.network(rules.resources[ri].network.as_deref().unwrap_or_default()) else { continue };
                if let Some((_, door)) = unit_component(&comps[net], r, width, length, rules.growth.road_access.max(1), |_, _| true) {
                    gate.at = door;
                }
                gate.import.insert(rid.clone(), cap);
                gate.export.insert(rid.clone(), cap);
            }
            if !gate.import.is_empty() {
                gates.push(gate);
            }
        }
        for gate in &mut gates {
            gate.carried_in.clear();
            gate.carried_out.clear();
            let wanted: BTreeSet<&String> = gate.import.keys().chain(gate.export.keys()).collect();
            for rid in wanted {
                let Some(ri) = rules.resource(rid) else { continue };
                let res = &rules.resources[ri];
                let Some(net) = rules.network(res.network.as_deref().unwrap_or_default()) else { continue };
                let grid = res.transport == Transport::Grid;
                // Units the gate reaches and how long the trip is, nearest
                // first: along its network (slowed by traffic), or from the
                // stops of a line through the gate tile (track, tram street,
                // bus route, metro tunnel). People and goods go no further
                // than `max_trip` to the border; power has no such limit.
                let gi = idx(gate.at.0, gate.at.1) as usize;
                let comp = comps[net][gi];
                let vehicles = rules.vehicles_for(rules.root(ri));
                let rides = |k: usize| {
                    vehicles.is_empty()
                        || match k {
                            0 => vehicles.iter().any(|v| v.ways().allows(RoadKind::Rail)),
                            1 => vehicles.iter().any(|v| v.ways().sails()),
                            2 => vehicles.iter().any(|v| v.line.as_deref() == Some("tram")),
                            3 => vehicles.iter().any(|v| v.line.as_deref() == Some("bus")),
                            _ => vehicles.iter().any(|v| v.line.as_deref() == Some("metro")),
                        }
                };
                let max_trip = if grid { f32::INFINITY } else { res.max_trip as f32 };
                let mut reach: Vec<(u32, usize)> = Vec::new();
                for (i, (p, _, r)) in units.iter().enumerate() {
                    let by_net = (comp != NO_COMPONENT)
                        .then(|| unit_component(&comps[net], r, width, length, if grid { 1 } else { rules.growth.road_access.max(1) }, |_, _| true))
                        .flatten()
                        .filter(|(c, _)| *c == comp)
                        .map(|(_, (w, l))| {
                            let d = (w.abs_diff(gate.at.0) + l.abs_diff(gate.at.1)) as f32;
                            if grid {
                                d
                            } else {
                                d / road_at(w, l).map_or(1.0, |k| k.speed()) * delay((w, l), gate.at)
                            }
                        });
                    let by_line = (!grid)
                        .then(|| {
                            lines
                                .iter()
                                .enumerate()
                                .filter(|&(k, (l, _))| k != 1 && rides(k) && l.line.get(gi).is_some_and(|&ln| ln != crate::rail::NO_LINE))
                                .filter_map(|(k, (l, speed))| {
                                    let (s, walk) = l.nearest_station(*p, STATION_WALK)?;
                                    let st = &l.stations[s];
                                    let ride = (st.platform.0.abs_diff(gate.at.0) + st.platform.1.abs_diff(gate.at.1)) as f32;
                                    let slow = if k == 3 { 1.0 + (delay(st.platform, gate.at) - 1.0) * BUS_CONGESTION } else { 1.0 };
                                    (st.line == l.line[gi]).then_some(STATION_TIME + walk as f32 + ride / speed * slow)
                                })
                                .fold(f32::INFINITY, f32::min)
                        })
                        .filter(|d| d.is_finite());
                    if let Some(d) = by_net.into_iter().chain(by_line).fold(None, |m: Option<f32>, d| Some(m.map_or(d, |m| m.min(d)))) {
                        if d <= max_trip {
                            reach.push(((d * 16.0) as u32, i));
                        }
                    }
                }
                reach.sort();
                // In: fill unmet needs, nearest first.
                let mut budget = gate.import.get(rid).copied().unwrap_or(0.0).min(bal.unmet[ri]);
                for &(_, i) in &reach {
                    if budget <= 1e-4 {
                        break;
                    }
                    let p = units[i].0;
                    let st = self.units.get_mut(&idx(p.0, p.1)).unwrap();
                    let take = (need[i][ri] - st.received[ri]).max(0.0).min(budget);
                    if take <= 0.0 {
                        continue;
                    }
                    st.received[ri] += take;
                    budget -= take;
                    bal.imported[ri] += take;
                    *gate.carried_in.entry(rid.clone()).or_default() += take;
                    if !grid {
                        let (out, back) = tide(ri, Some(i));
                        trips.push(Trip { resource: ri, from: gate.at, to: p, amount: take, out, back, road: 1.0 });
                    }
                    *tick_flows.entry(Link { resource: ri, from: gate.at, to: p }).or_default() += take;
                }
                // Out: take what had nowhere to go, nearest first.
                let mut room = gate.export.get(rid).copied().unwrap_or(0.0).min(bal.undelivered[ri] - bal.exported[ri]);
                for &(_, i) in &reach {
                    if room <= 1e-4 {
                        break;
                    }
                    let p = units[i].0;
                    let spare = if grid {
                        grid_spare.get_mut(&(i, ri))
                    } else {
                        self.units.get_mut(&idx(p.0, p.1)).map(|st| &mut st.stock[ri]).filter(|_| rules.units[units[i].1].sends.contains(&res.id))
                    };
                    let Some(spare) = spare else { continue };
                    let take = spare.min(room);
                    if take <= 0.0 {
                        continue;
                    }
                    *spare -= take;
                    room -= take;
                    bal.exported[ri] += take;
                    *gate.carried_out.entry(rid.clone()).or_default() += take;
                    if !grid {
                        let (out, back) = tide(ri, None);
                        trips.push(Trip { resource: ri, from: p, to: gate.at, amount: take, out, back, road: 1.0 });
                    }
                    *tick_flows.entry(Link { resource: ri, from: p, to: gate.at }).or_default() += take;
                }
            }
        }
        let gate_tiles: BTreeSet<u32> = gates.iter().map(|g| idx(g.at.0, g.at.1)).collect();
        // What the gates could still bring in counts as spare everywhere.
        let mut spare_any = vec![0f32; nres];
        for g in &gates {
            for (rid, cap) in &g.import {
                if let Some(ri) = rules.resource(rid) {
                    spare_any[rules.root(ri)] += (cap - g.carried_in.get(rid).copied().unwrap_or(0.0)).max(0.0);
                }
            }
        }
        for (p, _, _) in &units {
            let st = self.units.get_mut(&idx(p.0, p.1)).unwrap();
            // Outlets: what left the building this tick, to takers or abroad.
            let stock = &st.stock;
            for o in st.outlets.iter_mut() {
                let now: f32 = rules.family(o.0).iter().map(|&k| stock[k]).sum();
                o.2 = (o.2 - now).max(0.0);
            }
        }
        self.spare = spare;
        self.spare_any = spare_any;
        self.unit_net = unit_net;
        self.within_reach = within_reach;
        self.comp_root = comp_root;
        self.gates = gates;
        for ri in 0..nres {
            if bal.imported[ri] + bal.exported[ri] <= 0.0 {
                continue;
            }
            bal.unmet[ri] -= bal.imported[ri];
            bal.undelivered[ri] -= bal.exported[ri];
            self.treasury += ((bal.exported[ri] - bal.imported[ri]) * rules.resources[ri].price) as f64;
            self.book("Trade: sales", (bal.exported[ri] * rules.resources[ri].price) as f64);
            self.book("Trade: purchases", -(bal.imported[ri] * rules.resources[ri].price) as f64);
        }

        // This tick's traffic joins the running average, and road tiles
        // show it (inspector, vehicles avoiding busy roads).
        if self.rush.len() != cells {
            self.rush = self.congestion.clone();
        }
        for c in 0..cells {
            let share = |load: &[f32]| if road_cap[c] > 0.0 { load[c] / (road_cap[c] * ROAD_TRIPS) } else { 0.0 };
            self.congestion[c] = self.congestion[c] * 0.8 + share(&load) * 0.2;
            self.rush[c] = self.rush[c] * 0.5 + share(&load_now) * 0.5;
        }
        // Roads show the traffic of the hour, when there is a clock.
        let shown = if hour.is_some() { &self.rush } else { &self.congestion };
        let roads: Vec<(u32, u32)> = layer.structures().filter(|(_, s)| s.code == StructureCode::Road && !s.road.is_rail()).map(|(p, _)| p).collect();
        for (w, l) in roads {
            let t = (shown[((l / BUCKET) * cw + w / BUCKET) as usize] * 100.0).min(255.0) as u8;
            if let Some(crate::layer::TileKind::Main(s)) = layer.tile_mut(w, l).map(|t| &mut t.kind) {
                s.traffic = t;
            }
        }

        // The network view: links fade unless used again.
        for v in self.flows.values_mut() {
            *v *= 1.0 - FLOW_SMOOTHING;
        }
        for (k, v) in tick_flows {
            *self.flows.entry(k).or_default() += v * FLOW_SMOOTHING;
        }
        let known = |t: (u32, u32)| present.contains(&idx(t.0, t.1)) || gate_tiles.contains(&idx(t.0, t.1));
        self.flows.retain(|k, v| *v > 0.01 && known(k.from) && known(k.to));

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
                    let line = format!("Upkeep: {}", rules.units[units[i].1].name);
                    *self.ledger.entry(line).or_default() -= n as f64;
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
        // Running the city: so much a head, more per head as it grows.
        self.administration_unpaid = false;
        if let Some(a) = &rules.administration {
            let people = rules.population.as_deref().and_then(|p| rules.resource(p)).map_or(0.0, |ri| self.local_total(ri));
            let cost = a.cost_for(people) as f64;
            if cost > 0.0 {
                if *fund > 0 || self.treasury > cost {
                    self.treasury -= cost;
                    self.book(&format!("Upkeep: {}", a.name), -cost);
                } else {
                    self.administration_unpaid = true;
                }
            }
        }
        self.advance_research(&rules);
        // Running costs of policies (negative ones earn).
        self.treasury -= self.active_stances(&rules).map(|st| st.cost as f64).sum::<f64>();
        let costs: Vec<(String, f64)> =
            self.active_stances(&rules).filter(|st| st.cost != 0.0).map(|st| (format!("Policy: {}", st.name), -st.cost as f64)).collect();
        for (line, v) in costs {
            self.book(&line, v);
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
            let eff = self.units[&idx(p.0, p.1)].satisfaction.max(0.25) * self.funding_of(&rules.units[*u].id);
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
                            emit(&mut self.layers[li], width, length, x as f32, z as f32, a.amount * t.flow(), a.radius);
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
            let lis: Vec<(usize, f32)> = trail.iter().map(|(l, a)| (rules.layer(l).unwrap(), a * t.flow())).collect();
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
            // Each need counts by its size and its resource's weight; an
            // outlet counts like a need for takers of what was made, times
            // how much it matters. One met by less than half starves the
            // unit: it pulls the whole down, to a quarter when nothing of
            // it comes at all.
            let starve = |share: f32| (STARVED + (1.0 - STARVED) * share / 0.5).min(1.0);
            let (mut met, mut total, mut pinch) = (0.0f32, 0.0f32, 1.0f32);
            for (ri, v) in need[n].iter().copied().enumerate().filter(|(_, v)| *v > 0.0) {
                let share = (st.received[ri] / v.max(1e-6)).min(1.0);
                let w = v * rules.resources[ri].weight;
                met += share * w;
                total += w;
                // Only its own needs can starve it, not those of its trips.
                if def.needs.contains_key(&rules.resources[ri].id) {
                    pinch *= starve(share);
                }
            }
            for &(ri, made, sent) in &st.outlets {
                let id = &rules.resources[ri].id;
                let matters = def.outlets.get(id).copied().unwrap_or(1.0);
                if made > 1e-6 {
                    let share = (sent / made).min(1.0);
                    let w = def.produce.get(id).copied().unwrap_or(made) * rules.resources[ri].weight * matters;
                    met += share * w;
                    total += w;
                    pinch *= 1.0 - matters.min(1.0) * (1.0 - starve(share));
                }
            }
            let starving = pinch < 1.0;
            let mut s = if total > 0.0 { met / total * pinch } else { 1.0 };
            for (lname, b) in &def.requires {
                let v = self.layers[rules.layer(lname).unwrap()][i as usize];
                if b.min.is_some_and(|m| v < m) || b.max.is_some_and(|m| v > m) {
                    s *= 0.5;
                }
            }
            // Policies and taxes above (or below) their default rate.
            s += mood;
            for t in rules.taxes.iter().filter(|t| taxes_zone(t, &rules, unit_zone[*u]) && def.produce.contains_key(&t.resource)) {
                s += t.mood(self.tax_rate(&rules, &t.id));
            }
            // A city that cannot pay its clerks is badly run.
            if self.administration_unpaid && unit_zone[*u].is_some() {
                s -= rules.administration.as_ref().map_or(0.0, |a| a.unpaid);
            }
            let s = s.clamp(0.0, 1.0);
            let g = &rules.growth;
            let st = self.units.get_mut(&i).unwrap();
            st.satisfaction = st.satisfaction * 0.7 + s * 0.3;
            st.age = st.age.saturating_add(1);
            st.starved = if starving { st.starved.saturating_add(1) } else { 0 };
            st.thriving = if st.satisfaction > g.level_up && !starving { st.thriving.saturating_add(1) } else { 0 };
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
        self.read_sensors(&rules, layer, terrain, &trips);
        self.record(&rules, layer, *fund);
        trips
    }

    /// Leaves a sensor at a tile. Returns its name: the one given, or
    /// `Sensor <n>`.
    pub fn add_sensor(&mut self, name: Option<&str>, at: (u32, u32)) -> Result<String, String> {
        if self.sensors.len() >= MAX_SENSORS {
            return Err(format!("A city holds at most {MAX_SENSORS} sensors."));
        }
        let clean = |n: &str| n.trim().replace(['/', ',', '"'], " ");
        let name = match name.map(clean).filter(|n| !n.is_empty()) {
            Some(n) if self.sensors.iter().any(|s| s.name == n) => return Err(format!("There is already a sensor called {n}.")),
            Some(n) => n,
            None => (1..).map(|k| format!("Sensor {k}")).find(|n| self.sensors.iter().all(|s| &s.name != n)).unwrap(),
        };
        self.sensors.push(Sensor { name: name.clone(), at });
        Ok(name)
    }

    pub fn remove_sensor(&mut self, name: &str) -> bool {
        let n = self.sensors.len();
        self.sensors.retain(|s| s.name != name);
        self.readings.clear();
        self.sensors.len() != n
    }

    /// What a sensor recorded (the ticks kept in [`Self::series`]) as CSV:
    /// `tick`, `hour` when the city has a clock, then its readings.
    pub fn sensor_csv(&self, name: &str) -> Option<String> {
        let prefix = format!("Sensor/{name}/");
        let cols: Vec<usize> = (0..self.series.names.len()).filter(|&i| self.series.names[i].starts_with(&prefix)).collect();
        if cols.is_empty() {
            return None;
        }
        let hour = self.series.column("City/Hour");
        let mut out = String::from("tick");
        if hour.is_some() {
            out.push_str(",hour");
        }
        for &i in &cols {
            out.push(',');
            out.push_str(&self.series.names[i][prefix.len()..]);
        }
        out.push('\n');
        for (row, tick) in self.series.ticks.iter().enumerate() {
            if cols.iter().all(|&i| !self.series.columns[i][row].is_finite()) {
                continue;
            }
            out.push_str(&tick.to_string());
            let mut put = |v: f32, digits: usize| {
                out.push(',');
                if v.is_finite() {
                    out.push_str(&format!("{v:.digits$}"));
                }
            };
            if let Some(h) = hour {
                put(h[row], 2);
            }
            for &i in &cols {
                put(self.series.columns[i][row], 3);
            }
            out.push('\n');
        }
        Some(out)
    }

    /// What each sensor sees this tick. On a road: the trips driving past
    /// (in units carried), in all and by resource, along the quickest road
    /// route between their ends. On a building: what arrives and leaves,
    /// and its satisfaction. Everywhere: the maps at the tile.
    fn read_sensors(&mut self, rules: &Ruleset, layer: &BuildingLayer, terrain: &Terrain, trips: &[Trip]) {
        self.readings.clear();
        if self.sensors.is_empty() {
            return;
        }
        let width = layer.width();
        let nres = rules.resources.len();
        let is_road = |(w, l): (u32, u32)| layer.tile(w, l).and_then(|t| t.structure()).is_some_and(|s| s.code == StructureCode::Road && !s.road.is_rail());
        let inside = |(w, l): (u32, u32)| w < width && l < layer.length();
        // Road sensors take a bit each; the others watch a building, if any.
        let road_tiles: Vec<(u32, u32)> = self.sensors.iter().map(|s| s.at).filter(|&p| inside(p) && is_road(p)).collect();
        let mut passing = vec![vec![0f32; nres]; road_tiles.len()];
        if !road_tiles.is_empty() {
            let cache = &mut self.sensor_routes;
            if cache.revision != layer.revision() || cache.tiles != road_tiles {
                *cache = SensorRoutes { revision: layer.revision(), tiles: road_tiles.clone(), ..Default::default() };
            }
            // The road tile each end of a trip opens onto.
            let mut door = |p: (u32, u32)| -> Option<(u32, u32)> {
                *cache.doors.entry(p.1 * width + p.0).or_insert_with(|| {
                    (0..=5u32).find_map(|r| {
                        let (w1, l1, w2, l2) = terrain.around(p.0, p.1, r);
                        (Rect { w1, l1, w2, l2 }).tiles().filter(|&t| is_road(t)).min_by_key(|t| t.0.abs_diff(p.0) + t.1.abs_diff(p.1))
                    })
                })
            };
            let ends: Vec<Option<Ends>> = trips
                .iter()
                .map(|t| {
                    (t.road > 0.0 && t.flow() > 0.0).then_some(())?;
                    let (a, b) = (door(t.from)?, door(t.to)?);
                    Some((a.min(b), a.max(b)))
                })
                .collect();
            // Routes not met before, by where they start.
            let mut missing: BTreeMap<(u32, u32), BTreeSet<(u32, u32)>> = BTreeMap::new();
            for (a, b) in ends.iter().flatten() {
                if !cache.passes.contains_key(&(a.1 * width + a.0, b.1 * width + b.0)) {
                    missing.entry(*a).or_default().insert(*b);
                }
            }
            for (a, targets) in missing {
                let targets: Vec<(u32, u32)> = targets.into_iter().collect();
                let routes = crate::traffic::find_routes(layer, terrain, a, &targets);
                for (b, route) in targets.iter().zip(routes) {
                    let mask = route.iter().flatten().fold(0u32, |m, t| m | road_tiles.iter().position(|s| s == t).map_or(0, |k| 1 << k));
                    cache.passes.insert((a.1 * width + a.0, b.1 * width + b.0), mask);
                }
            }
            for (t, e) in trips.iter().zip(&ends) {
                let Some((a, b)) = e else { continue };
                let mask = cache.passes[&(a.1 * width + a.0, b.1 * width + b.0)];
                for (k, p) in passing.iter_mut().enumerate() {
                    if mask & (1 << k) != 0 {
                        p[rules.root(t.resource)] += t.flow() * t.road;
                    }
                }
            }
        }
        let mut readings = Vec::with_capacity(self.sensors.len());
        for s in &self.sensors {
            let mut v: Vec<(String, f32)> = Vec::new();
            if !inside(s.at) {
                readings.push(v);
                continue;
            }
            if let Some(k) = road_tiles.iter().position(|p| *p == s.at) {
                v.push(("Trips".into(), passing[k].iter().sum()));
                for (ri, r) in rules.resources.iter().enumerate() {
                    if r.transport == Transport::Agents && r.parent.is_none() {
                        v.push((r.name.clone(), passing[k][ri]));
                    }
                }
            } else if let Some(o) =
                layer.origin_of(s.at.0, s.at.1).filter(|o| layer.tile(o.0, o.1).and_then(|t| t.structure()).is_some_and(|st| st.unit.is_some()))
            {
                let (mut arriving, mut leaving) = (0f32, 0f32);
                for t in trips {
                    if t.to == o {
                        arriving += t.amount * t.out;
                        leaving += t.amount * t.back;
                    }
                    if t.from == o {
                        leaving += t.amount * t.out;
                        arriving += t.amount * t.back;
                    }
                }
                v.push(("Arriving".into(), arriving));
                v.push(("Leaving".into(), leaving));
                if let Some(st) = self.units.get(&(o.1 * width + o.0)) {
                    v.push(("Satisfaction".into(), st.satisfaction));
                }
            }
            let i = (s.at.1 * width + s.at.0) as usize;
            for (def, values) in rules.layers.iter().zip(&self.layers) {
                v.push((def.name.clone(), values.get(i).copied().unwrap_or(0.0)));
            }
            readings.push(v);
        }
        self.readings = readings;
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
        if let Some(h) = self.hour {
            v.push(("City/Hour".into(), h));
        }
        for (s, r) in self.sensors.iter().zip(&self.readings) {
            for (k, x) in r {
                v.push((format!("Sensor/{}/{k}", s.name), *x));
            }
        }
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
            let conducts = |w: u32, l: u32| conducts(net, layer, w, l);
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
                // A plot whose building was abandoned rests here too.
                if self.vacated.get(&(l * width + w)).is_some_and(|v| self.tick < v.until(rules.growth.grace)) {
                    continue;
                }
                let ok = zr.tests.iter().all(|t| {
                    let v = match t.scope {
                        crate::rules::Scope::Map => rules.layer(&t.id).map_or(0.0, |li| self.layers[li][(l * width + w) as usize]),
                        // `demand.<zone>`: that zone's demand.
                        crate::rules::Scope::Global => match t.id.strip_prefix("demand.").and_then(|z| rules.zones.iter().position(|x| x.id == z)) {
                            Some(zi) => self.demand[zi],
                            None => rules.resource(&t.id).map(|ri| self.balances.produced[ri]).or_else(|| self.metrics.get(&t.id).copied()).unwrap_or(0.0),
                        },
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
        let (width, length) = (layer.width(), layer.length());
        let comps = self.components(rules, layer, terrain);
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
            // What pulls the zone in: resources left with nowhere to go
            // (shoppers and goods for shops). A zone that lives off them
            // alone (no demand of its own) develops a plot only where some
            // is left on its street network: shops no customer can reach
            // would only close again.
            let pulls: Vec<(usize, usize)> = z
                .demand
                .iter()
                .filter(|t| t.from == DemandSource::Undelivered && t.weight > 0.0 && z.base_demand <= 0.5)
                .filter_map(|t| {
                    let ri = rules.resource(&t.resource)?;
                    let res = &rules.resources[ri];
                    (res.transport == Transport::Agents).then_some((rules.root(ri), rules.network(res.network.as_deref()?)?))
                })
                .collect();
            let pulled = |this: &Self, w: u32, l: u32| {
                pulls.is_empty()
                    || pulls.iter().any(|&(root, net)| {
                        unit_component(&comps[net], &Rect::sized(w, l, 1, 1), width, length, g.road_access.max(1), |_, _| true)
                            .and_then(|(c, _)| this.comp_root.get(&(root, c)))
                            .and_then(|m| this.spare.get(&(root, *m)))
                            .is_some_and(|v| *v > 0.01)
                    })
            };
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
                    if !still_plot || resting || !road_near(layer, w, l) || !pulled(self, w, l) {
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
                let d = &z.densities[(density as usize).min(z.densities.len() - 1)];
                let Some(k) = d.units.iter().position(|id| rules.unit(id) == Some(u as usize)) else { continue };
                // Long neglect shrinks a building, but only low satisfaction
                // empties a plot. Buildings grow only while their zone is
                // in demand (people want to move in).
                let neglected = g.neglect > 0 && st.starved >= g.neglect && k > 0;
                let (up, down) = (st.thriving >= g.patience && self.demand[zi] > 0.5, st.struggling >= g.patience || neglected);
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
                // Nor into one that needs more than its networks have to
                // spare (no water left, no workers looking for a job).
                let extra = if up && next_fits { self.growth_needs(rules, u as usize, rules.unit(&d.units[k + 1]).unwrap(), key) } else { Vec::new() };
                let supplied = extra.iter().all(|e| e.2);
                let changed = if up && next_fits && supplied && rng.r#gen::<f32>() < g.chance {
                    let next = rules.unit(&d.units[k + 1]).unwrap();
                    let ok = set_unit(layer, terrain, footprints, rng, p.0, p.1, Some(next), k as u8 + 2, rules);
                    self.churn[2] += ok as u32;
                    if ok {
                        // What it will take is no longer spare for the next one.
                        for (ri, amount, _) in extra {
                            let here = self.unit_net.get(&key).and_then(|v| v.get(ri)).copied().and_then(|c| self.spare.get_mut(&(ri, c)));
                            let mut rest = amount;
                            if let Some(v) = here {
                                let take = v.min(rest);
                                *v -= take;
                                rest -= take;
                            }
                            if let Some(v) = self.spare_any.get_mut(ri) {
                                *v = (*v - rest).max(0.0);
                            }
                        }
                    }
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
                        st.starved = 0;
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
    // From there it carries on across zoned land, built on or still empty,
    // a tile at a time: the back row of a deep block is covered through
    // the plots in front of it.
    let zoned = |i: usize| layer.structure_at(i as u32 % width, i as u32 / width).is_some_and(|s| s.code.is_zone());
    let step = amount / rad;
    let mut heap: std::collections::BinaryHeap<(u32, usize)> = got.iter().filter(|(i, _)| zoned(**i)).map(|(i, v)| ((v * 1000.0) as u32, *i)).collect();
    while let Some((v, i)) = heap.pop() {
        let v = v as f32 / 1000.0;
        if got.get(&i).is_some_and(|&g| g > v + 1e-3) || v <= step {
            continue;
        }
        for d in Direction::ALL {
            let Some((nw, nl)) = terrain.neighbour(i as u32 % width, i as u32 / width, d) else { continue };
            let j = (nl * width + nw) as usize;
            if zoned(j) && got.get(&j).is_none_or(|&g| g < v - step - 1e-3) {
                got.insert(j, v - step);
                heap.push((((v - step) * 1000.0) as u32, j));
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
/// Whether a tile is part of a network: one of its structures (and the
/// zones it lists), track only for rail networks (streets keep their level
/// crossings), and any building when the network runs through buildings.
pub fn conducts(net: &crate::rules::NetworkDef, layer: &BuildingLayer, w: u32, l: u32) -> bool {
    let has = |code: StructureCode| net.structures.iter().any(|s| structure_code(s) == Some(code));
    if layer.has_pipe(w, l) && has(StructureCode::WaterPipe) {
        return true;
    }
    match layer.structure_at(w, l) {
        None => false,
        // Track belongs to rail networks, not to the streets.
        Some(s) if s.code == StructureCode::Road && s.road.is_rail() => {
            net.structures.iter().any(|s| s == "rail") || (has(StructureCode::Road) && layer.is_level_crossing(w, l))
        }
        // Buildings pass it on, and so does zoned land not built on yet.
        Some(s) => has(s.code) || (net.through_buildings && (s.unit.is_some() || s.code.is_zone())),
    }
}

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
