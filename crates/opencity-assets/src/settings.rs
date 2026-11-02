//! User settings (replaces the legacy `config/opencity.xml`).

use std::path::{Path, PathBuf};

use opencity_mapgen::MapParams;
use serde::{Deserialize, Serialize};

use crate::AssetError;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub window: WindowSettings,
    pub audio: AudioSettings,
    /// Map used for new cities.
    pub map: MapParams,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WindowSettings {
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    /// Multisample anti-aliasing samples (0 disables).
    pub msaa: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AudioSettings {
    pub enabled: bool,
    /// 0.0 ..= 1.0
    pub volume: f32,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self { width: 1280, height: 800, fullscreen: false, msaa: 4 }
    }
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self { enabled: true, volume: 0.6 }
    }
}

impl Settings {
    /// `$XDG_CONFIG_HOME/opencity/settings.toml`, `~/.config/...` or `%APPDATA%\opencity\...`.
    pub fn default_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("opencity").join("settings.toml"))
    }

    /// Loads settings; a missing file yields the defaults.
    pub fn load(path: &Path) -> Result<Self, AssetError> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|source| AssetError::Settings { path: path.to_owned(), source }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(AssetError::Io { path: path.to_owned(), source }),
        }
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("settings serialise")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_file_uses_defaults() {
        let s: Settings = toml::from_str("[map]\nwidth = 80\nlength = 60\nrelief = \"hill\"\n").unwrap();
        assert_eq!(s.map.width, 80);
        assert_eq!(s.map.relief, opencity_mapgen::Relief::Hill);
        assert_eq!(s.window, WindowSettings::default());
        let back: Settings = toml::from_str(&s.to_toml()).unwrap();
        assert_eq!(back, s);
    }
}
