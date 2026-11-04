//! The research window: pooled resources (ideas, research points) and the
//! projects they unlock.

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::{
    rules::{Ruleset, TechDef, Transport},
    City,
};

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
        egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| {
            // Branches in the order the tree first names them; tiers within.
            let mut categories: Vec<&str> = Vec::new();
            for t in &rules.techs {
                if !categories.contains(&t.category.as_str()) {
                    categories.push(&t.category);
                }
            }
            for cat in categories {
                if !cat.is_empty() {
                    ui.label(RichText::new(cat).heading().size(15.0).color(Color32::from_rgb(200, 180, 120)));
                }
                let mut techs: Vec<&TechDef> = rules.techs.iter().filter(|t| t.category == cat).collect();
                techs.sort_by_key(|t| t.tier);
                for t in techs {
                    project(ui, gb, rules, t, out);
                }
            }
        });
    });
}

fn project(ui: &mut egui::Ui, gb: &opencity_core::glassbox::GlassBox, rules: &Ruleset, t: &TechDef, out: &mut Vec<Command>) {
    let done = gb.researched.contains(&t.id);
    let current = gb.research.as_deref() == Some(t.id.as_str());
    let can = gb.can_research(rules, &t.id);
    let out_ruled = gb.ruled_out(rules, &t.id);
    ui.horizontal(|ui| {
        let mark = if done {
            "✔"
        } else if current {
            "▶"
        } else if out_ruled {
            "✖"
        } else if can {
            "○"
        } else {
            "🔒"
        };
        let colour = if done {
            Color32::from_rgb(120, 210, 130)
        } else if out_ruled {
            Color32::DARK_GRAY
        } else {
            Color32::LIGHT_GRAY
        };
        ui.label(RichText::new(format!("{mark} {}", t.name)).strong().color(colour));
        ui.label(RichText::new(format!("tier {}", t.tier)).small().color(Color32::GRAY));
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
    let name_of = |r: &str| rules.resource(r).map_or(r.to_string(), |i| rules.resources[i].name.to_lowercase());
    let mut parts = vec![format!("Costs {}", t.cost.iter().map(|(r, c)| format!("{c:.0} {}", name_of(r))).collect::<Vec<_>>().join(" + "))];
    if !t.unlocks.is_empty() {
        parts.push(format!("unlocks {}", t.unlocks.iter().map(|u| unlock_name(rules, u)).collect::<Vec<_>>().join(", ")));
    }
    if !t.bonus.is_empty() {
        parts.push(t.bonus.iter().map(|(r, b)| format!("{:+.0}% {}", b * 100.0, name_of(r))).collect::<Vec<_>>().join(", "));
    }
    ui.label(RichText::new(parts.join(" · ")).small().color(Color32::GRAY));
    let tech_name = |q: &str| rules.tech(q).map_or(q.to_string(), |i| rules.techs[i].name.clone());
    if out_ruled {
        let by: Vec<String> = rules
            .techs
            .iter()
            .filter(|o| gb.researched.contains(&o.id) && (o.excludes.contains(&t.id) || t.excludes.contains(&o.id)))
            .map(|o| o.name.clone())
            .collect();
        ui.label(RichText::new(format!("Ruled out by {}", by.join(", "))).small().color(Color32::from_rgb(200, 110, 110)));
    } else if !done {
        if !t.excludes.is_empty() {
            let rivals: Vec<String> = t.excludes.iter().map(|q| tech_name(q)).collect();
            ui.label(RichText::new(format!("Rules out {}", rivals.join(", "))).small().color(Color32::from_rgb(230, 190, 90)));
        }
        if !can {
            let missing: Vec<String> = t.requires.iter().filter(|q| !gb.researched.contains(*q)).map(|q| tech_name(q)).collect();
            ui.label(RichText::new(format!("Needs {}", missing.join(", "))).small().color(Color32::from_rgb(240, 150, 90)));
        }
        let p = gb.research_progress(rules, &t.id);
        if p > 0.0 {
            ui.add(egui::ProgressBar::new(p).desired_height(6.0).show_percentage());
        }
    }
    ui.add_space(6.0);
}

/// Display name of a unit or tool id.
fn unlock_name<'a>(rules: &'a Ruleset, id: &'a str) -> &'a str {
    if let Some(u) = rules.unit(id) {
        return &rules.units[u].name;
    }
    if let Some(u) = rules.service_unit(id) {
        return &u.name;
    }
    opencity_core::Tool::ALL.iter().find(|t| t.id() == id).and_then(|t| t.road_kind()).map_or(id, |k| k.name())
}
