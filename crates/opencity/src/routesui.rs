//! The routes window: bus, tram and metro routes the player draws (opt-in).

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::transit::{Mode, MAX_STOPS, MAX_VEHICLES};
use opencity_core::City;

use crate::ui_egui::Command;

/// A change to the routes.
#[derive(Clone, Debug, PartialEq)]
pub enum RouteCmd {
    /// The player draws the routes, or the city runs them again.
    Manual(bool),
    Add(Mode),
    Delete(u32),
    AddStop(u32, (u32, u32)),
    RemoveStop(u32, usize),
    Vehicles(u32, u32),
    Circular(u32, bool),
    Rename(u32, String),
    /// Routes for every network that has none.
    Draw,
}

#[derive(Default)]
pub struct RoutesUi {
    pub open: bool,
    /// The route whose stops are listed.
    pub selected: Option<u32>,
    /// Clicks on the map add stops to the selected route.
    pub adding: bool,
    /// The name being typed, and whose it is.
    name: (Option<u32>, String),
}

const WARN: Color32 = Color32::from_rgb(235, 170, 80);

/// The colour a route is drawn in, on the map and in the list.
pub fn colour(id: u32) -> [u8; 3] {
    const PALETTE: [[u8; 3]; 8] =
        [[230, 70, 70], [70, 140, 240], [80, 200, 110], [240, 180, 50], [180, 100, 230], [60, 200, 210], [240, 120, 180], [160, 160, 170]];
    PALETTE[(id as usize + PALETTE.len() - 1) % PALETTE.len()]
}

fn stop_word(mode: Mode) -> &'static str {
    match mode {
        Mode::Bus => "bus stops",
        Mode::Tram => "tram stops",
        Mode::Metro => "metro stations",
    }
}

impl RoutesUi {
    /// The route that map clicks add stops to right now.
    pub fn picking(&self) -> Option<u32> {
        self.selected.filter(|_| self.open && self.adding)
    }

