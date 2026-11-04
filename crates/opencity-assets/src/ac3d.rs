//! Parser for AC3D (`.ac`) models (legacy `src/model/ac3d*`).
//!
//! Produces flat triangle lists grouped by texture, with every object
//! transform already applied, ready to upload to the GPU.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{read_text, AssetError};

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub rgb: [f32; 3],
    pub transparency: f32,
}

/// Triangles sharing one texture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub texture: Option<PathBuf>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub meshes: Vec<Mesh>,
}

impl Model {
    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(|m| m.indices.len() / 3).sum()
    }

    /// Axis-aligned bounds `(min, max)`.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut it = self.meshes.iter().flat_map(|m| m.positions.iter());
        let first = *it.next()?;
        Some(it.fold((first, first), |(mut lo, mut hi), p| {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
            (lo, hi)
        }))
    }
}

#[derive(Clone, Copy)]
struct Transform {
    rot: [[f32; 3]; 3],
    loc: [f32; 3],
}

impl Transform {
    const IDENTITY: Self = Self { rot: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], loc: [0.0; 3] };

    fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        let r = &self.rot;
        [
            r[0][0] * v[0] + r[1][0] * v[1] + r[2][0] * v[2] + self.loc[0],
            r[0][1] * v[0] + r[1][1] * v[1] + r[2][1] * v[2] + self.loc[1],
            r[0][2] * v[0] + r[1][2] * v[1] + r[2][2] * v[2] + self.loc[2],
        ]
    }

    /// `self` applied after `child` (child is local to self).
    fn then(&self, child: &Transform) -> Transform {
        let mut rot = [[0.0; 3]; 3];
        for (c, col) in child.rot.iter().enumerate() {
            let v = self.apply_dir(*col);
            rot[c] = v;
        }
        Transform { rot, loc: self.apply(child.loc) }
    }

    fn apply_dir(&self, v: [f32; 3]) -> [f32; 3] {
        let r = &self.rot;
        [r[0][0] * v[0] + r[1][0] * v[1] + r[2][0] * v[2], r[0][1] * v[0] + r[1][1] * v[1] + r[2][1] * v[2], r[0][2] * v[0] + r[1][2] * v[1] + r[2][2] * v[2]]
    }
}

struct Parser<'a> {
    path: &'a Path,
    lines: std::iter::Peekable<std::iter::Enumerate<std::str::Lines<'a>>>,
    line_no: usize,
    materials: Vec<Material>,
    meshes: BTreeMap<Option<PathBuf>, Mesh>,
}

impl<'a> Parser<'a> {
    fn err(&self, message: impl Into<String>) -> AssetError {
        AssetError::Parse { path: self.path.to_owned(), line: self.line_no, message: message.into() }
    }

