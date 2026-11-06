//! Scenario files (`<data>/scenarios/<id>.toml`, listed in `index.toml`)
//! and the city each one starts with.

use std::path::Path;

use opencity_core::goals::Scenario;
use opencity_core::{City, Footprints, Terrain};
use serde::Deserialize;

use crate::{read_text, AssetError};

#[derive(Deserialize)]
struct Index {
    scenarios: Vec<String>,
}

/// Reads one scenario by id.
pub fn load(data_dir: &Path, id: &str) -> Result<Scenario, AssetError> {
    let path = data_dir.join("scenarios").join(format!("{id}.toml"));
    toml::from_str(&read_text(&path)?).map_err(|source| AssetError::Settings { path, source })
}

/// Every scenario listed in `<data>/scenarios/index.toml`.
pub fn load_all(data_dir: &Path) -> Vec<Scenario> {
    let Ok(text) = read_text(&data_dir.join("scenarios").join("index.toml")) else { return Vec::new() };
    let Ok(index) = toml::from_str::<Index>(&text) else {
        log::warn!("invalid scenarios/index.toml");
        return Vec::new();
    };
    index.scenarios.iter().filter_map(|id| load(data_dir, id).map_err(|e| log::warn!("scenario {id}: {e}")).ok()).collect()
}

fn parse<T: serde::de::DeserializeOwned>(what: &str, v: &str) -> Result<T, AssetError> {
    toml::Value::String(v.to_string()).try_into().map_err(|_| AssetError::Parse { path: "scenario".into(), line: 0, message: format!("unknown {what} {v:?}") })
}

/// The city a scenario starts with: its map (generated, flat or imported),
/// rules, money, demo town and research, and its goals set. The skin is
/// the caller's to apply (`scenario.start.skin`).
pub fn build(data_dir: &Path, s: &Scenario, footprints: Footprints) -> Result<City, AssetError> {
    let st = &s.start;
    let mut city = if let Some(file) = &st.geojson {
        let path = data_dir.join(file);
        let text = read_text(&path)?;
        let opts =
            opencity_geo::ImportOptions { date: st.date.as_deref().and_then(opencity_geo::HistDate::parse), name: Some(s.name.clone()), ..Default::default() };
        let (city, _) = opencity_geo::import(&text, &opencity_geo::Rules::builtin(), footprints, &opts).map_err(|e| AssetError::Parse {
            path: path.clone(),
            line: 0,
            message: e.to_string(),
        })?;
        city
    } else if st.flat {
        City::new(Terrain::flat(st.width, st.length), None, footprints, st.seed)
    } else {
        let params = opencity_mapgen::MapParams {
            width: st.width,
            length: st.length,
            relief: parse("relief", &st.relief)?,
            water: parse("water", &st.water)?,
            shape: parse("shape", &st.shape)?,
            trees: parse("trees", &st.trees)?,
            rivers: st.rivers,
            seed: st.seed,
            heightmap: None,
        };
        let m = opencity_mapgen::generate(&params).map_err(|e| AssetError::Parse { path: "scenario".into(), line: 0, message: e.to_string() })?;
        City::new(Terrain::from_corner_heights(m.width, m.length, &m.corner_heights), Some(&m.tree_density), footprints, st.seed)
    };
    if let Some(id) = &st.rules {
        city.use_rules(crate::rules::load(data_dir, id)?);
        // Imported streets are asphalt: before asphalt, they are cobbled.
        if city.rules().is_some_and(|r| !r.allows(opencity_core::Tool::Road.id())) {
            let roads: Vec<(u32, u32)> = city
                .layer
                .structures()
                .filter(|(_, s)| s.code == opencity_core::StructureCode::Road && s.road == opencity_core::RoadKind::Street)
                .map(|(p, _)| p)
                .collect();
            for (w, l) in roads {
                if let Some(s) = city.layer.structure_mut(w, l) {
                    s.road = opencity_core::RoadKind::PavedStreet;
                }
            }
        }
    }
    if let Some(f) = st.fund {
        city.fund = f;
    }
    if st.demo {
        opencity_core::scenario::demo_town(&mut city);
    }
    if st.research_all {
        city.research_all();
    } else if let Some(gb) = city.glassbox.as_mut() {
        // The demo researches everything; a scenario starts from scratch.
        gb.researched.clear();
    }
    city.meta.name.get_or_insert_with(|| s.name.clone());
    city.set_goals(s);
    Ok(city)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_scenarios_build() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let all = load_all(&dir);
        assert!(all.len() >= 3);
        for s in &all {
            let c = build(&dir, s, Footprints::builtin()).unwrap_or_else(|e| panic!("{}: {e}", s.id));
            let g = c.goals.as_ref().unwrap();
            assert!(!g.goals.is_empty(), "{}", s.id);
            for goal in &g.goals {
                // Every metric is one the city knows how to measure.
                let (kind, _) = goal.metric.split_once(':').unwrap_or((&goal.metric, ""));
                assert!(["population", "fund", "buildings", "research", "unit", "structure", "produced", "unmet"].contains(&kind), "{}: {}", s.id, goal.metric);
            }
        }
    }
}