    pub fn window(&mut self, ctx: &egui::Context, city: &City, out: &mut Vec<Command>) {
        let mut open = self.open;
        egui::Window::new("Routes")
            .open(&mut open)
            .default_width(430.0)
            .default_pos([250.0, 70.0])
            .resizable(true)
            .show(ctx, |ui| self.contents(ui, city, out));
        self.open = open;
        if !open {
            self.adding = false;
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui, city: &City, out: &mut Vec<Command>) {
        let Some(transit) = city.transit() else {
            ui.label("Routes need GlassBox rules.");
            return;
        };
        let mut manual = transit.manual;
        if ui
            .checkbox(&mut manual, "I draw the routes")
            .on_hover_text("Off: every stop reaches every other stop on its streets, tram line or tunnel, for free. On: people ride only where a route goes, and you pay for its vehicles.")
            .changed()
        {
            out.push(Command::Route(RouteCmd::Manual(manual)));
        }
        if !transit.manual {
            ui.label(
                RichText::new(if transit.routes.is_empty() {
                    "The city runs buses, trams and metro trains between all the stops by itself. Tick the box to decide where they go: the city's routes are written down as a start."
                } else {
                    "The city runs everything by itself again. Your routes are kept for when you tick the box."
                })
                .small()
                .color(Color32::GRAY),
            );
            return;
        }
        let routes = city.routes();
        ui.horizontal_wrapped(|ui| {
            for mode in Mode::ALL {
                if ui.button(format!("+ {} route", mode.name())).clicked() {
                    out.push(Command::Route(RouteCmd::Add(mode)));
                }
            }
            if ui.button("Fill the gaps").on_hover_text("Adds a route for every street network, tram line and tunnel whose stops have none").clicked() {
                out.push(Command::Route(RouteCmd::Draw));
            }
        });
        ui.label(format!("Vehicles cost ${} a month.", transit.cost()));
        ui.separator();
        if routes.is_empty() {
            ui.label(RichText::new("No routes: nobody rides. Add one, then click its stops on the map.").color(WARN));
        }
        if self.selected.is_some_and(|id| transit.route(id).is_none()) {
            self.selected = None;
            self.adding = false;
        }
        egui::ScrollArea::vertical().max_height(180.0).id_salt("routes").show(ui, |ui| {
            egui::Grid::new("route_list").num_columns(7).striped(true).show(ui, |ui| {
                for h in ["", "Route", "Stops", "Vehicles", "Wait", "Riders", "Full"] {
                    ui.label(RichText::new(h).small().color(Color32::GRAY));
                }
                ui.end_row();
                for (r, info) in &routes {
                    let [cr, cg, cb] = colour(r.id);
                    ui.label(RichText::new("■").color(Color32::from_rgb(cr, cg, cb)));
                    if ui.selectable_label(self.selected == Some(r.id), &r.name).clicked() {
                        self.adding &= self.selected == Some(r.id);
                        self.selected = Some(r.id);
                    }
                    let trouble = !info.missing.is_empty() || !info.broken.is_empty() || r.stops.len() < 2;
                    ui.label(RichText::new(r.stops.len().to_string()).color(if trouble { WARN } else { Color32::LIGHT_GRAY }));
                    ui.label(r.vehicles.to_string());
                    ui.label(info.wait.map_or("-".to_string(), |w| format!("{w:.1}")));
                    ui.label(format!("{:.0}", info.riders));
                    let full = RichText::new(format!("{:.0}%", info.load * 100.0));
                    ui.label(if info.full_stops > 0 { full.color(WARN).strong() } else { full });
                    ui.end_row();
                }
            });
        });
        let Some((r, info)) = self.selected.and_then(|id| routes.iter().find(|(r, _)| r.id == id)) else {
            if !routes.is_empty() {
                ui.label(
                    RichText::new("Pick a route to change it. Wait is the time to board, in street tiles; riders are people per tick at its stops; full is how much of its vehicles' room its busiest stop takes.")
                        .small()
                        .color(Color32::GRAY),
                );
            }
            return;
        };
        ui.separator();
        if self.name.0 != Some(r.id) {
            self.name = (Some(r.id), r.name.clone());
        }
        ui.horizontal(|ui| {
            let edit = ui.add(egui::TextEdit::singleline(&mut self.name.1).desired_width(160.0));
            if edit.lost_focus() && self.name.1.trim() != r.name && !self.name.1.trim().is_empty() {
                out.push(Command::Route(RouteCmd::Rename(r.id, self.name.1.clone())));
            }
            ui.label(RichText::new(r.mode.name()).color(Color32::GRAY));
            if ui.button("Delete route").clicked() {
                out.push(Command::Route(RouteCmd::Delete(r.id)));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Vehicles");
            if ui.add_enabled(r.vehicles > 0, egui::Button::new("−")).clicked() {
                out.push(Command::Route(RouteCmd::Vehicles(r.id, r.vehicles - 1)));
            }
            ui.label(RichText::new(r.vehicles.to_string()).strong());
            if ui.add_enabled(r.vehicles < MAX_VEHICLES, egui::Button::new("+")).clicked() {
                out.push(Command::Route(RouteCmd::Vehicles(r.id, r.vehicles + 1)));
            }
            ui.label(format!("${} each a month", r.mode.vehicle_cost()));
            let mut circular = r.circular;
            if ui.checkbox(&mut circular, "Circular").on_hover_text("Round and round (three stops or more) instead of there and back").changed() {
                out.push(Command::Route(RouteCmd::Circular(r.id, circular)));
            }
        });
        ui.label(
            RichText::new(match info.wait {
                Some(w) => format!(
                    "Round trip {} tiles · wait {w:.1} · {:.0} riders a tick · room for {:.0} at each stop · ${} a month",
                    info.round_trip, info.riders, info.room, info.cost
                ),
                None if r.vehicles == 0 => "No vehicles: the route does not run.".to_string(),
                None => "Two stops or more make a route.".to_string(),
            })
            .small()
            .color(if info.wait.is_some() { Color32::GRAY } else { WARN }),
        );
        let full = r.stops.len() >= MAX_STOPS;
        if ui
            .add_enabled(
                !full || self.adding,
                egui::SelectableLabel::new(
                    self.adding,
                    if self.adding { "Adding stops: click them on the map (click here when done)" } else { "Add stops on the map" },
                ),
            )
            .clicked()
        {
            self.adding = !self.adding;
        }
        if self.adding {
            ui.label(
                RichText::new(format!("Click {} in the order the route serves them. A stop can be on several routes; people change there.", stop_word(r.mode)))
                    .small()
                    .color(Color32::GRAY),
            );
        }
        egui::ScrollArea::vertical().max_height(170.0).id_salt("route_stops").show(ui, |ui| {
            for (i, stop) in r.stops.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.small_button("x").on_hover_text("Take this stop off the route").clicked() {
                        out.push(Command::Route(RouteCmd::RemoveStop(r.id, i)));
                    }
                    if ui.small_button("Show").clicked() {
                        out.push(Command::Inspect(*stop));
                    }
                    let riders = city.transit().and_then(|t| t.riders.get(stop)).copied().unwrap_or(0.0);
                    let load = city.transit().and_then(|t| t.load.get(stop)).copied().unwrap_or(0.0);
                    ui.label(format!("{}. at {},{} · {riders:.0} riders", i + 1, stop.0, stop.1));
                    if load >= opencity_core::transit::FULL {
                        ui.label(RichText::new("full").color(WARN).strong())
                            .on_hover_text("The vehicles calling here take no more: people go by road, or not at all. Add vehicles, or another route.");
                    }
                    if info.missing.contains(stop) {
                        ui.label(RichText::new("gone, or nothing to stop at beside it").color(WARN));
                    } else if info.broken.contains(&i) {
                        ui.label(RichText::new("not joined to the next stop").color(WARN));
                    }
                });
            }
        });
    }
}
