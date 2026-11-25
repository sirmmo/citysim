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
    /// Price per unit when traded with neighbouring cities of a region
    /// (0: not traded).
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub price: f32,
    /// A pooled resource shared across a region as culture: each city takes
    /// in a share of what the others make (ideas spreading from town to town).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shared: bool,
    /// How much a unit of it counts in satisfaction: a building's
    /// satisfaction is the share of its needs met, each weighted by its
    /// amount times this (power counts for more than parking).
    #[serde(default = "default_share", skip_serializing_if = "is_one_f32")]
    pub weight: f32,
    /// When its trips are made, by the hour (`tide = { out = [8], back =
    /// [18] }`: out in the morning, back in the evening). None: the same
    /// at any hour. Kinds follow their resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tide: Option<TideDef>,
    /// What a need for it that is not met leaves on map layers at the
    /// building, per unit missing and tick (`spill = { water_pollution =
    /// 0.06 }` on sewage: what is not treated runs into the ground).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub spill: BTreeMap<String, f32>,
    /// What a unit made of it and found no taker for leaves on map layers
    /// at the building, per unit left over and tick (`idle = { crime =
    /// 0.15 }` on workers: people with no job to go to).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub idle: BTreeMap<String, f32>,
}

/// The hours of a resource's trips. Over a day the traffic is the same as
/// without a tide; it gathers around these hours.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TideDef {
    /// Hours (0 to 24) around which the trips set out.
    pub out: Vec<f32>,
    /// Hours around which they come back the way they went; none: one way.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub back: Vec<f32>,
    /// How far around each hour the trips spread, in hours.
    #[serde(default = "default_spread")]
    pub spread: f32,
    /// At takers that work round the clock these trips follow the shift
    /// changes (`[shifts]`) instead: staff, not customers.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shifts: bool,
}

fn default_spread() -> f32 {
    1.5
}

impl TideDef {
    /// How dense trips around `hours` are at `hour`, against an even day
    /// (the mean over 24 hours is 1).
    pub fn density(hours: &[f32], spread: f32, hour: f32) -> f32 {
        if hours.is_empty() {
            return 0.0;
        }
        let s = spread.max(0.25);
        let peak = 24.0 / (hours.len() as f32 * s * (2.0 * std::f32::consts::PI).sqrt());
        hours.iter().map(|p| (-1..=1).map(|k| (-(hour - p + 24.0 * k as f32).powi(2) / (2.0 * s * s)).exp()).sum::<f32>()).sum::<f32>() * peak
    }

    /// The share of a tick's trips on their way out and on their way back
    /// at `hour` (together 1 on average over the day).
    pub fn flow(&self, hour: f32) -> (f32, f32) {
        let out = Self::density(&self.out, self.spread, hour);
        if self.back.is_empty() {
            (out, 0.0)
        } else {
            (out / 2.0, Self::density(&self.back, self.spread, hour) / 2.0)
        }
    }
}

/// Shift changes of the units that work round the clock (`[shifts]`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShiftsDef {
    /// Hours at which one shift relieves the other.
    pub hours: Vec<f32>,
    #[serde(default = "default_shift_spread")]
    pub spread: f32,
}

fn default_shift_spread() -> f32 {
    1.0
}

impl Default for ShiftsDef {
    fn default() -> Self {
        Self { hours: vec![6.0, 14.0, 22.0], spread: 1.0 }
    }
}

impl ShiftsDef {
    /// Staff arriving and leaving at `hour`: each change brings one shift
    /// in and sends one home.
    pub fn flow(&self, hour: f32) -> (f32, f32) {
        let d = TideDef::density(&self.hours, self.spread, hour) / 2.0;
        (d, d)
    }
}

fn is_one_f32(v: &f32) -> bool {
    *v == 1.0
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
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
                price: r.price,
                shared: false,
                weight: r.weight,
                tide: None,
                spill: BTreeMap::new(),
                idle: BTreeMap::new(),
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
    /// Above this value nothing develops on zoned land, and the zone
    /// buildings standing there empty (fallout).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uninhabitable: Option<f32>,
    /// The layer is read off the land (0 to 10) and nothing else changes
    /// it: `"fertile"` (flat ground, best near water) or `"ore"` (high and
    /// steep ground).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<Ground>,
}

/// What a layer reads off the land (`ground`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ground {
    Fertile,
    Ore,
}

/// A unit that makes more or less by the land it stands on (`yields`): by
/// the value of a layer at its place, from three quarters of what the
/// rules write where the layer is empty, through as written at a third of
/// `full`, to one and a half times at `full` and above.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YieldDef {
    pub layer: String,
    pub full: f32,
}

