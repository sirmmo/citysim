//! The city: terrain + buildings + economy + calendar (legacy `City`).

use std::collections::VecDeque;

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg32;
use serde::{Deserialize, Serialize};

use crate::{
    glassbox::GlassBox,
    layer::{BuildError, BuildingLayer, Query, Rect, Status},
    rules::Ruleset,
    sim::{self, SimValues},
    traffic::{find_path, Vehicle, VehicleKind, MAX_VEHICLES},
    Footprints, Sound, StructureCode, Terrain, Tool,
};

pub const FUND_START: i64 = 15_000;
pub const RESIDENTIAL_TAX: u32 = 8;
pub const COMMERCIAL_TAX: u32 = 8;
pub const INDUSTRIAL_TAX: u32 = 9;
/// Yearly government help, in % of the collected income.
pub const INCOME_HELP: u32 = 10;
/// Simulated milliseconds per in-game day.
pub const MS_PER_DAY: f64 = 1750.0;
/// Years of history kept for the graphs.
pub const HISTORY_LEN: usize = 20;
pub const TERRAIN_COST: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Date {
    pub day: u32,
    pub month: u32,
    pub year: u32,
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.day, self.month, self.year)
    }
}

/// One yearly sample of the city resources (legacy `Ressource`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub fund: i64,
    pub population: u32,
    pub residential: u32,
    pub commercial: u32,
    pub industrial: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Speed {
    Paused,
    Normal,
    Fast,
}

impl Speed {
    pub fn factor(self) -> f64 {
        match self {
            Speed::Paused => 0.0,
            Speed::Normal => 1.0,
            Speed::Fast => 4.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolOutcome {
    pub cost: u32,
    pub sound: Sound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ToolError {
    #[error("the city is bankrupt")]
    Bankrupt,
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error("something is built too close")]
    TerrainOccupied,
    #[error("height limit reached")]
    TerrainLimit,
}

/// What the query tool reports about a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileInfo {
    pub code: StructureCode,
    pub level: u8,
    pub status: Status,
    pub traffic: u8,
}

/// How tile coordinates relate to the source map (set by importers).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeoRef {
    /// `true`: coordinates are WGS84 lon/lat; `false`: planar map units.
    pub wgs84: bool,
    /// Source coordinate of the west edge of tile column 0.
    pub west: f64,
    /// Source coordinate of the north edge of tile row 0.
    pub north: f64,
    /// Metres (WGS84) or map units (planar) per tile side.
    pub units_per_tile: f64,
    /// Cosine of the reference latitude (WGS84 only).
    pub lat_scale: f64,
}

/// Descriptive data that does not affect the simulation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CityMeta {
    pub name: Option<String>,
    /// Where the data came from (file, URL, attribution).
    pub source: Option<String>,
    pub georef: Option<GeoRef>,
    /// Year the city represents (historical imports); drives era skins.
    #[serde(default)]
    pub year: Option<i64>,
    /// World the city belongs to (fantasy maps); drives world skins.
    #[serde(default)]
    pub world: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct City {
    #[serde(default)]
    pub meta: CityMeta,
    pub terrain: Terrain,
    pub layer: BuildingLayer,
    pub fund: i64,
    /// Income collected since the last new year.
    pub income: u32,
    pub population: u32,
    pub date: Date,
    pub history: VecDeque<Record>,
    pub speed: Speed,
    pub values: SimValues,
    pub vehicles: Vec<Vehicle>,
    pub(crate) rng: Pcg32,
    day_clock_ms: f64,
    sim_clock_ms: f64,
    #[serde(skip, default = "Footprints::builtin")]
    footprints: Footprints,
    /// Data-driven GlassBox engine; `None` runs the classic OpenCity rules.
    #[serde(default)]
    pub glassbox: Option<GlassBox>,
    /// Density used by the zone tools (GlassBox rules): 0 low, 1 medium, 2 high.
    #[serde(default)]
    pub zone_density: u8,
}

impl City {
    /// A new city on `terrain`, with trees wherever `tree_density > 0`
    /// (one value per tile, row-major).
    pub fn new(terrain: Terrain, tree_density: Option<&[i32]>, footprints: Footprints, seed: u64) -> Self {
        let (width, length) = (terrain.width(), terrain.length());
        let mut city = Self {
            meta: CityMeta::default(),
            layer: BuildingLayer::new(width, length),
            terrain,
            fund: FUND_START,
            income: 0,
            population: 0,
            date: Date { day: 1, month: 1, year: 0 },
            history: std::iter::repeat_n(Record::default(), HISTORY_LEN).collect(),
            speed: Speed::Normal,
            values: SimValues::default(),
            vehicles: Vec::new(),
            rng: Pcg32::seed_from_u64(seed),
            day_clock_ms: 0.0,
            sim_clock_ms: 0.0,
            footprints,
            glassbox: None,
            zone_density: 0,
        };
        if let Some(density) = tree_density {
            assert_eq!(density.len(), (width * length) as usize, "tree density size");
            for (i, &d) in density.iter().enumerate() {
                if d > 0 {
                    let (w, l) = (i as u32 % width, i as u32 / width);
                    let _ = city.layer.build(Rect::tile(w, l), StructureCode::Flora, &city.terrain, &city.footprints, &mut city.rng);
                }
            }
        }
        city
    }

