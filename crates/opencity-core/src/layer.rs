//! The building layer: what stands on every tile (legacy `BuildingLayer`).
//!
//! The legacy code kept a grid of raw `Structure*` where every tile of a
//! multi-tile building pointed to a separate "part" object holding a pointer
//! to the main one. Here each tile owns either the [`Structure`] itself
//! (at the building's origin, its north-west corner) or a [`TileKind::Part`]
//! that records the origin coordinates. No pointers, no manual memory.

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{codes::path_graphic, Direction, Footprints, GraphicCode, StructureCode, Terrain};

/// Highest level a zone can reach (legacy `OC_STRUCTURE_LEVEL_MAX`).
pub const LEVEL_MAX: u8 = 99;
/// Levels per graphic step (legacy `OC_STRUCTURE_LEVEL_RATIO`).
pub const LEVEL_RATIO: u8 = 10;
/// Largest model side, in tiles, the layer supports.
pub const MAX_FOOTPRINT: u32 = 16;

/// Per-tile service flags (legacy `OC_STRUCTURE_R/C/I/W/E/G/P`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Status(pub u8);

impl Status {
    pub const RESIDENTIAL: u8 = 0x01;
    pub const COMMERCIAL: u8 = 0x02;
    pub const INDUSTRIAL: u8 = 0x04;
    pub const WATER: u8 = 0x08;
    pub const POWER: u8 = 0x10;
    pub const GAS: u8 = 0x20;
    pub const ROAD: u8 = 0x40;

