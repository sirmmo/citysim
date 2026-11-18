//! Transit routes drawn by the player (opt-in).
//!
//! Left alone, every stop reaches every other stop on its network: bus
//! stops on one street network, tram stops on one run of tram street,
//! metro stations on one run of tunnels (see [`crate::rail`]). With
//! [`Transit::manual`] on, people only ride between stops that a route
//! joins. A route is a list of stops of one kind, served back and forth
//! (or round, when circular) by the vehicles the player gives it: more
//! vehicles are a shorter wait and a bigger bill. Changing between routes
//! of one kind at a shared stop works, at the price of the second wait.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::glassbox::{BUS_SPEED, METRO_SPEED, TRAM_SPEED};
use crate::rail::Railways;
use crate::{City, StructureCode};

/// Boarding and leaving, in street tiles of time, however many vehicles run.
pub const WAIT_MIN: f32 = 1.0;
/// One vehicle on a round trip of this many tiles (at speed 1) adds one
/// tile of time to the wait.
pub const HEADWAY: f32 = 8.0;
/// Vehicles on one route at most.
pub const MAX_VEHICLES: u32 = 20;
/// Stops on one route at most.
pub const MAX_STOPS: usize = 40;
/// A stop this full (of what its vehicles take) counts as full.
pub const FULL: f32 = 0.95;

/// What runs on a route.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Bus,
    Tram,
    Metro,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Bus, Mode::Tram, Mode::Metro];

    pub fn id(self) -> &'static str {
        match self {
            Mode::Bus => "bus",
            Mode::Tram => "tram",
            Mode::Metro => "metro",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Bus => "Bus",
            Mode::Tram => "Tram",
            Mode::Metro => "Metro",
        }
    }

    /// The building a route of this kind stops at.
    pub fn stop(self) -> StructureCode {
        match self {
            Mode::Bus => StructureCode::BusStop,
            Mode::Tram => StructureCode::TramStop,
            Mode::Metro => StructureCode::MetroStation,
        }
    }

    pub fn speed(self) -> f32 {
        match self {
            Mode::Bus => BUS_SPEED,
            Mode::Tram => TRAM_SPEED,
            Mode::Metro => METRO_SPEED,
        }
    }

    /// What one vehicle costs to run for a month.
    /// Riders a vehicle takes in a tick at any one stop, unless the rules
    /// say otherwise (`seats` on the line's vehicle).
    pub fn seats(self) -> f32 {
        match self {
            Mode::Bus => 6.0,
            Mode::Tram => 15.0,
            Mode::Metro => 40.0,
        }
    }

    pub fn vehicle_cost(self) -> i64 {
        match self {
            Mode::Bus => 15,
            Mode::Tram => 25,
            Mode::Metro => 50,
        }
    }

    /// The stops and networks of this kind.
    pub fn network(self, city: &City) -> Railways {
        match self {
            Mode::Bus => Railways::buses(&city.layer, &city.terrain, city.footprints()),
            Mode::Tram => Railways::trams(&city.layer, &city.terrain, city.footprints()),
            Mode::Metro => Railways::metro(&city.layer, &city.terrain, city.footprints()),
        }
    }
}

/// One route: its stops in the order they are served.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub id: u32,
    pub name: String,
    pub mode: Mode,
    /// Origin tiles of the stop buildings.
    pub stops: Vec<(u32, u32)>,
    pub vehicles: u32,
    /// Round and round instead of there and back.
    #[serde(default)]
    pub circular: bool,
}

/// The routes of a city.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Transit {
    /// Stops are joined by routes only.
    #[serde(default)]
    pub manual: bool,
    #[serde(default)]
    pub routes: Vec<Route>,
    #[serde(default)]
    next_id: u32,
    /// People boarding or leaving at each stop per tick, averaged over
    /// some ticks.
    #[serde(skip)]
    pub riders: BTreeMap<(u32, u32), f32>,
    /// How full each stop is, 0..1: the riders its vehicles took of what
    /// they can take in a tick, averaged over some ticks (drawn routes only).
    #[serde(skip)]
    pub load: BTreeMap<(u32, u32), f32>,
    /// Counts the edits, for whoever draws the routes.
    #[serde(skip)]
    pub rev: u32,
}

/// From one platform tile to another.
pub(crate) type Leg = ((u32, u32), (u32, u32));

