//! Road pathfinding and vehicles (legacy `PathFinder`, `Vehicle`, `MovementManager`).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

use crate::{layer::BuildingLayer, Direction, GraphicCode, RoadKind, StructureCode, Terrain};

/// Maximum simultaneous vehicles (legacy `OC_MOVEMENT_MAX`).
pub const MAX_VEHICLES: usize = 100;
/// Milliseconds per animation step; a vehicle needs `traffic + 10` steps per tile.
pub const MS_PER_STEP: f32 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathCost {
    /// Fewest tiles.
    Distance,
    /// Avoid busy roads.
    Traffic,
}

/// Time units to cross one tile of a road of `kind` carrying `traffic`
/// (10 on an empty street; faster kinds cost less, loads over capacity more).
pub fn tile_time(kind: RoadKind, traffic: u8) -> f32 {
    (10.0 + traffic as f32 / kind.capacity()) / kind.speed()
}

/// [`tile_time`] for a vehicle with a top speed (walkers and carts are no
/// quicker on a highway than on a lane).
pub fn vehicle_tile_time(kind: RoadKind, traffic: u8, top_speed: f32) -> f32 {
    (10.0 + traffic as f32 / kind.capacity()) / kind.speed().min(top_speed)
}

/// Shortest road route from `from` to `to`, both included. Dijkstra over road
/// tiles: [`PathCost::Distance`] takes the quickest route on empty roads
/// (highways are faster, dirt roads slower), [`PathCost::Traffic`] also
/// avoids loaded roads.
pub fn find_path(layer: &BuildingLayer, terrain: &Terrain, from: (u32, u32), to: (u32, u32), cost: PathCost) -> Option<Vec<(u32, u32)>> {
    find_route(layer, terrain, from, to, cost, &Ways::ALL, f32::INFINITY)
}

/// Road kinds a vehicle may drive on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ways(u8);

impl Ways {
    /// Every kind of road. Rail is not a road: only vehicles that name it
    /// (`ways = ["rail"]`) run on track.
    pub const ALL: Ways = Ways(0b11_1111 & !(1 << RoadKind::Rail as u8));
    pub const RAIL: Ways = Ways(1 << RoadKind::Rail as u8);
    /// Open water: boats sail between harbours (no road kind).
    pub const WATER: Ways = Ways(1 << 7);

    pub const fn sails(self) -> bool {
        self.0 & Self::WATER.0 != 0
    }

    pub const fn union(self, other: Ways) -> Ways {
        Ways(self.0 | other.0)
    }
    pub const NONE: Ways = Ways(0);

    pub fn of(kinds: impl IntoIterator<Item = RoadKind>) -> Self {
        Ways(kinds.into_iter().fold(0, |m, k| m | 1 << k as u8))
    }

    pub const fn allows(self, kind: RoadKind) -> bool {
        self.0 & (1 << kind as u8) != 0
    }
}

impl Default for Ways {
    fn default() -> Self {
        Self::ALL
    }
}

