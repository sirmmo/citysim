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
//!
//! [[slot]]                       # a Quaternius town house in one of two colourways
//! parts = ["../quaternius/2Story.obj"]
//! paints = [{ Main = [0.9, 0.8, 0.6] }, { Main = [0.6, 0.7, 0.9], Bricks = [0.5, 0.4, 0.4] }]
//! ```
//!
//! Part paths are relative to the `.compose` file; `""` is no part. Without
//! `size`, an `.obj` part is fitted to the building's footprint and an `.ac`
//! part kept as it is (tile units), like a whole model. A part is turned
//! about the centre of its box (or of the footprint) before it is moved.
//! `paints` are colourways for `.obj` parts coloured by material: each
//! gives new colours to some materials by name (parts without those
//! materials, such as the Kenney kits' colour-mapped ones, are unchanged),
//! and one is picked per building like a tint.

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
    /// Colourways to pick from: new colours for materials, by name.
    #[serde(default)]
    pub paints: Vec<std::collections::HashMap<String, [f32; 3]>>,
}

impl Slot {
    fn choices(&self) -> usize {
        self.parts.len().max(1) * self.tints.len().max(1) * self.rotations.len().max(1) * self.paints.len().max(1)
    }
}

/// What a slot picked: indices of the part, tint and rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick {
    pub part: usize,
    pub tint: usize,
    pub rotation: usize,
    pub paint: usize,
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

    /// What each slot picks for variant `k`. Variant `k` is combination
    /// `k * step` (mod all of them), with `step` near the golden section of
    /// the count and sharing no factor with it: variants are all different,
    /// and each slot's first choice (its part) runs through every part
    /// before any repeats, whatever the other slots hold.
    pub fn picks(&self, k: usize) -> Vec<Pick> {
        let n = self.variant_count() as u64;
        let total = self.slot.iter().map(|s| s.choices() as u64).fold(1u64, |a, b| a.saturating_mul(b));
        let step = coprime_step(total);
        let mut c = ((k as u64 % n) as u128 * step as u128 % total as u128) as u64;
        self.slot
            .iter()
            .map(|s| {
                let m = s.choices() as u64;
                let pick = (c % m) as usize;
                c /= m;
                let (np, nt, nr) = (s.parts.len().max(1), s.tints.len().max(1), s.rotations.len().max(1));
                Pick { part: pick % np, tint: (pick / np) % nt, rotation: (pick / (np * nt)) % nr, paint: pick / (np * nt * nr) }
            })
            .collect()
    }
}

/// The smallest number from 0.618 of `total` on that shares no factor with it.
fn coprime_step(total: u64) -> u64 {
    fn gcd(a: u64, b: u64) -> u64 {
        if b == 0 {
            a
        } else {
            gcd(b, a % b)
        }
    }
    if total <= 1 {
        return 1;
    }
    let mut s = ((total as f64 * 0.618_034) as u64).max(1);
    while gcd(s, total) != 1 {
        s += 1;
    }
    s
}

/// Variant `k` of a composed model for a building of `footprint` tiles.
pub fn load_variant(path: &Path, footprint: (u32, u32), k: usize) -> Result<Model, AssetError> {
    let comp = Composition::load(path)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut out = Model::default();
    for (slot, pick) in comp.slot.iter().zip(comp.picks(k)) {
        let Some(rel) = slot.parts.get(pick.part).filter(|p| !p.is_empty()) else { continue };
        let no_paint = std::collections::HashMap::new();
        let paint = slot.paints.get(pick.paint).unwrap_or(&no_paint);
        let mut m = crate::load_painted_model(&dir.join(rel), footprint, paint)?;
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
    fn paints_give_materials_new_colours() {
        let dir = std::env::temp_dir().join(format!("oc-paints-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("q.mtl"), "newmtl Main\nKd 0.5 0.5 0.5\nnewmtl Trim\nKd 1 1 1\n").unwrap();
        let tri = "v 0 0 0\nv 1 0 0\nv 1 1 1\n";
        std::fs::write(dir.join("q.obj"), format!("mtllib q.mtl\n{tri}usemtl Main\nf 1 2 3\nusemtl Trim\nf 1 3 2\n")).unwrap();
        let comp = "[[slot]]\nparts = [\"q.obj\"]\npaints = [{ Main = [1, 0, 0] }, { Main = [0, 0, 1] }]\n";
        std::fs::write(dir.join("q.compose"), comp).unwrap();
        let path = dir.join("q.compose");
        assert_eq!(variants(&path), 2);
        let colours = |k| {
            let m = load_variant(&path, (1, 1), k).unwrap();
            m.meshes.iter().flat_map(|m| m.colors.clone()).collect::<Vec<_>>()
        };
        let (a, b) = (colours(0), colours(1));
        assert!(a.iter().any(|c| c[..3] == [1.0, 0.0, 0.0]), "{a:?}");
        assert!(b.iter().any(|c| c[..3] == [0.0, 0.0, 1.0]), "{b:?}");
        // The trim keeps its own colour, and nothing is left grey.
        assert!(a.iter().chain(&b).any(|c| c[..3] == [1.0, 1.0, 1.0]));
        assert!(!a.iter().chain(&b).any(|c| c[0] == 0.5));
        std::fs::remove_dir_all(&dir).ok();
    }

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
        // The first slot's parts all come up within as many variants.
        let parts: std::collections::HashSet<usize> = (0..3).map(|k| c.picks(k)[0].part).collect();
        assert_eq!(parts.len(), 3);
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
