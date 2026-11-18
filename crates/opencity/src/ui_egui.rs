//! Desktop UI built with egui: menu bar, tool palette, city panel, dialogs.
//! (The classic UI made of the original OpenCity images lives in `ui.rs` and
//! is used on Android or with `--classic-ui`.)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui_macroquad::egui::{self, Color32, RichText, TextureHandle, Vec2 as EVec2};
use opencity_assets::{Catalog, Skin};
use std::sync::Arc;

use opencity_core::{rules::Ruleset, City, Speed, Status, StructureCode, Tool};
use opencity_mapgen::{MapParams, Relief, Shape, TreeDensity, Water};

use crate::ui::Action;

/// Requests from the UI to the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Act(Action),
    SetSpeed(Speed),
    /// New map, with the skin to dress it in (`None`: classic modern look).
    NewCity(MapParams, Option<String>, Option<String>),
    /// Zone density for the zone tools (GlassBox rules).
    SetDensity(u8),
    /// Map layer drawn over the city (GlassBox rules).
    SetOverlay(Option<usize>),
    Save(PathBuf),
    Load(PathBuf),
    ImportGeojson {
        path: PathBuf,
        date: Option<String>,
        rules: Option<PathBuf>,
    },
    /// `None`: automatic; `Some(None)`: no skin; `Some(Some(id))`: that skin.
    SetSkin(Option<Option<String>>),
    ExportGeojson(PathBuf),
    /// Write the city as open data (a zip of GeoJSON, CSV and GTFS files).
    ExportData(PathBuf),
    ResetCamera,
    CloseQuery,
    /// Research project to work on (`None`: stop).
    SetResearch(Option<String>),
    /// Tax rate, in percent.
    SetTax(String, f32),
    /// Stance taken on a policy.
    SetStance(String, String),
    /// Network view settings (GlassBox rules).
    SetFlows(crate::flows::FlowView),
    /// Show the pipes underground (the bulldozer then digs them up).
    SetUnderground(bool),
    /// Inspect the building on this tile and look at it.
    Inspect((u32, u32)),
    /// A new region: name, slots across and down, tiles per city map, rules.
    NewRegion {
        name: String,
        size: [u32; 2],
        map_size: u32,
        rules: Option<String>,
    },
    OpenRegion(PathBuf),
    /// Join a region server: its URL, the player's name, the town's slot.
    JoinRegion {
        url: String,
        player: String,
        slot: [u32; 2],
    },
    /// A typed cheat code.
    Cheat(String),
    /// Start a scenario by id.
    StartScenario(String),
    /// Funding of a service (unit id), 0..1.5.
    SetFunding(String, f32),
    /// Borrow from the bank.
    TakeLoan(i64),
    /// Show day and night, seasons and weather on the map; traffic then
    /// follows the hour too.
    SetDayNight(bool),
    /// Leave a sensor at a tile (GlassBox rules).
    AddSensor((u32, u32)),
    /// Take a sensor away, by name.
    RemoveSensor(String),
    /// Save what a sensor recorded as a CSV file.
    SaveSensorCsv(String),
    /// Whether the city can vote its mayor out.
    SetElections(bool),
    /// Terms of office with elections and promises (opt-in).
    SetTerms(bool),
    /// Make the campaign promise at this place in the offers.
    Promise(usize),
    /// A neighbourhood's name or policy.
    District(crate::districtsui::DistrictCmd),
    /// A change to the transit routes the player draws.
    Route(crate::routesui::RouteCmd),
    /// Voted out: back in office after a campaign, or (true) for life.
    StayInOffice(bool),
    /// Switch disasters by chance on or off.
    SetDisasters(bool),
    /// Set off a disaster by hand.
    Strike(String),
    /// Open or found the city in a region slot.
    RegionCity([u32; 2]),
    SaveRegion,
    /// Turn the plant or service on this tile a quarter clockwise.
    Turn((u32, u32)),
    /// Answer the open question with one of its choices.
    Decide(usize),
    /// Make an event of the rules happen now.
    Event(String),
    /// Events come up by themselves, or not.
    SetEvents(bool),
    /// Pull down the building at a tile (a relic, from its window).
    Dismantle((u32, u32)),
    /// Run the city on rules from the rules editor (not saved).
    TryRules(Arc<Ruleset>),
    /// Write a rules set's files from the rules editor.
    SaveRules(opencity_assets::rules::Sources),
    /// Make the report link of the holding and copy it.
    HoldingReport,
    /// Copy the report link again.
    HoldingCopy,
    /// Open the report link (OpenTactics).
    HoldingOpen,
    Quit,
}

/// What the UI needs to know about the game this frame.
pub struct View<'a> {
    pub city: &'a City,
    pub catalog: &'a Catalog,
    pub active: Option<Action>,
    pub hover: Option<(u32, u32)>,
    pub query: Option<(u32, u32)>,
    pub message: Option<&'a str>,
    pub save_path: PathBuf,
    /// The sealed pack being played, if any: its menus are locked.
    pub pack: Option<&'a opencity_assets::pack::Pack>,
    pub skin: Option<&'a Skin>,
    pub skins: &'a [Skin],
    pub skin_choice: &'a Option<Option<String>>,
    pub rulesets: &'a [Arc<Ruleset>],
    pub overlay: Option<usize>,
    pub flows: &'a crate::flows::FlowView,
    /// The network link under the mouse and its amount per tick.
    pub flow_hover: Option<opencity_core::netmap::Stretch>,
    /// The region being played, if any.
    pub region: Option<&'a crate::region::RegionSession>,
    /// The underground view is on (by hand).
    pub underground: bool,
    /// The OpenTactics holding this city is run for, if any.
    pub holding: Option<crate::holdingui::HoldingView<'a>>,
}

#[derive(Default)]
enum Dialog {
    #[default]
    None,
    NewCity,
    Open,
    SaveAs,
    Import,
    Export,
    ExportData,
    NewRegion,
    OpenRegion,
    NewScenario,
    JoinRegion,
}

pub struct EguiUi {
    icons: HashMap<&'static str, TextureHandle>,
    dialog: Dialog,
    new_map: MapParams,
    /// Era picked in the new-city dialog (`None`: modern) and the culture's skin id.
    new_era: Option<String>,
    new_skin: Option<String>,
    /// Rules set for the new city (`None`: classic OpenCity rules).
    new_rules: Option<String>,
    path: String,
    import_date: String,
    import_rules: String,
    show_city: bool,
    /// Size of the interface: `None` picks it from the screen.
    pub zoom: Option<f32>,
    graphs: crate::graphs::Graphs,
    show_network: bool,
    show_research: bool,
    pub rules_editor: crate::rulesedit::RulesEditor,
    show_policies: bool,
    show_region: bool,
    show_budget: bool,
    show_news: bool,
    pub routes: crate::routesui::RoutesUi,
    pub districts: crate::districtsui::DistrictsUi,
    show_election: bool,
    show_decision: bool,
    /// The question the decision window last opened for (its id and how many events came before).
    asked: Option<(String, usize)>,
    show_goals: bool,
    /// The OpenTactics window (holdings only).
    show_holding: bool,
    day_night: bool,
    /// Scenarios offered in File > New scenario… (loaded when it opens).
    scenarios: Vec<opencity_core::goals::Scenario>,
    scenario_pick: usize,
    /// The cheat box (Ctrl+Shift+C), as in SimCity 3000.
    cheat_open: bool,
    cheat_text: String,
    region_name: String,
    join_url: String,
    join_player: String,
    join_slot: [u32; 2],
    region_size: [u32; 2],
    region_map: u32,
    /// Skin the current style and icons were built for.
    styled_for: Option<Option<String>>,
    /// The sidebar submenu open beside it, and the last tool used in each.
    open_group: Option<&'static str>,
    last_tool: HashMap<&'static str, Action>,
}

