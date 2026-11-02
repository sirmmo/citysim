//! Reading features from GeoJSON or Overpass/OSM JSON into a flat list.

use serde_json::Value;

use crate::{rules::Properties, GeoError};

pub type Point = [f64; 2];

#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    Point(Point),
    Line(Vec<Point>),
    /// Outer ring first, then holes.
    Polygon(Vec<Vec<Point>>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Feature {
    pub properties: Properties,
    pub geometry: Geometry,
}

/// Declared coordinate system, if the file says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclaredCrs {
    Wgs84,
    Planar,
}

pub struct Source {
    pub features: Vec<Feature>,
    pub crs: Option<DeclaredCrs>,
    /// Top-level `world` (or `ofm:world`) member naming a fantasy world.
    pub world: Option<String>,
}

/// Parses a GeoJSON document (FeatureCollection, Feature or bare geometry)
/// or an Overpass API / OSM JSON document (`{"elements": [...]}`, queried with `out geom`).
pub fn parse(text: &str) -> Result<Source, GeoError> {
    let v: Value = serde_json::from_str(text)?;
    if v.get("elements").is_some() {
        return Ok(Source { features: from_overpass(&v), crs: Some(DeclaredCrs::Wgs84), world: None });
    }
    let crs = v.pointer("/crs/properties/name").and_then(Value::as_str).map(|name| {
        let n = name.to_ascii_uppercase();
        if n.contains("CRS84") || n.ends_with(":4326") || n.ends_with("::4326") {
            DeclaredCrs::Wgs84
        } else {
            DeclaredCrs::Planar
        }
    });
    let mut features = Vec::new();
    collect_geojson(&v, &Properties::new(), &mut features)?;
    let world = ["world", "ofm:world"].iter().find_map(|k| v.get(*k)).and_then(Value::as_str).map(str::to_string);
    Ok(Source { features, crs, world })
}

fn properties(v: Option<&Value>) -> Properties {
    let mut out = Properties::new();
    let Some(Value::Object(map)) = v else { return out };
    for (k, val) in map {
        match val {
            // osmtogeojson nests the OSM tags.
            Value::Object(inner) if k == "tags" => {
                for (tk, tv) in inner {
                    out.insert(tk.clone(), scalar(tv));
                }
            }
            Value::Null => {}
            other => {
                out.insert(k.clone(), scalar(other));
            }
        }
    }
    out
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn point(v: &Value) -> Option<Point> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
}

fn line(v: &Value) -> Option<Vec<Point>> {
    v.as_array()?.iter().map(point).collect()
}

fn polygon(v: &Value) -> Option<Vec<Vec<Point>>> {
    v.as_array()?.iter().map(line).collect()
}

fn collect_geojson(v: &Value, props: &Properties, out: &mut Vec<Feature>) -> Result<(), GeoError> {
    let ty = v.get("type").and_then(Value::as_str).ok_or_else(|| GeoError::Format("missing \"type\"".into()))?;
    let coords = v.get("coordinates");
    let mut push = |g: Option<Geometry>| {
        if let Some(geometry) = g {
            out.push(Feature { properties: props.clone(), geometry });
        }
    };
    match ty {
        "FeatureCollection" => {
            for f in v.get("features").and_then(Value::as_array).into_iter().flatten() {
                collect_geojson(f, &Properties::new(), out)?;
            }
        }
        "Feature" => {
            let p = properties(v.get("properties"));
            if let Some(g) = v.get("geometry").filter(|g| !g.is_null()) {
                collect_geojson(g, &p, out)?;
            }
        }
        "GeometryCollection" => {
            for g in v.get("geometries").and_then(Value::as_array).into_iter().flatten() {
                collect_geojson(g, props, out)?;
            }
        }
        "Point" => push(coords.and_then(point).map(Geometry::Point)),
        "MultiPoint" => {
            for p in coords.and_then(Value::as_array).into_iter().flatten() {
                push(point(p).map(Geometry::Point));
            }
        }
        "LineString" => push(coords.and_then(line).map(Geometry::Line)),
        "MultiLineString" => {
            for l in coords.and_then(Value::as_array).into_iter().flatten() {
                push(line(l).map(Geometry::Line));
            }
        }
        "Polygon" => push(coords.and_then(polygon).map(Geometry::Polygon)),
        "MultiPolygon" => {
            for p in coords.and_then(Value::as_array).into_iter().flatten() {
                push(polygon(p).map(Geometry::Polygon));
            }
        }
        other => return Err(GeoError::Format(format!("unsupported GeoJSON type {other:?}"))),
    }
    Ok(())
}

