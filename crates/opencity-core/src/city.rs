//! The city: terrain + buildings + economy + calendar (legacy `City`).

use std::collections::{BTreeMap, VecDeque};

use rand::{Rng, SeedableRng};
use rand_pcg::Pcg32;
use serde::{Deserialize, Serialize};

use crate::{
    glassbox::{GlassBox, Trip},
    layer::{BuildError, BuildingLayer, Query, Rect, Status},
    rules::Ruleset,
    sim::{self, SimValues},
    traffic::{find_path, find_route, Vehicle, VehicleKind, Ways, MAX_VEHICLES},
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
/// The hour of the 1st of each month (see [`City::hour`]).
pub const DAWN_HOUR: f64 = 8.0;
/// Years of history kept for the graphs.
pub const HISTORY_LEN: usize = 20;
pub const TERRAIN_COST: u32 = 5;
/// Cost of digging one tile of canal.
pub const CANAL_COST: u32 = 30;

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

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ToolError {
    #[error("the city is bankrupt")]
    Bankrupt,
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error("something is built too close")]
    TerrainOccupied,
    #[error("height limit reached")]
    TerrainLimit,
    #[error("needs research: {0}")]
    Locked(String),
    #[error("something else is built on the border tile")]
    BorderTaken,
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

/// The family that came with Cousin Vinnie's money.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mafia {
    /// Their hold on the city: 1 when called, gone at 0.
    pub heat: f32,
    /// Protection money paid so far.
    #[serde(default)]
    pub collected: i64,
}

/// The season of a month.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub fn of_month(month: u32) -> Self {
        match month {
            3..=5 => Self::Spring,
            6..=8 => Self::Summer,
            9..=11 => Self::Autumn,
            _ => Self::Winter,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Spring => "Spring",
            Self::Summer => "Summer",
            Self::Autumn => "Autumn",
            Self::Winter => "Winter",
        }
    }
}

/// Today's weather, drawn each morning from the season's odds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weather {
    #[default]
    Clear,
    Cloudy,
    Rain,
    Storm,
    Snow,
}

impl Weather {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Clear => "Clear",
            Self::Cloudy => "Cloudy",
            Self::Rain => "Rain",
            Self::Storm => "Storm",
            Self::Snow => "Snow",
        }
    }

    pub const fn icon(self) -> &'static str {
        match self {
            Self::Clear => "☀",
            Self::Cloudy => "☁",
            Self::Rain => "🌧",
            Self::Storm => "⛈",
            Self::Snow => "❄",
        }
    }

    /// Odds (percent) of clear, cloudy, rain, storm and snow in a season.
    const fn odds(season: Season) -> [u32; 5] {
        match season {
            Season::Spring => [40, 25, 28, 7, 0],
            Season::Summer => [62, 15, 12, 11, 0],
            Season::Autumn => [30, 32, 30, 6, 2],
            Season::Winter => [28, 30, 12, 2, 28],
        }
    }
}

/// A loan from the bank, repaid monthly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Loan {
    pub amount: i64,
    /// Monthly repayment, interest included.
    pub payment: i64,
    pub months_left: u32,
}

/// Yearly interest on loans, and their length in months.
pub const LOAN_RATE: f64 = 0.06;
pub const LOAN_MONTHS: u32 = 60;
/// Loans the bank grants at once.
pub const MAX_LOANS: usize = 3;

/// What the family collects each month at full heat.
pub const PROTECTION_MONEY: f32 = 20_000.0;

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
    /// Disasters that struck since the game last took them (notices).
    #[serde(skip)]
    pub notices: Vec<crate::glassbox::DisasterEvent>,
    /// Other news for the player (the family's visits…), oldest first.
    #[serde(skip)]
    pub messages: Vec<String>,
    /// Money this month on the city's own lines (loans, the family,
    /// classic maintenance and taxes); GlassBox lines are in its ledger.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ledger: BTreeMap<String, f64>,
    /// Last month's budget, every line together.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub last_ledger: BTreeMap<String, f64>,
    /// Today's weather.
    #[serde(default)]
    pub weather: Weather,
    /// The scenario being played and its goals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goals: Option<crate::goals::Goals>,
    /// Loans being repaid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loans: Vec<Loan>,
    /// Neighbours' border crossings already built here (regions), so one
    /// the player bulldozes is not built again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub border_links: Vec<crate::region::Crossing>,
    /// Cousin Vinnie's family, once called (see [`City::cheat`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mafia: Option<Mafia>,
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
    /// Traffic follows the hour of the day (rush hours, quiet nights) under
    /// rules whose resources have a tide; off, it is the same at any hour.
    #[serde(default = "yes")]
    pub tides: bool,
    /// Days are passing ([`City::update`]): rounds stepped by hand have no
    /// hour, so their traffic has no tide.
    #[serde(skip)]
    clocked: bool,
}