/// What a route looks like on today's map.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteInfo {
    /// Tiles a vehicle covers before it is back where it started.
    pub round_trip: u32,
    /// Time to board, in street tiles; `None` when nothing runs.
    pub wait: Option<f32>,
    /// Stops that are gone or have no street, track or tunnel beside them.
    pub missing: Vec<(u32, u32)>,
    /// Places in `stops` whose way to the next stop is not joined.
    pub broken: Vec<usize>,
    /// People boarding or leaving at its stops per tick; a stop shared by
    /// several routes counts its share.
    pub riders: f32,
    /// Riders its vehicles take at a stop in a tick.
    pub room: f32,
    /// How full its fullest stop is, 0..1, and how many of its stops are full.
    pub load: f32,
    pub full_stops: usize,
    /// The bill for a month.
    pub cost: i64,
}

/// A route as its vehicles run it on today's map, for timetables and data
/// exports (see [`City::services`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    pub id: u32,
    pub name: String,
    pub mode: Mode,
    pub vehicles: u32,
    /// Round and round: after the last stop comes the first.
    pub ring: bool,
    /// The stops served, in order: the stop building's origin tile and the
    /// tile its vehicles halt on.
    pub stops: Vec<((u32, u32), (u32, u32))>,
    /// The tiles driven from each stop to the next (and from the last back
    /// to the first on a ring).
    pub legs: Vec<Vec<(u32, u32)>>,
}

/// A railway line or a ferry crossing as the city runs it: its stations or
/// harbours in order, and the way between them (see [`City::trunk_lines`]).
#[derive(Clone, Debug, PartialEq)]
pub struct TrunkLine {
    /// By boat, between harbours; otherwise by train, between stations.
    pub ferry: bool,
    /// The building's origin tile and the tile its vehicles halt on.
    pub stops: Vec<((u32, u32), (u32, u32))>,
    /// The tiles from each stop to the next.
    pub legs: Vec<Vec<(u32, u32)>>,
}

/// The stops of each line of a network that no route in `served` reaches,
/// in an order a route can take: from the stop furthest out, always on to
/// the nearest.
fn undrawn(net: &Railways, served: &[u32]) -> Vec<Vec<(u32, u32)>> {
    let mut lines: Vec<u32> = net.stations.iter().map(|s| s.line).filter(|l| !served.contains(l)).collect();
    lines.sort_unstable();
    lines.dedup();
    let mut out = Vec::new();
    for line in lines {
        let mut left: Vec<&crate::rail::Station> = net.stations.iter().filter(|s| s.line == line).collect();
        if left.len() < 2 {
            continue;
        }
        let (cx, cy) = left.iter().fold((0, 0), |(x, y), s| (x + s.platform.0, y + s.platform.1));
        let (cx, cy) = (cx / left.len() as u32, cy / left.len() as u32);
        let far = |s: &&crate::rail::Station| s.platform.0.abs_diff(cx) + s.platform.1.abs_diff(cy);
        let first = (0..left.len()).max_by_key(|&i| (far(&left[i]), std::cmp::Reverse(left[i].origin))).unwrap();
        let mut at = left.swap_remove(first);
        let mut stops = vec![at.origin];
        while !left.is_empty() && stops.len() < MAX_STOPS {
            let d = |s: &&crate::rail::Station| s.platform.0.abs_diff(at.platform.0) + s.platform.1.abs_diff(at.platform.1);
            let next = (0..left.len()).min_by_key(|&i| (d(&left[i]), left[i].origin)).unwrap();
            at = left.swap_remove(next);
            stops.push(at.origin);
        }
        out.push(stops);
    }
    out
}

/// The vehicles that keep a route's wait about what it is when routes run
/// themselves.
fn fair_vehicles(mode: Mode, round_trip: u32) -> u32 {
    ((round_trip as f32 / (mode.speed() * HEADWAY * 2.0)).ceil() as u32).clamp(1, MAX_VEHICLES)
}

/// A route's stops as stations of its network, and the legs between them.
struct Run {
    /// Station numbers of the stops that exist, with their place in `stops`.
    seq: Vec<(usize, usize)>,
    /// Tiles from each of `seq` to the next (and from the last back to the
    /// first, when circular); `None` when the two are not joined.
    legs: Vec<Option<u32>>,
    /// Round and round, with every leg there.
    ring: bool,
    round_trip: u32,
}

