//! Terrain generation for OpenCity (legacy `src/mapgen`).
//!
//! A diamond-square noise generator followed by a pipeline of filters
//! (normalise, flatten, island/volcano shapers, Gaussian blur), or a
//! greyscale PNG heightmap. Also produces the initial tree density.

use std::path::{Path, PathBuf};

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg32;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Relief {
    #[default]
    Plain,
    Hill,
    Mountain,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Water {
    Dry,
    #[default]
    Lake,
    Coast,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    #[default]
    None,
    Island,
    Volcano,
    Crater,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TreeDensity {
    #[default]
    Sparse,
    Normal,
    Dense,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapParams {
    pub width: u32,
    pub length: u32,
    #[serde(default)]
    pub relief: Relief,
    #[serde(default)]
    pub water: Water,
    #[serde(default)]
    pub shape: Shape,
    #[serde(default)]
    pub trees: TreeDensity,
    #[serde(default)]
    pub seed: u64,
    /// Greyscale PNG used instead of the generator (mid-grey = sea level).
    #[serde(default)]
    pub heightmap: Option<PathBuf>,
}

impl Default for MapParams {
    fn default() -> Self {
        Self {
            width: 50,
            length: 50,
            relief: Relief::default(),
            water: Water::default(),
            shape: Shape::default(),
            trees: TreeDensity::default(),
            seed: 0,
            heightmap: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MapGenError {
    #[error("cannot read heightmap {path}: {source}")]
    Heightmap { path: PathBuf, source: image::ImageError },
    #[error("the map must be at least 2x2 tiles")]
    TooSmall,
}

/// Generated terrain, ready for `opencity_core::Terrain::from_corner_heights`.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedMap {
    pub width: u32,
    pub length: u32,
    /// `(width + 1) * (length + 1)` corner heights, row-major.
    pub corner_heights: Vec<i32>,
    /// `width * length` values; a tree grows where the value is positive.
    pub tree_density: Vec<i32>,
}

pub fn generate(p: &MapParams) -> Result<GeneratedMap, MapGenError> {
    if p.width < 2 || p.length < 2 {
        return Err(MapGenError::TooSmall);
    }
    let mut rng = Pcg32::seed_from_u64(p.seed);
    let map_seed: u64 = rng.r#gen();
    let tree_seed: u64 = rng.r#gen();
    let (cw, cl) = (p.width + 1, p.length + 1);
    let terrain = match &p.heightmap {
        Some(path) => load_heightmap(path)?.crop(cw, cl),
        None => generate_terrain(p, map_seed),
    };
    let trees = generate_trees(p, tree_seed, &terrain);
    Ok(GeneratedMap { width: p.width, length: p.length, corner_heights: terrain.to_ints(), tree_density: trees.to_ints() })
}

fn generate_terrain(p: &MapParams, seed: u64) -> Grid {
    let (cw, cl) = (p.width + 1, p.length + 1);
    let mut map = diamond_square(seed, cw.max(cl)).crop(cw, cl);
    let water_level = match p.water {
        Water::Dry => 0.0,
        Water::Lake => 1.0,
        Water::Coast => 2.0,
    };
    let (min, max, flatten) = match p.relief {
        Relief::Plain => (-3.0 * water_level, 5.0, 3.0),
        Relief::Hill => (-5.0 * water_level, 10.0, 2.0),
        Relief::Mountain => (-10.0 * water_level, 20.0, 1.0),
    };
    map.normalize(min, max);
    map.flatten(flatten);
    let (w, l) = (p.width, p.length);
    match p.shape {
        Shape::Island => {
            map.add_shape(|x, y| bubble(w, l, x, y, 0.0, 3.0, -min / 2.0));
            map.chop_shape(|x, y| top_round(w, l, x, y, 1.0 - 4.0 / max / max, 5.0, max));
            map.chop_shape(|x, y| bubble(w, l, x, y, 0.5, 1.0, max));
        }
        Shape::Volcano => map.add_shape(|x, y| volcano(w, l, x, y, 0.0, 0.3, max)),
        Shape::None | Shape::Crater => {}
    }
    map.gauss_blur(2);
    map
}

fn generate_trees(p: &MapParams, seed: u64, terrain: &Grid) -> Grid {
    let mut trees = diamond_square(seed, p.width.max(p.length)).crop(p.width, p.length);
    trees.flatten(1.0);
    trees.gauss_blur(5);
    match p.trees {
        TreeDensity::Sparse => trees.normalize(-15.0, 5.0),
        TreeDensity::Normal => trees.normalize(-5.0, 5.0),
        TreeDensity::Dense => trees.normalize(0.0, 5.0),
    }
    // No trees under water: use the mean height of each tile's corners.
    let ground = terrain.half_shift();
    for y in 0..trees.h {
        for x in 0..trees.w {
            let v = trees.get(x as i64, y as i64);
            let keep = ground.get(x as i64, y as i64).trunc() >= 0.0 && v >= 0.0;
            trees.set(x, y, if keep { v } else { 0.0 });
        }
    }
    trees
}

fn load_heightmap(path: &Path) -> Result<Grid, MapGenError> {
    let img = image::open(path).map_err(|source| MapGenError::Heightmap { path: path.to_owned(), source })?.to_rgb8();
    let mut g = Grid::new(img.width(), img.height());
    for (x, y, px) in img.enumerate_pixels() {
        let grey = (px[0] as u32 + px[1] as u32 + px[2] as u32) / 3;
        g.set(x, y, grey as f32 - 128.0);
    }
    Ok(g)
}

/// A scalar field with clamped reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub w: u32,
    pub h: u32,
    data: Vec<f32>,
}

impl Grid {
    pub fn new(w: u32, h: u32) -> Self {
        Self { w, h, data: vec![0.0; (w * h) as usize] }
    }

    pub fn get(&self, x: i64, y: i64) -> f32 {
        let x = x.clamp(0, self.w as i64 - 1) as u32;
        let y = y.clamp(0, self.h as i64 - 1) as u32;
        self.data[(y * self.w + x) as usize]
    }

    pub fn set(&mut self, x: u32, y: u32, v: f32) {
        self.data[(y * self.w + x) as usize] = v;
    }

    fn min_max(&self) -> (f32, f32) {
        self.data.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)))
    }

    /// Centre crop (or clamped extension if the source is smaller).
    pub fn crop(&self, w: u32, h: u32) -> Grid {
        let dx = self.w as i64 / 2 - w as i64 / 2;
        let dy = self.h as i64 / 2 - h as i64 / 2;
        let mut g = Grid::new(w, h);
        for y in 0..h {
            for x in 0..w {
                g.set(x, y, self.get(x as i64 + dx, y as i64 + dy));
            }
        }
        g
    }

    /// Average of each 2x2 block: corner heights -> tile heights.
    pub fn half_shift(&self) -> Grid {
        let mut g = Grid::new(self.w - 1, self.h - 1);
        for y in 0..g.h {
            for x in 0..g.w {
                let (x, y) = (x as i64, y as i64);
                let v = (self.get(x, y) + self.get(x + 1, y) + self.get(x, y + 1) + self.get(x + 1, y + 1)) / 4.0;
                g.set(x as u32, y as u32, v);
            }
        }
        g
    }

    /// Truncates towards zero, like the legacy `(int)` cast.
    pub fn to_ints(&self) -> Vec<i32> {
        self.data.iter().map(|&v| v as i32).collect()
    }

    pub fn normalize(&mut self, to_min: f32, to_max: f32) {
        let (min, max) = self.min_max();
        let range = max - min;
        let a = if range.abs() > f32::EPSILON { (to_max - to_min) / range } else { 0.0 };
        let b = -min * a + to_min;
        self.data.iter_mut().for_each(|v| *v = a * *v + b);
    }

    /// Sharpens (`power > 1`) the relief: lowlands get flatter.
    pub fn flatten(&mut self, power: f32) {
        let (min, max) = self.min_max();
        let coef = (max - min).abs();
        if coef <= f32::EPSILON {
            return;
        }
        for v in &mut self.data {
            *v = if min >= 0.0 {
                ((*v - min) / coef).powf(power) * coef + min
            } else if *v >= 0.0 {
                if max > 0.0 { (*v / max).powf(power) * max } else { *v }
            } else {
                (v.abs() / min).powf(power).abs() * min
            };
        }
    }

    pub fn gauss_blur(&mut self, radius: u32) {
        let r = radius as i64;
        let mut weights: Vec<f32> = (-r..=r).map(|x| (-1.0 / radius as f32 * (x * x) as f32).exp()).collect();
        let sum: f32 = weights.iter().sum();
        weights.iter_mut().for_each(|w| *w /= sum);
        let mut tmp = Grid::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                let s = weights.iter().enumerate().map(|(i, w)| w * self.get(x as i64 + i as i64 - r, y as i64)).sum();
                tmp.set(x, y, s);
            }
        }
        for y in 0..self.h {
            for x in 0..self.w {
                let s = weights.iter().enumerate().map(|(i, w)| w * tmp.get(x as i64, y as i64 + i as i64 - r)).sum();
                self.set(x, y, s);
            }
        }
    }

    fn add_shape(&mut self, f: impl Fn(u32, u32) -> f32) {
        for y in 0..self.h {
            for x in 0..self.w {
                let v = self.get(x as i64, y as i64) + f(x, y);
                self.set(x, y, v);
            }
        }
    }

    fn chop_shape(&mut self, f: impl Fn(u32, u32) -> f32) {
        for y in 0..self.h {
            for x in 0..self.w {
                let v = self.get(x as i64, y as i64).min(f(x, y));
                self.set(x, y, v);
            }
        }
    }
}

