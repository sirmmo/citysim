//! OpenCity: the game.

mod audio;
#[cfg(not(target_os = "android"))]
mod bake;
#[cfg(not(target_os = "android"))]
mod budget;
mod camera;
mod flows;
#[cfg(not(target_os = "android"))]
mod goalsui;
mod gpu;
#[cfg(not(target_os = "android"))]
mod graphs;
#[cfg(not(target_os = "android"))]
mod network;
#[cfg(not(target_os = "android"))]
mod policies;
mod region;
#[cfg(not(target_os = "android"))]
mod regionui;
mod render;
#[cfg(not(target_os = "android"))]
mod research;
mod roads;
#[cfg(not(target_os = "android"))]
mod rulesedit;
mod ui;
#[cfg(not(target_os = "android"))]
mod ui_egui;
mod vehicles;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use clap::Parser;
use macroquad::prelude::*;
use opencity_assets::{find_data_dir, Catalog, Settings, Skin};
use opencity_core::{save, scenario, BuildingLayer, City, Speed, Terrain, Tool};

use crate::audio::Audio;
use crate::camera::OrbitCamera;
use crate::render::{pick_tile, Renderer};
use crate::ui::{Action, MenuItem, Ui};

#[derive(Parser, Debug, Clone)]
#[command(version, about = "OpenCity, a 3D city simulation")]
struct Args {
    /// Data directory (the one containing graphism.conf).
    #[arg(long, env = "OPENCITY_DATA")]
    data_dir: Option<PathBuf>,
    /// Settings file (default: ~/.config/opencity/settings.toml).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Open a saved city.
    #[arg(long)]
    load: Option<PathBuf>,
    /// Build the city from GeoJSON / Overpass JSON (OpenFantasyMap, OpenHistoryMap, OSM).
    #[arg(long)]
    geojson: Option<PathBuf>,
    /// Tag mapping rules for --geojson (TOML).
    #[arg(long, requires = "geojson")]
    rules: Option<PathBuf>,
    /// With --geojson: the date to rebuild (YYYY[-MM[-DD]]).
    #[arg(long, requires = "geojson")]
    date: Option<String>,
    /// With --geojson: metres (or map units) per tile.
    #[arg(long, requires = "geojson")]
    units_per_tile: Option<f64>,
    /// Override the map seed for new cities.
    #[arg(long)]
    seed: Option<u64>,
    /// Lay out a demo town on the new city.
    #[arg(long)]
    demo: bool,
    /// Simulate this many years before showing the city.
    #[arg(long, default_value_t = 0)]
    simulate_years: u32,
    /// Disable sound effects.
    #[arg(long)]
    no_audio: bool,
    /// Render a few frames, save a PNG screenshot and exit.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// GlassBox game rules for a new city (e.g. "glassbox"); default: classic OpenCity rules.
    #[arg(long)]
    game_rules: Option<String>,
    /// Draw this GlassBox map layer over the city (screenshots).
    #[arg(long, hide = true)]
    overlay: Option<String>,
    /// Skin to use (default: chosen from the city's era or world).
    #[arg(long)]
    skin: Option<String>,
    /// With --geojson: the fantasy world the map belongs to (selects world skins).
    #[arg(long, requires = "geojson")]
    world: Option<String>,
    /// Render this many frames, print the average frame time and exit.
    #[arg(long)]
    bench_frames: Option<u32>,
    /// Open the new-city dialog with this skin preselected (screenshots, docs).
    #[arg(long, hide = true)]
    show_new_city: Option<String>,
    /// Open the GlassBox graphs on this preset (screenshots, docs).
    #[arg(long, hide = true)]
    show_graphs: Option<String>,
    /// Open the rules editor on the city's rules: a building, resource or
    /// network id shows it in the graph; `text` shows the text (screenshots, docs).
    #[arg(long, hide = true)]
    show_rules: Option<String>,
    /// Open a sidebar submenu, e.g. "water" (screenshots, docs).
    #[arg(long, hide = true)]
    show_tools: Option<String>,
    /// Show the resource network and its panel (screenshots, docs).
    #[arg(long, hide = true)]
    show_network: bool,
    /// Show the pipes underground (screenshots, docs).
    #[arg(long, hide = true)]
    underground: bool,
    /// Open the research window with nothing researched, researching this project (screenshots, docs).
    #[arg(long, hide = true)]
    show_research: Option<String>,
    /// Open the policy window (screenshots, docs).
    #[arg(long, hide = true)]
    show_policies: bool,
    /// Set the hour of day (screenshots, docs).
    #[arg(long, hide = true)]
    at_hour: Option<f32>,
    /// Set the weather: clear, cloudy, rain, storm, snow (screenshots, docs).
    #[arg(long, hide = true)]
    weather: Option<String>,
    /// Set the month, 1-12 (screenshots, docs).
    #[arg(long, hide = true)]
    month: Option<u32>,
    /// Join a region server at this URL (with --player and --slot x,y).
    #[arg(long)]
    join: Option<String>,
    #[arg(long, default_value = "player")]
    player: String,
    #[arg(long, default_value = "0,0")]
    slot: String,
    /// Start a scenario by id (see assets/scenarios).
    #[arg(long)]
    scenario: Option<String>,
    /// Play a pack (assets/packs/<id>.toml): its scenario with a skin and
    /// rules that cannot be changed. A pack.toml in the data folder does
    /// the same at every launch.
    #[arg(long)]
    pack: Option<String>,
    /// Play a region: open the folder (or its region.json), or create a
    /// 3×3 region of 64-tile maps there with the rules of --game-rules.
    #[arg(long)]
    region: Option<PathBuf>,
    /// Inspect this tile at start, "x,y" (screenshots, docs).
    #[arg(long, hide = true)]
    inspect: Option<String>,
    /// Look at the --inspect tile from this distance in tiles (screenshots, docs).
    #[arg(long, hide = true, requires = "inspect")]
    zoom: Option<f32>,
    /// Run scripts/bake-0ad.sh now, even if it ran before.
    #[arg(long)]
    bake_0ad: bool,
    /// Do not install the 0 A.D. art automatically on first launch.
    #[arg(long, conflicts_with = "bake_0ad")]
    no_auto_bake: bool,
    /// Use the classic interface made of the original OpenCity artwork.
    #[arg(long)]
    classic_ui: bool,
    /// Window size override, e.g. 1280x800.
    #[arg(long, value_parser = parse_size)]
    size: Option<(u32, u32)>,
}

fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s.split_once('x').ok_or("expected WIDTHxHEIGHT")?;
    Ok((w.parse().map_err(|_| "bad width")?, h.parse().map_err(|_| "bad height")?))
}

struct Startup {
    args: Args,
    settings: Settings,
    settings_path: Option<PathBuf>,
}

static STARTUP: OnceLock<Startup> = OnceLock::new();
static BENCH_T0: OnceLock<f64> = OnceLock::new();

/// Seconds spent per frame phase (benchmark breakdown).
static PHASES: std::sync::Mutex<[f64; 4]> = std::sync::Mutex::new([0.0; 4]);

fn phase(i: usize, since: f64) -> f64 {
    let now = macroquad::time::get_time();
    if let Ok(mut p) = PHASES.lock() {
        p[i] += now - since;
    }
    now
}

fn startup() -> &'static Startup {
    STARTUP.get_or_init(|| {
        let args = Args::parse();
        let settings_path = args.config.clone().or_else(Settings::default_path);
        let settings = settings_path
            .as_deref()
            .map(|p| {
                Settings::load(p).unwrap_or_else(|e| {
                    eprintln!("warning: {e}; using defaults");
                    Settings::default()
                })
            })
            .unwrap_or_default();
        Startup { args, settings, settings_path }
    })
}