impl YieldDef {
    /// The factor on production where the layer is `v`.
    pub fn factor(&self, v: f32) -> f32 {
        0.75 + 0.75 * (v / self.full.max(1e-3)).clamp(0.0, 1.0)
    }
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
    /// A public transport line it runs on between stops, instead of going
    /// door to door: `"bus"` (bus stops on the streets) or `"tram"` (tram
    /// stops on tram streets). People near two stops of one line ride it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// For a vehicle of a line: the riders one of them takes at a stop in
    /// a tick, on routes the player draws (bus 6, tram 15, metro 40 when
    /// left out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<f32>,
}

impl VehicleDef {
    /// Whether it goes door to door on the roads (not trains, boats, buses
    /// or trams, which run between stops).
    pub fn door_to_door(&self) -> bool {
        self.line.is_none() && !self.ways().allows(crate::RoadKind::Rail) && !self.ways().sails()
    }

    pub fn kind(&self) -> crate::traffic::VehicleKind {
        crate::traffic::VehicleKind::from_id(&self.model).unwrap_or(crate::traffic::VehicleKind::Car)
    }

    /// The road kinds it may drive on.
    pub fn ways(&self) -> crate::traffic::Ways {
        if self.ways.is_empty() {
            return crate::traffic::Ways::ALL;
        }
        let roads = crate::traffic::Ways::of(self.ways.iter().filter_map(|w| crate::RoadKind::from_id(w)));
        if self.ways.iter().any(|w| w == "water") {
            roads.union(crate::traffic::Ways::WATER)
        } else {
            roads
        }
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
    /// Reach along the streets instead of as the crow flies: `radius` is a
    /// travel time (in street tiles) from the building, and places reach
    /// it from a road at most 2 tiles away. Fire, police and ambulances
    /// cover what they can drive to.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_road: bool,
}

/// Vehicles a service sends out to the buildings it covers (`dispatch`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dispatch {
    /// How it moves and looks without meshes: car, pickup, walker…
    #[serde(default = "default_dispatch_model")]
    pub model: String,
    /// `.obj` models (`kenney/car-kit/firetruck`), one picked per call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meshes: Vec<String>,
    /// One call every this many ticks, on average.
    #[serde(default = "default_dispatch_every")]
    pub every: u32,
}

fn default_dispatch_model() -> String {
    "car".into()
}

fn default_dispatch_every() -> u32 {
    10
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
    /// Sent resources that must find takers, with how much each matters
    /// (`{ workers = 1, shoppers = 0.5 }`: homes want jobs for their workers
    /// and, half as much, shops for their shoppers). The share left unsent
    /// counts against satisfaction like an unmet need, times this.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outlets: BTreeMap<String, f32>,
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
    /// Traded with the rest of the world through this unit (airports,
    /// seaports), per tick and resource: it brings in up to this much of
    /// what the city lacks and takes out up to this much of what has
    /// nowhere to go, at the resource's price.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub trade: BTreeMap<String, f32>,
    /// Vehicles it sends to the buildings it covers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch: Option<Dispatch>,
    /// Staffed day and night in shifts (hospitals, police, fire stations):
    /// its staff come and go at the shift changes, not with the rush hours.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub round_the_clock: bool,
    /// What it makes follows the sky: `"sun"` (the hour, the season and
    /// the clouds) or `"wind"` (the weather, and gusts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follows: Option<Follows>,
    /// What it makes depends on the land it stands on (fields on fertile
    /// ground, mines on ore).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yields: Option<YieldDef>,
    /// What is left of a building research made obsolete (see
    /// `obsoletes` on a tech): shown as a relic when inspected.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub relic: bool,
    /// Money paid back when it is pulled down: scrap, stone, the land.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub salvage: u32,
    /// The most a city can have of it (0: any number). A city hall is one.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub limit: u32,
    /// Points of approval the mayor has for each one that works, by how
    /// well it works (all buildings together give 8 at most).
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub approval: f32,
    /// What each one that works changes of the `[administration]` bill, as
    /// a share (`-0.25`: a quarter less); the bill never falls under half.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub administration: f32,
    /// Makes a neighbourhood of the tiles this far around it (0: none).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub district: u32,
}

/// What a unit's production follows (`follows`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Follows {
    Sun,
    Wind,
}

