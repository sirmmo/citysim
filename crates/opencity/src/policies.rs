//! The policy window: tax rates and the stance taken on each policy.

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::City;

use crate::ui_egui::Command;

pub fn window(ctx: &egui::Context, open: &mut bool, city: &City, out: &mut Vec<Command>) {
    let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) else { return };
    egui::Window::new("Policies").open(open).default_width(400.0).default_pos([200.0, 90.0]).resizable(true).show(ctx, |ui| {
        if rules.taxes.is_empty() && rules.policies.is_empty() {
            ui.label("These rules set no taxes or policies.");
            return;
        }
        egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
            if !rules.taxes.is_empty() {
                ui.label(RichText::new("Taxes").heading().size(15.0).color(Color32::from_rgb(200, 180, 120)));
                for t in &rules.taxes {
                    let mut rate = gb.tax_rate(rules, &t.id);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&t.name).strong()).on_hover_text(&t.description);
                        let r = ui.add(egui::Slider::new(&mut rate, t.range[0]..=t.range[1]).step_by(0.5).suffix(" %"));
                        if r.changed() {
                            out.push(Command::SetTax(t.id.clone(), rate));
                        }
                        if (rate - t.rate).abs() > 1e-3 && ui.small_button("↺").on_hover_text(format!("Back to {} %", t.rate)).clicked() {
                            out.push(Command::SetTax(t.id.clone(), t.rate));
                        }
                    });
                    let delta = rate - t.rate;
                    let effect = if delta.abs() < 1e-3 {
                        format!("Default rate {} %", t.rate)
                    } else {
                        format!("{:+.0}% income · demand {:+.1} · satisfaction {:+.0}", (rate / t.rate - 1.0) * 100.0, t.demand * delta, t.mood(rate) * 100.0)
                    };
                    ui.label(RichText::new(effect).small().color(Color32::GRAY));
                    ui.add_space(4.0);
                }
                ui.separator();
            }
            for p in &rules.policies {
                ui.label(RichText::new(&p.name).heading().size(15.0).color(Color32::from_rgb(200, 180, 120)));
                if !p.description.is_empty() {
                    ui.label(RichText::new(&p.description).italics().small());
                }
                let current = gb.stance(rules, &p.id).map(|s| s.id.clone());
                ui.horizontal_wrapped(|ui| {
                    for st in &p.stances {
                        let open = gb.stance_open(st);
                        let selected = current.as_deref() == Some(st.id.as_str());
                        let mut hover = st.description.clone();
                        if let Some(q) = st.requires.as_ref().filter(|_| !open) {
                            let name = rules.tech(q).map_or(q.as_str(), |i| rules.techs[i].name.as_str());
                            hover = format!("{hover}\nNeeds research: {name}");
                        }
                        let r = ui.add_enabled(open, egui::SelectableLabel::new(selected, &st.name)).on_hover_text(hover).on_disabled_hover_text(format!(
                            "Needs research: {}",
                            st.requires.as_deref().map_or("", |q| rules.tech(q).map_or(q, |i| rules.techs[i].name.as_str()))
                        ));
                        if r.clicked() && !selected {
                            out.push(Command::SetStance(p.id.clone(), st.id.clone()));
                        }
                    }
                });
                if let Some(st) = gb.stance(rules, &p.id) {
                    let mut parts: Vec<String> = Vec::new();
                    let name_of = |r: &str| rules.resource(r).map_or(r.to_string(), |i| rules.resources[i].name.to_lowercase());
                    parts.extend(st.bonus.iter().map(|(r, b)| format!("{:+.0}% {}", b * 100.0, name_of(r))));
                    parts.extend(st.needs.iter().map(|(r, b)| format!("{:+.0}% {} needed", b * 100.0, name_of(r))));
                    parts.extend(
                        st.demand
                            .iter()
                            .map(|(z, d)| format!("{} demand {d:+.1}", rules.zones.iter().find(|x| &x.id == z).map_or(z.as_str(), |x| x.name.as_str()))),
                    );
                    if st.satisfaction != 0.0 {
                        parts.push(format!("satisfaction {:+.0}", st.satisfaction * 100.0));
                    }
                    if st.cost != 0.0 {
                        parts.push(if st.cost > 0.0 { format!("costs ${:.1}/tick", st.cost) } else { format!("earns ${:.1}/tick", -st.cost) });
                    }
                    let text = if parts.is_empty() { st.description.clone() } else { format!("{} — {}", st.description, parts.join(" · ")) };
                    ui.label(RichText::new(text).small().color(Color32::GRAY));
                }
                ui.add_space(6.0);
            }
        });
    });
}
