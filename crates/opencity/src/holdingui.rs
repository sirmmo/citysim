//! The OpenTactics window, shown while a city is run as a holding: whose
//! city it is, how much time is left until the next OpenTactics day, and
//! the Report to OpenTactics button.

use egui_macroquad::egui::{self, Color32, RichText};

use crate::ui_egui::Command;

/// What the window shows.
pub struct HoldingView<'a> {
    pub holding: &'a crate::holding::Holding,
    /// The latest OpenTactics day seen for the holding.
    pub day: u32,
    pub months_run: u32,
    pub months_allowed: u32,
    pub population: u32,
    pub taxes: f64,
    /// The last report link made, if any.
    pub report: Option<&'a str>,
}

pub fn window(ctx: &egui::Context, open: &mut bool, v: &HoldingView, out: &mut Vec<Command>) {
    let h = v.holding;
    egui::Window::new("OpenTactics").open(open).default_width(340.0).default_pos([640.0, 60.0]).resizable(false).show(ctx, |ui| {
        ui.set_max_width(360.0);
        let army = if h.army.is_empty() { String::new() } else { format!(" for {}", h.army) };
        ui.label(RichText::new(format!("{}{army}", h.map)).strong());
        ui.label(RichText::new(format!("Tile {}, {} · day {}", h.tile[0], h.tile[1], v.day)).small().color(Color32::GRAY));
        let left = v.months_allowed.saturating_sub(v.months_run);
        if left == 0 {
            ui.label(RichText::new(format!("Waiting for day {} in OpenTactics", v.day + 1)).color(Color32::from_rgb(250, 210, 120)))
                .on_hover_text("Time stands still until a newer link from OpenTactics arrives. You can still build, plan and report.");
        } else {
            ui.label(format!("{left} of {} months left until day {} ({} a day)", v.months_allowed, v.day + 1, h.pace));
        }
        ui.separator();
        ui.label(crate::holding::summary(v.population, v.taxes));
        ui.label(format!("Pays {}% of its income in OpenTactics", crate::holding::income_pct(v.taxes)))
            .on_hover_text("50%, plus 100% for every $400 of taxes a month");
        if ui.button("Report to OpenTactics").on_hover_text("Makes a link that takes the city's results to OpenTactics, and copies it").clicked() {
            out.push(Command::HoldingReport);
        }
        if let Some(link) = v.report {
            ui.label(RichText::new("Copied. Open it where OpenTactics runs, or send it to the commander:").small());
            let mut text = link.to_string();
            ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if ui.button("Copy again").clicked() {
                    out.push(Command::HoldingCopy);
                }
                if ui.button("Open OpenTactics").clicked() {
                    out.push(Command::HoldingOpen);
                }
            });
        }
    });
}
