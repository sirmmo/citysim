//! Regions: a grid of neighbouring cities that trade across their borders.
//!
//! One city is played at a time; the others wait with a summary of what
//! they have left over and what they lack. Each month the open city buys
//! what it lacks from neighbours with a surplus and sells its own surplus
//! to neighbours that lack it, at the rules' `price` per resource. Two
//! neighbours trade only through a crossing on both sides of their shared
//! edge: a road, track or power line that reaches it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{City, StructureCode};

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
    /// What it has left over and lacks per tick, by resource id.
    #[serde(default)]
    pub surplus: BTreeMap<String, f32>,
    #[serde(default)]
    pub deficit: BTreeMap<String, f32>,
}

/// One month's trade of the open city with one neighbour.
#[derive(Clone, Debug, PartialEq)]
pub struct Deal {
    pub partner: [u32; 2],
    pub resource: String,
    /// Per tick; positive: bought (import), negative: sold (export).
    pub amount: f32,
}

impl City {
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
            ..Default::default()
        };
        if let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) {
            for (i, r) in rules.resources.iter().enumerate().filter(|(_, r)| r.price > 0.0 && r.parent.is_none()) {
                entry.surplus.insert(r.id.clone(), gb.surplus.get(i).copied().unwrap_or(0.0));
                entry.deficit.insert(r.id.clone(), gb.deficit.get(i).copied().unwrap_or(0.0));
            }
        }
        match self.cities.iter_mut().find(|c| c.slot == slot) {
            Some(c) => *c = entry,
            None => self.cities.push(entry),
        }
    }

    /// The open city's deals with its neighbours for the coming month:
    /// what it lacks, bought from neighbours with a surplus, and what it has
    /// left over, sold to neighbours that lack it, through crossings on
    /// both sides of each shared edge.
    pub fn trade(&self, slot: [u32; 2], city: &City) -> Vec<Deal> {
        let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) else { return Vec::new() };
        let mine = city.borders();
        let mut deals = Vec::new();
        for (i, r) in rules.resources.iter().enumerate().filter(|(_, r)| r.price > 0.0 && r.parent.is_none()) {
            let mut lack = gb.deficit.get(i).copied().unwrap_or(0.0);
            let mut spare = gb.surplus.get(i).copied().unwrap_or(0.0);
            for (side, open) in mine.iter().enumerate() {
                let Some(n) = self.neighbour(slot, side).filter(|n| *open && n.borders[(side + 2) % 4]) else { continue };
                let buy = lack.min(n.surplus.get(&r.id).copied().unwrap_or(0.0));
                if buy > 0.01 {
                    lack -= buy;
                    deals.push(Deal { partner: n.slot, resource: r.id.clone(), amount: buy });
                }
                let sell = spare.min(n.deficit.get(&r.id).copied().unwrap_or(0.0));
                if sell > 0.01 {
                    spare -= sell;
                    deals.push(Deal { partner: n.slot, resource: r.id.clone(), amount: -sell });
                }
            }
        }
        deals
    }
}

impl City {
    /// Sets the per-tick imports and exports from a month's deals.
    pub fn apply_trade(&mut self, deals: &[Deal]) {
        let Some(rules) = self.rules().cloned() else { return };
        let Some(gb) = self.glassbox.as_mut() else { return };
        let n = rules.resources.len();
        gb.imports = vec![0.0; n];
        gb.exports = vec![0.0; n];
        for d in deals {
            if let Some(i) = rules.resource(&d.resource) {
                if d.amount > 0.0 {
                    gb.imports[i] += d.amount;
                } else {
                    gb.exports[i] -= d.amount;
                }
            }
        }
    }
}
