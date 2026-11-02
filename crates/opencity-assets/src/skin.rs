//! Skin packs: re-dress the game for an era (historical maps) or a world
//! (fantasy maps) without touching the simulation.
//!
//! A skin lives in `<data>/skins/<id>/skin.toml` and is listed in
//! `<data>/skins/index.toml`. Any file placed under the skin folder with the
//! same relative path as a base asset (for example
//! `skins/renaissance/graphism/gui/road.png`) replaces it. See docs/SKINS.md.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{read_bytes, read_text, AssetError};

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Applies {
    /// Inclusive year range, for cities imported with a date.
    pub years: Option<[i64; 2]>,
    /// World names (case-insensitive; `*` wildcards at either end).
    #[serde(default)]
    pub worlds: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Look {
    /// Background colour (RGB, 0..1).
    pub sky: [f32; 3],
    /// Multiplier applied to model and terrain colours.
    pub tint: [f32; 3],
    /// 1 keeps colours, 0 makes them grey.
    pub saturation: f32,
    /// Water plane colour (RGBA, 0..1).
    pub water: [f32; 4],
    /// UI accent and panel colours (RGB, 0..255).
    pub ui_accent: [u8; 3],
    pub ui_panel: [u8; 3],
    /// Show cars and trucks.
    pub vehicles: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            sky: [0.45, 0.62, 0.85],
            tint: [1.0, 1.0, 1.0],
            saturation: 1.0,
            water: [0.15, 0.35, 0.75, 0.55],
            ui_accent: [52, 120, 200],
            ui_panel: [22, 26, 34],
            vehicles: true,
        }
    }
}

impl Look {
    /// Applies saturation and tint to a linear RGBA colour.
    pub fn apply(&self, c: [f32; 4]) -> [f32; 4] {
        let grey = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        let mix = |v: f32, i: usize| ((grey + (v - grey) * self.saturation) * self.tint[i]).clamp(0.0, 1.0);
        [mix(c[0], 0), mix(c[1], 1), mix(c[2], 2), c[3]]
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct RawSkin {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    applies: Applies,
    #[serde(default)]
    look: Look,
    /// Tool id -> display name.
    #[serde(default)]
    labels: BTreeMap<String, String>,
    /// Tool ids hidden in this skin.
    #[serde(default)]
    disabled_tools: Vec<String>,
    /// Graphic code -> model file (relative to the skin folder).
    #[serde(default)]
    models: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Skin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub applies: Applies,
    pub look: Look,
    pub labels: BTreeMap<String, String>,
    pub disabled_tools: Vec<String>,
    pub models: BTreeMap<u16, PathBuf>,
    /// Folder holding the skin's override files.
    pub dir: PathBuf,
}

#[derive(Deserialize)]
struct Index {
    skins: Vec<String>,
}

fn world_matches(pattern: &str, world: &str) -> bool {
    let (p, w) = (pattern.to_lowercase(), world.to_lowercase());
    match (p.strip_prefix('*'), p.strip_suffix('*')) {
        _ if p == "*" => true,
        (Some(rest), _) if rest.ends_with('*') => w.contains(rest.trim_end_matches('*')),
        (Some(rest), _) => w.ends_with(rest),
        (None, Some(rest)) => w.starts_with(rest),
        (None, None) => p == w,
    }
}

impl Skin {
    pub fn load(data_dir: &Path, id: &str) -> Result<Self, AssetError> {
        let dir = data_dir.join("skins").join(id);
        let path = dir.join("skin.toml");
        let raw: RawSkin = toml::from_str(&read_text(&path)?).map_err(|source| AssetError::Settings { path: path.clone(), source })?;
        let mut raw_models = raw.models;
        // Optional generated list (e.g. by tools/zeroad/bake_0ad.py).
        if let Ok(text) = read_text(&dir.join("models.toml")) {
            let generated: BTreeMap<String, String> = toml::from_str(&text).map_err(|source| AssetError::Settings { path: dir.join("models.toml"), source })?;
            raw_models.extend(generated);
        }
        let mut models = BTreeMap::new();
        for (code, file) in raw_models {
            // Art that is not baked/installed is simply not used.
            if read_bytes(&dir.join(&file)).is_err() {
                log::debug!("skin {id}: missing model {file}");
                continue;
            }
            let code = code.parse::<u16>().map_err(|_| AssetError::Parse { path: path.clone(), line: 0, message: format!("bad graphic code {code:?}") })?;
            models.insert(code, dir.join(file));
        }
        Ok(Self {
            id: id.to_string(),
            name: raw.name,
            description: raw.description,
            applies: raw.applies,
            look: raw.look,
            labels: raw.labels,
            disabled_tools: raw.disabled_tools,
            models,
            dir,
        })
    }

    /// Every skin listed in `<data>/skins/index.toml` (none if missing).
    pub fn load_all(data_dir: &Path) -> Vec<Self> {
        let Ok(text) = read_text(&data_dir.join("skins").join("index.toml")) else { return Vec::new() };
        let Ok(index) = toml::from_str::<Index>(&text) else {
            log::warn!("invalid skins/index.toml");
            return Vec::new();
        };
        index.skins.iter().filter_map(|id| Skin::load(data_dir, id).map_err(|e| log::warn!("skin {id}: {e}")).ok()).collect()
    }

    /// Picks the skin for a city: an explicit id, else a world match, else an era match.
    pub fn select<'a>(skins: &'a [Skin], explicit: Option<&str>, year: Option<i64>, world: Option<&str>) -> Option<&'a Skin> {
        if let Some(id) = explicit {
            return skins.iter().find(|s| s.id == id);
        }
        if let Some(w) = world {
            if let Some(s) = skins.iter().find(|s| s.applies.worlds.iter().any(|p| world_matches(p, w))) {
                return Some(s);
            }
        }
        let y = year?;
        skins.iter().find(|s| s.applies.years.is_some_and(|[a, b]| (a..=b).contains(&y)))
    }

