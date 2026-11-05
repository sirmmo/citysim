//! Packs: a scenario sealed with its skin and rules, launched as a game of
//! its own (`--pack <id>` or `<data>/pack.toml`).
//!
//! A pack names a scenario and may override its skin and rules. While a
//! pack is running the player cannot change them: no new cities, regions,
//! imports or other scenarios, no skin menu, no cheats and no disaster
//! switch unless the pack allows them.

use std::path::{Path, PathBuf};

use opencity_core::goals::Scenario;
use serde::Deserialize;

use crate::{read_text, AssetError};

/// One pack file.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    /// File name without `.toml`; also names the pack's save file.
    #[serde(default)]
    pub id: String,
    /// Window title and menu name.
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Scenario id (`<data>/scenarios/<id>.toml`).
    pub scenario: String,
    /// Skin id that replaces the scenario's; `"none"` for OpenCity's own look.
    #[serde(default)]
    pub skin: Option<String>,
    /// Rules id that replaces the scenario's.
    #[serde(default)]
    pub rules: Option<String>,
    /// Whether the cheat box opens.
    #[serde(default)]
    pub cheats: bool,
    /// Whether the Disasters menu (chance switch, set off) is shown.
    #[serde(default)]
    pub disaster_menu: bool,
}

/// The launcher file that turns a data folder into a one-pack game.
pub const LAUNCHER: &str = "pack.toml";

impl Pack {
    fn from_path(path: &Path, id: &str) -> Result<Self, AssetError> {
        Self::parse(path, &read_text(path)?, id)
    }

    fn parse(path: &Path, text: &str, id: &str) -> Result<Self, AssetError> {
        let mut p: Pack = toml::from_str(text).map_err(|source| AssetError::Settings { path: path.to_path_buf(), source })?;
        if p.id.is_empty() {
            p.id = id.to_string();
        }
        Ok(p)
    }

    /// `<data>/packs/<id>.toml`.
    pub fn load(data_dir: &Path, id: &str) -> Result<Self, AssetError> {
        Self::from_path(&data_dir.join("packs").join(format!("{id}.toml")), id)
    }

    /// The pack sealed into this data folder (`<data>/pack.toml`), if any.
    pub fn launcher(data_dir: &Path) -> Option<Result<Self, AssetError>> {
        // Read through the asset reader: on Android the data is in the APK.
        let path = data_dir.join(LAUNCHER);
        let text = read_text(&path).ok()?;
        Some(Self::parse(&path, &text, "pack"))
    }

    /// Every pack in `<data>/packs/`, by id.
    pub fn load_all(data_dir: &Path) -> Vec<Self> {
        let Ok(dir) = std::fs::read_dir(data_dir.join("packs")) else { return Vec::new() };
        let mut ids: Vec<String> = dir.filter_map(|e| e.ok()?.file_name().to_str()?.strip_suffix(".toml").map(str::to_string)).collect();
        ids.sort();
        ids.iter().filter_map(|id| Self::load(data_dir, id).map_err(|e| log::warn!("pack {id}: {e}")).ok()).collect()
    }

    /// The pack's scenario with the pack's skin and rules in place.
    pub fn scenario(&self, data_dir: &Path) -> Result<Scenario, AssetError> {
        let mut s = crate::scenario::load(data_dir, &self.scenario)?;
        if let Some(skin) = &self.skin {
            s.start.skin = Some(skin.clone());
        }
        if let Some(rules) = &self.rules {
            s.start.rules = Some(rules.clone());
        }
        Ok(s)
    }

    /// Where the pack's city is saved, beside `default_save`.
    pub fn save_path(&self, default_save: &Path) -> PathBuf {
        default_save.with_file_name(format!("{}.ocsave.json", self.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    #[test]
    fn shipped_packs_build_their_sealed_city() {
        let data = data_dir();
        let packs = Pack::load_all(&data);
        assert!(!packs.is_empty());
        let catalog = crate::Catalog::load(&data).unwrap();
        let skins = crate::Skin::load_all(&data);
        for p in &packs {
            let s = p.scenario(&data).unwrap_or_else(|e| panic!("pack {}: {e}", p.id));
            if let Some(skin) = s.start.skin.as_deref().filter(|s| *s != "none") {
                assert!(skins.iter().any(|k| k.id == skin), "pack {}: unknown skin {skin}", p.id);
            }
            let city = crate::scenario::build(&data, &s, catalog.footprints()).unwrap_or_else(|e| panic!("pack {}: {e}", p.id));
            assert!(city.goals.as_ref().is_some_and(|g| !g.goals.is_empty()), "pack {} has no goals", p.id);
            if let Some(r) = &p.rules {
                assert_eq!(city.rules().map(|x| x.id.as_str()), Some(r.as_str()));
            }
        }
    }

    #[test]
    fn launcher_and_save_path() {
        let data = std::env::temp_dir().join(format!("citysim-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        assert!(Pack::launcher(&data).is_none());
        std::fs::write(data.join(LAUNCHER), "id = \"roma\"\nname = \"Roma\"\nscenario = \"first-town\"\n").unwrap();
        let p = Pack::launcher(&data).unwrap().unwrap();
        assert_eq!((p.id.as_str(), p.cheats, p.disaster_menu), ("roma", false, false));
        assert_eq!(p.save_path(Path::new("/s/city.ocsave.json")), PathBuf::from("/s/roma.ocsave.json"));
        std::fs::write(data.join(LAUNCHER), "name = \"x\"\nscenario = \"a\"\nskins = 1\n").unwrap();
        assert!(Pack::launcher(&data).unwrap().is_err(), "unknown keys are refused");
        let _ = std::fs::remove_dir_all(&data);
    }
}
