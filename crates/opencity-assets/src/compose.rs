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
//! height = 1.6                   # then stretched to this tall (optional)
//! at = [0.1, 0.0, 0.05]          # then moved by this much (x, y, z tiles)
//! tints = [[1, 1, 1], [1, 0.9, 0.8]]   # colour multipliers to pick from
//!
//! [[slot]]                       # something in the front yard, or nothing
//! parts = ["", "../../graphism/tree/fir1/fir1.ac"]
//! size = [0.25, 0.25]
//! at = [0.05, 0, 0.7]
//! rotations = [0, 90, 180, 270]  # turned about its centre (degrees, one is picked)
//! ```
//!
//! Part paths are relative to the `.compose` file; `""` is no part. Without
//! `size`, an `.obj` part is fitted to the building's footprint and an `.ac`
//! part kept as it is (tile units), like a whole model. A part is turned
//! about the centre of its box (or of the footprint) before it is moved.

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
    /// Stretched to this height in tiles after the fit (optional).
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default)]
    pub at: [f32; 3],
    #[serde(default)]
    pub tints: Vec<[f32; 3]>,
    /// Turns about the vertical axis to pick from, in degrees.
    #[serde(default)]
    pub rotations: Vec<f32>,
}

impl Slot {
    fn choices(&self) -> usize {
        self.parts.len().max(1) * self.tints.len().max(1) * self.rotations.len().max(1)
    }
}

/// What a slot picked: indices of the part, tint and rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick {
    pub part: usize,
    pub tint: usize,
    pub rotation: usize,
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

    /// What each slot picks for variant `k`: combinations spread evenly
    /// over all of them, so variants differ in every slot.
    pub fn picks(&self, k: usize) -> Vec<Pick> {
        let (total, n) = (self.combinations(), self.variant_count());
        let mut c = (k % n) * total / n;
        self.slot
            .iter()
            .map(|s| {
                let m = s.choices();
                let pick = c % m;
                c /= m;
                let (np, nt) = (s.parts.len().max(1), s.tints.len().max(1));
                Pick { part: pick % np, tint: (pick / np) % nt, rotation: pick / (np * nt) }
            })
            .collect()
    }
}

/// Variant `k` of a composed model for a building of `footprint` tiles.
pub fn load_variant(path: &Path, footprint: (u32, u32), k: usize) -> Result<Model, AssetError> {
    let comp = Composition::load(path)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut out = Model::default();
    for (slot, pick) in comp.slot.iter().zip(comp.picks(k)) {
        let Some(rel) = slot.parts.get(pick.part).filter(|p| !p.is_empty()) else { continue };
        let mut m = crate::load_building_model(&dir.join(rel), footprint)?;
        // The centre the part turns about: its box, or the footprint.
        let mut centre = (footprint.0.max(1) as f32 / 2.0, footprint.1.max(1) as f32 / 2.0);
        if let (Some([bw, bl]), Some((lo, hi))) = (slot.size, m.bounds()) {
            // Fitted evenly into the box, standing on its floor.
            let s = (bw / (hi[0] - lo[0]).max(1e-3)).min(bl / (hi[2] - lo[2]).max(1e-3));
            let (cx, cz) = ((lo[0] + hi[0]) / 2.0, (lo[2] + hi[2]) / 2.0);
            for mesh in &mut m.meshes {
                for p in &mut mesh.positions {
                    *p = [bw / 2.0 + (p[0] - cx) * s, (p[1] - lo[1]) * s, bl / 2.0 + (p[2] - cz) * s];
                }
            }
            centre = (bw / 2.0, bl / 2.0);
        }
        if let (Some(h), Some((lo, hi))) = (slot.height, m.bounds()) {
            // Stretched upright: a square part can stand as a tower or a chimney.
            let s = h / (hi[1] - lo[1]).max(1e-3);
            for mesh in &mut m.meshes {
                for p in &mut mesh.positions {
                    p[1] = lo[1] + (p[1] - lo[1]) * s;
                }
            }
        }
        let angle = slot.rotations.get(pick.rotation).copied().unwrap_or(0.0).to_radians();
        if angle != 0.0 {
            let (sin, cos) = angle.sin_cos();
            for mesh in &mut m.meshes {
                for p in &mut mesh.positions {
                    let (x, z) = (p[0] - centre.0, p[2] - centre.1);
                    *p = [centre.0 + x * cos + z * sin, p[1], centre.1 - x * sin + z * cos];
                }
                for n in &mut mesh.normals {
                    *n = [n[0] * cos + n[2] * sin, n[1], -n[0] * sin + n[2] * cos];
                }
            }
        }
        let t = slot.tints.get(pick.tint).copied().unwrap_or([1.0; 3]);
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
            rotations = [0, 90]
            height = 1.5
            "#,
        )
        .unwrap();
        assert_eq!(c.combinations(), 24);
        assert_eq!(c.variant_count(), 6);
        let all: std::collections::HashSet<Vec<(usize, usize, usize)>> =
            (0..6).map(|k| c.picks(k).iter().map(|p| (p.part, p.tint, p.rotation)).collect()).collect();
        assert_eq!(all.len(), 6, "every variant is another combination");
        assert!((0..24).map(|k| c.picks(k)[1].rotation).any(|r| r == 1), "rotations are picked too");
        assert_eq!(c.slot[1].height, Some(1.5));
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
