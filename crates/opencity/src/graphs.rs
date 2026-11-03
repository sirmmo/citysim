//! GlassBox graphs: every series the engine samples each tick, plotted,
//! toggled per series and summarised for analysis.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use egui_macroquad::egui::{self, Color32, RichText};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use opencity_core::glassbox::GlassBox;
use opencity_core::series::{Series, Stats};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Range {
    Last(usize),
    All,
}

const RANGES: &[(Range, &str)] = &[(Range::Last(100), "Last 100 ticks"), (Range::Last(500), "Last 500 ticks"), (Range::All, "Everything kept")];

/// Presets: a name and the series prefixes it shows.
const PRESETS: &[(&str, &[&str])] = &[
    ("Buildings", &["Buildings/"]),
    ("Demand", &["Demand/"]),
    ("Satisfaction", &["Satisfaction/"]),
    ("Growth churn", &["Growth/"]),
    ("Supply", &["Delivered/", "Unmet/"]),
    ("Layers", &["Layer mean/"]),
    ("Metrics", &["Metric/"]),
];

pub struct Graphs {
    pub open: bool,
    selected: BTreeSet<String>,
    filter: String,
    range: Range,
    normalize: bool,
    smooth: usize,
    note: Option<String>,
    /// The user emptied the selection (do not refill it with the default preset).
    cleared: bool,
    /// Preset to apply when data first shows up.
    pending: Option<usize>,
}

impl Default for Graphs {
    fn default() -> Self {
        Self {
            open: false,
            selected: BTreeSet::new(),
            filter: String::new(),
            range: Range::Last(500),
            normalize: false,
            smooth: 1,
            note: None,
            cleared: false,
            pending: None,
        }
    }
}

/// A stable colour per series, whatever else is shown.
fn color_of(name: &str) -> Color32 {
    let h = name.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    egui::ecolor::Hsva::new((h % 360) as f32 / 360.0, 0.7, 0.95, 1.0).into()
}

fn group_of(name: &str) -> (&str, &str) {
    name.split_once('/').unwrap_or(("Other", name))
}

