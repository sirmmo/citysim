//! GeoJSON -> City.

use std::collections::BTreeMap;

use opencity_core::{layer::BuildingLayer, layer::Structure, terrain::HEIGHT_MAX, City, CityMeta, Footprints, GeoRef, Rect, StructureCode, Terrain};

use crate::{
    rules::{Build, Rules},
    source::{self, DeclaredCrs, Feature, Geometry, Point},
    GeoError,
};

/// Approximate metres per degree of latitude / of longitude at the equator.
pub const M_PER_DEG_LAT: f64 = 110_574.0;
pub const M_PER_DEG_LON: f64 = 111_320.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CrsMode {
    /// Use the file's `crs` member, else guess from the coordinate ranges.
    #[default]
    Auto,
    Wgs84,
    Planar,
}

/// A possibly-BCE calendar date, ordered chronologically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct HistDate {
    pub year: i64,
    pub month: u32,
    pub day: u32,
}

impl HistDate {
    /// Parses `1920`, `1920-05`, `1920-05-03`, `-0500`, `~1850`, `1850..1860` (start).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('~');
        let (neg, s) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            return None;
        }
        let year: i64 = digits.parse().ok()?;
        let mut rest = s[digits.len()..].split('-').skip(1).map(|p| p.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<u32>().ok());
        let month = rest.next().flatten().unwrap_or(1).clamp(1, 12);
        let day = rest.next().flatten().unwrap_or(1).clamp(1, 31);
        Some(Self { year: if neg { -year } else { year }, month, day })
    }
}

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub crs: CrsMode,
    /// Metres (WGS84) or map units (planar) per tile. Default: 16 m for WGS84,
    /// planar data is fitted to about 96 tiles.
    pub units_per_tile: Option<f64>,
    /// Largest city side; the scale is coarsened to fit.
    pub max_size: u32,
    /// Empty tiles around the data.
    pub margin: u32,
    /// Keep only features that existed on this date.
    pub date: Option<HistDate>,
    /// `[min_x, min_y, max_x, max_y]` in source coordinates.
    pub bbox: Option<[f64; 4]>,
    /// Elevation difference (metres) per terrain step.
    pub metres_per_step: f64,
    pub seed: u64,
    pub name: Option<String>,
    /// Fantasy world name (overrides the file's `world` member).
    pub world: Option<String>,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            crs: CrsMode::Auto,
            units_per_tile: None,
            max_size: 128,
            margin: 2,
            date: None,
            bbox: None,
            metres_per_step: 8.0,
            seed: 1,
            name: None,
            world: None,
        }
    }
}

/// What the import did, for the user.
#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    pub features: usize,
    pub outside_date: usize,
    pub outside_bbox: usize,
    pub unclassified: usize,
    /// kind -> (features, tiles placed, tiles refused)
    pub by_kind: BTreeMap<&'static str, (usize, usize, usize)>,
    pub elevation_samples: usize,
    pub width: u32,
    pub length: u32,
    pub units_per_tile: f64,
    pub wgs84: bool,
}

impl std::fmt::Display for ImportReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} x {} tiles, {:.1} {} per tile ({})",
            self.width,
            self.length,
            self.units_per_tile,
            if self.wgs84 { "m" } else { "units" },
            if self.wgs84 { "WGS84" } else { "planar" }
        )?;
        writeln!(f, "{} features: {} outside date, {} outside bbox, {} unclassified", self.features, self.outside_date, self.outside_bbox, self.unclassified)?;
        writeln!(f, "{} elevation samples", self.elevation_samples)?;
        for (k, (n, ok, ko)) in &self.by_kind {
            writeln!(f, "  {k:<14} {n:>6} features {ok:>7} tiles placed {ko:>6} refused")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct Projection {
    wgs84: bool,
    west: f64,
    north: f64,
    upt: f64,
    lat_scale: f64,
    margin: f64,
}

impl Projection {
    /// Source coordinate -> continuous tile coordinate (x = w, y = l).
    fn tile(&self, p: Point) -> [f64; 2] {
        let (dx, dy) = if self.wgs84 {
            ((p[0] - self.west) * M_PER_DEG_LON * self.lat_scale, (self.north - p[1]) * M_PER_DEG_LAT)
        } else {
            (p[0] - self.west, self.north - p[1])
        };
        [dx / self.upt + self.margin, dy / self.upt + self.margin]
    }

    fn georef(&self) -> GeoRef {
        let m = self.margin * self.upt;
        let (west, north) =
            if self.wgs84 { (self.west - m / (M_PER_DEG_LON * self.lat_scale), self.north + m / M_PER_DEG_LAT) } else { (self.west - m, self.north + m) };
        GeoRef { wgs84: self.wgs84, west, north, units_per_tile: self.upt, lat_scale: self.lat_scale }
    }
}

fn points(g: &Geometry) -> Box<dyn Iterator<Item = Point> + '_> {
    match g {
        Geometry::Point(p) => Box::new(std::iter::once(*p)),
        Geometry::Line(l) => Box::new(l.iter().copied()),
        Geometry::Polygon(rings) => Box::new(rings.iter().flatten().copied()),
    }
}

