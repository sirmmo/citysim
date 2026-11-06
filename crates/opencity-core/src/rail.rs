//! Railways, waterways, tram and bus lines: lines that link the places near
//! their stops.
//!
//! Track is a kind of road ([`RoadKind::Rail`]) that only trains use. A
//! station is a block building; any track tile next to it is its platform,
//! and it serves the line that track belongs to. Waterways work the same
//! way: a line is one connected body of water, and a harbour on its shore
//! is the stop, with a water tile beside it as the quay. Two places near
//! stops on the same line are linked, whatever the streets between them.
//! Trams run on tram streets between tram stops, and buses on the streets
//! between bus stops, the same way.

use std::collections::VecDeque;

use crate::{layer::BuildingLayer, Direction, Footprints, RoadKind, StructureCode, Terrain};

/// Line value of tiles that are not track.
pub const NO_LINE: u32 = u32::MAX;

/// A station and the line it serves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Station {
    /// Origin tile of the station building.
    pub origin: (u32, u32),
    /// Tiles the building covers.
    pub size: (u32, u32),
    /// Track tile next to the building where trains stop.
    pub platform: (u32, u32),
    pub line: u32,
}

impl Station {
    /// Manhattan distance from a tile to the building.
    pub fn distance(&self, (w, l): (u32, u32)) -> u32 {
        let (w1, l1) = self.origin;
        let (w2, l2) = (w1 + self.size.0 - 1, l1 + self.size.1 - 1);
        let d = |v: u32, a: u32, b: u32| if v < a { a - v } else { v.saturating_sub(b) };
        d(w, w1, w2) + d(l, l1, l2)
    }
}

/// The railway network of a city.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Railways {
    /// Line of each tile (row-major), [`NO_LINE`] off the track.
    pub line: Vec<u32>,
    pub stations: Vec<Station>,
}

fn is_track(layer: &BuildingLayer, w: u32, l: u32) -> bool {
    layer.structure_at(w, l).is_some_and(|s| s.code == StructureCode::Road && s.road == RoadKind::Rail)
}

impl Railways {
    /// Track lines and their stations.
    pub fn find(layer: &BuildingLayer, terrain: &Terrain, footprints: &Footprints) -> Self {
        Self::build(layer, terrain, footprints, StructureCode::Station, 1, 1, |w, l| is_track(layer, w, l))
    }

    /// Tram lines (connected tram streets) and their tram stops.
    pub fn trams(layer: &BuildingLayer, terrain: &Terrain, footprints: &Footprints) -> Self {
        let tram = |w, l| layer.structure_at(w, l).is_some_and(|s| s.code == StructureCode::Road && s.road == RoadKind::Tram);
        Self::build(layer, terrain, footprints, StructureCode::TramStop, 1, 1, tram)
    }

    /// Bus lines (each connected network of streets buildings open onto)
    /// and their bus stops.
    pub fn buses(layer: &BuildingLayer, terrain: &Terrain, footprints: &Footprints) -> Self {
        let street = |w, l| layer.structure_at(w, l).is_some_and(|s| s.code == StructureCode::Road && s.road.access());
        Self::build(layer, terrain, footprints, StructureCode::BusStop, 1, 1, street)
    }

    /// Metro lines (connected tunnels) and their stations: the platform is
    /// the tunnel under the entrance or beside it.
    pub fn metro(layer: &BuildingLayer, terrain: &Terrain, footprints: &Footprints) -> Self {
        Self::build(layer, terrain, footprints, StructureCode::MetroStation, 0, 1, |w, l| layer.has_tunnel(w, l))
    }

    /// Bodies of water and their harbours.
    pub fn water(layer: &BuildingLayer, terrain: &Terrain, footprints: &Footprints) -> Self {
        // The shore between flat land and water is a slope, so the quay may
        // be two tiles out.
        // Boats pass under bridges.
        Self::build(layer, terrain, footprints, StructureCode::Harbour, 1, 2, |w, l| {
            terrain.is_water(w, l) && layer.structure_at(w, l).is_none_or(|s| s.code == StructureCode::Road)
        })
    }

    /// Lines of `on_line` tiles, and the `stop` buildings with a line tile
    /// `near..=reach` tiles from them.
    fn build(
        layer: &BuildingLayer,
        terrain: &Terrain,
        footprints: &Footprints,
        stop: StructureCode,
        near: u32,
        reach: u32,
        on_line: impl Fn(u32, u32) -> bool,
    ) -> Self {
        let (width, length) = (layer.width(), layer.length());
        let mut line = vec![NO_LINE; (width * length) as usize];
        let mut next = 0;
        for l in 0..length {
            for w in 0..width {
                if line[(l * width + w) as usize] != NO_LINE || !on_line(w, l) {
                    continue;
                }
                line[(l * width + w) as usize] = next;
                let mut q = VecDeque::from([(w, l)]);
                while let Some((x, y)) = q.pop_front() {
                    for d in Direction::ALL {
                        if let Some((nx, ny)) = terrain.neighbour(x, y, d) {
                            let j = (ny * width + nx) as usize;
                            if line[j] == NO_LINE && on_line(nx, ny) {
                                line[j] = next;
                                q.push_back((nx, ny));
                            }
                        }
                    }
                }
                next += 1;
            }
        }
        let mut stations = Vec::new();
        for (origin, s) in layer.structures().filter(|(_, s)| s.code == stop) {
            let size = s.size(footprints);
            // The nearest line tile around the building (by its edges) is
            // the platform.
            let st = Station { origin, size, platform: (0, 0), line: NO_LINE };
            let r = reach as i64;
            let (w1, l1) = (origin.0 as i64 - r, origin.1 as i64 - r);
            let (w2, l2) = (origin.0 as i64 + size.0 as i64 - 1 + r, origin.1 as i64 + size.1 as i64 - 1 + r);
            let platform = (l1..=l2)
                .flat_map(|y| (w1..=w2).map(move |x| (x, y)))
                .filter(|&(x, y)| x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < length)
                .map(|(x, y)| (x as u32, y as u32))
                .filter(|&p| (near..=reach).contains(&st.distance(p)) && line[(p.1 * width + p.0) as usize] != NO_LINE)
                .min_by_key(|&p| (st.distance(p), p.1, p.0));
            if let Some(p) = platform {
                stations.push(Station { origin, size, platform: p, line: line[(p.1 * width + p.0) as usize] });
            }
        }
        Self { line, stations }
    }

