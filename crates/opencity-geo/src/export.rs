//! City -> GeoJSON, with OSM-style tags so the result opens in any GIS and
//! re-imports losslessly through the `opencity:build` rule.

use opencity_core::{codes::connect, layer::TileKind, City, Direction, GeoRef, StructureCode};
use serde_json::{json, Map, Value};

use crate::import::{M_PER_DEG_LAT, M_PER_DEG_LON};

#[derive(Clone, Debug)]
pub struct ExportOptions {
    /// Also write water tiles and an `ele` point on every terrain corner.
    pub terrain: bool,
    /// Metres per terrain step, for `ele`.
    pub metres_per_step: f64,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { terrain: true, metres_per_step: 8.0 }
    }
}

struct Unproject(GeoRef);

impl Unproject {
    /// Tile-space point -> source coordinates.
    fn at(&self, x: f64, y: f64) -> [f64; 2] {
        let g = &self.0;
        let (dx, dy) = (x * g.units_per_tile, y * g.units_per_tile);
        if g.wgs84 {
            [g.west + dx / (M_PER_DEG_LON * g.lat_scale), g.north - dy / M_PER_DEG_LAT]
        } else {
            [g.west + dx, g.north - dy]
        }
    }

    fn rect(&self, w: u32, l: u32, sw: u32, sl: u32) -> Value {
        let (x0, y0, x1, y1) = (w as f64, l as f64, (w + sw) as f64, (l + sl) as f64);
        // Counter-clockwise exterior ring (RFC 7946).
        json!({"type": "Polygon", "coordinates": [[self.at(x0, y1), self.at(x1, y1), self.at(x1, y0), self.at(x0, y0), self.at(x0, y1)]]})
    }
}

fn tags(code: StructureCode, level: u8) -> Map<String, Value> {
    use StructureCode::*;
    let floors = (level / 10 + 1).to_string();
    let (build, extra): (&str, &[(&str, &str)]) = match code {
        Residential => ("residential", &[("landuse", "residential"), ("building", "house")]),
        Commercial => ("commercial", &[("landuse", "retail"), ("building", "commercial")]),
        Industrial => ("industrial", &[("landuse", "industrial"), ("building", "industrial")]),
        Road => ("road", &[("highway", "residential")]),
        PowerLine => ("power_line", &[("power", "minor_line")]),
        CoalPlant => ("coal_plant", &[("power", "plant"), ("plant:source", "coal")]),
        NuclearPlant => ("nuclear_plant", &[("power", "plant"), ("plant:source", "nuclear")]),
        Park => ("park", &[("leisure", "park")]),
        Flora => ("trees", &[("natural", "tree")]),
        FireDept => ("fire_station", &[("amenity", "fire_station")]),
        PoliceDept => ("police", &[("amenity", "police")]),
        Hospital => ("hospital", &[("amenity", "hospital")]),
        School => ("school", &[("amenity", "school")]),
        HighSchool => ("high_school", &[("amenity", "school"), ("isced:level", "3")]),
        University => ("university", &[("amenity", "university")]),
        Stadium => ("stadium", &[("leisure", "stadium")]),
        Mall => ("mall", &[("shop", "mall")]),
        Museum => ("museum", &[("tourism", "museum")]),
        ModernArtMuseum => ("modern_art_museum", &[("tourism", "museum"), ("museum", "art")]),
        OperaHouse => ("opera_house", &[("amenity", "theatre"), ("theatre:genre", "opera")]),
        ArchaeologyMuseum => ("archaeology_museum", &[("tourism", "museum"), ("museum", "archaeological")]),
        Station => ("station", &[("railway", "station"), ("building", "train_station")]),
        Harbour => ("harbour", &[("landuse", "port"), ("harbour", "yes")]),
        WaterPipe => ("water_pipe", &[("man_made", "pipeline"), ("substance", "water")]),
        PumpingStation => ("pumping_station", &[("man_made", "water_works")]),
        WaterTower => ("water_tower", &[("man_made", "water_tower")]),
        WaterTreatment => ("water_treatment", &[("man_made", "wastewater_plant")]),
        Dump => ("dump", &[("landuse", "landfill")]),
        Recycling => ("recycling", &[("amenity", "recycling"), ("recycling_type", "centre")]),
        Incinerator => ("incinerator", &[("power", "plant"), ("plant:source", "waste")]),
        BusStop => ("bus_stop", &[("highway", "bus_stop"), ("shelter", "yes")]),
        TramStop => ("tram_stop", &[("railway", "tram_stop"), ("public_transport", "platform")]),
        MetroStation => ("metro_station", &[("railway", "subway_entrance")]),
        Airport => ("airport", &[("aeroway", "aerodrome")]),
        Seaport => ("seaport", &[("landuse", "port"), ("industrial", "port")]),
        Test | Part => ("ignore", &[]),
    };
    let mut m = Map::new();
    m.insert("opencity:build".into(), build.into());
    for (k, v) in extra {
        m.insert((*k).into(), (*v).into());
    }
    if code.is_zone() {
        m.insert("opencity:level".into(), level.to_string().into());
        m.insert("building:levels".into(), floors.into());
    }
    m
}

