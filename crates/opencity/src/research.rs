//! The research window: pooled resources (ideas, research points) and the
//! projects they unlock.

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::{rules::Transport, City};

use crate::ui_egui::Command;

pub fn window(ctx: &egui::Context, open: &mut bool, city: &City, out: &mut Vec<Command>) {
    let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) else { return };
    egui::Window::new("Research").open(open).default_width(380.0).default_pos([186.0, 80.0]).resizable(true).show(ctx, |ui| {
        // The pools, with what was made and drawn last tick.
        egui::Grid::new("pools").num_columns(3).show(ui, |ui| {
            for (i, r) in rules.resources.iter().enumerate().filter(|(_, r)| r.transport == Transport::Pool) {
                ui.label(&r.name);
                ui.label(RichText::new(format!("{:.0}", gb.pools.get(i).copied().unwrap_or(0.0))).monospace().strong());
                let made = gb.balances.produced.get(i).copied().unwrap_or(0.0);
                let used = gb.balances.delivered.get(i).copied().unwrap_or(0.0);
                ui.label(RichText::new(format!("+{made:.1} / -{used:.1} per tick")).small().color(Color32::GRAY));
                ui.end_row();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            for t in &rules.techs {
                let done = gb.researched.contains(&t.id);
                let current = gb.research.as_deref() == Some(t.id.as_str());
                let can = gb.can_research(rules, &t.id);
                ui.horizontal(|ui| {
                    let mark = if done {
                        "✔"
                    } else if current {
                        "▶"
                    } else if can {
                        "○"
                    } else {
                        "🔒"
                    };
                    ui.label(RichText::new(format!("{mark} {}", t.name)).strong().color(if done {
                        Color32::from_rgb(120, 210, 130)
                    } else {
                        Color32::LIGHT_GRAY
                    }));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if current {
                            if ui.small_button("Stop").clicked() {
                                out.push(Command::SetResearch(None));
                            }
                        } else if can && ui.small_button("Research").clicked() {
                            out.push(Command::SetResearch(Some(t.id.clone())));
                        }
                    });
                });
                if !t.description.is_empty() {
                    ui.label(RichText::new(&t.description).italics().small());
                }
                let cost: Vec<String> = t
                    .cost
                    .iter()
                    .map(|(r, c)| format!("{c:.0} {}", rules.resource(r).map_or(r.as_str(), |i| rules.resources[i].name.as_str()).to_lowercase()))
                    .collect();
                let unlocks: Vec<&str> = t.unlocks.iter().map(|u| unlock_name(rules, u)).collect();
                ui.label(RichText::new(format!("Costs {} · unlocks {}", cost.join(" + "), unlocks.join(", "))).small().color(Color32::GRAY));
                if !done && !can {
                    let missing: Vec<&str> = t
                        .requires
                        .iter()
                        .filter(|q| !gb.researched.contains(*q))
                        .map(|q| rules.tech(q).map_or(q.as_str(), |i| rules.techs[i].name.as_str()))
                        .collect();
                    ui.label(RichText::new(format!("Needs {}", missing.join(", "))).small().color(Color32::from_rgb(240, 150, 90)));
                }
                if !done {
                    let p = gb.research_progress(rules, &t.id);
                    if p > 0.0 {
                        ui.add(egui::ProgressBar::new(p).desired_height(6.0).show_percentage());
                    }
                }
                ui.add_space(6.0);
            }
        });
    });
}

/// Display name of a unit or tool id.
fn unlock_name<'a>(rules: &'a opencity_core::rules::Ruleset, id: &'a str) -> &'a str {
    if let Some(u) = rules.unit(id) {
        return &rules.units[u].name;
    }
    if let Some(u) = rules.service_unit(id) {
        return &u.name;
    }
    opencity_core::Tool::ALL.iter().find(|t| t.id() == id).and_then(|t| t.road_kind()).map_or(id, |k| k.name())
}
