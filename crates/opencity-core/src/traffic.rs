//! Road pathfinding and vehicles (legacy `PathFinder`, `Vehicle`, `MovementManager`).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

use crate::{layer::BuildingLayer, Direction, GraphicCode, StructureCode, Terrain};

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

/// Shortest road route from `from` to `to`, both included. Dijkstra over road
/// tiles; with [`PathCost::Traffic`] each tile costs `1 + traffic`.
pub fn find_path(layer: &BuildingLayer, terrain: &Terrain, from: (u32, u32), to: (u32, u32), cost: PathCost) -> Option<Vec<(u32, u32)>> {
    let is_road = |w, l| layer.code_at(w, l) == Some(StructureCode::Road);
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
            let step = match cost {
                PathCost::Distance => 1,
                PathCost::Traffic => 1 + layer.structure_at(q.0, q.1).map_or(0, |s| s.traffic as u32),
            };
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VehicleKind {
    Car,
    Pickup,
    Sports,
}

impl VehicleKind {
    pub const fn graphic(self) -> GraphicCode {
        match self {
            Self::Car => GraphicCode::VEHICLE_STD,
            Self::Pickup => GraphicCode::VEHICLE_PICKUP,
            Self::Sports => GraphicCode::VEHICLE_SPORT,
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vehicle {
    pub kind: VehicleKind,
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
            })
            .collect();
        Self { kind, route, index: 0, progress: 0.0 }
    }

    /// Advances the vehicle; returns false once it has arrived.
    pub fn advance(&mut self, dt_ms: f32) -> bool {
        let mut remaining = dt_ms;
        while remaining > 0.0 {
            let Some(next) = self.route.get(self.index + 1) else { return false };
            let ms_per_tile = (next.traffic as f32 + 10.0) * MS_PER_STEP;
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
}