impl Route {
    fn run(&self, net: &Railways) -> Run {
        let seq: Vec<(usize, usize)> =
            self.stops.iter().enumerate().filter_map(|(k, s)| net.stations.iter().position(|st| st.origin == *s).map(|i| (i, k))).collect();
        let leg = |a: usize, b: usize| {
            let (x, y) = (&net.stations[a], &net.stations[b]);
            (x.line == y.line).then(|| x.platform.0.abs_diff(y.platform.0) + x.platform.1.abs_diff(y.platform.1))
        };
        let mut legs: Vec<Option<u32>> = seq.windows(2).map(|w| leg(w[0].0, w[1].0)).collect();
        let ring = self.circular && seq.len() > 2 && legs.iter().all(Option::is_some) && leg(seq[seq.len() - 1].0, seq[0].0).is_some();
        if ring {
            legs.push(leg(seq[seq.len() - 1].0, seq[0].0));
        }
        let total: u32 = legs.iter().flatten().sum();
        Run { seq, legs, ring, round_trip: if ring { total } else { 2 * total } }
    }

    /// Time to board with the vehicles it has; `None` when nothing runs.
    fn wait(&self, round_trip: u32) -> Option<f32> {
        (self.vehicles > 0 && round_trip > 0).then(|| WAIT_MIN + round_trip as f32 / (self.mode.speed() * self.vehicles as f32 * HEADWAY))
    }
}

/// Riding times between the stops of one kind, over the routes.
pub(crate) struct Table {
    n: usize,
    /// Wait and ride from station to station, in street tiles of time.
    time: Vec<f32>,
    /// The stations a station reaches share a group; `None` off every route.
    pub group: Vec<Option<u32>>,
}

impl Table {
    pub fn time(&self, a: usize, b: usize) -> f32 {
        self.time[a * self.n + b]
    }
}

impl Transit {
    pub fn is_default(&self) -> bool {
        !self.manual && self.routes.is_empty()
    }

    pub fn route(&self, id: u32) -> Option<&Route> {
        self.routes.iter().find(|r| r.id == id)
    }

    /// The vehicles' bill for a month (nothing while routes run themselves).
    pub fn cost(&self) -> i64 {
        if !self.manual {
            return 0;
        }
        self.routes.iter().filter(|r| r.stops.len() > 1).map(|r| r.vehicles as i64 * r.mode.vehicle_cost()).sum()
    }

    /// The riding times of one kind; `None` while routes run themselves.
    pub(crate) fn table(&self, mode: Mode, net: &Railways) -> Option<Table> {
        if !self.manual {
            return None;
        }
        let n = net.stations.len();
        let mut time = vec![f32::INFINITY; n * n];
        for route in self.routes.iter().filter(|r| r.mode == mode) {
            let run = route.run(net);
            let Some(wait) = route.wait(run.round_trip) else { continue };
            let m = run.seq.len();
            for i in 0..m {
                // Forward from `i` as far as the legs are joined: once
                // round for a ring, to the end of the line otherwise.
                let mut dist = 0;
                for step in 0..(if run.ring { m - 1 } else { m - 1 - i }) {
                    let Some(d) = run.legs[(i + step) % run.legs.len()] else { break };
                    dist += d;
                    let j = (i + step + 1) % m;
                    let ride = if run.ring { dist.min(run.round_trip - dist) } else { dist };
                    let t = wait + ride as f32 / mode.speed();
                    let (a, b) = (run.seq[i].0, run.seq[j].0);
                    if a != b {
                        time[a * n + b] = time[a * n + b].min(t);
                        time[b * n + a] = time[b * n + a].min(t);
                    }
                }
            }
        }
        // Changing routes at a shared stop: the second wait is in the
        // second route's times already.
        for k in 0..n {
            for a in 0..n {
                let ak = time[a * n + k];
                if !ak.is_finite() {
                    continue;
                }
                for b in 0..n {
                    let t = ak + time[k * n + b];
                    if a != b && t < time[a * n + b] {
                        time[a * n + b] = t;
                    }
                }
            }
        }
        let group = (0..n)
            .map(|a| (0..n).find(|&b| a == b || time[a * n + b].is_finite()).filter(|_| (0..n).any(|b| time[a * n + b].is_finite())).map(|b| b as u32))
            .collect();
        Some(Table { n, time, group })
    }