fn window_conf() -> macroquad::conf::Conf {
    #[cfg(not(target_os = "android"))]
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let s = startup();
    let (w, h) = s.args.size.unwrap_or((s.settings.window.width, s.settings.window.height));
    let title = find_data_dir(s.args.data_dir.as_deref()).ok().and_then(|d| find_pack(&d, s.args.pack.as_deref())).map_or("OpenCity".to_owned(), |p| p.name);
    let window = Conf {
        window_title: title,
        window_width: w as i32,
        window_height: h as i32,
        fullscreen: cfg!(target_os = "android") || (s.settings.window.fullscreen && s.args.screenshot.is_none()),
        sample_count: s.settings.window.msaa as i32,
        window_resizable: true,
        high_dpi: true,
        ..Default::default()
    };
    // Terrain chunks are drawn as single meshes of up to 60k vertices.
    macroquad::conf::Conf { miniquad_conf: window, draw_call_vertex_capacity: 65_000, draw_call_index_capacity: 120_000, ..Default::default() }
}

/// Android: reads files packed in the APK (miniquad loads them synchronously there).
#[cfg(target_os = "android")]
fn apk_reader(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::cell::RefCell;
    use std::rc::Rc;
    let out: Rc<RefCell<Option<std::io::Result<Vec<u8>>>>> = Rc::new(RefCell::new(None));
    let slot = out.clone();
    let name = path.to_string_lossy().replace('\\', "/");
    macroquad::miniquad::fs::load_file(name.trim_start_matches("./"), move |r| {
        *slot.borrow_mut() = Some(r.map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string())));
    });
    let result = out.borrow_mut().take();
    result.unwrap_or_else(|| Err(std::io::Error::other("asset load did not complete")))
}

/// Border gates of the open region city: each crossing, and whether the
/// neighbour across has joined it (None: the edge of the region).
fn region_gates(r: &region::RegionSession, crossings: &[opencity_core::region::Crossing]) -> Vec<(opencity_core::region::Crossing, Option<bool>)> {
    let Some(slot) = r.slot else { return Vec::new() };
    crossings
        .iter()
        .map(|c| {
            let side = c.side as usize;
            let state = r.region.has_slot(slot, side).then(|| {
                let o = c.opposite();
                r.region.neighbour(slot, side).is_some_and(|n| n.crossings.iter().any(|x| x.side == o.side && x.at == o.at))
            });
            (c.clone(), state)
        })
        .collect()
}

/// The pack to play: `--pack <id>`, else the data folder's `pack.toml`.
fn find_pack(data_dir: &Path, id: Option<&str>) -> Option<opencity_assets::pack::Pack> {
    let found = match id {
        Some(id) => Some(opencity_assets::pack::Pack::load(data_dir, id)),
        None => opencity_assets::pack::Pack::launcher(data_dir),
    };
    found?.map_err(|e| log::warn!("pack: {e}")).ok()
}

/// Android apps can only write to their private data directory.
#[cfg(target_os = "android")]
fn save_path() -> PathBuf {
    PathBuf::from("/data/data/io.github.sirmmo.citysim/files/city.ocsave.json")
}

#[cfg(not(target_os = "android"))]
fn save_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("opencity").join("city.ocsave.json")
}

fn new_city(settings: &Settings, seed: u64, catalog: &Catalog) -> Result<City> {
    let mut params = settings.map.clone();
    params.seed = seed;
    new_city_with(params, catalog)
}

fn new_city_with(mut params: opencity_mapgen::MapParams, catalog: &Catalog) -> Result<City> {
    if let Some(h) = &params.heightmap {
        if h.is_relative() && !h.exists() {
            params.heightmap = Some(catalog.data_dir.join("heightmap").join(h));
        }
    }
    let m = opencity_mapgen::generate(&params)?;
    let terrain = Terrain::from_corner_heights(m.width, m.length, &m.corner_heights);
    Ok(City::new(terrain, Some(&m.tree_density), catalog.footprints(), params.seed))
}

