//! Roads drawn as smooth ribbons.
//!
//! The simulation keeps roads on tiles. For display, chains of road tiles
//! with one or two connections are turned into centre lines through the tile
//! centres, smoothed, and drawn as textured ribbons: L-turns become curves and
//! zig-zags (rasterised diagonals) become flowing diagonal streets. Junctions
//! keep the original crossing models, and ribbons end on their edges.
//!
//! Each kind of road has its own width and surface: chains stop where the
//! kind changes, and crossings of kinds without a model (highways, dirt and
//! paved streets) are drawn as flat patches.

use std::collections::HashSet;

use opencity_core::{City, Direction, RoadKind, StructureCode};

type P = [f32; 2];

/// One ribbon: centre points through tile centres, a closed loop or not.
#[derive(Debug)]
pub struct Line {
    pub points: Vec<P>,
    pub closed: bool,
    pub kind: RoadKind,
}

/// A crossing (or lone tile) of a kind without crossing models.
#[derive(Debug)]
pub struct Patch {
    pub tile: (u32, u32),
    pub mask: u8,
    pub kind: RoadKind,
}

/// Half the drawn width of a road kind, in tiles.
pub fn half_width(kind: RoadKind) -> f32 {
    kind.width() * 0.5
}

fn degree(mask: u8) -> u32 {
    mask.count_ones()
}

fn road_mask(city: &City, w: u32, l: u32) -> Option<u8> {
    road(city, w, l).map(|r| r.0)
}

fn road(city: &City, w: u32, l: u32) -> Option<(u8, RoadKind)> {
    city.layer.structure_at(w, l).filter(|s| s.code == StructureCode::Road).map(|s| (s.connections, s.road))
}

/// Whether the tile is drawn with the crossing models (streets that are not ribbons).
pub fn uses_model(mask: u8, kind: RoadKind) -> bool {
    kind == RoadKind::Street && !is_ribbon_tile(mask)
}

/// Crossings and lone tiles of kinds drawn as patches.
pub fn patches(city: &City) -> Vec<Patch> {
    city.layer
        .structures()
        .filter(|(_, s)| s.code == StructureCode::Road && s.road != RoadKind::Street && !is_ribbon_tile(s.connections))
        .map(|(tile, s)| Patch { tile, mask: s.connections, kind: s.road })
        .collect()
}

/// Whether the tile is drawn as part of a ribbon (not a junction).
pub fn is_ribbon_tile(mask: u8) -> bool {
    matches!(degree(mask), 1 | 2)
}

fn edge_mid(w: u32, l: u32, d: Direction) -> P {
    let (dx, dl) = d.delta();
    [w as f32 + 0.5 + dx as f32 * 0.5, l as f32 + 0.5 + dl as f32 * 0.5]
}

fn centre(w: u32, l: u32) -> P {
    [w as f32 + 0.5, l as f32 + 0.5]
}

/// Centre lines of every ribbon.
pub fn centre_lines(city: &City) -> Vec<Line> {
    let mut seen: HashSet<(u32, u32)> = HashSet::new();
    let mut lines = Vec::new();
    let tiles: Vec<((u32, u32), u8, RoadKind)> =
        city.layer.structures().filter(|(_, s)| s.code == StructureCode::Road).map(|(p, s)| (p, s.connections, s.road)).collect();
    let neighbour = |w: u32, l: u32, d: Direction| city.terrain.neighbour(w, l, d);
    // A neighbour the chain cannot run into: a crossing, or another kind.
    let stops = |p: (u32, u32), kind: RoadKind| road(city, p.0, p.1).is_some_and(|(m, k)| !is_ribbon_tile(m) || k != kind);
    // Chain starts first (dead ends and tiles touching a junction), then loops.
    let mut order: Vec<((u32, u32), u8, RoadKind)> = tiles.iter().copied().filter(|&(_, m, _)| is_ribbon_tile(m)).collect();
    order.sort_by_key(|&((w, l), m, k)| {
        let touches_junction = Direction::ALL.iter().any(|d| m & d.mask() != 0 && neighbour(w, l, *d).is_some_and(|n| stops(n, k)));
        !(degree(m) == 1 || touches_junction)
    });
    for ((w, l), mask, kind) in order {
        if seen.contains(&(w, l)) {
            continue;
        }
        // Walk forward from (w, l), first in one direction, then prepend the other.
        let dirs: Vec<Direction> = Direction::ALL.into_iter().filter(|d| mask & d.mask() != 0).collect();
        let mut forward = walk(city, (w, l), kind, dirs.first().copied(), &mut seen);
        let backward = match dirs.get(1) {
            Some(d) if !forward.closed => walk(city, (w, l), kind, Some(*d), &mut seen),
            _ => Walk::default(),
        };
        let mut pts: Vec<P> = backward.points.into_iter().rev().collect();
        if !pts.is_empty() {
            pts.pop(); // the shared start centre
        }
        pts.append(&mut forward.points);
        if pts.len() >= 2 {
            lines.push(Line { points: pts, closed: forward.closed, kind });
        }
    }
    lines
}