    /// The skin's version of a data file if it has one, else the base file.
    pub fn resolve(&self, data_dir: &Path, rel: &str) -> PathBuf {
        let own = self.dir.join(rel);
        if read_bytes(&own).is_ok() {
            own
        } else {
            data_dir.join(rel)
        }
    }

    pub fn label<'a>(&'a self, tool_id: &str, default: &'a str) -> &'a str {
        self.labels.get(tool_id).map(String::as_str).unwrap_or(default)
    }

    pub fn allows(&self, tool_id: &str) -> bool {
        !self.disabled_tools.iter().any(|t| t == tool_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    #[test]
    fn shipped_skins_load_and_select() {
        let skins = Skin::load_all(&data_dir());
        assert!(skins.len() >= 3);
        assert_eq!(Skin::select(&skins, None, Some(1600), None).map(|s| s.id.as_str()), Some("renaissance"));
        assert_eq!(Skin::select(&skins, None, Some(1890), None).map(|s| s.id.as_str()), Some("industrial"));
        assert_eq!(Skin::select(&skins, None, Some(2020), None), None);
        assert_eq!(Skin::select(&skins, None, Some(1600), Some("Eldoria")).map(|s| s.id.as_str()), Some("fantasy"));
        assert_eq!(Skin::select(&skins, Some("industrial"), None, None).map(|s| s.id.as_str()), Some("industrial"));
        let r = Skin::select(&skins, None, Some(1600), None).unwrap();
        assert_eq!(r.label("coal_plant", "Coal plant"), "Watermill");
        assert!(!r.allows("nuclear_plant"));
    }

    #[test]
    fn wildcards() {
        assert!(world_matches("*", "anything"));
        assert!(world_matches("eldor*", "Eldoria"));
        assert!(world_matches("*ria", "Eldoria"));
        assert!(world_matches("*dor*", "Eldoria"));
        assert!(!world_matches("mordor", "Eldoria"));
    }

    #[test]
    fn look_desaturates_and_tints() {
        let look = Look { saturation: 0.0, tint: [1.0, 0.5, 1.0], ..Default::default() };
        let c = look.apply([1.0, 0.0, 0.0, 1.0]);
        assert!((c[0] - 0.299).abs() < 1e-4 && (c[1] - 0.1495).abs() < 1e-4);
    }
}