    pub fn width(&self) -> u32 {
        self.terrain.width()
    }

    pub fn length(&self) -> u32 {
        self.terrain.length()
    }

    /// Switches this city to a GlassBox ruleset (new cities), or re-attaches
    /// the rules after loading a save that uses them.
    pub fn use_rules(&mut self, rules: std::sync::Arc<Ruleset>) {
        let (w, l) = (self.width(), self.length());
        match &mut self.glassbox {
            Some(g) => g.attach(rules, w, l),
            None => {
                self.convert_to_rules(&rules);
                self.glassbox = Some(GlassBox::new(rules, w, l));
            }
        }
    }

    /// Existing buildings under new rules: services become their units, and
    /// grown zones go back to plots that develop under the new rules.
    fn convert_to_rules(&mut self, rules: &Ruleset) {
        let origins: Vec<((u32, u32), StructureCode)> = self.layer.structures().filter(|(_, s)| s.unit.is_none()).map(|(p, s)| (p, s.code)).collect();
        for ((w, l), code) in origins {
            if code.is_zone() {
                let Some(s) = self.layer.structure_mut(w, l) else { continue };
                if s.level > 0 {
                    let old = s.graphic;
                    s.level = 0;
                    s.graphic = code.base_graphic();
                    self.layer.resize(w, l, old, &self.terrain, &self.footprints, &mut self.rng);
                }
                continue;
            }
            let tool = Tool::ALL.into_iter().find(|t| t.structure() == Some(code));
            if let Some(unit) = tool.and_then(|t| rules.services.get(t.id())).and_then(|u| rules.unit(u)) {
                if let Some(s) = self.layer.structure_mut(w, l) {
                    s.unit = Some(unit as u16);
                    s.level = 1;
                }
            }
        }
    }

    /// The GlassBox ruleset in use, if any.
    pub fn rules(&self) -> Option<&std::sync::Arc<Ruleset>> {
        self.glassbox.as_ref().and_then(|g| g.rules())
    }

    pub fn footprints(&self) -> &Footprints {
        &self.footprints
    }

    /// Replaces the model footprints (after loading a save, which does not store them).
    pub fn set_footprints(&mut self, footprints: Footprints) {
        self.footprints = footprints;
    }

    /// Tiles covered by a block structure placed with its origin at `(w, l)`.
    pub fn block_rect(&self, code: StructureCode, w: u32, l: u32) -> Rect {
        let (sw, sl) = BuildingLayer::block_size(code, &self.footprints);
        Rect::sized(w, l, sw, sl)
    }

    /// Whether `tool` would succeed when anchored at `(w, l)` (build preview).
    pub fn can_place(&self, tool: Tool, w: u32, l: u32) -> bool {
        match tool.structure() {
            Some(code) if code.is_block() => self.layer.can_build(self.block_rect(code, w, l), code, &self.terrain),
            Some(code) => self.layer.can_build(Rect::tile(w, l), code, &self.terrain),
            None => true,
        }
    }