#[derive(Default)]
struct Walk {
    points: Vec<P>,
    closed: bool,
}

/// Follows ribbon tiles of `kind` from `start` leaving in `first`; ends at
/// an edge midpoint when it meets a junction or another kind.
fn walk(city: &City, start: (u32, u32), kind: RoadKind, first: Option<Direction>, seen: &mut HashSet<(u32, u32)>) -> Walk {
    let mut out = Walk::default();
    let mut cur = start;
    let mut from: Option<Direction> = None;
    let mut next_dir = first;
    loop {
        seen.insert(cur);
        out.points.push(centre(cur.0, cur.1));
        let Some(mask) = road_mask(city, cur.0, cur.1) else { break };
        let dir = match next_dir.take() {
            Some(d) => Some(d),
            None => Direction::ALL.into_iter().find(|d| mask & d.mask() != 0 && Some(*d) != from.map(Direction::opposite)),
        };
        let Some(dir) = dir else { break };
        let Some(n) = city.terrain.neighbour(cur.0, cur.1, dir) else { break };
        match road(city, n.0, n.1) {
            Some((m, k)) if is_ribbon_tile(m) && k == kind => {
                if n == start {
                    out.closed = true;
                    break;
                }
                if seen.contains(&n) {
                    out.points.push(edge_mid(cur.0, cur.1, dir));
                    break;
                }
                from = Some(dir);
                cur = n;
            }
            Some(_) => {
                // Junction: stop on the shared edge.
                out.points.push(edge_mid(cur.0, cur.1, dir));
                break;
            }
            None => break,
        }
    }
    out
}

/// Laplacian smoothing then Chaikin corner cutting; open lines keep their ends.
pub fn smooth(points: &[P], closed: bool) -> Vec<P> {
    let mut p = points.to_vec();
    let n = p.len();
    if n < 3 {
        return p;
    }
    for _ in 0..3 {
        let q = p.clone();
        for i in 0..n {
            let (a, b) = if closed {
                ((i + n - 1) % n, (i + 1) % n)
            } else if i == 0 || i == n - 1 {
                continue;
            } else {
                (i - 1, i + 1)
            };
            p[i] = [0.25 * q[a][0] + 0.5 * q[i][0] + 0.25 * q[b][0], 0.25 * q[a][1] + 0.5 * q[i][1] + 0.25 * q[b][1]];
        }
    }
    for _ in 0..2 {
        let m = p.len();
        let mut out = Vec::with_capacity(m * 2);
        if !closed {
            out.push(p[0]);
        }
        let segs = if closed { m } else { m - 1 };
        for i in 0..segs {
            let (a, b) = (p[i], p[(i + 1) % m]);
            out.push([0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]]);
            out.push([0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]]);
        }
        if !closed {
            out.push(p[m - 1]);
        }
        p = out;
    }
    p
}

/// Side of the generated road surfaces, in texels.
pub const SURFACE_SIZE: u32 = 64;

/// Cheap deterministic noise in 0..1.
fn noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