fn import_geojson(path: &Path, date: Option<&str>, rules: Option<&Path>, units_per_tile: Option<f64>, world: Option<&str>, catalog: &Catalog) -> Result<City> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let rules = match rules {
        Some(r) => opencity_geo::Rules::parse(&std::fs::read_to_string(r).with_context(|| format!("reading {}", r.display()))?)?,
        None => opencity_geo::Rules::builtin(),
    };
    let opts = opencity_geo::ImportOptions {
        date: date.map(|d| opencity_geo::HistDate::parse(d).with_context(|| format!("bad date {d:?}"))).transpose()?,
        units_per_tile,
        world: world.map(str::to_string),
        name: path.file_stem().map(|s| s.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let (mut city, report) = opencity_geo::import(&text, &rules, catalog.footprints(), &opts)?;
    city.meta.source = Some(path.display().to_string());
    log::info!("GeoJSON import:\n{report}");
    Ok(city)
}

/// Saves made with GlassBox rules store only the rules id; attach the rules again.
fn attach_saved_rules(city: &mut City, rulesets: &[std::sync::Arc<opencity_core::rules::Ruleset>]) {
    if city.rules().is_none() {
        if let Some(id) = city.glassbox.as_ref().map(|g| g.rules_id.clone()) {
            match rulesets.iter().find(|r| r.id == id) {
                Some(r) => city.use_rules(r.clone()),
                None => log::warn!("rules {id} not found; the city will not simulate"),
            }
        }
    }
}

fn load_city(path: &Path, catalog: &Catalog) -> Result<City> {
    let f = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    Ok(save::load(std::io::BufReader::new(f), catalog.footprints())?)
}

fn save_city(city: &City, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let f = std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    save::save(city, std::io::BufWriter::new(f))?;
    Ok(())
}

fn tool_key() -> Option<Action> {
    let keys = [
        (KeyCode::X, Action::Tool(Some(Tool::Destroy))),
        (KeyCode::R, Action::Tool(Some(Tool::ZoneResidential))),
        (KeyCode::C, Action::Tool(Some(Tool::ZoneCommercial))),
        (KeyCode::I, Action::Tool(Some(Tool::ZoneIndustrial))),
        (KeyCode::P, Action::Tool(Some(Tool::Road))),
        (KeyCode::L, Action::Tool(Some(Tool::PowerLine))),
        (KeyCode::T, Action::Tool(Some(Tool::Trees))),
        (KeyCode::U, Action::Tool(Some(Tool::RaiseTerrain))),
        (KeyCode::J, Action::Tool(Some(Tool::LowerTerrain))),
        (KeyCode::Slash, Action::Query),
        (KeyCode::N, Action::Tool(None)),
    ];
    keys.iter().find(|(k, _)| is_key_pressed(*k)).map(|(_, a)| *a)
}

struct Game {
    city: City,
    /// Catalog with the active skin's models.
    catalog: Catalog,
    base_catalog: Catalog,
    skins: Vec<Skin>,
    skin: Option<Skin>,
    rulesets: Vec<std::sync::Arc<opencity_core::rules::Ruleset>>,
    /// GlassBox map layer drawn over the city.
    overlay: Option<usize>,
    /// `None`: pick automatically; `Some(None)`: no skin; `Some(Some(id))`: forced.
    skin_choice: Option<Option<String>>,
    renderer: Renderer,
    ui: Ui,
    audio: Audio,
    camera: OrbitCamera,
    active: Option<Action>,
    drag_start: Option<(u32, u32)>,
    query: Option<(Vec2, (u32, u32))>,
    message: Option<(String, f64)>,
    last_mouse: Vec2,
    resume_speed: Speed,
    hover: Option<(u32, u32)>,
    /// The GlassBox resource network on the map.
    flow_view: flows::FlowView,
    /// The underground view (pipes), switched on by hand (B); water tools
    /// show it too.
    underground: bool,
    flow_arcs: Vec<flows::Arc>,
    flow_key: Option<flows::ArcsKey>,
    flow_hover: Option<usize>,
    /// The region being played, if any.
    region: Option<region::RegionSession>,
    /// Day and night, seasons and weather on the map (View menu).
    day_night: bool,
    /// The sealed pack being played: its skin, rules and scenario are fixed.
    pack: Option<opencity_assets::pack::Pack>,
    /// The city's border crossings as last seen, to notice new ones.
    crossings: Vec<opencity_core::region::Crossing>,
    #[cfg(not(target_os = "android"))]
    egui: Option<ui_egui::EguiUi>,
    /// Background 0 A.D. art installation, if running.
    #[cfg(not(target_os = "android"))]
    bake: Option<std::sync::mpsc::Receiver<bake::Outcome>>,
}

impl Game {
    /// Where File > Save writes: the pack's own file while playing a pack.
    fn save_file(&self) -> PathBuf {
        match &self.pack {
            Some(p) => p.save_path(&save_path()),
            None => save_path(),
        }
    }

    /// Whether a pack forbids this command (other cities, skins, cheats…).
    #[cfg(not(target_os = "android"))]
    fn pack_forbids(&self, cmd: &ui_egui::Command) -> bool {
        use ui_egui::Command;
        let Some(p) = &self.pack else { return false };
        match cmd {
            Command::NewCity(..)
            | Command::ImportGeojson { .. }
            | Command::SetSkin(_)
            | Command::NewRegion { .. }
            | Command::OpenRegion(_)
            | Command::JoinRegion { .. }
            | Command::RegionCity(_) => true,
            Command::StartScenario(id) => *id != p.scenario,
            Command::Load(path) => *path != self.save_file(),
            Command::Cheat(_) => !p.cheats,
            Command::SetDisasters(_) | Command::Strike(_) => !p.disaster_menu,
            _ => false,
        }
    }

    fn say(&mut self, m: impl Into<String>) {
        self.message = Some((m.into(), get_time() + 3.0));
    }

    /// Turns a plant or service a quarter clockwise.
    fn turn_building(&mut self, w: u32, l: u32) {
        if let Err(e) = self.city.turn_building(w, l) {
            self.say(format!("Cannot turn it: {e}"));
        }
    }

    /// Starts a scenario: its city, rules, skin and goals (the pack's own
    /// skin and rules for the pack's scenario).
    fn start_scenario(&mut self, id: &str) {
        let dir = self.base_catalog.data_dir.clone();
        let scenario = match self.pack.as_ref().filter(|p| p.scenario == id) {
            Some(p) => p.scenario(&dir),
            None => opencity_assets::scenario::load(&dir, id),
        };
        let built = scenario.and_then(|s| opencity_assets::scenario::build(&dir, &s, self.base_catalog.footprints()).map(|c| (s, c)));
        match built {
            Ok((s, c)) => {
                self.region = None;
                self.overlay = None;
                self.skin_choice = s.start.skin.clone().map(|k| (k != "none").then_some(k));
                self.set_city(c);
                self.message = Some((format!("{}: {}", s.name, s.description), get_time() + 10.0));
            }
            Err(e) => self.say(format!("Cannot start the scenario: {e}")),
        }
    }

    /// Saves the open region city and records its summary; false on error.
    fn region_store(&mut self) -> bool {
        let Some(r) = self.region.as_mut() else { return false };
        let Some(slot) = r.slot else { return true };
        let name = r.name().unwrap_or_else(|| r.town_name(slot));
        let path = r.city_path(slot);
        let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        if let Err(e) = save_city(&self.city, &path) {
            self.say(format!("Save failed: {e:#}"));
            return false;
        }
        r.region.summarize(slot, &name, &file, &self.city);
        if let Err(e) = r.write() {
            self.say(format!("Save failed: {e:#}"));
            return false;
        }
        #[cfg(not(target_os = "android"))]
        if let Err(e) = r.push(slot) {
            self.say(format!("Region server unreachable: {e}"));
        }
        true
    }

    /// Opens (or founds) the city in a region slot, saving the open one.
    fn region_goto(&mut self, slot: [u32; 2]) {
        if self.region.as_ref().is_some_and(|r| r.slot == Some(slot)) {
            return;
        }
        if !self.region_store() {
            return;
        }
        if let Some(p) = self.region.as_ref().and_then(|r| r.other_player(slot)) {
            self.say(format!("{p} plays that town."));
            return;
        }
        if self.region.as_ref().is_some_and(|r| r.remote.is_some() && r.slot.is_some()) {
            self.say("Online, you play the town you claimed. Join again to claim another.");
            return;
        }
        let Some(r) = self.region.as_ref() else { return };
        let founded = r.city_path(slot).is_file();
        let city = if founded {
            load_city(&r.city_path(slot), &self.catalog)
        } else {
            let mut params = startup().settings.map.clone();
            params.width = r.region.map_size;
            params.length = r.region.map_size;
            params.seed = r.region.slot_seed(slot);
            new_city_with(params, &self.base_catalog).map(|mut c| {
                if let Some(rules) = r.region.rules.as_ref().and_then(|id| self.rulesets.iter().find(|x| &x.id == id).cloned()) {
                    c.use_rules(rules);
                }
                c.meta.name = Some(r.town_name(slot));
                c
            })
        };
        match city {
            Ok(c) => {
                let name = c.meta.name.clone().unwrap_or_else(|| r.town_name(slot));
                self.set_city(c);
                if let Some(r) = self.region.as_mut() {
                    r.slot = Some(slot);
                    r.deals.clear();
                    r.month = 0;
                }
                if !founded {
                    self.region_store();
                }
                self.say(if founded { format!("Opened {name}") } else { format!("Founded {name}") });
                // After the greeting, so news of a crossing is what shows.
                self.region_link();
            }
            Err(e) => self.say(format!("Cannot open the city: {e:#}")),
        }
    }

    fn region_new(&mut self, name: &str, size: [u32; 2], map_size: u32, rules: Option<String>) {
        let dir = region::regions_dir(&save_path()).join(region::folder_name(name));
        let seed = macroquad::rand::rand() as u64;
        let r = opencity_core::region::Region::new(name, size, map_size, seed, rules);
        match region::RegionSession::create(&dir, r) {
            Ok(s) => {
                self.region = Some(s);
                self.region_goto([0, 0]);
            }
            Err(e) => self.say(format!("Cannot create the region: {e:#}")),
        }
    }

    fn region_open(&mut self, dir: &Path) {
        let dir = if dir.is_file() { dir.parent().map(Path::to_path_buf).unwrap_or_default() } else { dir.to_path_buf() };
        match region::RegionSession::open(&dir) {
            Ok(s) => {
                let first = s.region.cities.iter().map(|c| c.slot).min_by_key(|s| (s[1], s[0])).unwrap_or([0, 0]);
                self.region = Some(s);
                self.region_goto(first);
            }
            Err(e) => self.say(format!("Cannot open the region: {e:#}")),
        }
    }

    /// Once a month the open city agrees its trade with the neighbours.
    fn region_month(&mut self) {
        let Some(r) = self.region.as_mut() else { return };
        let Some(slot) = r.slot else { return };
        let month = self.city.date.year * 12 + self.city.date.month;
        if month == r.month {
            return;
        }
        r.month = month;
        // Over the network: the neighbours' latest summaries, then ours.
        #[cfg(not(target_os = "android"))]
        if r.remote.is_some() {
            let name = r.name().unwrap_or_else(|| r.town_name(slot));
            let file = region::RegionSession::city_file(slot);
            if let Err(e) = r.pull() {
                log::warn!("region server: {e}");
            }
            r.region.summarize(slot, &name, &file, &self.city);
            if let Err(e) = r.push(slot) {
                log::warn!("region server: {e}");
            }
        }
        r.deals = r.region.trade(slot, &self.city);
        self.city.apply_trade(&r.deals);
        self.city.apply_culture(&r.region.culture_for(slot));
        // Crossings a neighbour built toward us since (over the network).
        self.region_link();
    }

    fn set_city(&mut self, mut city: City) {
        attach_saved_rules(&mut city, &self.rulesets);
        self.camera = OrbitCamera::new(city.width(), city.length());
        self.city = city;
        self.drag_start = None;
        self.query = None;
        self.crossings = self.city.crossings();
        self.apply_skin();
    }

    /// A road, track or power line just built to the edge of the map: in a
    /// region it runs on into the neighbour (recorded at once, and sent to
    /// the region server); on a lone map there is nothing beyond.
    fn watch_borders(&mut self) {
        let now = self.city.crossings();
        if now == self.crossings {
            return;
        }
        let new = now.iter().find(|c| !self.crossings.contains(c)).cloned();
        self.crossings = now;
        let Some(c) = new else { return };
        let (what, side) = (c.what(), c.side_name());
        let Some(r) = self.region.as_ref() else {
            let hint = if self.pack.is_some() { "" } else { " Play a region (File > New region… or Join region server…) to link towns." };
            self.say(format!("The {what} reaches the {side} edge of the map, but there is no region server: no town lies beyond.{hint}"));
            return;
        };
        let Some(slot) = r.slot else { return };
        let msg = if !r.region.has_slot(slot, c.side as usize) {
            format!("The {side} edge is the edge of the region: no town lies beyond.")
        } else if let Some(n) = r.region.neighbour(slot, c.side as usize) {
            let by = r.other_player(n.slot).map(|p| format!(" ({p}'s town; it appears there at their next update)")).unwrap_or_default();
            format!("Border crossing: the {what} runs on into {} at its {} edge{by}.", n.name, c.opposite().side_name())
        } else {
            format!("Border crossing: the {what} will run on into the town founded to the {side}.")
        };
        self.region_store();
        self.say(msg);
    }

    /// Builds the neighbours' crossings toward the open region city.
    fn region_link(&mut self) {
        let Some(r) = self.region.as_ref() else { return };
        let Some(slot) = r.slot else { return };
        let mut built = Vec::new();
        let mut failed = None;
        for (from, c) in r.region.incoming(slot) {
            match self.city.connect_border(&c) {
                Ok(true) => built.push((from, c)),
                Ok(false) => {}
                Err(e) => {
                    // Said once: the neighbour's crossing is not tried again.
                    failed = Some(format!("{from}'s {} cannot cross your {} edge at tile {}: {e}", c.what(), c.side_name(), c.at));
                    self.city.border_links.push(c);
                }
            }
        }
        self.crossings = self.city.crossings();
        if !built.is_empty() {
            // Our summary shows them too, so both sides read as joined.
            self.region_store();
        }
        match (built.as_slice(), failed) {
            ([(from, c)], _) => self.say(format!("Border crossing: {from}'s {} reaches your {} edge.", c.what(), c.side_name())),
            ([_, ..], _) => self.say(format!("Border crossings: {} links from your neighbours reach your edges.", built.len())),
            ([], Some(m)) => self.say(m),
            ([], None) => {}
        }
    }

    /// Chooses the skin for the current city and applies it.
    fn apply_skin(&mut self) {
        let meta = &self.city.meta;
        let skin = match &self.skin_choice {
            Some(None) => None,
            Some(Some(id)) => Skin::select(&self.skins, Some(id), None, None),
            None => Skin::select(&self.skins, None, meta.year, meta.world.as_deref()),
        }
        .cloned();
        self.catalog = match &skin {
            Some(s) => self.base_catalog.with_skin(s),
            None => self.base_catalog.clone(),
        };
        let look = skin.as_ref().map(|s| s.look.clone()).unwrap_or_default();
        self.renderer.set_skin(self.catalog.clone(), look);
        if let Some(s) = &skin {
            log::info!("skin: {}", s.name);
            if self.active.is_some_and(|a| matches!(a, Action::Tool(Some(t)) if !s.allows(t.id()))) {
                self.active = None;
            }
        }
        self.skin = skin;
    }

    fn handle_action(&mut self, a: Action) -> bool {
        match a {
            Action::Tool(t) => self.active = t.map(|t| Action::Tool(Some(t))),
            Action::Query => self.active = Some(Action::Query),
            Action::TogglePause => {
                if self.city.speed == Speed::Paused {
                    self.city.speed = self.resume_speed;
                } else {
                    self.resume_speed = self.city.speed;
                    self.city.speed = Speed::Paused;
                }
            }
            Action::OpenMenu => self.ui.menu_open = true,
            Action::ToggleFast => self.city.speed = if self.city.speed == Speed::Fast { Speed::Normal } else { Speed::Fast },
            Action::Menu(item) => {
                self.ui.menu_open = false;
                let s = startup();
                match item {
                    MenuItem::New if self.pack.is_some() => {
                        let id = self.pack.as_ref().map(|p| p.scenario.clone()).unwrap_or_default();
                        self.start_scenario(&id);
                    }
                    MenuItem::New => match new_city(&s.settings, macroquad::rand::rand() as u64, &self.catalog) {
                        Ok(c) => {
                            self.set_city(c);
                            self.say("New city");
                        }
                        Err(e) => self.say(format!("Cannot create city: {e}")),
                    },
                    MenuItem::Save => match save_city(&self.city, &self.save_file()) {
                        Ok(()) => self.say(format!("Saved to {}", self.save_file().display())),
                        Err(e) => self.say(format!("Save failed: {e:#}")),
                    },
                    MenuItem::Load => match load_city(&self.save_file(), &self.catalog) {
                        Ok(c) => {
                            self.set_city(c);
                            self.say("City loaded");
                        }
                        Err(e) => self.say(format!("Load failed: {e:#}")),
                    },
                    MenuItem::Quit => return false,
                    MenuItem::Close => {}
                }
            }
        }
        true
    }

    #[cfg(not(target_os = "android"))]
    fn run_command(&mut self, cmd: ui_egui::Command) -> bool {
        use ui_egui::Command;
        if self.pack_forbids(&cmd) {
            self.say("Not in this game");
            return true;
        }
        match cmd {
            Command::Act(a) => return self.handle_action(a),
            Command::SetSpeed(s) => self.city.speed = s,
            Command::NewCity(params, skin, rules) => match new_city_with(params, &self.base_catalog) {
                Ok(mut c) => {
                    if let Some(r) = rules.and_then(|id| self.rulesets.iter().find(|r| r.id == id).cloned()) {
                        c.use_rules(r);
                    }
                    self.overlay = None;
                    // The chosen era/culture dresses the new city (None: modern look).
                    self.skin_choice = Some(skin);
                    self.set_city(c);
                    self.say(match &self.skin {
                        Some(s) => format!("New city: {}", s.name),
                        None => "New city".to_string(),
                    });
                }
                Err(e) => self.say(format!("Cannot create city: {e:#}")),
            },
            Command::Save(p) => match save_city(&self.city, &p) {
                Ok(()) => self.say(format!("Saved to {}", p.display())),
                Err(e) => self.say(format!("Save failed: {e:#}")),
            },
            Command::Load(p) => match load_city(&p, &self.catalog) {
                Ok(c) => {
                    self.set_city(c);
                    self.say(format!("Opened {}", p.display()));
                }
                Err(e) => self.say(format!("Open failed: {e:#}")),
            },
            Command::ImportGeojson { path, date, rules } => match import_geojson(&path, date.as_deref(), rules.as_deref(), None, None, &self.base_catalog) {
                Ok(c) => {
                    self.set_city(c);
                    self.say(format!("Imported {}", path.display()));
                }
                Err(e) => self.say(format!("Import failed: {e:#}")),
            },
            Command::ExportGeojson(p) => {
                let json = opencity_geo::export(&self.city, &opencity_geo::ExportOptions::default());
                match std::fs::write(&p, json.to_string()) {
                    Ok(()) => self.say(format!("Exported {}", p.display())),
                    Err(e) => self.say(format!("Export failed: {e}")),
                }
            }
            Command::SetSkin(choice) => {
                self.skin_choice = choice;
                self.apply_skin();
            }
            Command::SetDensity(d) => self.city.zone_density = d,
            Command::SetOverlay(o) => self.overlay = o,
            Command::ResetCamera => self.camera = OrbitCamera::new(self.city.width(), self.city.length()),
            Command::CloseQuery => self.query = None,
            Command::Turn((w, l)) => self.turn_building(w, l),
            Command::SetTax(t, rate) => {
                if let Some(gb) = self.city.glassbox.as_mut() {
                    gb.set_tax_rate(&t, rate);
                }
            }
            Command::SetStance(p, st) => {
                if let Some(gb) = self.city.glassbox.as_mut() {
                    if !gb.set_stance(&p, &st) {
                        self.say("That stance needs research first");
                    }
                }
            }
            Command::SetResearch(t) => {
                if let Some(gb) = self.city.glassbox.as_mut() {
                    gb.set_research(t.as_deref());
                }
            }
            Command::SetFlows(v) => self.flow_view = v,
            Command::SetUnderground(on) => self.underground = on,
            Command::TryRules(rules) => {
                let name = rules.name.clone();
                self.city.use_rules(rules);
                self.say(format!("Trying the edited rules: {name}. Save them in the rules editor to keep them."));
            }
            Command::SaveRules(src) => {
                let dir = self.base_catalog.data_dir.clone();
                match src.write(&dir) {
                    Ok(()) => {
                        self.rulesets = opencity_assets::rules::load_all(&dir);
                        let id = src.id.clone();
                        if let Some(r) = self.rulesets.iter().find(|r| r.id == id).cloned() {
                            if self.city.rules().is_some_and(|c| c.id == id) {
                                self.city.use_rules(r);
                            }
                        }
                        #[cfg(not(target_os = "android"))]
                        if let Some(ui) = self.egui.as_mut() {
                            ui.rules_editor.saved();
                        }
                        self.say(format!("Saved the rules {id} to {}.", dir.join("rules").display()));
                    }
                    Err(e) => self.say(format!("Cannot save the rules: {e}")),
                }
            }
            Command::Inspect(tile) => {
                self.query = Some((self.last_mouse, tile));
                if let Some(o) = self.city.layer.origin_of(tile.0, tile.1) {
                    self.camera.target = flows::unit_centre(&self.city, o);
                }
            }
            Command::Quit => {
                self.region_store();
                // A pack carries on where it was left.
                if self.pack.is_some() {
                    if let Err(e) = save_city(&self.city, &self.save_file()) {
                        log::warn!("pack save: {e:#}");
                    }
                }
                return false;
            }
            Command::NewRegion { name, size, map_size, rules } => self.region_new(&name, size, map_size, rules),
            Command::OpenRegion(dir) => self.region_open(&dir),
            Command::JoinRegion { url, player, slot } => match region::join(&save_path(), &url, &player, slot) {
                Ok(s) => {
                    self.region = Some(s);
                    self.region_goto(slot);
                }
                Err(e) => self.say(format!("Cannot join: {e:#}")),
            },
            Command::RegionCity(slot) => self.region_goto(slot),
            Command::StartScenario(id) => self.start_scenario(&id),
            Command::Cheat(code) => match self.city.cheat(&code) {
                Some(m) => self.message = Some((m, get_time() + 8.0)),
                None => self.say("Nothing happens."),
            },
            Command::SetFunding(unit, f) => {
                if let Some(gb) = self.city.glassbox.as_mut() {
                    gb.set_funding(&unit, f);
                }
            }
            Command::TakeLoan(amount) => match self.city.take_loan(amount) {
                Ok(l) => self.say(format!("Borrowed ${}: ${} a month for {} months", l.amount, l.payment, l.months_left)),
                Err(e) => self.say(e),
            },
            Command::SetDayNight(on) => self.day_night = on,
            Command::SetDisasters(on) => {
                if let Some(gb) = self.city.glassbox.as_mut() {
                    gb.disasters_off = !on;
                }
            }
            Command::Strike(id) => {
                if self.city.strike(&id).is_none() {
                    self.say("Nothing for it to strike here");
                }
            }
            Command::SaveRegion => {
                if self.region_store() {
                    self.say("Region saved");
                }
            }
        }
        true
    }

    /// Picks up the result of the first-run art installation.
    #[cfg(not(target_os = "android"))]
    fn poll_bake(&mut self) {
        let Some(rx) = &self.bake else { return };
        match rx.try_recv() {
            Ok(outcome) => {
                self.bake = None;
                match outcome {
                    Ok(msg) => {
                        self.skins = Skin::load_all(&self.base_catalog.data_dir);
                        self.apply_skin();
                        let cultures = self.skins.iter().filter(|s| s.era == "Antiquity" && !s.models.is_empty()).count();
                        self.message = Some((format!("{msg}: {cultures} cultures in File > New city"), get_time() + 10.0));
                    }
                    Err(msg) => self.message = Some((msg, get_time() + 10.0)),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                let until = self.message.as_ref().map_or(0.0, |m| m.1);
                if until < get_time() + 1.0 {
                    self.message = Some(("Installing 0 A.D. art in the background (first run)…".into(), get_time() + 2.0));
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.bake = None,
        }
    }

    /// Rebuilds the network arcs when the city or the view changed, and
    /// finds the arc under the mouse.
    fn update_flows(&mut self, cam: &Camera3D, mouse: Vec2, ui_mouse: bool) {
        if !self.flow_view.on || self.city.glassbox.is_none() {
            self.flow_arcs.clear();
            self.flow_key = None;
            self.flow_hover = None;
            return;
        }
        let focus = self.query.map(|(_, t)| t);
        let tick = self.city.glassbox.as_ref().map_or(0, |g| g.tick);
        let key = (tick, self.flow_view.clone(), focus, self.city.layer.revision());
        if self.flow_key.as_ref() != Some(&key) {
            self.flow_arcs = flows::arcs(&self.city, &self.flow_view, focus);
            self.flow_key = Some(key);
        }
        self.flow_hover = if ui_mouse { None } else { flows::pick(&self.flow_arcs, cam, mouse) };
    }

    /// Tints buildings that lack a shown resource, by how much they lack.
    /// Whether the pipes and tunnels underground are shown (and
    /// bulldozed): by hand, or while a water or metro tool is in hand.
    fn underground_view(&self) -> bool {
        self.underground
            || matches!(
                self.active,
                Some(Action::Tool(Some(
                    Tool::WaterPipe | Tool::PumpingStation | Tool::WaterTower | Tool::WaterTreatment | Tool::MetroLine | Tool::MetroStation
                )))
            )
    }

    fn draw_shortages(&self) {
        let (Some(gb), Some(rules)) = (&self.city.glassbox, self.city.rules()) else { return };
        if !self.flow_view.shortages {
            return;
        }
        let w = self.city.width();
        for ((x, l), s) in self.city.layer.structures() {
            let Some(u) = s.unit.map(|u| u as usize).filter(|&u| u < rules.units.len()) else { continue };
            let def = &rules.units[u];
            let lack: f32 = gb
                .shortfall(rules, u, x, l, w)
                .iter()
                .filter(|(r, _)| self.flow_view.shows(rules, *r))
                .map(|(r, missing)| missing / def.needs.get(&rules.resources[*r].id).copied().unwrap_or(1.0).max(1e-6))
                .fold(0.0, f32::max);
            if lack > 0.0 {
                let (sw, sl) = s.size(self.city.footprints());
                let colour = Color::new(1.0, 0.15, 0.1, 0.25 + 0.45 * lack.min(1.0));
                self.renderer.draw_data_overlay_rect(&self.city.terrain, opencity_core::Rect::sized(x, l, sw, sl), colour);
            }
        }
    }

    fn frame(&mut self) -> bool {
        #[cfg(not(target_os = "android"))]
        self.poll_bake();
        let mouse: Vec2 = mouse_position().into();
        let mouse_delta = mouse - self.last_mouse;
        self.last_mouse = mouse;

        // The egui UI runs first so the map knows whether the pointer/keyboard are taken.
        #[allow(unused_mut)]
        let (mut egui_pointer, mut egui_keyboard, mut egui_on) = (false, false, false);
        #[cfg(not(target_os = "android"))]
        if let Some(mut ui) = self.egui.take() {
            egui_on = true;
            let message = self.message.as_ref().filter(|(_, until)| get_time() < *until).map(|(m, _)| m.clone());
            let view = ui_egui::View {
                city: &self.city,
                catalog: &self.catalog,
                active: self.active,
                hover: self.hover,
                query: self.query.map(|(_, t)| t),
                message: message.as_deref(),
                save_path: self.save_file(),
                pack: self.pack.as_ref(),
                skin: self.skin.as_ref(),
                skins: &self.skins,
                skin_choice: &self.skin_choice,
                rulesets: &self.rulesets,
                overlay: self.overlay,
                flows: &self.flow_view,
                flow_hover: self.flow_hover.and_then(|i| self.flow_arcs.get(i)).map(|a| a.stretch),
                underground: self.underground,
                region: self.region.as_ref(),
            };
            let mut commands = Vec::new();
            egui_macroquad::ui(|ctx| {
                commands = ui.show(ctx, &view);
                egui_pointer = ctx.wants_pointer_input() || ctx.is_pointer_over_area();
                egui_keyboard = ctx.wants_keyboard_input();
            });
            self.egui = Some(ui);
            for c in commands {
                if !self.run_command(c) {
                    return false;
                }
            }
        }

        if is_key_pressed(KeyCode::V) && !egui_keyboard && self.city.glassbox.is_some() {
            self.flow_view.on = !self.flow_view.on;
        }
        if is_key_pressed(KeyCode::B) && !egui_keyboard {
            self.underground = !self.underground;
        }
        if is_key_pressed(KeyCode::Escape) && !egui_keyboard {
            if self.active.is_some() || self.query.is_some() {
                self.active = None;
                self.query = None;
                self.drag_start = None;
            } else if !egui_on {
                self.ui.menu_open = !self.ui.menu_open;
            }
        }
        if !self.ui.menu_open && !egui_keyboard {
            if is_key_pressed(KeyCode::Space) {
                self.handle_action(Action::TogglePause);
            }
            if is_key_pressed(KeyCode::F) {
                self.handle_action(Action::ToggleFast);
            }
            // O turns the inspected plant or service.
            if is_key_pressed(KeyCode::O) {
                if let Some((_, (w, l))) = self.query {
                    self.turn_building(w, l);
                }
            }
            if let Some(a) = tool_key() {
                let allowed = |t: Tool| {
                    self.skin.as_ref().is_none_or(|s| s.allows(t.id()))
                        && self.city.rules().is_none_or(|r| r.allows(t.id()))
                        && self.city.tool_locked_by(t).is_none()
                };
                // P lays the street of the age: asphalt, else paved, else dirt.
                let a = match a {
                    Action::Tool(Some(Tool::Road)) => {
                        [Tool::Road, Tool::PavedStreet, Tool::DirtRoad].into_iter().find(|&t| allowed(t)).map(|t| Action::Tool(Some(t)))
                    }
                    Action::Tool(Some(t)) => allowed(t).then_some(a),
                    _ => Some(a),
                };
                if let Some(a) = a {
                    self.handle_action(a);
                }
            }
        }

        let ui_mouse = if egui_on { egui_pointer } else { self.ui.has_mouse() };
        let bounds = vec2(self.city.width() as f32, self.city.length() as f32);
        let gesture = !self.ui.menu_open && self.camera.handle_touch(bounds);
        if gesture {
            // A second finger cancels any tool drag.
            self.drag_start = None;
        } else if !self.ui.menu_open {
            self.camera.handle_input(bounds, mouse_delta, ui_mouse);
            // Without a tool, dragging with the left button / one finger pans.
            if self.active.is_none() && !ui_mouse && is_mouse_button_down(MouseButton::Left) && !is_mouse_button_pressed(MouseButton::Left) {
                self.camera.pan(-mouse_delta.x * 0.08, mouse_delta.y * 0.08, bounds);
            }
        }
        let speed = self.city.speed;
        if self.ui.menu_open {
            self.city.speed = Speed::Paused;
        }
        // Without the research window (classic interface, Android) the
        // city researches by itself, in the order the rules list projects.
        #[cfg(not(target_os = "android"))]
        let auto = self.egui.is_none();
        #[cfg(target_os = "android")]
        let auto = true;
        if let Some(gb) = self.city.glassbox.as_mut() {
            gb.auto_research = auto;
        }
        let t = get_time();
        self.city.update(get_frame_time().min(0.25) as f64 * 1000.0);
        self.region_month();
        self.watch_borders();
        // News from the city (the family's visits…).
        if let Some(m) = self.city.messages.pop() {
            self.city.messages.clear();
            self.message = Some((m, get_time() + 6.0));
        }
        // Disaster notices, with a look at the scene.
        if let Some(e) = self.city.notices.pop() {
            self.city.notices.clear();
            let lost = if e.hit == 1 { "1 building hit".to_string() } else { format!("{} buildings hit", e.hit) };
            self.message = Some((format!("{} at {}, {}: {lost}", e.name, e.at.0, e.at.1), get_time() + 6.0));
        }
        self.city.speed = speed;
        let t = phase(0, t);

        let cam = self.camera.camera();
        let hover = if ui_mouse { None } else { pick_tile(&cam, &self.city.terrain, mouse) };
        self.hover = hover;
        self.update_flows(&cam, mouse, ui_mouse);
        let t = phase(1, t);

        // Map interaction.
        if !ui_mouse && is_mouse_button_pressed(MouseButton::Left) {
            match self.active {
                Some(Action::Query) => self.query = hover.map(|h| (mouse, h)),
                Some(Action::Tool(Some(_))) => self.drag_start = hover,
                _ => {}
            }
        }
        if is_mouse_button_released(MouseButton::Left) {
            if let (Some(start), Some(Action::Tool(Some(tool)))) = (self.drag_start.take(), self.active) {
                let end = if tool.is_area() { hover.unwrap_or(start) } else { start };
                // Underground, the bulldozer digs up pipes and leaves the surface.
                let done = if tool == Tool::Destroy && self.underground_view() { self.city.dig_up(start, end) } else { self.city.apply_tool(tool, start, end) };
                match done {
                    Ok(o) => self.audio.play(o.sound),
                    Err(e) => self.say(e.to_string()),
                }
            }
        }

        // 3D scene.
        self.renderer.atmosphere = render::Atmosphere::of(self.city.hour(), self.city.season(), self.city.weather, self.day_night);
        let sky = self.renderer.look().sky;
        let sky = [sky[0] * self.renderer.atmosphere.sky.x, sky[1] * self.renderer.atmosphere.sky.y, sky[2] * self.renderer.atmosphere.sky.z];
        clear_background(Color::new(sky[0], sky[1], sky[2], 1.0));
        set_camera(&cam);
        self.renderer.draw_city(&self.city, &cam);
        if let Some(r) = self.region.as_ref() {
            render::draw_crossings(&self.city, &region_gates(r, &self.crossings));
        }
        if let (true, Some((rain, amount))) = (self.day_night, self.renderer.atmosphere.precipitation) {
            render::draw_precipitation(self.camera.target, get_time(), rain, amount);
        }
        phase(2, t);
        if let (Some(li), Some(gb), Some(rules)) = (self.overlay, &self.city.glassbox, self.city.rules()) {
            if let Some(def) = rules.layers.get(li) {
                let w = self.city.width();
                for l in 0..self.city.length() {
                    for x in 0..w {
                        let v = gb.layer_value(li, x, l, w) / def.max;
                        if v > 0.03 {
                            let c = def.colour;
                            self.renderer.draw_data_overlay(&self.city.terrain, x, l, Color::new(c[0], c[1], c[2], (v * 0.75).min(0.75)));
                        }
                    }
                }
            }
        }
        if self.underground_view() {
            // The surface fades back; the pipes show through it.
            set_default_camera();
            draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.03, 0.05, 0.09, 0.55));
            set_camera(&cam);
            self.renderer.draw_pipes(&self.city);
        }
        if self.flow_view.on {
            self.draw_shortages();
            self.renderer.draw_flows(&self.flow_arcs, &cam, get_time(), self.flow_hover);
        }
        if let Some(Action::Tool(Some(tool))) = self.active {
            if let (Some(start), Some(cur)) = (self.drag_start, hover) {
                let code = tool.structure();
                if code.is_some_and(|c| c.is_path()) || tool == Tool::MetroLine {
                    for (w, l) in BuildingLayer::path_route(opencity_core::Rect::new(start, cur)) {
                        self.renderer.draw_overlay(&self.city.terrain, opencity_core::Rect::tile(w, l), Color::new(1.0, 1.0, 0.2, 0.45));
                    }
                } else if tool.is_area() {
                    let color = if tool == Tool::Destroy { Color::new(1.0, 0.2, 0.1, 0.4) } else { Color::new(0.2, 0.6, 1.0, 0.4) };
                    self.renderer.draw_overlay(&self.city.terrain, opencity_core::Rect::new(start, cur), color);
                }
            } else if let Some((w, l)) = hover {
                let r = match tool.structure() {
                    Some(c) if c.is_block() => self.city.block_rect(c, w, l),
                    _ => opencity_core::Rect::tile(w, l),
                };
                let ok = self.city.can_place(tool, w, l);
                let color = if ok { Color::new(0.1, 1.0, 0.2, 0.45) } else { Color::new(1.0, 0.1, 0.1, 0.45) };
                self.renderer.draw_overlay(&self.city.terrain, r, color);
            }
        } else if let Some((w, l)) = hover {
            self.renderer.draw_overlay(&self.city.terrain, opencity_core::Rect::tile(w, l), Color::new(1.0, 1.0, 1.0, 0.25));
        }

        // 2D overlay.
        set_default_camera();
        if egui_on {
            #[cfg(not(target_os = "android"))]
            egui_macroquad::draw();
            return true;
        }
        let message = self.message.as_ref().filter(|(_, until)| get_time() < *until).map(|(m, _)| m.clone());
        if let Some((at, (w, l))) = self.query {
            let info = self.city.query(w, l).map(|i| {
                let name = self
                    .city
                    .layer
                    .structure_at(w, l)
                    .and_then(|s| self.catalog.get(s.graphic))
                    .and_then(|e| e.name.clone())
                    .unwrap_or_else(|| format!("{:?}", i.code));
                (name, i)
            });
            self.ui.draw_query(at + vec2(12.0, 12.0), info);
        }
        if let Some(a) = self.ui.draw(&self.city, self.active, message.as_deref()) {
            if !self.handle_action(a) {
                return false;
            }
        }
        if let Some((w, l)) = hover {
            draw_text(&format!("{w},{l}"), 8.0, screen_height() - 10.0, 20.0, WHITE);
        }
        true
    }
}

#[macroquad::main(window_conf)]
async fn main() -> Result<()> {
    #[cfg(target_os = "android")]
    opencity_assets::set_file_reader(apk_reader);
    let s = startup();
    let data_dir = find_data_dir(s.args.data_dir.as_deref())?;
    let catalog = Catalog::load(&data_dir)?;
    let mut city = match (&s.args.load, &s.args.geojson) {
        (Some(p), _) => load_city(p, &catalog)?,
        (None, Some(g)) => import_geojson(g, s.args.date.as_deref(), s.args.rules.as_deref(), s.args.units_per_tile, s.args.world.as_deref(), &catalog)?,
        (None, None) => new_city(&s.settings, s.args.seed.unwrap_or(s.settings.map.seed), &catalog)?,
    };
    let rulesets = opencity_assets::rules::load_all(&data_dir);
    if let Some(id) = &s.args.game_rules {
        match rulesets.iter().find(|r| &r.id == id) {
            Some(r) => city.use_rules(r.clone()),
            None => log::warn!("unknown rules {id}"),
        }
    }
    attach_saved_rules(&mut city, &rulesets);
    if s.args.demo {
        scenario::demo_town(&mut city);
    }
    let target = city.date.year + s.args.simulate_years;
    while city.date.year < target {
        city.update(opencity_core::city::MS_PER_DAY);
    }
    if let Some(p) = s.settings_path.as_ref().filter(|p| p.exists()) {
        log::info!("settings: {}", p.display());
    }
    let audio = if s.args.no_audio || !s.settings.audio.enabled || s.args.screenshot.is_some() {
        Audio::disabled()
    } else {
        Audio::load(&catalog, s.settings.audio.volume).await
    };
    let mut renderer = Renderer::new(catalog.clone());
    let ui = Ui::new(&mut renderer);
    let skins = Skin::load_all(&data_dir);
    let mut game = Game {
        camera: OrbitCamera::new(city.width(), city.length()),
        city,
        base_catalog: catalog.clone(),
        catalog,
        skins,
        skin: None,
        rulesets,
        overlay: None,
        skin_choice: s.args.skin.as_ref().map(|id| if id == "none" { None } else { Some(id.clone()) }),
        renderer,
        ui,
        audio,
        active: None,
        drag_start: None,
        query: None,
        message: None,
        last_mouse: mouse_position().into(),
        resume_speed: Speed::Normal,
        hover: None,
        flow_view: Default::default(),
        underground: false,
        flow_arcs: Vec::new(),
        flow_key: None,
        flow_hover: None,
        region: None,
        day_night: true,
        pack: find_pack(&data_dir, s.args.pack.as_deref()),
        crossings: Vec::new(),
        #[cfg(not(target_os = "android"))]
        egui: (!s.args.classic_ui).then(|| ui_egui::EguiUi::new(&s.settings.map)),
        // Never in automated runs (screenshots, benchmarks, CI).
        #[cfg(not(target_os = "android"))]
        bake: if s.args.no_auto_bake || s.args.screenshot.is_some() || s.args.bench_frames.is_some() { None } else { bake::start(&data_dir, s.args.bake_0ad) },
    };
    game.apply_skin();
    game.crossings = game.city.crossings();
    if let (Some(name), Some(rules)) = (&s.args.overlay, game.city.rules()) {
        game.overlay = rules.layer(name);
    }
    #[cfg(not(target_os = "android"))]
    if let (Some(id), Some(ui)) = (&s.args.show_new_city, game.egui.as_mut()) {
        ui.open_new_city(&game.skins, id);
    }
    #[cfg(not(target_os = "android"))]
    if let (Some(preset), Some(ui)) = (&s.args.show_graphs, game.egui.as_mut()) {
        ui.open_graphs(preset);
    }
    #[cfg(not(target_os = "android"))]
    if let (true, Some(ui)) = (s.args.show_policies, game.egui.as_mut()) {
        ui.open_policies();
    }
    if let Some(m) = s.args.month {
        game.city.date.month = m.clamp(1, 12);
    }
    if let Some(h) = s.args.at_hour {
        game.city.date.day = (((h - opencity_core::city::DAWN_HOUR as f32).rem_euclid(24.0) / 24.0 * 30.0) as u32 + 1).min(30);
    }
    if let Some(w) = &s.args.weather {
        use opencity_core::city::Weather;
        game.city.weather = match w.as_str() {
            "cloudy" => Weather::Cloudy,
            "rain" => Weather::Rain,
            "storm" => Weather::Storm,
            "snow" => Weather::Snow,
            _ => Weather::Clear,
        };
    }
    #[cfg(not(target_os = "android"))]
    if let Some(url) = &s.args.join {
        let slot: Vec<u32> = s.args.slot.split(',').filter_map(|v| v.trim().parse().ok()).collect();
        let slot = [slot.first().copied().unwrap_or(0), slot.get(1).copied().unwrap_or(0)];
        match region::join(&save_path(), url, &s.args.player, slot) {
            Ok(session) => {
                game.region = Some(session);
                game.region_goto(slot);
            }
            Err(e) => game.say(format!("Cannot join: {e:#}")),
        }
        if let Some(ui) = game.egui.as_mut() {
            ui.open_region();
        }
    }
    if let Some(pack) = game.pack.clone() {
        log::info!("pack: {}", pack.name);
        game.start_scenario(&pack.scenario);
        // Carry on from the pack's save, in the pack's skin.
        let saved = game.save_file();
        if saved.is_file() && s.args.load.is_none() {
            match load_city(&saved, &game.catalog) {
                Ok(c) => {
                    game.set_city(c);
                    game.say(format!("Welcome back to {}", pack.name));
                }
                Err(e) => game.say(format!("Cannot open the saved game: {e:#}")),
            }
        }
        #[cfg(not(target_os = "android"))]
        if let Some(ui) = game.egui.as_mut() {
            ui.open_goals();
        }
    } else if let Some(id) = &s.args.scenario {
        game.start_scenario(id);
        #[cfg(not(target_os = "android"))]
        if let Some(ui) = game.egui.as_mut() {
            ui.open_goals();
        }
    }
    if let Some(dir) = &s.args.region {
        if dir.join("region.json").is_file() || dir.is_file() {
            game.region_open(dir);
        } else {
            let name = dir.file_name().map_or("Region".to_string(), |n| n.to_string_lossy().into_owned());
            let r = opencity_core::region::Region::new(&name, [3, 3], 64, 7, s.args.game_rules.clone());
            match region::RegionSession::create(dir, r) {
                Ok(session) => {
                    game.region = Some(session);
                    game.region_goto([0, 0]);
                }
                Err(e) => log::warn!("region: {e:#}"),
            }
        }
        #[cfg(not(target_os = "android"))]
        if let Some(ui) = game.egui.as_mut() {
            ui.open_region();
        }
    }
    #[cfg(not(target_os = "android"))]
    if let Some((x, y)) = s.args.inspect.as_deref().and_then(|v| v.split_once(',')) {
        if let (Ok(x), Ok(y)) = (x.trim().parse(), y.trim().parse()) {
            game.run_command(ui_egui::Command::Inspect((x, y)));
            game.flow_view.focus_only = !s.args.show_network;
            if let Some(d) = s.args.zoom {
                game.camera.target = macroquad::prelude::vec3(x as f32 + 0.5, game.camera.target.y, y as f32 + 0.5);
                game.camera.distance = d;
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    if let (Some(tech), Some(ui)) = (&s.args.show_research, game.egui.as_mut()) {
        ui.open_research();
        // The demo town starts with everything researched: start over.
        if let Some(gb) = game.city.glassbox.as_mut() {
            gb.researched.clear();
            gb.set_research(Some(tech));
        }
    }
    #[cfg(not(target_os = "android"))]
    if let (Some(what), Some(id)) = (&s.args.show_rules, game.city.rules().map(|r| r.id.clone())) {
        let dir = game.base_catalog.data_dir.clone();
        if let Some(ui) = game.egui.as_mut() {
            ui.rules_editor.show(&dir, &id, what);
        }
    }
    #[cfg(not(target_os = "android"))]
    if let (Some(g), Some(ui)) = (&s.args.show_tools, game.egui.as_mut()) {
        ui.open_tools(g);
    }
    game.underground = s.args.underground;
    if s.args.show_network {
        game.flow_view.on = true;
        #[cfg(not(target_os = "android"))]
        if let Some(ui) = game.egui.as_mut() {
            ui.open_network();
        }
    }
    if s.args.screenshot.is_some() && s.args.zoom.is_none() {
        game.camera.distance *= 0.75;
        game.camera.pitch = 38f32.to_radians();
    }
    let mut frames = 0;
    let bench_start = get_time();
    loop {
        if !game.frame() {
            break;
        }
        frames += 1;
        if let Some(n) = s.args.bench_frames {
            if frames == 10 {
                BENCH_T0.set(get_time()).ok();
            }
            if frames == n + 10 {
                let t0 = BENCH_T0.get().copied().unwrap_or(bench_start);
                println!("bench: {n} frames, {:.2} ms/frame, {} structures", (get_time() - t0) * 1000.0 / n as f64, game.city.layer.structures().count());
                if let Ok(p) = PHASES.lock() {
                    let total = (n + 10) as f64;
                    println!(
                        "bench phases (ms/frame): simulate {:.2}, pick {:.2}, draw {:.2}",
                        p[0] * 1000.0 / total,
                        p[1] * 1000.0 / total,
                        p[2] * 1000.0 / total
                    );
                }
                break;
            }
        }
        if let Some(path) = &s.args.screenshot {
            if frames == 5 {
                let img = get_screen_data();
                let (w, h) = (img.width as u32, img.height as u32);
                let mut out = image::RgbaImage::from_raw(w, h, img.bytes).context("screenshot buffer")?;
                image::imageops::flip_vertical_in_place(&mut out);
                out.save(path)?;
                log::info!("screenshot saved to {}", path.display());
                break;
            }
        }
        next_frame().await;
    }
    Ok(())
}
