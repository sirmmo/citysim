//! Wavefront `.obj` models, as the CC0 Kenney kits ship them (car, train
//! and watercraft kits): positions, texture coordinates into one shared
//! colour map, and polygon faces. Normals and materials are ignored; the
//! renderer shades faces from their direction.

use std::path::Path;

use crate::{read_text, AssetError};

/// A triangle mesh: positions, texture coordinates (v pointing down, as
/// image rows run) and triangles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjMesh {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub triangles: Vec<[u32; 3]>,
}

impl ObjMesh {
    /// Smallest and largest corner of the bounding box.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in &self.positions {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        (lo, hi)
    }
}

/// Parses `.obj` text. Each face corner becomes its own vertex (position
/// and texture coordinate together); polygons are fanned into triangles.
pub fn parse(text: &str) -> Result<ObjMesh, String> {
    let mut v: Vec<[f32; 3]> = Vec::new();
    let mut vt: Vec<[f32; 2]> = Vec::new();
    let mut mesh = ObjMesh::default();
    let mut corners: std::collections::HashMap<(usize, Option<usize>), u32> = std::collections::HashMap::new();
    for (n, line) in text.lines().enumerate() {
        let mut it = line.split_whitespace();
        let err = |m: &str| format!("line {}: {m}", n + 1);
        let num = |s: Option<&str>| s.and_then(|s| s.parse::<f32>().ok());
        match it.next() {
            Some("v") => v.push([
                num(it.next()).ok_or_else(|| err("bad vertex"))?,
                num(it.next()).ok_or_else(|| err("bad vertex"))?,
                num(it.next()).ok_or_else(|| err("bad vertex"))?,
            ]),
            Some("vt") => vt.push([num(it.next()).ok_or_else(|| err("bad uv"))?, 1.0 - num(it.next()).unwrap_or(0.0)]),
            Some("f") => {
                let mut face = Vec::new();
                for c in it {
                    let mut parts = c.split('/');
                    let index = |s: Option<&str>, len: usize| -> Option<usize> {
                        let i: i64 = s.filter(|s| !s.is_empty())?.parse().ok()?;
                        let i = if i < 0 { len as i64 + i } else { i - 1 };
                        (0..len as i64).contains(&i).then_some(i as usize)
                    };
                    let p = index(parts.next(), v.len()).ok_or_else(|| err("bad face"))?;
                    let t = index(parts.next(), vt.len());
                    let id = *corners.entry((p, t)).or_insert_with(|| {
                        mesh.positions.push(v[p]);
                        mesh.uvs.push(t.map_or([0.0, 0.0], |t| vt[t]));
                        (mesh.positions.len() - 1) as u32
                    });
                    face.push(id);
                }
                for i in 1..face.len().saturating_sub(1) {
                    mesh.triangles.push([face[0], face[i], face[i + 1]]);
                }
            }
            _ => {}
        }
    }
    Ok(mesh)
}

/// Reads and parses an `.obj` file.
pub fn load(path: &Path) -> Result<ObjMesh, AssetError> {
    parse(&read_text(path)?).map_err(|message| AssetError::Parse { path: path.to_owned(), line: 0, message })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quads_become_two_triangles_sharing_corners() {
        let m = parse("v 0 0 0\nv 1 0 0\nv 1 0 1\nv 0 0 1\nvt 0 0\nvt 1 1\nf 1/1 2/1 3/2 4/2\n").unwrap();
        assert_eq!(m.triangles, vec![[0, 1, 2], [0, 2, 3]]);
        assert_eq!(m.positions.len(), 4);
        assert_eq!(m.uvs[2], [1.0, 0.0]);
        assert_eq!(m.bounds(), ([0.0, 0.0, 0.0], [1.0, 0.0, 1.0]));
    }

    #[test]
    fn shipped_kenney_models_parse() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/kenney");
        let mut n = 0;
        for kit in std::fs::read_dir(&dir).unwrap() {
            for f in std::fs::read_dir(kit.unwrap().path()).unwrap() {
                let p = f.unwrap().path();
                if p.extension().is_some_and(|e| e == "obj") {
                    let m = load(&p).unwrap();
                    assert!(!m.triangles.is_empty(), "{}", p.display());
                    n += 1;
                }
            }
        }
        assert!(n >= 20);
    }
}