    /// Applies a tool between two corner tiles (identical for single-tile tools).
    pub fn apply_tool(&mut self, tool: Tool, a: (u32, u32), b: (u32, u32)) -> Result<ToolOutcome, ToolError> {
        if self.fund < 0 {
            return Err(ToolError::Bankrupt);
        }
        let rect = Rect::new(a, b);
        let cost = match tool {
            Tool::Destroy => self.layer.destroy(rect)?,
            Tool::RaiseTerrain | Tool::LowerTerrain => {
                let (w1, l1, w2, l2) = self.terrain.around(a.0, a.1, 1);
                if self.layer.contains(Rect { w1, l1, w2, l2 }, Query::Any) {
                    return Err(ToolError::TerrainOccupied);
                }
                if !self.terrain.change_tile_height(a.0, a.1, tool == Tool::RaiseTerrain) {
                    return Err(ToolError::TerrainLimit);
                }
                TERRAIN_COST
            }
            _ => {
                let code = tool.structure().expect("building tool");
                let rect = if code.is_block() { Rect::tile(a.0, a.1) } else { rect };
                let mut cost = match tool.road_kind() {
                    Some(kind) => self.layer.build_road(rect, kind, &self.terrain)?,
                    None => self.layer.build(rect, code, &self.terrain, &self.footprints, &mut self.rng)?,
                };
                if let Some(per_tile) = self.zone_cost(code) {
                    cost = cost / code.build_cost().max(1) * per_tile;
                }
                // Services priced by the rules (e.g. a well instead of a park).
                if let Some(price) = self.rules().and_then(|r| r.service_unit(tool.id())).map(|u| u.build_cost).filter(|&c| c > 0) {
                    cost = cost / code.build_cost().max(1) * price;
                }
                self.after_build(tool, code, rect);
                cost
            }
        };
        self.fund -= cost as i64;
        Ok(ToolOutcome { cost, sound: tool.sound() })
    }

    /// Zoning cost per tile at the current density, under GlassBox rules.
    fn zone_cost(&self, code: StructureCode) -> Option<u32> {
        let rules = self.rules()?;
        let z = rules.zone_for(code)?;
        rules.zones[z].densities.get(self.zone_density as usize).map(|d| d.cost)
    }

    /// GlassBox bookkeeping after a build: zone density, service units.
    fn after_build(&mut self, tool: Tool, code: StructureCode, rect: Rect) {
        let Some(rules) = self.rules().cloned() else { return };
        if code.is_zone() {
            let density = self.zone_density;
            for (w, l) in rect.tiles() {
                if let Some(s) = self.layer.structure_mut(w, l) {
                    if s.code == code && s.unit.is_none() {
                        s.density = density;
                    }
                }
            }
        } else if let Some(unit) = rules.services.get(tool.id()).and_then(|u| rules.unit(u)) {
            // Blocks have one origin; area tools (parks) make every new tile a unit.
            let tiles: Vec<(u32, u32)> = if code.is_block() { vec![(rect.w1, rect.l1)] } else { rect.tiles().collect() };
            for (w, l) in tiles {
                if let Some(s) = self.layer.structure_mut(w, l) {
                    if s.code == code && s.unit.is_none() {
                        s.unit = Some(unit as u16);
                        s.level = 1;
                    }
                }
            }
        }
    }

    pub fn query(&self, w: u32, l: u32) -> Option<TileInfo> {
        let tile = self.layer.tile(w, l)?;
        let s = self.layer.structure_at(w, l)?;
        Some(TileInfo { code: s.code, level: s.level, status: tile.status, traffic: s.traffic })
    }

    /// Simulated milliseconds between two simulator rounds. The legacy engine
    /// slept this long between rounds: big empty maps are slow, busy ones fast.
    pub fn sim_interval_ms(&self) -> f64 {
        let area = (self.width() * self.length()) as f64;
        let occupied = self.layer.occupied() as f64;
        ((area + 1.0).log10() * 500.0 - (occupied + 1.0).log10() * 400.0).max(10.0)
    }