    pub fn set(&mut self, bits: u8) {
        self.0 |= bits;
    }
    pub fn unset(&mut self, bits: u8) {
        self.0 &= !bits;
    }
    /// All the given bits are set.
    pub fn has(self, bits: u8) -> bool {
        self.0 & bits == bits
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Structure {
    pub code: StructureCode,
    pub graphic: GraphicCode,
    pub level: u8,
    /// Road / power line connections (see [`crate::codes::connect`]).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub connections: u8,
    /// Road traffic load computed by the traffic simulator.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub traffic: u8,
    /// Zone density (GlassBox rules): 0 low, 1 medium, 2 high.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub density: u8,
    /// Unit type (index in the GlassBox ruleset) of a developed building.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<u16>,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

impl Structure {
    pub fn new(code: StructureCode) -> Self {
        Self { code, graphic: code.base_graphic(), level: 0, connections: 0, traffic: 0, density: 0, unit: None }
    }

    /// Graphic code a zone shows at the given level.
    pub fn zone_graphic(code: StructureCode, level: u8) -> GraphicCode {
        code.base_graphic().offset((level / LEVEL_RATIO) as u16)
    }

    pub fn next_level_graphic(&self) -> GraphicCode {
        if self.code.is_zone() && self.level < LEVEL_MAX {
            Self::zone_graphic(self.code, self.level + 1)
        } else {
            self.graphic
        }
    }

    pub fn previous_level_graphic(&self) -> GraphicCode {
        if self.code.is_zone() && self.level > 0 {
            Self::zone_graphic(self.code, self.level - 1)
        } else {
            self.graphic
        }
    }

    fn update_path_graphic(&mut self) {
        self.graphic = path_graphic(self.code.base_graphic(), self.connections);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TileKind {
    Main(Structure),
    Part { origin: (u32, u32) },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    pub kind: TileKind,
    #[serde(default)]
    pub status: Status,
}

impl Tile {
    fn main(s: Structure) -> Self {
        Self { kind: TileKind::Main(s), status: Status::default() }
    }

    fn part(origin: (u32, u32)) -> Self {
        Self { kind: TileKind::Part { origin }, status: Status::default() }
    }

    /// The code of *this tile*: a part tile reports [`StructureCode::Part`].
    pub fn code(&self) -> StructureCode {
        match &self.kind {
            TileKind::Main(s) => s.code,
            TileKind::Part { .. } => StructureCode::Part,
        }
    }

    pub fn structure(&self) -> Option<&Structure> {
        match &self.kind {
            TileKind::Main(s) => Some(s),
            TileKind::Part { .. } => None,
        }
    }
}

/// Inclusive tile rectangle with `w1 <= w2` and `l1 <= l2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub w1: u32,
    pub l1: u32,
    pub w2: u32,
    pub l2: u32,
}

impl Rect {
    /// Rectangle spanned by two corners in any order.
    pub fn new(a: (u32, u32), b: (u32, u32)) -> Self {
        Self { w1: a.0.min(b.0), l1: a.1.min(b.1), w2: a.0.max(b.0), l2: a.1.max(b.1) }
    }

    pub fn tile(w: u32, l: u32) -> Self {
        Self { w1: w, l1: l, w2: w, l2: l }
    }

    pub fn sized(w: u32, l: u32, width: u32, length: u32) -> Self {
        Self { w1: w, l1: l, w2: w + width - 1, l2: l + length - 1 }
    }

    pub fn tiles(self) -> impl Iterator<Item = (u32, u32)> {
        (self.l1..=self.l2).flat_map(move |l| (self.w1..=self.w2).map(move |w| (w, l)))
    }
}

/// What [`BuildingLayer::contains`] looks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    Any,
    Code(StructureCode),
    /// A tile with the power flag set (legacy `OC_STRUCTURE_ELECTRIC`).
    Powered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BuildError {
    #[error("outside of the map")]
    OutOfBounds,
    #[error("the ground is not suitable")]
    NotConstructible,
    #[error("nothing to do here")]
    Nothing,
}

/// Change counter for renderers; never serialised and ignored by equality.
#[derive(Clone, Copy, Debug, Default)]
pub struct Revision(pub u64);

impl PartialEq for Revision {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for Revision {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildingLayer {
    width: u32,
    length: u32,
    tiles: Vec<Option<Tile>>,
    #[serde(skip)]
    revision: Revision,
}

impl BuildingLayer {
    pub fn new(width: u32, length: u32) -> Self {
        Self { width, length, tiles: vec![None; (width * length) as usize], revision: Revision(1) }
    }

    /// Increases whenever a structure, its graphic or its level changes.
    pub fn revision(&self) -> u64 {
        self.revision.0
    }

    fn touch(&mut self) {
        self.revision.0 += 1;
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn length(&self) -> u32 {
        self.length
    }

    #[inline]
    fn idx(&self, w: u32, l: u32) -> usize {
        debug_assert!(w < self.width && l < self.length, "tile ({w},{l}) out of bounds");
        (l * self.width + w) as usize
    }

    pub fn in_bounds(&self, r: Rect) -> bool {
        r.w2 < self.width && r.l2 < self.length
    }

    pub fn tile(&self, w: u32, l: u32) -> Option<&Tile> {
        self.tiles[self.idx(w, l)].as_ref()
    }

    pub fn tile_mut(&mut self, w: u32, l: u32) -> Option<&mut Tile> {
        let i = self.idx(w, l);
        self.tiles[i].as_mut()
    }

    pub fn code_at(&self, w: u32, l: u32) -> Option<StructureCode> {
        self.tile(w, l).map(Tile::code)
    }

    /// Origin of the structure covering the tile.
    pub fn origin_of(&self, w: u32, l: u32) -> Option<(u32, u32)> {
        self.tile(w, l).map(|t| match t.kind {
            TileKind::Main(_) => (w, l),
            TileKind::Part { origin } => origin,
        })
    }

    /// The structure covering the tile (following part tiles to their origin).
    pub fn structure_at(&self, w: u32, l: u32) -> Option<&Structure> {
        let (ow, ol) = self.origin_of(w, l)?;
        self.tile(ow, ol)?.structure()
    }

    pub fn structure_mut(&mut self, w: u32, l: u32) -> Option<&mut Structure> {
        self.touch();
        match &mut self.tile_mut(w, l)?.kind {
            TileKind::Main(s) => Some(s),
            TileKind::Part { .. } => None,
        }
    }

    /// Number of occupied tiles (legacy `Structure::GetNumber`).
    pub fn occupied(&self) -> usize {
        self.tiles.iter().filter(|t| t.is_some()).count()
    }

    /// All structures with their origin.
    pub fn structures(&self) -> impl Iterator<Item = ((u32, u32), &Structure)> {
        let width = self.width;
        self.tiles.iter().enumerate().filter_map(move |(i, t)| {
            let s = t.as_ref()?.structure()?;
            Some((((i as u32) % width, (i as u32) / width), s))
        })
    }

    /// All occupied tiles.
    pub fn tiles(&self) -> impl Iterator<Item = ((u32, u32), &Tile)> {
        let width = self.width;
        self.tiles.iter().enumerate().filter_map(move |(i, t)| Some((((i as u32) % width, (i as u32) / width), t.as_ref()?)))
    }

    pub fn for_each_status(&mut self, mut f: impl FnMut(&mut Status)) {
        for t in self.tiles.iter_mut().flatten() {
            f(&mut t.status);
        }
    }

    /// Picks a random structure origin with the given code.
    pub fn random_structure(&self, code: StructureCode, rng: &mut impl Rng) -> Option<(u32, u32)> {
        let candidates: Vec<(u32, u32)> = self.structures().filter(|(_, s)| s.code == code).map(|(p, _)| p).collect();
        (!candidates.is_empty()).then(|| candidates[rng.gen_range(0..candidates.len())])
    }

    /// At least one tile of the rectangle matches.
    pub fn contains(&self, r: Rect, q: Query) -> bool {
        r.tiles().any(|(w, l)| match (self.tile(w, l), q) {
            (None, _) => false,
            (Some(_), Query::Any) => true,
            (Some(t), Query::Code(c)) => t.code() == c,
            (Some(t), Query::Powered) => t.status.has(Status::POWER),
        })
    }

    /// Every occupied tile of the rectangle belongs to a structure with this code
    /// (empty tiles are allowed).
    pub fn contains_only(&self, r: Rect, code: StructureCode) -> bool {
        r.tiles().all(|(w, l)| self.structure_at(w, l).is_none_or(|s| s.code == code))
    }

    /// Whether `code` can be built on every tile of `r` (legacy `IsConstructive`).
    pub fn can_build(&self, r: Rect, code: StructureCode, terrain: &Terrain) -> bool {
        if !self.in_bounds(r) {
            return false;
        }
        r.tiles().all(
            |(w, l)| {
                if code.is_path() {
                    self.path_constructible(w, l, code, terrain)
                } else {
                    self.tile(w, l).is_none() && terrain.is_tile_flat(w, l)
                }
            },
        )
    }

    fn path_constructible(&self, w: u32, l: u32, code: StructureCode, terrain: &Terrain) -> bool {
        self.code_at(w, l).is_none_or(|c| c == code) && terrain.is_tile_path_friendly(w, l)
    }

    /// Builds `code` over `r` and returns the cost. Zones, parks and trees
    /// fill every free flat tile; roads and power lines follow an L-shaped
    /// route between the corners; plants and services are a single block
    /// anchored at `(r.w1, r.l1)`.
    pub fn build(&mut self, r: Rect, code: StructureCode, terrain: &Terrain, footprints: &Footprints, rng: &mut impl Rng) -> Result<u32, BuildError> {
        if !self.in_bounds(r) {
            return Err(BuildError::OutOfBounds);
        }
        self.touch();
        match code {
            StructureCode::Part => Err(BuildError::Nothing),
            c if c.is_path() => self.build_path(r, c, terrain),
            c if c.is_block() || c == StructureCode::Test => self.build_block(r.w1, r.l1, c, terrain, footprints),
            c => self.build_area(r, c, terrain, rng),
        }
    }

    fn build_area(&mut self, r: Rect, code: StructureCode, terrain: &Terrain, rng: &mut impl Rng) -> Result<u32, BuildError> {
        let mut cost = 0;
        for (w, l) in r.tiles() {
            if self.tile(w, l).is_none() && terrain.is_tile_flat(w, l) {
                let mut s = Structure::new(code);
                if code == StructureCode::Flora {
                    s.graphic = GraphicCode(rng.gen_range(GraphicCode::TREE_BEGIN.0..=GraphicCode::TREE_END.0));
                }
                let i = self.idx(w, l);
                self.tiles[i] = Some(Tile::main(s));
                cost += code.build_cost();
            }
        }
        if cost > 0 || code.build_cost() == 0 {
            Ok(cost)
        } else {
            Err(BuildError::Nothing)
        }
    }

    /// Size of the block `code` occupies.
    pub fn block_size(code: StructureCode, footprints: &Footprints) -> (u32, u32) {
        footprints.size(code.base_graphic())
    }

    fn build_block(&mut self, w: u32, l: u32, code: StructureCode, terrain: &Terrain, footprints: &Footprints) -> Result<u32, BuildError> {
        let (sw, sl) = Self::block_size(code, footprints);
        let r = Rect::sized(w, l, sw, sl);
        if !self.in_bounds(r) {
            return Err(BuildError::OutOfBounds);
        }
        if !self.can_build(r, code, terrain) {
            return Err(BuildError::NotConstructible);
        }
        self.place_block(r, Structure::new(code));
        if code.is_power_plant() {
            self.tile_mut(w, l).unwrap().status.set(Status::POWER);
        }
        Ok(code.build_cost())
    }

    /// Puts `s` at the origin of `r` and part tiles on the rest.
    pub(crate) fn place_block(&mut self, r: Rect, s: Structure) {
        self.touch();
        let origin = (r.w1, r.l1);
        for (w, l) in r.tiles() {
            let i = self.idx(w, l);
            self.tiles[i] = Some(Tile::part(origin));
        }
        let i = self.idx(r.w1, r.l1);
        self.tiles[i] = Some(Tile::main(s));
    }

    /// The L-shaped route the path tool lays between two corners
    /// (the longer side first, legacy `_BuildPathStructure`).
    pub fn path_route(r: Rect) -> Vec<(u32, u32)> {
        let mut route = Vec::new();
        if r.w2 - r.w1 >= r.l2 - r.l1 {
            route.extend((r.w1..r.w2).map(|w| (w, r.l1)));
            route.extend((r.l1..=r.l2).map(|l| (r.w2, l)));
        } else {
            route.extend((r.l1..r.l2).map(|l| (r.w1, l)));
            route.extend((r.w1..=r.w2).map(|w| (w, r.l2)));
        }
        route
    }

    fn build_path(&mut self, r: Rect, code: StructureCode, terrain: &Terrain) -> Result<u32, BuildError> {
        let route = Self::path_route(r);
        if !route.iter().all(|&(w, l)| self.path_constructible(w, l, code, terrain)) {
            return Err(BuildError::NotConstructible);
        }
        let mut cost = 0;
        for (w, l) in route {
            if self.tile(w, l).is_some() {
                continue;
            }
            let i = self.idx(w, l);
            self.tiles[i] = Some(Tile::main(Structure::new(code)));
            cost += code.build_cost();
            for dir in Direction::ALL {
                if let Some((nw, nl)) = terrain.neighbour(w, l, dir) {
                    if self.code_at(nw, nl) == Some(code) {
                        self.connect(w, l, dir, true);
                        self.connect(nw, nl, dir.opposite(), true);
                    }
                }
            }
        }
        Ok(cost)
    }

    fn connect(&mut self, w: u32, l: u32, dir: Direction, on: bool) {
        if let Some(s) = self.structure_mut(w, l) {
            if on {
                s.connections |= dir.mask();
            } else {
                s.connections &= !dir.mask();
            }
            s.update_path_graphic();
        }
    }

    /// Recomputes every road/power-line connection from the grid
    /// (used after loading legacy saves).
    pub fn rebuild_connections(&mut self) {
        self.touch();
        let mut updates = Vec::new();
        for ((w, l), s) in self.structures() {
            if !s.code.is_path() {
                continue;
            }
            let mut mask = 0;
            for dir in Direction::ALL {
                let (dw, dl) = dir.delta();
                let (nw, nl) = (w as i64 + dw as i64, l as i64 + dl as i64);
                if nw >= 0 && nl >= 0 && (nw as u32) < self.width && (nl as u32) < self.length && self.code_at(nw as u32, nl as u32) == Some(s.code) {
                    mask |= dir.mask();
                }
            }
            updates.push(((w, l), mask));
        }
        for ((w, l), mask) in updates {
            let s = self.structure_mut(w, l).unwrap();
            s.connections = mask;
            s.update_path_graphic();
        }
    }

    /// Bulldozes everything touching `r` and returns the cost
    /// (legacy `DestroyStructure`: each tile is billed by its own code).
    pub fn destroy(&mut self, r: Rect) -> Result<u32, BuildError> {
        if !self.in_bounds(r) {
            return Err(BuildError::OutOfBounds);
        }
        let mut cost = 0;
        let mut removed = false;
        for (w, l) in r.tiles() {
            if let Some(c) = self.destroy_tile(w, l) {
                cost += c;
                removed = true;
            }
        }
        if removed {
            Ok(cost)
        } else {
            Err(BuildError::Nothing)
        }
    }

    /// Removes the structure covering `(w, l)`; returns its cost.
    pub fn destroy_tile(&mut self, w: u32, l: u32) -> Option<u32> {
        self.touch();
        let tile = self.tile(w, l)?;
        let level = self.structure_at(w, l).map_or(0, |s| s.level);
        let cost = tile.code().destroy_cost(level);
        let code = tile.code();
        if code.is_path() {
            let i = self.idx(w, l);
            self.tiles[i] = None;
            for dir in Direction::ALL {
                let (dw, dl) = dir.delta();
                let (nw, nl) = (w as i64 + dw as i64, l as i64 + dl as i64);
                if nw >= 0 && nl >= 0 && (nw as u32) < self.width && (nl as u32) < self.length && self.code_at(nw as u32, nl as u32) == Some(code) {
                    self.connect(nw as u32, nl as u32, dir.opposite(), false);
                }
            }
        } else {
            let origin = self.origin_of(w, l)?;
            self.remove_structure(origin);
        }
        Some(cost)
    }

    /// Clears the origin tile and every part tile pointing to it.
    fn remove_structure(&mut self, origin: (u32, u32)) {
        // Parts always lie south-east of the origin, within the largest footprint.
        let w2 = (origin.0 + MAX_FOOTPRINT - 1).min(self.width - 1);
        let l2 = (origin.1 + MAX_FOOTPRINT - 1).min(self.length - 1);
        for (w, l) in (Rect { w1: origin.0, l1: origin.1, w2, l2 }).tiles() {
            if self.origin_of(w, l) == Some(origin) {
                let i = self.idx(w, l);
                self.tiles[i] = None;
            }
        }
    }

    /// Re-shapes the zone at `(w, l)` after its graphic changed from
    /// `old_graphic` (legacy `ResizeStructure`). Growing swallows the
    /// neighbouring zones of the same kind; shrinking hands the freed tiles
    /// back as fresh level-0 zones.
    pub fn resize(&mut self, w: u32, l: u32, old_graphic: GraphicCode, terrain: &Terrain, footprints: &Footprints, rng: &mut impl Rng) {
        self.touch();
        let Some(s) = self.tile(w, l).and_then(Tile::structure) else { return };
        if s.graphic == old_graphic {
            return;
        }
        let code = s.code;
        let (ow, ol) = footprints.size(old_graphic);
        let (nw, nl) = footprints.size(s.graphic);
        let old_rect = Rect::sized(w, l, ow, ol);
        let new_rect = Rect::sized(w, l, nw, nl);

        for (dw, dl) in new_rect.tiles() {
            match self.origin_of(dw, dl) {
                Some(o) if o == (w, l) => {}
                None => {
                    let i = self.idx(dw, dl);
                    self.tiles[i] = Some(Tile::part((w, l)));
                }
                Some(o) => {
                    // Swallow another structure; give its remaining tiles back as zones.
                    let other = self.structure_at(dw, dl).cloned();
                    self.remove_structure(o);
                    let i = self.idx(dw, dl);
                    self.tiles[i] = Some(Tile::part((w, l)));
                    if let Some(other) = other {
                        let (xw, xl) = footprints.size(other.graphic);
                        if xw > 1 || xl > 1 {
                            let _ = self.build_area(Rect::sized(o.0, o.1, xw, xl), other.code, terrain, rng);
                        }
                    }
                }
            }
        }
        for (dw, dl) in old_rect.tiles() {
            let inside_new = dw <= new_rect.w2 && dl <= new_rect.l2;
            if !inside_new {
                let i = self.idx(dw, dl);
                self.tiles[i] = Some(Tile::main(Structure::new(code)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_pcg::Pcg32;

    fn setup() -> (BuildingLayer, Terrain, Footprints, Pcg32) {
        (BuildingLayer::new(20, 20), Terrain::flat(20, 20), Footprints::builtin(), Pcg32::seed_from_u64(1))
    }

    #[test]
    fn roads_connect_and_disconnect() {
        let (mut layer, t, f, mut rng) = setup();
        let cost = layer.build(Rect::new((2, 2), (6, 4)), StructureCode::Road, &t, &f, &mut rng).unwrap();
        // L route: (2..6, 2) then (6, 2..=4) -> 4 + 3 tiles
        assert_eq!(cost, 7 * 5);
        assert_eq!(layer.structure_at(6, 2).unwrap().graphic, path_graphic(GraphicCode::ROAD_BASE, crate::codes::connect::WEST | crate::codes::connect::SOUTH));
        assert_eq!(layer.structure_at(4, 2).unwrap().graphic, GraphicCode(155)); // W_E
        layer.destroy(Rect::tile(6, 3)).unwrap();
        assert_eq!(layer.structure_at(6, 4).unwrap().graphic, GraphicCode(150)); // isolated
        assert_eq!(layer.structure_at(6, 2).unwrap().graphic, GraphicCode(153));
        // O_W
    }

    #[test]
    fn blocks_have_parts_and_are_destroyed_whole() {
        let (mut layer, t, f, mut rng) = setup();
        let cost = layer.build(Rect::tile(1, 1), StructureCode::CoalPlant, &t, &f, &mut rng).unwrap();
        assert_eq!(cost, 2000);
        assert_eq!(layer.occupied(), 15);
        assert_eq!(layer.code_at(5, 3), Some(StructureCode::Part));
        assert_eq!(layer.origin_of(5, 3), Some((1, 1)));
        // overlapping build fails
        assert_eq!(layer.build(Rect::tile(4, 2), StructureCode::FireDept, &t, &f, &mut rng), Err(BuildError::NotConstructible));
        let cost = layer.destroy(Rect::tile(3, 2)).unwrap();
        assert_eq!(cost, 200);
        assert_eq!(layer.occupied(), 0);
    }

    #[test]
    fn zones_fill_free_flat_tiles() {
        let (mut layer, mut t, f, mut rng) = setup();
        t.change_tile_height(0, 0, true);
        let cost = layer.build(Rect::new((0, 0), (2, 1)), StructureCode::Residential, &t, &f, &mut rng).unwrap();
        // (0,0) is raised but flat; (1,0), (0,1), (1,1) are slopes
        assert_eq!(cost, 3 * 5);
        assert!(layer.contains(Rect::new((0, 0), (5, 5)), Query::Code(StructureCode::Residential)));
    }

    #[test]
    fn growth_swallows_neighbours_and_shrink_restores() {
        let (mut layer, t, f, mut rng) = setup();
        layer.build(Rect::new((0, 0), (4, 4)), StructureCode::Residential, &t, &f, &mut rng).unwrap();
        let s = layer.structure_mut(0, 0).unwrap();
        let old = s.graphic;
        s.level = 80;
        s.graphic = Structure::zone_graphic(StructureCode::Residential, 80); // gc 18: 2x2
        layer.resize(0, 0, old, &t, &f, &mut rng);
        assert_eq!(layer.origin_of(1, 1), Some((0, 0)));
        assert_eq!(layer.code_at(2, 0), Some(StructureCode::Residential));
        let s = layer.structure_mut(0, 0).unwrap();
        let old = s.graphic;
        s.level = 0;
        s.graphic = Structure::zone_graphic(StructureCode::Residential, 0);
        layer.resize(0, 0, old, &t, &f, &mut rng);
        assert_eq!(layer.code_at(1, 1), Some(StructureCode::Residential));
        assert_eq!(layer.structures().count(), 25);
    }
}
