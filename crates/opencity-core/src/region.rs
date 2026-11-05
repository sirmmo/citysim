//! Regions: a grid of neighbouring cities that trade across their borders.
//!
//! One city is played at a time; the others wait with a summary of what
//! they have left over and what they lack. Each month the open city buys
//! what it lacks from neighbours with a surplus and sells its own surplus
//! to neighbours that lack it, at the rules' `price` per resource. Two
//! neighbours trade only through a crossing on both sides of their shared
//! edge: a road, track or power line that reaches it.
//!
//! A crossing built to the edge of one map is mirrored on the neighbour's
//! map at the same place along the shared edge ([`Region::incoming`],
//! [`City::connect_border`]), so the two towns join up.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{City, RoadKind, StructureCode, Tool, ToolError};

/// Sides of a city map, in `borders` order.
pub const SIDES: [&str; 4] = ["north", "east", "south", "west"];

/// The region file (`region.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub name: String,
    /// Slots across and down.
    pub size: [u32; 2],
    /// Tiles per side of each city map.
    pub map_size: u32,
    pub seed: u64,
    /// GlassBox rules of every city (none: classic).
    #[serde(default)]
    pub rules: Option<String>,
    #[serde(default)]
    pub cities: Vec<RegionCity>,
}

/// A founded city and its summary.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RegionCity {
    pub slot: [u32; 2],
    pub name: String,
    /// Save file, relative to the region folder.
    pub file: String,
    #[serde(default)]
    pub population: u32,
    #[serde(default)]
    pub fund: i64,
    #[serde(default)]
    pub date: String,
    /// Crossings to the north, east, south and west edges.
    #[serde(default)]
    pub borders: [bool; 4],
    /// Where they are and what crosses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crossings: Vec<Crossing>,
    /// What it has left over and lacks per tick, by resource id.
    #[serde(default)]
    pub surplus: BTreeMap<String, f32>,
    #[serde(default)]
    pub deficit: BTreeMap<String, f32>,
    /// What it makes of each shared pool per tick (its culture).
    #[serde(default)]
    pub culture: BTreeMap<String, f32>,
    /// The player who plays it (region servers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<String>,
}

/// Share of the rest of the region's culture each city takes in.
pub const CULTURE_SHARE: f32 = 0.1;

/// One month's trade of the open city with one neighbour.
#[derive(Clone, Debug, PartialEq)]
pub struct Deal {
    pub partner: [u32; 2],
    pub resource: String,
    /// Per tick; positive: bought (import), negative: sold (export).
    pub amount: f32,
    /// The open city's crossings it goes through (shared out evenly):
    /// each one is a node of the city's network that sends what is bought
    /// in and takes what is sold out.
    pub gates: Vec<Crossing>,
}

/// A road, track or power line on an edge tile of a map.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Crossing {
    /// 0 north, 1 east, 2 south, 3 west.
    pub side: u8,
    /// Tile along the edge (x for north and south, y for east and west).
    pub at: u32,
    /// The tool that builds it: a road kind's id (`street`, `rail`…) or
    /// `power_line`.
    pub link: String,
}

impl Crossing {
    /// The same crossing seen from the neighbour across the edge.
    pub fn opposite(&self) -> Self {
        Self { side: (self.side + 2) % 4, ..self.clone() }
    }

    /// Its tile on a map of this size.
    pub fn tile(&self, w: u32, l: u32) -> (u32, u32) {
        match self.side {
            0 => (self.at, 0),
            1 => (w - 1, self.at),
            2 => (self.at, l - 1),
            _ => (0, self.at),
        }
    }

