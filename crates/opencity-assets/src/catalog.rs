//! The model catalog (legacy `graphism.conf` + per-model `.conf` / `.xml`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use opencity_core::{Footprint, Footprints, GraphicCode};

use crate::{read_text, AssetError};

#[derive(Clone, Debug, PartialEq)]
pub struct ModelEntry {
    /// `.ac` model file, absolute or relative to the working directory.
    pub model: PathBuf,
    pub footprint: Footprint,
    /// English name, if the model has metadata.
    pub name: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub data_dir: PathBuf,
    pub models: BTreeMap<GraphicCode, ModelEntry>,
}

/// Parses `key=value` lines, ignoring blanks and `#` comments.
pub fn parse_key_values(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let (k, v) = line.split_once('=')?;
        Some((k.trim(), v.trim()))
    })
}

impl Catalog {
    pub fn load(data_dir: &Path) -> Result<Self, AssetError> {
        let index_path = data_dir.join("graphism.conf");
        let index = read_text(&index_path)?;
        let mut models = BTreeMap::new();
        for (key, rel) in parse_key_values(&index) {
            let Ok(code) = key.parse::<u16>() else { continue };
            let model = data_dir.join(rel);
            let mut entry = ModelEntry { model: model.clone(), footprint: Footprint::default(), name: None, description: None, author: None };
            let conf_path = model.with_extension("conf");
            match read_text(&conf_path) {
                Ok(conf) => {
                    for (k, v) in parse_key_values(&conf) {
                        match k {
                            "width" => entry.footprint.width = v.parse::<f32>().map_or(1, |f| f.round().max(1.0) as u32),
                            "length" => entry.footprint.length = v.parse::<f32>().map_or(1, |f| f.round().max(1.0) as u32),
                            "height" => entry.footprint.height = v.parse().unwrap_or(1.0),
                            "name" => entry.name = Some(v.to_string()),
                            "description" => entry.description = Some(v.to_string()),
                            "author" => entry.author = Some(v.to_string()),
                            _ => {}
                        }
                    }
                }
                Err(e) => log::warn!("model {code}: {e}"),
            }
            if let Ok(xml) = read_text(&model.with_extension("xml")) {
                apply_xml_metadata(&mut entry, &xml);
            }
            models.insert(GraphicCode(code), entry);
        }
        log::info!("catalog: {} models from {}", models.len(), index_path.display());
        Ok(Self { data_dir: data_dir.to_owned(), models })
    }

    pub fn footprints(&self) -> Footprints {
        let mut f = Footprints::new();
        for (&gc, e) in &self.models {
            f.insert(gc, e.footprint);
        }
        f
    }

    pub fn get(&self, gc: GraphicCode) -> Option<&ModelEntry> {
        self.models.get(&gc)
    }

    /// Replaces model files with the skin's versions (footprints are kept so
    /// the simulation is unaffected).
    pub fn with_skin(&self, skin: &crate::Skin) -> Self {
        let mut c = self.clone();
        for (code, path) in &skin.models {
            if let Some(e) = c.models.get_mut(&GraphicCode(*code)) {
                e.model = path.clone();
            }
        }
        c
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.data_dir.join(rel)
    }
}

/// Reads the English name/description from a model's XML metadata.
fn apply_xml_metadata(entry: &mut ModelEntry, xml: &str) {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return };
    let english = |tag: &str| {
        doc.descendants()
            .filter(|n| n.has_tag_name(tag) && n.parent().is_some_and(|p| p.has_tag_name("model")))
            .find(|n| n.attribute("lang") == Some("en"))
            .and_then(|n| n.text())
            .map(str::to_string)
    };
    if let Some(n) = english("name") {
        entry.name = Some(n);
    }
    if let Some(d) = english("description") {
        entry.description = Some(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    #[test]
    fn loads_shipped_catalog() {
        let c = Catalog::load(&data_dir()).unwrap();
        assert_eq!(c.models.len(), 81);
        let coal = c.get(GraphicCode::EPLANT_COAL).unwrap();
        assert_eq!((coal.footprint.width, coal.footprint.length), (5, 3));
        assert!(coal.model.is_file());
        let family = c.get(GraphicCode(13)).unwrap();
        assert_eq!(family.name.as_deref(), Some("USA family's house"));
    }

    #[test]
    fn shipped_footprints_match_builtin_table() {
        let c = Catalog::load(&data_dir()).unwrap().footprints();
        let b = Footprints::builtin();
        for gc in 0..GraphicCode::MAX {
            assert_eq!(c.size(GraphicCode(gc)), b.size(GraphicCode(gc)), "graphic code {gc}");
        }
    }
}