type ToolEntry = (&'static str, Action, &'static str, &'static str);

/// A sidebar group: its name, its tools, and whether they sit in a submenu
/// that opens beside the sidebar.
type Group = (&'static str, &'static [ToolEntry], bool);

const GROUPS: &[Group] = &[
    (
        "Zones",
        &[
            ("residential", Action::Tool(Some(Tool::ZoneResidential)), "Residential", "R"),
            ("commercial", Action::Tool(Some(Tool::ZoneCommercial)), "Commercial", "C"),
            ("industrial", Action::Tool(Some(Tool::ZoneIndustrial)), "Industrial", "I"),
        ],
        false,
    ),
    (
        "Roads",
        &[
            ("road", Action::Tool(Some(Tool::Road)), "Street", "P"),
            ("road_highway", Action::Tool(Some(Tool::Highway)), "Highway", ""),
            ("road_paved_street", Action::Tool(Some(Tool::PavedStreet)), "Paved street", "P"),
            ("road_dirt_road", Action::Tool(Some(Tool::DirtRoad)), "Dirt road", ""),
            ("bus_stop", Action::Tool(Some(Tool::BusStop)), "Bus stop", ""),
        ],
        true,
    ),
    (
        "Rail",
        &[
            ("road_rail", Action::Tool(Some(Tool::Rail)), "Railway", ""),
            ("government", Action::Tool(Some(Tool::Station)), "Railway station", ""),
            ("tram_line", Action::Tool(Some(Tool::TramLine)), "Tram line", ""),
            ("tram_stop", Action::Tool(Some(Tool::TramStop)), "Tram stop", ""),
            ("metro_line", Action::Tool(Some(Tool::MetroLine)), "Metro line", ""),
            ("metro_station", Action::Tool(Some(Tool::MetroStation)), "Metro station", ""),
        ],
        true,
    ),
    (
        "Ports",
        &[
            ("government", Action::Tool(Some(Tool::Harbour)), "Harbour", ""),
            ("lower", Action::Tool(Some(Tool::Canal)), "Canal", ""),
            ("seaport", Action::Tool(Some(Tool::Seaport)), "Seaport", ""),
            ("airport", Action::Tool(Some(Tool::Airport)), "Airport", ""),
        ],
        true,
    ),
    (
        "Power",
        &[
            ("power_line", Action::Tool(Some(Tool::PowerLine)), "Power line", "L"),
            ("power_plant_coal", Action::Tool(Some(Tool::CoalPlant)), "Coal plant", ""),
            ("power_plant_nuclear", Action::Tool(Some(Tool::NuclearPlant)), "Nuclear plant", ""),
            ("power_plant_solar", Action::Tool(Some(Tool::SolarPlant)), "Solar farm", ""),
            ("power_plant_fusion", Action::Tool(Some(Tool::FusionPlant)), "Fusion plant", ""),
            ("power_plant_wind", Action::Tool(Some(Tool::WindTurbine)), "Wind turbine", ""),
        ],
        true,
    ),
    (
        "Water",
        &[
            ("water_pipe", Action::Tool(Some(Tool::WaterPipe)), "Water pipe", ""),
            ("water_pump", Action::Tool(Some(Tool::PumpingStation)), "Pumping station", ""),
            ("water_tower", Action::Tool(Some(Tool::WaterTower)), "Water tower", ""),
            ("water_works", Action::Tool(Some(Tool::Waterworks)), "Waterworks", ""),
            ("water_treatment", Action::Tool(Some(Tool::WaterTreatment)), "Sewage treatment plant", ""),
            ("sewage_outfall", Action::Tool(Some(Tool::SewageOutfall)), "Sewage outfall", ""),
            ("wetland", Action::Tool(Some(Tool::Wetland)), "Reed bed", ""),
        ],
        true,
    ),
    (
        "Waste",
        &[
            ("waste_dump", Action::Tool(Some(Tool::Dump)), "Landfill", ""),
            ("waste_recycling", Action::Tool(Some(Tool::Recycling)), "Recycling centre", ""),
            ("waste_incinerator", Action::Tool(Some(Tool::Incinerator)), "Incinerator", ""),
        ],
        true,
    ),
    (
        "Services",
        &[
            ("fire", Action::Tool(Some(Tool::FireDept)), "Fire department", ""),
            ("police", Action::Tool(Some(Tool::PoliceDept)), "Police", ""),
            ("hospital", Action::Tool(Some(Tool::Hospital)), "Hospital", ""),
            ("education", Action::Tool(Some(Tool::School)), "School", ""),
            ("high_school", Action::Tool(Some(Tool::HighSchool)), "High school", ""),
            ("mayors_office", Action::Tool(Some(Tool::MayorsOffice)), "Mayor's office", ""),
            ("city_hall", Action::Tool(Some(Tool::CityHall)), "City hall", ""),
            ("neighbourhood_office", Action::Tool(Some(Tool::NeighbourhoodOffice)), "Neighbourhood office", ""),
            ("university", Action::Tool(Some(Tool::University)), "University", ""),
        ],
        true,
    ),
    (
        "Amenities",
        &[
            ("museum", Action::Tool(Some(Tool::Museum)), "Museum", ""),
            ("archaeology_museum", Action::Tool(Some(Tool::ArchaeologyMuseum)), "Archaeology museum", ""),
            ("modern_art_museum", Action::Tool(Some(Tool::ModernArtMuseum)), "Modern art museum", ""),
            ("opera_house", Action::Tool(Some(Tool::OperaHouse)), "Opera house", ""),
            ("stadium", Action::Tool(Some(Tool::Stadium)), "Stadium", ""),
            ("mall", Action::Tool(Some(Tool::Mall)), "Shopping mall", ""),
            ("resort_hotel", Action::Tool(Some(Tool::ResortHotel)), "Resort hotel", ""),
            ("observation_tower", Action::Tool(Some(Tool::ObservationTower)), "Observation tower", ""),
            ("amusement_park", Action::Tool(Some(Tool::AmusementPark)), "Amusement park", ""),
        ],
        true,
    ),
    (
        "Land",
        &[
            ("park_city", Action::Tool(Some(Tool::Park)), "Park", ""),
            ("tree", Action::Tool(Some(Tool::Trees)), "Trees", "T"),
            ("raise", Action::Tool(Some(Tool::RaiseTerrain)), "Raise terrain", "U"),
            ("lower", Action::Tool(Some(Tool::LowerTerrain)), "Lower terrain", "J"),
        ],
        true,
    ),
    ("Tools", &[("bulldozer", Action::Tool(Some(Tool::Destroy)), "Bulldozer", "X"), ("query", Action::Query, "Inspect", "/")], false),
];

/// Water, waste, buses, trams, higher schools and amenities work only under GlassBox rules.
fn needs_rules(t: Tool) -> bool {
    matches!(
        t,
        Tool::WaterPipe
            | Tool::PumpingStation
            | Tool::WaterTower
            | Tool::WaterTreatment
            | Tool::Waterworks
            | Tool::SewageOutfall
            | Tool::Wetland
            | Tool::Dump
            | Tool::Recycling
            | Tool::Incinerator
            | Tool::TramLine
            | Tool::BusStop
            | Tool::TramStop
            | Tool::MetroLine
            | Tool::MetroStation
            | Tool::HighSchool
            | Tool::University
            | Tool::Stadium
            | Tool::ResortHotel
            | Tool::ObservationTower
            | Tool::AmusementPark
            | Tool::MayorsOffice
            | Tool::CityHall
            | Tool::NeighbourhoodOffice
            | Tool::Mall
            | Tool::Museum
            | Tool::ModernArtMuseum
            | Tool::OperaHouse
            | Tool::ArchaeologyMuseum
            | Tool::SolarPlant
            | Tool::FusionPlant
            | Tool::WindTurbine
    )
}

fn tool_cost(a: Action) -> Option<u32> {
    match a {
        Action::Tool(Some(Tool::RaiseTerrain | Tool::LowerTerrain)) => Some(opencity_core::city::TERRAIN_COST),
        Action::Tool(Some(t)) => match t.road_kind() {
            Some(k) => Some(k.build_cost()),
            None => t.structure().map(|s| s.build_cost()).filter(|&c| c > 0),
        },
        _ => None,
    }
}

impl EguiUi {
    /// How much bigger than its usual size the interface is drawn, on top
    /// of the screen's pixel density: what the player chose, else none.
    pub fn zoom_factor(&self) -> f32 {
        self.zoom.unwrap_or(1.0)
    }

    pub fn new(settings_map: &MapParams) -> Self {
        Self {
            icons: HashMap::new(),
            dialog: Dialog::None,
            new_map: settings_map.clone(),
            new_era: None,
            new_skin: None,
            new_rules: Some("glassbox".into()),
            path: String::new(),
            import_date: String::new(),
            import_rules: String::new(),
            show_city: macroquad::window::screen_width() / macroquad::window::screen_dpi_scale() >= 900.0,
            zoom: crate::ui::USER_SCALE.get().copied(),
            graphs: Default::default(),
            show_network: false,
            show_research: false,
            rules_editor: Default::default(),
            open_group: None,
            last_tool: HashMap::new(),
            show_policies: false,
            show_region: false,
            show_budget: false,
            show_news: false,
            routes: Default::default(),
            districts: Default::default(),
            show_election: false,
            show_decision: false,
            asked: None,
            show_goals: false,
            show_holding: true,
            day_night: true,
            scenarios: Vec::new(),
            scenario_pick: 0,
            cheat_open: false,
            cheat_text: String::new(),
            region_name: "My region".into(),
            join_url: "http://localhost:8077".into(),
            join_player: std::env::var("USER").unwrap_or_else(|_| "player".into()),
            join_slot: [0, 0],
            region_size: [3, 3],
            region_map: 64,
            styled_for: None,
        }
    }

    /// Opens the new-city dialog with a skin preselected.
    pub fn open_new_city(&mut self, skins: &[Skin], skin_id: &str) {
        self.dialog = Dialog::NewCity;
        if let Some(s) = skins.iter().find(|s| s.id == skin_id) {
            self.new_era = Some(s.era.clone());
            self.new_skin = Some(s.id.clone());
        }
    }

    fn style(&mut self, ctx: &egui::Context, skin: Option<&Skin>) {
        let key = Some(skin.map(|s| s.id.clone()));
        if self.styled_for == key {
            return;
        }
        self.styled_for = key;
        self.icons.clear();
        let look = skin.map(|s| s.look.clone()).unwrap_or_default();
        let ([pr, pg, pb], [ar, ag, ab]) = (look.ui_panel, look.ui_accent);
        let mut v = egui::Visuals::dark();
        v.window_corner_radius = 8.into();
        v.panel_fill = Color32::from_rgba_unmultiplied(pr, pg, pb, 235);
        v.window_fill = Color32::from_rgb(pr.saturating_add(6), pg.saturating_add(7), pb.saturating_add(9));
        v.selection.bg_fill = Color32::from_rgb(ar, ag, ab);
        ctx.set_visuals(v);
        ctx.style_mut(|s| {
            s.spacing.item_spacing = EVec2::new(6.0, 5.0);
            s.spacing.button_padding = EVec2::new(6.0, 4.0);
        });
    }

    fn icon(&mut self, ctx: &egui::Context, catalog: &Catalog, skin: Option<&Skin>, name: &'static str) -> Option<TextureHandle> {
        if let Some(t) = self.icons.get(name) {
            return Some(t.clone());
        }
        let rel = format!("graphism/gui/{name}.png");
        let path = match skin {
            Some(s) => s.resolve(&catalog.data_dir, &rel),
            None => catalog.path(&rel),
        };
        let img = match opencity_assets::read_bytes(&path).ok().and_then(|b| image::load_from_memory(&b).ok()) {
            Some(img) => img.to_rgba8(),
            // Road kinds without their own icon: the street icon, repainted.
            None => {
                let kind = name.strip_prefix("road_").and_then(opencity_core::RoadKind::from_id)?;
                let rel = "graphism/gui/road.png";
                let path = skin.map_or_else(|| catalog.path(rel), |s| s.resolve(&catalog.data_dir, rel));
                let mut img = image::load_from_memory(&opencity_assets::read_bytes(&path).ok()?).ok()?.to_rgba8();
                repaint_road_icon(&mut img, kind);
                img
            }
        };
        let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        let tex = ctx.load_texture(name, color, egui::TextureOptions::LINEAR);
        self.icons.insert(name, tex.clone());
        Some(tex)
    }

    /// SimCity 3000's cheat box: Ctrl+Shift+C opens a line to type into.
    fn cheat_box(&mut self, ctx: &egui::Context, out: &mut Vec<Command>) {
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::C)) {
            self.cheat_open = !self.cheat_open;
            self.cheat_text.clear();
        }
        if !self.cheat_open {
            return;
        }
        egui::Window::new("Enter cheat code").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 60.0]).show(ctx, |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.cheat_text).desired_width(260.0));
            r.request_focus();
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                out.push(Command::Cheat(std::mem::take(&mut self.cheat_text)));
                self.cheat_open = false;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.cheat_open = false;
            }
        });
    }

    pub fn open_goals(&mut self) {
        self.show_goals = true;
    }

    pub fn open_region(&mut self) {
        self.show_region = true;
    }

    pub fn open_election(&mut self) {
        self.show_election = true;
    }

    pub fn open_routes(&mut self) {
        self.routes.open = true;
    }

    pub fn open_news(&mut self) {
        self.show_news = true;
    }

    pub fn open_policies(&mut self) {
        self.show_policies = true;
    }

    /// Opens a sidebar submenu by its group name (screenshots, docs).
    pub fn open_tools(&mut self, group: &str) {
        self.open_group = GROUPS.iter().find(|g| g.0.eq_ignore_ascii_case(group)).map(|g| g.0);
    }

    pub fn open_research(&mut self) {
        self.show_research = true;
    }

    pub fn open_network(&mut self) {
        self.show_network = true;
    }

    /// Opens the GlassBox graphs on a preset (by name, e.g. "Growth churn").
    pub fn open_graphs(&mut self, preset: &str) {
        self.graphs.open_on(preset);
    }

    /// Builds the UI; returns the commands issued this frame.
    pub fn show(&mut self, ctx: &egui::Context, view: &View) -> Vec<Command> {
        self.style(ctx, view.skin);
        let mut out = Vec::new();
        if view.pack.is_none_or(|p| p.cheats) {
            self.cheat_box(ctx, &mut out);
        }
        self.menu_bar(ctx, view, &mut out);
        self.tools(ctx, view, &mut out);
        if self.show_city {
            self.city_panel(ctx, view, &mut out);
        }
        self.status_bar(ctx, view);
        self.query_window(ctx, view, &mut out);
        let export_dir = view.save_path.parent().map(Path::to_path_buf).unwrap_or_default();
        self.graphs.show(ctx, view.city.glassbox.as_ref(), &export_dir);
        if self.show_network {
            crate::network::window(ctx, &mut self.show_network, view.city, view.flows, view.query, &mut out);
        }
        if let (true, Some(r)) = (self.show_region, view.region) {
            crate::regionui::window(ctx, &mut self.show_region, r, view.city, &mut out);
        }
        if self.show_goals {
            crate::goalsui::window(ctx, &mut self.show_goals, view.city);
        }
        if let (true, Some(h)) = (self.show_holding, view.holding.as_ref()) {
            crate::holdingui::window(ctx, &mut self.show_holding, h, &mut out);
        }
        if self.show_election {
            crate::election::window(ctx, &mut self.show_election, view.city, &mut out);
        }
        // A new question opens its window by itself, once.
        let asking = view.city.affairs.pending.as_ref().map(|p| (p.id.clone(), view.city.affairs.past.len()));
        if asking != self.asked {
            self.show_decision = asking.is_some();
            self.asked = asking;
        }
        if self.show_decision {
            crate::affairsui::window(ctx, &mut self.show_decision, view.city, &mut out);
        }
        if self.show_news {
            crate::news::window(ctx, &mut self.show_news, view.city, &mut out);
        }
        if self.districts.open {
            self.districts.window(ctx, view.city, &mut out);
        }
        if self.routes.open {
            self.routes.window(ctx, view.city, &mut out);
        }
        if self.show_budget {
            crate::budget::window(ctx, &mut self.show_budget, view.city, &mut out);
        }
        if self.show_policies {
            crate::policies::window(ctx, &mut self.show_policies, view.city, &mut out);
        }
        if self.rules_editor.open {
            let ids: Vec<String> = view.rulesets.iter().map(|r| r.id.clone()).collect();
            let current = view.city.rules().map(|r| r.id.clone());
            self.rules_editor.window(ctx, &view.catalog.data_dir, &ids, current.as_deref(), &mut out);
        }
        if self.show_research {
            crate::research::window(ctx, &mut self.show_research, view.city, &mut out);
        }
        if let Some(stretch) = view.flow_hover {
            crate::network::hover_tooltip(ctx, view.city, stretch);
        }
        if view.city.ousted() && matches!(self.dialog, Dialog::None) {
            self.voted_out(ctx, view, &mut out);
        }
        self.dialogs(ctx, view, &mut out);
        out
    }

    fn menu_bar(&mut self, ctx: &egui::Context, view: &View, out: &mut Vec<Command>) {
        let city = view.city;
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                if let Some(pack) = view.pack {
                    ui.menu_button("Game", |ui| {
                        if ui.button(format!("Restart {}", pack.name)).on_hover_text(&pack.description).clicked() {
                            out.push(Command::StartScenario(pack.scenario.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Save").clicked() {
                            out.push(Command::Save(view.save_path.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Back to the last save").clicked() {
                            out.push(Command::Load(view.save_path.clone()));
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Save and quit").clicked() {
                            out.push(Command::Quit);
                        }
                    });
                }
                if view.pack.is_none() {
                    ui.menu_button("File", |ui| {
                        if ui.button("New city…").clicked() {
                            self.dialog = Dialog::NewCity;
                            ui.close_menu();
                        }
                        if ui.button("New scenario…").clicked() {
                            self.scenarios = opencity_assets::scenario::load_all(&view.catalog.data_dir);
                            self.scenario_pick = 0;
                            self.dialog = Dialog::NewScenario;
                            ui.close_menu();
                        }
                        if ui.button("Open…").clicked() {
                            self.path = view.save_path.display().to_string();
                            self.dialog = Dialog::Open;
                            ui.close_menu();
                        }
                        if ui.button("Save").clicked() {
                            out.push(Command::Save(view.save_path.clone()));
                            ui.close_menu();
                        }
                        if ui.button("Save as…").clicked() {
                            self.path = view.save_path.display().to_string();
                            self.dialog = Dialog::SaveAs;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("New region…").clicked() {
                            self.dialog = Dialog::NewRegion;
                            ui.close_menu();
                        }
                        if ui.button("Open region…").clicked() {
                            self.path = crate::region::regions_dir(&view.save_path).display().to_string();
                            self.dialog = Dialog::OpenRegion;
                            ui.close_menu();
                        }
                        if ui.button("Join region server…").clicked() {
                            self.dialog = Dialog::JoinRegion;
                            ui.close_menu();
                        }
                        if view.region.is_some() && ui.button("Save region").clicked() {
                            out.push(Command::SaveRegion);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Import GeoJSON…").clicked() {
                            self.path.clear();
                            self.dialog = Dialog::Import;
                            ui.close_menu();
                        }
                        if ui.button("Export GeoJSON…").clicked() {
                            self.path = "city.geojson".into();
                            self.dialog = Dialog::Export;
                            ui.close_menu();
                        }
                        if ui.button("Export open data…").on_hover_text("Buildings, streets, zones, trips and transit timetables as a real city publishes them (GeoJSON, CSV, GTFS), ready for city2graph").clicked() {
                            self.path = "city-data.zip".into();
                            self.dialog = Dialog::ExportData;
                            ui.close_menu();
                        }
                        #[cfg(target_arch = "wasm32")]
                        if crate::web::can_install() {
                            ui.separator();
                            if ui.button("Install as an app…").on_hover_text("Adds citysim to this device. It then starts from its own icon, in its own window, with or without a network.").clicked() {
                                crate::web::install();
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui.button("Quit").clicked() {
                            out.push(Command::Quit);
                        }
                    });
                }
                if let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) {
                    if !rules.disasters.is_empty() && view.pack.is_none_or(|p| p.disaster_menu) {
                        ui.menu_button("Disasters", |ui| {
                            let mut on = !gb.disasters_off;
                            if ui.checkbox(&mut on, "Disasters strike by chance").changed() {
                                out.push(Command::SetDisasters(on));
                            }
                            ui.separator();
                            for d in &rules.disasters {
                                if ui.button(format!("Set off: {}", d.name)).on_hover_text(&d.description).clicked() {
                                    out.push(Command::Strike(d.id.clone()));
                                    ui.close_menu();
                                }
                            }
                            if !rules.events.is_empty() {
                                ui.separator();
                                let mut on = city.affairs.on;
                                if ui.checkbox(&mut on, "Events come up by themselves").on_hover_text("Strikes, festivals, offers: now and then something asks for a decision").changed() {
                                    out.push(Command::SetEvents(on));
                                }
                                ui.menu_button("Make happen", |ui| {
                                    for e in &rules.events {
                                        let label = if e.external { format!("{} (from outside)", e.name) } else { e.name.clone() };
                                        if ui.button(label).on_hover_text(&e.text).clicked() {
                                            out.push(Command::Event(e.id.clone()));
                                            ui.close_menu();
                                        }
                                    }
                                });
                            }
                        });
                    }
                }
                if view.holding.is_some() && ui.button("OpenTactics").on_hover_text("The holding: time left and the report").clicked() {
                    self.show_holding = !self.show_holding;
                }
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_city, "City panel");
                    ui.menu_button("Size of the interface", |ui| {
                        for (label, z) in [("Automatic", None), ("Normal", Some(1.0)), ("Large", Some(1.3)), ("Huge", Some(1.6))] {
                            if ui.radio(self.zoom == z, label).clicked() {
                                self.zoom = z;
                                ui.close_menu();
                            }
                        }
                    });
                    if ui
                        .checkbox(&mut self.day_night, "Day and night, weather")
                        .on_hover_text(
                            "With it, traffic follows the hour: people leave for work in the morning and come home in the evening, and the nights are quiet",
                        )
                        .changed()
                    {
                        out.push(Command::SetDayNight(self.day_night));
                    }
                    let mut elections = view.city.mayor.elections;
                    if ui
                        .checkbox(&mut elections, "Elections")
                        .on_hover_text(format!(
                            "With them, a city of {} people or more votes you out after {} months in a row with approval under {:.0}%. Without, you are mayor for life",
                            view.city.vote_rules().voters,
                            view.city.vote_rules().oust_months,
                            view.city.vote_rules().oust_below * 100.0
                        ))
                        .changed()
                    {
                        out.push(Command::SetElections(elections));
                    }
                    if let Some(transit) = view.city.transit() {
                        let mut manual = transit.manual;
                        if ui
                            .checkbox(&mut manual, "Draw bus, tram and metro routes yourself")
                            .on_hover_text("People then ride only where your routes go, and you pay for the vehicles. The Routes window opens; the city's own routes are written down as a start")
                            .changed()
                        {
                            out.push(Command::Route(crate::routesui::RouteCmd::Manual(manual)));
                            self.routes.open = manual;
                        }
                    }
                    let mut terms = view.city.mayor.terms;
                    if ui
                        .checkbox(&mut terms, "Terms of office and promises")
                        .on_hover_text(format!(
                            "An election every {} years: you stay with half the vote, which is your approval, plus {:.0} for each campaign promise kept, less {:.0} for each broken",
                            view.city.vote_rules().term_years,
                            opencity_core::mayor::PROMISE_KEPT,
                            opencity_core::mayor::PROMISE_BROKEN
                        ))
                        .changed()
                    {
                        out.push(Command::SetTerms(terms));
                        self.show_election = terms;
                    }
                    if let Some(gb) = view.city.glassbox.as_ref().filter(|gb| !gb.sensors.is_empty()) {
                        ui.menu_button(format!("Sensors ({})", gb.sensors.len()), |ui| {
                            for s in &gb.sensors {
                                if ui.button(format!("{} · tile {}, {}", s.name, s.at.0, s.at.1)).clicked() {
                                    out.push(Command::Inspect(s.at));
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                    if view.city.glassbox.is_some() {
                        ui.checkbox(&mut self.graphs.open, "GlassBox graphs (G)");
                        let mut on = view.flows.on;
                        if ui.checkbox(&mut on, "Resource network on the map (V)").changed() {
                            out.push(Command::SetFlows(crate::flows::FlowView { on, ..view.flows.clone() }));
                        }
                        ui.checkbox(&mut self.show_network, "Resource network panel");
                    }
                    let mut under = view.underground;
                    if ui
                        .checkbox(&mut under, "Underground: pipes and metro (B)")
                        .on_hover_text("See the pipes and metro tunnels under the city; the bulldozer then digs them up and leaves the surface")
                        .changed()
                    {
                        out.push(Command::SetUnderground(under));
                    }
                    if view.pack.is_none()
                        && !view.rulesets.is_empty()
                        && ui.button("Rules editor…").on_hover_text("Edit the GlassBox rules: text with hints, or as a graph").clicked()
                    {
                        self.rules_editor.open = true;
                        ui.close_menu();
                    }
                    if let Some(rules) = view.city.rules() {
                        ui.menu_button("Data layer", |ui| {
                            if ui.radio(view.overlay.is_none(), "None").clicked() {
                                out.push(Command::SetOverlay(None));
                                ui.close_menu();
                            }
                            for (i, layer) in rules.layers.iter().enumerate() {
                                if ui.radio(view.overlay == Some(i), &layer.name).clicked() {
                                    out.push(Command::SetOverlay(Some(i)));
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                    if view.pack.is_none() {
                        ui.menu_button("Skin", |ui| {
                            if ui.radio(view.skin_choice.is_none(), "Automatic (era / world)").clicked() {
                                out.push(Command::SetSkin(None));
                                ui.close_menu();
                            }
                            if ui.radio(*view.skin_choice == Some(None), "Classic (none)").clicked() {
                                out.push(Command::SetSkin(Some(None)));
                                ui.close_menu();
                            }
                            for s in view.skins {
                                let chosen = *view.skin_choice == Some(Some(s.id.clone()));
                                if ui.radio(chosen, &s.name).on_hover_text(&s.description).clicked() {
                                    out.push(Command::SetSkin(Some(Some(s.id.clone()))));
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                    if ui.button("Reset camera").clicked() {
                        out.push(Command::ResetCamera);
                        ui.close_menu();
                    }
                });
                ui.separator();
                for (speed, label, hint) in [(Speed::Paused, "⏸", "Pause (Space)"), (Speed::Normal, "▶", "Normal speed"), (Speed::Fast, "⏩", "Fast (F)")]
                {
                    if ui.selectable_label(city.speed == speed, label).on_hover_text(hint).clicked() {
                        out.push(Command::SetSpeed(speed));
                    }
                }
                ui.separator();
                ui.label(RichText::new(format!("Date {}", city.date)).monospace());
                let h = city.hour();
                ui.label(
                    RichText::new(format!("{:02}:{:02} · {} · {}", h as u32, ((h.fract()) * 60.0) as u32, city.season().name(), city.weather.name()))
                        .monospace(),
                )
                .on_hover_text({
                    let [sun, wind] = city.sky();
                    format!(
                        "{}, {}\nSun {:.0}% and wind {:.0}% of an average day: what solar farms and wind turbines make now.",
                        city.season().name(),
                        city.weather.name(),
                        sun * 100.0,
                        wind * 100.0
                    )
                });
                let fund = RichText::new(format!("$ {}", city.fund)).monospace().strong();
                ui.label(if city.fund < 0 { fund.color(Color32::from_rgb(240, 90, 80)) } else { fund });
                ui.label(RichText::new(format!("Pop {}", city.population)).monospace());
                approval_label(ui, city);
                if let Some(name) = &city.meta.name {
                    ui.separator();
                    ui.label(RichText::new(name).italics());
                }
                if let Some(s) = view.skin {
                    ui.separator();
                    ui.label(RichText::new(&s.name).small()).on_hover_text(&s.description);
                }
            });
        });
    }

    /// Whether a tool is offered: the skin, the rules and the kind of game allow it.
    fn offered(view: &View, action: Action) -> bool {
        let id = match action {
            Action::Tool(Some(t)) => {
                if needs_rules(t) && view.city.glassbox.is_none() {
                    return false;
                }
                t.id()
            }
            _ => "inspect",
        };
        !(view.skin.is_some_and(|s| !s.allows(id)) || view.city.rules().is_some_and(|r| !r.allows(id)))
    }

    /// One tool's button; true when clicked.
    fn tool_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, view: &View, entry: ToolEntry, out: &mut Vec<Command>) -> bool {
        let (icon, action, label, key) = entry;
        let id = match action {
            Action::Tool(Some(t)) => t.id(),
            _ => "inspect",
        };
        // What the rules build names a service; skins name the rest.
        let service = view.city.service(id).map(|u| u.name.as_str());
        let label = service.unwrap_or_else(|| view.skin.map_or(label, |s| s.label(id, label)));
        let selected = view.active == Some(action);
        let tex = self.icon(ctx, view.catalog, view.skin, icon);
        let cost = match (action, view.city.rules()) {
            (Action::Tool(Some(t)), Some(rules)) if t.structure().is_some_and(|c| c.is_zone()) => {
                rules.zone_for(t.structure().unwrap()).and_then(|z| rules.zones[z].densities.get(view.city.zone_density as usize)).map(|d| d.cost)
            }
            (Action::Tool(Some(t)), Some(_)) if view.city.service(t.id()).is_some_and(|u| u.build_cost > 0) => view.city.service(t.id()).map(|u| u.build_cost),
            _ => tool_cost(action),
        };
        let text = match cost {
            Some(c) => format!("{label}  ${c}"),
            None => label.to_string(),
        };
        let button = match &tex {
            Some(t) => egui::Button::image_and_text(egui::Image::new((t.id(), EVec2::splat(26.0))), text),
            None => egui::Button::new(text),
        };
        // Tools waiting for research are shown, greyed out.
        let (locked, obsolete) = match action {
            Action::Tool(Some(t)) => (view.city.tool_locked_by(t), view.city.tool_obsolete(t)),
            _ => (None, None),
        };
        let full = match action {
            Action::Tool(Some(t)) => view.city.tool_at_limit(t),
            _ => None,
        };
        let why = locked
            .as_ref()
            .map(|t| format!("Needs research: {t}"))
            .or(obsolete.as_ref().map(|t| format!("Obsolete since {t}")))
            .or(full.as_ref().map(|n| format!("The city has its {} already", n.to_lowercase())));
        let resp = ui.add_enabled_ui(why.is_none(), |ui| ui.add_sized([166.0, 32.0], button.selected(selected))).inner;
        let resp = match &why {
            Some(why) => resp.on_disabled_hover_text(why),
            None if key.is_empty() => resp,
            None => resp.on_hover_text(format!("Shortcut: {key}")),
        };
        if resp.clicked() {
            out.push(Command::Act(if selected { Action::Tool(None) } else { action }));
        }
        resp.clicked()
    }

    fn tools(&mut self, ctx: &egui::Context, view: &View, out: &mut Vec<Command>) {
        // The submenu beside the sidebar: where it opens.
        let mut flyout: Option<(&'static str, egui::Pos2)> = None;
        egui::SidePanel::left("tools").resizable(false).exact_width(178.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for &(group, entries, submenu) in GROUPS {
                    let offered: Vec<ToolEntry> = entries.iter().copied().filter(|e| Self::offered(view, e.1)).collect();
                    if offered.is_empty() {
                        continue;
                    }
                    if !submenu {
                        ui.add_space(4.0);
                        ui.label(RichText::new(group).small().strong().color(Color32::GRAY));
                        for e in offered {
                            self.tool_button(ui, ctx, view, e, out);
                        }
                    } else {
                        // The group's last tool, one click away; ▸ opens the rest.
                        let active = offered.iter().find(|e| view.active == Some(e.1)).copied();
                        let last = active.or_else(|| self.last_tool.get(group).and_then(|a| offered.iter().find(|e| e.1 == *a).copied())).unwrap_or(offered[0]);
                        let open = self.open_group == Some(group);
                        ui.add_space(2.0);
                        let row = ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let tex = self.icon(ctx, view.catalog, view.skin, last.0);
                            let quick = match &tex {
                                Some(t) => egui::Button::image(egui::Image::new((t.id(), EVec2::splat(26.0)))),
                                None => egui::Button::new("•"),
                            };
                            let locked = match last.1 {
                                Action::Tool(Some(t)) => view.city.tool_locked_by(t).is_some() || view.city.tool_obsolete(t).is_some(),
                                _ => false,
                            };
                            let name = match last.1 {
                                Action::Tool(Some(t)) => {
                                    let id = t.id();
                                    view.city
                                        .rules()
                                        .and_then(|r| r.service_unit(id))
                                        .map(|u| u.name.clone())
                                        .unwrap_or_else(|| view.skin.map_or(last.2, |s| s.label(id, last.2)).to_string())
                                }
                                _ => last.2.to_string(),
                            };
                            let q = ui.add_enabled(!locked, quick.min_size(EVec2::new(34.0, 32.0)).selected(active.is_some())).on_hover_text(&name);
                            if q.clicked() {
                                out.push(Command::Act(if active.is_some() { Action::Tool(None) } else { last.1 }));
                            }
                            let label = match active {
                                Some(_) => RichText::new(format!("{group}: {name}")).strong(),
                                None => RichText::new(group),
                            };
                            let g = ui
                                .add_sized([128.0, 32.0], egui::Button::new(label).selected(open).wrap_mode(egui::TextWrapMode::Truncate))
                                .on_hover_text(format!("More {}", group.to_lowercase()));
                            ui.painter().text(
                                g.rect.right_center() - EVec2::new(8.0, 0.0),
                                egui::Align2::RIGHT_CENTER,
                                "▶",
                                egui::FontId::proportional(10.0),
                                Color32::GRAY,
                            );
                            if g.clicked() {
                                self.open_group = if open { None } else { Some(group) };
                            }
                            g.rect
                        });
                        if self.open_group == Some(group) {
                            flyout = Some((group, egui::pos2(row.response.rect.right() + 8.0, row.inner.top())));
                        }
                    }
                    if group == "Zones" {
                        if let Some(rules) = view.city.rules() {
                            let names: Vec<&str> = rules.zones.first().map(|z| z.densities.iter().map(|d| d.name.as_str()).collect()).unwrap_or_default();
                            ui.horizontal_wrapped(|ui| {
                                for (i, name) in names.iter().enumerate() {
                                    let short = name.split_whitespace().next().unwrap_or(name);
                                    if ui.selectable_label(view.city.zone_density == i as u8, short).on_hover_text(*name).clicked() {
                                        out.push(Command::SetDensity(i as u8));
                                    }
                                }
                            });
                        }
                        ui.add_space(4.0);
                        ui.label(RichText::new("Build").small().strong().color(Color32::GRAY));
                    }
                }
            });
        });
        // The open submenu: its tools in a column beside the sidebar.
        let Some((group, pos)) = flyout else { return };
        let Some(&(_, entries, _)) = GROUPS.iter().find(|g| g.0 == group) else { return };
        let area = egui::Area::new(egui::Id::new("tool_flyout")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(group).strong());
                for &e in entries.iter().filter(|e| Self::offered(view, e.1)) {
                    if self.tool_button(ui, ctx, view, e, out) {
                        self.last_tool.insert(group, e.1);
                        self.open_group = None;
                    }
                }
            });
        });
        // A click anywhere else closes it (the sidebar's own buttons toggle it).
        let clicked_outside =
            ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && p.x > 178.0));
        if clicked_outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.open_group = None;
        }
    }

    fn city_panel(&mut self, ctx: &egui::Context, view: &View, out: &mut Vec<Command>) {
        let city = view.city;
        let v = city.values;
        // Every part folds away under its title, and the whole panel scrolls:
        // on a screen that is not tall, the parts further down start folded.
        let tall = ctx.screen_rect().height() >= 760.0;
        let part = |ui: &mut egui::Ui, title: &str, open: bool, body: &mut dyn FnMut(&mut egui::Ui)| {
            egui::CollapsingHeader::new(RichText::new(title).small().color(Color32::GRAY))
                .id_salt(("city_part", title.split(':').next().unwrap_or(title)))
                .default_open(open)
                .show(ui, |ui| body(ui));
        };
        egui::SidePanel::right("city").resizable(false).exact_width(230.0).show(ctx, |ui| {
            ui.heading("City");
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                part(ui, "Zones and power", true, &mut |ui| {
                    let total = (v.residential + v.commercial + v.industrial).max(1) as f32;
                    let power_used = v.power_capacity - v.power;
                    let power = if v.power_capacity > 0 { power_used.max(0) as f32 / v.power_capacity as f32 } else { 0.0 };
                    for (label, value, fraction, color) in [
                        ("Residential", v.residential.to_string(), v.residential as f32 / total, Color32::from_rgb(70, 190, 90)),
                        ("Commercial", v.commercial.to_string(), v.commercial as f32 / total, Color32::from_rgb(80, 150, 230)),
                        ("Industrial", v.industrial.to_string(), v.industrial as f32 / total, Color32::from_rgb(230, 190, 60)),
                        ("Power used", format!("{power_used} / {}", v.power_capacity), power, Color32::from_rgb(240, 220, 80)),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(value).monospace()));
                        });
                        ui.add(egui::ProgressBar::new(fraction).fill(color).desired_height(6.0));
                    }
                });
                if let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) {
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("📈 Graphs (G)").on_hover_text("Plot and analyse every GlassBox series").clicked() {
                            self.graphs.open = !self.graphs.open;
                        }
                        if ui.button("🔀 Network").on_hover_text("Show who sends what to whom on the map, and analyse it").clicked() {
                            self.show_network = !self.show_network;
                            if self.show_network && !view.flows.on {
                                out.push(Command::SetFlows(crate::flows::FlowView { on: true, ..view.flows.clone() }));
                            }
                        }
                        if !rules.techs.is_empty()
                            && ui.button("🔬 Research").on_hover_text("Spend ideas and research points to unlock buildings and tools").clicked()
                        {
                            self.show_research = !self.show_research;
                        }
                        if !(rules.taxes.is_empty() && rules.policies.is_empty())
                            && ui.button("📜 Policies").on_hover_text("Set tax rates and the city's policies").clicked()
                        {
                            self.show_policies = !self.show_policies;
                        }
                        if view.city.goals.is_some() && ui.button("🎯 Goals").on_hover_text("The scenario's goals and how they are going").clicked() {
                            self.show_goals = !self.show_goals;
                        }
                        if ui.button("💰 Budget").on_hover_text("Money by line, service funding and loans").clicked() {
                            self.show_budget = !self.show_budget;
                        }
                        let n = view.city.advice().len();
                        let news = if n > 0 { format!("📰 News ({n})") } else { "📰 News".to_string() };
                        if ui.button(news).on_hover_text("What needs you now, and the news so far").clicked() {
                            self.show_news = !self.show_news;
                        }
                        if view.city.manual_routes() && ui.button("Routes").on_hover_text("The bus, tram and metro routes you draw").clicked() {
                            self.routes.open = !self.routes.open;
                        }
                        if view.city.rules().is_some_and(|r| r.units.iter().any(|u| u.district > 0))
                            && ui
                                .button("Neighbourhoods")
                                .on_hover_text("The land around each neighbourhood office: its figures, its name, its policy")
                                .clicked()
                        {
                            self.districts.open = !self.districts.open;
                        }
                        if let Some(q) = view.city.question() {
                            let label = RichText::new(format!("Decide ({}m)", q.months_left)).color(Color32::from_rgb(240, 190, 80)).strong();
                            if ui.button(label).on_hover_text(format!("{}: waiting for your answer", q.name)).clicked() {
                                self.show_decision = !self.show_decision;
                            }
                        }
                        if let Some(years) = view.city.years_to_election() {
                            if ui
                                .button(format!("✔ Election ({years}y)"))
                                .on_hover_text("The next election, how it would go today, and your promises")
                                .clicked()
                            {
                                self.show_election = !self.show_election;
                            }
                        }
                        if view.region.is_some() && ui.button("🗺 Region").on_hover_text("The region's cities and this month's trade").clicked() {
                            self.show_region = !self.show_region;
                        }
                    });
                    part(ui, "Demand", true, &mut |ui| {
                        let max = gb.demand.iter().cloned().fold(1.0f32, f32::max);
                        for (z, d) in rules.zones.iter().zip(&gb.demand) {
                            ui.horizontal(|ui| {
                                ui.label(&z.name);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(format!("{d:.0}")).monospace()));
                            });
                            ui.add(egui::ProgressBar::new((d / max).clamp(0.0, 1.0)).desired_height(6.0));
                        }
                    });
                    if rules.people.is_some() {
                        part(ui, "People", true, &mut |ui| {
                            egui::Grid::new("people").num_columns(2).show(ui, |ui| {
                                let warn = |bad: bool| if bad { Color32::from_rgb(240, 150, 90) } else { Color32::LIGHT_GRAY };
                                ui.label("Out of work").on_hover_text(
                            "Workers with no job to go to, over the last days. Where they live, crime rises: zone works and shops, or let the homes thin out",
                        );
                                ui.label(RichText::new(format!("{:.0}%", gb.jobless * 100.0)).monospace().color(warn(gb.jobless > 0.1)));
                                ui.end_row();
                                ui.label("Without a home").on_hover_text(
                            "People whose home is gone (abandoned, burnt, evacuated or pulled down). They wait for room in new homes, and in time leave town",
                        );
                                ui.label(RichText::new(format!("{:.0}", gb.homeless)).monospace().color(warn(gb.homeless >= 1.0)));
                                ui.end_row();
                            });
                        });
                    }
                    part(ui, "Resources per tick: delivered / wanted", tall, &mut |ui| {
                        egui::Grid::new("balances").num_columns(2).show(ui, |ui| {
                            let b = &gb.balances;
                            for (i, r) in rules.resources.iter().enumerate() {
                                let (got, missing) = (b.delivered.get(i).copied().unwrap_or(0.0), b.unmet.get(i).copied().unwrap_or(0.0));
                                let made = b.produced.get(i).copied().unwrap_or(0.0);
                                if got + missing + made <= 0.0 {
                                    continue;
                                }
                                ui.label(&r.name);
                                let text = match r.transport {
                                    opencity_core::rules::Transport::Local => format!("{made:.0}"),
                                    opencity_core::rules::Transport::City if made > 0.0 => format!("+{made:.1}"),
                                    opencity_core::rules::Transport::City => format!("-{got:.1}"),
                                    opencity_core::rules::Transport::Pool => format!("+{made:.1}  ({:.0})", gb.pools.get(i).copied().unwrap_or(0.0)),
                                    _ => format!("{got:.0} / {:.0}", got + missing),
                                };
                                let short = missing > (got + missing) * 0.1;
                                ui.label(RichText::new(text).monospace().color(if short { Color32::from_rgb(240, 150, 90) } else { Color32::LIGHT_GRAY }));
                                ui.end_row();
                            }
                        });
                    });
                }
                if let Some(gb) = city.glassbox.as_ref().filter(|g| !g.metrics.is_empty()) {
                    part(ui, &format!("Metrics: at tick {}", gb.tick), tall, &mut |ui| {
                        egui::Grid::new("metrics").num_columns(2).show(ui, |ui| {
                            for (name, v) in &gb.metrics {
                                ui.label(name);
                                ui.label(RichText::new(format!("{v:.1}")).monospace());
                                ui.end_row();
                            }
                        });
                    });
                }
                part(ui, "Last 20 years", tall, &mut |ui| chart(ui, city));
                part(ui, "About the city", tall, &mut |ui| {
                    egui::Grid::new("facts").num_columns(2).show(ui, |ui| {
                        ui.label("Size");
                        ui.label(format!("{} × {}", city.width(), city.length()));
                        ui.end_row();
                        ui.label("Buildings");
                        ui.label(city.layer.occupied().to_string());
                        ui.end_row();
                        ui.label("Vehicles");
                        ui.label(city.vehicles.len().to_string());
                        ui.end_row();
                        ui.label("Income");
                        ui.label(format!("{} this year", city.income));
                        ui.end_row();
                        if let Some(src) = &city.meta.source {
                            ui.label("Source");
                            ui.label(RichText::new(src).small());
                            ui.end_row();
                        }
                        if let Some(g) = &city.meta.georef {
                            ui.label("Scale");
                            ui.label(format!("{:.0} {} / tile", g.units_per_tile, if g.wgs84 { "m" } else { "units" }));
                            ui.end_row();
                        }
                    });
                });
            });
        });
    }

    fn status_bar(&mut self, ctx: &egui::Context, view: &View) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some((w, l)) = view.hover {
                    ui.label(RichText::new(format!("{w:>3},{l:<3}")).monospace());
                    let h = view.city.terrain.tile_min(w, l);
                    ui.label(format!("height {h}"));
                    if let Some(s) = view.city.layer.structure_at(w, l) {
                        ui.label(format!("· {}", display_name(view, s)));
                    }
                }
                if let Some(m) = view.message {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(m).color(Color32::from_rgb(250, 210, 120)));
                    });
                }
            });
        });
    }

    fn query_window(&mut self, ctx: &egui::Context, view: &View, out: &mut Vec<Command>) {
        let Some((w, l)) = view.query else { return };
        let city = view.city;
        let rules = city.rules();
        let s = city.layer.structure_at(w, l);
        let def = s.and_then(|s| s.unit).and_then(|u| rules.map(|r| &r.units[u as usize]));
        let title = s.map_or_else(|| "Empty land".to_string(), |s| display_name(view, s));
        let mut open = true;
        egui::Window::new(RichText::new(title).strong())
            .id(egui::Id::new("query_window"))
            .open(&mut open)
            .resizable(false)
            .default_width(300.0)
            .default_pos([200.0, 120.0])
            .show(ctx, |ui| {
                ui.set_max_width(320.0);
                if let (Some(s), Some(info)) = (s, city.query(w, l)) {
                    let origin = city.layer.origin_of(w, l).unwrap_or((w, l));
                    // What it is: zone, density and level, or service, or road.
                    ui.label(RichText::new(kind_line(view, s)).color(Color32::GRAY));
                    let description = match def {
                        Some(d) => (!d.description.is_empty()).then_some(d.description.as_str()),
                        None if s.code == StructureCode::Road => None,
                        None => view.catalog.get(s.graphic).and_then(|e| e.description.as_deref()),
                    };
                    if let Some(d) = description {
                        ui.label(RichText::new(d).italics().small());
                    }
                    if let Some(d) = city.district_at(origin.0, origin.1) {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Neighbourhood: {}", d.name)).small());
                            if ui.small_button("Open").clicked() {
                                self.districts.open = true;
                                self.districts.selected = Some(d.at);
                            }
                        });
                    }
                    if let (Some(d), Some(gb)) = (def, &city.glassbox) {
                        if d.relic {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new("A relic. Keep it, or").small());
                                let label = if d.salvage > 0 { format!("pull it down for ${}", d.salvage) } else { "pull it down".to_string() };
                                if ui.small_button(label).on_hover_text("Clears the land. What can be sold of it is paid to the city.").clicked() {
                                    out.push(Command::Dismantle(origin));
                                }
                            });
                        } else if let Some(left) = gb.closes_in(&d.id) {
                            let months = (left as f32 / (opencity_core::glassbox::OBSOLETE_GRACE as f32 / 12.0)).ceil().max(1.0);
                            ui.label(
                                RichText::new(format!("Obsolete: it closes in {months:.0} months and stays as a relic."))
                                    .small()
                                    .color(Color32::from_rgb(230, 170, 60)),
                            );
                        }
                    }
                    ui.add_space(2.0);
                    if s.code == StructureCode::Road {
                        road_details(ui, s.road, info.traffic, rules.map(|r| &**r));
                    } else if s.code.is_block() {
                        ui.horizontal(|ui| {
                            ui.label(size_line(city, s));
                            if ui.small_button("⟳ Turn").on_hover_text("Turn it a quarter clockwise (O). New buildings face the street.").clicked() {
                                out.push(Command::Turn(origin));
                            }
                        });
                    } else {
                        ui.label(size_line(city, s));
                    }
                    if let (Some(gb), Some(rules), Some(u)) = (&city.glassbox, rules, s.unit) {
                        unit_details(ui, city, gb, rules, u as usize, origin);
                        crate::network::unit_links(ui, city, gb, rules, origin, out);
                    } else if !s.code.is_path() {
                        ui.label(format!("Level {}", info.level));
                        ui.horizontal_wrapped(|ui| {
                            for (label, bit) in [
                                ("Residents", Status::RESIDENTIAL),
                                ("Shops", Status::COMMERCIAL),
                                ("Jobs", Status::INDUSTRIAL),
                                ("Power", Status::POWER),
                                ("Road", Status::ROAD),
                            ] {
                                let on = info.status.has(bit);
                                let text = RichText::new(if on { format!("✔ {label}") } else { format!("✖ {label}") });
                                ui.label(if on { text.color(Color32::from_rgb(110, 210, 110)) } else { text.color(Color32::from_rgb(220, 110, 100)) });
                            }
                        });
                    }
                }
                if city.layer.has_pipe(w, l) {
                    ui.label(RichText::new("💧 A water main runs underneath (B shows the pipes)").color(Color32::from_rgb(110, 180, 240)));
                }
                if city.layer.has_tunnel(w, l) {
                    ui.label(RichText::new("Ⓜ A metro tunnel runs underneath (B shows it)").color(Color32::from_rgb(240, 150, 90)));
                }
                ui.separator();
                ui.label(RichText::new(format!("Tile {w}, {l} · ground height {}", city.terrain.tile_min(w, l))).small().color(Color32::GRAY));
                if let (Some(gb), Some(rules)) = (&city.glassbox, rules) {
                    let values: Vec<String> =
                        rules.layers.iter().enumerate().map(|(i, layer)| format!("{} {:.1}", layer.name, gb.layer_value(i, w, l, city.width()))).collect();
                    ui.label(RichText::new(values.join(" · ")).small().color(Color32::GRAY));
                    sensor_section(ui, city, gb, (w, l), out);
                }
            });
        if !open {
            out.push(Command::CloseQuery);
        }
    }

    /// The city voted the mayor out: what now.
    fn voted_out(&mut self, ctx: &egui::Context, view: &View, out: &mut Vec<Command>) {
        let city = view.city;
        egui::Window::new("Voted out of office").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_max_width(420.0);
            let red = Color32::from_rgb(230, 120, 100);
            if city.mayor.by_election {
                let share = city.mayor.last_vote.unwrap_or(0.0);
                ui.label(RichText::new(format!("You lost the election with {share:.0}% of the vote.")).heading().color(red));
                ui.label("Half was needed: approval, plus promises kept, less promises broken. The city stands still until you decide what next.");
            } else {
                ui.label(RichText::new(format!("{:.0}% of the city is behind you.", city.mayor.approval * 100.0)).heading().color(red));
                ui.label(format!(
                    "After {} months under {:.0}%, the council held a vote of no confidence, and you lost it. The city stands still until you decide what next.",
                    city.vote_rules().oust_months,
                    city.vote_rules().oust_below * 100.0
                ));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("What people hold against you:").strong());
            approval_lines(ui, city);
            ui.separator();
            if ui
                .button("Run for office again")
                .on_hover_text("You win the campaign and start over at 50% approval: put things right before the next vote")
                .clicked()
            {
                out.push(Command::StayInOffice(false));
            }
            if ui.button("Stay on as mayor for life").on_hover_text("No more votes in this city (View > Elections brings them back)").clicked() {
                out.push(Command::StayInOffice(true));
            }
            match view.pack {
                Some(pack) => {
                    if ui.button(format!("Restart {}", pack.name)).clicked() {
                        out.push(Command::StartScenario(pack.scenario.clone()));
                    }
                }
                None => {
                    if ui.button("Start a new city").clicked() {
                        self.dialog = Dialog::NewCity;
                    }
                }
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context, view: &View, out: &mut Vec<Command>) {
        let mut close = false;
        match self.dialog {
            Dialog::None => {}
            Dialog::NewCity => {
                egui::Window::new("New city").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    let before = self.new_skin.clone();
                    era_culture_picker(ui, view.skins, &mut self.new_era, &mut self.new_skin);
                    if self.new_skin != before {
                        self.new_rules = rules_for_age(view.rulesets, view.skins, self.new_skin.as_deref(), self.new_rules.take());
                    }
                    rules_picker(ui, view.rulesets, &mut self.new_rules);
                    ui.separator();
                    let m = &mut self.new_map;
                    egui::Grid::new("newcity").num_columns(2).show(ui, |ui| {
                        ui.label("Width");
                        ui.add(egui::DragValue::new(&mut m.width).range(16..=256));
                        ui.end_row();
                        ui.label("Length");
                        ui.add(egui::DragValue::new(&mut m.length).range(16..=256));
                        ui.end_row();
                        ui.label("Relief");
                        egui::ComboBox::from_id_salt("relief").selected_text(format!("{:?}", m.relief)).show_ui(ui, |ui| {
                            for r in [Relief::Plain, Relief::Hill, Relief::Mountain] {
                                ui.selectable_value(&mut m.relief, r, format!("{r:?}"));
                            }
                        });
                        ui.end_row();
                        ui.label("Water");
                        egui::ComboBox::from_id_salt("water").selected_text(format!("{:?}", m.water)).show_ui(ui, |ui| {
                            for w in [Water::Dry, Water::Lake, Water::Coast] {
                                ui.selectable_value(&mut m.water, w, format!("{w:?}"));
                            }
                        });
                        ui.end_row();
                        ui.label("Shape");
                        egui::ComboBox::from_id_salt("shape").selected_text(format!("{:?}", m.shape)).show_ui(ui, |ui| {
                            for s in [Shape::None, Shape::Island, Shape::Volcano] {
                                ui.selectable_value(&mut m.shape, s, format!("{s:?}"));
                            }
                        });
                        ui.end_row();
                        ui.label("Rivers");
                        ui.add(egui::DragValue::new(&mut m.rivers).range(0..=4))
                            .on_hover_text("Carved from the high ground down to the water or the edge; roads and rails cross them on bridges");
                        ui.end_row();
                        ui.label("Trees");
                        egui::ComboBox::from_id_salt("trees").selected_text(format!("{:?}", m.trees)).show_ui(ui, |ui| {
                            for t in [TreeDensity::Sparse, TreeDensity::Normal, TreeDensity::Dense] {
                                ui.selectable_value(&mut m.trees, t, format!("{t:?}"));
                            }
                        });
                        ui.end_row();
                        ui.label("Seed");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut m.seed));
                            if ui.button("🎲").clicked() {
                                m.seed = macroquad::rand::rand() as u64;
                            }
                        });
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            out.push(Command::NewCity(self.new_map.clone(), self.new_skin.clone(), self.new_rules.clone()));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            }
            Dialog::NewScenario => {
                egui::Window::new("New scenario").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    if self.scenarios.is_empty() {
                        ui.label("No scenarios found (assets/scenarios/index.toml).");
                    }
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            for (i, s) in self.scenarios.iter().enumerate() {
                                if ui.selectable_label(self.scenario_pick == i, &s.name).clicked() {
                                    self.scenario_pick = i;
                                }
                            }
                        });
                        ui.separator();
                        if let Some(s) = self.scenarios.get(self.scenario_pick) {
                            ui.vertical(|ui| {
                                ui.set_max_width(320.0);
                                ui.label(RichText::new(&s.name).strong());
                                ui.label(RichText::new(&s.description).small());
                                ui.add_space(4.0);
                                for g in &s.goals {
                                    let when = match (g.always, g.within_years) {
                                        (true, _) => " (all game long)".to_string(),
                                        (_, Some(y)) => format!(" (within {y} years)"),
                                        _ => String::new(),
                                    };
                                    ui.label(RichText::new(format!("🎯 {}{when}", g.name)).small());
                                }
                            });
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!self.scenarios.is_empty(), egui::Button::new("Start")).clicked() {
                            out.push(Command::StartScenario(self.scenarios[self.scenario_pick].id.clone()));
                            self.show_goals = true;
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            }
            Dialog::NewRegion => {
                egui::Window::new("New region").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    ui.label(RichText::new("A grid of neighbouring cities that trade across their borders. You play one at a time.").small());
                    egui::Grid::new("newregion").num_columns(2).show(ui, |ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut self.region_name);
                        ui.end_row();
                        ui.label("Cities across");
                        ui.add(egui::DragValue::new(&mut self.region_size[0]).range(1..=6));
                        ui.end_row();
                        ui.label("Cities down");
                        ui.add(egui::DragValue::new(&mut self.region_size[1]).range(1..=6));
                        ui.end_row();
                        ui.label("Map size");
                        ui.add(egui::DragValue::new(&mut self.region_map).range(32..=192).suffix(" tiles"));
                        ui.end_row();
                    });
                    rules_picker(ui, view.rulesets, &mut self.new_rules);
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            out.push(Command::NewRegion {
                                name: self.region_name.clone(),
                                size: self.region_size,
                                map_size: self.region_map,
                                rules: self.new_rules.clone(),
                            });
                            self.show_region = true;
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            }
            Dialog::JoinRegion => {
                egui::Window::new("Join region server").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    ui.label(RichText::new("Play one town of a region hosted with `opencity-cli region-server`. Neighbours are other players' towns: trade and ideas cross the borders.").small());
                    egui::Grid::new("join").num_columns(2).show(ui, |ui| {
                        ui.label("Server");
                        ui.add(egui::TextEdit::singleline(&mut self.join_url).desired_width(260.0));
                        ui.end_row();
                        ui.label("Your name");
                        ui.text_edit_singleline(&mut self.join_player);
                        ui.end_row();
                        ui.label("Town slot");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut self.join_slot[0]).range(0..=15).prefix("x "));
                            ui.add(egui::DragValue::new(&mut self.join_slot[1]).range(0..=15).prefix("y "));
                        });
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Join").clicked() {
                            out.push(Command::JoinRegion { url: self.join_url.clone(), player: self.join_player.clone(), slot: self.join_slot });
                            self.show_region = true;
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            }
            Dialog::OpenRegion => {
                egui::Window::new("Open region").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    if cfg!(target_arch = "wasm32") {
                        ui.label("Regions saved in this browser");
                    } else {
                        ui.label("Region folder (or its region.json)");
                        ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(420.0));
                    }
                    let dir = PathBuf::from(&self.path);
                    let found = crate::region::list(&dir);
                    if found.is_empty() && cfg!(target_arch = "wasm32") {
                        ui.label(RichText::new("None yet: File > New region… founds one.").small().color(Color32::GRAY));
                    }
                    for p in found {
                        let name = p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                        let chosen = self.path == p.display().to_string();
                        if ui.selectable_label(chosen, name).clicked() {
                            self.path = p.display().to_string();
                        }
                    }
                    ui.horizontal(|ui| {
                        let can_open = !cfg!(target_arch = "wasm32") || crate::region::exists(&dir.join("region.json"));
                        if ui.add_enabled(can_open, egui::Button::new("Open")).clicked() {
                            out.push(Command::OpenRegion(PathBuf::from(&self.path)));
                            self.show_region = true;
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            }
            Dialog::Open | Dialog::SaveAs | Dialog::Export | Dialog::ExportData => {
                let (title, verb) = match self.dialog {
                    Dialog::Open => ("Open city", "Open"),
                    Dialog::SaveAs => ("Save city as", "Save"),
                    Dialog::ExportData => ("Export open data (.zip)", "Export"),
                    _ => ("Export GeoJSON", "Export"),
                };
                egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    ui.label("File");
                    ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(380.0));
                    ui.horizontal(|ui| {
                        if ui.button(verb).clicked() && !self.path.trim().is_empty() {
                            let p = PathBuf::from(self.path.trim());
                            out.push(match self.dialog {
                                Dialog::Open => Command::Load(p),
                                Dialog::SaveAs => Command::Save(p),
                                Dialog::ExportData => Command::ExportData(p),
                                _ => Command::ExportGeojson(p),
                            });
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            }
            Dialog::Import => {
                egui::Window::new("Import GeoJSON").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    ui.label("GeoJSON or Overpass JSON file (OpenFantasyMap, OpenHistoryMap, OSM)");
                    ui.add(egui::TextEdit::singleline(&mut self.path).hint_text("data/ohm-bologna-center.json").desired_width(380.0));
                    ui.label("Date (historical data, optional)");
                    ui.add(egui::TextEdit::singleline(&mut self.import_date).hint_text("1600 or 1850-06-12").desired_width(160.0));
                    ui.label("Tag rules (optional TOML file)");
                    ui.add(egui::TextEdit::singleline(&mut self.import_rules).hint_text("built-in OSM rules").desired_width(380.0));
                    ui.horizontal(|ui| {
                        if ui.button("Import").clicked() && !self.path.trim().is_empty() {
                            let opt = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
                            out.push(Command::ImportGeojson {
                                path: PathBuf::from(self.path.trim()),
                                date: opt(&self.import_date),
                                rules: opt(&self.import_rules).map(PathBuf::from),
                            });
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            }
        }
        if close {
            self.dialog = Dialog::None;
        }
        let _ = view;
    }
}

/// The sensor at a tile: what it reads now and its buttons, or the button
/// that leaves one there.
fn sensor_section(ui: &mut egui::Ui, city: &City, gb: &opencity_core::glassbox::GlassBox, tile: (u32, u32), out: &mut Vec<Command>) {
    ui.separator();
    let Some((i, s)) = gb.sensors.iter().enumerate().find(|(_, s)| s.at == tile) else {
        let what = match city.layer.structure_at(tile.0, tile.1) {
            Some(s) if s.code == StructureCode::Road => "the trips driving past, in all and by what they carry",
            Some(s) if s.unit.is_some() => "what arrives at this building and what leaves it",
            _ => "the maps at this tile",
        };
        if ui
            .small_button("📡 Leave a sensor here")
            .on_hover_text(format!("It records {what}, tick by tick, with the maps here. See it in the GlassBox graphs (G) or save it as a CSV file."))
            .clicked()
        {
            out.push(Command::AddSensor(tile));
        }
        return;
    };
    let h = city.hour();
    let clock = if gb.hour.is_some() { format!(" · {:02}:{:02}", h as u32, (h.fract() * 60.0) as u32) } else { String::new() };
    ui.label(RichText::new(format!("📡 {}{clock}", s.name)).strong());
    match gb.readings.get(i).filter(|r| !r.is_empty()) {
        Some(r) => {
            egui::Grid::new("sensor_now").num_columns(2).spacing([14.0, 2.0]).show(ui, |ui| {
                // What moves first; the maps are already listed above.
                for (k, v) in r.iter().filter(|(k, _)| gb.rules().is_none_or(|rules| rules.layers.iter().all(|l| &l.name != k))) {
                    ui.label(RichText::new(k).small());
                    ui.label(RichText::new(format!("{v:.1}")).small().monospace());
                    ui.end_row();
                }
            });
        }
        None => {
            ui.label(RichText::new("Its first reading comes with the next tick.").small().color(Color32::GRAY));
        }
    }
    ui.horizontal(|ui| {
        if ui.small_button("Save CSV").on_hover_text("Everything it recorded over the ticks kept (2000 at most), one row per tick, with the hour").clicked() {
            out.push(Command::SaveSensorCsv(s.name.clone()));
        }
        if ui.small_button("Remove").clicked() {
            out.push(Command::RemoveSensor(s.name.clone()));
        }
    });
}

/// A GlassBox unit's needs, deliveries and satisfaction.
fn unit_details(ui: &mut egui::Ui, city: &City, gb: &opencity_core::glassbox::GlassBox, rules: &Ruleset, unit: usize, origin: (u32, u32)) {
    let def = &rules.units[unit];
    let Some(st) = gb.unit_state(origin.0, origin.1, city.width()) else { return };
    ui.add(egui::ProgressBar::new(st.satisfaction).text(format!("Satisfaction {:.0}%", st.satisfaction * 100.0)));
    if let Some((a, d)) = gb.affliction(origin.0, origin.1, city.width()).and_then(|a| Some((a, rules.disaster(&a.disaster)?))) {
        use opencity_core::rules::DisasterEffect;
        let left = a.until.saturating_sub(gb.tick);
        let text = match d.effect {
            DisasterEffect::Destroy => format!("🔥 On fire ({}): lost in {left} ticks unless it is put out", d.name),
            DisasterEffect::Damage => format!("🌊 {}: damaged, for {left} more ticks", d.name),
            DisasterEffect::Sicken => format!("✚ {}: its people are ill, for {left} more ticks", d.name),
        };
        let help = match (d.effect, d.protection.as_deref().and_then(|p| rules.layer(p))) {
            (DisasterEffect::Destroy, Some(li)) => {
                format!("{} here decides how soon it is put out, and whether it passes to the buildings next door", rules.layers[li].name)
            }
            (DisasterEffect::Destroy, None) => "Nothing in these rules puts it out".to_string(),
            _ => "It passes by itself".to_string(),
        };
        ui.label(RichText::new(text).color(Color32::from_rgb(240, 130, 60))).on_hover_text(help);
    }
    if let Some(li) = gb.uninhabitable(rules, origin.0, origin.1, city.width()) {
        let l = &rules.layers[li];
        let v = gb.layer_value(li, origin.0, origin.1, city.width());
        ui.label(RichText::new(format!("☢ {} {v:.0}: nobody can stay here", l.name)).color(Color32::from_rgb(240, 130, 60))).on_hover_text(format!(
            "Homes, shops and industry empty, and nothing is built, until it falls under {:.0}. It fades by itself, slowly.",
            l.uninhabitable.unwrap_or(0.0)
        ));
    }
    if rules.round_the_clock(unit, origin.1 * city.width() + origin.0) {
        let hours: Vec<String> =
            rules.shifts.clone().unwrap_or_default().hours.iter().map(|h| format!("{:02}:{:02}", *h as u32, (h.fract() * 60.0) as u32)).collect();
        ui.label(RichText::new(format!("🕐 Works round the clock: shifts change at {}", hours.join(", "))).small())
            .on_hover_text("Its staff come and go at the shift changes, not with the rush hours");
    }
    if let Some(g) = gb.gates.iter().find(|g| g.is_world() && g.owner == Some(origin)) {
        let list = |m: &std::collections::BTreeMap<String, f32>| {
            let s: Vec<String> = m
                .iter()
                .filter(|(_, v)| **v > 0.005)
                .map(|(r, v)| format!("{v:.1} {}", rules.resource(r).map_or(r.as_str(), |i| rules.resources[i].name.as_str())))
                .collect();
            if s.is_empty() {
                "nothing".to_string()
            } else {
                s.join(", ").to_lowercase()
            }
        };
        ui.label(RichText::new(format!("Trade with the world last tick: in {}; out {}", list(&g.carried_in), list(&g.carried_out))).small()).on_hover_text(
            format!(
                "Up to {} per tick each way, at the resource's price; it brings in what the city lacks and takes out what has nowhere to go",
                list(&g.import)
            ),
        );
    }
    egui::Grid::new("unit").num_columns(2).show(ui, |ui| {
        // Its own needs, plus what its trips asked of other networks last tick.
        let own = |i: usize| def.needs.get(&rules.resources[i].id).copied().unwrap_or(0.0);
        let needs: Vec<(usize, f32)> = if st.needs.is_empty() {
            def.needs.iter().filter_map(|(r, n)| Some((rules.resource(r)?, *n))).collect()
        } else {
            st.needs.iter().copied().enumerate().filter(|(_, n)| *n > 0.0).collect()
        };
        for (i, need) in needs {
            let got = st.received.get(i).copied().unwrap_or(0.0);
            let trips = need - own(i);
            let label = ui.label(if trips > 0.005 {
                format!("needs {} (+{trips:.1} for trips)", rules.resources[i].name)
            } else {
                format!("needs {}", rules.resources[i].name)
            });
            if trips > 0.005 {
                label.on_hover_text("Asked by the trips this building sent last tick (resource `uses`)");
            }
            let ok = got >= need * 0.95;
            ui.label(RichText::new(format!("{got:.1} / {need:.1}")).monospace().color(if ok {
                Color32::from_rgb(110, 210, 110)
            } else {
                Color32::from_rgb(230, 120, 100)
            }));
            ui.end_row();
        }
        for (r, amount) in &def.produce {
            let i = rules.resource(r).unwrap();
            // An outlet: how much of what it made found takers.
            if let Some(&(_, made, sent)) = st.outlets.iter().find(|o| o.0 == i).filter(|_| def.outlets.contains_key(r)) {
                ui.label(format!("sends {}", rules.resources[i].name)).on_hover_text("An outlet: what finds no taker within reach counts against satisfaction");
                let ok = sent >= made * 0.95;
                ui.label(RichText::new(format!("{sent:.1} / {made:.1}")).monospace().color(if ok {
                    Color32::from_rgb(110, 210, 110)
                } else {
                    Color32::from_rgb(230, 120, 100)
                }));
                ui.end_row();
                continue;
            }
            ui.label(format!("makes {}", rules.resources[i].name));
            match def.follows {
                // What the sky leaves of it right now.
                Some(f) => {
                    let sky = gb.sky_factor(def);
                    let what = if f == opencity_core::rules::Follows::Sun { "sun" } else { "wind" };
                    ui.label(RichText::new(format!("{:.1} of {amount:.1}", amount * sky)).monospace())
                        .on_hover_text(format!("It makes what the {what} gives: {:.0}% of an average day right now.", sky * 100.0));
                }
                None => match gb.yield_at(rules, def, origin.0, origin.1, city.width()) {
                    // What the land gives it here.
                    Some((f, li, v)) => {
                        ui.label(RichText::new(format!("{:.1} of {amount:.1}", amount * f)).monospace()).on_hover_text(format!(
                            "The land here gives {:.0}% of that: {} is {v:.1} (View > Data layer shows where it is better).",
                            f * 100.0,
                            rules.layers[li].name
                        ));
                    }
                    None => {
                        ui.label(RichText::new(format!("{amount:.1}")).monospace());
                    }
                },
            }
            ui.end_row();
        }
        for (l, b) in &def.requires {
            let li = rules.layer(l).unwrap();
            let v = gb.layer_value(li, origin.0, origin.1, city.width());
            let ok = !(b.min.is_some_and(|m| v < m) || b.max.is_some_and(|m| v > m));
            let bound = match (b.min, b.max) {
                (Some(a), _) => format!("≥ {a}"),
                (_, Some(a)) => format!("≤ {a}"),
                _ => String::new(),
            };
            ui.label(format!("{} {bound}", rules.layers[li].name));
            ui.label(RichText::new(format!("{v:.1}")).monospace().color(if ok { Color32::from_rgb(110, 210, 110) } else { Color32::from_rgb(230, 120, 100) }));
            ui.end_row();
        }
    });
    // What keeps it from growing: needs of the next level nothing can supply.
    let density = city.layer.structure_at(origin.0, origin.1).map_or(0, |s| s.density);
    if let Some(next) = rules.next_unit(unit, density) {
        let key = origin.1 * city.width() + origin.0;
        let short: Vec<String> =
            gb.growth_needs(rules, unit, next, key).into_iter().filter(|n| !n.2).map(|n| rules.resources[n.0].name.to_lowercase()).collect();
        if !short.is_empty() {
            ui.label(RichText::new(format!("Cannot grow into a {}: no {} to spare", rules.units[next].name.to_lowercase(), short.join(", "))).small())
                .on_hover_text("Buildings level up only into needs their networks can supply");
        }
    }
    if st.starved > 0 {
        ui.label(RichText::new("Starved of something it needs: it will not grow, and shrinks if this lasts").small().color(Color32::from_rgb(230, 120, 100)));
    }
}

/// Repaints the street icon (a light road with darker dashes on a grey
/// button) as another kind of road.
fn repaint_road_icon(img: &mut image::RgbaImage, kind: opencity_core::RoadKind) {
    use opencity_core::RoadKind as K;
    for (x, y, p) in img.enumerate_pixels_mut() {
        let [r, g, b, a] = p.0;
        // The road is the warm (yellowish) part; the button is grey.
        if (r as i32 - b as i32) < 18 {
            continue;
        }
        let lum = (r as f32 * 0.3 + g as f32 * 0.59 + b as f32 * 0.11) / 229.0;
        let mark = lum < 0.85;
        let cobble = ((x / 3) + (y / 3)) % 2 == 0;
        let base: [f32; 3] = match (kind, mark) {
            (K::Street | K::Tram, _) => continue,
            (K::Highway, false) => [95.0, 98.0, 104.0],
            (K::Highway, true) => [245.0, 245.0, 240.0],
            (K::DirtRoad, false) => [176.0, 136.0, 88.0],
            (K::DirtRoad, true) => [140.0, 104.0, 64.0],
            (K::PavedStreet, _) if cobble => [190.0, 182.0, 166.0],
            (K::PavedStreet, _) => [160.0, 152.0, 138.0],
            // Sleepers across ballast, steel rails along.
            (K::Rail, _) if x % 16 == 5 || x % 16 == 10 => [205.0, 205.0, 212.0],
            (K::Rail, _) if y % 6 < 2 => [92.0, 64.0, 40.0],
            (K::Rail, _) => [128.0, 120.0, 108.0],
        };
        let shade = if mark { 1.0 } else { lum.min(1.1) };
        *p = image::Rgba([(base[0] * shade).min(255.0) as u8, (base[1] * shade).min(255.0) as u8, (base[2] * shade).min(255.0) as u8, a]);
    }
}

/// The rules to offer for a new city in a skin's age: the current choice if
/// it suits the age (or is the classic rules), else the first set that does.
fn rules_for_age(rulesets: &[Arc<Ruleset>], skins: &[Skin], skin: Option<&str>, current: Option<String>) -> Option<String> {
    // Without a skin the city is modern.
    let years = skin.and_then(|id| skins.iter().find(|s| s.id == id)).and_then(|s| s.applies.years).unwrap_or([1946, 9999]);
    let fits = |id: &str| rulesets.iter().find(|r| r.id == id).is_some_and(|r| r.fits_years(years));
    match current {
        None => None,
        Some(id) if fits(&id) => Some(id),
        Some(id) => rulesets.iter().find(|r| r.years.is_some() && r.fits_years(years)).map(|r| r.id.clone()).or(Some(id)),
    }
}

fn rules_picker(ui: &mut egui::Ui, rulesets: &[Arc<Ruleset>], chosen: &mut Option<String>) {
    egui::Grid::new("rules").num_columns(2).show(ui, |ui| {
        ui.label("Rules");
        let text = chosen.as_ref().and_then(|id| rulesets.iter().find(|r| &r.id == id)).map_or("Classic OpenCity".to_string(), |r| r.name.clone());
        egui::ComboBox::from_id_salt("rules_combo").selected_text(text).width(220.0).show_ui(ui, |ui| {
            if ui.selectable_label(chosen.is_none(), "Classic OpenCity").on_hover_text("The original OpenCity growth rules").clicked() {
                *chosen = None;
            }
            for r in rulesets {
                if ui.selectable_label(chosen.as_ref() == Some(&r.id), &r.name).on_hover_text(&r.description).clicked() {
                    *chosen = Some(r.id.clone());
                }
            }
        });
        ui.end_row();
    });
}

/// Era and culture choice for a new city. Cultures are the skins of the era.
fn era_culture_picker(ui: &mut egui::Ui, skins: &[Skin], era: &mut Option<String>, skin: &mut Option<String>) {
    egui::Grid::new("era").num_columns(2).show(ui, |ui| {
        ui.label("Era");
        let era_text = era.clone().unwrap_or_else(|| "Modern".into());
        egui::ComboBox::from_id_salt("era_combo").selected_text(era_text).width(220.0).show_ui(ui, |ui| {
            if ui.selectable_label(era.is_none(), "Modern").clicked() {
                *era = None;
                *skin = None;
            }
            // "Modern" is the entry above (its default skin applies by itself).
            for e in Skin::eras(skins).into_iter().filter(|e| *e != "Modern") {
                if ui.selectable_label(era.as_deref() == Some(e), e).clicked() {
                    *era = Some(e.to_string());
                    *skin = skins.iter().find(|s| s.era == e).map(|s| s.id.clone());
                }
            }
        });
        ui.end_row();
        if let Some(e) = era.clone() {
            let cultures: Vec<&Skin> = skins.iter().filter(|s| s.era == e).collect();
            ui.label("Culture");
            let current =
                cultures.iter().find(|s| Some(&s.id) == skin.as_ref()).map(|s| s.culture.clone().unwrap_or_else(|| s.name.clone())).unwrap_or_default();
            egui::ComboBox::from_id_salt("culture_combo").selected_text(current).width(220.0).show_ui(ui, |ui| {
                for s in &cultures {
                    let label = s.culture.clone().unwrap_or_else(|| s.name.clone());
                    if ui.selectable_label(skin.as_ref() == Some(&s.id), label).on_hover_text(&s.description).clicked() {
                        *skin = Some(s.id.clone());
                    }
                }
            });
            ui.end_row();
        }
    });
    if let Some(s) = skin.as_ref().and_then(|id| skins.iter().find(|s| &s.id == id)) {
        ui.label(RichText::new(&s.description).small().italics());
    }
}

/// What a structure is called: its GlassBox unit, its kind of road, else
/// its skin or model name.
fn display_name(view: &View, s: &opencity_core::Structure) -> String {
    match s.unit.and_then(|u| view.city.rules().map(|r| &r.units[u as usize])) {
        Some(d) => d.name.clone(),
        None if s.code == StructureCode::Road => s.road.name().to_string(),
        None => structure_name(view.catalog, view.skin, s),
    }
}

/// Name of a structure without a GlassBox unit: the skin's name for the
/// tool that builds it, else the model's name.
fn structure_name(catalog: &Catalog, skin: Option<&Skin>, s: &opencity_core::Structure) -> String {
    let tool = Tool::ALL.into_iter().find(|t| t.structure() == Some(s.code) && t.road_kind().is_none_or(|k| k == s.road));
    if let (Some(t), Some(skin)) = (tool, skin) {
        if let Some(label) = skin.labels.get(t.id()) {
            return label.clone();
        }
    }
    catalog.get(s.graphic).and_then(|e| e.name.clone()).unwrap_or_else(|| format!("{:?}", s.code))
}

/// "Residential · Medium density · level 2 of 3", "Service", "Road"...
fn kind_line(view: &View, s: &opencity_core::Structure) -> String {
    let rules = view.city.rules();
    if s.code == StructureCode::Road {
        return if s.road.is_rail() {
            "Track · trains only; roads cross it on level crossings".into()
        } else if s.road == opencity_core::RoadKind::Tram {
            "Tram street · cars use it, and trams run between its tram stops".into()
        } else if s.road.access() {
            "Road".into()
        } else {
            "Road · no access for buildings".into()
        };
    }
    if let (Some(rules), Some(u)) = (rules, s.unit) {
        let id = &rules.units[u as usize].id;
        if let Some(z) = rules.zone_for(s.code) {
            let zone = &rules.zones[z];
            if let Some(d) = zone.densities.get(s.density as usize) {
                let level = d.units.iter().position(|x| x == id).map_or(String::new(), |i| format!(" · level {} of {}", i + 1, d.units.len()));
                return format!("{} · {}{level}", zone.name, d.name);
            }
            return zone.name.clone();
        }
        return "Service".into();
    }
    match s.code {
        StructureCode::WaterPipe => "Water main · carries water and sewage to the buildings beside it (now underground)".into(),
        StructureCode::PowerLine => "Power line".into(),
        c if c.is_zone() && s.level == 0 => format!("{c:?} zone · empty plot"),
        c if c.is_zone() => format!("{c:?} zone"),
        c => format!("{c:?}").replace("Dept", " department"),
    }
}

/// Footprint and height of a building.
fn size_line(city: &City, s: &opencity_core::Structure) -> String {
    let fp = city.footprints().get(s.graphic);
    let (w, l) = s.size(city.footprints());
    let tiles = if w * l == 1 { "1 tile".to_string() } else { format!("{w} × {l} tiles") };
    if fp.height <= 0.0 {
        return format!("Size {tiles}");
    }
    // One tile of height is about three floors.
    let floors = (fp.height / 0.3).round().max(1.0) as u32;
    format!("Size {tiles} · {floors} floor{}", if floors == 1 { "" } else { "s" })
}

/// Speed, capacity and width of a road, against a street.
fn road_details(ui: &mut egui::Ui, kind: opencity_core::RoadKind, traffic: u8, rules: Option<&opencity_core::rules::Ruleset>) {
    // The vehicles the rules let onto this kind of road.
    let vehicles: Option<Vec<&str>> = rules.and_then(|r| {
        let net = r.networks.iter().find(|n| n.structures.iter().any(|s| s == "road") && !n.vehicles.is_empty())?;
        Some(net.vehicles.iter().filter(|v| v.ways().allows(kind)).map(|v| v.name.as_str()).collect())
    });
    egui::Grid::new("road").num_columns(2).show(ui, |ui| {
        ui.label("Speed");
        ui.label(format!("× {:.1} of a street", kind.speed()));
        ui.end_row();
        ui.label("Capacity");
        ui.label(format!("× {:.1} of a street", kind.capacity()));
        ui.end_row();
        ui.label("Width");
        ui.label(format!("{} lane{} · {:.2} tiles", kind.lanes(), if kind.lanes() == 1 { "" } else { "s" }, kind.width()));
        ui.end_row();
        ui.label("Cost");
        ui.label(format!("${} per tile, ${}/month upkeep", kind.build_cost(), kind.maintenance_cost()));
        ui.end_row();
        ui.label("Traffic");
        if rules.is_some() {
            // GlassBox: the share of the roads' capacity around here that
            // this tick's trips take, and what it does to trips.
            let jam = (traffic as f32 / 100.0).min(2.0);
            let slow = 1.0 + opencity_core::glassbox::CONGESTION_DELAY * jam;
            ui.label(format!("{traffic}% of capacity · trips take ×{slow:.1}"));
        } else {
            ui.label(format!("{traffic}"));
        }
        ui.end_row();
        if let Some(v) = vehicles {
            ui.label("Used by");
            ui.label(if v.is_empty() { "nobody (closed to every vehicle)".to_string() } else { v.join(", ") });
            ui.end_row();
        }
    });
}

/// Population and fund over the recorded years.
fn chart(ui: &mut egui::Ui, city: &City) {
    let (rect, _) = ui.allocate_exact_size(EVec2::new(ui.available_width(), 90.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(16, 19, 26));
    let series: [(Vec<f32>, Color32); 2] = [
        (city.history.iter().map(|r| r.population as f32).collect(), Color32::from_rgb(110, 210, 120)),
        (city.history.iter().map(|r| r.fund as f32).collect(), Color32::from_rgb(240, 200, 80)),
    ];
    for (values, color) in series {
        let (min, max) = values.iter().fold((0f32, 1f32), |(a, b), &v| (a.min(v), b.max(v)));
        let n = values.len().max(2) as f32 - 1.0;
        let points: Vec<egui::Pos2> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                egui::pos2(
                    rect.left() + 4.0 + (rect.width() - 8.0) * i as f32 / n,
                    rect.bottom() - 4.0 - (rect.height() - 8.0) * (v - min) / (max - min).max(1.0),
                )
            })
            .collect();
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
    }
    painter.text(
        rect.left_top() + EVec2::new(6.0, 4.0),
        egui::Align2::LEFT_TOP,
        "population",
        egui::FontId::proportional(10.0),
        Color32::from_rgb(110, 210, 120),
    );
    painter.text(rect.left_top() + EVec2::new(6.0, 16.0), egui::Align2::LEFT_TOP, "fund", egui::FontId::proportional(10.0), Color32::from_rgb(240, 200, 80));
}

/// The mayor's approval rating in the menu bar; hovering lists what makes it.
fn approval_label(ui: &mut egui::Ui, city: &City) {
    let vr = city.vote_rules();
    let (below, months, voters) = (vr.oust_below, vr.oust_months, vr.voters);
    let m = &city.mayor;
    let text = RichText::new(format!("Approval {:.0}%", m.approval * 100.0)).monospace();
    let text = if m.approval < below {
        text.color(Color32::from_rgb(240, 90, 80)).strong()
    } else if m.approval < below + 0.12 {
        text.color(Color32::from_rgb(235, 190, 90))
    } else {
        text
    };
    ui.label(text).on_hover_ui(|ui| {
        ui.set_max_width(340.0);
        ui.label(RichText::new("What the city makes of its mayor").strong());
        approval_lines(ui, city);
        ui.separator();
        let rule = if !m.elections {
            "No elections: you are mayor for life (View > Elections).".to_string()
        } else if let Some(left) = city.months_to_vote() {
            format!("The council votes you out in {left} months unless approval is back above {:.0}%.", below * 100.0)
        } else {
            format!("Under {:.0}% for {months} months in a row, a city of {voters} people or more votes its mayor out.", below * 100.0)
        };
        ui.label(RichText::new(rule).small());
    });
}

/// What adds to and takes from the approval rating, and where it is heading.
fn approval_lines(ui: &mut egui::Ui, city: &City) {
    for (i, (what, points)) in city.approval_factors().iter().enumerate() {
        let n = if i == 0 { format!("{points:.0}") } else { format!("{points:+.0}") };
        ui.label(format!("{n:>4}  {what}"));
    }
    let (now, to) = (city.mayor.approval * 100.0, city.approval_target() * 100.0);
    let trend = if (to - now).abs() < 1.0 { "holding".to_string() } else { format!("heading for {to:.0}%") };
    ui.label(RichText::new(format!("Now {now:.0}%, {trend} (it moves a fifth of the way each month).")).small().color(Color32::GRAY));
}