/// [`find_path`] for a vehicle that only drives on `ways` and never goes
/// faster than `top_speed` (in road speed units: a street is 1).
pub fn find_route(
    layer: &BuildingLayer,
    terrain: &Terrain,
    from: (u32, u32),
    to: (u32, u32),
    cost: PathCost,
    ways: &Ways,
    top_speed: f32,
) -> Option<Vec<(u32, u32)>> {
    // Road vehicles also cross track at level crossings.
    let crosses = (0..8).any(|k| k != RoadKind::Rail as u8 && ways.0 & (1 << k) != 0);
    let is_road = |w, l| {
        layer
            .structure_at(w, l)
            .is_some_and(|s| s.code == StructureCode::Road && (ways.allows(s.road) || (crosses && s.road.is_rail() && layer.is_level_crossing(w, l))))
    };
    if !is_road(from.0, from.1) || !is_road(to.0, to.1) {
        return None;
    }
    let width = layer.width();
    let idx = |(w, l): (u32, u32)| (l * width + w) as usize;
    let n = (width * layer.length()) as usize;
    let mut dist = vec![u32::MAX; n];
    let mut prev = vec![u32::MAX; n];
    let mut heap = BinaryHeap::new();
    dist[idx(from)] = 0;
    heap.push(Reverse((0u32, from)));
    while let Some(Reverse((d, p))) = heap.pop() {
        if p == to {
            break;
        }
        if d > dist[idx(p)] {
            continue;
        }
        for dir in Direction::ALL {
            let Some(q) = terrain.neighbour(p.0, p.1, dir) else { continue };
            if !is_road(q.0, q.1) {
                continue;
            }
            let s = layer.structure_at(q.0, q.1);
            let kind = s.map_or(RoadKind::Street, |s| s.road);
            let traffic = match cost {
                PathCost::Distance => 0,
                PathCost::Traffic => s.map_or(0, |s| s.traffic),
            };
            let step = (vehicle_tile_time(kind, traffic, top_speed) * 4.0).round() as u32;
            let nd = d + step;
            if nd < dist[idx(q)] {
                dist[idx(q)] = nd;
                prev[idx(q)] = idx(p) as u32;
                heap.push(Reverse((nd, q)));
            }
        }
    }
    if dist[idx(to)] == u32::MAX {
        return None;
    }
    let mut path = vec![to];
    let mut cur = idx(to);
    while cur != idx(from) {
        cur = prev[cur] as usize;
        path.push((cur as u32 % width, cur as u32 / width));
    }
    path.reverse();
    Some(path)
}

