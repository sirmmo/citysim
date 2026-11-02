//! Reads the triangles and prop points of a COLLADA (`.dae`) mesh.

use std::collections::HashMap;

use roxmltree::{Document, Node};

pub type Mat = [[f64; 4]; 4];
/// Triangle positions and UVs of one geometry.
type Geometry = (Vec<[f64; 3]>, Vec<[f64; 2]>);
pub const IDENTITY: Mat = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

pub fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut r = [[0.0; 4]; 4];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

pub fn apply(m: &Mat, p: [f64; 3]) -> [f64; 3] {
    let v = [p[0], p[1], p[2], 1.0];
    let r = |i: usize| (0..4).map(|k| m[i][k] * v[k]).sum::<f64>();
    [r(0), r(1), r(2)]
}

/// Triangles (3 positions and UVs each) in the mesh's own space, and prop points.
#[derive(Debug, Default)]
pub struct Mesh {
    pub z_up: bool,
    pub positions: Vec<[f64; 3]>,
    pub uvs: Vec<[f64; 2]>,
    /// Prop point name (lower case, without `prop-`/`prop_`) -> transform.
    pub props: HashMap<String, Mat>,
}

fn el<'a, 'b>(n: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn els<'a, 'b>(n: Node<'a, 'b>, name: &'b str) -> impl Iterator<Item = Node<'a, 'b>> + 'b
where
    'a: 'b,
{
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn floats(n: Node) -> Vec<f64> {
    n.text().unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect()
}

fn ints(n: Node) -> Vec<usize> {
    n.text().unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect()
}

struct Input {
    semantic: String,
    source: String,
    offset: usize,
    set: usize,
}

pub fn parse(text: &str) -> Option<Mesh> {
    let doc = Document::parse(text).ok()?;
    let root = doc.root_element();
    let z_up = root.descendants().find(|n| n.tag_name().name() == "up_axis").and_then(|n| n.text()).is_none_or(|t| t.trim() == "Z_UP");
    // Geometries: id -> (positions, uvs)
    let mut geometries: HashMap<String, Geometry> = HashMap::new();
    for geom in root.descendants().filter(|n| n.tag_name().name() == "geometry") {
        let Some(mesh) = el(geom, "mesh") else { continue };
        let mut sources: HashMap<String, (Vec<f64>, usize)> = HashMap::new();
        for s in els(mesh, "source") {
            let data = s.descendants().find(|n| n.tag_name().name() == "float_array").map(floats).unwrap_or_default();
            let stride =
                s.descendants().find(|n| n.tag_name().name() == "accessor").and_then(|a| a.attribute("stride")).and_then(|v| v.parse().ok()).unwrap_or(3);
            sources.insert(s.attribute("id").unwrap_or("").to_string(), (data, stride));
        }
        let vertices: HashMap<String, String> = els(mesh, "vertices")
            .filter_map(|v| {
                let pos = els(v, "input").find(|i| i.attribute("semantic") == Some("POSITION"))?;
                Some((v.attribute("id")?.to_string(), pos.attribute("source")?.trim_start_matches('#').to_string()))
            })
            .collect();
        let (mut positions, mut uvs) = (Vec::new(), Vec::new());
        for prim in mesh.children().filter(|c| c.is_element() && matches!(c.tag_name().name(), "triangles" | "polylist" | "polygons")) {
            let inputs: Vec<Input> = els(prim, "input")
                .map(|i| Input {
                    semantic: i.attribute("semantic").unwrap_or("").to_string(),
                    source: i.attribute("source").unwrap_or("").trim_start_matches('#').to_string(),
                    offset: i.attribute("offset").and_then(|v| v.parse().ok()).unwrap_or(0),
                    set: i.attribute("set").and_then(|v| v.parse().ok()).unwrap_or(0),
                })
                .collect();
            let stride = inputs.iter().map(|i| i.offset).max().unwrap_or(0) + 1;
            let Some(vin) = inputs.iter().find(|i| i.semantic == "VERTEX") else { continue };
            let Some(tin) = inputs.iter().filter(|i| i.semantic == "TEXCOORD").min_by_key(|i| i.set) else { continue };
            let Some((pdata, pstride)) = vertices.get(&vin.source).and_then(|s| sources.get(s)) else { continue };
            let Some((tdata, tstride)) = sources.get(&tin.source) else { continue };
            // Polygons as index lists, then fan-triangulated.
            let mut polys: Vec<Vec<usize>> = Vec::new();
            match prim.tag_name().name() {
                "triangles" => {
                    let p = el(prim, "p").map(ints).unwrap_or_default();
                    polys.extend(p.chunks(3 * stride).map(|c| c.to_vec()));
                }
                "polylist" => {
                    let counts = el(prim, "vcount").map(ints).unwrap_or_default();
                    let p = el(prim, "p").map(ints).unwrap_or_default();
                    let mut at = 0;
                    for c in counts {
                        polys.push(p.get(at..at + c * stride).map(|s| s.to_vec()).unwrap_or_default());
                        at += c * stride;
                    }
                }
                _ => polys.extend(els(prim, "p").map(ints)),
            }
            for poly in polys {
                let n = poly.len() / stride;
                let vert = |k: usize| {
                    let pi = poly[k * stride + vin.offset];
                    let ti = poly[k * stride + tin.offset];
                    let p = [
                        pdata.get(pi * pstride).copied().unwrap_or(0.0),
                        pdata.get(pi * pstride + 1).copied().unwrap_or(0.0),
                        pdata.get(pi * pstride + 2).copied().unwrap_or(0.0),
                    ];
                    let t = [tdata.get(ti * tstride).copied().unwrap_or(0.0), tdata.get(ti * tstride + 1).copied().unwrap_or(0.0)];
                    (p, t)
                };
                for k in 1..n.saturating_sub(1) {
                    for idx in [0, k, k + 1] {
                        let (p, t) = vert(idx);
                        positions.push(p);
                        uvs.push(t);
                    }
                }
            }
        }
        geometries.insert(geom.attribute("id").unwrap_or("").to_string(), (positions, uvs));
    }
    let mut out = Mesh { z_up, ..Default::default() };
    let scene = root.descendants().find(|n| n.tag_name().name() == "visual_scene");
    match scene {
        Some(scene) => {
            for node in els(scene, "node") {
                walk(node, &IDENTITY, &geometries, &mut out);
            }
        }
        None => {
            for (p, t) in geometries.into_values() {
                out.positions.extend(p);
                out.uvs.extend(t);
            }
        }
    }
    Some(out)
}

fn local_transform(node: Node) -> Mat {
    let mut m = IDENTITY;
    for t in node.children().filter(|c| c.is_element()) {
        let v = floats(t);
        let step: Mat = match t.tag_name().name() {
            "matrix" if v.len() == 16 => [[v[0], v[1], v[2], v[3]], [v[4], v[5], v[6], v[7]], [v[8], v[9], v[10], v[11]], [v[12], v[13], v[14], v[15]]],
            "translate" if v.len() == 3 => [[1.0, 0.0, 0.0, v[0]], [0.0, 1.0, 0.0, v[1]], [0.0, 0.0, 1.0, v[2]], [0.0, 0.0, 0.0, 1.0]],
            "scale" if v.len() == 3 => [[v[0], 0.0, 0.0, 0.0], [0.0, v[1], 0.0, 0.0], [0.0, 0.0, v[2], 0.0], [0.0, 0.0, 0.0, 1.0]],
            "rotate" if v.len() == 4 => {
                let (x, y, z) = (v[0], v[1], v[2]);
                let len = (x * x + y * y + z * z).sqrt().max(1e-12);
                let (x, y, z) = (x / len, y / len, z / len);
                let (s, c) = v[3].to_radians().sin_cos();
                let t1 = 1.0 - c;
                [
                    [t1 * x * x + c, t1 * x * y - s * z, t1 * x * z + s * y, 0.0],
                    [t1 * x * y + s * z, t1 * y * y + c, t1 * y * z - s * x, 0.0],
                    [t1 * x * z - s * y, t1 * y * z + s * x, t1 * z * z + c, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ]
            }
            _ => continue,
        };
        m = mul(&m, &step);
    }
    m
}

fn walk(node: Node, parent: &Mat, geometries: &HashMap<String, Geometry>, out: &mut Mesh) {
    let world = mul(parent, &local_transform(node));
    let name = node.attribute("id").or(node.attribute("name")).unwrap_or("").to_lowercase();
    if let Some(point) = name.strip_prefix("prop-").or(name.strip_prefix("prop_")) {
        out.props.insert(point.to_string(), world);
    }
    for inst in els(node, "instance_geometry") {
        let url = inst.attribute("url").unwrap_or("").trim_start_matches('#');
        if let Some((p, t)) = geometries.get(url) {
            out.positions.extend(p.iter().map(|v| apply(&world, *v)));
            out.uvs.extend(t.iter().copied());
        }
    }
    for child in els(node, "node") {
        walk(child, &world, geometries, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_triangles_props_and_transforms() {
        let dae = r##"<?xml version="1.0"?><COLLADA xmlns="http://www.collada.org/2005/11/COLLADASchema" version="1.4.1">
<asset><up_axis>Z_UP</up_axis></asset>
<library_geometries><geometry id="g"><mesh>
<source id="pos"><float_array count="12">0 0 0 1 0 0 1 1 0 0 1 0</float_array><technique_common><accessor stride="3"/></technique_common></source>
<source id="uv"><float_array count="8">0 0 1 0 1 1 0 1</float_array><technique_common><accessor stride="2"/></technique_common></source>
<vertices id="v"><input semantic="POSITION" source="#pos"/></vertices>
<polylist count="1"><input semantic="VERTEX" source="#v" offset="0"/><input semantic="TEXCOORD" source="#uv" offset="1" set="0"/><vcount>4</vcount><p>0 0 1 1 2 2 3 3</p></polylist>
</mesh></geometry></library_geometries>
<library_visual_scenes><visual_scene id="s">
<node id="root"><translate>10 0 0</translate><instance_geometry url="#g"/>
<node id="prop-root"><translate>0 0 2</translate></node></node>
</visual_scene></library_visual_scenes></COLLADA>"##;
        let m = parse(dae).unwrap();
        assert!(m.z_up);
        assert_eq!(m.positions.len(), 6); // quad -> 2 triangles
        assert_eq!(m.positions[1], [11.0, 0.0, 0.0]);
        assert_eq!(m.uvs[2], [1.0, 1.0]);
        let p = m.props["root"];
        assert_eq!(apply(&p, [0.0, 0.0, 0.0]), [10.0, 0.0, 2.0]);
    }
}
