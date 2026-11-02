//! The micro-simulators (legacy `simulator/*.cpp`).
//!
//! The legacy engine ran them in an SDL thread guarded by a global mutex.
//! Here each one is a plain function over the city state, called from
//! [`crate::City::update`] on a fixed simulated-time schedule, which makes
//! the whole simulation deterministic for a given seed.

use std::collections::VecDeque;

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{
    Direction, Footprints, StructureCode, Terrain,
    layer::{BuildingLayer, LEVEL_MAX, Query, Rect, Status, Structure},
};

/// Chance (in %) that a satisfied zone grows during one step.
pub const GROW_CHANCE: u32 = 70;
/// Chance (in %) that an unsatisfied zone declines during one step.
pub const DECLINE_CHANCE: u32 = 40;
pub const COAL_PLANT_POWER: i32 = 20_000;
pub const NUCLEAR_PLANT_POWER: i32 = 50_000;
pub const SERVICE_POWER_CONSUMPTION: i32 = 50;
/// Radius within which a consumer picks up power from a powered tile.
pub const POWER_CONSUMER_RANGE: u32 = 2;
pub const POWER_LINE_RANGE: u32 = 1;
/// Radius of road tiles considered by the traffic simulator.
pub const TRAFFIC_RANGE: u32 = 2;
/// Minimum traffic load before vehicles start appearing.
pub const TRAFFIC_VEHICLE_MIN: u8 = 50;
/// Chance (in %) of spawning a vehicle on a busy road.
pub const TRAFFIC_VEHICLE_CHANCE: u32 = 10;

/// What a zone needs within which radius to grow.
struct ZoneNeeds {
    own: u8,
    road: u32,
    others: [(StructureCode, u8, u32); 2],
}

fn needs(code: StructureCode) -> ZoneNeeds {
    use StructureCode::*;
    match code {
        Residential => ZoneNeeds { own: Status::RESIDENTIAL, road: 3, others: [(Commercial, Status::COMMERCIAL, 4), (Industrial, Status::INDUSTRIAL, 7)] },
        Commercial => ZoneNeeds { own: Status::COMMERCIAL, road: 2, others: [(Residential, Status::RESIDENTIAL, 4), (Industrial, Status::INDUSTRIAL, 8)] },
        Industrial => ZoneNeeds { own: Status::INDUSTRIAL, road: 4, others: [(Residential, Status::RESIDENTIAL, 7), (Commercial, Status::COMMERCIAL, 8)] },
        other => unreachable!("{other:?} is not a zone"),
    }
}

/// Aggregated simulator values shown in the status bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimValues {
    /// Sum of residential levels.
    pub residential: u32,
    pub commercial: u32,
    pub industrial: u32,
    /// Production minus consumption.
    pub power: i32,
    pub power_capacity: i32,
}