fn fmt(v: f32) -> String {
    if !v.is_finite() {
        "–".into()
    } else if v.abs() >= 1000.0 {
        format!("{v:.0}")
    } else if v.abs() >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

impl Graphs {
    /// Opens the window; the preset applies once data is available.
    pub fn open_on(&mut self, preset: &str) {
        self.open = true;
        self.pending = PRESETS.iter().position(|p| p.0.eq_ignore_ascii_case(preset));
    }

    fn select_preset(&mut self, series: &Series, prefixes: &[&str]) {
        self.selected = series.names.iter().filter(|n| prefixes.iter().any(|p| n.starts_with(p))).cloned().collect();
    }

    /// Rows of the current range: first sample index.
    fn start(&self, series: &Series) -> usize {
        match self.range {
            Range::Last(n) => series.len().saturating_sub(n),
            Range::All => 0,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, gb: Option<&GlassBox>, export_dir: &Path) {
        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::G) && i.modifiers.is_none()) {
            self.open = !self.open;
        }
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("GlassBox graphs").open(&mut open).default_size([820.0, 520.0]).resizable(true).show(ctx, |ui| {
            let Some(gb) = gb.filter(|g| !g.series.is_empty()) else {
                ui.label("No GlassBox data yet. Start a city with GlassBox rules and let time run.");
                return;
            };
            let series = &gb.series;
            if let Some(p) = self.pending.take() {
                self.select_preset(series, PRESETS[p].1);
            } else if self.selected.is_empty() && !self.cleared {
                self.select_preset(series, PRESETS[0].1);
            }
            self.controls(ui, series, gb.tick, export_dir);
            ui.separator();
            egui::SidePanel::left("graph_series").resizable(true).default_width(230.0).show_inside(ui, |ui| self.series_list(ui, series));
            egui::CentralPanel::default().show_inside(ui, |ui| {
                self.plot(ui, series);
                ui.separator();
                self.stats(ui, series);
            });
        });
        self.open = open;
    }

    fn controls(&mut self, ui: &mut egui::Ui, series: &Series, tick: u64, export_dir: &Path) {
        ui.horizontal_wrapped(|ui| {
            for (name, prefixes) in PRESETS {
                if ui.small_button(*name).clicked() {
                    self.select_preset(series, prefixes);
                }
            }
            if ui.small_button("None").clicked() {
                self.selected.clear();
                self.cleared = true;
            }
        });
        ui.horizontal(|ui| {
            let label = RANGES.iter().find(|r| r.0 == self.range).map_or("", |r| r.1);
            egui::ComboBox::from_id_salt("graph_range").selected_text(label).show_ui(ui, |ui| {
                for (r, name) in RANGES {
                    ui.selectable_value(&mut self.range, *r, *name);
                }
            });
            ui.checkbox(&mut self.normalize, "Normalise").on_hover_text("Scale each series to its own maximum, to compare shapes");
            ui.label("Smoothing");
            egui::ComboBox::from_id_salt("graph_smooth").selected_text(if self.smooth <= 1 { "off".into() } else { format!("{} ticks", self.smooth) }).show_ui(
                ui,
                |ui| {
                    for n in [1, 5, 20, 50] {
                        ui.selectable_value(&mut self.smooth, n, if n == 1 { "off".into() } else { format!("{n} ticks") });
                    }
                },
            );
            if ui.button("Export CSV").on_hover_text("The shown series over everything kept; all series when none is shown").clicked() {
                let only: Vec<&str> = self.selected.iter().map(String::as_str).collect();
                let path = export_dir.join(format!("glassbox-tick{tick}.csv"));
                self.note = Some(match std::fs::create_dir_all(export_dir).and_then(|_| std::fs::write(&path, series.to_csv(&only))) {
                    Ok(()) => format!("Saved {}", path.display()),
                    Err(e) => format!("Could not save {}: {e}", path.display()),
                });
            }
            ui.label(RichText::new(format!("{} ticks kept", series.len())).small().color(Color32::GRAY));
        });
        if let Some(note) = &self.note {
            ui.label(RichText::new(note).small());
        }
    }

    fn series_list(&mut self, ui: &mut egui::Ui, series: &Series) {
        ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter series…"));
        let filter = self.filter.to_lowercase();
        let mut groups: BTreeMap<&str, Vec<(usize, &str)>> = BTreeMap::new();
        for (i, name) in series.names.iter().enumerate() {
            if filter.is_empty() || name.to_lowercase().contains(&filter) {
                let (g, label) = group_of(name);
                groups.entry(g).or_default().push((i, label));
            }
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (group, items) in groups {
                let on = items.iter().filter(|(i, _)| self.selected.contains(&series.names[*i])).count();
                let title = if on > 0 { format!("{group} ({on})") } else { group.to_string() };
                egui::CollapsingHeader::new(title).id_salt(group).default_open(on > 0 || !filter.is_empty()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button("all").clicked() {
                            self.selected.extend(items.iter().map(|(i, _)| series.names[*i].clone()));
                        }
                        if ui.small_button("none").clicked() {
                            items.iter().for_each(|(i, _)| {
                                self.selected.remove(&series.names[*i]);
                            });
                        }
                    });
                    for (i, label) in items {
                        let name = &series.names[i];
                        let last = series.columns[i].back().copied().unwrap_or(f32::NAN);
                        let mut on = self.selected.contains(name);
                        ui.horizontal(|ui| {
                            let text = RichText::new(label).color(if on { color_of(name) } else { ui.visuals().text_color() });
                            if ui.checkbox(&mut on, text).changed() {
                                if on {
                                    self.selected.insert(name.clone());
                                } else {
                                    self.selected.remove(name);
                                }
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(fmt(last)).small().monospace()));
                        });
                    }
                });
            }
        });
    }

    /// Values of a series over the range, smoothed (trailing mean) and normalised.
    fn values(&self, series: &Series, i: usize) -> Vec<[f64; 2]> {
        let start = self.start(series);
        let col = &series.columns[i];
        let raw: Vec<(u64, f32)> = series.ticks.iter().zip(col.iter()).skip(start).map(|(t, v)| (*t, *v)).collect();
        let k = self.smooth.max(1);
        let mut out: Vec<[f64; 2]> = Vec::with_capacity(raw.len());
        for j in 0..raw.len() {
            let win = &raw[j.saturating_sub(k - 1)..=j];
            let finite: Vec<f32> = win.iter().map(|x| x.1).filter(|v| v.is_finite()).collect();
            if raw[j].1.is_finite() && !finite.is_empty() {
                out.push([raw[j].0 as f64, (finite.iter().sum::<f32>() / finite.len() as f32) as f64]);
            }
        }
        if self.normalize {
            let m = out.iter().map(|p| p[1].abs()).fold(0.0, f64::max);
            if m > 0.0 {
                out.iter_mut().for_each(|p| p[1] /= m);
            }
        }
        out
    }

    fn plot(&mut self, ui: &mut egui::Ui, series: &Series) {
        let height = (ui.available_height() - 150.0).max(160.0);
        let normalize = self.normalize;
        Plot::new("glassbox_plot")
            .legend(Legend::default())
            .height(height)
            .x_axis_label("tick")
            .y_axis_label(if normalize { "share of maximum" } else { "" })
            .label_formatter(|name, v| if name.is_empty() { format!("tick {:.0}\n{:.2}", v.x, v.y) } else { format!("{name}\ntick {:.0}: {:.2}", v.x, v.y) })
            .show(ui, |p| {
                for name in &self.selected {
                    let Some(i) = series.names.iter().position(|n| n == name) else { continue };
                    let pts = self.values(series, i);
                    p.line(Line::new(PlotPoints::from(pts)).name(name).color(color_of(name)).width(1.5));
                }
            });
    }

    fn stats(&mut self, ui: &mut egui::Ui, series: &Series) {
        if self.selected.is_empty() {
            ui.label(RichText::new("Tick series on the left, or pick a preset, to plot them.").color(Color32::GRAY));
            return;
        }
        let start = self.start(series);
        let mut remove = None;
        egui::ScrollArea::vertical().id_salt("graph_stats").show(ui, |ui| {
            egui::Grid::new("graph_stats_grid").striped(true).num_columns(8).show(ui, |ui| {
                for h in ["Series", "last", "min", "max", "mean", "σ", "trend/100t", "reversals"] {
                    ui.label(RichText::new(h).small().strong());
                }
                ui.end_row();
                for name in &self.selected {
                    let Some(i) = series.names.iter().position(|n| n == name) else { continue };
                    let st = Stats::of(series.ticks.iter().skip(start).copied(), series.columns[i].iter().skip(start).copied());
                    if ui.add(egui::Label::new(RichText::new(name).color(color_of(name))).sense(egui::Sense::click())).on_hover_text("Click to hide").clicked()
                    {
                        remove = Some(name.clone());
                    }
                    match st {
                        Some(s) => {
                            for v in [s.last, s.min, s.max, s.mean, s.std_dev] {
                                ui.label(RichText::new(fmt(v)).monospace());
                            }
                            let arrow = if s.trend.abs() < 1e-3 {
                                "→"
                            } else if s.trend > 0.0 {
                                "↗"
                            } else {
                                "↘"
                            };
                            ui.label(RichText::new(format!("{arrow} {}", fmt(s.trend))).monospace());
                            // Many reversals relative to the samples: the series oscillates.
                            let n = series.len().saturating_sub(start).max(1) as f32;
                            let jittery = s.reversals as f32 > n * 0.3;
                            ui.label(RichText::new(s.reversals.to_string()).monospace().color(if jittery {
                                Color32::from_rgb(240, 150, 90)
                            } else {
                                ui.visuals().text_color()
                            }))
                            .on_hover_text("Times the series changed direction: high values mean oscillation");
                        }
                        None => {
                            for _ in 0..7 {
                                ui.label("–");
                            }
                        }
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(r) = remove {
            self.selected.remove(&r);
        }
    }
}
