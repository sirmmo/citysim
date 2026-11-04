//! The goals window: a scenario's goals, how far each has come, and
//! whether the game is won or lost.

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::{goals::GoalStatus, City};

pub fn window(ctx: &egui::Context, open: &mut bool, city: &City) {
    let Some(g) = &city.goals else { return };
    egui::Window::new(format!("Goals: {}", g.name)).open(open).default_width(380.0).default_pos([230.0, 90.0]).resizable(true).show(ctx, |ui| {
        match &g.status {
            GoalStatus::Won => {
                ui.label(RichText::new("🏆 Scenario won!").heading().color(Color32::from_rgb(120, 210, 130)));
            }
            GoalStatus::Lost(why) => {
                ui.label(RichText::new(format!("Scenario lost: {why}")).heading().color(Color32::from_rgb(230, 120, 100)));
            }
            GoalStatus::Playing => {}
        }
        if !g.description.is_empty() {
            ui.label(RichText::new(&g.description).italics().small());
        }
        ui.separator();
        let years = city.date.year.saturating_sub(g.start_year);
        for (i, goal) in g.goals.iter().enumerate() {
            let v = city.goal_value(&goal.metric);
            let reached = g.reached.get(i).copied().unwrap_or(false);
            let holds = goal.holds(v);
            let (mark, colour) = match (goal.always, reached, holds) {
                (true, _, true) | (false, true, _) => ("✔", Color32::from_rgb(120, 210, 130)),
                (true, _, false) => ("✖", Color32::from_rgb(230, 120, 100)),
                _ => ("○", Color32::LIGHT_GRAY),
            };
            ui.label(RichText::new(format!("{mark} {}", goal.name)).strong().color(colour));
            let mut detail = format!("now {} · target {}", fmt(v), goal.target());
            if goal.always {
                detail.push_str(" · all game long");
            } else if let Some(y) = goal.within_years {
                detail.push_str(&format!(" · {} of {y} years left", y.saturating_sub(years)));
            }
            ui.label(RichText::new(detail).small().color(Color32::GRAY));
            if let (false, false, Some(t)) = (goal.always, reached, goal.at_least) {
                if t > 0.0 {
                    ui.add(egui::ProgressBar::new((v / t).clamp(0.0, 1.0) as f32).desired_height(6.0));
                }
            }
            ui.add_space(4.0);
        }
    });
}

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 || v.abs() >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}