impl UnitDef {
    /// How far its calls go: the widest of its emissions.
    pub fn reach(&self) -> u32 {
        self.emits.values().map(|e| e.radius).max().unwrap_or(12)
    }
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
    /// Share of its buildings that work round the clock in shifts (0.05:
    /// one in twenty), picked by where they stand.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub round_the_clock: f32,
    /// City builder mode: the densities are one ladder (the low units,
    /// then the medium ones, then the high ones) that every plot climbs as
    /// far as its needs are met, whatever density was painted. Leave it
    /// off for densities that are kinds rather than levels (fields, mines,
    /// workshops): those grow as painted in either mode.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ladder: bool,
    /// City builder mode: the unit a new plot of a ladder zone starts as
    /// (a tent), below the first unit of the ladder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder_start: Option<String>,
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
    /// Ticks in a row a unit may go with one of its needs (or outlets) met
    /// by less than half before it levels down or is abandoned, whatever
    /// its satisfaction (0: never). Such a unit does not level up either.
    #[serde(default = "default_neglect")]
    pub neglect: u32,
}

fn default_neglect() -> u32 {
    120
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
            neglect: default_neglect(),
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
    /// Policy file (`<data>/policies/<id>.toml`) merged into `taxes` and
    /// `policies` on load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_set: Option<String>,
    /// Taxes the city sets the rate of (`[[tax]]`).
    #[serde(default, rename = "tax", skip_serializing_if = "Vec::is_empty")]
    pub taxes: Vec<TaxDef>,
    /// Policies: axes of stances the city picks one of (`[[policy]]`).
    #[serde(default, rename = "policy", skip_serializing_if = "Vec::is_empty")]
    pub policies: Vec<PolicyDef>,
    /// Disasters (`[[disaster]]`).
    #[serde(default, rename = "disaster", skip_serializing_if = "Vec::is_empty")]
    pub disasters: Vec<DisasterDef>,
    /// Policies for single neighbourhoods (`[[neighbourhood_policy]]`).
    #[serde(default, rename = "neighbourhood_policy", skip_serializing_if = "Vec::is_empty")]
    pub district_policies: Vec<DistrictPolicyDef>,
    /// Events, with or without choices (`[[event]]`).
    #[serde(default, rename = "event", skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<EventDef>,
    /// What residents hold against the mayor (`[[complaint]]`).
    #[serde(default, rename = "complaint", skip_serializing_if = "Vec::is_empty")]
    pub complaints: Vec<ComplaintDef>,
    /// Running the city (`[administration]`): a cost per head that grows
    /// with the population.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub administration: Option<AdministrationDef>,
    /// People without a job or a home (`[people]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub people: Option<PeopleDef>,
    /// When a city turns on its mayor (`[mayor]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mayor: Option<MayorDef>,
    /// Shift changes of the units that work round the clock (`[shifts]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shifts: Option<ShiftsDef>,
}

/// Something residents complain about (`[[complaint]]`): a map layer too
/// high or too low where they live. It costs the mayor `points` of approval
/// (out of 100) times the share of residents living with it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComplaintDef {
    /// How residents put it ("Dirty air").
    pub name: String,
    pub layer: String,
    /// They complain where the layer is above this…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub above: Option<f32>,
    /// …or below this (coverage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub below: Option<f32>,
    /// Approval lost when every resident lives with it.
    pub points: f32,
    /// What to do about it, for the advice list.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub advice: String,
}

/// When a city turns on its mayor (`[mayor]`); what is left out is as in
/// [`MayorDef::default`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MayorDef {
    /// Approval (0..1) under which the months count against the mayor.
    pub oust_below: f32,
    /// Months in a row under it before the vote of no confidence.
    pub oust_months: u8,
    /// Smaller places than this have no vote.
    pub voters: u32,
    /// Years between elections, with terms of office.
    pub term_years: u32,
}

impl Default for MayorDef {
    fn default() -> Self {
        Self { oust_below: 0.30, oust_months: 12, voters: 500, term_years: 4 }
    }
}

/// People without a job or a home (`[people]`). The jobless are those of
/// the `workers` resource nobody took on. The homeless are the people of
/// homes that are gone (abandoned, burnt, evacuated, bulldozed, or grown
/// smaller): they wait for room in new homes and, little by little, leave.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeopleDef {
    /// The resource that is people going to work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workers: Option<String>,
    /// Share of the homeless who leave town each tick.
    #[serde(default)]
    pub homeless_leave: f32,
}

/// Running the city (`[administration]`): paid from the treasury every tick
/// for each inhabitant, more per head as the city grows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdministrationDef {
    /// Budget line.
    #[serde(default = "default_administration")]
    pub name: String,
    /// Per inhabitant and tick.
    pub cost: f32,
    /// Population at which the cost per head has doubled (it triples at
    /// twice that, and so on); 0: the same at any size.
    #[serde(default)]
    pub doubles_at: f32,
    /// Satisfaction every zone building loses while the city cannot pay.
    #[serde(default)]
    pub unpaid: f32,
}

fn default_administration() -> String {
    "Administration".into()
}