    /// Advances the city by `dt_ms` real milliseconds at the current speed.
    pub fn update(&mut self, dt_ms: f64) {
        let dt = dt_ms * self.speed.factor();
        if dt <= 0.0 {
            return;
        }
        self.vehicles.retain_mut(|v| v.advance(dt as f32));
        self.sim_clock_ms += dt;
        while self.sim_clock_ms >= self.sim_interval_ms() {
            self.sim_clock_ms -= self.sim_interval_ms();
            self.simulate_round();
        }
        self.day_clock_ms += dt;
        while self.day_clock_ms >= MS_PER_DAY {
            self.day_clock_ms -= MS_PER_DAY;
            self.next_day();
        }
    }

    /// One round of every micro-simulator.
    pub fn simulate_round(&mut self) {
        if self.glassbox.is_some() {
            return self.simulate_glassbox();
        }
        for code in [StructureCode::Residential, StructureCode::Commercial, StructureCode::Industrial] {
            sim::zone_step(&mut self.layer, &self.terrain, &self.footprints, code, &mut self.rng);
        }
        sim::power_step(&mut self.layer, &self.terrain);
        if let Some(start) = sim::traffic_step(&mut self.layer, &self.terrain, &mut self.rng) {
            self.spawn_vehicle(start);
        }
        self.values = sim::refresh_values(&self.layer);
    }

    fn simulate_glassbox(&mut self) {
        let mut gb = self.glassbox.take().unwrap();
        let trips = gb.tick(&mut self.layer, &self.terrain, &self.footprints, &mut self.fund, &mut self.rng);
        // Road traffic loads still come from the classic traffic simulator.
        let _ = sim::traffic_step(&mut self.layer, &self.terrain, &mut self.rng);
        // A sample of the trips becomes visible vehicles.
        for t in trips.iter().filter(|t| t.amount >= 1.0).take(64) {
            if self.vehicles.len() >= MAX_VEHICLES || self.rng.gen_range(0..100) >= 15 {
                continue;
            }
            let (Some(a), Some(b)) = (self.nearest_road(t.from), self.nearest_road(t.to)) else { continue };
            let kind = [VehicleKind::Car, VehicleKind::Pickup, VehicleKind::Sports][t.resource % 3];
            if let Some(path) = find_path(&self.layer, &self.terrain, a, b, kind.path_cost()) {
                if path.len() > 1 {
                    self.vehicles.push(Vehicle::new(kind, &path, &self.layer, &self.terrain));
                }
            }
        }
        self.values = gb.values(&self.layer);
        if let Some(pop) = gb.rules().and_then(|r| r.population.as_deref().and_then(|p| r.resource(p))) {
            self.population = gb.local_total(pop) as u32;
        }
        self.glassbox = Some(gb);
    }

    /// A road tile next to (or near) `p`.
    fn nearest_road(&self, p: (u32, u32)) -> Option<(u32, u32)> {
        for r in 1..=3 {
            let (w1, l1, w2, l2) = self.terrain.around(p.0, p.1, r);
            if let Some(t) = (Rect { w1, l1, w2, l2 }).tiles().find(|&(w, l)| self.layer.code_at(w, l) == Some(StructureCode::Road)) {
                return Some(t);
            }
        }
        None
    }

    /// Recomputes power and the aggregated values without advancing time
    /// (after editing the layer directly, e.g. in importers).
    pub fn refresh(&mut self) {
        sim::power_step(&mut self.layer, &self.terrain);
        self.values = sim::refresh_values(&self.layer);
    }

    /// Mutable access to the layer, terrain and RNG at once (for importers).
    pub fn parts_mut(&mut self) -> (&mut BuildingLayer, &mut Terrain, &Footprints, &mut Pcg32) {
        (&mut self.layer, &mut self.terrain, &self.footprints, &mut self.rng)
    }

    fn spawn_vehicle(&mut self, start: (u32, u32)) {
        if self.vehicles.len() >= MAX_VEHICLES {
            return;
        }
        let Some(dest) = self.layer.random_structure(StructureCode::Road, &mut self.rng) else { return };
        let kind = [VehicleKind::Car, VehicleKind::Pickup, VehicleKind::Sports][self.rng.gen_range(0..3)];
        if let Some(path) = find_path(&self.layer, &self.terrain, start, dest, kind.path_cost()) {
            if path.len() > 1 {
                self.vehicles.push(Vehicle::new(kind, &path, &self.layer, &self.terrain));
            }
        }
    }

