//! Model footprints: how many tiles each graphic code covers.
//!
//! The simulation needs this to grow zones into bigger buildings and to place
//! power plants and public services. The asset crate fills it from the model
//! catalog; tests can build one by hand.

use std::collections::HashMap;

use crate::GraphicCode;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    pub width: u32,
    pub length: u32,
    /// Approximate model height in tiles (informational).
    pub height: f32,
}

impl Default for Footprint {
    fn default() -> Self {
        Self { width: 1, length: 1, height: 1.0 }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Footprints {
    map: HashMap<GraphicCode, Footprint>,
}

impl Footprints {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, gc: GraphicCode, fp: Footprint) {
        self.map.insert(gc, fp);
    }

    /// Footprint of a model, defaulting to a single tile.
    pub fn get(&self, gc: GraphicCode) -> Footprint {
        self.map.get(&gc).copied().unwrap_or_default()
    }

    pub fn size(&self, gc: GraphicCode) -> (u32, u32) {
        let f = self.get(gc);
        (f.width.max(1), f.length.max(1))
    }

    /// Multi-tile footprints of the original OpenCity 0.0.7 asset pack (every
    /// other model is 1x1). Used by tests and when no asset directory is given.
    /// Generated from `assets/graphism/**/*.conf`.
    pub fn builtin() -> Self {
        let mut f = Self::new();
        for (gc, width, length, height) in [
            (1, 5, 1, 5.0),
            (16, 1, 2, 1.15),
            (18, 2, 2, 2.2),
            (19, 5, 1, 5.0),
            (24, 2, 1, 0.75),
            (25, 4, 4, 9.6),
            (26, 2, 2, 0.88),
            (27, 2, 2, 1.15),
            (28, 2, 2, 1.45),
            (29, 3, 3, 19.57),
            (38, 2, 2, 0.75),
            (39, 4, 2, 1.2),
            (300, 5, 3, 2.8),
            (303, 6, 6, 2.6),
            (350, 2, 2, 1.5),
            (360, 2, 2, 1.5),
            (370, 6, 3, 2.35),
            (390, 3, 3, 1.5),
            (395, 3, 2, 0.7),
            (396, 2, 2, 1.0),
            (380, 2, 2, 0.9),
            (381, 1, 1, 1.8),
            (382, 3, 3, 0.8),
            (383, 3, 3, 0.7),
            (384, 2, 2, 0.66),
            (385, 2, 2, 2.2),
            (397, 6, 3, 1.7),
            (398, 4, 3, 1.5),
        ] {
            f.insert(GraphicCode(gc), Footprint { width, length, height });
        }
        f
    }
}
