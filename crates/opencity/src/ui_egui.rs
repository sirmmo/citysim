//! Desktop UI built with egui: menu bar, tool palette, city panel, dialogs.
//! (The classic UI made of the original OpenCity images lives in `ui.rs` and
//! is used on Android or with `--classic-ui`.)

use std::collections::HashMap;
use std::path::PathBuf;

use egui_macroquad::egui::{self, Color32, RichText, TextureHandle, Vec2 as EVec2};
use opencity_assets::{Catalog, Skin};
use opencity_core::{City, Speed, Status, Tool};
use opencity_mapgen::{MapParams, Relief, Shape, TreeDensity, Water};

use crate::ui::Action;

/// Requests from the UI to the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Act(Action),
    SetSpeed(Speed),
    /// New map, with the skin to dress it in (`None`: classic modern look).
    NewCity(MapParams, Option<String>),
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
    ResetCamera,
    CloseQuery,
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
    pub skin: Option<&'a Skin>,
    pub skins: &'a [Skin],
    pub skin_choice: &'a Option<Option<String>>,
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
}

pub struct EguiUi {
    icons: HashMap<&'static str, TextureHandle>,
    dialog: Dialog,
    new_map: MapParams,
    /// Era picked in the new-city dialog (`None`: modern) and the culture's skin id.
    new_era: Option<String>,
    new_skin: Option<String>,
    path: String,
    import_date: String,
    import_rules: String,
    show_city: bool,
    /// Skin the current style and icons were built for.
    styled_for: Option<Option<String>>,
}

