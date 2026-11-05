//! The "Resource network" window: what the map's network view shows, with
//! per-resource statistics, the busiest links and the worst shortages.

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::glassbox::{GlassBox, Link};
use opencity_core::rules::{Ruleset, Transport};
use opencity_core::City;

use crate::flows::{resource_colour, FlowView};
use crate::ui_egui::Command;

pub fn colour32(c: macroquad::color::Color) -> Color32 {
    Color32::from_rgb((c.r * 255.0) as u8, (c.g * 255.0) as u8, (c.b * 255.0) as u8)
}

/// "House (12, 9)": the unit at a building origin.
pub fn unit_label(city: &City, rules: &Ruleset, at: (u32, u32)) -> String {
    let name = city.layer.structure_at(at.0, at.1).and_then(|s| s.unit).and_then(|u| rules.units.get(u as usize)).map_or("?", |u| u.name.as_str());
    format!("{name} ({}, {})", at.0, at.1)
}

fn tiles(k: &Link) -> u32 {
    k.from.0.abs_diff(k.to.0) + k.from.1.abs_diff(k.to.1)
}

pub fn window(ctx: &egui::Context, open: &mut bool, city: &City, flows: &FlowView, focus: Option<(u32, u32)>, out: &mut Vec<Command>) {
    let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) else { return };
    let mut v = flows.clone();
    egui::Window::new("Resource network").open(open).default_width(400.0).default_pos([186.0, 470.0]).resizable(true).show(ctx, |ui| {
        ui.checkbox(&mut v.on, "Show the network on the map (V)");
        ui.add_space(4.0);
        resources(ui, gb, rules, &mut v);
        ui.horizontal(|ui| {
            ui.label("Hide links under");
            ui.add(egui::Slider::new(&mut v.min_amount, 0.0..=5.0).logarithmic(true).suffix(" / tick"));
        });
        ui.checkbox(&mut v.shortages, "Tint buildings that lack a shown resource");
        ui.add_enabled(focus.is_some(), egui::Checkbox::new(&mut v.focus_only, "Only the inspected building's links"))
            .on_disabled_hover_text("Inspect a building first (/ then click)");
        egui::CollapsingHeader::new("Busiest links").id_salt("net_busiest").show(ui, |ui| busiest(ui, city, gb, rules, &v, out));
        egui::CollapsingHeader::new("Biggest shortages").id_salt("net_short").show(ui, |ui| shortages(ui, city, gb, rules, &v, out));
    });
    if v != *flows {
        out.push(Command::SetFlows(v));
    }
}

fn resources(ui: &mut egui::Ui, gb: &GlassBox, rules: &Ruleset, v: &mut FlowView) {
    let mut stats = vec![(0u32, 0f32, 0f32); rules.resources.len()];
    for (k, amount) in &gb.flows {
        let s = &mut stats[k.resource];
        s.0 += 1;
        s.1 += amount;
        s.2 += amount * tiles(k) as f32;
    }
    egui::Grid::new("network_resources").striped(true).num_columns(6).show(ui, |ui| {
        for h in ["Resource", "links", "per tick", "mean trip", "served", "unmet"] {
            ui.label(RichText::new(h).small().strong());
        }
        ui.end_row();
        for (i, r) in rules.resources.iter().enumerate() {
            if !matches!(r.transport, Transport::Agents | Transport::Grid) || r.parent.is_some() {
                continue;
            }
            // A resource with kinds: one row for the family (its links are the
            // kinds' links), then a row per kind.
            let kinds = rules.kinds(i);
            let family = rules.family(i);
            let b = &gb.balances;
            let mut on = family.iter().any(|&k| v.shows(rules, k) && (kinds.is_empty() || k != i));
            let label = RichText::new(format!("■ {}", r.name)).color(colour32(resource_colour(rules, i)));
            if ui.checkbox(&mut on, label).changed() {
                for &k in if kinds.is_empty() { &family[..] } else { &kinds[..] } {
                    if v.shows(rules, k) != on {
                        v.toggle(&rules.resources[k].id);
                    }
                }
            }
            let sum = |f: &dyn Fn(usize) -> (u32, f32, f32)| family.iter().map(|&k| f(k)).fold((0, 0.0, 0.0), |a, x| (a.0 + x.0, a.1 + x.1, a.2 + x.2));
            row(ui, sum(&|k| stats[k]), b.delivered.get(i).copied().unwrap_or(0.0), b.unmet.get(i).copied().unwrap_or(0.0));
            for &k in &kinds {
                let mut on = v.shows(rules, k);
                let kr = &rules.resources[k];
                let label = RichText::new(format!("   ■ {}", kr.name)).color(colour32(resource_colour(rules, k)));
                if ui.checkbox(&mut on, label).changed() {
                    v.toggle(&kr.id);
                }
                row(ui, stats[k], b.delivered.get(k).copied().unwrap_or(0.0), b.unmet.get(k).copied().unwrap_or(0.0));
            }
        }
    });
}

/// links, per tick, mean trip, served and unmet cells of a resource row.
fn row(ui: &mut egui::Ui, (links, total, dist): (u32, f32, f32), got: f32, unmet: f32) {
    let served = if got + unmet > 0.0 { got / (got + unmet) } else { 1.0 };
    ui.label(RichText::new(links.to_string()).monospace());
    ui.label(RichText::new(format!("{total:.0}")).monospace());
    ui.label(RichText::new(if total > 0.0 { format!("{:.1} tiles", dist / total) } else { "–".into() }).monospace());
    ui.label(RichText::new(format!("{:.0}%", served * 100.0)).monospace().color(if served < 0.9 {
        Color32::from_rgb(240, 150, 90)
    } else {
        ui.visuals().text_color()
    }));
    ui.label(RichText::new(format!("{unmet:.0}")).monospace());
    ui.end_row();
}

