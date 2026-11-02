//! GlassBox rules files (`<data>/rules/*.toml`).

use std::path::Path;
use std::sync::Arc;

use opencity_core::rules::Ruleset;
use serde::Deserialize;

use crate::{read_text, AssetError};

#[derive(Deserialize)]
struct Index {
    rules: Vec<String>,
}

/// Loads and validates one rules file by id.
pub fn load(data_dir: &Path, id: &str) -> Result<Arc<Ruleset>, AssetError> {
    let path = data_dir.join("rules").join(format!("{id}.toml"));
    let mut rules: Ruleset = toml::from_str(&read_text(&path)?).map_err(|source| AssetError::Settings { path: path.clone(), source })?;
    for script in rules.scripts.clone() {
        let file = data_dir.join("rules").join(&script);
        let text = read_text(&file)?;
        let warnings =
            opencity_core::gbx::compile(&mut rules, &script, &text).map_err(|e| AssetError::Parse { path: file.clone(), line: 0, message: e.message })?;
        warnings.iter().for_each(|w| log::info!("{w}"));
    }
    rules.validate().map_err(|e| AssetError::Parse { path, line: 0, message: e.to_string() })?;
    Ok(Arc::new(rules))
}

/// Every rules set listed in `<data>/rules/index.toml`.
pub fn load_all(data_dir: &Path) -> Vec<Arc<Ruleset>> {
    let Ok(text) = read_text(&data_dir.join("rules").join("index.toml")) else { return Vec::new() };
    let Ok(index) = toml::from_str::<Index>(&text) else {
        log::warn!("invalid rules/index.toml");
        return Vec::new();
    };
    index.rules.iter().filter_map(|id| load(data_dir, id).map_err(|e| log::warn!("rules {id}: {e}")).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_rules_load() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let all = load_all(&dir);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, "glassbox");
    }
}
