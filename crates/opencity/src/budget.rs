//! The budget window: last month's money by line, this month so far,
//! service funding and loans.

use std::collections::BTreeMap;

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::City;

use crate::ui_egui::Command;

fn lines(ui: &mut egui::Ui, id: &str, ledger: &BTreeMap<String, f64>) {
    if ledger.is_empty() {
        ui.label(RichText::new("Nothing yet.").small().color(Color32::GRAY));
        return;
    }
    let (mut income, mut spending): (Vec<_>, Vec<_>) = ledger.iter().filter(|(_, v)| v.abs() >= 0.5).partition(|(_, v)| **v > 0.0);
    income.sort_by(|a, b| b.1.total_cmp(a.1));
    spending.sort_by(|a, b| a.1.total_cmp(b.1));
    egui::Grid::new(id).num_columns(2).striped(true).show(ui, |ui| {
        for (name, v) in income.iter().chain(spending.iter()) {
            ui.label(name.as_str());
            let colour = if **v > 0.0 { Color32::from_rgb(120, 210, 130) } else { Color32::from_rgb(230, 140, 110) };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(format!("{v:+.0}")).monospace().color(colour)));
            ui.end_row();
        }
        let net: f64 = ledger.values().sum();
        ui.label(RichText::new("Net").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(format!("{net:+.0}")).monospace().strong()));
        ui.end_row();
    });
}

pub fn window(ctx: &egui::Context, open: &mut bool, city: &City, out: &mut Vec<Command>) {
    egui::Window::new("Budget").open(open).default_width(380.0).default_pos([210.0, 80.0]).resizable(true).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Funds ${}", city.fund)).strong());
            ui.label(RichText::new(format!("· {}", city.date)).color(Color32::GRAY));
        });
        egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
            egui::CollapsingHeader::new("Last month").default_open(true).show(ui, |ui| lines(ui, "last", &city.last_ledger));
            egui::CollapsingHeader::new("This month so far").default_open(false).show(ui, |ui| lines(ui, "now", &city.budget()));
            // Service funding: what each kind of service costs and covers.
            if let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) {
                let mut present: BTreeMap<usize, u32> = BTreeMap::new();
                for (_, s) in city.layer.structures() {
                    if let Some(u) = s.unit.map(|u| u as usize).filter(|&u| u < rules.units.len()) {
                        if rules.services.values().any(|id| *id == rules.units[u].id) {
                            *present.entry(u).or_default() += 1;
                        }
                    }
                }
                if !present.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("Service funding").strong())
                        .on_hover_text("Funding scales what a service costs and how far and how strongly it covers the city.");
                    for (u, n) in present {
                        let def = &rules.units[u];
                        let mut pct = gb.funding_of(&def.id) * 100.0;
                        ui.horizontal(|ui| {
                            ui.label(format!("{} ×{n}", def.name));
                            if ui.add(egui::Slider::new(&mut pct, 0.0..=150.0).step_by(5.0).suffix(" %")).changed() {
                                out.push(Command::SetFunding(def.id.clone(), pct / 100.0));
                            }
                        });
                    }
                }
            }
            ui.separator();
            ui.label(RichText::new("Loans").strong()).on_hover_text("6% a year, repaid monthly over five years; three at most.");
            for l in &city.loans {
                ui.label(format!("${} · ${}/month · {} months left", l.amount, l.payment, l.months_left));
            }
            ui.horizontal(|ui| {
                for amount in [10_000, 50_000, 200_000] {
                    if ui.add_enabled(city.loans.len() < opencity_core::city::MAX_LOANS, egui::Button::new(format!("Borrow ${amount}"))).clicked() {
                        out.push(Command::TakeLoan(amount));
                    }
                }
            });
            if let Some(m) = &city.mafia {
                ui.separator();
                ui.label(
                    RichText::new(format!("The family's hold: {:.0}% · paid so far ${}", m.heat * 100.0, m.collected)).color(Color32::from_rgb(230, 140, 110)),
                )
                .on_hover_text("Police stations loosen their hold month by month.");
            }
        });
    });
}