    pub fn side_name(&self) -> &'static str {
        SIDES[self.side as usize % 4]
    }

    /// A word for what crosses: "street", "railway", "power line".
    pub fn what(&self) -> String {
        match RoadKind::from_id(&self.link) {
            Some(RoadKind::Rail) => "railway".into(),
            Some(k) => k.name().to_lowercase(),
            None => "power line".into(),
        }
    }

    /// Whether this crossing joins the neighbour's `other` (seen from its
    /// side): same place, and a track meets a track, a street a street, a
    /// power line a street or a power line.
    pub fn joins(&self, other: &Crossing) -> bool {
        let rail = |l: &str| l == RoadKind::Rail.id();
        let line = |l: &str| l == Tool::PowerLine.id();
        self.side == (other.side + 2) % 4
            && self.at == other.at
            && match (line(&self.link), line(&other.link)) {
                (true, true) => true,
                (true, false) => !rail(&other.link),
                (false, true) => !rail(&self.link),
                (false, false) => rail(&self.link) == rail(&other.link),
            }
    }

    /// Whether the resource can go through this crossing: its network is
    /// made of what crosses here (power over lines and streets, trips over
    /// streets, trains over track).
    pub fn carries(&self, rules: &crate::rules::Ruleset, resource: usize) -> bool {
        use crate::rules::Transport;
        let res = &rules.resources[rules.root(resource)];
        if !matches!(res.transport, Transport::Agents | Transport::Grid) {
            return false;
        }
        let Some(net) = rules.network(res.network.as_deref().unwrap_or_default()).map(|n| &rules.networks[n]) else { return false };
        let has = |s: &str| net.structures.iter().any(|x| x == s);
        match RoadKind::from_id(&self.link) {
            Some(RoadKind::Rail) => {
                has("rail")
                    || (res.transport == Transport::Agents && {
                        let v = rules.vehicles_for(rules.root(resource));
                        v.is_empty() || v.iter().any(|v| v.ways().allows(RoadKind::Rail))
                    })
            }
            Some(_) => has("road"),
            None => has("power_line"),
        }
    }

    fn tool(&self) -> Option<Tool> {
        match RoadKind::from_id(&self.link) {
            Some(k) => Tool::ALL.into_iter().find(|t| t.road_kind() == Some(k)),
            None => (self.link == Tool::PowerLine.id()).then_some(Tool::PowerLine),
        }
    }
}

impl City {
    /// Every road, track and power line on an edge tile (corner tiles count
    /// for both of their edges), in side and position order.
    pub fn crossings(&self) -> Vec<Crossing> {
        let (w, l) = (self.width(), self.length());
        let mut out = Vec::new();
        for side in 0..4u8 {
            let n = if side % 2 == 0 { w } else { l };
            for at in 0..n {
                let c = Crossing { side, at, link: String::new() };
                let (x, y) = c.tile(w, l);
                let link = match self.layer.structure_at(x, y) {
                    Some(s) if s.code == StructureCode::Road => s.road.id(),
                    Some(s) if s.code == StructureCode::PowerLine => Tool::PowerLine.id(),
                    _ => continue,
                };
                out.push(Crossing { link: link.to_string(), ..c });
            }
        }
        out
    }

    /// Builds a neighbour's crossing on this map, free: Ok(true) when
    /// built, Ok(false) when something that joins it is already there
    /// (any road for a road, track for track, a road or line for power)
    /// or it was linked before (the player may have bulldozed it since).
    pub fn connect_border(&mut self, c: &Crossing) -> Result<bool, ToolError> {
        if self.border_links.contains(c) {
            return Ok(false);
        }
        let (w, l) = c.tile(self.width(), self.length());
        let rail = c.link == RoadKind::Rail.id();
        let joined = self.layer.structure_at(w, l).is_some_and(|s| match s.code {
            StructureCode::Road => c.tool() == Some(Tool::PowerLine) || (s.road == RoadKind::Rail) == rail,
            StructureCode::PowerLine => c.tool() == Some(Tool::PowerLine),
            _ => false,
        });
        if joined {
            self.border_links.push(c.clone());
            return Ok(false);
        }
        // Never replace what the player built there.
        if self.layer.structure_at(w, l).is_some() {
            return Err(ToolError::BorderTaken);
        }
        let tool = c.tool().ok_or(ToolError::Locked(c.link.clone()))?;
        self.build_tool(tool, crate::layer::Rect::tile(w, l), (w, l))?;
        self.border_links.push(c.clone());
        Ok(true)
    }