    /// Where the vehicles of one kind drive (from platform to platform,
    /// both ways) and how many there are.
    pub(crate) fn legs(&self, mode: Mode, net: &Railways) -> (Vec<Leg>, usize) {
        let mut legs = Vec::new();
        let mut vehicles = 0;
        for r in self.routes.iter().filter(|r| r.mode == mode && r.vehicles > 0) {
            let run = r.run(net);
            let m = run.seq.len();
            let before = legs.len();
            for (i, _) in run.legs.iter().enumerate().filter(|(_, l)| l.is_some()) {
                let (a, b) = (net.stations[run.seq[i].0].platform, net.stations[run.seq[(i + 1) % m].0].platform);
                legs.extend([(a, b), (b, a)]);
            }
            if legs.len() > before {
                vehicles += r.vehicles as usize;
            }
        }
        (legs, vehicles)
    }

    /// What the vehicles calling at each station of a network can take in
    /// a tick: for each route through it, its vehicles times their seats.
    /// `None` while routes run themselves (there is always room then).
    pub(crate) fn room(&self, mode: Mode, net: &Railways, seats: f32) -> Option<Vec<f32>> {
        if !self.manual {
            return None;
        }
        let mut room = vec![0.0; net.stations.len()];
        for r in self.routes.iter().filter(|r| r.mode == mode && r.vehicles > 0) {
            let mut at: Vec<usize> = r.run(net).seq.iter().map(|(i, _)| *i).collect();
            at.sort_unstable();
            at.dedup();
            for i in at {
                room[i] += r.vehicles as f32 * seats;
            }
        }
        Some(room)
    }

    /// Takes how full the stops were this tick into the averages.
    pub(crate) fn count_load(&mut self, counted: BTreeMap<(u32, u32), f32>) {
        for v in self.load.values_mut() {
            *v *= 0.8;
        }
        for (stop, share) in counted {
            *self.load.entry(stop).or_default() += 0.2 * share;
        }
        self.load.retain(|_, v| *v > 0.001);
    }

    /// Riders per tick on each route (a stop on several routes shares its
    /// riders between them), or all riders together while routes run themselves.
    pub fn ridership(&self) -> Vec<(String, f32)> {
        if !self.manual {
            // Every ride is counted where it starts and where it ends.
            return vec![("All lines".to_string(), self.riders.values().sum::<f32>() / 2.0)];
        }
        let mut on: BTreeMap<(u32, u32), f32> = BTreeMap::new();
        let stops_of = |r: &Route| {
            let mut s = r.stops.clone();
            s.sort_unstable();
            s.dedup();
            s
        };
        for r in &self.routes {
            for s in stops_of(r) {
                *on.entry(s).or_default() += 1.0;
            }
        }
        self.routes.iter().map(|r| (r.name.clone(), stops_of(r).iter().map(|s| self.riders.get(s).copied().unwrap_or(0.0) / on[s]).sum())).collect()
    }

    /// Takes the people counted at the stops this tick into the averages.
    pub(crate) fn count_riders(&mut self, counted: BTreeMap<(u32, u32), f32>) {
        for v in self.riders.values_mut() {
            *v *= 0.8;
        }
        for (stop, n) in counted {
            *self.riders.entry(stop).or_default() += 0.2 * n;
        }
        self.riders.retain(|_, v| *v > 0.01);
    }
}

impl City {
    pub fn transit(&self) -> Option<&Transit> {
        self.glassbox.as_ref().map(|g| &g.transit)
    }

    fn transit_mut(&mut self) -> Result<&mut Transit, String> {
        let t = &mut self.glassbox.as_mut().ok_or("routes need GlassBox rules")?.transit;
        t.rev = t.rev.wrapping_add(1);
        Ok(t)
    }

    fn route_mut(&mut self, id: u32) -> Result<&mut Route, String> {
        self.transit_mut()?.routes.iter_mut().find(|r| r.id == id).ok_or_else(|| format!("no route {id}"))
    }

    /// Whether the player draws the routes.
    pub fn manual_routes(&self) -> bool {
        self.transit().is_some_and(|t| t.manual)
    }

    /// Hands the routes to the player, or back to the city. The first time,
    /// with no routes drawn yet, the city's own are written down so that
    /// nothing stops running.
    pub fn set_manual_routes(&mut self, on: bool) -> Result<(), String> {
        if on && self.transit().is_some_and(|t| t.routes.is_empty()) {
            self.draw_routes();
        }
        self.transit_mut()?.manual = on;
        Ok(())
    }

