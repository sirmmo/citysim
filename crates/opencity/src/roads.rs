//! Roads drawn as smooth ribbons.
//!
//! The simulation keeps roads on tiles. For display, chains of road tiles
//! with one or two connections are turned into centre lines through the tile
//! centres, smoothed, and drawn as textured ribbons: L-turns become curves and
//! zig-zags (rasterised diagonals) become flowing diagonal streets. Junctions
//! keep the original crossing models, and ribbons end on their edges.

use std::collections::HashSet;

use opencity_core::{City, Direction, StructureCode};

type P = [f32; 2];

fn degree(mask: u8) -> u32 {
    mask.count_ones()
}

fn road_mask(city: &City, w: u32, l: u32) -> Option<u8> {
    city.layer.structure_at(w, l).filter(|s| s.code == StructureCode::Road).map(|s| s.connections)
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

/// Centre lines of every ribbon, as (points, closed loop?).
pub fn centre_lines(city: &City) -> Vec<(Vec<P>, bool)> {
    let mut seen: HashSet<(u32, u32)> = HashSet::new();
    let mut lines = Vec::new();
    let tiles: Vec<((u32, u32), u8)> = city.layer.structures().filter(|(_, s)| s.code == StructureCode::Road).map(|(p, s)| (p, s.connections)).collect();
    let neighbour = |w: u32, l: u32, d: Direction| city.terrain.neighbour(w, l, d);
    let is_ribbon = |p: (u32, u32)| road_mask(city, p.0, p.1).is_some_and(is_ribbon_tile);
    // Chain starts first (dead ends and tiles touching a junction), then loops.
    let mut order: Vec<((u32, u32), u8)> = tiles.iter().copied().filter(|&(_, m)| is_ribbon_tile(m)).collect();
    order.sort_by_key(|&((w, l), m)| {
        let touches_junction = Direction::ALL.iter().any(|d| m & d.mask() != 0 && neighbour(w, l, *d).is_some_and(|n| !is_ribbon(n)));
        !(degree(m) == 1 || touches_junction)
    });
    for ((w, l), mask) in order {
        if seen.contains(&(w, l)) {
            continue;
        }
        // Walk forward from (w, l), first in one direction, then prepend the other.
        let dirs: Vec<Direction> = Direction::ALL.into_iter().filter(|d| mask & d.mask() != 0).collect();
        let mut forward = walk(city, (w, l), dirs.first().copied(), &mut seen);
        let backward = match dirs.get(1) {
            Some(d) if !forward.closed => walk(city, (w, l), Some(*d), &mut seen),
            _ => Walk::default(),
        };
        let mut pts: Vec<P> = backward.points.into_iter().rev().collect();
        if !pts.is_empty() {
            pts.pop(); // the shared start centre
        }
        pts.append(&mut forward.points);
        if pts.len() >= 2 {
            lines.push((pts, forward.closed));
        }
    }
    lines
}

#[derive(Default)]
struct Walk {
    points: Vec<P>,
    closed: bool,
}

/// Follows ribbon tiles from `start` leaving in `first`; ends at an edge
/// midpoint when it meets a junction.
fn walk(city: &City, start: (u32, u32), first: Option<Direction>, seen: &mut HashSet<(u32, u32)>) -> Walk {
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
        match road_mask(city, n.0, n.1) {
            Some(m) if is_ribbon_tile(m) => {
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
        for (pts, closed) in &lines {
            assert!(!closed);
            let s = smooth(pts, false);
            assert_eq!(s.first(), pts.first());
            assert_eq!(s.last(), pts.last());
        }
        assert!(lines.iter().any(|(p, _)| p.contains(&[6.5, 5.0]) || p.contains(&[6.0, 5.5]) || p.contains(&[7.0, 5.5]) || p.contains(&[6.5, 6.0])));
    }
}
