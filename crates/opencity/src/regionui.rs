//! The region window: the grid of cities, the open city's border
//! crossings and this month's trade with its neighbours.

use egui_macroquad::egui::{self, Color32, RichText};
use opencity_core::{region::SIDES, City};

use crate::region::RegionSession;
use crate::ui_egui::Command;

/// Map symbols of the towns by size (Kenney Cartography Pack, CC0), kept
/// as egui textures between frames.
const SYMBOLS: [(&str, &[u8]); 5] = [
    ("found", include_bytes!("../../../assets/graphism/gui/map/found.png")),
    ("hamlet", include_bytes!("../../../assets/graphism/gui/map/hamlet.png")),
    ("town", include_bytes!("../../../assets/graphism/gui/map/town.png")),
    ("city", include_bytes!("../../../assets/graphism/gui/map/city.png")),
    ("capital", include_bytes!("../../../assets/graphism/gui/map/capital.png")),
];

/// The symbol for a town of `population` (None: an empty slot).
pub fn symbol_for(population: Option<u32>) -> usize {
    match population {
        None => 0,
        Some(p) if p < 500 => 1,
        Some(p) if p < 3000 => 2,
        Some(p) if p < 12000 => 3,
        Some(_) => 4,
    }
}

fn symbol(ctx: &egui::Context, k: usize) -> Option<egui::TextureHandle> {
    let (name, bytes) = SYMBOLS[k];
    let id = egui::Id::new(("region_symbol", name));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(t);
    }
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    let t = ctx.load_texture(name, color, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    Some(t)
}

