//! The resource network laid on the map. Each link of the GlassBox
//! network (a sender to a receiver, see [`crate::glassbox::Link`]) is
//! followed along the network that carries it: streets of every kind for
//! people and vehicles, track between stations for trains, water between
//! harbours for boats, and power lines, streets and buildings for power.
//! The links are then summed per stretch. A border gate is one more node
//! on that network, sending into it and taking out of it at the edge.

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::{
    glassbox::{conducts, Link, STATION_WALK},
    rail::{Railways, NO_LINE},
    rules::Transport,
    City, Direction, Rect,
};

/// What a stretch runs along.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Way {
    /// Streets and roads (people on foot, cars, lorries, buses).
    Street,
    /// Track, from station to station.
    Rail,
    /// Water, from harbour to harbour.
    Water,
    /// A grid: power lines, streets and the buildings that conduct.
    Grid,
    /// From a building to the network (or straight across, when no way
    /// along the network is found).
    Walk,
}

impl Way {
    pub fn name(self) -> &'static str {
        match self {
            Way::Street => "along the streets",
            Way::Rail => "by rail",
            Way::Water => "by water",
            Way::Grid => "through the grid",
            Way::Walk => "on foot",
        }
    }
}

/// A resource moving from one tile to the next (or from a building to its
/// street), per tick: the sum of every link that passes there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stretch {
    pub from: (u32, u32),
    pub to: (u32, u32),
    pub resource: usize,
    pub way: Way,
    pub amount: f32,
}

impl Stretch {
    pub fn tiles(&self) -> u32 {
        self.from.0.abs_diff(self.to.0) + self.from.1.abs_diff(self.to.1)
    }
}

/// A stretch's key: from, to, resource, way.
type StretchKey = ((u32, u32), (u32, u32), usize, Way);
type Tile = (u32, u32);

impl City {
    /// The links laid along their networks and summed per stretch.
    pub fn network_stretches(&self, links: &[(Link, f32)]) -> Vec<Stretch> {
        let Some(rules) = self.rules() else { return Vec::new() };
        let (width, length) = (self.width(), self.length());
        let n = (width * length) as usize;
        let idx = |p: (u32, u32)| (p.1 * width + p.0) as usize;
        let rail = Railways::find(&self.layer, &self.terrain, self.footprints());
        let water = Railways::water(&self.layer, &self.terrain, self.footprints());
        let mut masks: HashMap<usize, Vec<bool>> = HashMap::new();
        let mut sums: BTreeMap<StretchKey, f32> = BTreeMap::new();
        let mut add = |a: (u32, u32), b: (u32, u32), r: usize, way: Way, v: f32| {
            if a != b {
                *sums.entry((a, b, r, way)).or_default() += v;
            }
        };
        // The tiles a building covers (a gate: its tile), widened by `reach`.
        let area = |o: (u32, u32), reach: u32| -> Rect {
            let (sw, sl) = self.layer.structure_at(o.0, o.1).filter(|s| s.unit.is_some()).map_or((1, 1), |s| s.size(self.footprints()));
            Rect {
                w1: o.0.saturating_sub(reach),
                l1: o.1.saturating_sub(reach),
                w2: (o.0 + sw - 1 + reach).min(width - 1),
                l2: (o.1 + sl - 1 + reach).min(length - 1),
            }
        };
        // Where a building (or a node on the network itself) joins the
        // network: its own tile, else the nearest tile `ok` accepts within
        // reach, ring by ring, as the simulation finds it.
        let nearest = |o: Tile, reach: u32, ok: &dyn Fn(Tile) -> bool| -> Option<Tile> {
            if ok(o) {
                return Some(o);
            }
            let centre = area(o, 0);
            let (cx2, cz2) = ((centre.w1 + centre.w2) as i64, (centre.l1 + centre.l2) as i64);
            (1..=reach).find_map(|d| area(o, d).tiles().filter(|&t| ok(t)).min_by_key(|t| ((t.0 as i64 * 2 - cx2).abs() + (t.1 as i64 * 2 - cz2).abs(), *t)))
        };
        // Links grouped by network and sender: one search from each sender.
        let mut by_source: BTreeMap<(usize, Tile), Vec<(Link, f32)>> = BTreeMap::new();
        for &(k, v) in links {
            let res = &rules.resources[rules.root(k.resource)];
            match res.network.as_deref().and_then(|id| rules.network(id)) {
                Some(net) if matches!(res.transport, Transport::Agents | Transport::Grid) => by_source.entry((net, k.from)).or_default().push((k, v)),
                _ => add(k.from, k.to, k.resource, Way::Walk, v),
            }
        }
        for ((net, from), list) in by_source {
            let mask = masks.entry(net).or_insert_with(|| {
                let def = &rules.networks[net];
                (0..n as u32).map(|i| conducts(def, &self.layer, i % width, i / width)).collect()
            });
            let grid = rules.resources[rules.root(list[0].0.resource)].transport == Transport::Grid;
            let (way, reach) = if grid { (Way::Grid, 1) } else { (Way::Street, rules.growth.road_access.max(1)) };
            // Breadth-first over the network from the tiles the sender opens onto.
            let mut prev = vec![u32::MAX; n];
            let mut q = VecDeque::new();
            let starts: Vec<Tile> = nearest(from, reach, &|t| mask[idx(t)]).into_iter().collect();
            for t in starts {
                prev[idx(t)] = idx(t) as u32;
                q.push_back(t);
            }
            while let Some(p) = q.pop_front() {
                for d in Direction::ALL {
                    if let Some(t) = self.terrain.neighbour(p.0, p.1, d) {
                        if mask[idx(t)] && prev[idx(t)] == u32::MAX {
                            prev[idx(t)] = idx(p) as u32;
                            q.push_back(t);
                        }
                    }
                }
            }
            for (k, v) in list {
                // The nearest reached tile the receiver opens onto.
                let end = nearest(k.to, reach, &|t| prev[idx(t)] != u32::MAX);
                if let Some(end) = end {
                    let mut path = vec![end];
                    let mut c = idx(end);
                    while prev[c] as usize != c {
                        c = prev[c] as usize;
                        path.push((c as u32 % width, c as u32 / width));
                    }
                    path.reverse();
                    add(k.from, path[0], k.resource, way, v);
                    for s in path.windows(2) {
                        add(s[0], s[1], k.resource, way, v);
                    }
                    add(end, k.to, k.resource, way, v);
                    continue;
                }
                // Other street networks are reached by train or boat.
                let mut done = false;
                if !grid {
                    for (lines, way) in [(&rail, Way::Rail), (&water, Way::Water)] {
                        let stop = |p: (u32, u32)| -> Option<(u32, u32)> {
                            if lines.line.get(idx(p)).is_some_and(|&l| l != NO_LINE) {
                                return Some(p);
                            }
                            lines.nearest_station(p, STATION_WALK).map(|(s, _)| lines.stations[s].platform)
                        };
                        let Some(route) = stop(k.from).zip(stop(k.to)).and_then(|(a, b)| lines.route(width, a, b)) else { continue };
                        add(k.from, route[0], k.resource, Way::Walk, v);
                        for s in route.windows(2) {
                            add(s[0], s[1], k.resource, way, v);
                        }
                        add(route[route.len() - 1], k.to, k.resource, Way::Walk, v);
                        done = true;
                        break;
                    }
                }
                if !done {
                    add(k.from, k.to, k.resource, Way::Walk, v);
                }
            }
        }
        sums.into_iter().map(|((from, to, resource, way), amount)| Stretch { from, to, resource, way, amount }).collect()
    }
}