/// The quickest routes on empty roads from one road tile to several (both
/// ends included), for any road vehicle; `None` where no road leads.
pub fn find_routes(layer: &BuildingLayer, terrain: &Terrain, from: (u32, u32), to: &[(u32, u32)]) -> Vec<Option<Vec<(u32, u32)>>> {
    let is_road = |w, l| layer.structure_at(w, l).is_some_and(|s| s.code == StructureCode::Road && (!s.road.is_rail() || layer.is_level_crossing(w, l)));
    let width = layer.width();
    let idx = |(w, l): (u32, u32)| (l * width + w) as usize;
    let n = (width * layer.length()) as usize;
    if !is_road(from.0, from.1) {
        return vec![None; to.len()];
    }
    let mut dist = vec![u32::MAX; n];
    let mut prev = vec![u32::MAX; n];
    let mut done = vec![false; n];
    let mut heap = BinaryHeap::new();
    let mut wanted: std::collections::HashSet<usize> = to.iter().filter(|t| is_road(t.0, t.1)).map(|&t| idx(t)).collect();
    dist[idx(from)] = 0;
    heap.push(Reverse((0u32, from)));
    while let Some(Reverse((d, p))) = heap.pop() {
        if d > dist[idx(p)] {
            continue;
        }
        done[idx(p)] = true;
        wanted.remove(&idx(p));
        if wanted.is_empty() {
            break;
        }
        for dir in Direction::ALL {
            let Some(q) = terrain.neighbour(p.0, p.1, dir) else { continue };
            if !is_road(q.0, q.1) {
                continue;
            }
            let kind = layer.structure_at(q.0, q.1).map_or(RoadKind::Street, |s| s.road);
            let nd = d + (tile_time(kind, 0) * 4.0).round() as u32;
            if nd < dist[idx(q)] {
                dist[idx(q)] = nd;
                prev[idx(q)] = idx(p) as u32;
                heap.push(Reverse((nd, q)));
            }
        }
    }
    to.iter()
        .map(|&t| {
            (t.0 < width && idx(t) < n && done[idx(t)]).then(|| {
                let mut path = vec![t];
                let mut cur = idx(t);
                while cur != idx(from) {
                    cur = prev[cur] as usize;
                    path.push((cur as u32 % width, cur as u32 / width));
                }
                path.reverse();
                path
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VehicleKind {
    Car,
    Pickup,
    Sports,
    Walker,
    Handcart,
    HorseCart,
    Bus,
}

impl VehicleKind {
    pub const ALL: [VehicleKind; 7] = [Self::Car, Self::Pickup, Self::Sports, Self::Walker, Self::Handcart, Self::HorseCart, Self::Bus];
    /// The vehicles of rules that name none (and of the classic simulation).
    pub const MOTOR: [VehicleKind; 3] = [Self::Car, Self::Pickup, Self::Sports];

    /// Model id used by rules files (`model = "horse_cart"`).
    pub const fn id(self) -> &'static str {
        match self {
            Self::Car => "car",
            Self::Pickup => "pickup",
            Self::Sports => "sports_car",
            Self::Walker => "walker",
            Self::Handcart => "handcart",
            Self::HorseCart => "horse_cart",
            Self::Bus => "bus",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }

    /// The model from the data files; `None` for those drawn procedurally.
    pub const fn graphic(self) -> Option<GraphicCode> {
        match self {
            Self::Car => Some(GraphicCode::VEHICLE_STD),
            Self::Pickup => Some(GraphicCode::VEHICLE_PICKUP),
            Self::Sports => Some(GraphicCode::VEHICLE_SPORT),
            _ => None,
        }
    }

    /// Pickups take the shortest route, cars avoid traffic.
    pub const fn path_cost(self) -> PathCost {
        match self {
            Self::Pickup => PathCost::Distance,
            _ => PathCost::Traffic,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Waypoint {
    pub w: u32,
    pub l: u32,
    pub height_min: i8,
    pub height_max: i8,
    pub traffic: u8,
    #[serde(default)]
    pub road: RoadKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vehicle {
    pub kind: VehicleKind,
    /// Top speed in road speed units (a street is 1); none for motor vehicles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_speed: Option<f32>,
    /// A boat: it sails on water between harbours.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub water: bool,
    /// `.obj` model it is drawn with (see `VehicleDef::meshes`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<String>,
    /// The public transport line it serves (`"bus"`, `"tram"`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    pub route: Vec<Waypoint>,
    /// Index of the waypoint the vehicle is leaving.
    pub index: usize,
    /// Progress towards the next waypoint, `0..1`.
    pub progress: f32,
}

/// Where to draw a vehicle: tile-space position (centre of the lane) and heading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehiclePose {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub heading: Direction,
}

impl Vehicle {
    pub fn new(kind: VehicleKind, path: &[(u32, u32)], layer: &BuildingLayer, terrain: &Terrain) -> Self {
        let route = path
            .iter()
            .map(|&(w, l)| Waypoint {
                w,
                l,
                height_min: terrain.tile_min(w, l),
                height_max: terrain.tile_max(w, l),
                traffic: layer.structure_at(w, l).map_or(0, |s| s.traffic),
                road: layer.structure_at(w, l).map_or(RoadKind::Street, |s| s.road),
            })
            .collect();
        Self { kind, top_speed: None, water: false, mesh: None, line: None, route, index: 0, progress: 0.0 }
    }

    /// Whether it runs on track (a train).
    pub fn on_rail(&self) -> bool {
        !self.water && self.route.first().is_some_and(|w| w.road == RoadKind::Rail)
    }

    /// A boat following water tiles at the water's surface.
    pub fn boat(path: &[(u32, u32)]) -> Self {
        let route = path.iter().map(|&(w, l)| Waypoint { w, l, height_min: 0, height_max: 0, traffic: 0, road: RoadKind::Street }).collect();
        Self { kind: VehicleKind::Car, top_speed: Some(1.2), water: true, mesh: None, line: None, route, index: 0, progress: 0.0 }
    }

    /// Advances the vehicle; returns false once it has arrived.
    pub fn advance(&mut self, dt_ms: f32) -> bool {
        let mut remaining = dt_ms;
        while remaining > 0.0 {
            let Some(next) = self.route.get(self.index + 1) else { return false };
            let ms_per_tile = vehicle_tile_time(next.road, next.traffic, self.top_speed.unwrap_or(f32::INFINITY)) * MS_PER_STEP;
            let left = (1.0 - self.progress) * ms_per_tile;
            if remaining < left {
                self.progress += remaining / ms_per_tile;
                return true;
            }
            remaining -= left;
            self.index += 1;
            self.progress = 0.0;
        }
        self.index + 1 < self.route.len()
    }

    pub fn pose(&self) -> VehiclePose {
        let a = self.route[self.index];
        let b = self.route.get(self.index + 1).copied().unwrap_or(a);
        let t = self.progress;
        let heading = match (b.w as i64 - a.w as i64, b.l as i64 - a.l as i64) {
            (0, d) if d < 0 => Direction::North,
            (d, 0) if d > 0 => Direction::East,
            (d, 0) if d < 0 => Direction::West,
            _ => Direction::South,
        };
        let lerp = |x: f32, y: f32| x + (y - x) * t;
        let ha = (a.height_min as f32 + a.height_max as f32) / 2.0;
        let hb = (b.height_min as f32 + b.height_max as f32) / 2.0;
        VehiclePose { x: lerp(a.w as f32, b.w as f32) + 0.5, y: lerp(ha, hb), z: lerp(a.l as f32, b.l as f32) + 0.5, heading }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{layer::Rect, Footprints};
    use rand::SeedableRng;

    #[test]
    fn path_follows_roads() {
        let t = Terrain::flat(10, 10);
        let f = Footprints::builtin();
        let mut rng = rand_pcg::Pcg32::seed_from_u64(0);
        let mut layer = BuildingLayer::new(10, 10);
        layer.build(Rect::new((0, 0), (5, 5)), StructureCode::Road, &t, &f, &mut rng).unwrap();
        let p = find_path(&layer, &t, (0, 0), (5, 5), PathCost::Distance).unwrap();
        assert_eq!(p.len(), 11);
        assert_eq!(p.first(), Some(&(0, 0)));
        assert_eq!(p.last(), Some(&(5, 5)));
        assert!(find_path(&layer, &t, (0, 0), (9, 9), PathCost::Distance).is_none());

        let mut v = Vehicle::new(VehicleKind::Car, &p, &layer, &t);
        assert!(v.advance(100.0));
        assert!(v.pose().x > 0.5);
        assert!(!v.advance(10_000.0));
    }

    #[test]
    fn highways_are_quicker_and_dirt_roads_slower() {
        assert!(tile_time(RoadKind::Highway, 0) < tile_time(RoadKind::Street, 0));
        assert!(tile_time(RoadKind::DirtRoad, 0) > tile_time(RoadKind::PavedStreet, 0));
        assert!(tile_time(RoadKind::DirtRoad, 20) > tile_time(RoadKind::Highway, 20));
        // A highway detour beats the direct street.
        let t = Terrain::flat(12, 6);
        let mut layer = BuildingLayer::new(12, 6);
        layer.build_road(Rect::new((0, 0), (11, 0)), RoadKind::Street, &t).unwrap();
        layer.build_road(Rect::new((0, 0), (0, 2)), RoadKind::Highway, &t).unwrap();
        layer.build_road(Rect::new((0, 2), (11, 2)), RoadKind::Highway, &t).unwrap();
        layer.build_road(Rect::new((11, 0), (11, 2)), RoadKind::Highway, &t).unwrap();
        let p = find_path(&layer, &t, (1, 0), (10, 0), PathCost::Distance).unwrap();
        assert!(p.contains(&(5, 2)), "{p:?}");
        // A cart barred from the highway keeps to the street.
        let ways = Ways::of([RoadKind::Street]);
        let p = find_route(&layer, &t, (1, 0), (10, 0), PathCost::Distance, &ways, 0.5).unwrap();
        assert_eq!(p.len(), 10);
        // A walker is no quicker on a highway.
        assert_eq!(vehicle_tile_time(RoadKind::Highway, 0, 0.4), vehicle_tile_time(RoadKind::Street, 0, 0.4));
        assert!(find_route(&layer, &t, (1, 0), (0, 2), PathCost::Distance, &ways, 1.0).is_none());
    }

    #[test]
    fn vehicle_models_have_ids() {
        for k in VehicleKind::ALL {
            assert_eq!(VehicleKind::from_id(k.id()), Some(k));
        }
        assert!(VehicleKind::Car.graphic().is_some() && VehicleKind::HorseCart.graphic().is_none());
    }
}