    /// The nearest station within `reach` tiles of a tile, with its distance.
    pub fn nearest_station(&self, tile: (u32, u32), reach: u32) -> Option<(usize, u32)> {
        self.stations.iter().enumerate().map(|(i, s)| (i, s.distance(tile))).filter(|&(_, d)| d <= reach).min_by_key(|&(i, d)| (d, i))
    }

    /// The shortest way over a line's tiles between two of its tiles (boats
    /// on water); `None` when they are not joined.
    pub fn route(&self, width: u32, from: (u32, u32), to: (u32, u32)) -> Option<Vec<(u32, u32)>> {
        let at = |p: (u32, u32)| (p.1 * width + p.0) as usize;
        let line = *self.line.get(at(from))?;
        if line == NO_LINE || self.line.get(at(to)) != Some(&line) {
            return None;
        }
        let length = self.line.len() as u32 / width;
        let mut prev: std::collections::HashMap<(u32, u32), (u32, u32)> = std::collections::HashMap::new();
        let mut q = VecDeque::from([from]);
        prev.insert(from, from);
        while let Some(p) = q.pop_front() {
            if p == to {
                let mut path = vec![to];
                let mut c = to;
                while c != from {
                    c = prev[&c];
                    path.push(c);
                }
                path.reverse();
                return Some(path);
            }
            let (x, y) = (p.0 as i64, p.1 as i64);
            for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                if nx < 0 || ny < 0 || nx >= width as i64 || ny >= length as i64 {
                    continue;
                }
                let n = (nx as u32, ny as u32);
                if self.line[at(n)] == line && !prev.contains_key(&n) {
                    prev.insert(n, p);
                    q.push_back(n);
                }
            }
        }
        None
    }

    /// Pairs of different stations on the same line.
    pub fn connections(&self) -> Vec<(usize, usize)> {
        let n = self.stations.len();
        (0..n).flat_map(|a| (0..n).map(move |b| (a, b))).filter(|&(a, b)| a != b && self.stations[a].line == self.stations[b].line).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{City, Tool};

    #[test]
    fn harbours_on_one_lake_are_linked() {
        let mut t = Terrain::flat(40, 20);
        // A lake from w 10 to 30.
        for w in 10..=31 {
            for l in 4..=16 {
                t.set_corner(w, l, -1);
            }
        }
        let mut c = City::new(t, None, Footprints::builtin(), 1);
        c.fund = 1_000_000;
        c.apply_tool(Tool::Harbour, (7, 8), (7, 8)).unwrap();
        c.apply_tool(Tool::Harbour, (32, 8), (32, 8)).unwrap();
        let w = Railways::water(&c.layer, &c.terrain, c.footprints());
        assert_eq!(w.stations.len(), 2, "{:?}", w.stations);
        assert_eq!(w.connections().len(), 2);
        let path = w.route(40, w.stations[0].platform, w.stations[1].platform).unwrap();
        assert!(path.iter().all(|&(x, y)| c.terrain.is_water(x, y)));
        // Inland, a harbour cannot be built.
        assert!(c.apply_tool(Tool::Harbour, (2, 16), (2, 16)).is_err());
    }

    #[test]
    fn stations_on_one_line_are_connected() {
        let mut c = City::new(Terrain::flat(40, 20), None, Footprints::builtin(), 1);
        c.fund = 1_000_000;
        c.apply_tool(Tool::Rail, (2, 10), (37, 10)).unwrap();
        c.apply_tool(Tool::Station, (4, 8), (4, 8)).unwrap();
        c.apply_tool(Tool::Station, (30, 11), (30, 11)).unwrap();
        // A spur of track that touches nothing, and a station off the line.
        c.apply_tool(Tool::Rail, (10, 2), (14, 2)).unwrap();
        c.apply_tool(Tool::Station, (20, 15), (20, 15)).unwrap();
        let r = Railways::find(&c.layer, &c.terrain, c.footprints());
        assert_eq!(r.stations.len(), 2, "{:?}", r.stations);
        assert_eq!(r.stations[0].line, r.stations[1].line);
        assert_eq!(r.connections().len(), 2);
        assert_ne!(r.line[(2 * 40 + 10) as usize], r.stations[0].line);
        assert_eq!(r.nearest_station((5, 3), 6).map(|(i, _)| i), Some(0));
    }
}
