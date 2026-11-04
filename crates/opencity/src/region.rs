//! Playing a region: one open city among neighbours that trade with it.
//!
//! The region lives in a folder: `region.json` (see `opencity_core::region`)
//! and one save file per founded city. Switching cities saves the open one
//! and records its summary; every month the open city agrees its trade with
//! the neighbours from their summaries.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use opencity_core::region::{Deal, Region};

/// Names for new towns, picked by slot.
const NAMES: &[&str] = &[
    "Ashford",
    "Brookside",
    "Coldwater",
    "Dunmore",
    "Elmsworth",
    "Fairhaven",
    "Glenrock",
    "Highmoor",
    "Ironbridge",
    "Juniper Hill",
    "Kingsmere",
    "Larkfield",
    "Millbrook",
    "Northgate",
    "Oakhurst",
    "Pinecrest",
];

pub struct RegionSession {
    pub dir: PathBuf,
    pub region: Region,
    /// Slot of the open city.
    pub slot: Option<[u32; 2]>,
    /// This month's trade of the open city.
    pub deals: Vec<Deal>,
    /// Month the deals were agreed in.
    pub month: u32,
}

impl RegionSession {
    pub fn create(dir: &Path, region: Region) -> Result<Self> {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let s = Self { dir: dir.to_owned(), region, slot: None, deals: Vec::new(), month: 0 };
        s.write()?;
        Ok(s)
    }

    pub fn open(dir: &Path) -> Result<Self> {
        let path = dir.join("region.json");
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let region = Region::from_json(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(Self { dir: dir.to_owned(), region, slot: None, deals: Vec::new(), month: 0 })
    }

    pub fn write(&self) -> Result<()> {
        std::fs::write(self.dir.join("region.json"), self.region.to_json()).context("writing region.json")
    }

    /// Save file of a slot's city.
    pub fn city_file(slot: [u32; 2]) -> String {
        format!("city-{}-{}.ocsave.json", slot[0], slot[1])
    }

    pub fn city_path(&self, slot: [u32; 2]) -> PathBuf {
        self.dir.join(self.region.city(slot).map_or_else(|| Self::city_file(slot), |c| c.file.clone()))
    }

    /// A name for a new town in a slot.
    pub fn town_name(&self, slot: [u32; 2]) -> String {
        let i = (slot[1] * self.region.size[0] + slot[0]) as usize;
        NAMES[(i + self.region.seed as usize) % NAMES.len()].to_string()
    }

    /// Name of the open city.
    pub fn name(&self) -> Option<String> {
        self.slot.and_then(|s| self.region.city(s)).map(|c| c.name.clone())
    }
}

/// Where new regions go: next to the default save file.
pub fn regions_dir(save_path: &Path) -> PathBuf {
    save_path.parent().map_or_else(|| PathBuf::from("regions"), |p| p.join("regions"))
}

/// A folder name from a region name.
pub fn folder_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "region".into()
    } else {
        s
    }
}
