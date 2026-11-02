//! Loading OpenCity data files.
//!
//! * [`catalog`]: which 3D model shows each graphic code (`graphism.conf`),
//!   model footprints (`*.conf`) and descriptions (`*.xml`).
//! * [`ac3d`]: parser for the AC3D `.ac` model format used by every model.
//! * [`settings`]: user settings, TOML (replaces the old `opencity.xml`).

pub mod ac3d;
pub mod catalog;
pub mod settings;

use std::path::{Path, PathBuf};

pub use catalog::{Catalog, ModelEntry};
pub use settings::Settings;

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("cannot read {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{path}:{line}: {message}")]
    Parse { path: PathBuf, line: usize, message: String },
    #[error("invalid settings file {path}: {source}")]
    Settings { path: PathBuf, source: toml::de::Error },
    #[error("no OpenCity data directory found (looked in {0})")]
    NoDataDir(String),
}

pub(crate) fn read_text(path: &Path) -> Result<String, AssetError> {
    let bytes = std::fs::read(path).map_err(|source| AssetError::Io { path: path.to_owned(), source })?;
    // Legacy files are a mix of UTF-8 and Latin-1.
    Ok(match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
    })
}

/// Locates the data directory (the one containing `graphism.conf`):
/// the explicit path, `$OPENCITY_DATA`, next to the executable
/// (`./assets`, `../share/opencity`), then `./assets`.
pub fn find_data_dir(explicit: Option<&Path>) -> Result<PathBuf, AssetError> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = explicit {
        candidates.push(p.to_owned());
    }
    if let Some(p) = std::env::var_os("OPENCITY_DATA") {
        candidates.push(p.into());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("assets"));
            candidates.push(dir.join("../share/opencity"));
        }
    }
    candidates.push(PathBuf::from("assets"));
    candidates
        .iter()
        .find(|p| p.join("graphism.conf").is_file())
        .cloned()
        .ok_or_else(|| AssetError::NoDataDir(candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")))
}
