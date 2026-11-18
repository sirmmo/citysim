//! The neighbourhoods window: the land around each neighbourhood office,
//! its figures, its name and the policy that holds there.

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::City;

use crate::ui_egui::Command;

/// A change to a neighbourhood, by the tile of its office.
#[derive(Clone, Debug, PartialEq)]
pub enum DistrictCmd {
    Rename((u32, u32), String),
    Policy((u32, u32), Option<String>),
}

#[derive(Default)]
pub struct DistrictsUi {
    pub open: bool,
    /// The office of the neighbourhood shown.
    pub selected: Option<(u32, u32)>,
    /// The name being typed, and whose it is.
    name: (Option<(u32, u32)>, String),
}

/// The colour a neighbourhood is shown in, by its place in the list.
pub fn colour(n: usize) -> [u8; 3] {
    const PALETTE: [[u8; 3]; 8] =
        [[240, 180, 50], [70, 140, 240], [80, 200, 110], [230, 70, 70], [180, 100, 230], [60, 200, 210], [240, 120, 180], [160, 160, 170]];
    PALETTE[n % PALETTE.len()]
}

impl DistrictsUi {
    pub fn window(&mut self, ctx: &egui::Context, city: &City, out: &mut Vec<Command>) {
        let mut open = self.open;
        egui::Window::new("Neighbourhoods").open(&mut open).default_width(420.0).default_pos([240.0, 90.0]).resizable(true).show(ctx, |ui| {
            let Some(rules) = city.rules() else {
                ui.label("Neighbourhoods need GlassBox rules.");
                return;
            };
            let all = city.districts();
            if all.is_empty() {
                let office = rules.units.iter().find(|u| u.district > 0).map_or("neighbourhood office", |u| u.name.as_str());
                ui.label(format!("There are none yet. Build a {} (Services): the land around it becomes a neighbourhood.", office.to_lowercase()));
                return;
            }
            if !all.iter().any(|d| Some(d.at) == self.selected) {
                self.selected = Some(all[0].at);
            }
            egui::ScrollArea::vertical().max_height(150.0).id_salt("district_list").show(ui, |ui| {
                egui::Grid::new("districts").num_columns(5).striped(true).show(ui, |ui| {
                    for h in ["", "Residents", "Jobs", "Content", "Policy"] {
                        ui.label(RichText::new(h).small().color(Color32::GRAY));
                    }
                    ui.end_row();
                    for (n, d) in all.iter().enumerate() {
                        let [r, g, b] = colour(n);
                        let chosen = self.selected == Some(d.at);
                        if ui.selectable_label(chosen, RichText::new(format!("■ {}", d.name)).color(Color32::from_rgb(r, g, b))).clicked() {
                            self.selected = Some(d.at);
                        }
                        ui.label(RichText::new(format!("{:.0}", d.residents)).monospace());
                        ui.label(RichText::new(format!("{:.0}", d.jobs)).monospace());
                        ui.label(RichText::new(format!("{:.0}%", d.satisfaction * 100.0)).monospace());
                        let policy = d.policy.as_ref().and_then(|p| rules.district_policies.iter().find(|x| x.id == *p)).map_or("", |p| p.name.as_str());
                        ui.label(RichText::new(policy).small());
                        ui.end_row();
                    }
                });
            });
            let Some(d) = all.iter().find(|d| Some(d.at) == self.selected) else { return };
            ui.separator();
            ui.horizontal(|ui| {
                if self.name.0 != Some(d.at) {
                    self.name = (Some(d.at), d.name.clone());
                }
                let edit = ui.add(egui::TextEdit::singleline(&mut self.name.1).desired_width(190.0));
                if edit.lost_focus() && self.name.1.trim() != d.name {
                    out.push(Command::District(DistrictCmd::Rename(d.at, self.name.1.clone())));
                    self.name.0 = None;
                }
                if ui.small_button("Show").on_hover_text(format!("Look at its {}", d.office.to_lowercase())).clicked() {
                    out.push(Command::Inspect(d.at));
                }
            });
            ui.label(
                RichText::new(format!("{} tiles around the {} at {}, {} · {} buildings", d.tiles, d.office.to_lowercase(), d.at.0, d.at.1, d.buildings))
                    .small()
                    .color(Color32::GRAY),
            );
            egui::Grid::new("district_maps").num_columns(4).show(ui, |ui| {
                for (k, (name, v)) in d.maps.iter().enumerate() {
                    ui.label(RichText::new(name).small());
                    ui.label(RichText::new(format!("{v:.1}")).monospace().small());
                    if k % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            ui.label(RichText::new("Averages over its tiles.").small().color(Color32::GRAY));
            ui.separator();
            ui.label(RichText::new("Policy here").strong());
            if rules.district_policies.is_empty() {
                ui.label(RichText::new("These rules have no neighbourhood policies.").small().color(Color32::GRAY));
                return;
            }
            if ui.radio(d.policy.is_none(), "None").clicked() && d.policy.is_some() {
                out.push(Command::District(DistrictCmd::Policy(d.at, None)));
            }
            for p in &rules.district_policies {
                let on = d.policy.as_deref() == Some(p.id.as_str());
                let label = if p.cost > 0.0 { format!("{}  (${:.0} a month)", p.name, p.cost) } else { p.name.clone() };
                if ui.radio(on, label).clicked() && !on {
                    out.push(Command::District(DistrictCmd::Policy(d.at, Some(p.id.clone()))));
                }
                if !p.description.is_empty() {
                    ui.label(RichText::new(format!("      {}", p.description)).small().color(Color32::GRAY));
                }
            }
        });
        self.open = open;
    }
}
