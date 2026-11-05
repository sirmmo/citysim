//! Models composed from parts, for variety: a `.compose` file (TOML) lists
//! slots, each with parts to pick from (and colour tints), and every
//! building drawn with it gets one combination, picked by its tile.
//!
//! ```toml
//! variants = 12                  # combinations used at most (default 12)
//!
//! [[slot]]                       # the house itself
//! parts = ["../kenney/city-suburban/building-type-h.obj", "../kenney/city-suburban/building-type-e.obj"]
//! size = [0.8, 0.7]              # fitted into this box, in tiles (optional)
//! at = [0.1, 0.0, 0.05]          # then moved by this much (x, y, z tiles)
//! tints = [[1, 1, 1], [1, 0.9, 0.8]]   # colour multipliers to pick from
//!
//! [[slot]]                       # something in the front yard, or nothing
//! parts = ["", "../../graphism/tree/fir1/fir1.ac"]
//! size = [0.25, 0.25]
//! at = [0.05, 0, 0.7]
//! ```
//!
//! Part paths are relative to the `.compose` file; `""` is no part. Without
//! `size`, an `.obj` part is fitted to the building's footprint and an `.ac`
//! part kept as it is (tile units), like a whole model.

use std::path::Path;

use serde::Deserialize;

use crate::ac3d::Model;
use crate::{read_text, AssetError};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Composition {
    /// Combinations used at most.
    #[serde(default = "default_variants")]
    pub variants: usize,
    #[serde(default)]
    pub slot: Vec<Slot>,
}

fn default_variants() -> usize {
    12
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub parts: Vec<String>,
    #[serde(default)]
    pub size: Option<[f32; 2]>,
    #[serde(default)]
    pub at: [f32; 3],
    #[serde(default)]
    pub tints: Vec<[f32; 3]>,
}

impl Slot {
    fn choices(&self) -> usize {
        self.parts.len().max(1) * self.tints.len().max(1)
    }
}

impl Composition {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    pub fn load(path: &Path) -> Result<Self, AssetError> {
        Self::parse(&read_text(path)?).map_err(|message| AssetError::Parse { path: path.to_owned(), line: 0, message })
    }

    /// All combinations of the slots' choices.
    pub fn combinations(&self) -> usize {
        self.slot.iter().map(Slot::choices).fold(1usize, |a, b| a.saturating_mul(b))
    }

    /// How many variants buildings are spread over.
    pub fn variant_count(&self) -> usize {
        self.combinations().min(self.variants.max(1))
    }

    /// The (part, tint) picked in each slot by variant `k`: combinations
    /// spread evenly over all of them, so variants differ in every slot.
    pub fn picks(&self, k: usize) -> Vec<(usize, usize)> {
        let (total, n) = (self.combinations(), self.variant_count());
        let mut c = (k % n) * total / n;
        self.slot
            .iter()
            .map(|s| {
                let m = s.choices();
                let pick = c % m;
                c /= m;
                (pick % s.parts.len().max(1), pick / s.parts.len().max(1))
            })
            .collect()
    }
}

/// Variant `k` of a composed model for a building of `footprint` tiles.
pub fn load_variant(path: &Path, footprint: (u32, u32), k: usize) -> Result<Model, AssetError> {
    let comp = Composition::load(path)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut out = Model::default();
    for (slot, (part, tint)) in comp.slot.iter().zip(comp.picks(k)) {
        let Some(rel) = slot.parts.get(part).filter(|p| !p.is_empty()) else { continue };
        let mut m = crate::load_building_model(&dir.join(rel), footprint)?;
        if let (Some([bw, bl]), Some((lo, hi))) = (slot.size, m.bounds()) {
            // Fitted evenly into the box, standing on its floor.
            let s = (bw / (hi[0] - lo[0]).max(1e-3)).min(bl / (hi[2] - lo[2]).max(1e-3));
            let (cx, cz) = ((lo[0] + hi[0]) / 2.0, (lo[2] + hi[2]) / 2.0);
            for mesh in &mut m.meshes {
                for p in &mut mesh.positions {
                    *p = [bw / 2.0 + (p[0] - cx) * s, (p[1] - lo[1]) * s, bl / 2.0 + (p[2] - cz) * s];
                }
            }
        }
        let t = slot.tints.get(tint).copied().unwrap_or([1.0; 3]);
        for mut mesh in m.meshes {
            for p in &mut mesh.positions {
                for (v, d) in p.iter_mut().zip(slot.at) {
                    *v += d;
                }
            }
            for c in &mut mesh.colors {
                for (v, f) in c.iter_mut().zip(t) {
                    *v *= f;
                }
            }
            out.meshes.push(mesh);
        }
    }
    Ok(out)
}

/// Whether a model path is a composition.
pub fn is_composed(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("compose"))
}

/// Variants of the model at `path`: those of a composition, else 1.
pub fn variants(path: &Path) -> usize {
    if is_composed(path) {
        Composition::load(path).map_or(1, |c| c.variant_count())
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_spread_over_the_combinations() {
        let c = Composition::parse(
            r#"
            variants = 6
            [[slot]]
            parts = ["a", "b", "c"]
            tints = [[1, 1, 1], [0.5, 0.5, 0.5]]
            [[slot]]
            parts = ["", "x"]
            "#,
        )
        .unwrap();
        assert_eq!(c.combinations(), 12);
        assert_eq!(c.variant_count(), 6);
        let all: std::collections::HashSet<Vec<(usize, usize)>> = (0..6).map(|k| c.picks(k)).collect();
        assert_eq!(all.len(), 6, "every variant is another combination");
        assert!(Composition::parse("[[slot]]\nparts = []\nwhat = 1").is_err(), "unknown keys are refused");
    }

    #[test]
    fn shipped_compositions_load() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/composed");
        let mut n = 0;
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if is_composed(&p) {
                let c = Composition::load(&p).unwrap();
                for k in 0..c.variant_count() {
                    let m = load_variant(&p, (1, 1), k).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                    assert!(m.triangle_count() > 0, "{} variant {k} is empty", p.display());
                }
                n += 1;
            }
        }
        assert!(n > 0);
    }
}