/// Surface texture of a road kind, RGBA, `SURFACE_SIZE` square. `u` (x) runs
/// across the road, `v` (y) along it and repeats every tile. Streets use the
/// original OpenCity texture instead (`None`).
pub fn surface(kind: RoadKind) -> Option<Vec<u8>> {
    let n = SURFACE_SIZE;
    let mut out = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = ((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32);
            let grain = noise(x, y, kind as u32) - 0.5;
            let rgb: [f32; 3] = match kind {
                RoadKind::Street => return None,
                RoadKind::Highway => {
                    let asphalt = 0.30 + grain * 0.06;
                    let mut c = [asphalt, asphalt + 0.005, asphalt + 0.015];
                    let edge = (0.05..0.08).contains(&u) || (0.92..0.95).contains(&u);
                    let lane = ((0.27..0.29).contains(&u) || (0.71..0.73).contains(&u)) && v < 0.5;
                    let median = (0.475..0.49).contains(&u) || (0.51..0.525).contains(&u);
                    if edge || lane {
                        c = [0.92, 0.92, 0.9];
                    } else if median {
                        c = [0.9, 0.75, 0.2];
                    } else if !(0.03..0.97).contains(&u) {
                        c = [0.55, 0.56, 0.58]; // guard rail
                    }
                    c
                }
                RoadKind::DirtRoad => {
                    let rut = ((u - 0.3).abs() < 0.06 || (u - 0.7).abs() < 0.06) as u8 as f32;
                    let verge = ((0.12 - u).max(u - 0.88).max(0.0) / 0.12).min(1.0);
                    let earth = [0.56 - rut * 0.08, 0.43 - rut * 0.07, 0.29 - rut * 0.05];
                    let grass = [0.42, 0.45, 0.24];
                    let pebble = if noise(x / 2, y / 2, 7) > 0.93 { 0.08 } else { 0.0 };
                    [0, 1, 2].map(|i| earth[i] * (1.0 - verge) + grass[i] * verge + grain * 0.08 + pebble)
                }
                RoadKind::Rail => {
                    // Ballast, wooden sleepers across, two steel rails along.
                    let rail = (u - 0.32).abs() < 0.035 || (u - 0.68).abs() < 0.035;
                    let sleeper = (v * 8.0).fract() < 0.38 && (0.16..0.84).contains(&u);
                    if rail {
                        let k = 0.72 + grain * 0.05;
                        [k, k, k + 0.04]
                    } else if sleeper {
                        [0.36 + grain * 0.05, 0.25 + grain * 0.04, 0.16]
                    } else {
                        let stone = 0.47 + grain * 0.12 + noise(x / 2, y / 2, 13) * 0.08;
                        [stone, stone - 0.02, stone - 0.05]
                    }
                }
                RoadKind::PavedStreet => {
                    // Kerbs, then rows of cobbles with mortar joints, every other row offset.
                    if !(0.07..0.93).contains(&u) {
                        let k = 0.68 + grain * 0.04;
                        [k, k, k - 0.02]
                    } else {
                        let rows = 8.0;
                        let row = (v * rows).floor();
                        let cols = 6.0;
                        let shift = if row as u32 % 2 == 0 { 0.0 } else { 0.5 };
                        let cu = (u - 0.07) / 0.86 * cols + shift;
                        let (fu, fv) = (cu.fract(), (v * rows).fract());
                        let joint = fu < 0.1 || fv < 0.12;
                        let stone = noise(cu.floor() as u32, row as u32, 11) * 0.12;
                        if joint {
                            [0.33, 0.31, 0.28]
                        } else {
                            let base = 0.52 + stone + grain * 0.05;
                            [base + 0.03, base + 0.01, base - 0.03]
                        }
                    }
                }
            };
            out.extend(rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8));
            out.push(255);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencity_core::{Footprints, Terrain, Tool};

    #[test]
    fn chains_stop_at_junctions_and_smoothing_keeps_ends() {
        let mut c = City::new(Terrain::flat(20, 20), None, Footprints::builtin(), 1);
        c.apply_tool(Tool::Road, (2, 5), (10, 5)).unwrap(); // straight
        c.apply_tool(Tool::Road, (6, 5), (6, 12)).unwrap(); // T junction at (6,5)
        let lines = centre_lines(&c);
        assert_eq!(lines.len(), 3, "{lines:?}");
        for line in &lines {
            assert!(!line.closed);
            let s = smooth(&line.points, false);
            assert_eq!(s.first(), line.points.first());
            assert_eq!(s.last(), line.points.last());
        }
        assert!(lines.iter().any(|l| [[6.5, 5.0], [6.0, 5.5], [7.0, 5.5], [6.5, 6.0]].iter().any(|p| l.points.contains(p))));
        assert!(patches(&c).is_empty(), "street crossings use models");
    }

    #[test]
    fn surfaces_cover_every_kind_but_streets() {
        assert!(surface(RoadKind::Street).is_none());
        for k in [RoadKind::Highway, RoadKind::DirtRoad, RoadKind::PavedStreet, RoadKind::Rail] {
            assert_eq!(surface(k).unwrap().len(), (SURFACE_SIZE * SURFACE_SIZE * 4) as usize);
        }
    }

    #[test]
    fn chains_stop_where_the_kind_changes() {
        let mut c = City::new(Terrain::flat(20, 20), None, Footprints::builtin(), 1);
        c.apply_tool(Tool::DirtRoad, (2, 5), (8, 5)).unwrap();
        c.apply_tool(Tool::PavedStreet, (9, 5), (14, 5)).unwrap();
        c.apply_tool(Tool::PavedStreet, (11, 5), (11, 9)).unwrap(); // paved T crossing
        let lines = centre_lines(&c);
        let kinds: Vec<RoadKind> = lines.iter().map(|l| l.kind).collect();
        assert!(kinds.contains(&RoadKind::DirtRoad) && kinds.contains(&RoadKind::PavedStreet), "{lines:?}");
        // The dirt road ends on the edge it shares with the paved street.
        let dirt = lines.iter().find(|l| l.kind == RoadKind::DirtRoad).unwrap();
        assert!(dirt.points.contains(&[9.0, 5.5]), "{:?}", dirt.points);
        let p = patches(&c);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].tile, p[0].kind), ((11, 5), RoadKind::PavedStreet));
    }
}