    /// Whether a road, track or power line reaches each edge (north, east,
    /// south, west): the city's border crossings.
    pub fn borders(&self) -> [bool; 4] {
        let (w, l) = (self.width(), self.length());
        let crossing = |x: u32, y: u32| self.layer.structure_at(x, y).is_some_and(|s| matches!(s.code, StructureCode::Road | StructureCode::PowerLine));
        [(0..w).any(|x| crossing(x, 0)), (0..l).any(|y| crossing(w - 1, y)), (0..w).any(|x| crossing(x, l - 1)), (0..l).any(|y| crossing(0, y))]
    }
}

impl Region {
    pub fn new(name: &str, size: [u32; 2], map_size: u32, seed: u64, rules: Option<String>) -> Self {
        Self { name: name.to_string(), size, map_size, seed, rules, cities: Vec::new() }
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn city(&self, slot: [u32; 2]) -> Option<&RegionCity> {
        self.cities.iter().find(|c| c.slot == slot)
    }

    /// Terrain seed of a slot's map.
    pub fn slot_seed(&self, slot: [u32; 2]) -> u64 {
        self.seed.wrapping_mul(1_000_003).wrapping_add(slot[1] as u64 * self.size[0] as u64 + slot[0] as u64)
    }

    /// The neighbour across a side (0 north, 1 east, 2 south, 3 west).
    pub fn neighbour(&self, slot: [u32; 2], side: usize) -> Option<&RegionCity> {
        let [x, y] = slot.map(|v| v as i64);
        let (nx, ny) = match side {
            0 => (x, y - 1),
            1 => (x + 1, y),
            2 => (x, y + 1),
            _ => (x - 1, y),
        };
        if nx < 0 || ny < 0 {
            return None;
        }
        self.city([nx as u32, ny as u32])
    }

    /// The neighbours' crossings toward a slot, seen from its side (the
    /// tiles its own map should have to join them).
    pub fn incoming(&self, slot: [u32; 2]) -> Vec<(String, Crossing)> {
        let mut out = Vec::new();
        for side in 0..4u8 {
            let Some(n) = self.neighbour(slot, side as usize) else { continue };
            for c in n.crossings.iter().filter(|c| c.side == (side + 2) % 4) {
                out.push((n.name.clone(), c.opposite()));
            }
        }
        out
    }

    /// Whether there is a slot across a side (founded or not).
    pub fn has_slot(&self, slot: [u32; 2], side: usize) -> bool {
        match side {
            0 => slot[1] > 0,
            1 => slot[0] + 1 < self.size[0],
            2 => slot[1] + 1 < self.size[1],
            _ => slot[0] > 0,
        }
    }

    /// Records (or adds) a city's summary from its current state.
    pub fn summarize(&mut self, slot: [u32; 2], name: &str, file: &str, city: &City) {
        let mut entry = RegionCity {
            slot,
            name: name.to_string(),
            file: file.to_string(),
            population: city.population,
            fund: city.fund,
            date: city.date.to_string(),
            borders: city.borders(),
            crossings: city.crossings(),
            ..Default::default()
        };
        if let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) {
            for (i, r) in rules.resources.iter().enumerate().filter(|(_, r)| r.price > 0.0 && r.parent.is_none()) {
                entry.surplus.insert(r.id.clone(), gb.surplus.get(i).copied().unwrap_or(0.0));
                entry.deficit.insert(r.id.clone(), gb.deficit.get(i).copied().unwrap_or(0.0));
            }
            for (i, r) in rules.resources.iter().enumerate().filter(|(_, r)| r.shared) {
                entry.culture.insert(r.id.clone(), gb.balances.produced.get(i).copied().unwrap_or(0.0));
            }
        }
        entry.player = self.city(slot).and_then(|c| c.player.clone());
        match self.cities.iter_mut().find(|c| c.slot == slot) {
            Some(c) => *c = entry,
            None => self.cities.push(entry),
        }
    }