type ToolEntry = (&'static str, Action, &'static str, &'static str);

const GROUPS: &[(&str, &[ToolEntry])] = &[
    (
        "Zones",
        &[
            ("residential", Action::Tool(Some(Tool::ZoneResidential)), "Residential", "R"),
            ("commercial", Action::Tool(Some(Tool::ZoneCommercial)), "Commercial", "C"),
            ("industrial", Action::Tool(Some(Tool::ZoneIndustrial)), "Industrial", "I"),
        ],
    ),
    (
        "Networks",
        &[
            ("road", Action::Tool(Some(Tool::Road)), "Road", "P"),
            ("power_line", Action::Tool(Some(Tool::PowerLine)), "Power line", "L"),
            ("power_plant_coal", Action::Tool(Some(Tool::CoalPlant)), "Coal plant", ""),
            ("power_plant_nuclear", Action::Tool(Some(Tool::NuclearPlant)), "Nuclear plant", ""),
        ],
    ),
    (
        "Services",
        &[
            ("fire", Action::Tool(Some(Tool::FireDept)), "Fire department", ""),
            ("police", Action::Tool(Some(Tool::PoliceDept)), "Police", ""),
            ("hospital", Action::Tool(Some(Tool::Hospital)), "Hospital", ""),
            ("education", Action::Tool(Some(Tool::School)), "School", ""),
        ],
    ),
    (
        "Land",
        &[
            ("park_city", Action::Tool(Some(Tool::Park)), "Park", ""),
            ("tree", Action::Tool(Some(Tool::Trees)), "Trees", "T"),
            ("raise", Action::Tool(Some(Tool::RaiseTerrain)), "Raise terrain", "U"),
            ("lower", Action::Tool(Some(Tool::LowerTerrain)), "Lower terrain", "J"),
            ("bulldozer", Action::Tool(Some(Tool::Destroy)), "Bulldozer", "X"),
            ("query", Action::Query, "Inspect", "/"),
        ],
    ),
];

fn tool_cost(a: Action) -> Option<u32> {
    match a {
        Action::Tool(Some(Tool::RaiseTerrain | Tool::LowerTerrain)) => Some(opencity_core::city::TERRAIN_COST),
        Action::Tool(Some(t)) => t.structure().map(|s| s.build_cost()).filter(|&c| c > 0),
        _ => None,
    }
}

impl EguiUi {
    pub fn new(settings_map: &MapParams) -> Self {
        Self {
            icons: HashMap::new(),
            dialog: Dialog::None,
            new_map: settings_map.clone(),
            new_era: None,
            new_skin: None,
            path: String::new(),
            import_date: String::new(),
            import_rules: String::new(),
            show_city: true,
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
        let bytes = opencity_assets::read_bytes(&path).ok()?;
        let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
        let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        let tex = ctx.load_texture(name, color, egui::TextureOptions::LINEAR);
        self.icons.insert(name, tex.clone());
        Some(tex)
    }

    /// Builds the UI; returns the commands issued this frame.
    pub fn show(&mut self, ctx: &egui::Context, view: &View) -> Vec<Command> {
        self.style(ctx, view.skin);
        let mut out = Vec::new();
        self.menu_bar(ctx, view, &mut out);
        self.tools(ctx, view, &mut out);
        if self.show_city {
            self.city_panel(ctx, view);
        }
        self.status_bar(ctx, view);
        self.query_window(ctx, view, &mut out);
        self.dialogs(ctx, view, &mut out);
        out
    }

    fn menu_bar(&mut self, ctx: &egui::Context, view: &View, out: &mut Vec<Command>) {
        let city = view.city;
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New city…").clicked() {
                        self.dialog = Dialog::NewCity;
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
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        out.push(Command::Quit);
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_city, "City panel");
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
                let fund = RichText::new(format!("$ {}", city.fund)).monospace().strong();
                ui.label(if city.fund < 0 { fund.color(Color32::from_rgb(240, 90, 80)) } else { fund });
                ui.label(RichText::new(format!("Pop {}", city.population)).monospace());
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

    fn tools(&mut self, ctx: &egui::Context, view: &View, out: &mut Vec<Command>) {
        egui::SidePanel::left("tools").resizable(false).exact_width(178.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (group, entries) in GROUPS {
                    ui.add_space(4.0);
                    ui.label(RichText::new(*group).small().strong().color(Color32::GRAY));
                    for &(icon, action, label, key) in *entries {
                        let (id, label) = match action {
                            Action::Tool(Some(t)) => (t.id(), label),
                            _ => ("inspect", label),
                        };
                        if view.skin.is_some_and(|s| !s.allows(id)) {
                            continue;
                        }
                        let label = view.skin.map_or(label, |s| s.label(id, label));
                        let selected = view.active == Some(action);
                        let tex = self.icon(ctx, view.catalog, view.skin, icon);
                        let text = match tool_cost(action) {
                            Some(c) => format!("{label}  ${c}"),
                            None => label.to_string(),
                        };
                        let button = match &tex {
                            Some(t) => egui::Button::image_and_text(egui::Image::new((t.id(), EVec2::splat(26.0))), text),
                            None => egui::Button::new(text),
                        };
                        let resp = ui.add_sized([166.0, 32.0], button.selected(selected));
                        let resp = if key.is_empty() { resp } else { resp.on_hover_text(format!("Shortcut: {key}")) };
                        if resp.clicked() {
                            out.push(Command::Act(if selected { Action::Tool(None) } else { action }));
                        }
                    }
                }
            });
        });
    }

    fn city_panel(&mut self, ctx: &egui::Context, view: &View) {
        let city = view.city;
        let v = city.values;
        egui::SidePanel::right("city").resizable(false).exact_width(230.0).show(ctx, |ui| {
            ui.heading("City");
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
            ui.separator();
            ui.label(RichText::new("Last 20 years").small().color(Color32::GRAY));
            chart(ui, city);
            ui.separator();
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
    }

    fn status_bar(&mut self, ctx: &egui::Context, view: &View) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some((w, l)) = view.hover {
                    ui.label(RichText::new(format!("{w:>3},{l:<3}")).monospace());
                    let h = view.city.terrain.tile_min(w, l);
                    ui.label(format!("height {h}"));
                    if let Some(s) = view.city.layer.structure_at(w, l) {
                        ui.label(format!("· {}", structure_name(view.catalog, s)));
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
        let mut open = true;
        egui::Window::new(format!("Tile {w},{l}")).open(&mut open).resizable(false).default_pos([200.0, 120.0]).show(ctx, |ui| {
            match (view.city.query(w, l), view.city.layer.structure_at(w, l)) {
                (Some(info), Some(s)) => {
                    ui.heading(structure_name(view.catalog, s));
                    if let Some(desc) = view.catalog.get(s.graphic).and_then(|e| e.description.as_deref()) {
                        ui.label(RichText::new(desc).italics().small());
                    }
                    ui.label(format!("Level {}   Traffic {}", info.level, info.traffic));
                    ui.horizontal(|ui| {
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
                _ => {
                    ui.label("Empty land");
                }
            }
            ui.label(format!("Ground height {}", view.city.terrain.tile_min(w, l)));
        });
        if !open {
            out.push(Command::CloseQuery);
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context, view: &View, out: &mut Vec<Command>) {
        let mut close = false;
        match self.dialog {
            Dialog::None => {}
            Dialog::NewCity => {
                egui::Window::new("New city").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    era_culture_picker(ui, view.skins, &mut self.new_era, &mut self.new_skin);
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
                            out.push(Command::NewCity(self.new_map.clone(), self.new_skin.clone()));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            }
            Dialog::Open | Dialog::SaveAs | Dialog::Export => {
                let (title, verb) = match self.dialog {
                    Dialog::Open => ("Open city", "Open"),
                    Dialog::SaveAs => ("Save city as", "Save"),
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
            for e in Skin::eras(skins) {
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

fn structure_name(catalog: &Catalog, s: &opencity_core::Structure) -> String {
    catalog.get(s.graphic).and_then(|e| e.name.clone()).unwrap_or_else(|| format!("{:?}", s.code))
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
