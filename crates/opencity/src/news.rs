//! The news window: what needs the mayor now, and what was said before.

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::City;

use crate::ui_egui::Command;

pub fn window(ctx: &egui::Context, open: &mut bool, city: &City, out: &mut Vec<Command>) {
    egui::Window::new("News and advice").open(open).default_width(420.0).default_pos([240.0, 80.0]).resizable(true).show(ctx, |ui| {
        ui.label(RichText::new("What needs you now").strong());
        let advice = city.advice();
        if advice.is_empty() {
            ui.label(RichText::new("Nothing: the city runs itself for the moment.").color(Color32::from_rgb(120, 210, 130)));
        }
        for a in &advice {
            ui.horizontal_wrapped(|ui| {
                if let Some(at) = a.at {
                    if ui.small_button("Show").on_hover_text("Look at where it is worst").clicked() {
                        out.push(Command::Inspect(at));
                    }
                }
                ui.label(format!("• {}", a.text));
            });
        }
        ui.separator();
        ui.label(RichText::new("News").strong());
        if city.news.is_empty() {
            ui.label(RichText::new("Nothing has happened yet.").small().color(Color32::GRAY));
        }
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for n in city.news.iter().rev() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(n.date.to_string()).monospace().small().color(Color32::GRAY));
                    ui.label(&n.text);
                });
            }
        });
    });
}