impl AdministrationDef {
    /// The cost per tick for a population.
    pub fn cost_for(&self, population: f32) -> f32 {
        let p = population.max(0.0);
        self.cost * p * if self.doubles_at > 0.0 { 1.0 + p / self.doubles_at } else { 1.0 }
    }
}

/// A disaster (`[[disaster]]`): now and then it strikes a neighbourhood;
/// coverage on a map layer protects the buildings there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisasterDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Mean ticks between strikes (0: only when set off by hand).
    #[serde(default)]
    pub every: u32,
    /// Tiles around the spot it reaches.
    #[serde(default = "default_disaster_radius")]
    pub radius: u32,
    pub effect: DisasterEffect,
    /// Map layer that protects (fire coverage against fire).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protection: Option<String>,
    /// Layer value that protects fully (each building's risk falls with it).
    #[serde(default = "default_disaster_full")]
    pub full: f32,
    /// Strikes only near water, on low ground (floods).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub near_water: bool,
    /// Ticks a struck building stays in trouble (0: over at once). A
    /// building hit by a disaster that destroys burns this long before it
    /// is lost; for the others it is how long the mark stays on the map.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub lasts: u32,
    /// While it lasts: the chance each tick of passing to each building
    /// next door, scaled down by the protection there.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub spread: f32,
    /// While it lasts: the chance each tick, under full protection, that
    /// it is dealt with (a fire put out, the building saved).
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub relief: f32,
    /// It starts at a unit of this kind (an accident at a nuclear plant),
    /// which is lost, and nowhere else. `every` then counts for each such
    /// unit, by how well it is run: a tenth as often at full satisfaction
    /// and funding, four times as often with neither.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// What it leaves on a map layer around the spot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallout: Option<Fallout>,
}

/// What a disaster leaves on a map layer (`fallout`): `amount` at the spot,
/// fading to nothing past `radius`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fallout {
    pub layer: String,
    pub amount: f32,
    pub radius: u32,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

/// What a disaster does to a building it hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisasterEffect {
    /// Gone: the plot is empty, and rests before building again.
    Destroy,
    /// One level down.
    Damage,
    /// Its people fall ill: satisfaction drops sharply.
    Sicken,
}

fn default_disaster_radius() -> u32 {
    2
}

fn default_disaster_full() -> f32 {
    10.0
}

/// A tax (`[[tax]]`): scales a city resource (taxes) made by the units of
/// some zones. Above its default rate the zones pay more but want less.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The city resource it scales (`taxes`).
    pub resource: String,
    /// Zone ids whose units pay it; empty means every unit making `resource`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<String>,
    /// Default rate, in percent: units make `resource` as written at this
    /// rate, and in proportion at others.
    pub rate: f32,
    /// Lowest and highest rate the city may set.
    pub range: [f32; 2],
    /// Zone demand added per point above the default (below: subtracted).
    #[serde(default)]
    pub demand: f32,
    /// Satisfaction added per point above the default, for paying units.
    #[serde(default)]
    pub satisfaction: f32,
    /// Satisfaction lost per squared point above the default, on top: a
    /// little more tax is shrugged off, a lot drives people out.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub steep: f32,
}

impl TaxDef {
    /// What a rate adds to the satisfaction of the units paying it.
    pub fn mood(&self, rate: f32) -> f32 {
        let d = rate - self.rate;
        self.satisfaction * d - if d > 0.0 { self.steep * d * d } else { 0.0 }
    }
}

/// A policy (`[[policy]]`): an axis of stances; the first is the default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "stance")]
    pub stances: Vec<StanceDef>,
}

/// One stance of a policy, with its trade-offs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StanceDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Research project to finish before the stance can be taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<String>,
    /// Extra production, as a share per resource (as for research).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bonus: BTreeMap<String, f32>,
    /// Change in what every unit needs, as a share per resource
    /// (`{ car = -0.5 }`: half the parking).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub needs: BTreeMap<String, f32>,
    /// Demand added per zone id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub demand: BTreeMap<String, f32>,
    /// Shares of the kinds of a resource under this stance, in place of
    /// those the rules write (`{ "car.electric" = 0.6, "car.gasoline" =
    /// 0.4 }`). Kinds not named keep their share.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub kinds: BTreeMap<String, f32>,
    /// Change in what traffic leaves on map layers (the areas of agent
    /// rules), as a share per layer (`{ "Noise.level" = -0.3 }`: a third
    /// less noise from every trip). What a kind leaves by its own `trail`
    /// follows the kind's share instead.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub traffic: BTreeMap<String, f32>,
    /// Change in what units leave on map layers (their `emits`), as a
    /// share per layer (`{ pollution = -0.3 }`: works and plants smoke a
    /// third less; `{ crime = 0.4 }`: police stations, which emit against
    /// crime, do so 40% more).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub emits: BTreeMap<String, f32>,
    /// Change in the odds of a disaster, as a share per disaster id
    /// (`{ meltdown = 1.0 }`: twice as likely).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub disasters: BTreeMap<String, f32>,
    /// Satisfaction added to every unit (0.05 is five points).
    #[serde(default)]
    pub satisfaction: f32,
    /// Paid from the treasury each tick (negative: earned).
    #[serde(default)]
    pub cost: f32,
}

