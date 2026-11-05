//! Loading OpenCity data files.
//!
//! * [`catalog`]: which 3D model shows each graphic code (`graphism.conf`),
//!   model footprints (`*.conf`) and descriptions (`*.xml`).
//! * [`ac3d`]: parser for the AC3D `.ac` model format used by every model.
//! * [`settings`]: user settings, TOML (replaces the old `opencity.xml`).

pub mod ac3d;
pub mod catalog;
pub mod compose;
pub mod obj;
pub mod pack;
pub mod rules;
pub mod scenario;
pub mod settings;
pub mod skin;

use std::path::{Path, PathBuf};

pub use catalog::{Catalog, ModelEntry};
pub use settings::Settings;
pub use skin::{Look, Skin};

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

/// Loads a building model: AC3D (`.ac`) as is, or Wavefront (`.obj`, the
/// Kenney kits) fitted to a footprint of `w` × `l` tiles, textured with the
/// `colormap.png` next to it.
pub fn load_building_model(path: &Path, footprint: (u32, u32)) -> Result<ac3d::Model, AssetError> {
    if compose::is_composed(path) {
        compose::load_variant(path, footprint, 0)
    } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("obj")) {
        let obj = obj::load(path)?;
        let tex = path.parent().map(|d| d.join("colormap.png")).filter(|t| read_bytes(t).is_ok());
        Ok(obj::to_building(&obj, tex, footprint))
    } else {
        ac3d::load(path)
    }
}

/// How data files are read. Desktop builds read the filesystem; Android
/// installs a reader for the assets packed in the APK.
pub type FileReader = fn(&Path) -> std::io::Result<Vec<u8>>;

static READER: std::sync::OnceLock<FileReader> = std::sync::OnceLock::new();

/// Replaces the filesystem reader (call once, before loading anything).
pub fn set_file_reader(reader: FileReader) {
    let _ = READER.set(reader);
}

/// Reads a data file through the installed reader.
pub fn read_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    match READER.get() {
        Some(r) => r(path),
        None => std::fs::read(path),
    }
}

pub(crate) fn read_text(path: &Path) -> Result<String, AssetError> {
    let bytes = read_bytes(path).map_err(|source| AssetError::Io { path: path.to_owned(), source })?;
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
    // Android: assets sit at the root of the APK.
    candidates.push(PathBuf::new());
    candidates
        .iter()
        .find(|p| read_bytes(&p.join("graphism.conf")).is_ok())
        .cloned()
        .ok_or_else(|| AssetError::NoDataDir(candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")))
}