    /// Adds a route for every network with two stops or more that has
    /// none: its stops from one end to the other, with the vehicles that
    /// keep the wait about what it is when routes run themselves. Returns
    /// how many were added.
    pub fn draw_routes(&mut self) -> usize {
        let mut added = Vec::new();
        for mode in Mode::ALL {
            let net = mode.network(self);
            let served: Vec<u32> = self
                .transit()
                .map(|t| t.routes.iter().filter(|r| r.mode == mode).flat_map(|r| r.run(&net).seq).map(|(i, _)| net.stations[i].line).collect())
                .unwrap_or_default();
            added.extend(undrawn(&net, &served).into_iter().map(|stops| (mode, stops)));
        }
        let n = added.len();
        for (mode, stops) in added {
            let Ok(id) = self.add_route(mode) else { continue };
            let net = mode.network(self);
            if let Ok(r) = self.route_mut(id) {
                r.stops = stops;
                r.vehicles = fair_vehicles(mode, r.run(&net).round_trip);
            }
        }
        n
    }

    /// A new, empty route with one vehicle; returns its id.
    pub fn add_route(&mut self, mode: Mode) -> Result<u32, String> {
        let t = self.transit_mut()?;
        t.next_id += 1;
        let id = t.next_id;
        let nth = t.routes.iter().filter(|r| r.mode == mode).count() + 1;
        t.routes.push(Route { id, name: format!("{} {nth}", mode.name()), mode, stops: Vec::new(), vehicles: 1, circular: false });
        Ok(id)
    }

    pub fn delete_route(&mut self, id: u32) -> Result<(), String> {
        let t = self.transit_mut()?;
        let before = t.routes.len();
        t.routes.retain(|r| r.id != id);
        if t.routes.len() == before {
            return Err(format!("no route {id}"));
        }
        Ok(())
    }

    /// Adds the stop on a tile to the end of a route. The stop must be of
    /// the route's kind, not the one just before, and joined to the one
    /// before (one street network, tram line or tunnel).
    pub fn route_add_stop(&mut self, id: u32, tile: (u32, u32)) -> Result<(), String> {
        let route = self.transit().and_then(|t| t.route(id)).ok_or_else(|| format!("no route {id}"))?.clone();
        let what = match route.mode {
            Mode::Bus => "bus stop",
            Mode::Tram => "tram stop",
            Mode::Metro => "metro station",
        };
        let origin = self
            .layer
            .origin_of(tile.0, tile.1)
            .filter(|o| self.layer.structure_at(o.0, o.1).is_some_and(|s| s.code == route.mode.stop()))
            .ok_or_else(|| format!("that is not a {what}"))?;
        if route.stops.len() >= MAX_STOPS {
            return Err(format!("a route has {MAX_STOPS} stops at most"));
        }
        if route.stops.last() == Some(&origin) {
            return Err("that stop is the route's last one already".into());
        }
        let net = route.mode.network(self);
        let station = net.stations.iter().find(|s| s.origin == origin).ok_or(match route.mode {
            Mode::Bus => "that bus stop has no street beside it",
            Mode::Tram => "that tram stop has no tram street beside it",
            Mode::Metro => "that metro station has no tunnel under or beside it",
        })?;
        if let Some(last) = route.stops.last().and_then(|l| net.stations.iter().find(|s| s.origin == *l)) {
            if last.line != station.line {
                return Err(match route.mode {
                    Mode::Bus => "no street joins that stop to the route's last one",
                    Mode::Tram => "no tram street joins that stop to the route's last one",
                    Mode::Metro => "no tunnel joins that station to the route's last one",
                }
                .into());
            }
        }
        self.route_mut(id)?.stops.push(origin);
        Ok(())
    }

    /// Takes the stop at a place in the list off a route.
    pub fn route_remove_stop(&mut self, id: u32, index: usize) -> Result<(), String> {
        let r = self.route_mut(id)?;
        if index >= r.stops.len() {
            return Err(format!("the route has {} stops", r.stops.len()));
        }
        r.stops.remove(index);
        Ok(())
    }

    pub fn set_route_vehicles(&mut self, id: u32, vehicles: u32) -> Result<(), String> {
        self.route_mut(id)?.vehicles = vehicles.min(MAX_VEHICLES);
        Ok(())
    }