pub fn window(ctx: &egui::Context, open: &mut bool, r: &RegionSession, city: &City, out: &mut Vec<Command>) {
    egui::Window::new(format!("Region: {}", r.region.name)).open(open).default_width(420.0).default_pos([220.0, 90.0]).resizable(true).show(ctx, |ui| {
        match &r.remote {
            Some(net) => {
                ui.label(RichText::new(format!("Online at {} as {}. Other towns are other players'; trade and ideas cross the borders monthly.", net.url, net.player)).small());
            }
            None => {
                ui.label(RichText::new("Click a city to play it (the open one is saved), or found a new one. Neighbours trade through roads, track or power lines that reach their shared edge; ideas spread across the region.").small());
            }
        }
        ui.add_space(4.0);
        egui::Grid::new("region_grid").spacing([6.0, 6.0]).show(ui, |ui| {
            for y in 0..r.region.size[1] {
                for x in 0..r.region.size[0] {
                    let slot = [x, y];
                    let current = r.slot == Some(slot);
                    let text = match r.region.city(slot) {
                        Some(c) => {
                            let pop = if current { city.population } else { c.population };
                            match &c.player {
                                Some(p) => format!("{}\n{} people\n👤 {p}", c.name, pop),
                                None => format!("{}\n{} people", c.name, pop),
                            }
                        }
                        None => "+ Found\na city".to_string(),
                    };
                    let text = RichText::new(text).size(12.0);
                    let k = symbol_for(r.region.city(slot).map(|c| if current { city.population } else { c.population }));
                    let mut b = match symbol(ctx, k) {
                        Some(t) => egui::Button::image_and_text(egui::Image::new((t.id(), egui::vec2(30.0, 30.0))), text),
                        None => egui::Button::new(text),
                    }
                    .min_size(egui::vec2(124.0, 48.0));
                    if current {
                        b = b.fill(Color32::from_rgb(70, 110, 70));
                    }
                    // Online, a player plays only the town they claimed.
                    let taken = r.other_player(slot).is_some() || (r.remote.is_some() && r.slot != Some(slot));
                    if ui.add_enabled(!taken, b).clicked() && !current {
                        out.push(Command::RegionCity(slot));
                    }
                }
                ui.end_row();
            }
        });
        let Some(slot) = r.slot else { return };
        ui.separator();
        let borders = city.borders();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Border crossings:").strong());
            for (side, name) in SIDES.iter().enumerate() {
                let neighbour = r.region.neighbour(slot, side);
                let open = borders[side] && neighbour.is_some_and(|n| n.borders[(side + 2) % 4]);
                let (mark, colour) = match (borders[side], neighbour, open) {
                    (_, _, true) => ("✔", Color32::from_rgb(120, 210, 130)),
                    (true, None, _) => ("–", Color32::GRAY),
                    (_, None, _) => ("·", Color32::DARK_GRAY),
                    _ => ("✖", Color32::from_rgb(220, 140, 100)),
                };
                let hover = match neighbour {
                    None => format!("No city to the {name}"),
                    Some(n) if open => format!("Trading with {}", n.name),
                    Some(n) if !borders[side] => format!("Build a road, track or power line to the {name} edge to trade with {}", n.name),
                    Some(n) => format!("{} has no crossing on its side yet", n.name),
                };
                ui.label(RichText::new(format!("{mark} {name}")).color(colour)).on_hover_text(hover);
            }
        });
        ui.label(RichText::new("This month's trade (per tick)").strong());
        if r.deals.is_empty() {
            ui.label(RichText::new("None yet: neighbours need a surplus you lack, or a lack you can fill, and crossings on both sides.").small().color(Color32::GRAY));
        } else {
            let rules = city.rules();
            egui::Grid::new("deals").num_columns(3).striped(true).show(ui, |ui| {
                for d in &r.deals {
                    let partner = r.region.city(d.partner).map_or("?", |c| c.name.as_str());
                    let (name, price) = rules
                        .and_then(|ru| ru.resource(&d.resource).map(|i| (ru.resources[i].name.clone(), ru.resources[i].price)))
                        .unwrap_or((d.resource.clone(), 0.0));
                    let (verb, colour) = if d.amount > 0.0 { ("buys", Color32::from_rgb(240, 170, 100)) } else { ("sells", Color32::from_rgb(120, 210, 130)) };
                    ui.label(partner);
                    ui.label(RichText::new(format!("{verb} {:.1} {}", d.amount.abs(), name.to_lowercase())).color(colour));
                    ui.label(RichText::new(format!("${:+.2}", -d.amount * price)).monospace());
                    ui.end_row();
                }
            });
        }
        // The gates: what went through each crossing last tick.
        let gates = city.glassbox.as_ref().map(|g| g.gates.as_slice()).unwrap_or_default();
        if !gates.is_empty() {
            ui.label(RichText::new("Through the crossings (last tick)").strong());
            let rules = city.rules();
            let name = |id: &str| rules.and_then(|ru| ru.resource(id).map(|i| ru.resources[i].name.to_lowercase())).unwrap_or_else(|| id.to_string());
            egui::Grid::new("gates").num_columns(3).striped(true).show(ui, |ui| {
                for g in gates {
                    let partner = if g.is_world() { "the world" } else { r.region.city(g.partner).map_or("?", |c| c.name.as_str()) };
                    let what = if g.is_world() { "port".to_string() } else { opencity_core::region::Crossing { side: 0, at: 0, link: g.link.clone() }.what() };
                    ui.label(format!("{what} at {},{} ↔ {partner}", g.at.0, g.at.1));
                    let list = |m: &std::collections::BTreeMap<String, f32>, plan: &std::collections::BTreeMap<String, f32>| {
                        plan.keys().map(|k| format!("{:.1}/{:.1} {}", m.get(k).copied().unwrap_or(0.0), plan[k], name(k))).collect::<Vec<_>>().join(", ")
                    };
                    ui.label(RichText::new(format!("in: {}", list(&g.carried_in, &g.import))).color(Color32::from_rgb(240, 170, 100)))
                        .on_hover_text("Brought in from the neighbour / agreed, per tick. Less than agreed: nothing needing it can be reached from this crossing.");
                    ui.label(RichText::new(format!("out: {}", list(&g.carried_out, &g.export))).color(Color32::from_rgb(120, 210, 130)));
                    ui.end_row();
                }
            });
        }
        ui.add_space(4.0);
        if ui.button("💾 Save region").clicked() {
            out.push(Command::SaveRegion);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_grow_with_the_town() {
        assert_eq!(symbol_for(None), 0);
        assert_eq!(symbol_for(Some(0)), 1);
        assert_eq!(symbol_for(Some(2500)), 2);
        assert_eq!(symbol_for(Some(50_000)), 4);
        for (name, bytes) in SYMBOLS {
            assert!(image::load_from_memory(bytes).is_ok(), "{name}");
        }
    }
}