/// Picks a random zone of `code`, updates its service flags and maybe makes it
/// grow or decline (legacy `ResidentialSim/CommercialSim/IndustrialSim::Main`).
/// Returns whether the zone changed level.
pub fn zone_step(layer: &mut BuildingLayer, terrain: &Terrain, footprints: &Footprints, code: StructureCode, rng: &mut impl Rng) -> bool {
    let Some((w, l)) = layer.random_structure(code, rng) else { return false };
    let n = needs(code);
    let mut status = layer.tile(w, l).unwrap().status;
    status.unset(Status::WATER | Status::GAS | Status::RESIDENTIAL | Status::COMMERCIAL | Status::INDUSTRIAL | Status::ROAD);
    status.set(n.own);
    let around = |r| {
        let (w1, l1, w2, l2) = terrain.around(w, l, r);
        Rect { w1, l1, w2, l2 }
    };
    if layer.contains(around(n.road), Query::Code(StructureCode::Road)) {
        status.set(Status::ROAD);
    }
    for (other, bit, range) in n.others {
        if layer.contains(around(range), Query::Code(other)) {
            status.set(bit);
        }
    }
    layer.tile_mut(w, l).unwrap().status = status;

    let satisfied = status.has(Status::POWER | Status::RESIDENTIAL | Status::COMMERCIAL | Status::INDUSTRIAL | Status::ROAD);
    let roll = rng.gen_range(0..100);
    let s = layer.structure_at(w, l).unwrap().clone();
    let old = s.graphic;
    let changed = if satisfied {
        roll < GROW_CHANCE && s.level < LEVEL_MAX && can_take_shape(layer, terrain, footprints, w, l, &s, s.next_level_graphic()) && {
            let s = layer.structure_mut(w, l).unwrap();
            s.graphic = s.next_level_graphic();
            s.level += 1;
            true
        }
    } else {
        roll < DECLINE_CHANCE && s.level > 0 && can_take_shape(layer, terrain, footprints, w, l, &s, s.previous_level_graphic()) && {
            let s = layer.structure_mut(w, l).unwrap();
            s.graphic = s.previous_level_graphic();
            s.level -= 1;
            true
        }
    };
    if changed {
        layer.resize(w, l, old, terrain, footprints, rng);
    }
    changed
}

/// Whether the zone at `(w, l)` has room to switch to model `next`
/// (legacy `CheckLevelUp` / `CheckLevelDown`).
fn can_take_shape(layer: &BuildingLayer, terrain: &Terrain, footprints: &Footprints, w: u32, l: u32, s: &Structure, next: crate::GraphicCode) -> bool {
    if next == s.graphic {
        return true;
    }
    let (nw, nl) = footprints.size(next);
    if footprints.size(s.graphic) == (nw, nl) {
        return true;
    }
    let r = Rect::sized(w, l, nw, nl);
    layer.in_bounds(r) && terrain.is_area_flat(r.w1, r.l1, r.w2, r.l2) && layer.contains_only(r, s.code)
}

/// Propagates power from every plant through connected structures
/// (legacy `ElectricitySim::Main`).
///
/// Power flows by flood fill over 4-connected occupied tiles. A consumer is
/// lit when a powered tile lies within [`POWER_CONSUMER_RANGE`]; a power line
/// relays power within [`POWER_LINE_RANGE`]. Roads, parks and trees carry
/// the flood but never get power themselves.
pub fn power_step(layer: &mut BuildingLayer, terrain: &Terrain) {
    let (width, length) = (layer.width(), layer.length());
    let idx = |w: u32, l: u32| (l * width + w) as usize;
    layer.for_each_status(|s| s.unset(Status::POWER));
    let plants: Vec<(u32, u32)> = layer.structures().filter(|(_, s)| s.code.is_power_plant()).map(|(p, _)| p).collect();
    let mut seen = vec![false; (width * length) as usize];
    let mut queue = VecDeque::new();
    for plant in plants {
        if seen[idx(plant.0, plant.1)] {
            continue;
        }
        seen[idx(plant.0, plant.1)] = true;
        queue.push_back(plant);
        while let Some((w, l)) = queue.pop_front() {
            let code = layer.code_at(w, l).unwrap();
            let range = match code {
                c if c.is_power_plant() => Some(0),
                c if c.is_zone() || c.is_public_service() || c == StructureCode::Part => Some(POWER_CONSUMER_RANGE),
                StructureCode::PowerLine => Some(POWER_LINE_RANGE),
                _ => None,
            };
            let powered = match range {
                Some(0) => true,
                Some(r) => {
                    let (w1, l1, w2, l2) = terrain.around(w, l, r);
                    layer.contains(Rect { w1, l1, w2, l2 }, Query::Powered)
                }
                None => false,
            };
            if powered {
                layer.tile_mut(w, l).unwrap().status.set(Status::POWER);
            }
            for dir in Direction::ALL {
                if let Some((nw, nl)) = terrain.neighbour(w, l, dir) {
                    if !seen[idx(nw, nl)] && layer.tile(nw, nl).is_some() {
                        seen[idx(nw, nl)] = true;
                        queue.push_back((nw, nl));
                    }
                }
            }
        }
    }
}

