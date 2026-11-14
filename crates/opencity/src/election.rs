//! The election window (terms of office): when the next election is, how
//! it would go today, and the campaign promises.

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::mayor::{FATIGUE, MAX_PROMISES, PROMISE_BROKEN, PROMISE_KEPT, TERM_YEARS};
use opencity_core::City;

use crate::ui_egui::Command;

const GOOD: Color32 = Color32::from_rgb(120, 210, 130);
const BAD: Color32 = Color32::from_rgb(230, 120, 100);

pub fn window(ctx: &egui::Context, open: &mut bool, city: &City, out: &mut Vec<Command>) {
    let m = &city.mayor;
    egui::Window::new("Election").open(open).default_width(400.0).default_pos([260.0, 100.0]).resizable(true).show(ctx, |ui| {
        let Some(years) = city.years_to_election() else {
            ui.label("This city has no terms of office. View > Terms of office and promises starts them.");
            return;
        };
        let when = match (years, m.election_year) {
            (0, _) => "The election is at the new year.".to_string(),
            (1, Some(y)) => format!("The election is next year, on 1/1/{y}."),
            (_, Some(y)) => format!("The next election is in {years} years, on 1/1/{y}."),
            _ => String::new(),
        };
        ui.label(RichText::new(when).strong());
        let share = city.vote_forecast();
        ui.horizontal(|ui| {
            ui.label("Held today, you would get");
            ui.label(RichText::new(format!("{share:.0}%")).heading().color(if share >= 50.0 { GOOD } else { BAD }));
            ui.label("of the vote. Half wins.");
        });
        ui.add(egui::ProgressBar::new(share / 100.0).desired_height(8.0).fill(if share >= 50.0 { GOOD } else { BAD }));
        egui::Grid::new("vote").num_columns(2).show(ui, |ui| {
            ui.label("Approval");
            ui.label(RichText::new(format!("{:.0}", m.approval * 100.0)).monospace());
            ui.end_row();
            for p in &m.promises {
                let kept = city.promise_kept(p);
                ui.label(if kept { "Promise kept" } else { "Promise not kept yet" });
                ui.label(RichText::new(if kept { format!("+{PROMISE_KEPT:.0}") } else { format!("-{PROMISE_BROKEN:.0}") }).monospace());
                ui.end_row();
            }
            if m.terms_won > 0 {
                ui.label(format!("Terms served: {}", m.terms_won));
                ui.label(RichText::new(format!("-{:.0}", FATIGUE * m.terms_won.min(3) as f32)).monospace());
                ui.end_row();
            }
        });
        if let Some(v) = m.last_vote {
            ui.label(RichText::new(format!("Last election: {v:.0}% of the vote.")).small().color(Color32::GRAY));
        }
        ui.separator();
        ui.label(RichText::new(format!("Promises ({} of {MAX_PROMISES})", m.promises.len())).strong());
        if m.promises.is_empty() {
            ui.label(RichText::new("None made. A term without promises is judged on approval alone.").small().color(Color32::GRAY));
        }
        for p in &m.promises {
            let now = city.goal_value(&p.goal.metric);
            let kept = city.promise_kept(p);
            ui.label(RichText::new(format!("{} {}", if kept { "✔" } else { "○" }, p.goal.name)).color(if kept { GOOD } else { Color32::LIGHT_GRAY }));
            ui.label(RichText::new(format!("now {now:.0} · promised {}", p.goal.target())).small().color(Color32::GRAY));
        }
        if city.can_promise() {
            ui.separator();
            ui.label(RichText::new("You could promise").strong());
            ui.label(
                RichText::new(format!(
                    "A promise kept on election day is worth {PROMISE_KEPT:.0} points of the vote, one broken costs {PROMISE_BROKEN:.0}. Promises are made in the first year of a term and cannot be taken back."
                ))
                .small()
                .color(Color32::GRAY),
            );
            for (i, g) in city.promise_offers().iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("Promise").clicked() {
                        out.push(Command::Promise(i));
                    }
                    ui.label(format!("{} (now {:.0})", g.name, city.goal_value(&g.metric)));
                });
            }
        } else if m.promises.len() < MAX_PROMISES {
            ui.label(RichText::new(format!("Promises are made in the first year of a term (terms last {TERM_YEARS} years).")).small().color(Color32::GRAY));
        }
    });
}