/// A policy file (`<data>/policies/<id>.toml`): taxes and policies kept
/// apart from the rules, like tech trees.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicySet {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "tax")]
    pub taxes: Vec<TaxDef>,
    #[serde(default, rename = "policy")]
    pub policies: Vec<PolicyDef>,
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

    /// Adds a policy file's taxes and policies (call before `validate`).
    pub fn add_policy_set(&mut self, set: PolicySet) {
        self.taxes.extend(set.taxes);
        self.policies.extend(set.policies);
    }

    pub fn disaster(&self, id: &str) -> Option<&DisasterDef> {
        self.disasters.iter().find(|d| d.id == id)
    }

    pub fn tax(&self, id: &str) -> Option<&TaxDef> {
        self.taxes.iter().find(|t| t.id == id)
    }

    pub fn policy(&self, id: &str) -> Option<&PolicyDef> {
        self.policies.iter().find(|p| p.id == id)
    }

    /// The unit a zone building grows into at its plot's density, if any
    /// (`builder`: in city builder mode, up its zone's ladder).
    pub fn next_unit(&self, unit: usize, density: u8, builder: bool) -> Option<usize> {
        let id = &self.units.get(unit)?.id;
        let chain = self.chain(self.zone_of_unit(id)?, density, builder);
        let k = chain.iter().position(|u| u == id)?;
        self.unit(chain.get(k + 1)?)
    }

    /// Zone of a unit id, by the densities that list it (or its city
    /// builder start).
    pub fn zone_of_unit(&self, unit: &str) -> Option<usize> {
        self.zones.iter().position(|z| z.builder_start.as_deref() == Some(unit) || z.densities.iter().any(|d| d.units.iter().any(|u| u == unit)))
    }

    /// Whether a zone's buildings climb its whole ladder: city builder
    /// mode, in a zone whose densities are levels.
    pub fn climbs(&self, zone: usize, builder: bool) -> bool {
        builder && self.zones[zone].ladder
    }

    /// The units a building of a zone grows through, level by level: those
    /// of its plot's density or, when it [climbs](Self::climbs), its start
    /// unit and then every density's units, low to high.
    pub fn chain(&self, zone: usize, density: u8, builder: bool) -> Vec<&str> {
        let z = &self.zones[zone];
        if self.climbs(zone, builder) {
            z.builder_start.iter().chain(z.densities.iter().flat_map(|d| &d.units)).map(String::as_str).collect()
        } else {
            z.densities[(density as usize).min(z.densities.len() - 1)].units.iter().map(String::as_str).collect()
        }
    }

    /// The density a unit of a zone stands for: the first that lists it
    /// (the lowest for its start unit).
    pub fn density_of(&self, zone: usize, unit: &str) -> u8 {
        self.zones[zone].densities.iter().position(|d| d.units.iter().any(|u| u == unit)).unwrap_or(0) as u8
    }

    /// The density the zone tools paint for a zone: the chosen one, or the
    /// lowest (the cheapest) where buildings climb the whole ladder.
    pub fn painted_density(&self, zone: usize, chosen: u8, builder: bool) -> u8 {
        if self.climbs(zone, builder) {
            0
        } else {
            chosen
        }
    }
}

/// A policy the mayor can set for one neighbourhood
/// (`[[neighbourhood_policy]]`): it holds for the buildings there only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistrictPolicyDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Money a month for each neighbourhood that has it.
    #[serde(default)]
    pub cost: f32,
    /// Extra production of the buildings there, as a share per resource
    /// (`{ taxes = 0.15 }`).
    #[serde(default)]
    pub bonus: BTreeMap<String, f32>,
    /// Added to the satisfaction of the homes, shops and works there.
    #[serde(default)]
    pub satisfaction: f32,
    /// What the neighbourhood's office leaves on the map while the policy
    /// holds, as a unit's `emits`.
    #[serde(default)]
    pub emits: BTreeMap<String, Emission>,
}

