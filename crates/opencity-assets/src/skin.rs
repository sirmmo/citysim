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
    /// Used for cities with neither a date nor a world (new modern cities).
    #[serde(default)]
    pub default: bool,
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
    /// Show cars and trucks (people on foot and carts are always shown).
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
    era: Option<String>,
    #[serde(default)]
    culture: Option<String>,
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
    /// Era shown in the new-city dialog (e.g. "Antiquity"); defaults to the name.
    pub era: String,
    /// Culture within the era (e.g. "Romans"), if any.
    pub culture: Option<String>,
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

/// A skin model file: `models/…` is shared art under the data directory
/// (the Kenney kits); anything else is relative to the skin folder.
fn model_path(data_dir: &Path, dir: &Path, file: &str) -> PathBuf {
    if file.starts_with("models/") {
        data_dir.join(file)
    } else {
        dir.join(file)
    }
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
            if read_bytes(&model_path(data_dir, &dir, &file)).is_err() {
                log::debug!("skin {id}: missing model {file}");
                continue;
            }
            let code = code.parse::<u16>().map_err(|_| AssetError::Parse { path: path.clone(), line: 0, message: format!("bad graphic code {code:?}") })?;
            models.insert(code, model_path(data_dir, &dir, &file));
        }
        Ok(Self {
            id: id.to_string(),
            era: raw.era.unwrap_or_else(|| raw.name.clone()),
            culture: raw.culture,
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

    /// Skins listed in `<data>/skins/generated.toml` (written by art bakes,
    /// matched first) and `<data>/skins/index.toml`.
    pub fn load_all(data_dir: &Path) -> Vec<Self> {
        let mut out: Vec<Skin> = Vec::new();
        for file in ["generated.toml", "index.toml"] {
            let Ok(text) = read_text(&data_dir.join("skins").join(file)) else { continue };
            let Ok(index) = toml::from_str::<Index>(&text) else {
                log::warn!("invalid skins/{file}");
                continue;
            };
            for id in index.skins {
                if out.iter().any(|s| s.id == id) {
                    continue;
                }
                match Skin::load(data_dir, &id) {
                    Ok(s) => out.push(s),
                    Err(e) => log::warn!("skin {id}: {e}"),
                }
            }
        }
        out
    }

    /// Eras offered for new cities, oldest first (skins without dates last).
    pub fn eras(skins: &[Skin]) -> Vec<&str> {
        let mut eras: Vec<(&str, i64)> = Vec::new();
        for s in skins {
            let start = s.applies.years.map_or(i64::MAX, |y| y[0]);
            match eras.iter_mut().find(|(e, _)| *e == s.era) {
                Some(e) => e.1 = e.1.min(start),
                None => eras.push((&s.era, start)),
            }
        }
        eras.sort_by_key(|(_, start)| *start);
        eras.into_iter().map(|(e, _)| e).collect()
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
        match year {
            Some(y) => skins.iter().find(|s| s.applies.years.is_some_and(|[a, b]| (a..=b).contains(&y))),
            None if world.is_none() => skins.iter().find(|s| s.applies.default),
            None => None,
        }
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
        // Modern dates and undated cities get the Kenney city kits.
        assert_eq!(Skin::select(&skins, None, Some(2020), None).map(|s| s.id.as_str()), Some("modern"));
        assert_eq!(Skin::select(&skins, None, None, None).map(|s| s.id.as_str()), Some("modern"));
        let modern = skins.iter().find(|s| s.id == "modern").unwrap();
        assert!(modern.models.len() >= 28, "missing Kenney models: {}", modern.models.len());
        assert!(modern.models.values().all(|p| p.extension().is_some_and(|e| e == "obj")));
        assert_eq!(Skin::select(&skins, None, Some(1600), Some("Eldoria")).map(|s| s.id.as_str()), Some("fantasy"));
        assert_eq!(Skin::select(&skins, Some("industrial"), None, None).map(|s| s.id.as_str()), Some("industrial"));
        let r = Skin::select(&skins, None, Some(1600), None).unwrap();
        assert_eq!(r.label("coal_plant", "Coal plant"), "Watermill");
        assert!(!r.allows("nuclear_plant"));
    }

    #[test]
    fn eras_are_ordered_by_date() {
        let skins = Skin::load_all(&data_dir());
        let eras = Skin::eras(&skins);
        let pos = |e: &str| eras.iter().position(|x| *x == e).unwrap();
        assert!(pos("Antiquity") < pos("Renaissance"));
        assert!(pos("Renaissance") < pos("Industrial age"));
        assert_eq!(eras.last(), Some(&"Fantasy"));
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