    pub fn set_route_circular(&mut self, id: u32, circular: bool) -> Result<(), String> {
        self.route_mut(id)?.circular = circular;
        Ok(())
    }

    pub fn rename_route(&mut self, id: u32, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("a route needs a name".into());
        }
        self.route_mut(id)?.name = name.chars().take(40).collect();
        Ok(())
    }

    /// Every route with what it looks like on today's map, in list order.
    pub fn routes(&self) -> Vec<(&Route, RouteInfo)> {
        let Some(t) = self.transit() else { return Vec::new() };
        let nets: Vec<(Mode, Railways)> = Mode::ALL.into_iter().filter(|m| t.routes.iter().any(|r| r.mode == *m)).map(|m| (m, m.network(self))).collect();
        // A stop on several routes shares its riders between them.
        let mut on: BTreeMap<(u32, u32), f32> = BTreeMap::new();
        for r in &t.routes {
            let mut stops = r.stops.clone();
            stops.sort_unstable();
            stops.dedup();
            for s in stops {
                *on.entry(s).or_default() += 1.0;
            }
        }
        t.routes
            .iter()
            .map(|r| {
                let net = &nets.iter().find(|(m, _)| *m == r.mode).unwrap().1;
                let run = r.run(net);
                let missing = r.stops.iter().filter(|s| !net.stations.iter().any(|st| st.origin == **s)).copied().collect();
                let broken = run.legs.iter().enumerate().filter(|(_, l)| l.is_none()).map(|(i, _)| run.seq[i].1).collect();
                let mut stops = r.stops.clone();
                stops.sort_unstable();
                stops.dedup();
                let riders = stops.iter().map(|s| t.riders.get(s).copied().unwrap_or(0.0) / on[s]).sum();
                let cost = if r.stops.len() > 1 { r.vehicles as i64 * r.mode.vehicle_cost() } else { 0 };
                let loads: Vec<f32> = stops.iter().map(|s| t.load.get(s).copied().unwrap_or(0.0)).collect();
                let (load, full_stops) = (loads.iter().copied().fold(0.0, f32::max), loads.iter().filter(|l| **l >= FULL).count());
                let room = r.vehicles as f32 * self.rules().and_then(|x| x.line_seats(r.mode.id())).unwrap_or(r.mode.seats());
                (r, RouteInfo { round_trip: run.round_trip, wait: r.wait(run.round_trip), missing, broken, riders, room, load, full_stops, cost })
            })
            .collect()
    }

    /// The tiles a route's vehicles pass, leg by leg, along its streets,
    /// tracks or tunnels (for drawing).
    pub fn route_path(&self, id: u32) -> Vec<Vec<(u32, u32)>> {
        let Some(r) = self.transit().and_then(|t| t.route(id)) else { return Vec::new() };
        let net = r.mode.network(self);
        let run = r.run(&net);
        let m = run.seq.len();
        (0..run.legs.len())
            .filter_map(|i| net.route(self.width(), net.stations[run.seq[i].0].platform, net.stations[run.seq[(i + 1) % m].0].platform))
            .collect()
    }

    /// What runs today, route by route: the routes the player drew, or the
    /// ones the city runs by itself (one along each street, track or tunnel
    /// with two stops or more). Routes with no vehicle or fewer than two
    /// joined stops are left out; a route broken in the middle runs up to
    /// the break.
    pub fn services(&self) -> Vec<Service> {
        let Some(t) = self.transit() else { return Vec::new() };
        let nets: Vec<(Mode, Railways)> = Mode::ALL.into_iter().map(|m| (m, m.network(self))).collect();
        let net_of = |m: Mode| &nets.iter().find(|(k, _)| *k == m).unwrap().1;
        let mut routes = Vec::new();
        if t.manual {
            routes.clone_from(&t.routes);
        } else {
            for (mode, net) in &nets {
                for (n, stops) in undrawn(net, &[]).into_iter().enumerate() {
                    let mut r =
                        Route { id: routes.len() as u32 + 1, name: format!("{} {}", mode.name(), n + 1), mode: *mode, stops, vehicles: 1, circular: false };
                    r.vehicles = fair_vehicles(*mode, r.run(net).round_trip);
                    routes.push(r);
                }
            }
        }
        let mut out = Vec::new();
        for r in routes.into_iter().filter(|r| r.vehicles > 0) {
            let net = net_of(r.mode);
            let run = r.run(net);
            let joined = if run.ring { run.seq.len() } else { 1 + run.legs.iter().take_while(|l| l.is_some()).count() };
            if run.seq.len() < 2 || joined < 2 {
                continue;
            }
            let at = |i: usize| &net.stations[run.seq[i].0];
            let stops: Vec<_> = (0..joined).map(|i| (at(i).origin, at(i).platform)).collect();
            let hops = if run.ring { joined } else { joined - 1 };
            let legs: Option<Vec<_>> = (0..hops).map(|i| net.route(self.width(), at(i).platform, at((i + 1) % joined).platform)).collect();
            let Some(legs) = legs else { continue };
            out.push(Service { id: r.id, name: r.name, mode: r.mode, vehicles: r.vehicles, ring: run.ring, stops, legs });
        }
        out
    }

    /// The railway lines and ferry crossings: one for each track or body of
    /// water with two stations or harbours or more, from one end to the other.
    pub fn trunk_lines(&self) -> Vec<TrunkLine> {
        let nets =
            [(false, Railways::find(&self.layer, &self.terrain, self.footprints())), (true, Railways::water(&self.layer, &self.terrain, self.footprints()))];
        let mut out = Vec::new();
        for (ferry, net) in &nets {
            for origins in undrawn(net, &[]) {
                let stations: Vec<&crate::rail::Station> = origins.iter().filter_map(|o| net.stations.iter().find(|s| s.origin == *o)).collect();
                let legs: Vec<Vec<(u32, u32)>> = stations.windows(2).map_while(|p| net.route(self.width(), p[0].platform, p[1].platform)).collect();
                if legs.is_empty() {
                    continue;
                }
                let stops = stations.iter().take(legs.len() + 1).map(|s| (s.origin, s.platform)).collect();
                out.push(TrunkLine { ferry: *ferry, stops, legs });
            }
        }
        out
    }

    /// Each month: stops that were pulled down leave their routes, and the
    /// vehicles are paid for.
    pub(crate) fn transit_month(&mut self) {
        let Some(t) = self.transit() else { return };
        if t.routes.is_empty() {
            return;
        }
        let gone: Vec<(u32, (u32, u32))> = t
            .routes
            .iter()
            .flat_map(|r| r.stops.iter().filter(|s| self.layer.structure_at(s.0, s.1).is_none_or(|b| b.code != r.mode.stop())).map(|s| (r.id, *s)))
            .collect();
        let cost = t.cost();
        if !gone.is_empty() {
            if let Ok(t) = self.transit_mut() {
                for (id, stop) in &gone {
                    if let Some(r) = t.routes.iter_mut().find(|r| r.id == *id) {
                        r.stops.retain(|s| s != stop);
                    }
                }
            }
        }
        if cost > 0 {
            self.fund -= cost;
            self.book("Transit vehicles", -(cost as f64));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Footprints, Terrain, Tool};

    /// Three bus stops along one street, at w 4, 20 and 36.
    fn town() -> City {
        let mut c = City::new(Terrain::flat(48, 16), None, Footprints::builtin(), 1);
        c.fund = 100_000;
        let dir = format!("{}/../../assets/rules", env!("CARGO_MANIFEST_DIR"));
        let mut r: crate::rules::Ruleset = toml::from_str(&std::fs::read_to_string(format!("{dir}/glassbox.toml")).unwrap()).unwrap();
        for script in r.scripts.clone() {
            crate::gbx::compile(&mut r, &script, &std::fs::read_to_string(format!("{dir}/{script}")).unwrap()).unwrap();
        }
        c.use_rules(std::sync::Arc::new(r));
        c.research_all();
        c.apply_tool(Tool::Road, (2, 8), (44, 8)).unwrap();
        for w in [4, 20, 36] {
            c.apply_tool(Tool::BusStop, (w, 9), (w, 9)).unwrap();
        }
        c
    }

    fn times(c: &City) -> (Table, Railways) {
        let net = Mode::Bus.network(c);
        (c.transit().unwrap().table(Mode::Bus, &net).unwrap(), net)
    }

    fn at(net: &Railways, w: u32) -> usize {
        net.stations.iter().position(|s| s.origin == (w, 9)).unwrap()
    }

    #[test]
    fn stops_are_joined_by_routes_only() {
        let mut c = town();
        c.set_manual_routes(true).unwrap();
        // The city wrote its own route down: all three stops, end to end.
        let first = c.transit().unwrap().routes[0].clone();
        assert_eq!(first.stops.len(), 3);
        assert!(first.stops[1] == (20, 9), "{:?}", first.stops);
        let (t, net) = times(&c);
        let (a, b, d) = (at(&net, 4), at(&net, 20), at(&net, 36));
        assert!(t.time(a, d).is_finite() && t.time(a, b) < t.time(a, d));
        // Without it, nothing rides.
        c.delete_route(first.id).unwrap();
        let (t, _) = times(&c);
        assert!(!t.time(a, b).is_finite() && t.group[a].is_none());
        // A route between two of the stops leaves the third out.
        let id = c.add_route(Mode::Bus).unwrap();
        c.route_add_stop(id, (4, 9)).unwrap();
        assert!(c.route_add_stop(id, (4, 9)).is_err(), "the same stop twice in a row");
        assert!(c.route_add_stop(id, (10, 8)).is_err(), "a street is not a stop");
        c.route_add_stop(id, (20, 9)).unwrap();
        let (t, _) = times(&c);
        assert!(t.time(a, b).is_finite() && !t.time(a, d).is_finite() && t.group[d].is_none());
        // A second route from there on: a change, and a second wait.
        let direct = {
            let mut c = c.clone();
            c.route_add_stop(id, (36, 9)).unwrap();
            times(&c).0.time(a, d)
        };
        let id2 = c.add_route(Mode::Bus).unwrap();
        c.route_add_stop(id2, (20, 9)).unwrap();
        c.route_add_stop(id2, (36, 9)).unwrap();
        let (t, _) = times(&c);
        assert!(t.time(a, d).is_finite() && t.time(a, d) > direct, "{} against {direct}", t.time(a, d));
        assert_eq!(t.group[a], t.group[d]);
        // Back in the city's hands, the routes are kept but not used.
        c.set_manual_routes(false).unwrap();
        assert!(c.transit().unwrap().table(Mode::Bus, &Mode::Bus.network(&c)).is_none());
        assert_eq!(c.transit().unwrap().cost(), 0);
    }

    #[test]
    fn vehicles_shorten_the_wait_and_cost_money() {
        let mut c = town();
        c.set_manual_routes(true).unwrap();
        let id = c.transit().unwrap().routes[0].id;
        let wait = |c: &City| c.routes()[0].1.wait.unwrap();
        c.set_route_vehicles(id, 1).unwrap();
        let one = wait(&c);
        c.set_route_vehicles(id, 4).unwrap();
        assert!(wait(&c) < one && wait(&c) > WAIT_MIN);
        assert_eq!(c.routes()[0].1.round_trip, 64);
        assert_eq!(c.routes()[0].1.cost, 4 * Mode::Bus.vehicle_cost());
        // Round the three stops instead of there and back: the same tiles.
        c.set_route_circular(id, true).unwrap();
        assert_eq!(c.routes()[0].1.round_trip, 64);
        c.set_route_vehicles(id, 0).unwrap();
        assert!(c.routes()[0].1.wait.is_none());
        assert!(!times(&c).0.time(0, 1).is_finite(), "no vehicles, no rides");
        c.set_route_vehicles(id, 4).unwrap();
        // The month's bill, and a stop pulled down leaves the route.
        let fund = c.fund;
        c.transit_month();
        assert_eq!(c.fund, fund - 4 * Mode::Bus.vehicle_cost());
        c.apply_tool(Tool::Destroy, (20, 9), (20, 9)).unwrap();
        assert_eq!(c.routes()[0].1.missing, vec![(20, 9)]);
        c.transit_month();
        assert_eq!(c.transit().unwrap().routes[0].stops, vec![(4, 9), (36, 9)]);
        // The route follows the street between its stops.
        let path = c.route_path(id);
        assert_eq!(path.len(), 1, "two stops make no ring");
        assert!(path[0].iter().all(|p| p.1 == 8) && path[0].len() == 33, "{:?}", path[0]);
        // Saved and loaded, the routes are still there.
        let gb = c.glassbox.as_ref().unwrap();
        let back: crate::glassbox::GlassBox = serde_json::from_str(&serde_json::to_string(gb).unwrap()).unwrap();
        assert_eq!(back.transit.routes, gb.transit.routes);
        assert!(back.transit.manual);
    }
}
