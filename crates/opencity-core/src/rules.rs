//! Data-driven city rules, in the spirit of SimCity's GlassBox engine.
//!
//! Everything the simulation knows is declared here and loaded from a rules
//! file (`assets/rules/*.toml`):
//!
//! * **Resources**: what flows through the city (workers, goods, power…), and
//!   how it travels: carried by agents along a network, propagated through a
//!   grid, paid into the city treasury, or kept by the building.
//! * **Map layers**: fields over the map (pollution, land value, coverage of
//!   services) with diffusion and decay.
//! * **Networks**: which structures form them and which resources they move.
//! * **Units**: buildings. Each produces some resources, needs others, sends
//!   its stock to buildings that need it, emits into map layers, and may
//!   require layer values to thrive.
//! * **Zones**: zone types with density subtypes; each density lists the
//!   units that grow on it, level by level. Zone demand is computed from
//!   resource balances.
//! * **Services**: which unit a service tool builds.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::StructureCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// Carried by agents along a network (workers, shoppers, goods).
    Agents,
    /// Shared instantly by everything connected to a network (power, water).
    Grid,
    /// Paid into or out of the city treasury (taxes, upkeep).
    City,
    /// Stays in the building (residents, statistics).
    Local,
    /// Gathered city-wide into a stock that never travels and keeps from
    /// tick to tick (ideas, research points). Needs draw on the stock, and
    /// research spends it.
    Pool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceDef {
    pub id: String,
    pub name: String,
    pub transport: Transport,
    /// Network carrying it (for agents and grid transport).
    #[serde(default)]
    pub network: Option<String>,
    /// Longest trip in tiles for agent transport.
    #[serde(default = "default_trip")]
    pub max_trip: u32,
    /// Colour of its links on the network view (RGB, 0..1).
    #[serde(default)]
    pub colour: Option<[f32; 3]>,
    /// Kinds of this resource (`car` → `car.gasoline`, `car.electric`).
    /// A need for the resource is a slot any kind fills; a need for a kind
    /// takes only that kind. Producing the resource makes its kinds by `share`.
    #[serde(default, rename = "subtype", skip_serializing_if = "Vec::is_empty")]
    pub subtypes: Vec<SubtypeDef>,
    /// For a kind: the resource it is a kind of (set when the rules are read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// For a kind: its share of what is produced as the parent resource.
    #[serde(default = "default_share")]
    pub share: f32,
    /// Secondary impact: other resources needed per unit moved, asked of
    /// the sender the next tick (electric cars draw power, petrol cars fuel).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub uses: BTreeMap<String, f32>,
    /// Secondary impact: added to map layers per unit moved and tile travelled.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub trail: BTreeMap<String, f32>,
}

/// A kind of resource, declared inside it as `[[resource.subtype]]`.
/// It moves like its parent (same transport and network).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubtypeDef {
    /// Short id; the resource id becomes `<parent>.<id>`.
    pub id: String,
    pub name: String,
    #[serde(default = "default_share")]
    pub share: f32,
    #[serde(default)]
    pub max_trip: Option<u32>,
    #[serde(default)]
    pub colour: Option<[f32; 3]>,
    #[serde(default)]
    pub uses: BTreeMap<String, f32>,
    #[serde(default)]
    pub trail: BTreeMap<String, f32>,
}

fn default_trip() -> u32 {
    40
}

fn default_share() -> f32 {
    1.0
}

