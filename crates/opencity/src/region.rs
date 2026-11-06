//! Playing a region: one open city among neighbours that trade with it.
//!
//! The region lives in a folder: `region.json` (see `opencity_core::region`)
//! and one save file per founded city (in the browser, under those names in
//! local storage). Switching cities saves the open one and records its
//! summary; every month the open city agrees its trade with the neighbours
//! from their summaries.
//!
//! On a region server (`opencity-cli region-server`) the summaries are
//! exchanged over HTTP. The desktop asks and waits; the browser can only
//! start a request and read the answer at a later frame, so joining,
//! pulling and pushing are all polled from the game loop.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use opencity_core::region::{Deal, Region};

/// Reads a text file of a region (local storage in the browser).
pub fn read_text(path: &Path) -> std::io::Result<String> {
    #[cfg(target_arch = "wasm32")]
    return crate::web::storage_read(&path.to_string_lossy());
    #[cfg(not(target_arch = "wasm32"))]
    std::fs::read_to_string(path)
}

/// Writes a text file of a region, creating its folder.
pub fn write_text(path: &Path, text: &str) -> std::io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    return crate::web::storage_write(&path.to_string_lossy(), text);
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, text)
    }
}

/// Whether a region file (a town's save, `region.json`) exists.
pub fn exists(path: &Path) -> bool {
    #[cfg(target_arch = "wasm32")]
    return crate::web::storage_keys().iter().any(|k| k == &*path.to_string_lossy());
    #[cfg(not(target_arch = "wasm32"))]
    path.is_file()
}

/// The region folders under `dir` (those holding a `region.json`), sorted.
pub fn list(dir: &Path) -> Vec<PathBuf> {
    #[cfg(target_arch = "wasm32")]
    let mut found: Vec<PathBuf> = {
        let prefix = format!("{}/", dir.to_string_lossy().trim_end_matches('/'));
        crate::web::storage_keys()
            .iter()
            .filter_map(|k| k.strip_prefix(&prefix).and_then(|rest| rest.strip_suffix("/region.json")).filter(|name| !name.contains('/')))
            .map(|name| dir.join(name))
            .collect()
    };
    #[cfg(not(target_arch = "wasm32"))]
    let mut found: Vec<PathBuf> =
        std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.join("region.json").is_file()).collect()).unwrap_or_default();
    found.sort();
    found
}

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
    #[cfg_attr(target_os = "android", allow(dead_code))]
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
    /// Requests to the region server still unanswered (browser).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pending: Vec<Pending>,
}

/// A request to the region server in flight.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
struct Pending {
    #[cfg(target_arch = "wasm32")]
    request: crate::web::Request,
    /// True for a pull (the answer is the region), false for a push.
    pull: bool,
}

impl RegionSession {
    fn new(remote: Option<Remote>, dir: PathBuf, region: Region) -> Self {
        Self { remote, dir, region, slot: None, deals: Vec::new(), month: 0, pending: Vec::new() }
    }

    pub fn create(dir: &Path, region: Region) -> Result<Self> {
        let s = Self::new(None, dir.to_owned(), region);
        s.write()?;
        Ok(s)
    }

    pub fn open(dir: &Path) -> Result<Self> {
        let path = dir.join("region.json");
        let text = read_text(&path).with_context(|| format!("reading {}", path.display()))?;
        let region = Region::from_json(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(Self::new(None, dir.to_owned(), region))
    }

    pub fn write(&self) -> Result<()> {
        write_text(&self.dir.join("region.json"), &self.region.to_json()).context("writing region.json")
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
        // A name the region file gives the slot comes first.
        if let Some(c) = self.region.city(slot).filter(|c| !c.name.is_empty()) {
            return c.name.clone();
        }
        let i = (slot[1] * self.region.size[0] + slot[0]) as usize;
        NAMES[(i + self.region.seed as usize) % NAMES.len()].to_string()
    }

    /// Name of the open city.
    pub fn name(&self) -> Option<String> {
        self.slot.and_then(|s| self.region.city(s)).map(|c| c.name.clone()).filter(|n| !n.is_empty())
    }
}

/// Joining a region server: claiming a slot. The desktop finishes at the
/// first poll; the browser when the answer arrives.
pub struct Joining {
    base: PathBuf,
    url: String,
    player: String,
    pub slot: [u32; 2],
    #[cfg(target_arch = "wasm32")]
    request: crate::web::Request,
}

/// The body of a claim.
fn claim_body(player: &str, slot: [u32; 2]) -> String {
    serde_json::json!({ "slot": slot, "player": player }).to_string()
}

/// Starts joining a region server at `url` as `player`, for a town slot.
pub fn join(base: &Path, url: &str, player: &str, slot: [u32; 2]) -> Joining {
    let url = url.trim().trim_end_matches('/').to_string();
    let url = if url.contains("://") { url } else { format!("http://{url}") };
    #[cfg(target_arch = "wasm32")]
    let request = crate::web::http_start("POST", &format!("{url}/claim"), &claim_body(player, slot));
    Joining {
        base: base.to_owned(),
        url,
        player: player.to_string(),
        slot,
        #[cfg(target_arch = "wasm32")]
        request,
    }
}

impl Joining {
    /// The session from the server's answer to the claim: the player's
    /// town is kept in a local folder named after the server and slot.
    fn session(&self, status: u16, text: &str) -> Result<RegionSession> {
        if status != 200 {
            let reply: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
            anyhow::bail!("{}", reply["error"].as_str().map_or_else(|| format!("the server answered {status}"), str::to_string));
        }
        let reply: serde_json::Value = serde_json::from_str(text).context("the answer is not JSON (is that a region server?)")?;
        let token = reply["token"].as_str().context("no token")?.to_string();
        let region: Region = serde_json::from_value(reply["region"].clone())?;
        let host: String = self.url.chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
        let dir = regions_dir(&self.base).join(format!("net-{}-{}-{}", host.trim_matches('-'), self.slot[0], self.slot[1]));
        Ok(RegionSession::new(Some(Remote { url: self.url.clone(), token, player: self.player.clone() }), dir, region))
    }
}

#[cfg(target_arch = "wasm32")]
impl Joining {
    pub fn poll(&mut self) -> Option<Result<RegionSession>> {
        let answer = crate::web::http_poll(&self.request)?;
        Some(answer.and_then(|(status, text)| self.session(status, &text)))
    }
}

#[cfg(target_arch = "wasm32")]
impl RegionSession {
    /// Asks for the latest summaries of every town (one request at a time).
    pub fn pull(&mut self) -> Result<()> {
        let Some(r) = &self.remote else { return Ok(()) };
        if self.pending.iter().any(|p| p.pull) {
            return Ok(());
        }
        let request = crate::web::http_start("GET", &format!("{}/region", r.url), "");
        self.pending.push(Pending { request, pull: true });
        Ok(())
    }