fn latlon(v: &Value) -> Option<Point> {
    Some([v.get("lon")?.as_f64()?, v.get("lat")?.as_f64()?])
}

/// Overpass `out geom` output: nodes have lat/lon, ways and relation members a `geometry` list.
fn from_overpass(v: &Value) -> Vec<Feature> {
    let mut out = Vec::new();
    for e in v.get("elements").and_then(Value::as_array).into_iter().flatten() {
        let props = properties(e.get("tags"));
        if props.is_empty() {
            continue;
        }
        match e.get("type").and_then(Value::as_str) {
            Some("node") => {
                if let Some(p) = latlon(e) {
                    out.push(Feature { properties: props, geometry: Geometry::Point(p) });
                }
            }
            Some("way") => {
                let pts: Vec<Point> = e.get("geometry").and_then(Value::as_array).into_iter().flatten().filter_map(latlon).collect();
                if pts.len() < 2 {
                    continue;
                }
                let closed = pts.len() > 3 && pts.first() == pts.last();
                let linear = props.contains_key("highway")
                    || props.contains_key("barrier")
                    || props.get("power").is_some_and(|p| p.ends_with("line"))
                    || props.get("waterway").is_some_and(|w| w != "riverbank");
                let geometry = if closed && !linear { Geometry::Polygon(vec![pts]) } else { Geometry::Line(pts) };
                out.push(Feature { properties: props, geometry });
            }
            Some("relation") => {
                // Multipolygons: every closed outer member becomes a polygon, inner members holes of the previous one.
                let mut polys: Vec<Vec<Vec<Point>>> = Vec::new();
                for m in e.get("members").and_then(Value::as_array).into_iter().flatten() {
                    let pts: Vec<Point> = m.get("geometry").and_then(Value::as_array).into_iter().flatten().filter_map(latlon).collect();
                    if pts.len() < 4 || pts.first() != pts.last() {
                        continue;
                    }
                    match m.get("role").and_then(Value::as_str) {
                        Some("inner") => {
                            if let Some(p) = polys.last_mut() {
                                p.push(pts);
                            }
                        }
                        _ => polys.push(vec![pts]),
                    }
                }
                for p in polys {
                    out.push(Feature { properties: props.clone(), geometry: Geometry::Polygon(p) });
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geojson_multi_geometries_are_flattened() {
        let s = parse(r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","properties":{"building":"house","building:levels":2},"geometry":{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,0]]],[[[2,2],[3,2],[3,3],[2,2]]]]}},
            {"type":"Feature","properties":{"tags":{"highway":"primary"}},"geometry":{"type":"LineString","coordinates":[[0,0],[5,5]]}},
            {"type":"Feature","properties":{},"geometry":null}
        ]}"#)
        .unwrap();
        assert_eq!(s.features.len(), 3);
        assert_eq!(s.features[0].properties["building:levels"], "2");
        assert_eq!(s.features[2].properties["highway"], "primary");
        assert_eq!(s.crs, None);
    }

    #[test]
    fn overpass_ways_become_lines_or_polygons() {
        let s = parse(
            r#"{"elements":[
            {"type":"way","tags":{"building":"yes"},"geometry":[{"lat":0,"lon":0},{"lat":0,"lon":1},{"lat":1,"lon":1},{"lat":0,"lon":0}]},
            {"type":"way","tags":{"highway":"residential"},"geometry":[{"lat":0,"lon":0},{"lat":0,"lon":1},{"lat":1,"lon":1},{"lat":0,"lon":0}]},
            {"type":"node","tags":{"natural":"tree"},"lat":0.5,"lon":0.5}
        ]}"#,
        )
        .unwrap();
        assert!(matches!(s.features[0].geometry, Geometry::Polygon(_)));
        assert!(matches!(s.features[1].geometry, Geometry::Line(_)));
        assert!(matches!(s.features[2].geometry, Geometry::Point(_)));
        assert_eq!(s.crs, Some(DeclaredCrs::Wgs84));
    }
}