/// Reads resources, adding each declared subtype as a resource of its own
/// right after its parent.
fn flatten_resources<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<ResourceDef>, D::Error> {
    let list: Vec<ResourceDef> = Vec::deserialize(d)?;
    let mut out: Vec<ResourceDef> = Vec::new();
    for r in list {
        let kinds: Vec<ResourceDef> = r
            .subtypes
            .iter()
            .map(|k| ResourceDef {
                id: format!("{}.{}", r.id, k.id),
                name: k.name.clone(),
                transport: r.transport,
                network: r.network.clone(),
                max_trip: k.max_trip.unwrap_or(r.max_trip),
                colour: k.colour,
                subtypes: Vec::new(),
                parent: Some(r.id.clone()),
                share: k.share,
                uses: k.uses.clone(),
                trail: k.trail.clone(),
            })
            .collect();
        out.push(r);
        for k in kinds {
            if !out.iter().any(|x| x.id == k.id) {
                out.push(k);
            }
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerDef {
    pub id: String,
    pub name: String,
    /// Share of a tile's value spread to its neighbours per tick (0..1).
    #[serde(default)]
    pub diffusion: f32,
    /// Share of the value lost per tick (0..1).
    #[serde(default)]
    pub decay: f32,
    /// Overlay colour for high values (RGB 0..1).
    #[serde(default = "default_layer_colour")]
    pub colour: [f32; 3],
    /// Value shown as full intensity in the overlay.
    #[serde(default = "default_layer_max")]
    pub max: f32,
}

fn default_layer_colour() -> [f32; 3] {
    [1.0, 0.3, 0.2]
}
fn default_layer_max() -> f32 {
    100.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkDef {
    pub id: String,
    pub name: String,
    /// Structure codes forming the network ("road", "power_line").
    pub structures: Vec<String>,
    /// Whether adjacent buildings also conduct (like OpenCity's power).
    #[serde(default)]
    pub through_buildings: bool,
    /// Vehicles moving along it (`[[network.vehicle]]`), which makes it a
    /// vehicle network: agent trips are shown as these vehicles, and each
    /// keeps to the ways it may use. Without any, a road network carries
    /// cars, pickups and sports cars on every road.
    #[serde(default, rename = "vehicle", skip_serializing_if = "Vec::is_empty")]
    pub vehicles: Vec<VehicleDef>,
}

/// A kind of vehicle on a network, declared inside it as `[[network.vehicle]]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleDef {
    pub id: String,
    pub name: String,
    /// What it looks like: car, pickup, sports_car, walker, handcart, horse_cart.
    pub model: String,
    /// Ways it may use: road kinds (street, highway, paved_street, dirt_road).
    /// Empty means every way of the network.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ways: Vec<String>,
    /// Top speed in road speed units (a street is 1): a walker is no
    /// quicker on a highway. Trips of the resources it carries are measured
    /// at this speed at most.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    /// Resources (or kinds) whose trips it makes; empty means any resource
    /// on the network. A resource's kinds are carried by what carries it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carries: Vec<String>,
    /// How often it is picked among the vehicles fit for a trip.
    #[serde(default = "default_share")]
    pub share: f32,
    /// Takes the quickest route on empty roads instead of avoiding traffic.
    #[serde(default)]
    pub direct: bool,
    /// 3D models to draw it with instead of `model`'s, one picked per trip:
    /// `.obj` files under `<data>/models/` (`"kenney/car-kit/taxi"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meshes: Vec<String>,
}

impl VehicleDef {
    pub fn kind(&self) -> crate::traffic::VehicleKind {
        crate::traffic::VehicleKind::from_id(&self.model).unwrap_or(crate::traffic::VehicleKind::Car)
    }

    /// The road kinds it may drive on.
    pub fn ways(&self) -> crate::traffic::Ways {
        if self.ways.is_empty() {
            return crate::traffic::Ways::ALL;
        }
        crate::traffic::Ways::of(self.ways.iter().filter_map(|w| crate::RoadKind::from_id(w)))
    }

    pub fn top_speed(&self) -> f32 {
        self.speed.unwrap_or(f32::INFINITY)
    }

    pub fn path_cost(&self) -> crate::traffic::PathCost {
        if self.direct {
            crate::traffic::PathCost::Distance
        } else {
            crate::traffic::PathCost::Traffic
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bound {
    #[serde(default)]
    pub min: Option<f32>,
    #[serde(default)]
    pub max: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Emission {
    pub amount: f32,
    /// Tiles around the building reached, with linear falloff.
    #[serde(default)]
    pub radius: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitDef {
    pub id: String,
    pub name: String,
    /// One line shown when the unit is inspected.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Model shown (OpenCity graphic code); its footprint is the unit's size.
    pub graphic: u16,
    /// Produced per tick, scaled by how well the unit's needs are met.
    #[serde(default)]
    pub produce: BTreeMap<String, f32>,
    /// Needed per tick; delivered by networks, agents or the treasury.
    #[serde(default)]
    pub needs: BTreeMap<String, f32>,
    /// Produced resources pushed to buildings that need them.
    #[serde(default)]
    pub sends: Vec<String>,
    /// Added to map layers per tick.
    #[serde(default)]
    pub emits: BTreeMap<String, Emission>,
    /// Layer conditions at the unit's tile; each unmet one halves satisfaction.
    #[serde(default)]
    pub requires: BTreeMap<String, Bound>,
    /// Cost when placed with a service tool (0: the tool's own cost).
    #[serde(default)]
    pub build_cost: u32,
    /// Metrics recorded per tick (Extended GlassBox `log`), scaled like production.
    #[serde(default)]
    pub logs: BTreeMap<String, f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DensityDef {
    pub id: String,
    pub name: String,
    /// Zoning cost per tile.
    #[serde(default = "default_zone_cost")]
    pub cost: u32,
    /// Units growing on this density, from level 1 upwards.
    pub units: Vec<String>,
}

fn default_zone_cost() -> u32 {
    5
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DemandSource {
    /// Needed by buildings but not delivered (e.g. open jobs).
    Unmet,
    /// Produced but not delivered anywhere (e.g. unemployed workers).
    Undelivered,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemandTerm {
    pub resource: String,
    pub from: DemandSource,
    #[serde(default = "one")]
    pub weight: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZoneDef {
    pub id: String,
    pub name: String,
    /// "residential", "commercial" or "industrial": the OpenCity zone it uses.
    pub structure: String,
    pub densities: Vec<DensityDef>,
    /// Zone demand: positive terms make it grow.
    #[serde(default)]
    pub demand: Vec<DemandTerm>,
    /// Demand always present (immigration, start-up).
    #[serde(default)]
    pub base_demand: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrowthDef {
    /// Satisfaction above which units may level up.
    pub level_up: f32,
    /// Satisfaction below which units level down (and plots empty at level 1).
    pub level_down: f32,
    /// Chance per tick that an eligible unit changes level.
    pub chance: f32,
    /// New buildings per tick at most, per zone type.
    pub max_new: u32,
    /// Road tiles must be this close to a plot for it to develop.
    pub road_access: u32,
    /// Ticks a unit stays as it is after being built or changing level.
    #[serde(default = "default_grace")]
    pub grace: u32,
    /// Ticks in a row satisfaction must stay past a threshold before a unit
    /// levels up, levels down or is abandoned.
    #[serde(default = "default_patience")]
    pub patience: u32,
    /// A zone stops developing new plots while more than this share of its
    /// established units (older than `patience`) are failing.
    #[serde(default = "default_saturation")]
    pub saturation: f32,
}

fn default_saturation() -> f32 {
    0.1
}

fn default_grace() -> u32 {
    40
}

fn default_patience() -> u32 {
    15
}

impl Default for GrowthDef {
    fn default() -> Self {
        Self {
            level_up: 0.8,
            level_down: 0.3,
            chance: 0.2,
            max_new: 3,
            road_access: 3,
            grace: default_grace(),
            patience: default_patience(),
            saturation: default_saturation(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ruleset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Years the rules suit (inclusive); a new city in an age outside them
    /// gets another rules set by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub years: Option<[i64; 2]>,
    /// Tool ids not offered with these rules (e.g. power lines before electricity).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_tools: Vec<String>,
    #[serde(default, rename = "resource", deserialize_with = "flatten_resources")]
    pub resources: Vec<ResourceDef>,
    #[serde(default, rename = "layer")]
    pub layers: Vec<LayerDef>,
    #[serde(default, rename = "network")]
    pub networks: Vec<NetworkDef>,
    #[serde(default, rename = "unit")]
    pub units: Vec<UnitDef>,
    #[serde(default, rename = "zone")]
    pub zones: Vec<ZoneDef>,
    /// Service tool id ("coal_plant", "fire_station"…) -> unit id.
    #[serde(default)]
    pub services: BTreeMap<String, String>,
    /// Resource counted as the city's population.
    #[serde(default)]
    pub population: Option<String>,
    #[serde(default)]
    pub growth: GrowthDef,
    /// GlassBox rule scripts (`.gbx`, relative to the rules file) compiled into this set.
    #[serde(default)]
    pub scripts: Vec<String>,
    /// Per-tile layer rules (GlassBox `mapRule`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub map_rules: Vec<MapRule>,
    /// Development rules (GlassBox `zoneRule`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zone_rules: Vec<ZoneRule>,
    /// Effects of moving agents (Extended GlassBox `agentRule`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_rules: Vec<AgentRule>,
    /// Tech tree file (`<data>/techs/<id>.toml`) merged into `techs` on load.
    /// Without one (and without inline `[[tech]]`), nothing is locked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tech_tree: Option<String>,
    /// Research projects: inline `[[tech]]` plus those of the tech tree.
    #[serde(default, rename = "tech", skip_serializing_if = "Vec::is_empty")]
    pub techs: Vec<TechDef>,
}

/// A tech tree file (`<data>/techs/<id>.toml`): research projects kept apart
/// from the rules, so rule sets can share one, swap it or go without.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TechTree {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "tech")]
    pub techs: Vec<TechDef>,
}

impl Ruleset {
    /// Adds a tech tree's projects (call before `validate`).
    pub fn add_tech_tree(&mut self, tree: TechTree) {
        self.techs.extend(tree.techs);
    }
}

/// A research project (`[[tech]]`): paid from pooled resources, it unlocks
/// units and tools. A unit or tool named by any project stays locked until
/// that project is done.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TechDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Pooled resources to spend (`{ research = 200 }`).
    pub cost: BTreeMap<String, f32>,
    /// Projects to finish first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    /// Unit ids and tool ids it unlocks.
    #[serde(default)]
    pub unlocks: Vec<String>,
    /// Branch of the tree, for grouping (`"Housing"`, `"Doctrine"`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    /// Depth shown in the tree (1 = basic). Display and ordering only:
    /// `requires` and the cost are the gates.
    #[serde(default = "first_tier", skip_serializing_if = "is_first_tier")]
    pub tier: u32,
    /// Rival projects: finishing this one rules them out for good, and
    /// finishing one of them rules this out (a town's doctrine).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excludes: Vec<String>,
    /// Extra production once done, as a share per resource
    /// (`{ goods = 0.2 }` makes 20% more goods).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bonus: BTreeMap<String, f32>,
}

fn first_tier() -> u32 {
    1
}

fn is_first_tier(v: &u32) -> bool {
    *v == 1
}

/// Comparison used by rule conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cmp {
    AtLeast,
    AtMost,
    Greater,
    Less,
    Is,
}

impl Cmp {
    pub fn holds(self, value: f32, bound: f32) -> bool {
        match self {
            Cmp::AtLeast => value >= bound,
            Cmp::AtMost => value <= bound,
            Cmp::Greater => value > bound,
            Cmp::Less => value < bound,
            Cmp::Is => (value - bound).abs() < 1e-3,
        }
    }
}

/// Where a condition reads its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// A map layer at the tile.
    Map,
    /// A city-wide bin: last tick's production of a resource.
    Global,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Test {
    pub scope: Scope,
    pub id: String,
    pub cmp: Cmp,
    pub value: f32,
}

/// Runs on every tile every `every` ticks: when the conditions hold and the
/// inputs are available on the tile, the inputs are taken and outputs added.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapRule {
    pub name: String,
    pub every: u32,
    pub conditions: Vec<Test>,
    pub inputs: Vec<(String, f32)>,
    pub outputs: Vec<(String, f32)>,
}

/// Every `every` ticks, samples empty plots and builds `create` where the tests hold.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ZoneRule {
    pub name: String,
    /// Zone id the plots must belong to (any zone when absent).
    pub zone: Option<String>,
    pub every: u32,
    pub sample: u32,
    pub tests: Vec<Test>,
    pub create: String,
}

/// Emission of a map value along the route of agents carrying `resources`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentRule {
    pub name: String,
    /// Resources whose trips the rule follows (any of them; a resource also
    /// covers its kinds). Empty: every trip.
    #[serde(default)]
    pub resources: Vec<String>,
    /// Emit every `every` tiles travelled.
    pub every: u32,
    pub areas: Vec<AreaEmission>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AreaEmission {
    pub layer: String,
    pub amount: f32,
    pub radius: u32,
}

#[derive(Debug, thiserror::Error)]
#[error("rules {rules:?}: {message}")]
pub struct RulesError {
    pub rules: String,
    pub message: String,
}

impl Ruleset {
    /// Whether the tool (by id) is offered with these rules.
    pub fn allows(&self, tool_id: &str) -> bool {
        !self.disabled_tools.iter().any(|t| t == tool_id)
    }

    /// Whether the rules suit a city in `year` (rules without years suit any).
    pub fn fits_year(&self, year: i64) -> bool {
        self.years.is_none_or(|[a, b]| (a..=b).contains(&year))
    }

    /// Whether the rules suit any year of the range.
    pub fn fits_years(&self, range: [i64; 2]) -> bool {
        self.years.is_none_or(|[a, b]| a <= range[1] && range[0] <= b)
    }

    /// The unit a service tool builds, if the rules map it.
    pub fn service_unit(&self, tool_id: &str) -> Option<&UnitDef> {
        self.services.get(tool_id).and_then(|u| self.unit(u)).map(|u| &self.units[u])
    }

    pub fn resource(&self, id: &str) -> Option<usize> {
        self.resources.iter().position(|r| r.id == id)
    }

    /// The resource a kind belongs to (itself when it is not a kind).
    pub fn root(&self, resource: usize) -> usize {
        self.resources[resource].parent.as_deref().and_then(|p| self.resource(p)).unwrap_or(resource)
    }

    /// The kinds of a resource.
    pub fn kinds(&self, resource: usize) -> Vec<usize> {
        let id = &self.resources[resource].id;
        (0..self.resources.len()).filter(|&k| self.resources[k].parent.as_ref() == Some(id)).collect()
    }

    /// A resource and its kinds: the resources that can fill a need for it.
    pub fn family(&self, resource: usize) -> Vec<usize> {
        std::iter::once(resource).chain(self.kinds(resource)).collect()
    }
    pub fn layer(&self, id: &str) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }
    pub fn unit(&self, id: &str) -> Option<usize> {
        self.units.iter().position(|u| u.id == id)
    }
    pub fn network(&self, id: &str) -> Option<usize> {
        self.networks.iter().position(|n| n.id == id)
    }

    /// The vehicles that may carry a resource (or kind), with its network.
    /// Empty when the network names no vehicles.
    pub fn vehicles_for(&self, resource: usize) -> Vec<&VehicleDef> {
        let r = &self.resources[resource];
        let Some(net) = r.network.as_deref().and_then(|n| self.network(n)) else { return Vec::new() };
        let root = &self.resources[self.root(resource)].id;
        self.networks[net]
            .vehicles
            .iter()
            .filter(|v| {
                v.carries.is_empty()
                    || v.carries.iter().any(|c| {
                        // The resource, the resource it is a kind of, or one of its kinds.
                        c == &r.id || c == root || self.resource(c).is_some_and(|k| self.resources[k].parent.as_ref() == Some(&r.id))
                    })
            })
            .collect()
    }

    pub fn tech(&self, id: &str) -> Option<usize> {
        self.techs.iter().position(|t| t.id == id)
    }

    /// Whether `tech` is ruled out by a rival among `done`.
    pub fn tech_excluded(&self, tech: &str, done: &std::collections::BTreeSet<String>) -> bool {
        let own = self.tech(tech).is_some_and(|t| self.techs[t].excludes.iter().any(|e| done.contains(e)));
        own || self.techs.iter().any(|t| done.contains(&t.id) && t.excludes.iter().any(|e| e == tech))
    }

    /// The research projects that unlock a unit or tool id.
    pub fn locked_by<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a TechDef> + 'a {
        self.techs.iter().filter(move |t| t.unlocks.iter().any(|u| u == id))
    }

    /// Whether the resource travels on a network that names its vehicles.
    pub fn vehicle_network(&self, resource: usize) -> bool {
        self.resources[resource].network.as_deref().and_then(|n| self.network(n)).is_some_and(|n| !self.networks[n].vehicles.is_empty())
    }

    /// Zone using an OpenCity zone structure.
    pub fn zone_for(&self, code: StructureCode) -> Option<usize> {
        self.zones.iter().position(|z| structure_code(&z.structure) == Some(code))
    }

    /// Checks every cross-reference.
    pub fn validate(&self) -> Result<(), RulesError> {
        let err = |message: String| Err(RulesError { rules: self.id.clone(), message });
        for r in &self.resources {
            if let Some(p) = &r.parent {
                match self.resource(p).map(|i| &self.resources[i]) {
                    None => return err(format!("resource {}: unknown parent {p}", r.id)),
                    Some(pr) if pr.parent.is_some() => return err(format!("resource {}: kinds cannot have kinds", r.id)),
                    Some(pr) if !matches!(pr.transport, Transport::Agents | Transport::Grid) => {
                        return err(format!("resource {}: only agent and grid resources have kinds", pr.id))
                    }
                    Some(pr) if pr.transport != r.transport || pr.network != r.network => return err(format!("resource {}: a kind moves like {p}", r.id)),
                    _ => {}
                }
                if !r.share.is_finite() || r.share < 0.0 {
                    return err(format!("resource {}: share must be 0 or more", r.id));
                }
            }
            for u in r.uses.keys() {
                if self.resource(u).is_none() {
                    return err(format!("resource {}: uses unknown resource {u}", r.id));
                }
            }
            for l in r.trail.keys() {
                if self.layer(l).is_none() {
                    return err(format!("resource {}: trail on unknown layer {l}", r.id));
                }
            }
            match (r.transport, &r.network) {
                (Transport::Agents | Transport::Grid, None) => return err(format!("resource {}: {:?} transport needs a network", r.id, r.transport)),
                (_, Some(n)) if self.network(n).is_none() => return err(format!("resource {}: unknown network {n}", r.id)),
                _ => {}
            }
        }
        for n in &self.networks {
            for s in &n.structures {
                if structure_code(s).is_none() {
                    return err(format!("network {}: unknown structure {s}", n.id));
                }
            }
            for v in &n.vehicles {
                let at = |m: String| err(format!("network {} vehicle {}: {m}", n.id, v.id));
                if crate::traffic::VehicleKind::from_id(&v.model).is_none() {
                    return at(format!("unknown model {}", v.model));
                }
                if !v.ways.is_empty() && !n.structures.iter().any(|s| s == "road") {
                    return at("ways are road kinds, but the network has no roads".into());
                }
                for w in &v.ways {
                    if crate::RoadKind::from_id(w).is_none() {
                        return at(format!("unknown way {w}"));
                    }
                }
                for c in &v.carries {
                    match self.resource(c) {
                        None => return at(format!("carries unknown resource {c}")),
                        Some(i) if self.resources[i].network.as_deref() != Some(n.id.as_str()) => {
                            return at(format!("carries {c}, which does not travel on this network"))
                        }
                        _ => {}
                    }
                }
                if v.speed.is_some_and(|x| !x.is_finite() || x <= 0.0) || !v.share.is_finite() || v.share < 0.0 {
                    return at("speed must be above 0 and share 0 or more".into());
                }
            }
        }
        for u in &self.units {
            for r in u.produce.keys().chain(u.needs.keys()).chain(u.sends.iter()) {
                if self.resource(r).is_none() {
                    return err(format!("unit {}: unknown resource {r}", u.id));
                }
            }
            for l in u.emits.keys().chain(u.requires.keys()) {
                if self.layer(l).is_none() {
                    return err(format!("unit {}: unknown layer {l}", u.id));
                }
            }
        }
        for z in &self.zones {
            match structure_code(&z.structure) {
                Some(c) if c.is_zone() => {}
                _ => return err(format!("zone {}: structure must be residential, commercial or industrial", z.id)),
            }
            if z.densities.is_empty() || z.densities.len() > 3 {
                return err(format!("zone {}: 1 to 3 densities", z.id));
            }
            for d in &z.densities {
                if d.units.is_empty() {
                    return err(format!("zone {} density {}: no units", z.id, d.id));
                }
                for u in &d.units {
                    if self.unit(u).is_none() {
                        return err(format!("zone {} density {}: unknown unit {u}", z.id, d.id));
                    }
                }
            }
            for t in &z.demand {
                if self.resource(&t.resource).is_none() {
                    return err(format!("zone {}: unknown resource {}", z.id, t.resource));
                }
            }
        }
        for (tool, unit) in &self.services {
            if self.unit(unit).is_none() {
                return err(format!("service {tool}: unknown unit {unit}"));
            }
        }
        for r in &self.map_rules {
            for (l, _) in r.inputs.iter().chain(r.outputs.iter()) {
                if self.layer(l).is_none() {
                    return err(format!("mapRule {}: unknown map {l}", r.name));
                }
            }
        }
        for r in &self.zone_rules {
            if self.unit(&r.create).is_none() {
                return err(format!("zoneRule {}: unknown unit {}", r.name, r.create));
            }
            if let Some(z) = &r.zone {
                if !self.zones.iter().any(|x| &x.id == z) {
                    return err(format!("zoneRule {}: unknown zone {z}", r.name));
                }
            }
        }
        for r in &self.agent_rules {
            for a in &r.areas {
                if self.layer(&a.layer).is_none() {
                    return err(format!("agentRule {}: unknown map {}", r.name, a.layer));
                }
            }
        }
        for t in &self.techs {
            let at = |m: String| err(format!("tech {}: {m}", t.id));
            if t.cost.is_empty() {
                return at("costs nothing".into());
            }
            for (r, v) in &t.cost {
                match self.resource(r) {
                    Some(i) if self.resources[i].transport == Transport::Pool && v.is_finite() && *v > 0.0 => {}
                    Some(_) => return at(format!("cost in {r}: pooled resources only, more than 0")),
                    None => return at(format!("unknown resource {r}")),
                }
            }
            for q in &t.requires {
                if self.tech(q).is_none() || q == &t.id {
                    return at(format!("unknown requirement {q}"));
                }
            }
            for u in &t.unlocks {
                if self.unit(u).is_none() && !crate::Tool::ALL.iter().any(|x| x.id() == u) {
                    return at(format!("unlocks unknown unit or tool {u}"));
                }
            }
            for q in &t.excludes {
                if self.tech(q).is_none() || q == &t.id {
                    return at(format!("excludes unknown project {q}"));
                }
            }
            for (r, b) in &t.bonus {
                if self.resource(r).is_none() {
                    return at(format!("bonus to unknown resource {r}"));
                }
                if !b.is_finite() || *b <= -1.0 {
                    return at(format!("bonus to {r} must be more than -1"));
                }
            }
            if self.techs.iter().filter(|o| o.id == t.id).count() > 1 {
                return at("defined twice".into());
            }
        }
        if let Some(p) = &self.population {
            if self.resource(p).is_none() {
                return err(format!("population: unknown resource {p}"));
            }
        }
        Ok(())
    }
}

/// Structure code from a rules-file name.
pub fn structure_code(name: &str) -> Option<StructureCode> {
    use StructureCode::*;
    Some(match name {
        "residential" => Residential,
        "commercial" => Commercial,
        "industrial" => Industrial,
        "road" => Road,
        "power_line" => PowerLine,
        "coal_plant" => CoalPlant,
        "nuclear_plant" => NuclearPlant,
        "park" => Park,
        "fire_station" => FireDept,
        "police" => PoliceDept,
        "hospital" => Hospital,
        "school" => School,
        _ => return None,
    })
}