fn feature(geometry: Value, properties: Map<String, Value>) -> Value {
    json!({"type": "Feature", "geometry": geometry, "properties": properties})
}

pub fn export(city: &City, opts: &ExportOptions) -> Value {
    let georef = city.meta.georef.unwrap_or(GeoRef { wgs84: false, west: 0.0, north: city.length() as f64, units_per_tile: 1.0, lat_scale: 1.0 });
    let un = Unproject(georef);
    let mut features = Vec::new();
    for ((w, l), tile) in city.layer.tiles() {
        let TileKind::Main(s) = &tile.kind else { continue };
        let mut props = tags(s.code, s.level);
        if s.code == StructureCode::Road && s.road.is_rail() {
            props.remove("highway");
            props.insert("opencity:build".into(), "rail".into());
            props.insert("railway".into(), "rail".into());
        }
        if s.code == StructureCode::Road && s.road == opencity_core::RoadKind::Tram {
            props.insert("opencity:build".into(), "tram".into());
            props.insert("railway".into(), "tram".into());
        }
        match s.code {
            StructureCode::Road | StructureCode::PowerLine => {
                let c = |dw: f64, dl: f64| un.at(w as f64 + 0.5 + dw, l as f64 + 0.5 + dl);
                let mut segments = Vec::new();
                for (dir, bit) in [(Direction::East, connect::EAST), (Direction::South, connect::SOUTH)] {
                    if s.connections & bit != 0 {
                        let (dw, dl) = dir.delta();
                        segments.push(json!([c(0.0, 0.0), c(dw as f64, dl as f64)]));
                    }
                }
                if s.connections == 0 {
                    segments.push(json!([c(-0.5, 0.0), c(0.5, 0.0)]));
                }
                for seg in segments {
                    features.push(feature(json!({"type": "LineString", "coordinates": seg}), props.clone()));
                }
            }
            StructureCode::Flora => features.push(feature(json!({"type": "Point", "coordinates": un.at(w as f64 + 0.5, l as f64 + 0.5)}), props)),
            StructureCode::Part | StructureCode::Test => {}
            _ => {
                let (sw, sl) = s.size(city.footprints());
                features.push(feature(un.rect(w, l, sw, sl), props));
            }
        }
    }
    // Pipes and metro tunnels run underground, under whatever stands on the tile.
    let mut pipe = tags(StructureCode::WaterPipe, 0);
    pipe.insert("location".into(), "underground".into());
    let mut metro = Map::new();
    for (k, v) in [("opencity:build", "metro"), ("railway", "subway"), ("tunnel", "yes"), ("location", "underground")] {
        metro.insert(k.into(), v.into());
    }
    let layer = &city.layer;
    type Run<'a> = (Vec<(u32, u32)>, fn(&opencity_core::BuildingLayer, u32, u32) -> bool, &'a Map<String, Value>);
    let runs: [Run; 2] = [(layer.pipes().collect(), |y, w, l| y.has_pipe(w, l), &pipe), (layer.tunnels().collect(), |y, w, l| y.has_tunnel(w, l), &metro)];
    for (tiles, has, props) in runs {
        for (w, l) in tiles {
            let c = |dw: f64, dl: f64| un.at(w as f64 + 0.5 + dw, l as f64 + 0.5 + dl);
            let mut segments = Vec::new();
            for (dw, dl) in [(1, 0), (0, 1)] {
                if has(layer, w + dw, l + dl) {
                    segments.push(json!([c(0.0, 0.0), c(dw as f64, dl as f64)]));
                }
            }
            let alone = segments.is_empty() && !(w > 0 && has(layer, w - 1, l)) && !(l > 0 && has(layer, w, l - 1));
            if alone {
                segments.push(json!([c(-0.5, 0.0), c(0.5, 0.0)]));
            }
            for seg in segments {
                features.push(feature(json!({"type": "LineString", "coordinates": seg}), props.clone()));
            }
        }
    }
    if opts.terrain {
        let t = &city.terrain;
        for l in 0..t.length() {
            for w in 0..t.width() {
                if t.tile_max(w, l) < 0 {
                    let mut p = Map::new();
                    p.insert("natural".into(), "water".into());
                    p.insert("opencity:build".into(), "water".into());
                    features.push(feature(un.rect(w, l, 1, 1), p));
                }
            }
        }
        for l in 0..=t.length() {
            for w in 0..=t.width() {
                let mut p = Map::new();
                p.insert("ele".into(), json!(t.corner(w, l) as f64 * opts.metres_per_step));
                p.insert("opencity:build".into(), "ignore".into());
                features.push(feature(json!({"type": "Point", "coordinates": un.at(w as f64, l as f64)}), p));
            }
        }
    }
    let mut fc = json!({"type": "FeatureCollection", "features": features});
    if !georef.wgs84 {
        // Mark planar data explicitly so re-imports do not guess.
        fc["crs"] = json!({"type": "name", "properties": {"name": "urn:opencity:planar"}});
    }
    if let Some(n) = &city.meta.name {
        fc["name"] = n.clone().into();
    }
    fc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{import, ImportOptions, Rules};
    use opencity_core::{Footprints, Terrain, Tool};

    #[test]
    fn export_reimports_the_same_city() {
        let mut c = City::new(Terrain::flat(30, 30), None, Footprints::builtin(), 1);
        c.apply_tool(Tool::Road, (2, 2), (20, 10)).unwrap();
        c.apply_tool(Tool::ZoneResidential, (3, 3), (8, 6)).unwrap();
        c.apply_tool(Tool::CoalPlant, (10, 12), (10, 12)).unwrap();
        c.apply_tool(Tool::Trees, (25, 25), (27, 27)).unwrap();
        c.apply_tool(Tool::LowerTerrain, (15, 25), (15, 25)).unwrap();
        let json = export(&c, &ExportOptions::default()).to_string();
        let opts = ImportOptions { units_per_tile: Some(1.0), margin: 0, ..Default::default() };
        let (d, report) = import(&json, &Rules::builtin(), Footprints::builtin(), &opts).unwrap();
        assert_eq!((d.width(), d.length()), (30, 30), "{report}");
        for code in [StructureCode::Road, StructureCode::Residential, StructureCode::CoalPlant, StructureCode::Flora] {
            let count = |city: &City| city.layer.structures().filter(|(_, s)| s.code == code).count();
            assert_eq!(count(&c), count(&d), "{code:?}\n{report}");
        }
        assert_eq!(d.layer.code_at(4, 4), Some(StructureCode::Residential));
        assert_eq!(c.layer.structure_at(10, 2).map(|s| s.graphic), d.layer.structure_at(10, 2).map(|s| s.graphic));
    }
}