fn parse_number(s: &str) -> Option<f64> {
    let t: String = s.trim().chars().take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')).collect();
    t.parse().ok()
}

struct Classified<'a> {
    feature: &'a Feature,
    build: Build,
}

/// Imports a city from GeoJSON / Overpass JSON text.
pub fn import(text: &str, rules: &Rules, footprints: Footprints, opts: &ImportOptions) -> Result<(City, ImportReport), GeoError> {
    let src = source::parse(text)?;
    let mut report = ImportReport { features: src.features.len(), ..Default::default() };

    // 1. Filter by date and bbox, classify.
    let in_bbox = |f: &Feature| match opts.bbox {
        None => true,
        Some([x0, y0, x1, y1]) => points(&f.geometry).any(|p| p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1),
    };
    let mut kept: Vec<Classified> = Vec::new();
    let mut elevation: Vec<(Point, f64)> = Vec::new();
    for f in &src.features {
        if let Some(date) = opts.date {
            let start = rules.first(&f.properties, &rules.start_date).and_then(HistDate::parse);
            let end = rules.first(&f.properties, &rules.end_date).and_then(HistDate::parse);
            if start.is_some_and(|s| s > date) || end.is_some_and(|e| e <= date) {
                report.outside_date += 1;
                continue;
            }
        }
        if !in_bbox(f) {
            report.outside_bbox += 1;
            continue;
        }
        if let Some(e) = rules.first(&f.properties, &rules.elevation).and_then(parse_number) {
            elevation.extend(points(&f.geometry).map(|p| (p, e)));
        }
        match rules.classify(&f.properties) {
            Some(Build::Ignore) => {}
            Some(build) => kept.push(Classified { feature: f, build }),
            None => report.unclassified += 1,
        }
    }
    report.elevation_samples = elevation.len();
    if kept.is_empty() && elevation.is_empty() {
        return Err(GeoError::Empty("no feature matched the rules (check --date, --bbox or --rules)".into()));
    }

    // 2. Extent and projection.
    let all = kept.iter().flat_map(|c| points(&c.feature.geometry)).chain(elevation.iter().map(|(p, _)| *p));
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in all {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    if let Some([bx0, by0, bx1, by1]) = opts.bbox {
        (x0, y0, x1, y1) = (bx0.max(x0), by0.max(y0), bx1.min(x1), by1.min(y1));
    }
    let wgs84 = match opts.crs {
        CrsMode::Wgs84 => true,
        CrsMode::Planar => false,
        CrsMode::Auto => match src.crs {
            Some(DeclaredCrs::Wgs84) => true,
            Some(DeclaredCrs::Planar) => false,
            None => x0 >= -180.0 && x1 <= 180.0 && y0 >= -90.0 && y1 <= 90.0 && (x1 - x0) < 2.0 && (y1 - y0) < 2.0,
        },
    };
    let lat_scale = if wgs84 { ((y0 + y1) / 2.0).to_radians().cos().max(0.01) } else { 1.0 };
    let (extent_x, extent_y) = if wgs84 { ((x1 - x0) * M_PER_DEG_LON * lat_scale, (y1 - y0) * M_PER_DEG_LAT) } else { (x1 - x0, y1 - y0) };
    let extent = extent_x.max(extent_y).max(f64::EPSILON);
    let inner_max = opts.max_size.saturating_sub(2 * opts.margin).max(4) as f64;
    let mut upt = opts.units_per_tile.unwrap_or(if wgs84 { 16.0 } else { extent / inner_max.min(96.0) });
    if extent / upt > inner_max {
        upt = extent / inner_max;
    }
    let proj = Projection { wgs84, west: x0, north: y1, upt, lat_scale, margin: opts.margin as f64 };
    let width = ((extent_x / upt).ceil() as u32 + 2 * opts.margin).clamp(8, opts.max_size.max(8));
    let length = ((extent_y / upt).ceil() as u32 + 2 * opts.margin).clamp(8, opts.max_size.max(8));
    report.width = width;
    report.length = length;
    report.units_per_tile = upt;
    report.wgs84 = wgs84;

    // 3. Terrain: elevation (inverse distance weighting) and water.
    let mut terrain = Terrain::flat(width, length);
    if !elevation.is_empty() {
        let step = elevation.len().div_ceil(3000);
        let samples: Vec<([f64; 2], f64)> = elevation.iter().step_by(step).map(|(p, e)| (proj.tile(*p), *e)).collect();
        let base = samples.iter().map(|s| s.1).fold(f64::MAX, f64::min);
        for l in 0..=length {
            for w in 0..=width {
                let (mut num, mut den) = (0.0, 0.0);
                for (p, e) in &samples {
                    let d2 = (p[0] - w as f64).powi(2) + (p[1] - l as f64).powi(2);
                    if d2 < 1e-6 {
                        (num, den) = (*e, 1.0);
                        break;
                    }
                    num += e / d2;
                    den += 1.0 / d2;
                }
                let h = ((num / den - base) / opts.metres_per_step).round().clamp(0.0, HEIGHT_MAX as f64);
                terrain.set_corner(w, l, h as i32);
            }
        }
    }
    let rasterized: Vec<Vec<(u32, u32)>> = kept.iter().map(|c| rasterize(&c.feature.geometry, &proj, width, length)).collect();
    for (c, tiles) in kept.iter().zip(&rasterized) {
        if c.build == Build::Water {
            for &(w, l) in tiles {
                for (cw, cl) in [(w, l), (w + 1, l), (w, l + 1), (w + 1, l + 1)] {
                    terrain.set_corner(cw, cl, (terrain.corner(cw, cl) as i32).min(-1));
                }
            }
        }
    }

    // 4. Structures, in an order that lets networks claim their tiles first.
    let mut city = City::new(terrain, None, footprints, opts.seed);
    city.meta = CityMeta {
        name: opts.name.clone(),
        source: None,
        georef: Some(proj.georef()),
        year: opts.date.map(|d| d.year),
        world: opts.world.clone().or_else(|| src.world.clone()),
    };
    let mut order: Vec<usize> = (0..kept.len()).collect();
    let area = |i: usize| rasterized[i].len();
    order.sort_by_key(|&i| (priority(kept[i].build), area(i)));
    for i in order {
        let c = &kept[i];
        let entry = report.by_kind.entry(c.build.name()).or_default();
        entry.0 += 1;
        let (ok, ko) = place(&mut city, c, &rasterized[i], rules);
        entry.1 += ok;
        entry.2 += ko;
    }
    city.refresh();
    Ok((city, report))
}

fn priority(b: Build) -> u8 {
    use Build::*;
    match b {
        Water => 0,
        Road => 1,
        Rail => 1,
        PowerLine | WaterPipe | Tram | Metro => 2,
        CoalPlant | NuclearPlant | SolarPlant | FusionPlant => 3,
        FireStation | Police | Hospital | School | HighSchool | University | Station | Harbour | PumpingStation | WaterTower | WaterTreatment | Dump
        | Recycling | Incinerator | BusStop | TramStop | MetroStation | Airport | Seaport => 4,
        Stadium | Mall | Museum | ModernArtMuseum | OperaHouse | ArchaeologyMuseum => 4,
        Residential | Commercial | Industrial => 5,
        Park => 6,
        Trees => 7,
        Ignore => 8,
    }
}

fn code_of(b: Build) -> Option<StructureCode> {
    use Build::*;
    Some(match b {
        Residential => StructureCode::Residential,
        Commercial => StructureCode::Commercial,
        Industrial => StructureCode::Industrial,
        Road => StructureCode::Road,
        PowerLine => StructureCode::PowerLine,
        CoalPlant => StructureCode::CoalPlant,
        NuclearPlant => StructureCode::NuclearPlant,
        SolarPlant => StructureCode::SolarPlant,
        FusionPlant => StructureCode::FusionPlant,
        Park => StructureCode::Park,
        Trees => StructureCode::Flora,
        FireStation => StructureCode::FireDept,
        Police => StructureCode::PoliceDept,
        Hospital => StructureCode::Hospital,
        School => StructureCode::School,
        HighSchool => StructureCode::HighSchool,
        University => StructureCode::University,
        Stadium => StructureCode::Stadium,
        Mall => StructureCode::Mall,
        Museum => StructureCode::Museum,
        ModernArtMuseum => StructureCode::ModernArtMuseum,
        OperaHouse => StructureCode::OperaHouse,
        ArchaeologyMuseum => StructureCode::ArchaeologyMuseum,
        Rail | Tram => StructureCode::Road,
        Station => StructureCode::Station,
        Harbour => StructureCode::Harbour,
        WaterPipe => StructureCode::WaterPipe,
        PumpingStation => StructureCode::PumpingStation,
        WaterTower => StructureCode::WaterTower,
        WaterTreatment => StructureCode::WaterTreatment,
        Dump => StructureCode::Dump,
        Recycling => StructureCode::Recycling,
        Incinerator => StructureCode::Incinerator,
        BusStop => StructureCode::BusStop,
        TramStop => StructureCode::TramStop,
        MetroStation => StructureCode::MetroStation,
        Airport => StructureCode::Airport,
        Seaport => StructureCode::Seaport,
        Water | Metro | Ignore => return None,
    })
}

/// Places one feature; returns (tiles placed, tiles refused).
fn place(city: &mut City, c: &Classified, tiles: &[(u32, u32)], rules: &Rules) -> (usize, usize) {
    if c.build == Build::Metro {
        // Tunnels run under whatever is there.
        let ok = tiles.iter().filter(|&&(w, l)| city.layer.lay_tunnel(Rect::tile(w, l)).is_ok()).count();
        return (ok, tiles.len() - ok);
    }
    let Some(code) = code_of(c.build) else { return (tiles.len(), 0) };
    let props = &c.feature.properties;
    let (layer, terrain, footprints, rng) = city.parts_mut();
    if code.is_block() {
        let (sw, sl) = BuildingLayer::block_size(code, footprints);
        // Centre the block on the feature, then search nearby.
        let (cw, cl) = centroid(tiles);
        let origin = (cw.saturating_sub(sw / 2), cl.saturating_sub(sl / 2));
        for r in 0..=4i64 {
            for dl in -r..=r {
                for dw in -r..=r {
                    let (w, l) = (origin.0 as i64 + dw, origin.1 as i64 + dl);
                    if w < 0 || l < 0 {
                        continue;
                    }
                    if layer.build(Rect::tile(w as u32, l as u32), code, terrain, footprints, rng).is_ok() {
                        return ((sw * sl) as usize, 0);
                    }
                }
            }
        }
        return (0, 1);
    }
    let level = props
        .get("opencity:level")
        .and_then(|v| v.parse::<u32>().ok())
        .or_else(|| rules.first(props, &rules.levels).and_then(parse_number).map(|floors| ((floors.max(1.0) - 1.0) * 10.0 + 5.0) as u32));
    let (mut ok, mut ko) = (0, 0);
    for &(w, l) in tiles {
        let built = if c.build == Build::Rail {
            layer.build_road(Rect::tile(w, l), opencity_core::RoadKind::Rail, terrain)
        } else if c.build == Build::Tram {
            layer.build_road(Rect::tile(w, l), opencity_core::RoadKind::Tram, terrain)
        } else {
            layer.build(Rect::tile(w, l), code, terrain, footprints, rng)
        };
        match built {
            Ok(_) => {
                ok += 1;
                if let (Some(level), true) = (level, code.is_zone()) {
                    set_level(layer, footprints, w, l, code, level);
                }
            }
            Err(_) => ko += usize::from(layer.code_at(w, l) != Some(code)),
        }
    }
    (ok, ko)
}

/// Highest level up to `target` whose model fits on one tile.
fn set_level(layer: &mut BuildingLayer, footprints: &Footprints, w: u32, l: u32, code: StructureCode, target: u32) {
    let level = (0..=target.min(99) as u8).rev().find(|&lv| footprints.size(Structure::zone_graphic(code, lv)) == (1, 1)).unwrap_or(0);
    if let Some(s) = layer.structure_mut(w, l) {
        s.level = level;
        s.graphic = Structure::zone_graphic(code, level);
    }
}

fn centroid(tiles: &[(u32, u32)]) -> (u32, u32) {
    if tiles.is_empty() {
        return (0, 0);
    }
    let n = tiles.len() as u64;
    let (sw, sl) = tiles.iter().fold((0u64, 0u64), |(a, b), &(w, l)| (a + w as u64, b + l as u64));
    ((sw / n) as u32, (sl / n) as u32)
}

/// Tiles covered by a geometry.
fn rasterize(g: &Geometry, proj: &Projection, width: u32, length: u32) -> Vec<(u32, u32)> {
    let inside = |x: f64, y: f64| x >= 0.0 && y >= 0.0 && x < width as f64 && y < length as f64;
    match g {
        Geometry::Point(p) => {
            let [x, y] = proj.tile(*p);
            if inside(x, y) {
                vec![(x as u32, y as u32)]
            } else {
                vec![]
            }
        }
        Geometry::Line(pts) => {
            let pts: Vec<[f64; 2]> = pts.iter().map(|p| proj.tile(*p)).collect();
            let mut out: Vec<(u32, u32)> = Vec::new();
            for seg in pts.windows(2) {
                for t in supercover(seg[0], seg[1]) {
                    if inside(t.0 as f64, t.1 as f64) && out.last() != Some(&(t.0 as u32, t.1 as u32)) {
                        out.push((t.0 as u32, t.1 as u32));
                    }
                }
            }
            if pts.len() == 1 && inside(pts[0][0], pts[0][1]) {
                out.push((pts[0][0] as u32, pts[0][1] as u32));
            }
            out
        }
        Geometry::Polygon(rings) => {
            let rings: Vec<Vec<[f64; 2]>> = rings.iter().map(|r| r.iter().map(|p| proj.tile(*p)).collect()).collect();
            let Some(outer) = rings.first() else { return vec![] };
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for p in outer {
                x0 = x0.min(p[0]);
                y0 = y0.min(p[1]);
                x1 = x1.max(p[0]);
                y1 = y1.max(p[1]);
            }
            let mut out = Vec::new();
            let (lmin, lmax) = (y0.floor().max(0.0) as u32, (y1.ceil().max(0.0) as u32).min(length));
            let (wmin, wmax) = (x0.floor().max(0.0) as u32, (x1.ceil().max(0.0) as u32).min(width));
            for l in lmin..lmax {
                for w in wmin..wmax {
                    let (cx, cy) = (w as f64 + 0.5, l as f64 + 0.5);
                    if rings.iter().filter(|r| point_in_ring(cx, cy, r)).count() % 2 == 1 {
                        out.push((w, l));
                    }
                }
            }
            if out.is_empty() {
                // Smaller than a tile: use the tile of its centre.
                let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                if inside(cx, cy) {
                    out.push((cx as u32, cy as u32));
                }
            }
            out
        }
    }
}

fn point_in_ring(x: f64, y: f64, ring: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + n - 1) % n]);
        if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
    }
    inside
}

