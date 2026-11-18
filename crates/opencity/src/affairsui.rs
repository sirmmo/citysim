//! The decision window: an event of the rules that asks the mayor to
//! choose, with what each choice does.

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::City;

use crate::ui_egui::Command;

pub fn window(ctx: &egui::Context, open: &mut bool, city: &City, out: &mut Vec<Command>) {
    let Some(q) = city.question() else {
        *open = false;
        return;
    };
    egui::Window::new(RichText::new(&q.name).strong())
        .id(egui::Id::new("decision_window"))
        .open(open)
        .default_width(380.0)
        .default_pos([300.0, 140.0])
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_max_width(400.0);
            ui.label(&q.text);
            ui.add_space(6.0);
            let last = q.options.len().saturating_sub(1);
            for (i, o) in q.options.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    if ui.button(RichText::new(&o.label).strong()).clicked() {
                        out.push(Command::Decide(i));
                    }
                    ui.label(RichText::new(&o.does).small().color(Color32::GRAY));
                });
                if i == last {
                    let months = if q.months_left == 1 { "1 month".to_string() } else { format!("{} months", q.months_left) };
                    ui.label(RichText::new(format!("Without an answer in {months}, this is what happens.")).small().italics().color(Color32::GRAY));
                }
            }
            ui.add_space(4.0);
            ui.label(RichText::new("Sums follow the size of the city. Approval from a decision fades by a tenth a month.").small().color(Color32::GRAY));
        });
}