/// Diamond-square fractal noise on a wrapping `2^n` square covering `min_side`.
pub fn diamond_square(seed: u64, min_side: u32) -> Grid {
    let side = min_side.max(2).next_power_of_two();
    let mut rng = Pcg32::seed_from_u64(seed);
    let mut g = Grid::new(side, side);
    g.set(0, 0, 128.0);
    let at = |g: &Grid, x: i64, y: i64| g.get(x.rem_euclid(side as i64), y.rem_euclid(side as i64));
    let mut p = side / 2;
    while p != 0 {
        let pi = p as i64;
        let mut jitter = || rng.gen_range(-(p as f32)..=p as f32);
        for i in (p..side).step_by(2 * p as usize) {
            for j in (p..side).step_by(2 * p as usize) {
                let (x, y) = (i as i64, j as i64);
                let v = (at(&g, x - pi, y - pi) + at(&g, x + pi, y - pi) + at(&g, x + pi, y + pi) + at(&g, x - pi, y + pi)) / 4.0 + jitter();
                g.set(i, j, v);
            }
        }
        for (i0, j0) in [(p, 0), (0, p)] {
            for i in (i0..side).step_by(2 * p as usize) {
                for j in (j0..side).step_by(2 * p as usize) {
                    let (x, y) = (i as i64, j as i64);
                    let v = (at(&g, x - pi, y) + at(&g, x + pi, y) + at(&g, x, y - pi) + at(&g, x, y + pi)) / 4.0 + jitter();
                    g.set(i, j, v);
                }
            }
        }
        p /= 2;
    }
    g
}

