//! City -> a folder of open urban data, shaped like what a real city
//! publishes: building footprints, street segments, land use, a GTFS feed
//! for the transit lines, census-like zones and an origin-destination table
//! of the trips the simulation made.
//!
//! The files are what [city2graph](https://city2graph.net) reads (GeoJSON
//! through GeoPandas, `load_gtfs`, `od_matrix_to_graph`); a script that
//! loads them all is written next to them.
//!
//! Everything is in WGS84. A city that was not imported from a map is put
//! on the equator at the prime meridian, with tiles of [`TILE_M`] metres.

use std::collections::{BTreeMap, BTreeSet};

use opencity_core::transit::{Mode, Service};
use opencity_core::{codes::connect, layer::TileKind, City, RoadKind, Structure, StructureCode};
use serde_json::{json, Map, Value};

use crate::import::{M_PER_DEG_LAT, M_PER_DEG_LON};

/// Metres to a tile side when the city has no map behind it.
pub const TILE_M: f64 = 16.0;
/// Tiles to a side of a zone of the origin-destination table.
pub const ZONE_TILES: u32 = 8;
/// How far a building's footprint stands back from the edge of its plot, in tiles.
const SETBACK: f64 = 0.12;

type Tile = (u32, u32);

/// The files of a data export, by name.
#[derive(Clone, Debug, Default)]
pub struct Dataset {
    pub files: Vec<(String, Vec<u8>)>,
    /// What is in it, for a message: (what, how many).
    pub counts: Vec<(&'static str, usize)>,
}

impl Dataset {
    /// One zip archive holding every file.
    pub fn zip(&self) -> Vec<u8> {
        zip(&self.files)
    }

    /// Writes the files into a folder, made if need be.
    pub fn write_dir(&self, dir: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        for (name, bytes) in &self.files {
            std::fs::write(dir.join(name), bytes)?;
        }
        Ok(())
    }

    /// "120 buildings, 45 street segments, …".
    pub fn summary(&self) -> String {
        self.counts.iter().filter(|(_, n)| *n > 0).map(|(what, n)| format!("{n} {what}")).collect::<Vec<_>>().join(", ")
    }

    pub fn file(&self, name: &str) -> Option<&[u8]> {
        self.files.iter().find(|(n, _)| n == name).map(|(_, b)| b.as_slice())
    }
}

/// Tile space -> longitude and latitude.
struct Frame {
    west: f64,
    north: f64,
    m: f64,
    lat_scale: f64,
    real: bool,
}

impl Frame {
    fn of(city: &City) -> Self {
        match city.meta.georef {
            Some(g) if g.wgs84 => Self { west: g.west, north: g.north, m: g.units_per_tile, lat_scale: g.lat_scale, real: true },
            _ => Self { west: 0.0, north: city.length() as f64 * TILE_M / M_PER_DEG_LAT, m: TILE_M, lat_scale: 1.0, real: false },
        }
    }

    fn at(&self, x: f64, y: f64) -> [f64; 2] {
        let r = |v: f64| (v * 1e7).round() / 1e7;
        [r(self.west + x * self.m / (M_PER_DEG_LON * self.lat_scale)), r(self.north - y * self.m / M_PER_DEG_LAT)]
    }

    fn centre(&self, (w, l): Tile) -> [f64; 2] {
        self.at(w as f64 + 0.5, l as f64 + 0.5)
    }

    /// A rectangle of tiles, set back by `inset` tiles on every side.
    fn rect(&self, w: u32, l: u32, sw: u32, sl: u32, inset: f64) -> Value {
        let (x0, y0, x1, y1) = (w as f64 + inset, l as f64 + inset, (w + sw) as f64 - inset, (l + sl) as f64 - inset);
        json!({"type": "Polygon", "coordinates": [[self.at(x0, y1), self.at(x1, y1), self.at(x1, y0), self.at(x0, y0), self.at(x0, y1)]]})
    }