/// What an event or one of its choices does (`effect = { … }`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct EffectDef {
    /// Money at once, for each thousand residents (a city under 500
    /// counts as 500). Negative: the city pays.
    pub money: f32,
    /// Money each month for `months`, for each thousand residents.
    pub monthly: f32,
    pub months: u32,
    /// Points of approval; they fade by a tenth a month.
    pub approval: f32,
    /// Units made obsolete and what each becomes, as on a tech.
    pub obsoletes: BTreeMap<String, String>,
    /// Months the obsolete buildings keep working (0: they close at once).
    pub grace: u32,
    /// Service tools that build another unit from now on
    /// (`{ city_hall = "provisional_council" }`).
    pub services: BTreeMap<String, String>,
}

/// One answer to an event (`[[event.choice]]`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceDef {
    pub label: String,
    /// What is said once it is taken.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub effect: EffectDef,
    /// A gamble: the odds (0..1) that `effect` and `text` are what
    /// happens; otherwise `else_effect` and `else_text` are. 0: no gamble.
    #[serde(default)]
    pub chance: f32,
    #[serde(default)]
    pub else_text: String,
    #[serde(default)]
    pub else_effect: EffectDef,
}

/// Something that happens to the city (`[[event]]`), with or without a
/// choice to make. The last choice is the one taken when the mayor does
/// not answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventDef {
    pub id: String,
    pub name: String,
    pub text: String,
    /// Never comes up by itself: it is set off from outside (a game the
    /// city is part of, the Disasters menu, the MCP server).
    #[serde(default)]
    pub external: bool,
    /// How often it is the one that comes up, against the others.
    #[serde(default = "one")]
    pub weight: f32,
    #[serde(default)]
    pub min_population: u32,
    /// Units of which the city must have at least one standing.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Comes up by itself no more than once.
    #[serde(default)]
    pub once: bool,
    /// What it does when it happens, before any choice.
    #[serde(default)]
    pub effect: EffectDef,
    #[serde(default, rename = "choice")]
    pub choices: Vec<ChoiceDef>,
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
    /// Units this project makes obsolete, and the unit each becomes
    /// (`{ coal_plant = "old_power_station" }`): once it is done no more are
    /// built, and a year later the ones standing close and turn into
    /// what is named, a relic to keep or to pull down.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub obsoletes: BTreeMap<String, String>,
    /// A project of the whole region: every town pays into it, what the
    /// others have paid counts, and once one town finishes it they all
    /// have it. A town on its own pays the lot.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub regional: bool,
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
    /// The seats the rules give the vehicles of a line (`"bus"`, `"tram"`,
    /// `"metro"`), when they say.
    pub fn line_seats(&self, line: &str) -> Option<f32> {
        self.networks.iter().flat_map(|n| &n.vehicles).find(|v| v.line.as_deref() == Some(line)).and_then(|v| v.seats)
    }

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

    /// The tide of a resource (a kind follows the resource it is a kind of).
    pub fn tide(&self, resource: usize) -> Option<&TideDef> {
        self.resources[resource].tide.as_ref().or_else(|| self.resources[self.root(resource)].tide.as_ref())
    }

    /// Whether the building of this unit at tile index `key` works round
    /// the clock: by its own rules, or as one of its zone's share.
    pub fn round_the_clock(&self, unit: usize, key: u32) -> bool {
        let def = &self.units[unit];
        def.round_the_clock
            || self.zone_of_unit(&def.id).is_some_and(|z| {
                let share = self.zones[z].round_the_clock;
                share > 0.0 && ((key.wrapping_mul(2_654_435_761) >> 12) % 1000) as f32 / 1000.0 < share
            })
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
        let hours = |h: &[f32]| h.iter().all(|h| (0.0..=24.0).contains(h));
        if let Some(sh) = &self.shifts {
            if sh.hours.is_empty() || !hours(&sh.hours) {
                return err("shifts: hours must be between 0 and 24".into());
            }
        }
        for z in &self.zones {
            if !(0.0..=1.0).contains(&z.round_the_clock) {
                return err(format!("zone {}: round_the_clock is a share, 0 to 1", z.id));
            }
        }
        for u in &self.units {
            if let Some(y) = u.yields.as_ref().filter(|y| self.layer(&y.layer).is_none() || y.full <= 0.0) {
                return err(format!("unit {}: yields by unknown layer {} (or full is not above 0)", u.id, y.layer));
            }
        }
        for c in &self.complaints {
            if self.layer(&c.layer).is_none() {
                return err(format!("complaint {}: unknown layer {}", c.name, c.layer));
            }
            if c.above.is_some() == c.below.is_some() {
                return err(format!("complaint {}: give either above or below", c.name));
            }
        }
        if let Some(m) = &self.mayor {
            if !(0.0..=1.0).contains(&m.oust_below) || m.oust_months == 0 || m.term_years == 0 {
                return err("mayor: oust_below is a share (0 to 1), oust_months and term_years at least 1".into());
            }
        }
        if let Some(p) = &self.people {
            if let Some(w) = p.workers.as_ref().filter(|w| self.resource(w).is_none()) {
                return err(format!("people: unknown workers resource {w}"));
            }
            if !(0.0..=1.0).contains(&p.homeless_leave) {
                return err("people: homeless_leave is a share, 0 to 1".into());
            }
        }
        for r in &self.resources {
            if let Some(l) = r.spill.keys().find(|l| self.layer(l).is_none()) {
                return err(format!("resource {}: spill on unknown layer {l}", r.id));
            }
            if let Some(l) = r.idle.keys().find(|l| self.layer(l).is_none()) {
                return err(format!("resource {}: idle on unknown layer {l}", r.id));
            }
            if let Some(t) = &r.tide {
                if r.transport != Transport::Agents {
                    return err(format!("resource {}: only agent resources have a tide", r.id));
                }
                if t.out.is_empty() || !hours(&t.out) || !hours(&t.back) {
                    return err(format!("resource {}: tide hours must be between 0 and 24, with at least one in `out`", r.id));
                }
            }
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
                    if crate::RoadKind::from_id(w).is_none() && w != "water" {
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
            if let Some(d) = &u.dispatch {
                if crate::traffic::VehicleKind::from_id(&d.model).is_none() || d.every == 0 {
                    return err(format!("unit {}: dispatch needs a known model and every > 0", u.id));
                }
            }
            for r in u.produce.keys().chain(u.needs.keys()).chain(u.sends.iter()) {
                if self.resource(r).is_none() {
                    return err(format!("unit {}: unknown resource {r}", u.id));
                }
            }
            for (r, w) in &u.outlets {
                if !u.produce.contains_key(r) || !u.sends.contains(r) {
                    return err(format!("unit {}: outlet {r} must be produced and sent", u.id));
                }
                if !(w.is_finite() && *w > 0.0) {
                    return err(format!("unit {}: outlet {r} must weigh more than 0", u.id));
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
            if let Some(u) = &z.builder_start {
                if !z.ladder {
                    return err(format!("zone {}: builder_start needs ladder = true", z.id));
                }
                if self.unit(u).is_none() {
                    return err(format!("zone {}: unknown builder_start unit {u}", z.id));
                }
                if self.zones.iter().any(|o| o.densities.iter().any(|d| d.units.contains(u))) {
                    return err(format!("zone {}: builder_start unit {u} is already listed by a density", z.id));
                }
            }
        }
        for (tool, unit) in &self.services {
            if self.unit(unit).is_none() {
                return err(format!("service {tool}: unknown unit {unit}"));
            }
        }
        for t in &self.techs {
            for u in t.obsoletes.iter().flat_map(|(a, b)| [a, b]) {
                if self.unit(u).is_none() {
                    return err(format!("tech {}: obsoletes names the unknown unit {u}", t.id));
                }
            }
        }
        for p in &self.district_policies {
            if let Some(r) = p.bonus.keys().find(|r| self.resource(r).is_none()) {
                return err(format!("neighbourhood policy {}: unknown resource {r}", p.id));
            }
            if let Some(l) = p.emits.keys().find(|l| self.layer(l).is_none()) {
                return err(format!("neighbourhood policy {}: unknown map {l}", p.id));
            }
        }
        if self.district_policies.len() > 250 {
            return err("at most 250 neighbourhood policies".to_string());
        }
        for e in &self.events {
            let effects = std::iter::once(&e.effect).chain(e.choices.iter().flat_map(|c| [&c.effect, &c.else_effect]));
            for u in e.requires.iter().chain(effects.flat_map(|f| f.obsoletes.iter().flat_map(|(a, b)| [a, b]).chain(f.services.values()))) {
                if self.unit(u).is_none() {
                    return err(format!("event {}: unknown unit {u}", e.id));
                }
            }
            if e.choices.iter().any(|c| !(0.0..=1.0).contains(&c.chance)) {
                return err(format!("event {}: chance is a share between 0 and 1", e.id));
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
        for d in &self.disasters {
            if let Some(l) = d.protection.as_ref().filter(|l| self.layer(l).is_none()) {
                return err(format!("disaster {}: unknown protection layer {l}", d.id));
            }
            if let Some(u) = d.source.as_ref().filter(|u| self.unit(u).is_none()) {
                return err(format!("disaster {}: unknown source unit {u}", d.id));
            }
            if let Some(f) = d.fallout.as_ref().filter(|f| self.layer(&f.layer).is_none()) {
                return err(format!("disaster {}: fallout on unknown layer {}", d.id, f.layer));
            }
            if !(d.full.is_finite() && d.full > 0.0) {
                return err(format!("disaster {}: full must be above 0", d.id));
            }
            if !((0.0..=1.0).contains(&d.spread) && (0.0..=1.0).contains(&d.relief)) {
                return err(format!("disaster {}: spread and relief are chances, between 0 and 1", d.id));
            }
        }
        for t in &self.taxes {
            let at = |m: String| err(format!("tax {}: {m}", t.id));
            match self.resource(&t.resource) {
                Some(i) if self.resources[i].transport == Transport::City => {}
                _ => return at(format!("{} is not a city resource", t.resource)),
            }
            if let Some(z) = t.zones.iter().find(|z| !self.zones.iter().any(|d| &d.id == *z)) {
                return at(format!("unknown zone {z}"));
            }
            let [lo, hi] = t.range;
            if !(lo.is_finite() && hi.is_finite() && 0.0 <= lo && lo <= t.rate && t.rate <= hi && t.rate > 0.0) {
                return at("needs 0 <= range[0] <= rate <= range[1], and a rate above 0".into());
            }
            if self.taxes.iter().filter(|o| o.id == t.id).count() > 1 {
                return at("defined twice".into());
            }
        }
        for p in &self.policies {
            let at = |m: String| err(format!("policy {}: {m}", p.id));
            if p.stances.is_empty() {
                return at("has no stances".into());
            }
            if self.policies.iter().filter(|o| o.id == p.id).count() > 1 {
                return at("defined twice".into());
            }
            for st in &p.stances {
                if p.stances.iter().filter(|o| o.id == st.id).count() > 1 {
                    return at(format!("stance {} defined twice", st.id));
                }
                if let Some(q) = st.requires.as_ref().filter(|q| self.tech(q).is_none()) {
                    return at(format!("stance {} requires unknown project {q}", st.id));
                }
                if let Some(r) = st.bonus.keys().find(|r| self.resource(r).is_none()) {
                    return at(format!("stance {}: bonus to unknown resource {r}", st.id));
                }
                if let Some(z) = st.demand.keys().find(|z| !self.zones.iter().any(|d| &d.id == *z)) {
                    return at(format!("stance {}: demand for unknown zone {z}", st.id));
                }
                if let Some(k) = st.kinds.keys().find(|k| self.resource(k).is_none_or(|i| self.resources[i].parent.is_none())) {
                    return at(format!("stance {}: {k} is not a kind of a resource", st.id));
                }
                if st.kinds.values().any(|s| !s.is_finite() || *s < 0.0) {
                    return at(format!("stance {}: shares of kinds cannot be negative", st.id));
                }
                if let Some(d) = st.disasters.keys().find(|d| self.disaster(d).is_none()) {
                    return at(format!("stance {}: unknown disaster {d}", st.id));
                }
                if let Some(l) = st.emits.keys().find(|l| self.layer(l).is_none()) {
                    return at(format!("stance {}: emits on unknown layer {l}", st.id));
                }
                if let Some(l) = st.traffic.keys().find(|l| self.layer(l).is_none()) {
                    return at(format!("stance {}: traffic on unknown layer {l}", st.id));
                }
                if !(st.satisfaction.is_finite() && st.cost.is_finite() && st.bonus.values().all(|b| b.is_finite() && *b > -1.0)) {
                    return at(format!("stance {}: numbers out of range", st.id));
                }
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
        "solar_plant" => SolarPlant,
        "fusion_plant" => FusionPlant,
        "wind_turbine" => WindTurbine,
        "waterworks" => Waterworks,
        "sewage_outfall" => SewageOutfall,
        "wetland" => Wetland,
        "resort_hotel" => ResortHotel,
        "observation_tower" => ObservationTower,
        "amusement_park" => AmusementPark,
        "mayors_office" => MayorsOffice,
        "city_hall" => CityHall,
        "neighbourhood_office" => NeighbourhoodOffice,
        "park" => Park,
        "fire_station" => FireDept,
        "police" => PoliceDept,
        "hospital" => Hospital,
        "school" => School,
        "high_school" => HighSchool,
        "university" => University,
        "stadium" => Stadium,
        "mall" => Mall,
        "museum" => Museum,
        "modern_art_museum" => ModernArtMuseum,
        "opera_house" => OperaHouse,
        "archaeology_museum" => ArchaeologyMuseum,
        "water_pipe" => WaterPipe,
        "pumping_station" => PumpingStation,
        "water_tower" => WaterTower,
        "water_treatment" => WaterTreatment,
        "dump" => Dump,
        "recycling" => Recycling,
        "incinerator" => Incinerator,
        "bus_stop" => BusStop,
        "tram_stop" => TramStop,
        "metro_station" => MetroStation,
        _ => return None,
    })
}
