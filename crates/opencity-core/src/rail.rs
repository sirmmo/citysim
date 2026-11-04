//! Railways: lines of connected track and the stations beside them.
//!
//! Track is a kind of road ([`RoadKind::Rail`]) that only trains use. A
//! station is a block building; any track tile next to it is its platform,
//! and it serves the line that track belongs to. Two places near stations on
//! the same line are linked, whatever the streets between them.

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
    pub fn find(layer: &BuildingLayer, terrain: &Terrain, footprints: &Footprints) -> Self {
        let (width, length) = (layer.width(), layer.length());
        let mut line = vec![NO_LINE; (width * length) as usize];
        let mut next = 0;
        for l in 0..length {
            for w in 0..width {
                if line[(l * width + w) as usize] != NO_LINE || !is_track(layer, w, l) {
                    continue;
                }
                line[(l * width + w) as usize] = next;
                let mut q = VecDeque::from([(w, l)]);
                while let Some((x, y)) = q.pop_front() {
                    for d in Direction::ALL {
                        if let Some((nx, ny)) = terrain.neighbour(x, y, d) {
                            let j = (ny * width + nx) as usize;
                            if line[j] == NO_LINE && is_track(layer, nx, ny) {
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
        for (origin, s) in layer.structures().filter(|(_, s)| s.code == StructureCode::Station) {
            let size = s.size(footprints);
            // The first track tile around the building is the platform.
            let (w1, l1) = (origin.0 as i64 - 1, origin.1 as i64 - 1);
            let (w2, l2) = (origin.0 as i64 + size.0 as i64, origin.1 as i64 + size.1 as i64);
            // Tiles sharing an edge with the building (no corners).
            let ring = (w1 + 1..w2).flat_map(|x| [(x, l1), (x, l2)]).chain((l1 + 1..l2).flat_map(|y| [(w1, y), (w2, y)]));
            let platform = ring
                .filter(|&(x, y)| x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < length)
                .map(|(x, y)| (x as u32, y as u32))
                .find(|&(x, y)| line[(y * width + x) as usize] != NO_LINE);
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