    /// Sends this town's summary.
    pub fn push(&mut self, slot: [u32; 2]) -> Result<()> {
        let (Some(r), Some(c)) = (&self.remote, self.region.city(slot)) else { return Ok(()) };
        let body = serde_json::json!({ "token": r.token, "summary": c }).to_string();
        let request = crate::web::http_start("PUT", &format!("{}/city", r.url), &body);
        self.pending.push(Pending { request, pull: false });
        Ok(())
    }

    /// Takes in the answers that arrived: true when the region was
    /// refreshed by a pull, an error when the server refused or is down.
    pub fn poll(&mut self) -> Result<bool> {
        let mut fresh = false;
        let mut failed = None;
        let mut i = 0;
        while i < self.pending.len() {
            let Some(answer) = crate::web::http_poll(&self.pending[i].request) else {
                i += 1;
                continue;
            };
            let p = self.pending.remove(i);
            match answer {
                Ok((200, text)) if p.pull => match Region::from_json(&text) {
                    Ok(region) => {
                        self.region = region;
                        fresh = true;
                    }
                    Err(e) => failed = Some(anyhow::anyhow!("bad region from the server: {e}")),
                },
                Ok((200, _)) => {}
                Ok((status, text)) => {
                    let reply: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                    failed = Some(anyhow::anyhow!("{}", reply["error"].as_str().map_or_else(|| format!("the server answered {status}"), str::to_string)));
                }
                Err(e) => failed = Some(e),
            }
        }
        match failed {
            Some(e) => Err(e),
            None => Ok(fresh),
        }
    }
}

/// Android has no network layer: the region plays alone.
#[cfg(target_os = "android")]
impl Joining {
    pub fn poll(&mut self) -> Option<Result<RegionSession>> {
        Some(Err(anyhow::anyhow!("joining a region server is not available on Android")))
    }
}

#[cfg(target_os = "android")]
impl RegionSession {
    pub fn pull(&mut self) -> Result<()> {
        Ok(())
    }
    pub fn push(&mut self, _slot: [u32; 2]) -> Result<()> {
        Ok(())
    }
    pub fn poll(&mut self) -> Result<bool> {
        Ok(false)
    }
}

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
impl Joining {
    /// Claims the slot, waiting for the answer.
    pub fn poll(&mut self) -> Option<Result<RegionSession>> {
        let sent = ureq::post(&format!("{}/claim", self.url))
            .timeout(std::time::Duration::from_secs(5))
            .set("Content-Type", "application/json")
            .send_string(&claim_body(&self.player, self.slot));
        let (status, text) = match sent {
            Ok(r) => (200, r.into_string().unwrap_or_default()),
            Err(ureq::Error::Status(code, r)) => (code, r.into_string().unwrap_or_default()),
            Err(e) => return Some(Err(anyhow::anyhow!(e))),
        };
        Some(self.session(status, &text))
    }
}

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
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
    pub fn push(&mut self, slot: [u32; 2]) -> Result<()> {
        let (Some(r), Some(c)) = (&self.remote, self.region.city(slot)) else { return Ok(()) };
        ureq::put(&format!("{}/city", r.url))
            .timeout(std::time::Duration::from_secs(5))
            .set("Content-Type", "application/json")
            .send_string(&serde_json::json!({ "token": r.token, "summary": c }).to_string())?;
        Ok(())
    }

    /// Requests are answered at once on the desktop: nothing to take in.
    pub fn poll(&mut self) -> Result<bool> {
        Ok(false)
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