    /// Culture the city in `slot` takes in per tick: a share of what every
    /// other city of the region makes of each shared pool.
    pub fn culture_for(&self, slot: [u32; 2]) -> BTreeMap<String, f32> {
        let mut inflow: BTreeMap<String, f32> = BTreeMap::new();
        for c in self.cities.iter().filter(|c| c.slot != slot) {
            for (r, v) in &c.culture {
                *inflow.entry(r.clone()).or_default() += v * CULTURE_SHARE;
            }
        }
        inflow
    }

    /// The open city's deals with its neighbours for the coming month:
    /// what it lacks, bought from neighbours with a surplus, and what it has
    /// left over, sold to neighbours that lack it, through crossings on
    /// both sides of each shared edge.
    pub fn trade(&self, slot: [u32; 2], city: &City) -> Vec<Deal> {
        let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) else { return Vec::new() };
        let mine = city.crossings();
        let mut deals = Vec::new();
        for (i, r) in rules.resources.iter().enumerate().filter(|(_, r)| r.price > 0.0 && r.parent.is_none()) {
            let mut lack = gb.deficit.get(i).copied().unwrap_or(0.0);
            let mut spare = gb.surplus.get(i).copied().unwrap_or(0.0);
            for side in 0..4u8 {
                let Some(n) = self.neighbour(slot, side as usize) else { continue };
                // Crossings joined on both sides that carry the resource.
                let gates: Vec<Crossing> = mine
                    .iter()
                    .filter(|c| c.side == side && c.carries(rules, i) && n.crossings.iter().any(|o| c.joins(o) && o.carries(rules, i)))
                    .cloned()
                    .collect();
                if gates.is_empty() {
                    continue;
                }
                let buy = lack.min(n.surplus.get(&r.id).copied().unwrap_or(0.0));
                if buy > 0.01 {
                    lack -= buy;
                    deals.push(Deal { partner: n.slot, resource: r.id.clone(), amount: buy, gates: gates.clone() });
                }
                let sell = spare.min(n.deficit.get(&r.id).copied().unwrap_or(0.0));
                if sell > 0.01 {
                    spare -= sell;
                    deals.push(Deal { partner: n.slot, resource: r.id.clone(), amount: -sell, gates });
                }
            }
        }
        deals
    }
}

impl City {
    /// Sets the per-tick culture flowing in from the rest of the region.
    pub fn apply_culture(&mut self, inflow: &BTreeMap<String, f32>) {
        let Some(rules) = self.rules().cloned() else { return };
        let Some(gb) = self.glassbox.as_mut() else { return };
        gb.culture_in = rules.resources.iter().map(|r| if r.shared { inflow.get(&r.id).copied().unwrap_or(0.0) } else { 0.0 }).collect();
    }

    /// Opens the crossings of a month's deals as gates of the network:
    /// each sends its share of what is bought and takes its share of what
    /// is sold, per tick.
    pub fn apply_trade(&mut self, deals: &[Deal]) {
        let Some(rules) = self.rules().cloned() else { return };
        let (w, l) = (self.width(), self.length());
        let Some(gb) = self.glassbox.as_mut() else { return };
        let mut gates: Vec<crate::glassbox::Gate> = Vec::new();
        for d in deals.iter().filter(|d| rules.resource(&d.resource).is_some() && !d.gates.is_empty()) {
            let share = d.amount / d.gates.len() as f32;
            for c in &d.gates {
                let at = c.tile(w, l);
                let i = match gates.iter().position(|g| g.at == at) {
                    Some(i) => i,
                    None => {
                        gates.push(crate::glassbox::Gate { at, link: c.link.clone(), partner: d.partner, ..Default::default() });
                        gates.len() - 1
                    }
                };
                let g = &mut gates[i];
                let book = if share > 0.0 { &mut g.import } else { &mut g.export };
                *book.entry(d.resource.clone()).or_default() += share.abs();
            }
        }
        gb.gates = gates;
    }
}
