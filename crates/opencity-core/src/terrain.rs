//! Height field of the city (legacy `Map`).
//!
//! Heights live on the *corners* of the tiles, so a `width x length` city has
//! `(width + 1) x (length + 1)` height samples.

use serde::{Deserialize, Serialize};

pub const HEIGHT_MAX: i8 = 10;
pub const HEIGHT_MIN: i8 = -10;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Terrain {
    width: u32,
    length: u32,
    /// Row-major corner heights, `(width+1) * (length+1)`.
    heights: Vec<i8>,
}

/// The four corner heights of a tile, in legacy order:
/// `[ (w,l), (w,l+1), (w+1,l+1), (w+1,l) ]`.
pub type Corners = [i8; 4];

impl Terrain {
    pub fn flat(width: u32, length: u32) -> Self {
        Self { width, length, heights: vec![0; ((width + 1) * (length + 1)) as usize] }
    }

    /// Builds a terrain from corner heights (`(width+1) * (length+1)` values,
    /// row-major). Values are clamped to the playable range of an `i8`.
    pub fn from_corner_heights(width: u32, length: u32, heights: &[i32]) -> Self {
        assert_eq!(heights.len(), ((width + 1) * (length + 1)) as usize, "height grid size");
        let heights = heights.iter().map(|&h| h.clamp(i8::MIN as i32, i8::MAX as i32) as i8).collect();
        Self { width, length, heights }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn length(&self) -> u32 {
        self.length
    }

    pub fn corner_heights(&self) -> &[i8] {
        &self.heights
    }

    #[inline]
    fn idx(&self, w: u32, l: u32) -> usize {
        (l * (self.width + 1) + w) as usize
    }

    pub fn corner(&self, w: u32, l: u32) -> i8 {
        self.heights[self.idx(w, l)]
    }

    pub fn tile_corners(&self, w: u32, l: u32) -> Corners {
        [self.corner(w, l), self.corner(w, l + 1), self.corner(w + 1, l + 1), self.corner(w + 1, l)]
    }

    pub fn tile_min(&self, w: u32, l: u32) -> i8 {
        *self.tile_corners(w, l).iter().min().unwrap()
    }

    pub fn tile_max(&self, w: u32, l: u32) -> i8 {
        *self.tile_corners(w, l).iter().max().unwrap()
    }

    /// Whether the tile lies under water (below the sea level, height 0).
    pub fn is_water(&self, w: u32, l: u32) -> bool {
        self.tile_max(w, l) < 0
    }

    pub fn is_tile_flat(&self, w: u32, l: u32) -> bool {
        let c = self.tile_corners(w, l);
        c.iter().all(|&h| h == c[0])
    }

    /// Every tile in the inclusive rectangle is flat (heights may differ between tiles).
    pub fn is_area_flat(&self, w1: u32, l1: u32, w2: u32, l2: u32) -> bool {
        (l1..=l2).all(|l| (w1..=w2).all(|w| self.is_tile_flat(w, l)))
    }

    /// Whether a road or power line may be laid on this tile: the slope must be
    /// at most one step and form a ramp (an even number of low corners).
    pub fn is_tile_path_friendly(&self, w: u32, l: u32) -> bool {
        let c = self.tile_corners(w, l);
        let (min, max) = (*c.iter().min().unwrap(), *c.iter().max().unwrap());
        if max - min > 1 {
            return false;
        }
        c.iter().filter(|&&h| h == min).count() % 2 == 0
    }

    /// Raises (`delta > 0`) or lowers the tile: the lowest (resp. highest)
    /// corners move by one step. Returns false at the height limits.
    pub fn change_tile_height(&mut self, w: u32, l: u32, raise: bool) -> bool {
        let mut c = self.tile_corners(w, l);
        let (reference, step) = if raise {
            let r = *c.iter().min().unwrap();
            if r >= HEIGHT_MAX {
                return false;
            }
            (r, 1)
        } else {
            let r = *c.iter().max().unwrap();
            if r <= HEIGHT_MIN {
                return false;
            }
            (r, -1)
        };
        for h in c.iter_mut().filter(|h| **h == reference) {
            *h += step;
        }
        let (i0, i1) = (self.idx(w, l), self.idx(w, l + 1));
        self.heights[i0] = c[0];
        self.heights[i0 + 1] = c[3];
        self.heights[i1] = c[1];
        self.heights[i1 + 1] = c[2];
        true
    }

    /// Sets every corner of the tiles in the inclusive rectangle to `height`
    /// (scenario and importer helper; the player uses [`Self::change_tile_height`]).
    pub fn level_area(&mut self, w1: u32, l1: u32, w2: u32, l2: u32, height: i8) {
        for l in l1..=(l2 + 1).min(self.length) {
            for w in w1..=(w2 + 1).min(self.width) {
                let i = self.idx(w, l);
                self.heights[i] = height;
            }
        }
    }

    /// Sets one corner height, clamped to the playable range.
    pub fn set_corner(&mut self, w: u32, l: u32, height: i32) {
        let i = self.idx(w, l);
        self.heights[i] = height.clamp(HEIGHT_MIN as i32, HEIGHT_MAX as i32) as i8;
    }

    /// Clamps `(w + dw, l + dl)` to the tile grid (legacy `GetPossibleWH`).
    pub fn clamp_offset(&self, w: u32, l: u32, dw: i32, dl: i32) -> (u32, u32) {
        let nw = (w as i64 + dw as i64).clamp(0, self.width as i64 - 1);
        let nl = (l as i64 + dl as i64).clamp(0, self.length as i64 - 1);
        (nw as u32, nl as u32)
    }

    /// Inclusive square of tiles around `(w, l)` with the given radius, clamped to the map.
    pub fn around(&self, w: u32, l: u32, radius: u32) -> (u32, u32, u32, u32) {
        let r = radius as i32;
        let (w1, l1) = self.clamp_offset(w, l, -r, -r);
        let (w2, l2) = self.clamp_offset(w, l, r, r);
        (w1, l1, w2, l2)
    }

    pub fn neighbour(&self, w: u32, l: u32, dir: crate::Direction) -> Option<(u32, u32)> {
        let (dw, dl) = dir.delta();
        let nw = w as i64 + dw as i64;
        let nl = l as i64 + dl as i64;
        (nw >= 0 && nl >= 0 && nw < self.width as i64 && nl < self.length as i64).then_some((nw as u32, nl as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raise_then_lower_is_identity() {
        let mut t = Terrain::flat(4, 4);
        assert!(t.change_tile_height(1, 1, true));
        assert_eq!(t.tile_corners(1, 1), [1, 1, 1, 1]);
        assert!(!t.is_tile_flat(0, 0));
        assert!(t.is_tile_path_friendly(0, 1)); // ramp: two low, two high corners
        assert!(!t.is_tile_path_friendly(0, 0)); // one high corner only
        assert!(t.change_tile_height(1, 1, false));
        assert_eq!(t, Terrain::flat(4, 4));
    }

    #[test]
    fn height_limits() {
        let mut t = Terrain::flat(1, 1);
        for _ in 0..HEIGHT_MAX {
            assert!(t.change_tile_height(0, 0, true));
        }
        assert!(!t.change_tile_height(0, 0, true));
    }
}