fn busiest(ui: &mut egui::Ui, city: &City, gb: &GlassBox, rules: &Ruleset, v: &FlowView, out: &mut Vec<Command>) {
    ui.label(RichText::new("Click to inspect the receiver").small().color(Color32::GRAY));
    let mut links: Vec<(&Link, &f32)> = gb.flows.iter().filter(|(k, _)| v.shows(rules, k.resource)).collect();
    links.sort_by(|a, b| b.1.total_cmp(a.1).then(a.0.cmp(b.0)));
    egui::Grid::new("network_busiest").num_columns(3).show(ui, |ui| {
        for (k, amount) in links.into_iter().take(8) {
            ui.label(RichText::new("■").color(colour32(resource_colour(rules, k.resource))));
            let text = format!("{}  to  {}", unit_label(city, rules, k.from), unit_label(city, rules, k.to));
            if ui.link(text).clicked() {
                out.push(Command::Inspect(k.to));
            }
            ui.label(RichText::new(format!("{amount:.1} · {} tiles", tiles(k))).monospace().small());
            ui.end_row();
        }
    });
}

fn shortages(ui: &mut egui::Ui, city: &City, gb: &GlassBox, rules: &Ruleset, v: &FlowView, out: &mut Vec<Command>) {
    ui.label(RichText::new("Click to inspect").small().color(Color32::GRAY));
    let w = city.width();
    let mut lacking: Vec<((u32, u32), f32, String)> = Vec::new();
    for (p, s) in city.layer.structures() {
        let Some(u) = s.unit.map(|u| u as usize).filter(|&u| u < rules.units.len()) else { continue };
        let short: Vec<(usize, f32)> = gb.shortfall(rules, u, p.0, p.1, w).into_iter().filter(|(r, _)| v.shows(rules, *r)).collect();
        if short.is_empty() {
            continue;
        }
        let total: f32 = short.iter().map(|x| x.1).sum();
        let what = short.iter().map(|(r, m)| format!("{} −{m:.1}", rules.resources[*r].name)).collect::<Vec<_>>().join(", ");
        lacking.push((p, total, what));
    }
    if lacking.is_empty() {
        ui.label("Every building gets what it needs.");
        return;
    }
    let n = lacking.len();
    lacking.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    egui::Grid::new("network_short").num_columns(2).show(ui, |ui| {
        for (p, _, what) in lacking.into_iter().take(8) {
            if ui.link(unit_label(city, rules, p)).clicked() {
                out.push(Command::Inspect(p));
            }
            ui.label(RichText::new(what).small().color(Color32::from_rgb(240, 150, 90)));
            ui.end_row();
        }
    });
    ui.label(RichText::new(format!("{n} buildings lack something")).small().color(Color32::GRAY));
}

/// Tooltip for the link under the mouse.
pub fn hover_tooltip(ctx: &egui::Context, city: &City, s: opencity_core::netmap::Stretch) {
    let Some(rules) = city.rules() else { return };
    let r = &rules.resources[s.resource];
    egui::show_tooltip_at_pointer(ctx, egui::LayerId::background(), egui::Id::new("flow_tip"), |ui| {
        ui.label(RichText::new(&r.name).strong().color(colour32(resource_colour(rules, s.resource))));
        ui.label(s.way.name());
        ui.label(RichText::new(format!("{:.2} per tick", s.amount)).monospace());
    });
}

/// The inspected building's links, in and out.
pub fn unit_links(ui: &mut egui::Ui, city: &City, gb: &GlassBox, rules: &Ruleset, origin: (u32, u32), out: &mut Vec<Command>) {
    let mut ins: Vec<(&Link, &f32)> = gb.flows.iter().filter(|(k, _)| k.to == origin).collect();
    let mut outs: Vec<(&Link, &f32)> = gb.flows.iter().filter(|(k, _)| k.from == origin).collect();
    if ins.is_empty() && outs.is_empty() {
        return;
    }
    ins.sort_by(|a, b| b.1.total_cmp(a.1).then(a.0.cmp(b.0)));
    outs.sort_by(|a, b| b.1.total_cmp(a.1).then(a.0.cmp(b.0)));
    let (ni, no) = (ins.len(), outs.len());
    egui::CollapsingHeader::new(format!("Network: {ni} in, {no} out")).id_salt("unit_links").default_open(true).show(ui, |ui| {
        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            egui::Grid::new("unit_links_grid").num_columns(3).show(ui, |ui| {
                for (dir, list) in [("from", ins), ("to", outs)] {
                    for (k, amount) in list {
                        let other = if dir == "from" { k.from } else { k.to };
                        ui.label(RichText::new(format!("{} {dir}", rules.resources[k.resource].name)).color(colour32(resource_colour(rules, k.resource))));
                        if ui.link(unit_label(city, rules, other)).clicked() {
                            out.push(Command::Inspect(other));
                        }
                        ui.label(RichText::new(format!("{amount:.1} · {} tiles", tiles(k))).monospace().small());
                        ui.end_row();
                    }
                }
            });
        });
    });
}
