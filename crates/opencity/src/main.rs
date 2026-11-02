//! OpenCity: the game.

mod audio;
mod camera;
mod render;
mod ui;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use clap::Parser;
use macroquad::prelude::*;
use opencity_assets::{find_data_dir, Catalog, Settings};
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
    let window = Conf {
        window_title: "OpenCity".to_owned(),
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
    if let Some(h) = &params.heightmap {
        if h.is_relative() && !h.exists() {
            params.heightmap = Some(catalog.data_dir.join("heightmap").join(h));
        }
    }
    let m = opencity_mapgen::generate(&params)?;
    let terrain = Terrain::from_corner_heights(m.width, m.length, &m.corner_heights);
    Ok(City::new(terrain, Some(&m.tree_density), catalog.footprints(), seed))
}

fn import_geojson(args: &Args, path: &Path, catalog: &Catalog) -> Result<City> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let rules = match &args.rules {
        Some(r) => opencity_geo::Rules::parse(&std::fs::read_to_string(r)?)?,
        None => opencity_geo::Rules::builtin(),
    };
    let opts = opencity_geo::ImportOptions {
        date: args.date.as_deref().map(|d| opencity_geo::HistDate::parse(d).with_context(|| format!("bad date {d:?}"))).transpose()?,
        units_per_tile: args.units_per_tile,
        name: path.file_stem().map(|s| s.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let (mut city, report) = opencity_geo::import(&text, &rules, catalog.footprints(), &opts)?;
    city.meta.source = Some(path.display().to_string());
    log::info!("GeoJSON import:\n{report}");
    Ok(city)
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
    catalog: Catalog,
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
}

impl Game {
    fn say(&mut self, m: impl Into<String>) {
        self.message = Some((m.into(), get_time() + 3.0));
    }

    fn set_city(&mut self, city: City) {
        self.camera = OrbitCamera::new(city.width(), city.length());
        self.city = city;
        self.drag_start = None;
        self.query = None;
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
                    MenuItem::New => match new_city(&s.settings, macroquad::rand::rand() as u64, &self.catalog) {
                        Ok(c) => {
                            self.set_city(c);
                            self.say("New city");
                        }
                        Err(e) => self.say(format!("Cannot create city: {e}")),
                    },
                    MenuItem::Save => match save_city(&self.city, &save_path()) {
                        Ok(()) => self.say(format!("Saved to {}", save_path().display())),
                        Err(e) => self.say(format!("Save failed: {e:#}")),
                    },
                    MenuItem::Load => match load_city(&save_path(), &self.catalog) {
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

    fn frame(&mut self) -> bool {
        let mouse: Vec2 = mouse_position().into();
        let mouse_delta = mouse - self.last_mouse;
        self.last_mouse = mouse;

        if is_key_pressed(KeyCode::Escape) {
            if self.active.is_some() || self.query.is_some() {
                self.active = None;
                self.query = None;
                self.drag_start = None;
            } else {
                self.ui.menu_open = !self.ui.menu_open;
            }
        }
        if !self.ui.menu_open {
            if is_key_pressed(KeyCode::Space) {
                self.handle_action(Action::TogglePause);
            }
            if is_key_pressed(KeyCode::F) {
                self.handle_action(Action::ToggleFast);
            }
            if let Some(a) = tool_key() {
                self.handle_action(a);
            }
        }

        let ui_mouse = self.ui.has_mouse();
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
        self.city.update(get_frame_time().min(0.25) as f64 * 1000.0);
        self.city.speed = speed;

        let cam = self.camera.camera();
        let hover = if ui_mouse { None } else { pick_tile(&cam, &self.city.terrain, mouse) };

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
                match self.city.apply_tool(tool, start, end) {
                    Ok(o) => self.audio.play(o.sound),
                    Err(e) => self.say(e.to_string()),
                }
            }
        }

        // 3D scene.
        clear_background(Color::new(0.45, 0.62, 0.85, 1.0));
        set_camera(&cam);
        self.renderer.draw_city(&self.city);
        if let Some(Action::Tool(Some(tool))) = self.active {
            if let (Some(start), Some(cur)) = (self.drag_start, hover) {
                let code = tool.structure();
                if code.is_some_and(|c| c.is_path()) {
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
        (None, Some(g)) => import_geojson(&s.args, g, &catalog)?,
        (None, None) => new_city(&s.settings, s.args.seed.unwrap_or(s.settings.map.seed), &catalog)?,
    };
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
    let mut game = Game {
        camera: OrbitCamera::new(city.width(), city.length()),
        city,
        catalog,
        renderer,
        ui,
        audio,
        active: None,
        drag_start: None,
        query: None,
        message: None,
        last_mouse: mouse_position().into(),
        resume_speed: Speed::Normal,
    };
    if s.args.screenshot.is_some() {
        game.camera.distance *= 0.75;
        game.camera.pitch = 38f32.to_radians();
    }
    let mut frames = 0;
    loop {
        if !game.frame() {
            break;
        }
        frames += 1;
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