fn yes() -> bool {
    true
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
            notices: Vec::new(),
            messages: Vec::new(),
            border_links: Vec::new(),
            mafia: None,
            ledger: BTreeMap::new(),
            last_ledger: BTreeMap::new(),
            loans: Vec::new(),
            goals: None,
            weather: Weather::Clear,
            rng: Pcg32::seed_from_u64(seed),
            day_clock_ms: 0.0,
            sim_clock_ms: 0.0,
            footprints,
            glassbox: None,
            zone_density: 0,
            tides: true,
            clocked: false,
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
            Some(g) => {
                // The save numbered its units by another list: renumber
                // each building by its unit's id (one whose unit is gone
                // goes back to an empty plot).
                let old = g.attach(rules.clone(), w, l);
                if !old.is_empty() {
                    let origins: Vec<(u32, u32)> = self.layer.structures().filter(|(_, s)| s.unit.is_some()).map(|(p, _)| p).collect();
                    for (w, l) in origins {
                        let Some(s) = self.layer.structure_mut(w, l) else { continue };
                        s.unit = s.unit.and_then(|u| old.get(u as usize)).and_then(|id| rules.unit(id)).map(|u| u as _);
                    }
                }
            }
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
    /// The research project (by name) a tool still waits for: the tool's
    /// own id, or the unit a service tool builds.
    pub fn tool_locked_by(&self, tool: Tool) -> Option<String> {
        let gb = self.glassbox.as_ref()?;
        let rules = gb.rules()?;
        let unit = rules.services.get(tool.id()).map(String::as_str);
        [Some(tool.id()), unit].into_iter().flatten().find_map(|id| gb.locked_by(rules, id)).map(|t| t.name.clone())
    }

    /// Turns the plant or service at `(w, l)` a quarter clockwise, free of
    /// charge; returns its new facing.
    pub fn turn_building(&mut self, w: u32, l: u32) -> Result<u8, ToolError> {
        Ok(self.layer.turn_block(w, l, &self.terrain, &self.footprints)?)
    }

    /// Finishes every research project (demo towns, sandbox play).
    pub fn research_all(&mut self) {
        if let Some(gb) = self.glassbox.as_mut() {
            gb.research_all();
        }
    }

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

    /// Tiles a block structure placed at `(w, l)` would cover, turned to
    /// face the street as building it would.
    pub fn block_rect(&self, code: StructureCode, w: u32, l: u32) -> Rect {
        self.layer.block_fit(w, l, code, &self.terrain, &self.footprints).map(|(_, r)| r).unwrap_or_else(|_| {
            let (sw, sl) = BuildingLayer::block_size(code, &self.footprints);
            Rect::sized(w, l, sw, sl)
        })
    }

    /// Whether `tool` would succeed when anchored at `(w, l)` (build preview).
    pub fn can_place(&self, tool: Tool, w: u32, l: u32) -> bool {
        match tool.structure() {
            Some(code) if code.is_block() => self.layer.block_fit(w, l, code, &self.terrain, &self.footprints).is_ok(),
            Some(code) => self.layer.can_build(Rect::tile(w, l), code, &self.terrain),
            None => true,
        }
    }

    /// Builds a structure tool over a rectangle (a block at the corner `a`)
    /// and returns its cost, without charging it or checking research.
    pub(crate) fn build_tool(&mut self, tool: Tool, rect: Rect, a: (u32, u32)) -> Result<u32, ToolError> {
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
        Ok(cost)
    }

    /// Applies a tool between two corner tiles (identical for single-tile tools).
    pub fn apply_tool(&mut self, tool: Tool, a: (u32, u32), b: (u32, u32)) -> Result<ToolOutcome, ToolError> {
        if self.fund < 0 {
            return Err(ToolError::Bankrupt);
        }
        if let Some(t) = self.tool_locked_by(tool) {
            return Err(ToolError::Locked(t));
        }
        let rect = Rect::new(a, b);
        let cost = match tool {
            Tool::Destroy => self.layer.destroy(rect)?,
            Tool::Canal => self.dig_canal(rect)?,
            Tool::MetroLine => self.layer.lay_tunnel(rect)?,
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
            _ => self.build_tool(tool, rect, a)?,
        };
        self.fund -= cost as i64;
        Ok(ToolOutcome { cost, sound: tool.sound() })
    }

    /// The bulldozer in the underground view: digs up the water pipes and
    /// metro tunnels under the rectangle, leaving what stands on them.
    pub fn dig_up(&mut self, a: (u32, u32), b: (u32, u32)) -> Result<ToolOutcome, ToolError> {
        if self.fund < 0 {
            return Err(ToolError::Bankrupt);
        }
        let cost = self.layer.dig_up(Rect::new(a, b))?;
        self.fund -= cost as i64;
        Ok(ToolOutcome { cost, sound: Tool::Destroy.sound() })
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
        self.clocked = true;
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
        self.mafia_tick();
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
        self.spawn_trains();
        self.spawn_boats();
        self.values = sim::refresh_values(&self.layer);
    }

    fn simulate_glassbox(&mut self) {
        let mut gb = self.glassbox.take().unwrap();
        gb.hour = (self.tides && self.clocked).then(|| self.hour());
        let trips = gb.tick(&mut self.layer, &self.terrain, &self.footprints, &mut self.fund, &mut self.rng);
        // Road traffic comes from the trips (`GlassBox::congestion`).
        // A sample of the trips becomes visible vehicles.
        // Picked at random, so every resource shows its vehicles (not
        // just those listed first, like workers on foot).
        let visible: Vec<&Trip> = trips.iter().filter(|t| t.amount >= 1.0).collect();
        // What is shown follows the hour: fewer vehicles while little
        // moves, each in the direction it goes now. These draws have their
        // own dice, so what is shown never changes what happens.
        let mut show = <Pcg32 as rand::SeedableRng>::seed_from_u64(gb.tick);
        for _ in 0..if visible.is_empty() { 0 } else { 10 } {
            let t = visible[self.rng.gen_range(0..visible.len())];
            if self.vehicles.len() >= MAX_VEHICLES {
                break;
            }
            let moving = t.out + t.back;
            if moving < 1.0 && show.gen_range(0.0..1.0) >= moving {
                continue;
            }
            let home = show.gen_range(0.0..moving.max(1e-6)) >= t.out;
            let (from, to) = if home { (t.to, t.from) } else { (t.from, t.to) };
            let (Some(a), Some(b)) = (self.nearest_road(from), self.nearest_road(to)) else { continue };
            // The rules say which vehicles carry the resource and on which
            // ways; networks without vehicles get the classic cars.
            let (kind, ways, top, cost, mesh) = match gb.rules().map(|r| r.vehicles_for(t.resource)) {
                // Trains run on their own (see `spawn_trains`).
                // Trains, boats, buses and trams run on their own (see
                // `spawn_trains`, `spawn_line`).
                Some(fit) if fit.iter().any(|v| v.door_to_door()) => {
                    let fit: Vec<_> = fit.into_iter().filter(|v| v.door_to_door()).collect();
                    let total: f32 = fit.iter().map(|v| v.share).sum();
                    let mut x = self.rng.gen_range(0.0..total.max(1e-6));
                    let v = fit.iter().find(|v| {
                        x -= v.share;
                        x < 0.0
                    });
                    let v = v.unwrap_or(&fit[fit.len() - 1]);
                    let mesh = (!v.meshes.is_empty()).then(|| v.meshes[self.rng.gen_range(0..v.meshes.len())].clone());
                    (v.kind(), v.ways(), v.top_speed(), v.path_cost(), mesh)
                }
                Some(_) if gb.rules().is_some_and(|r| r.vehicle_network(t.resource)) => continue,
                _ => {
                    let k = VehicleKind::MOTOR[t.resource % 3];
                    (k, Ways::ALL, f32::INFINITY, k.path_cost(), None)
                }
            };
            if let Some(path) = find_route(&self.layer, &self.terrain, a, b, cost, &ways, top) {
                if path.len() > 1 {
                    let mut v = Vehicle::new(kind, &path, &self.layer, &self.terrain);
                    v.top_speed = top.is_finite().then_some(top);
                    v.mesh = mesh;
                    self.vehicles.push(v);
                }
            }
        }
        self.values = gb.values(&self.layer);
        self.glassbox = Some(gb);
        self.spawn_trains();
        self.spawn_boats();
        self.spawn_line("tram");
        self.spawn_line("bus");
        self.dispatch_services();
        self.respond_to_disasters();
        if let Some(gb) = &self.glassbox {
            self.notices.extend(gb.events.iter().cloned());
            // Research done: news for the player.
            if let Some(rules) = gb.rules() {
                for id in &gb.discoveries {
                    let name = rules.tech(id).map_or(id.as_str(), |t| rules.techs[t].name.as_str());
                    self.messages.push(format!("Discovered: {name}"));
                }
            }
        }
        let gb = self.glassbox.take().unwrap();
        if let Some(pop) = gb.rules().and_then(|r| r.population.as_deref().and_then(|p| r.resource(p))) {
            self.population = gb.local_total(pop) as u32;
        }
        self.glassbox = Some(gb);
    }

    /// A road tile next to (or near) `p`.
    fn nearest_road(&self, p: (u32, u32)) -> Option<(u32, u32)> {
        for r in 1..=3 {
            let (w1, l1, w2, l2) = self.terrain.around(p.0, p.1, r);
            let road = |w, l| self.layer.structure_at(w, l).is_some_and(|s| s.code == StructureCode::Road && !s.road.is_rail());
            if let Some(t) = (Rect { w1, l1, w2, l2 }).tiles().find(|&(w, l)| road(w, l)) {
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

    /// Trains between stations on one line: about one per station on the
    /// track at a time, drawn with the rules' rail vehicle (or a diesel).
    fn spawn_trains(&mut self) {
        let trains = self.vehicles.iter().filter(|v| v.top_speed.is_none() && v.on_rail()).count();
        let rail = crate::rail::Railways::find(&self.layer, &self.terrain, &self.footprints);
        let pairs = rail.connections();
        if pairs.is_empty() || trains >= rail.stations.len() || self.vehicles.len() >= MAX_VEHICLES || self.rng.gen_range(0..8) != 0 {
            return;
        }
        let (a, b) = pairs[self.rng.gen_range(0..pairs.len())];
        let (from, to) = (rail.stations[a].platform, rail.stations[b].platform);
        let meshes: Vec<String> = self
            .rules()
            .map(|r| r.networks.iter().flat_map(|n| &n.vehicles).filter(|v| v.ways().allows(crate::RoadKind::Rail)).flat_map(|v| v.meshes.clone()).collect())
            .unwrap_or_default();
        let mesh = if meshes.is_empty() { "kenney/train-kit/train-diesel-a".to_string() } else { meshes[self.rng.gen_range(0..meshes.len())].clone() };
        if let Some(path) = find_route(&self.layer, &self.terrain, from, to, crate::traffic::PathCost::Distance, &Ways::RAIL, f32::INFINITY) {
            if path.len() > 1 {
                let mut v = Vehicle::new(VehicleKind::Car, &path, &self.layer, &self.terrain);
                v.mesh = Some(mesh);
                self.vehicles.push(v);
            }
        }
    }

    /// Typed cheats (SimCity 3000's cheat box). `CallCousinVinnie` wires
    /// $1,000,000 to the city, and the family moves in: they collect
    /// protection money every month, crime rises (police coverage falls)
    /// and buildings catch fire, until police stations drive them out.
    /// Returns what happened, or `None` for an unknown code.
    pub fn cheat(&mut self, code: &str) -> Option<String> {
        let code: String = code.trim().trim_end_matches(';').chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
        match code.as_str() {
            "callcousinvinnie" => {
                self.fund += 1_000_000;
                let m = self.mafia.get_or_insert(Mafia { heat: 0.0, collected: 0 });
                m.heat = (m.heat + 1.0).min(3.0);
                Some("Cousin Vinnie wired $1,000,000. \"Don't worry about it. We'll be in touch.\"".into())
            }
            _ => None,
        }
    }

    /// Borrows from the bank: the money now, repaid over five years with
    /// interest. Fails when three loans are outstanding.
    pub fn take_loan(&mut self, amount: i64) -> Result<Loan, String> {
        if self.loans.len() >= MAX_LOANS {
            return Err(format!("The bank lends on at most {MAX_LOANS} loans at a time."));
        }
        if amount <= 0 {
            return Err("Borrow something.".into());
        }
        let total = amount as f64 * (1.0 + LOAN_RATE * LOAN_MONTHS as f64 / 12.0);
        let loan = Loan { amount, payment: (total / LOAN_MONTHS as f64).ceil() as i64, months_left: LOAN_MONTHS };
        self.fund += amount;
        self.book("Loans: borrowed", amount as f64);
        self.loans.push(loan.clone());
        Ok(loan)
    }

    fn book(&mut self, line: &str, amount: f64) {
        if amount != 0.0 {
            *self.ledger.entry(line.to_string()).or_default() += amount;
        }
    }

    /// This month so far, every line together.
    pub fn budget(&self) -> BTreeMap<String, f64> {
        let mut all = self.ledger.clone();
        if let Some(gb) = &self.glassbox {
            for (k, v) in &gb.ledger {
                *all.entry(k.clone()).or_default() += v;
            }
        }
        all
    }

    /// Month end: loan repayments, then this month becomes last month.
    fn close_month(&mut self) {
        let mut repaid = 0;
        for l in &mut self.loans {
            repaid += l.payment;
            l.months_left -= 1;
        }
        self.loans.retain(|l| l.months_left > 0);
        self.fund -= repaid;
        self.book("Loans: repayments", -(repaid as f64));
        self.last_ledger = self.budget();
        self.ledger.clear();
        if let Some(gb) = self.glassbox.as_mut() {
            gb.ledger.clear();
        }
    }

    /// The family's monthly visit: protection money, and police stations
    /// loosening their hold.
    fn mafia_month(&mut self) {
        let stations = self.layer.structures().filter(|(_, s)| s.code == StructureCode::PoliceDept).count() as f32;
        let Some(m) = self.mafia.as_mut() else { return };
        let money = (m.heat * PROTECTION_MONEY) as i64;
        self.fund -= money;
        m.collected += money;
        *self.ledger.entry("The family: protection".into()).or_default() -= money as f64;
        m.heat -= 0.01 + 0.04 * stations;
        if m.heat <= 0.0 {
            self.messages.push(format!("The family packs up and leaves town, ${} richer.", m.collected));
            self.mafia = None;
        } else {
            self.messages.push(format!("The family collected ${money} in protection money."));
        }
    }

    /// While the family is in town: crime (police coverage falls) and arson.
    fn mafia_tick(&mut self) {
        let Some(heat) = self.mafia.as_ref().map(|m| m.heat) else { return };
        let rules = self.rules().cloned();
        if let (Some(gb), Some(rules)) = (self.glassbox.as_mut(), rules) {
            if let Some(li) = rules.layer("police_safety") {
                gb.layers[li].iter_mut().for_each(|v| *v = (*v - heat * 3.0).max(0.0));
            }
        }
        if self.rng.gen_range(0.0..1.0) < heat / 2500.0 {
            let fire = self.rules().and_then(|r| r.disasters.iter().find(|d| d.effect == crate::rules::DisasterEffect::Destroy).map(|d| d.id.clone()));
            match fire {
                Some(id) => {
                    if let Some(e) = self.strike(&id) {
                        self.messages.push(format!("Arson at {}, {}: {} buildings burnt. Somebody didn't pay.", e.at.0, e.at.1, e.hit));
                    }
                }
                None => {
                    if let Some((w, l)) = self.layer.random_structure(StructureCode::Commercial, &mut self.rng) {
                        self.layer.destroy_tile(w, l);
                        self.messages.push(format!("Arson at {w}, {l}: a shop burnt down. Somebody didn't pay."));
                    }
                }
            }
        }
    }

    /// Sets off a disaster by hand at a likely spot; `None` when the rules
    /// have no such disaster or nothing it could strike.
    pub fn strike(&mut self, id: &str) -> Option<crate::glassbox::DisasterEvent> {
        let mut gb = self.glassbox.take()?;
        let e = gb.strike(id, None, &mut self.layer, &self.terrain, &self.footprints, &mut self.rng);
        self.glassbox = Some(gb);
        if let Some(e) = &e {
            self.respond_to_disasters();
            self.notices.push(e.clone());
        }
        e
    }

    /// The nearest service covering a disaster's protection layer sends
    /// its vehicle to the scene.
    fn respond_to_disasters(&mut self) {
        let Some(rules) = self.rules().cloned() else { return };
        let events = self.glassbox.as_ref().map(|g| g.events.clone()).unwrap_or_default();
        for e in events {
            let Some(layer) = rules.disaster(&e.id).and_then(|d| d.protection.clone()) else { continue };
            let responder = self
                .layer
                .structures()
                .filter_map(|(p, s)| s.unit.map(|u| (p, u as usize)))
                .filter(|(_, u)| *u < rules.units.len() && rules.units[*u].dispatch.is_some() && rules.units[*u].emits.contains_key(&layer))
                .min_by_key(|(p, _)| p.0.abs_diff(e.at.0) + p.1.abs_diff(e.at.1));
            let Some((p, u)) = responder else { continue };
            let d = rules.units[u].dispatch.clone().unwrap();
            let (Some(a), Some(b)) = (self.nearest_road(p), self.nearest_road(e.at)) else { continue };
            if let Some(path) = find_route(&self.layer, &self.terrain, a, b, crate::traffic::PathCost::Distance, &Ways::ALL, f32::INFINITY) {
                if path.len() > 1 && self.vehicles.len() < MAX_VEHICLES {
                    let mut v = Vehicle::new(VehicleKind::from_id(&d.model).unwrap_or(VehicleKind::Car), &path, &self.layer, &self.terrain);
                    v.mesh = d.meshes.first().cloned();
                    self.vehicles.push(v);
                }
            }
        }
    }

    /// Digs a canal along the path route of `r`: empty land with nothing
    /// built around it is lowered below the water line. Returns the cost.
    fn dig_canal(&mut self, r: Rect) -> Result<u32, ToolError> {
        let route = BuildingLayer::path_route(r);
        let mut cost = 0;
        for (w, l) in route {
            if self.terrain.is_water(w, l) {
                continue;
            }
            let (w1, l1, w2, l2) = self.terrain.around(w, l, 1);
            if self.layer.contains(Rect { w1, l1, w2, l2 }, crate::layer::Query::Any) {
                return Err(ToolError::TerrainOccupied);
            }
            for (x, y) in [(w, l), (w + 1, l), (w, l + 1), (w + 1, l + 1)] {
                if self.terrain.corner(x, y) >= 0 {
                    self.terrain.set_corner(x, y, -1);
                }
            }
            cost += CANAL_COST;
        }
        self.layer.touch();
        Ok(cost)
    }

    /// Boats between harbours on one body of water, drawn with the rules'
    /// water vehicle (or a small ship).
    fn spawn_boats(&mut self) {
        let boats = self.vehicles.iter().filter(|v| v.water).count();
        let water = crate::rail::Railways::water(&self.layer, &self.terrain, &self.footprints);
        let pairs = water.connections();
        if pairs.is_empty() || boats >= water.stations.len() || self.vehicles.len() >= MAX_VEHICLES || self.rng.gen_range(0..10) != 0 {
            return;
        }
        let (a, b) = pairs[self.rng.gen_range(0..pairs.len())];
        let Some(path) = water.route(self.width(), water.stations[a].platform, water.stations[b].platform) else { return };
        if path.len() < 2 {
            return;
        }
        let meshes: Vec<String> = self
            .rules()
            .map(|r| r.networks.iter().flat_map(|n| &n.vehicles).filter(|v| v.ways().sails()).flat_map(|v| v.meshes.clone()).collect())
            .unwrap_or_default();
        let mut v = Vehicle::boat(&path);
        v.mesh = Some(if meshes.is_empty() { "kenney/watercraft/ship-small".to_string() } else { meshes[self.rng.gen_range(0..meshes.len())].clone() });
        self.vehicles.push(v);
    }

    /// Buses or trams between stops on one line: about one per stop at a
    /// time, drawn with the rules' vehicle for the line (without one, none run).
    fn spawn_line(&mut self, id: &str) {
        let Some(def) = self.rules().and_then(|r| r.networks.iter().flat_map(|n| &n.vehicles).find(|v| v.line.as_deref() == Some(id))).cloned() else { return };
        let stop = if id == "tram" { StructureCode::TramStop } else { StructureCode::BusStop };
        let running = self.vehicles.iter().filter(|v| v.line.as_deref() == Some(id)).count();
        if self.vehicles.len() >= MAX_VEHICLES || !self.layer.structures().any(|(_, s)| s.code == stop) || self.rng.gen_range(0..6) != 0 {
            return;
        }
        let lines = if id == "tram" {
            crate::rail::Railways::trams(&self.layer, &self.terrain, &self.footprints)
        } else {
            crate::rail::Railways::buses(&self.layer, &self.terrain, &self.footprints)
        };
        let pairs = lines.connections();
        if pairs.is_empty() || running >= lines.stations.len() {
            return;
        }
        let (a, b) = pairs[self.rng.gen_range(0..pairs.len())];
        let (from, to) = (lines.stations[a].platform, lines.stations[b].platform);
        let ways = if id == "tram" { Ways::of([crate::RoadKind::Tram]) } else { def.ways() };
        if let Some(path) = find_route(&self.layer, &self.terrain, from, to, crate::traffic::PathCost::Distance, &ways, def.top_speed()) {
            if path.len() > 1 {
                let mut v = Vehicle::new(def.kind(), &path, &self.layer, &self.terrain);
                v.top_speed = def.speed;
                v.mesh = (!def.meshes.is_empty()).then(|| def.meshes[self.rng.gen_range(0..def.meshes.len())].clone());
                v.line = Some(id.to_string());
                self.vehicles.push(v);
            }
        }
    }

    /// Services with a `dispatch` send a vehicle now and then to one of
    /// the buildings they cover, along the streets.
    fn dispatch_services(&mut self) {
        let Some(rules) = self.rules().cloned() else { return };
        let services: Vec<((u32, u32), usize)> = self
            .layer
            .structures()
            .filter_map(|(p, s)| s.unit.map(|u| (p, u as usize)))
            .filter(|(_, u)| *u < rules.units.len() && rules.units[*u].dispatch.is_some())
            .collect();
        for (p, u) in services {
            let def = &rules.units[u];
            let d = def.dispatch.as_ref().unwrap();
            if self.vehicles.len() >= MAX_VEHICLES || self.rng.gen_range(0..d.every) != 0 {
                continue;
            }
            let reach = def.reach() as i64;
            let targets: Vec<(u32, u32)> = self
                .layer
                .structures()
                .filter(|(q, s)| s.unit.is_some() && *q != p && (q.0 as i64 - p.0 as i64).abs() + (q.1 as i64 - p.1 as i64).abs() <= reach)
                .map(|(q, _)| q)
                .collect();
            if targets.is_empty() {
                continue;
            }
            let t = targets[self.rng.gen_range(0..targets.len())];
            let (Some(a), Some(b)) = (self.nearest_road(p), self.nearest_road(t)) else { continue };
            let kind = VehicleKind::from_id(&d.model).unwrap_or(VehicleKind::Car);
            if let Some(path) = find_route(&self.layer, &self.terrain, a, b, crate::traffic::PathCost::Distance, &Ways::ALL, f32::INFINITY) {
                if path.len() > 1 {
                    let mut v = Vehicle::new(kind, &path, &self.layer, &self.terrain);
                    v.mesh = (!d.meshes.is_empty()).then(|| d.meshes[self.rng.gen_range(0..d.meshes.len())].clone());
                    self.vehicles.push(v);
                }
            }
        }
    }

    fn spawn_vehicle(&mut self, start: (u32, u32)) {
        if self.vehicles.len() >= MAX_VEHICLES {
            return;
        }
        let Some(dest) = self.layer.random_structure(StructureCode::Road, &mut self.rng) else { return };
        let kind = VehicleKind::MOTOR[self.rng.gen_range(0..3)];
        if let Some(path) = find_path(&self.layer, &self.terrain, start, dest, kind.path_cost()) {
            if path.len() > 1 {
                self.vehicles.push(Vehicle::new(kind, &path, &self.layer, &self.terrain));
            }
        }
    }

    /// The hour shown by day and night, 0 to 24. A game day passes in under
    /// two seconds, so the light turns once a month instead, from 8 in the
    /// morning on the 1st (a new city opens in daylight).
    pub fn hour(&self) -> f32 {
        let month = (self.date.day.saturating_sub(1) as f64 + self.day_clock_ms / MS_PER_DAY) / 30.0;
        (month * 24.0 + DAWN_HOUR) as f32 % 24.0
    }

    pub fn season(&self) -> Season {
        Season::of_month(self.date.month)
    }

    /// How the season and the weather change the odds of disasters that
    /// destroy (fire), flood and sicken.
    fn climate_odds(&self) -> [f32; 3] {
        let s = self.season();
        let fire = match (s, self.weather) {
            (_, Weather::Storm) => 2.0,
            (_, Weather::Rain | Weather::Snow) => 0.5,
            (Season::Summer, _) => 1.5,
            _ => 1.0,
        };
        let flood = match (s, self.weather) {
            (_, Weather::Storm) => 4.0,
            (_, Weather::Rain) => 2.5,
            (Season::Spring, _) => 1.5,
            (Season::Summer, Weather::Clear) => 0.3,
            _ => 1.0,
        };
        let sick = if s == Season::Winter { 1.6 } else { 0.8 };
        [fire, flood, sick]
    }

    fn next_day(&mut self) {
        self.date.day += 1;
        // Each week's weather, from the season's odds.
        if self.date.day % 7 != 1 {
            return self.after_weather();
        }
        let odds = Weather::odds(self.season());
        let mut roll = self.rng.gen_range(0..100u32);
        self.weather = [Weather::Clear, Weather::Cloudy, Weather::Rain, Weather::Storm, Weather::Snow]
            .into_iter()
            .zip(odds)
            .find(|(_, p)| {
                if roll < *p {
                    true
                } else {
                    roll -= p;
                    false
                }
            })
            .map_or(Weather::Clear, |(w, _)| w);
        self.after_weather();
    }

    /// Passes the weather's odds on to the simulation, then the rest of the day.
    fn after_weather(&mut self) {
        let odds = self.climate_odds();
        if let Some(gb) = self.glassbox.as_mut() {
            gb.climate = Some(odds);
        }
        self.end_of_day();
    }

    fn end_of_day(&mut self) {
        if self.date.day <= 30 {
            return;
        }
        self.date.day = 1;
        let v = self.values;
        if self.glassbox.is_none() {
            self.population = v.residential + v.commercial / 2 + v.industrial / 3;
        }
        self.date.month += 1;
        self.mafia_month();
        self.close_month();
        self.check_goals();
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
        self.book("Maintenance", -(maintenance as f64));
        let v = self.values;
        self.income += (v.residential * RESIDENTIAL_TAX + v.commercial * COMMERCIAL_TAX + v.industrial * INDUSTRIAL_TAX) / 100;
        if new_year {
            self.income += self.income * INCOME_HELP / 100;
            self.fund += self.income as i64;
            self.book("Taxes (yearly)", self.income as f64);
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