/// 4-connected grid cells crossed by a segment (Amanatides & Woo).
fn supercover(a: [f64; 2], b: [f64; 2]) -> Vec<(i64, i64)> {
    let (mut x, mut y) = (a[0].floor() as i64, a[1].floor() as i64);
    let (ex, ey) = (b[0].floor() as i64, b[1].floor() as i64);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (sx, sy) = (dx.signum() as i64, dy.signum() as i64);
    let t_delta_x = if dx != 0.0 { (1.0 / dx).abs() } else { f64::INFINITY };
    let t_delta_y = if dy != 0.0 { (1.0 / dy).abs() } else { f64::INFINITY };
    let frac = |v: f64, s: i64| if s > 0 { v.floor() + 1.0 - v } else { v - v.floor() };
    let mut t_max_x = if dx != 0.0 { frac(a[0], sx) * t_delta_x } else { f64::INFINITY };
    let mut t_max_y = if dy != 0.0 { frac(a[1], sy) * t_delta_y } else { f64::INFINITY };
    let mut out = vec![(x, y)];
    let limit = ((ex - x).abs() + (ey - y).abs()) as usize + 2;
    while (x, y) != (ex, ey) && out.len() <= limit {
        if t_max_x < t_max_y {
            x += sx;
            t_max_x += t_delta_x;
        } else {
            y += sy;
            t_max_y += t_delta_y;
        }
        out.push((x, y));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(HistDate::parse("1920"), Some(HistDate { year: 1920, month: 1, day: 1 }));
        assert_eq!(HistDate::parse("1920-05-03"), Some(HistDate { year: 1920, month: 5, day: 3 }));
        assert!(HistDate::parse("-0500").unwrap() < HistDate::parse("0010").unwrap());
        assert_eq!(HistDate::parse("~1850..1860").unwrap().year, 1850);
        assert_eq!(HistDate::parse("unknown"), None);
    }

    #[test]
    fn supercover_is_4_connected() {
        let cells = supercover([0.5, 0.5], [3.5, 2.5]);
        assert_eq!(cells.first(), Some(&(0, 0)));
        assert_eq!(cells.last(), Some(&(3, 2)));
        for w in cells.windows(2) {
            assert_eq!((w[0].0 - w[1].0).abs() + (w[0].1 - w[1].1).abs(), 1);
        }
    }
}
