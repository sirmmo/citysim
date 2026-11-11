//! Wavefront `.obj` models, as the CC0 Kenney kits ship them: positions,
//! texture coordinates into one shared colour map (the city, car, train and
//! watercraft kits) or a diffuse colour per material from the `.mtl` next
//! to the model (the nature kit), and polygon faces. Normals are ignored;
//! the renderer shades faces from their direction.

use std::collections::HashMap;
use std::path::Path;

use crate::{read_text, AssetError};

/// A triangle mesh: positions, texture coordinates (v pointing down, as
/// image rows run), a colour per vertex (white unless the material gives
/// one) and triangles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjMesh {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// Diffuse colours by material name from `.mtl` text; materials with a
/// texture map keep white (the map carries the colour).
pub fn parse_materials(text: &str) -> HashMap<String, [f32; 3]> {
    let mut out = HashMap::new();
    let mut current = None;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("newmtl") => current = it.next().map(str::to_string),
            Some("Kd") => {
                let v: Vec<f32> = it.filter_map(|s| s.parse().ok()).collect();
                if let (Some(name), [r, g, b, ..]) = (&current, v.as_slice()) {
                    out.insert(name.clone(), [*r, *g, *b]);
                }
            }
            Some("map_Kd") => {
                if let Some(name) = &current {
                    out.insert(name.clone(), [1.0, 1.0, 1.0]);
                }
            }
            _ => {}
        }
    }
    out
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

/// Parses `.obj` text. Each face corner becomes its own vertex (position,
/// texture coordinate and material colour together); polygons are fanned
/// into triangles. `materials` gives the colour of each `usemtl` name.
pub fn parse(text: &str) -> Result<ObjMesh, String> {
    parse_with(text, &HashMap::new())
}

pub fn parse_with(text: &str, materials: &HashMap<String, [f32; 3]>) -> Result<ObjMesh, String> {
    let mut v: Vec<[f32; 3]> = Vec::new();
    let mut vt: Vec<[f32; 2]> = Vec::new();
    let mut mesh = ObjMesh::default();
    let mut color = [1.0f32; 3];
    let mut corners: HashMap<(usize, Option<usize>, [u32; 3]), u32> = HashMap::new();
    for (n, line) in text.lines().enumerate() {
        let mut it = line.split_whitespace();
        let err = |m: &str| format!("line {}: {m}", n + 1);
        let num = |s: Option<&str>| s.and_then(|s| s.parse::<f32>().ok());
        match it.next() {
            Some("usemtl") => color = it.next().and_then(|m| materials.get(m).copied()).unwrap_or([1.0; 3]),
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
                    let id = *corners.entry((p, t, color.map(f32::to_bits))).or_insert_with(|| {
                        mesh.positions.push(v[p]);
                        mesh.uvs.push(t.map_or([0.0, 0.0], |t| vt[t]));
                        mesh.colors.push(color);
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

/// A building model from an `.obj`: scaled evenly to fit a footprint of
/// `w` × `l` tiles (less a margin), centred on it, standing on the ground,
/// in the frame of OpenCity's models (x along w, z along l, y up). Faces
/// get flat normals and the kit's colour map as texture.
pub fn to_building(obj: &ObjMesh, texture: Option<std::path::PathBuf>, (w, l): (u32, u32)) -> crate::ac3d::Model {
    let (lo, hi) = obj.bounds();
    let (w, l) = (w.max(1) as f32, l.max(1) as f32);
    let size = [(hi[0] - lo[0]).max(1e-3), (hi[2] - lo[2]).max(1e-3)];
    let margin = 0.06;
    let s = ((w - 2.0 * margin) / size[0]).min((l - 2.0 * margin) / size[1]);
    let (cx, cz) = ((lo[0] + hi[0]) / 2.0, (lo[2] + hi[2]) / 2.0);
    let place = |p: [f32; 3]| [w / 2.0 + (p[0] - cx) * s, (p[1] - lo[1]) * s, l / 2.0 + (p[2] - cz) * s];
    let mut mesh = crate::ac3d::Mesh { texture, ..Default::default() };
    for t in &obj.triangles {
        let p = t.map(|i| place(obj.positions[i as usize]));
        let (a, b) = ([p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]], [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]]);
        let n = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
        let n = [n[0] / len, n[1] / len, n[2] / len];
        for (j, pos) in p.into_iter().enumerate() {
            mesh.indices.push(mesh.positions.len() as u32);
            mesh.positions.push(pos);
            mesh.normals.push(n);
            mesh.uvs.push(obj.uvs[t[j] as usize]);
            let [r, g, b] = obj.colors.get(t[j] as usize).copied().unwrap_or([1.0; 3]);
            mesh.colors.push([r, g, b, 1.0]);
        }
    }
    crate::ac3d::Model { meshes: vec![mesh] }
}

/// Reads and parses an `.obj` file, with the colours of the `.mtl` files
/// it names (missing ones leave the model white).
pub fn load(path: &Path) -> Result<ObjMesh, AssetError> {
    let text = read_text(path)?;
    let mut materials = HashMap::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("mtllib ") {
            let mtl = path.parent().unwrap_or(Path::new(".")).join(name.trim());
            if let Ok(t) = read_text(&mtl) {
                materials.extend(parse_materials(&t));
            }
        }
    }
    parse_with(&text, &materials).map_err(|message| AssetError::Parse { path: path.to_owned(), line: 0, message })
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
    fn materials_colour_their_faces() {
        let mtl = parse_materials("newmtl bark\nKd 0.5 0.25 0\nnewmtl painted\nmap_Kd x.png\nKd 0 0 1\n");
        assert_eq!(mtl["bark"], [0.5, 0.25, 0.0]);
        assert_eq!(mtl["painted"], [0.0, 0.0, 1.0]);
        let m = parse_with("v 0 0 0\nv 1 0 0\nv 1 0 1\nusemtl bark\nf 1 2 3\nusemtl none\nf 1 3 2\n", &mtl).unwrap();
        // The same corner in another material is another vertex.
        assert_eq!(m.positions.len(), 6);
        assert_eq!(m.colors[0], [0.5, 0.25, 0.0]);
        assert_eq!(m.colors[3], [1.0, 1.0, 1.0]);
    }

    #[test]
    fn buildings_fit_their_footprint() {
        let obj = parse("v -1 0 -0.5\nv 1 0 -0.5\nv 1 2 0.5\nf 1 2 3\n").unwrap();
        let m = to_building(&obj, None, (2, 1));
        let (lo, hi) = m.bounds().unwrap();
        assert!(lo[0] >= 0.0 && hi[0] <= 2.0 && lo[2] >= 0.0 && hi[2] <= 1.0 && lo[1] == 0.0, "{lo:?} {hi:?}");
        // The tighter side (1 tile deep) sets the even scale: 0.88 after margins.
        assert!((hi[2] - lo[2] - 0.88).abs() < 1e-3 && (hi[0] - lo[0] - 1.76).abs() < 1e-3, "{lo:?} {hi:?}");
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
                    assert_eq!(m.colors.len(), m.positions.len(), "{}", p.display());
                    n += 1;
                }
            }
        }
        assert!(n >= 20);
        // The nature kit colours its faces from materials, not a colour map.
        let oak = load(&dir.join("nature/tree_oak.obj")).unwrap();
        assert!(oak.colors.iter().any(|c| c[1] > 0.7 && c[0] < 0.3), "leaves should be green");
    }
}