    fn next_line(&mut self) -> Option<&'a str> {
        loop {
            let (n, line) = self.lines.next()?;
            self.line_no = n + 1;
            if !line.trim().is_empty() {
                return Some(line.trim());
            }
        }
    }

    fn expect_line(&mut self) -> Result<&'a str, AssetError> {
        self.next_line().ok_or_else(|| self.err("unexpected end of file"))
    }

    fn floats<const N: usize>(&self, parts: &[&str]) -> Result<[f32; N], AssetError> {
        if parts.len() < N {
            return Err(self.err(format!("expected {N} numbers")));
        }
        let mut out = [0.0; N];
        for (o, p) in out.iter_mut().zip(parts) {
            *o = p.parse().map_err(|_| self.err(format!("bad number {p:?}")))?;
        }
        Ok(out)
    }

    fn parse_material(&mut self, line: &str) -> Result<(), AssetError> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let find = |key: &str| parts.iter().position(|p| *p == key);
        let rgb = match find("rgb") {
            Some(i) => self.floats::<3>(&parts[i + 1..])?,
            None => [1.0; 3],
        };
        let transparency = match find("trans") {
            Some(i) => self.floats::<1>(&parts[i + 1..])?[0],
            None => 0.0,
        };
        self.materials.push(Material { rgb, transparency });
        Ok(())
    }

    fn parse_object(&mut self, parent: Transform, dir: &Path) -> Result<(), AssetError> {
        let mut local = Transform::IDENTITY;
        let mut texture: Option<PathBuf> = None;
        let mut texrep = [1.0f32, 1.0];
        let mut texoff = [0.0f32, 0.0];
        let mut vertices: Vec<[f32; 3]> = Vec::new();
        loop {
            let line = self.expect_line()?;
            let mut parts = line.split_whitespace();
            let key = parts.next().unwrap_or("");
            let rest: Vec<&str> = parts.collect();
            match key {
                "name" | "crease" | "url" | "subdivide" | "texoff_" => {}
                "data" => {
                    let len: usize = rest.first().and_then(|n| n.parse().ok()).unwrap_or(0);
                    let mut read = 0;
                    while read < len {
                        read += self.expect_line()?.len() + 1;
                    }
                }
                "loc" => local.loc = self.floats::<3>(&rest)?,
                "rot" => {
                    let r = self.floats::<9>(&rest)?;
                    local.rot = [[r[0], r[1], r[2]], [r[3], r[4], r[5]], [r[6], r[7], r[8]]];
                }
                "texture" => {
                    let name = line["texture".len()..].trim().trim_matches('"');
                    texture = Some(dir.join(name));
                }
                "texrep" => texrep = self.floats::<2>(&rest)?,
                "texoff" => texoff = self.floats::<2>(&rest)?,
                "numvert" => {
                    let world = parent.then(&local);
                    let n: usize = rest.first().and_then(|n| n.parse().ok()).ok_or_else(|| self.err("bad numvert"))?;
                    vertices.reserve(n);
                    for _ in 0..n {
                        let l = self.expect_line()?;
                        let v = self.floats::<3>(&l.split_whitespace().collect::<Vec<_>>())?;
                        vertices.push(world.apply(v));
                    }
                }
                "numsurf" => {
                    let n: usize = rest.first().and_then(|n| n.parse().ok()).ok_or_else(|| self.err("bad numsurf"))?;
                    for _ in 0..n {
                        self.parse_surface(&vertices, &texture, texrep, texoff)?;
                    }
                }
                "kids" => {
                    let n: usize = rest.first().and_then(|n| n.parse().ok()).ok_or_else(|| self.err("bad kids"))?;
                    let world = parent.then(&local);
                    for _ in 0..n {
                        let l = self.expect_line()?;
                        if !l.starts_with("OBJECT") {
                            return Err(self.err("expected OBJECT"));
                        }
                        self.parse_object(world, dir)?;
                    }
                    return Ok(());
                }
                other => return Err(self.err(format!("unknown token {other:?}"))),
            }
        }
    }

    fn parse_surface(&mut self, vertices: &[[f32; 3]], texture: &Option<PathBuf>, texrep: [f32; 2], texoff: [f32; 2]) -> Result<(), AssetError> {
        let line = self.expect_line()?;
        let flags = line
            .strip_prefix("SURF")
            .map(str::trim)
            .and_then(|f| u32::from_str_radix(f.trim_start_matches("0x"), 16).ok())
            .ok_or_else(|| self.err("expected SURF"))?;
        let mut mat = 0usize;
        let mut line = self.expect_line()?;
        if let Some(m) = line.strip_prefix("mat") {
            mat = m.trim().parse().map_err(|_| self.err("bad mat"))?;
            line = self.expect_line()?;
        }
        let n: usize = line.strip_prefix("refs").and_then(|r| r.trim().parse().ok()).ok_or_else(|| self.err("expected refs"))?;
        let mut refs = Vec::with_capacity(n);
        for _ in 0..n {
            let l = self.expect_line()?;
            let p: Vec<&str> = l.split_whitespace().collect();
            let idx: usize = p.first().and_then(|i| i.parse().ok()).ok_or_else(|| self.err("bad ref"))?;
            let uv = if p.len() >= 3 { self.floats::<2>(&p[1..])? } else { [0.0, 0.0] };
            let v = *vertices.get(idx).ok_or_else(|| self.err("vertex index out of range"))?;
            refs.push((v, [uv[0] * texrep[0] + texoff[0], uv[1] * texrep[1] + texoff[1]]));
        }
        // Only filled polygons (type 0); lines are skipped.
        if flags & 0x0F != 0 || refs.len() < 3 {
            return Ok(());
        }
        let material = self.materials.get(mat).cloned().unwrap_or(Material { rgb: [1.0; 3], transparency: 0.0 });
        let color = [material.rgb[0], material.rgb[1], material.rgb[2], 1.0 - material.transparency];
        let normal = polygon_normal(refs.iter().map(|r| r.0));
        let mesh = self.meshes.entry(texture.clone()).or_insert_with(|| Mesh { texture: texture.clone(), ..Default::default() });
        let two_sided = flags & 0x20 != 0;
        for (n, flip) in [(normal, false), ([-normal[0], -normal[1], -normal[2]], true)] {
            if flip && !two_sided {
                break;
            }
            let base = mesh.positions.len() as u32;
            for &(p, uv) in &refs {
                mesh.positions.push(p);
                mesh.normals.push(n);
                // AC3D uses OpenGL's bottom-left texture origin.
                mesh.uvs.push([uv[0], 1.0 - uv[1]]);
                mesh.colors.push(color);
            }
            for i in 1..refs.len() as u32 - 1 {
                if flip {
                    mesh.indices.extend([base, base + i + 1, base + i]);
                } else {
                    mesh.indices.extend([base, base + i, base + i + 1]);
                }
            }
        }
        Ok(())
    }
}