    fn line(&self, tiles: &[Tile]) -> Value {
        json!({"type": "LineString", "coordinates": tiles.iter().map(|t| self.centre(*t)).collect::<Vec<_>>()})
    }
}

fn feature(geometry: Value, properties: Map<String, Value>) -> Value {
    json!({"type": "Feature", "geometry": geometry, "properties": properties})
}

fn collection(features: Vec<Value>) -> Vec<u8> {
    json!({"type": "FeatureCollection", "features": features}).to_string().into_bytes()
}

fn tile_id((w, l): Tile) -> String {
    format!("{w}_{l}")
}

fn zone_of((w, l): Tile) -> Tile {
    (w / ZONE_TILES, l / ZONE_TILES)
}

fn zone_id((c, r): Tile) -> String {
    format!("z{c}_{r}")
}

/// Whether a structure is a building (not a road, a line, a park or an
/// empty plot).
fn is_building(s: &Structure) -> bool {
    use StructureCode::*;
    match s.code {
        Road | PowerLine | Flora | Part | Test | Park | Wetland | Dump | WaterPipe => false,
        c if c.is_zone() => s.unit.is_some() || s.level > 0,
        _ => true,
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Runs of tiles between junctions and ends: every edge of the graph ends
/// up in one run. A run also ends where the kind of its edges changes.
fn runs<K: Copy + PartialEq>(edges: &BTreeSet<(Tile, Tile)>, kind: impl Fn(Tile, Tile) -> K) -> Vec<(K, Vec<Tile>)> {
    let mut adj: BTreeMap<Tile, Vec<Tile>> = BTreeMap::new();
    for (a, b) in edges {
        adj.entry(*a).or_default().push(*b);
        adj.entry(*b).or_default().push(*a);
    }
    let key = |a: Tile, b: Tile| if a < b { (a, b) } else { (b, a) };
    let breaks = |t: Tile| {
        let n = &adj[&t];
        n.len() != 2 || kind(t, n[0]) != kind(t, n[1])
    };
    let mut seen: BTreeSet<(Tile, Tile)> = BTreeSet::new();
    let mut out = Vec::new();
    let mut walk = |from: Tile, to: Tile, seen: &mut BTreeSet<(Tile, Tile)>| {
        let mut path = vec![from, to];
        seen.insert(key(from, to));
        let (mut prev, mut cur) = (from, to);
        while !breaks(cur) {
            let n = &adj[&cur];
            let next = if n[0] == prev { n[1] } else { n[0] };
            if !seen.insert(key(cur, next)) {
                break;
            }
            path.push(next);
            (prev, cur) = (cur, next);
        }
        out.push((kind(from, to), path));
    };
    for (&t, ns) in &adj {
        if breaks(t) {
            for &n in ns {
                if !seen.contains(&key(t, n)) {
                    walk(t, n, &mut seen);
                }
            }
        }
    }
    // What is left are rings with no junction.
    for &(a, b) in edges {
        if !seen.contains(&key(a, b)) {
            walk(a, b, &mut seen);
        }
    }
    out
}

/// A path without the tiles in the middle of its straight stretches.
fn corners(path: &[Tile]) -> Vec<Tile> {
    let step = |a: Tile, b: Tile| (b.0 as i64 - a.0 as i64, b.1 as i64 - a.1 as i64);
    let mut out: Vec<Tile> = Vec::new();
    for (i, &p) in path.iter().enumerate() {
        if i == 0 || i + 1 == path.len() || step(path[i - 1], p) != step(p, path[i + 1]) {
            out.push(p);
        }
    }
    out
}

fn road_tags(kind: RoadKind) -> &'static [(&'static str, &'static str)] {
    match kind {
        RoadKind::Street => &[("highway", "residential")],
        RoadKind::Highway => &[("highway", "motorway")],
        RoadKind::DirtRoad => &[("highway", "track"), ("surface", "ground")],
        RoadKind::PavedStreet => &[("highway", "residential"), ("surface", "sett")],
        RoadKind::Rail => &[("railway", "rail")],
        RoadKind::Tram => &[("highway", "residential"), ("railway", "tram")],
    }
}

fn hms(secs: u32) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// A CSV cell.
fn cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv(header: &str, rows: &[Vec<String>]) -> Vec<u8> {
    let mut s = String::from(header);
    s.push('\n');
    for r in rows {
        s.push_str(&r.iter().map(|c| cell(c)).collect::<Vec<_>>().join(","));
        s.push('\n');
    }
    s.into_bytes()
}

/// Speed between stops in km/h, and GTFS `route_type` and colour.
fn mode_facts(mode: Mode) -> (f64, u32, &'static str) {
    match mode {
        Mode::Bus => (25.0, 3, "D9480F"),
        Mode::Tram => (30.0, 0, "2F9E44"),
        Mode::Metro => (45.0, 1, "1971C2"),
    }
}

/// Seconds a vehicle stands at a stop, and at the end of the line.
const DWELL: u32 = 20;
const LAYOVER: u32 = 120;
/// The service day, and how much longer than at rush hour the wait is in
/// each part of it.
const PERIODS: [(u32, u32, f64); 5] = [
    (5 * 3600 + 1800, 7 * 3600, 1.5),
    (7 * 3600, 9 * 3600 + 1800, 1.0),
    (9 * 3600 + 1800, 16 * 3600 + 1800, 1.5),
    (16 * 3600 + 1800, 19 * 3600 + 1800, 1.0),
    (19 * 3600 + 1800, 24 * 3600, 2.0),
];

/// The year the timetable is valid for: the city's, or 2000 for a city
/// whose calendar is not one a timetable tool would take.
fn feed_year(city: &City) -> u32 {
    if (1900..=9999).contains(&city.date.year) {
        city.date.year
    } else {
        2000
    }
}

/// The transit lines as a GTFS feed (the files of the zip), or `None` when
/// nothing runs.
fn gtfs(city: &City, frame: &Frame, services: &[Service]) -> Option<Vec<(String, Vec<u8>)>> {
    if services.is_empty() {
        return None;
    }
    let name = city.meta.name.clone().unwrap_or_else(|| "citysim".into());
    let year = feed_year(city);
    let mut stops: BTreeMap<(Mode, Tile), (String, String, Tile)> = BTreeMap::new();
    let mut nth: BTreeMap<Mode, usize> = BTreeMap::new();
    let (mut routes, mut trips, mut times, mut freqs, mut shapes) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for s in services {
        let (kph, route_type, colour) = mode_facts(s.mode);
        for (origin, platform) in &s.stops {
            stops.entry((s.mode, *origin)).or_insert_with(|| {
                let n = nth.entry(s.mode).or_default();
                *n += 1;
                (format!("{}_{}", s.mode.id(), tile_id(*origin)), format!("{} stop {n}", s.mode.name()), *platform)
            });
        }
        let route_id = format!("r{}", s.id);
        routes.push(vec![route_id.clone(), "citysim".into(), s.name.clone(), String::new(), route_type.to_string(), colour.into(), "FFFFFF".into()]);
        let ride = |leg: &Vec<Tile>| (((leg.len().saturating_sub(1)) as f64 * frame.m / (kph / 3.6)).round() as u32).max(10);
        let one_way: u32 = s.legs.iter().map(ride).sum::<u32>() + DWELL * s.legs.len() as u32;
        let round = if s.ring { one_way } else { 2 * (one_way + LAYOVER) };
        let headway = ((round as f64 / s.vehicles as f64 / 30.0).round() as u32 * 30).clamp(120, 3600);
        // There and back; a ring runs one way round and ends where it began.
        for dir in 0..if s.ring { 1 } else { 2 } {
            let trip_id = format!("{route_id}_{dir}");
            let n = s.stops.len();
            let order: Vec<usize> = if dir == 0 { (0..n).collect() } else { (0..n).rev().collect() };
            let mut legs: Vec<Vec<Tile>> = if dir == 0 { s.legs.clone() } else { s.legs.iter().rev().map(|l| l.iter().rev().copied().collect()).collect() };
            let mut seq: Vec<usize> = order.clone();
            if s.ring {
                seq.push(order[0]);
            } else {
                legs.truncate(n - 1);
            }
            let head = &stops[&(s.mode, s.stops[*seq.last().unwrap()].0)].1;
            trips.push(vec![route_id.clone(), "daily".into(), trip_id.clone(), head.clone(), dir.to_string(), trip_id.clone()]);
            let mut t = PERIODS[0].0;
            for (k, &i) in seq.iter().enumerate() {
                if k > 0 {
                    t += ride(&legs[k - 1]);
                }
                let arrive = t;
                if k > 0 && k + 1 < seq.len() {
                    t += DWELL;
                }
                times.push(vec![trip_id.clone(), hms(arrive), hms(t), stops[&(s.mode, s.stops[i].0)].0.clone(), (k + 1).to_string(), "1".into()]);
            }
            let mut dist = 0.0;
            let mut pt = 0;
            for (k, leg) in legs.iter().enumerate() {
                for (j, tile) in leg.iter().enumerate().skip(usize::from(k > 0)) {
                    if j > 0 {
                        dist += frame.m;
                    }
                    pt += 1;
                    let [lon, lat] = frame.centre(*tile);
                    shapes.push(vec![trip_id.clone(), lat.to_string(), lon.to_string(), pt.to_string(), format!("{dist:.0}")]);
                }
            }
            for (from, to, factor) in PERIODS {
                let h = ((headway as f64 * factor / 30.0).round() as u32 * 30).min(3600);
                freqs.push(vec![trip_id.clone(), hms(from), hms(to), h.to_string(), "0".into()]);
            }
        }
    }
    let stop_rows: Vec<Vec<String>> = stops
        .values()
        .map(|(id, name, platform)| {
            let [lon, lat] = frame.centre(*platform);
            vec![id.clone(), name.clone(), lat.to_string(), lon.to_string()]
        })
        .collect();
    let day = |m: u32, d: u32| format!("{year:04}{m:02}{d:02}");
    Some(vec![
        (
            "agency.txt".into(),
            csv(
                "agency_id,agency_name,agency_url,agency_timezone",
                &[vec!["citysim".into(), format!("{name} transit"), "http://ingmmo.com/citysim/".into(), "Etc/UTC".into()]],
            ),
        ),
        ("stops.txt".into(), csv("stop_id,stop_name,stop_lat,stop_lon", &stop_rows)),
        ("routes.txt".into(), csv("route_id,agency_id,route_short_name,route_long_name,route_type,route_color,route_text_color", &routes)),
        ("trips.txt".into(), csv("route_id,service_id,trip_id,trip_headsign,direction_id,shape_id", &trips)),
        ("stop_times.txt".into(), csv("trip_id,arrival_time,departure_time,stop_id,stop_sequence,timepoint", &times)),
        ("frequencies.txt".into(), csv("trip_id,start_time,end_time,headway_secs,exact_times", &freqs)),
        ("shapes.txt".into(), csv("shape_id,shape_pt_lat,shape_pt_lon,shape_pt_sequence,shape_dist_traveled", &shapes)),
        (
            "calendar.txt".into(),
            csv(
                "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date",
                &[["daily", "1", "1", "1", "1", "1", "1", "1"].into_iter().map(String::from).chain([day(1, 1), day(12, 31)]).collect()],
            ),
        ),
        (
            "feed_info.txt".into(),
            csv(
                "feed_publisher_name,feed_publisher_url,feed_lang,feed_start_date,feed_end_date,feed_version",
                &[vec!["citysim".into(), "http://ingmmo.com/citysim/".into(), "en".into(), day(1, 1), day(12, 31), env!("CARGO_PKG_VERSION").into()]],
            ),
        ),
    ])
}

/// The city as open data (see the module's notes).
pub fn dataset(city: &City) -> Dataset {
    let frame = Frame::of(city);
    let rules = city.rules().cloned();
    let gb = city.glassbox.as_ref();
    let width = city.width();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    #[derive(Default)]
    struct Zone {
        residents: f64,
        jobs: f64,
        buildings: u32,
    }
    let mut zones: BTreeMap<Tile, Zone> = BTreeMap::new();

    let district_of = city.district_map();
    let district_names: Vec<String> = city.districts().into_iter().map(|d| d.name).collect();
    // Buildings and land use.
    let (mut buildings, mut land) = (Vec::new(), Vec::new());
    let mut population = 0.0;
    for ((w, l), tile) in city.layer.tiles() {
        let TileKind::Main(s) = &tile.kind else { continue };
        let zone = zones.entry(zone_of((w, l))).or_default();
        let (sw, sl) = s.size(city.footprints());
        let mut props = crate::export::tags(s.code, s.level);
        props.remove("opencity:level");
        let kind = props.remove("opencity:build").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        if is_building(s) {
            let class = match s.code {
                StructureCode::Residential | StructureCode::Commercial | StructureCode::Industrial => kind.as_str(),
                _ => "civic",
            };
            props.insert("id".into(), tile_id((w, l)).into());
            props.insert("class".into(), class.into());
            props.insert("kind".into(), kind.clone().into());
            props.insert("level".into(), s.level.into());
            props.entry("building").or_insert("yes".into());
            let unit = s.unit.and_then(|u| rules.as_ref().and_then(|r| r.units.get(u as usize)));
            let (residents, jobs) =
                unit.map_or((0.0, 0.0), |u| (u.produce.get("residents").copied().unwrap_or(0.0) as f64, u.needs.get("workers").copied().unwrap_or(0.0) as f64));
            if let Some(u) = unit {
                props.insert("unit".into(), u.id.clone().into());
                props.insert("name".into(), u.name.clone().into());
            }
            props.insert("residents".into(), round2(residents).into());
            props.insert("jobs".into(), round2(jobs).into());
            if let Some(name) = district_of.get((l * width + w) as usize).copied().flatten().and_then(|d| district_names.get(d as usize)) {
                props.insert("neighbourhood".into(), name.clone().into());
            }
            if let Some(state) = gb.and_then(|g| g.unit_state(w, l, width)) {
                props.insert("satisfaction".into(), round2(state.satisfaction as f64).into());
            }
            props.insert("area_m2".into(), (((sw as f64 - 2.0 * SETBACK) * (sl as f64 - 2.0 * SETBACK) * frame.m * frame.m).round()).into());
            zone.residents += residents;
            zone.jobs += jobs;
            zone.buildings += 1;
            population += residents;
            buildings.push(feature(frame.rect(w, l, sw, sl, SETBACK), props));
        } else {
            let class = match s.code {
                StructureCode::Park => "park",
                StructureCode::Wetland => "wetland",
                StructureCode::Dump => "landfill",
                c if c.is_zone() => "plot",
                _ => continue,
            };
            props.remove("building");
            props.remove("building:levels");
            props.insert("class".into(), class.into());
            props.insert("kind".into(), kind.into());
            land.push(feature(frame.rect(w, l, sw, sl, 0.0), props));
        }
    }
    // Water, a row of tiles at a time.
    let t = &city.terrain;
    for l in 0..t.length() {
        let mut w = 0;
        while w < t.width() {
            if t.tile_max(w, l) >= 0 {
                w += 1;
                continue;
            }
            let start = w;
            while w < t.width() && t.tile_max(w, l) < 0 {
                w += 1;
            }
            let mut props = Map::new();
            props.insert("class".into(), "water".into());
            props.insert("natural".into(), "water".into());
            land.push(feature(frame.rect(start, l, w - start, 1, 0.0), props));
        }
    }

    // Streets, tracks and tunnels, from junction to junction.
    let order = |k: RoadKind| RoadKind::ALL.iter().position(|r| *r == k).unwrap_or(0);
    let road = |p: Tile| city.layer.structure_at(p.0, p.1).filter(|s| s.code == StructureCode::Road);
    let mut edges: BTreeSet<(Tile, Tile)> = BTreeSet::new();
    for ((w, l), tile) in city.layer.tiles() {
        let TileKind::Main(s) = &tile.kind else { continue };
        if s.code != StructureCode::Road {
            continue;
        }
        for (bit, n) in [(connect::EAST, (w + 1, l)), (connect::SOUTH, (w, l + 1))] {
            if s.connections & bit != 0 && road(n).is_some() {
                edges.insert(((w, l), n));
            }
        }
    }
    let mut segments = Vec::new();
    let segment = |n: usize, tags: &[(&str, &str)], class: &str, path: &[Tile], traffic: Option<f64>| {
        let mut props = Map::new();
        props.insert("id".into(), format!("s{}", n + 1).into());
        props.insert("class".into(), class.into());
        for (k, v) in tags {
            props.insert((*k).into(), (*v).into());
        }
        props.insert("length_m".into(), round2((path.len() - 1) as f64 * frame.m).into());
        if let Some(t) = traffic {
            props.insert("traffic".into(), round2(t).into());
        }
        feature(frame.line(&corners(path)), props)
    };
    let edge_kind = |a: Tile, b: Tile| {
        let (ka, kb) = (road(a).map_or(RoadKind::Street, |s| s.road), road(b).map_or(RoadKind::Street, |s| s.road));
        if order(ka) <= order(kb) {
            ka
        } else {
            kb
        }
    };
    for (kind, path) in runs(&edges, edge_kind) {
        let traffic = path.iter().filter_map(|p| road(*p)).map(|s| s.traffic as f64).sum::<f64>() / path.len() as f64;
        segments.push(segment(segments.len(), road_tags(kind), kind.id(), &path, Some(traffic)));
    }
    let streets = segments.len();
    let tunnels: BTreeSet<Tile> = city.layer.tunnels().collect();
    let tunnel_edges: BTreeSet<(Tile, Tile)> =
        tunnels.iter().flat_map(|&(w, l)| [(w + 1, l), (w, l + 1)].into_iter().filter(|n| tunnels.contains(n)).map(move |n| ((w, l), n))).collect();
    for (_, path) in runs(&tunnel_edges, |_, _| ()) {
        segments.push(segment(segments.len(), &[("railway", "subway"), ("tunnel", "yes")], "metro", &path, None));
    }
    let segment_count = segments.len();

    // Transit.
    let services = city.services();
    let feed = gtfs(city, &frame, &services);
    let has_gtfs = feed.is_some();

    // Trips of the last tick, by building and by zone.
    let trips = gb.map(|g| g.last_trips.as_slice()).unwrap_or_default();
    let purpose = |r: usize| {
        rules.as_ref().and_then(|x| x.resources.get(r)).map(|d| d.parent.clone().unwrap_or_else(|| d.id.clone())).unwrap_or_else(|| format!("resource_{r}"))
    };
    let mut purposes: Vec<String> = Vec::new();
    let mut by_pair: BTreeMap<(Tile, Tile, String), (f64, f64)> = BTreeMap::new();
    let mut od: BTreeMap<(Tile, Tile), BTreeMap<String, f64>> = BTreeMap::new();
    for t in trips.iter().filter(|t| t.amount > 0.0) {
        let p = purpose(t.resource);
        if !purposes.contains(&p) {
            purposes.push(p.clone());
        }
        let e = by_pair.entry((t.from, t.to, p.clone())).or_default();
        e.0 += t.amount as f64;
        e.1 += (t.amount * t.road) as f64;
        let (a, b) = (zone_of(t.from), zone_of(t.to));
        zones.entry(a).or_default();
        zones.entry(b).or_default();
        *od.entry((a, b)).or_default().entry(p).or_default() += t.amount as f64;
    }
    purposes.sort();
    let trip_rows: Vec<Vec<String>> = by_pair
        .iter()
        .map(|((a, b, p), (flow, road))| {
            vec![tile_id(*a), tile_id(*b), zone_id(zone_of(*a)), zone_id(zone_of(*b)), p.clone(), format!("{flow:.3}"), format!("{:.2}", road / flow)]
        })
        .collect();
    let od_rows: Vec<Vec<String>> = od
        .iter()
        .map(|((a, b), by)| {
            let mut row = vec![zone_id(*a), zone_id(*b), format!("{:.3}", by.values().sum::<f64>())];
            row.extend(purposes.iter().map(|p| format!("{:.3}", by.get(p).copied().unwrap_or(0.0))));
            row
        })
        .collect();

    let zone_features: Vec<Value> = zones
        .iter()
        .map(|(&(c, r), z)| {
            let (w, l) = (c * ZONE_TILES, r * ZONE_TILES);
            let (sw, sl) = (ZONE_TILES.min(city.width() - w), ZONE_TILES.min(city.length() - l));
            let mut props = Map::new();
            props.insert("zone_id".into(), zone_id((c, r)).into());
            props.insert("residents".into(), round2(z.residents).into());
            props.insert("jobs".into(), round2(z.jobs).into());
            props.insert("buildings".into(), z.buildings.into());
            feature(frame.rect(w, l, sw, sl, 0.0), props)
        })
        .collect();

    let counts = vec![
        ("buildings", buildings.len()),
        ("street segments", streets),
        ("metro segments", segment_count - streets),
        ("land use areas", land.len()),
        ("zones", zone_features.len()),
        ("transit routes", services.len()),
        ("trips between buildings", trip_rows.len()),
        ("flows between zones", od_rows.len()),
    ];
    let about = json!({
        "name": city.meta.name,
        "generator": format!("citysim {}", env!("CARGO_PKG_VERSION")),
        "date": city.date.to_string(),
        "year": city.date.year,
        "gtfs_year": feed_year(city),
        "rules": rules.as_ref().map(|r| r.name.clone()),
        "tick": gb.map(|g| g.tick),
        "crs": "EPSG:4326",
        "georeferenced": frame.real,
        "metres_per_tile": frame.m,
        "tiles": [city.width(), city.length()],
        "zone_tiles": ZONE_TILES,
        "residents": round2(population),
        "purposes": purposes,
        "counts": counts.iter().map(|(k, v)| ((*k).to_string(), json!(v))).collect::<Map<String, Value>>(),
        "files": {
            "buildings.geojson": "building footprints: id, class, kind, unit, name, level, residents, jobs, satisfaction, area_m2, OSM-style tags",
            "segments.geojson": "streets, tracks and metro tunnels from junction to junction: id, class, length_m, traffic, OSM-style tags",
            "landuse.geojson": "parks, wetlands, landfills, empty plots and water",
            "zones.geojson": "a grid of zones: zone_id, residents, jobs, buildings",
            "od.csv": "trips of one tick between zones: origin, destination, flow (all purposes), then one column per purpose",
            "trips.csv": "the same trips between buildings (ids as in buildings.geojson; other ids are the map's gates)",
            "gtfs.zip": "the transit routes as a GTFS feed with frequencies and shapes",
            "ridership.csv": "people getting on or off at each stop in a tick (stop ids of the feed), and how full the stop is on routes the player drew",
        },
    });

    files.push(("city.json".into(), serde_json::to_vec_pretty(&about).unwrap_or_default()));
    files.push(("buildings.geojson".into(), collection(buildings)));
    files.push(("segments.geojson".into(), collection(segments)));
    files.push(("landuse.geojson".into(), collection(land)));
    files.push(("zones.geojson".into(), collection(zone_features)));
    if !od_rows.is_empty() {
        files.push(("od.csv".into(), csv(&format!("origin,destination,flow{}", purposes.iter().map(|p| format!(",{p}")).collect::<String>()), &od_rows)));
        files.push(("trips.csv".into(), csv("origin,destination,origin_zone,destination_zone,purpose,flow,road_share", &trip_rows)));
    }
    if let Some(feed) = feed {
        files.push(("gtfs.zip".into(), zip(&feed)));
    }
    // Riders at the stops, by the feed's stop ids.
    if let Some(t) = city.transit().filter(|t| !t.riders.is_empty()) {
        let mut seen = BTreeSet::new();
        let rows: Vec<Vec<String>> = services
            .iter()
            .flat_map(|s| s.stops.iter().map(move |(origin, _)| (s.mode, *origin)))
            .filter(|k| seen.insert(*k))
            .map(|(mode, origin)| {
                let riders = t.riders.get(&origin).copied().unwrap_or(0.0);
                let load = if t.manual { format!("{:.2}", t.load.get(&origin).copied().unwrap_or(0.0)) } else { String::new() };
                vec![format!("{}_{}", mode.id(), tile_id(origin)), format!("{riders:.2}"), load]
            })
            .collect();
        if !rows.is_empty() {
            files.push(("ridership.csv".into(), csv("stop_id,riders,load", &rows)));
        }
    }
    files.push(("city2graph_example.py".into(), include_bytes!("../data/city2graph_example.py").to_vec()));
    files.push(("README.md".into(), include_bytes!("../data/README.md").to_vec()));
    debug_assert!(has_gtfs == files.iter().any(|(n, _)| n == "gtfs.zip"));
    Dataset { files, counts }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    !bytes.iter().fold(!0u32, |c, b| table[((c ^ *b as u32) & 0xFF) as usize] ^ (c >> 8))
}

/// A zip archive of the files, stored as they are (no compression).
pub fn zip(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    // 1 January 2000, midnight, in DOS terms.
    const DATE: u16 = (20 << 9) | (1 << 5) | 1;
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for (name, data) in files {
        let (crc, size, offset) = (crc32(data), data.len() as u32, out.len() as u32);
        let header = |sig: u32, extra: &[u16]| {
            let mut h = sig.to_le_bytes().to_vec();
            for v in extra.iter().chain(&[20, 0, 0, 0, DATE]) {
                h.extend(v.to_le_bytes());
            }
            for v in [crc, size, size] {
                h.extend(v.to_le_bytes());
            }
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            h
        };
        out.extend(header(0x0403_4b50, &[]));
        out.extend(name.as_bytes());
        out.extend(data);
        central.extend(header(0x0201_4b50, &[20]));
        // Comment length, disk, internal and external attributes, offset.
        central.extend([0u8; 10]);
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let (start, n) = (out.len() as u32, files.len() as u16);
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    for v in [0, 0, n, n] {
        out.extend(v.to_le_bytes());
    }
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencity_core::{Footprints, Terrain, Tool};

    /// A street with three bus stops and homes along it, under GlassBox rules.
    fn town() -> City {
        let mut c = City::new(Terrain::flat(48, 16), None, Footprints::builtin(), 1);
        c.fund = 100_000;
        let dir = format!("{}/../../assets/rules", env!("CARGO_MANIFEST_DIR"));
        let mut r: opencity_core::rules::Ruleset = toml::from_str(&std::fs::read_to_string(format!("{dir}/glassbox.toml")).unwrap()).unwrap();
        for script in r.scripts.clone() {
            opencity_core::gbx::compile(&mut r, &script, &std::fs::read_to_string(format!("{dir}/{script}")).unwrap()).unwrap();
        }
        c.use_rules(std::sync::Arc::new(r));
        c.research_all();
        c.apply_tool(Tool::Road, (2, 8), (44, 8)).unwrap();
        c.apply_tool(Tool::Road, (20, 2), (20, 8)).unwrap();
        for w in [4, 22, 36] {
            c.apply_tool(Tool::BusStop, (w, 9), (w, 9)).unwrap();
        }
        c.apply_tool(Tool::ZoneResidential, (5, 7), (12, 7)).unwrap();
        c
    }

    fn features(data: &Dataset, file: &str) -> Vec<Value> {
        let v: Value = serde_json::from_slice(data.file(file).unwrap()).unwrap();
        v["features"].as_array().unwrap().clone()
    }

    #[test]
    fn a_town_comes_out_as_open_data() {
        let c = town();
        let data = dataset(&c);
        // Three bus stops are buildings; the plots are land until they grow.
        assert_eq!(features(&data, "buildings.geojson").len(), 3);
        assert!(features(&data, "landuse.geojson").iter().any(|f| f["properties"]["class"] == "plot"));
        // The street is cut at the junction with the side street: three segments.
        let segments = features(&data, "segments.geojson");
        assert_eq!(segments.len(), 3);
        let length: f64 = segments.iter().map(|f| f["properties"]["length_m"].as_f64().unwrap()).sum();
        assert_eq!(length, (42.0 + 6.0) * TILE_M);
        // One bus line there and back, as a zip a reader can walk.
        let feed = data.file("gtfs.zip").unwrap();
        assert_eq!(&feed[..4], b"PK\x03\x04");
        let text = String::from_utf8_lossy(feed);
        assert!(text.contains("r1_0,05:30:00,05:30:00,bus_4_9,1,1"), "{text}");
        assert_eq!(text.matches("bus_22_9,").count(), 3, "once in stops, once each way");
        assert!(text.contains("daily,1,1,1,1,1,1,1,20000101,20001231"));
        // No tick yet: no trips to tell of.
        assert!(data.file("od.csv").is_none());
        assert!(data.zip().len() > data.files.iter().map(|(_, b)| b.len()).sum::<usize>());
    }

    #[test]
    fn trips_are_counted_between_zones() {
        let mut c = City::new(Terrain::flat(72, 64), None, Footprints::builtin(), 1);
        c.use_rules(town().rules().unwrap().clone());
        opencity_core::scenario::demo_town(&mut c);
        for _ in 0..200 {
            c.simulate_round();
        }
        let data = dataset(&c);
        let od = String::from_utf8(data.file("od.csv").expect("a town that ran has trips").to_vec()).unwrap();
        let mut lines = od.lines();
        let header: Vec<&str> = lines.next().unwrap().split(',').collect();
        assert_eq!(&header[..3], ["origin", "destination", "flow"]);
        let zones: Vec<String> = features(&data, "zones.geojson").iter().map(|f| f["properties"]["zone_id"].as_str().unwrap().to_string()).collect();
        for line in lines {
            let row: Vec<&str> = line.split(',').collect();
            assert!(zones.iter().any(|z| z == row[0]) && zones.iter().any(|z| z == row[1]), "{line}");
            let parts: f64 = row[3..].iter().map(|v| v.parse::<f64>().unwrap()).sum();
            assert!((row[2].parse::<f64>().unwrap() - parts).abs() < 0.01, "{line}");
        }
        assert!(features(&data, "buildings.geojson").iter().any(|f| f["properties"]["residents"].as_f64().unwrap() > 0.0));
    }

    #[test]
    fn crc_of_the_usual_check_string() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn runs_end_at_junctions_and_cover_every_edge() {
        // A cross: four arms of two edges each around (2, 2).
        let mut edges = BTreeSet::new();
        for i in 0..2 {
            edges.insert(((i, 2), (i + 1, 2)));
            edges.insert(((i + 2, 2), (i + 3, 2)));
            edges.insert(((2, i), (2, i + 1)));
            edges.insert(((2, i + 2), (2, i + 3)));
        }
        let r = runs(&edges, |_, _| ());
        assert_eq!(r.len(), 4);
        assert!(r.iter().all(|(_, p)| p.len() == 3 && (p[0] == (2, 2) || p[2] == (2, 2))));
        // A ring with no junction comes out as one closed run.
        let ring: BTreeSet<(Tile, Tile)> = [((0, 0), (1, 0)), ((1, 0), (1, 1)), ((0, 1), (1, 1)), ((0, 0), (0, 1))].into();
        let r = runs(&ring, |_, _| ());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].1.len(), 5);
        assert_eq!(corners(&[(0, 0), (1, 0), (2, 0), (2, 1)]), vec![(0, 0), (2, 0), (2, 1)]);
    }
}