    fn next_day(&mut self) {
        self.date.day += 1;
        if self.date.day <= 30 {
            return;
        }
        self.date.day = 1;
        let v = self.values;
        if self.glassbox.is_none() {
            self.population = v.residential + v.commercial / 2 + v.industrial / 3;
        }
        self.date.month += 1;
        if self.date.month > 12 {
            self.date.month = 1;
            self.date.year += 1;
            self.pay_bills(true);
            self.history.pop_front();
            self.history.push_back(Record {
                fund: self.fund,
                population: self.population,
                residential: v.residential,
                commercial: v.commercial,
                industrial: v.industrial,
            });
        } else {
            self.pay_bills(false);
        }
    }

    /// Monthly upkeep and taxes; taxes are paid out once a year (legacy `_DoBill`).
    fn pay_bills(&mut self, new_year: bool) {
        if self.glassbox.is_some() {
            // Money flows through the rules (taxes and upkeep resources).
            return;
        }
        let maintenance: u32 = self
            .layer
            .tiles()
            .map(|(_, t)| match t.structure() {
                Some(s) if s.code == StructureCode::Road => s.road.maintenance_cost(),
                _ => t.code().maintenance_cost(),
            })
            .sum();
        self.fund -= maintenance as i64;
        let v = self.values;
        self.income += (v.residential * RESIDENTIAL_TAX + v.commercial * COMMERCIAL_TAX + v.industrial * INDUSTRIAL_TAX) / 100;
        if new_year {
            self.income += self.income * INCOME_HELP / 100;
            self.fund += self.income as i64;
            log::info!(
                "Happy new year {}! income {} fund {} R/C/I {}/{}/{}",
                self.date.year,
                self.income,
                self.fund,
                v.residential,
                v.commercial,
                v.industrial
            );
            self.income = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn town() -> City {
        let mut c = City::new(Terrain::flat(40, 40), None, Footprints::builtin(), 42);
        c.apply_tool(Tool::CoalPlant, (0, 0), (0, 0)).unwrap();
        c.apply_tool(Tool::PowerLine, (5, 1), (8, 2)).unwrap();
        c.apply_tool(Tool::Road, (8, 3), (30, 3)).unwrap();
        c.apply_tool(Tool::ZoneResidential, (8, 4), (16, 8)).unwrap();
        c.apply_tool(Tool::ZoneCommercial, (17, 4), (22, 8)).unwrap();
        c.apply_tool(Tool::ZoneIndustrial, (23, 4), (30, 8)).unwrap();
        c
    }

    #[test]
    fn tools_charge_the_fund() {
        let c = town();
        let spent = 2000 + 5 * 3 + 23 * 5 + 45 * 5 + 30 * 5 + 40 * 8;
        assert_eq!(c.fund, FUND_START - spent);
    }

    #[test]
    fn terrain_tool_refuses_near_buildings() {
        let mut c = town();
        assert_eq!(c.apply_tool(Tool::RaiseTerrain, (10, 5), (10, 5)), Err(ToolError::TerrainOccupied));
        assert!(c.apply_tool(Tool::RaiseTerrain, (35, 35), (35, 35)).is_ok());
    }

    #[test]
    fn a_powered_connected_town_grows() {
        let mut c = town();
        for _ in 0..3000 {
            c.simulate_round();
        }
        let v = c.values;
        assert!(v.residential > 0 && v.commercial > 0 && v.industrial > 0, "{v:?}");
        assert!(v.power < v.power_capacity);
    }

    #[test]
    fn calendar_and_bills() {
        let mut c = town();
        let fund = c.fund;
        c.update(MS_PER_DAY * 30.0 * 12.0 + 1.0);
        assert_eq!(c.date, Date { day: 1, month: 1, year: 1 });
        assert_ne!(c.fund, fund);
        assert_eq!(c.history.len(), HISTORY_LEN);
    }

    #[test]
    fn same_seed_same_city() {
        let (mut a, mut b) = (town(), town());
        for _ in 0..500 {
            a.simulate_round();
            b.simulate_round();
        }
        assert_eq!(a.layer, b.layer);
    }
}
