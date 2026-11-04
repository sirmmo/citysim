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

/// A region hosted by a region server (`opencity-cli region-server`).
#[derive(Clone, Debug)]
pub struct Remote {
    pub url: String,
    pub token: String,
    pub player: String,
}

pub struct RegionSession {
    /// Set when the region is played over the network.
    pub remote: Option<Remote>,
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
        let s = Self { remote: None, dir: dir.to_owned(), region, slot: None, deals: Vec::new(), month: 0 };
        s.write()?;
        Ok(s)
    }

    pub fn open(dir: &Path) -> Result<Self> {
        let path = dir.join("region.json");
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let region = Region::from_json(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(Self { remote: None, dir: dir.to_owned(), region, slot: None, deals: Vec::new(), month: 0 })
    }

    pub fn write(&self) -> Result<()> {
        std::fs::write(self.dir.join("region.json"), self.region.to_json()).context("writing region.json")
    }

    /// Save file of a slot's city.
    pub fn city_file(slot: [u32; 2]) -> String {
        format!("city-{}-{}.ocsave.json", slot[0], slot[1])
    }

    pub fn city_path(&self, slot: [u32; 2]) -> PathBuf {
        let file = self.region.city(slot).map(|c| c.file.clone()).filter(|f| !f.is_empty()).unwrap_or_else(|| Self::city_file(slot));
        self.dir.join(file)
    }

    /// Who plays a slot's town, other than us (region servers).
    pub fn other_player(&self, slot: [u32; 2]) -> Option<String> {
        let r = self.remote.as_ref()?;
        self.region.city(slot).and_then(|c| c.player.clone()).filter(|p| p != &r.player)
    }

    /// A name for a new town in a slot.
    pub fn town_name(&self, slot: [u32; 2]) -> String {
        let i = (slot[1] * self.region.size[0] + slot[0]) as usize;
        NAMES[(i + self.region.seed as usize) % NAMES.len()].to_string()
    }

    /// Name of the open city.
    pub fn name(&self) -> Option<String> {
        self.slot.and_then(|s| self.region.city(s)).map(|c| c.name.clone()).filter(|n| !n.is_empty())
    }
}

/// Joins a region server: claims a slot and returns the session, with
/// the player's town kept in a local folder.
#[cfg(not(target_os = "android"))]
pub fn join(base: &Path, url: &str, player: &str, slot: [u32; 2]) -> Result<RegionSession> {
    let url = url.trim_end_matches('/').to_string();
    let body = serde_json::json!({ "slot": slot, "player": player }).to_string();
    let text = ureq::post(&format!("{url}/claim"))
        .timeout(std::time::Duration::from_secs(5))
        .set("Content-Type", "application/json")
        .send_string(&body)
        .map_err(|e| match e {
            ureq::Error::Status(_, r) => anyhow::anyhow!(r.into_string().unwrap_or_default()),
            e => anyhow::anyhow!(e),
        })?
        .into_string()?;
    let reply: serde_json::Value = serde_json::from_str(&text)?;
    let token = reply["token"].as_str().context("no token")?.to_string();
    let region: Region = serde_json::from_value(reply["region"].clone())?;
    let host: String = url.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
    let dir = regions_dir(base).join(format!("net-{}-{}-{}", host.trim_matches('-'), slot[0], slot[1]));
    std::fs::create_dir_all(&dir)?;
    Ok(RegionSession { remote: Some(Remote { url, token, player: player.to_string() }), dir, region, slot: None, deals: Vec::new(), month: 0 })
}

#[cfg(not(target_os = "android"))]
impl RegionSession {
    /// Fetches the latest summaries of every town.
    pub fn pull(&mut self) -> Result<()> {
        let Some(r) = &self.remote else { return Ok(()) };
        let text = ureq::get(&format!("{}/region", r.url)).timeout(std::time::Duration::from_secs(5)).call()?.into_string()?;
        let region: Region = serde_json::from_str(&text)?;
        self.region = region;
        Ok(())
    }

    /// Sends this town's summary.
    pub fn push(&self, slot: [u32; 2]) -> Result<()> {
        let (Some(r), Some(c)) = (&self.remote, self.region.city(slot)) else { return Ok(()) };
        ureq::put(&format!("{}/city", r.url))
            .timeout(std::time::Duration::from_secs(5))
            .set("Content-Type", "application/json")
            .send_string(&serde_json::json!({ "token": r.token, "summary": c }).to_string())?;
        Ok(())
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