/// Newell's method: robust normal for any planar polygon.
fn polygon_normal(points: impl Iterator<Item = [f32; 3]> + Clone) -> [f32; 3] {
    let pts: Vec<[f32; 3]> = points.collect();
    let mut n = [0.0f32; 3];
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 {
        [n[0] / len, n[1] / len, n[2] / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

pub fn parse(text: &str, path: &Path) -> Result<Model, AssetError> {
    let mut p = Parser { path, lines: text.lines().enumerate().peekable(), line_no: 0, materials: Vec::new(), meshes: BTreeMap::new() };
    let header = p.expect_line()?;
    if !header.starts_with("AC3D") {
        return Err(p.err("not an AC3D file"));
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    while let Some(line) = p.next_line() {
        if line.starts_with("MATERIAL") {
            p.parse_material(line)?;
        } else if line.starts_with("OBJECT") {
            p.parse_object(Transform::IDENTITY, dir)?;
        } else {
            return Err(p.err(format!("unexpected {line:?}")));
        }
    }
    Ok(Model { meshes: p.meshes.into_values().filter(|m| !m.indices.is_empty()).collect() })
}

pub fn load(path: &Path) -> Result<Model, AssetError> {
    parse(&read_text(path)?, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUAD: &str = r#"AC3Db
MATERIAL "m" rgb 1 0.5 0  amb 0.2 0.2 0.2  emis 0 0 0  spec 0 0 0  shi 10  trans 0
OBJECT world
kids 1
OBJECT poly
name "rect"
loc 0.5 0 0.5
texture "road.png"
numvert 4
-0.5 0 0.5
0.5 0 0.5
0.5 0 -0.5
-0.5 0 -0.5
numsurf 1
SURF 0x30
mat 0
refs 4
0 0 0
1 1 0
2 1 1
3 0 1
kids 0
"#;

    #[test]
    fn parses_two_sided_quad() {
        let m = parse(QUAD, Path::new("/x/road.ac")).unwrap();
        assert_eq!(m.meshes.len(), 1);
        assert_eq!(m.triangle_count(), 4);
        assert_eq!(m.meshes[0].texture.as_deref(), Some(Path::new("/x/road.png")));
        assert_eq!(m.bounds(), Some(([0.0, 0.0, 0.0], [1.0, 0.0, 1.0])));
        assert_eq!(m.meshes[0].colors[0], [1.0, 0.5, 0.0, 1.0]);
    }

    #[test]
    fn every_shipped_model_parses() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let catalog = crate::Catalog::load(&dir).unwrap();
        for (gc, entry) in &catalog.models {
            let fp = (entry.footprint.width, entry.footprint.length);
            let m = crate::load_building_model(&entry.model, fp).unwrap_or_else(|e| panic!("{gc:?}: {e}"));
            assert!(m.triangle_count() > 0, "{gc:?} has no triangles");
            for mesh in &m.meshes {
                if let Some(t) = &mesh.texture {
                    assert!(t.is_file(), "{gc:?}: missing texture {}", t.display());
                }
            }
        }
    }
}