/// Updates the traffic load of one random road (legacy `TrafficSim::Main`).
/// Returns the road position if a vehicle should be spawned there.
pub fn traffic_step(layer: &mut BuildingLayer, terrain: &Terrain, rng: &mut impl Rng) -> Option<(u32, u32)> {
    let (w, l) = layer.random_structure(StructureCode::Road, rng)?;
    let (w1, l1, w2, l2) = terrain.around(w, l, TRAFFIC_RANGE);
    let (mut roads, mut load) = (0u32, 0u32);
    for (x, y) in (Rect { w1, l1, w2, l2 }).tiles() {
        if let Some(s) = layer.tile(x, y).and_then(|t| t.structure()) {
            match s.code {
                StructureCode::Road => roads += 1,
                c if c.is_zone() => load += s.level as u32,
                _ => {}
            }
        }
    }
    let traffic = (load / roads.max(1)).min(u8::MAX as u32) as u8;
    layer.structure_mut(w, l).unwrap().traffic = traffic;
    (traffic > TRAFFIC_VEHICLE_MIN && rng.gen_range(0..100) < TRAFFIC_VEHICLE_CHANCE).then_some((w, l))
}

/// Recomputes the aggregated values (legacy `MainSim::RefreshSimValue`).
pub fn refresh_values(layer: &BuildingLayer) -> SimValues {
    let mut v = SimValues::default();
    for (_, tile) in layer.tiles() {
        let powered = tile.status.has(Status::POWER) as i32;
        match &tile.kind {
            crate::layer::TileKind::Part { .. } => v.power -= powered,
            crate::layer::TileKind::Main(s) => {
                let level = s.level as u32;
                match s.code {
                    StructureCode::Residential => v.residential += level,
                    StructureCode::Commercial => v.commercial += level,
                    StructureCode::Industrial => v.industrial += level,
                    StructureCode::PowerLine => v.power -= powered,
                    StructureCode::CoalPlant => v.power_capacity += COAL_PLANT_POWER,
                    StructureCode::NuclearPlant => v.power_capacity += NUCLEAR_PLANT_POWER,
                    c if c.is_public_service() => v.power -= SERVICE_POWER_CONSUMPTION * powered,
                    _ => {}
                }
                if s.code.is_zone() {
                    v.power -= level as i32 * powered;
                }
            }
        }
    }
    v.power += v.power_capacity;
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_pcg::Pcg32;

    #[test]
    fn power_reaches_zone_through_line() {
        let t = Terrain::flat(30, 10);
        let f = Footprints::builtin();
        let mut rng = Pcg32::seed_from_u64(7);
        let mut layer = BuildingLayer::new(30, 10);
        layer.build(Rect::tile(0, 0), StructureCode::CoalPlant, &t, &f, &mut rng).unwrap(); // 5x3
        layer.build(Rect::new((5, 1), (15, 1)), StructureCode::PowerLine, &t, &f, &mut rng).unwrap();
        layer.build(Rect::tile(16, 1), StructureCode::Residential, &t, &f, &mut rng).unwrap();
        layer.build(Rect::tile(25, 5), StructureCode::Residential, &t, &f, &mut rng).unwrap();
        power_step(&mut layer, &t);
        assert!(layer.tile(16, 1).unwrap().status.has(Status::POWER));
        assert!(layer.tile(10, 1).unwrap().status.has(Status::POWER));
        assert!(!layer.tile(25, 5).unwrap().status.has(Status::POWER));
        let v = refresh_values(&layer);
        assert_eq!(v.power_capacity, COAL_PLANT_POWER);
        // 14 powered part tiles + 11 line tiles; the level-0 zone draws nothing
        assert_eq!(v.power, COAL_PLANT_POWER - 14 - 11);
    }
}