fn centre(w: u32, l: u32, x: u32, y: u32) -> (f32, f32, f32) {
    let (w2, h2) = (w / 2, l / 2);
    ((x as i64 - w2 as i64) as f32, (y as i64 - h2 as i64) as f32, w2.max(h2) as f32)
}

fn bubble(w: u32, l: u32, x: u32, y: u32, base: f32, rude: f32, level: f32) -> f32 {
    let (dx, dy, c) = centre(w, l, x, y);
    (-((dx * dx + dy * dy).sqrt() / c * rude).exp() / (2f32.sqrt() * rude).exp() + base) * level
}

fn top_round(w: u32, l: u32, x: u32, y: u32, base: f32, rude: f32, level: f32) -> f32 {
    let (dx, dy, c) = centre(w, l, x, y);
    (-(dx.abs().max(dy.abs()) / c * rude).exp() / rude.exp() + base) * level
}

fn volcano(w: u32, l: u32, x: u32, y: u32, base: f32, rude: f32, level: f32) -> f32 {
    let (dx, dy, c) = centre(w, l, x, y);
    let cc = c * c * 4.0 / 100.0;
    let t = (dx * dx + dy * dy) / cc * rude;
    (t * (-t).exp() + base) * 2.6 * level
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_sized() {
        let p = MapParams { width: 30, length: 20, seed: 9, ..Default::default() };
        let a = generate(&p).unwrap();
        assert_eq!(a.corner_heights.len(), 31 * 21);
        assert_eq!(a.tree_density.len(), 30 * 20);
        assert_eq!(a, generate(&p).unwrap());
        assert!(a.corner_heights.iter().all(|h| (-3..=5).contains(h)));
    }

    #[test]
    fn every_relief_and_shape_stays_in_range() {
        for relief in [Relief::Plain, Relief::Hill, Relief::Mountain] {
            for shape in [Shape::None, Shape::Island, Shape::Volcano] {
                let p = MapParams { width: 40, length: 40, relief, shape, water: Water::Coast, seed: 1, ..Default::default() };
                let m = generate(&p).unwrap();
                assert!(m.corner_heights.iter().all(|h| (-127..=127).contains(h)), "{relief:?} {shape:?}");
            }
        }
    }
}
