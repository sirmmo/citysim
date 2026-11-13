//! The rules editor: the rules file of a GlassBox rules set, its scripts,
//! tech tree and policies, edited in the game.
//!
//! * **Text**: each file with colours, completion from what the rules
//!   already define (resources, networks, units, layers, zones, projects,
//!   the fields of the section being written), and a check after every
//!   change that points at the file and line of the first problem.
//! * **Graph**: one building, resource, network or map layer at a time,
//!   with what goes in and comes out of it. A building's needs, products,
//!   the map layers it raises and those it needs can be changed, added and
//!   removed there, and so can a layer's numbers. New buildings (blank or
//!   copies), resources and map layers are created from the graph too. The
//!   rules file is rewritten in place, comments and layout kept.
//!
//! **Try on this city** runs the city on the edited rules at once; **Save**
//! writes the files to the data folder.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use egui_macroquad::egui::{self, text::LayoutJob, Color32, FontId, Pos2, Rect, RichText, Stroke, TextFormat, Vec2};
use opencity_assets::rules::{Problem, Sources};
use opencity_core::rules::{Ruleset, Transport};

use crate::ui_egui::Command;

/// Kinds of things the rules define, for colours and hints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Resource,
    Unit,
    Network,
    Layer,
    Zone,
    Tech,
    Structure,
    Field,
    Keyword,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Resource => "resource",
            Kind::Unit => "building",
            Kind::Network => "network",
            Kind::Layer => "map layer",
            Kind::Zone => "zone",
            Kind::Tech => "project",
            Kind::Structure => "structure",
            Kind::Field => "field",
            Kind::Keyword => "keyword",
        }
    }

    fn colour(self) -> Color32 {
        match self {
            Kind::Resource => Color32::from_rgb(110, 200, 220),
            Kind::Unit => Color32::from_rgb(235, 150, 200),
            Kind::Network => Color32::from_rgb(180, 150, 245),
            Kind::Layer => Color32::from_rgb(190, 220, 120),
            Kind::Zone => Color32::from_rgb(240, 200, 110),
            Kind::Tech => Color32::from_rgb(130, 220, 160),
            Kind::Structure => Color32::from_rgb(200, 200, 160),
            Kind::Field => Color32::from_rgb(150, 180, 230),
            Kind::Keyword => Color32::from_rgb(120, 160, 255),
        }
    }
}

/// Structure names networks and zones may list.
const STRUCTURES: &[&str] = &[
    "residential",
    "commercial",
    "industrial",
    "road",
    "rail",
    "power_line",
    "coal_plant",
    "nuclear_plant",
    "park",
    "fire_station",
    "police",
    "hospital",
    "school",
    "water_pipe",
    "pumping_station",
    "water_tower",
    "water_treatment",
    "dump",
    "recycling",
    "incinerator",
    "bus_stop",
    "tram_stop",
    "metro_station",
    "airport",
    "seaport",
];

/// GlassBox script words.
const KEYWORDS: &[&str] = &[
    "unitRule",
    "rule",
    "mapRule",
    "zoneRule",
    "agentRule",
    "end",
    "appliesTo",
    "via",
    "using",
    "options",
    "rate",
    "local",
    "global",
    "agent",
    "map",
    "in",
    "out",
    "radius",
    "atLeast",
    "atMost",
    "greater",
    "less",
    "is",
    "test",
    "log",
    "successEvent",
    "onFail",
    "timeTrigger",
    "sample",
    "random",
    "createUnit",
    "condition",
    "repeatAfter",
    "area",
];

/// Fields of each TOML section (by its header, `` for the top).
fn fields(section: &str) -> &'static [&'static str] {
    match section {
        "" => &[
            "id",
            "name",
            "description",
            "years",
            "disabled_tools",
            "population",
            "scripts",
            "tech_tree",
            "policy_set",
            "map_rules",
            "zone_rules",
            "agent_rules",
        ],
        "resource" => {
            &["id", "name", "transport", "network", "max_trip", "colour", "subtype", "parent", "share", "uses", "trail", "price", "shared", "weight", "tide"]
        }
        "resource.subtype" => &["id", "name", "share", "max_trip", "colour", "uses", "trail"],
        "layer" => &["id", "name", "diffusion", "decay", "colour", "max"],
        "network" => &["id", "name", "structures", "through_buildings"],
        "network.vehicle" => &["id", "name", "model", "ways", "speed", "carries", "share", "direct", "meshes", "line"],
        "unit" => &[
            "id",
            "name",
            "description",
            "graphic",
            "produce",
            "needs",
            "sends",
            "outlets",
            "emits",
            "requires",
            "build_cost",
            "logs",
            "trade",
            "dispatch",
            "round_the_clock",
        ],
        "zone" => &["id", "name", "structure", "densities", "demand", "base_demand", "round_the_clock"],
        "growth" => &["level_up", "level_down", "chance", "max_new", "road_access", "grace", "patience", "saturation", "neglect"],
        "administration" => &["name", "cost", "doubles_at", "unpaid"],
        "shifts" => &["hours", "spread"],
        "disaster" => &["id", "name", "description", "every", "radius", "effect", "protection", "full", "near_water"],
        "tax" => &["id", "name", "description", "resource", "zones", "rate", "range", "demand", "satisfaction", "steep"],
        "policy" => &["id", "name", "description"],
        "policy.stance" => &["id", "name", "description", "requires", "bonus", "needs", "demand", "satisfaction", "cost"],
        "tech" => &["id", "name", "description", "cost", "requires", "unlocks", "category", "tier", "excludes", "bonus"],
        _ => &[],
    }
}

/// Everything the rules define, by id.
#[derive(Default)]
struct Vocabulary {
    words: BTreeMap<String, (Kind, String)>,
}

impl Vocabulary {
    fn of(rules: &Ruleset) -> Self {
        let mut v = Vocabulary::default();
        let mut add = |id: &str, kind: Kind, name: &str| {
            v.words.entry(id.to_string()).or_insert((kind, name.to_string()));
        };
        for r in &rules.resources {
            add(&r.id, Kind::Resource, &r.name);
        }
        for u in &rules.units {
            add(&u.id, Kind::Unit, &u.name);
        }
        for n in &rules.networks {
            add(&n.id, Kind::Network, &n.name);
        }
        for l in &rules.layers {
            add(&l.id, Kind::Layer, &l.name);
        }
        for z in &rules.zones {
            add(&z.id, Kind::Zone, &z.name);
        }
        for t in &rules.techs {
            add(&t.id, Kind::Tech, &t.name);
        }
        for s in STRUCTURES {
            add(s, Kind::Structure, "");
        }
        v
    }

    fn kind(&self, word: &str) -> Option<Kind> {
        self.words.get(word).map(|w| w.0)
    }
}

/// What the graph shows.
#[derive(Clone, Debug, PartialEq)]
enum Focus {
    Unit(String),
    Resource(String),
    Network(String),
    Layer(String),
}

#[derive(Default)]
pub struct RulesEditor {
    pub open: bool,
    src: Option<Sources>,
    /// As last read or saved.
    saved: Option<Sources>,
    file: usize,
    graph: bool,
    /// Bumped on every change; the check runs when it moves.
    rev: u64,
    checked: Option<u64>,
    built: Option<Arc<Ruleset>>,
    vocab: Vocabulary,
    problem: Option<Problem>,
    warnings: Vec<String>,
    focus: Option<Focus>,
    search: String,
    /// Line to show in the text (file, 1-based line).
    jump: Option<(usize, usize)>,
    /// Completion: shown last frame, and which entry is picked.
    hints_shown: bool,
    hint_pick: usize,
    add_resource: String,
    note: String,
    /// An id to show in the graph once the rules are built.
    pending: Option<String>,
    /// The building, resource or map layer being created, with its id and name.
    creating: Option<NewThing>,
    new_id: String,
    new_name: String,
    /// The layer picked to add to a building's emits or requires.
    add_layer: String,
}

impl RulesEditor {
    /// Opens the editor on a rules set (the city's, usually).
    pub fn open_on(&mut self, data_dir: &Path, id: &str) {
        self.open = true;
        if self.src.as_ref().is_some_and(|s| s.id == id) {
            return;
        }
        match Sources::read(data_dir, id) {
            Ok(src) => {
                self.saved = Some(src.clone());
                self.src = Some(src);
                self.file = 0;
                self.rev += 1;
                self.focus = None;
                self.note.clear();
            }
            Err(e) => self.note = format!("Cannot read rules {id}: {e}"),
        }
    }

    /// Opens the graph on a building, resource or network by id, or the
    /// text at a hint when `what` is `text:<word>` (screenshots, docs).
    pub fn show(&mut self, data_dir: &Path, rules: &str, what: &str) {
        self.open_on(data_dir, rules);
        self.graph = !what.starts_with("text");
        self.pending = Some(what.to_string());
    }

    /// Marks the edited files as saved (after the game wrote them).
    pub fn saved(&mut self) {
        self.saved = self.src.clone();
    }

    fn dirty(&self) -> bool {
        self.src != self.saved
    }

    fn check(&mut self) {
        let Some(src) = &self.src else { return };
        if self.checked == Some(self.rev) {
            return;
        }
        self.checked = Some(self.rev);
        match src.build_with_warnings() {
            Ok((rules, warnings)) => {
                if let Some(id) = self.pending.take() {
                    self.focus = if rules.units.iter().any(|u| u.id == id) {
                        Some(Focus::Unit(id))
                    } else if rules.resource(&id).is_some() {
                        Some(Focus::Resource(id))
                    } else if rules.networks.iter().any(|n| n.id == id) {
                        Some(Focus::Network(id))
                    } else if rules.layer(&id).is_some() {
                        Some(Focus::Layer(id))
                    } else {
                        None
                    };
                }
                self.vocab = Vocabulary::of(&rules);
                self.built = Some(Arc::new(rules));
                self.problem = None;
                self.warnings = warnings;
            }
            Err(p) => self.problem = Some(p),
        }
    }

    pub fn window(&mut self, ctx: &egui::Context, data_dir: &Path, ids: &[String], city_rules: Option<&str>, out: &mut Vec<Command>) {
        if !self.open {
            return;
        }
        if self.src.is_none() {
            if let Some(id) = city_rules.or(ids.first().map(|s| s.as_str())) {
                self.open_on(data_dir, id);
            }
        }
        self.check();
        let mut open = self.open;
        egui::Window::new("Rules editor").open(&mut open).default_size([860.0, 620.0]).default_pos([200.0, 60.0]).resizable(true).show(ctx, |ui| {
            self.toolbar(ui, data_dir, ids, city_rules, out);
            ui.separator();
            if self.src.is_none() {
                ui.label(&self.note);
                return;
            }
            self.status(ui);
            ui.separator();
            if self.graph {
                self.graph_view(ui);
            } else {
                self.text_view(ui);
            }
        });
        self.open = open;
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, data_dir: &Path, ids: &[String], city_rules: Option<&str>, out: &mut Vec<Command>) {
        ui.horizontal_wrapped(|ui| {
            let current = self.src.as_ref().map(|s| s.id.clone()).unwrap_or_default();
            egui::ComboBox::from_id_salt("rules_set").selected_text(format!("Rules: {current}")).show_ui(ui, |ui| {
                for id in ids {
                    if ui.selectable_label(*id == current, id).clicked() && *id != current {
                        if self.dirty() {
                            self.note = "Save or revert your changes before opening other rules.".into();
                        } else {
                            self.src = None;
                            self.open_on(data_dir, id);
                        }
                    }
                }
            });
            ui.separator();
            ui.selectable_value(&mut self.graph, false, "📝 Text");
            ui.selectable_value(&mut self.graph, true, "🔀 Graph");
            ui.separator();
            let ok = self.problem.is_none() && self.built.is_some();
            let on_city = city_rules == Some(current.as_str());
            if ui
                .add_enabled(ok, egui::Button::new("▶ Try on this city"))
                .on_hover_text(if on_city { "Run the city on the edited rules now (not saved)" } else { "Switch the city to these rules now (not saved)" })
                .clicked()
            {
                if let Some(r) = &self.built {
                    out.push(Command::TryRules(r.clone()));
                }
            }
            if ui.add_enabled(ok && self.dirty(), egui::Button::new("💾 Save")).on_hover_text("Write the files to the data folder").clicked() {
                if let Some(src) = &self.src {
                    out.push(Command::SaveRules(src.clone()));
                }
            }
            if ui.add_enabled(self.dirty(), egui::Button::new("↺ Revert")).clicked() {
                self.src = self.saved.clone();
                self.rev += 1;
            }
            if self.dirty() {
                ui.label(RichText::new("● edited").color(Color32::from_rgb(240, 190, 90)));
            }
        });
    }

    fn status(&mut self, ui: &mut egui::Ui) {
        if !self.note.is_empty() {
            ui.label(RichText::new(&self.note).small());
        }
        match &self.problem {
            Some(p) => {
                let at = if p.line > 0 { format!("{}:{}", p.name, p.line) } else { p.name.clone() };
                let text = RichText::new(format!("✖ {at}: {}", p.message)).color(Color32::from_rgb(240, 110, 100));
                if ui.link(text).on_hover_text("Show it").clicked() {
                    self.graph = false;
                    self.file = p.file;
                    self.jump = Some((p.file, p.line.max(1)));
                }
            }
            None => {
                let r = self.built.as_ref();
                let count = r.map_or(String::new(), |r| {
                    format!(
                        "{} resources · {} networks · {} buildings · {} layers · {} zones · {} projects",
                        r.resources.len(),
                        r.networks.len(),
                        r.units.len(),
                        r.layers.len(),
                        r.zones.len(),
                        r.techs.len()
                    )
                });
                ui.label(RichText::new(format!("✔ Valid: {count}")).color(Color32::from_rgb(120, 210, 130)));
                for w in self.warnings.iter().take(3) {
                    ui.label(RichText::new(format!("⚠ {w}")).small().color(Color32::from_rgb(220, 190, 110)));
                }
            }
        }
    }

    // ------------------------------------------------------------- text

    fn text_view(&mut self, ui: &mut egui::Ui) {
        let Some(src) = self.src.as_mut() else { return };
        ui.horizontal_wrapped(|ui| {
            for (i, (name, _)) in src.files.iter().enumerate() {
                let edited = self.saved.as_ref().and_then(|s| s.files.get(i)).is_none_or(|f| f.1 != src.files[i].1);
                let label = if edited { format!("{name} ●") } else { name.clone() };
                if ui.selectable_label(self.file == i, label).clicked() {
                    self.file = i;
                }
            }
        });
        self.file = self.file.min(src.files.len().saturating_sub(1));
        let file = self.file;
        let script = src.files[file].0.ends_with(".gbx");
        let id = egui::Id::new(("rules_text", src.id.as_str(), file));
        // Keys the hint list takes before the text sees them.
        let (mut accept, mut moved) = (false, 0i32);
        if self.hints_shown {
            ui.input_mut(|i| {
                accept = i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    moved = 1;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    moved = -1;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    moved = 99;
                }
            });
        }
        let vocab = &self.vocab;
        let mut layouter = |ui: &egui::Ui, text: &str, wrap: f32| {
            let mut job = highlight(text, script, vocab);
            job.wrap.max_width = wrap;
            ui.fonts(|f| f.layout_job(job))
        };
        let mut changed = false;
        let mut hints: Vec<(String, Kind, String)> = Vec::new();
        let mut caret: Option<(Pos2, usize, usize)> = None;
        egui::ScrollArea::vertical().id_salt(("rules_scroll", file)).auto_shrink([false, false]).max_height(ui.available_height() - 4.0).show(ui, |ui| {
            let text = &mut src.files[file].1;
            let output = egui::TextEdit::multiline(text)
                .id(id)
                .code_editor()
                .desired_width(f32::INFINITY)
                .desired_rows(30)
                .lock_focus(true)
                .layouter(&mut layouter)
                .show(ui);
            changed = output.response.changed();
            if let Some((f, line)) = self.jump.filter(|j| j.0 == file) {
                let at = char_of_line(text, line);
                let mut state = output.state.clone();
                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(at))));
                state.store(ui.ctx(), id);
                let r = output.galley.pos_from_ccursor(egui::text::CCursor::new(at)).translate(output.galley_pos.to_vec2());
                ui.scroll_to_rect(r.expand(40.0), Some(egui::Align::Center));
                ui.ctx().memory_mut(|m| m.request_focus(id));
                let _ = f;
                self.jump = None;
            }
            if output.response.has_focus() {
                if let Some(c) = output.cursor_range.filter(|c| c.primary.ccursor == c.secondary.ccursor) {
                    let at = c.primary.ccursor.index;
                    let (start, word) = word_before(text, at);
                    if !word.is_empty() {
                        hints = suggestions(text, at, &word, script, vocab);
                        let pos = output.galley.pos_from_ccursor(c.primary.ccursor).translate(output.galley_pos.to_vec2()).left_bottom();
                        caret = Some((pos, start, at));
                    }
                }
            }
        });
        // The hint list under the caret.
        self.hints_shown = false;
        if let (Some((pos, start, end)), false) = (caret, hints.is_empty()) {
            if moved == 99 {
                hints.clear();
            }
            if !hints.is_empty() {
                self.hint_pick = (self.hint_pick as i32 + moved.clamp(-1, 1)).rem_euclid(hints.len() as i32) as usize;
                let mut chosen = accept.then(|| hints[self.hint_pick].0.clone());
                egui::Area::new(egui::Id::new("rules_hints")).order(egui::Order::Foreground).fixed_pos(pos + Vec2::new(0.0, 2.0)).show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        for (i, (word, kind, name)) in hints.iter().enumerate() {
                            let text = RichText::new(format!("{word}  ")).monospace().color(kind.colour());
                            let r = ui.horizontal(|ui| {
                                let r = ui.selectable_label(i == self.hint_pick, text);
                                ui.label(
                                    RichText::new(if name.is_empty() { kind.label().to_string() } else { format!("{} · {name}", kind.label()) })
                                        .small()
                                        .color(Color32::GRAY),
                                );
                                r
                            });
                            if r.inner.clicked() {
                                chosen = Some(word.clone());
                            }
                        }
                        ui.label(RichText::new("Tab to complete · ↑↓ · Esc").small().color(Color32::DARK_GRAY));
                    });
                });
                self.hints_shown = true;
                if let Some(word) = chosen {
                    let text = &mut src.files[file].1;
                    let (b0, b1) = (byte_of_char(text, start), byte_of_char(text, end));
                    text.replace_range(b0..b1, &word);
                    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(start + word.chars().count()))));
                    state.store(ui.ctx(), id);
                    ui.ctx().memory_mut(|m| m.request_focus(id));
                    changed = true;
                    self.hint_pick = 0;
                }
            }
        } else {
            self.hint_pick = 0;
        }
        if changed {
            self.rev += 1;
        }
    }

    // ------------------------------------------------------------ graph

    fn graph_view(&mut self, ui: &mut egui::Ui) {
        let Some(rules) = self.built.clone() else {
            ui.label("Fix the problem above to see the graph.");
            return;
        };
        ui.horizontal(|ui| {
            ui.label("Find:");
            ui.add(egui::TextEdit::singleline(&mut self.search).desired_width(160.0).hint_text("building, resource, network…"));
            if self.focus.is_some() && ui.button("⟲ All").clicked() {
                self.focus = None;
            }
            if let Some(f) = &self.focus {
                let id = match f {
                    Focus::Unit(i) | Focus::Resource(i) | Focus::Network(i) | Focus::Layer(i) => i.clone(),
                };
                if ui.button("📝 Show in text").clicked() {
                    if let Some((file, line)) = self.src.as_ref().and_then(|s| find_definition(s, &id)) {
                        self.file = file;
                        self.jump = Some((file, line));
                        self.graph = false;
                    }
                }
            }
        });
        if self.creating.is_some() {
            self.creation_form(ui, &rules);
            ui.separator();
        }
        let q = self.search.to_lowercase();
        if self.focus.is_none() || !q.is_empty() {
            self.overview(ui, &rules, &q);
            return;
        }
        match self.focus.clone() {
            Some(Focus::Unit(id)) => self.unit_graph(ui, &rules, &id),
            Some(Focus::Resource(id)) => resource_graph(ui, &rules, &id, &mut self.focus),
            Some(Focus::Network(id)) => network_graph(ui, &rules, &id, &mut self.focus),
            Some(Focus::Layer(id)) => self.layer_graph(ui, &rules, &id),
            None => {}
        }
    }

    /// Every resource, network and building, grouped; click one to see it.
    fn overview(&mut self, ui: &mut egui::Ui, rules: &Ruleset, q: &str) {
        let hit = |id: &str, name: &str| q.is_empty() || id.to_lowercase().contains(q) || name.to_lowercase().contains(q);
        ui.horizontal(|ui| {
            ui.label(RichText::new("New:").small());
            for (label, what) in [
                ("+ Building", NewThing::Unit { graphic: 11, copy_of: None }),
                ("+ Resource", NewThing::Resource { transport: Transport::Agents, network: rules.networks.first().map(|n| n.id.clone()).unwrap_or_default() }),
                ("+ Map layer", NewThing::Layer),
            ] {
                if ui.button(label).clicked() {
                    self.start_creating(what, "", "");
                }
            }
        });
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Map layers").strong());
            ui.horizontal_wrapped(|ui| {
                for l in rules.layers.iter().filter(|l| hit(&l.id, &l.name)) {
                    if node_button(ui, &l.name, Kind::Layer).on_hover_text(&l.id).clicked() {
                        self.focus = Some(Focus::Layer(l.id.clone()));
                        self.search.clear();
                    }
                }
            });
            ui.label(RichText::new("Networks").strong());
            ui.horizontal_wrapped(|ui| {
                for n in rules.networks.iter().filter(|n| hit(&n.id, &n.name)) {
                    if node_button(ui, &n.name, Kind::Network).clicked() {
                        self.focus = Some(Focus::Network(n.id.clone()));
                        self.search.clear();
                    }
                }
            });
            ui.label(RichText::new("Resources").strong());
            ui.horizontal_wrapped(|ui| {
                for r in rules.resources.iter().filter(|r| hit(&r.id, &r.name)) {
                    let b = node_button(ui, &r.name, Kind::Resource).on_hover_text(transport_text(rules, &r.id));
                    if b.clicked() {
                        self.focus = Some(Focus::Resource(r.id.clone()));
                        self.search.clear();
                    }
                }
            });
            // Buildings by zone, then services.
            let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
            for (i, u) in rules.units.iter().enumerate() {
                let g = rules.zone_of_unit(&u.id).map_or("Services and plants".to_string(), |z| rules.zones[z].name.clone());
                match groups.iter_mut().find(|x| x.0 == g) {
                    Some(x) => x.1.push(i),
                    None => groups.push((g, vec![i])),
                }
            }
            for (g, list) in groups {
                let list: Vec<usize> = list.into_iter().filter(|&i| hit(&rules.units[i].id, &rules.units[i].name)).collect();
                if list.is_empty() {
                    continue;
                }
                ui.label(RichText::new(g).strong());
                ui.horizontal_wrapped(|ui| {
                    for i in list {
                        let u = &rules.units[i];
                        if node_button(ui, &u.name, Kind::Unit).on_hover_text(&u.id).clicked() {
                            self.focus = Some(Focus::Unit(u.id.clone()));
                            self.search.clear();
                        }
                    }
                });
            }
        });
    }

    /// A building: what it needs on the left, what it makes on the right,
    /// both editable when it is defined in the rules file.
    fn unit_graph(&mut self, ui: &mut egui::Ui, rules: &Ruleset, id: &str) {
        let Some(u) = rules.units.iter().find(|u| u.id == id) else {
            self.focus = None;
            return;
        };
        let main = self.src.as_ref().map(|s| s.files[0].1.clone()).unwrap_or_default();
        let own = unit_tables(&main, id);
        let editable = own.is_some();
        let (own_needs, own_produce) = own.unwrap_or_default();
        let needs: Vec<(String, f32)> = u.needs.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let produce: Vec<(String, f32)> = u.produce.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let zone = rules.zone_of_unit(id).map(|z| rules.zones[z].name.clone()).unwrap_or_else(|| "service".into());
        ui.label(RichText::new(format!("{} · {zone}", u.name)).heading());
        if !u.description.is_empty() {
            ui.label(RichText::new(&u.description).italics().small());
        }
        if !editable {
            ui.label(RichText::new("Defined by a script: edit it in the text.").small().color(Color32::GRAY));
        } else if ui.button("Duplicate…").on_hover_text("A new building starting as a copy of this one").clicked() {
            self.start_creating(NewThing::Unit { graphic: u.graphic, copy_of: Some(id.to_string()) }, &format!("{id}_2"), &format!("{} (2)", u.name));
        }
        let mut edit: Option<(&'static str, String, Option<f32>)> = None;
        let mut go: Option<Focus> = None;
        let rows = needs.len().max(produce.len()).max(1);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(700.0), rows as f32 * ROW + 40.0), egui::Sense::hover());
        let centre = Rect::from_center_size(rect.center(), Vec2::new(170.0, 54.0));
        let painter = ui.painter_at(rect);
        let left = |i: usize| Rect::from_min_size(Pos2::new(rect.left() + 8.0, rect.top() + 20.0 + i as f32 * ROW), Vec2::new(230.0, ROW - 8.0));
        let right = |i: usize| Rect::from_min_size(Pos2::new(rect.right() - 238.0, rect.top() + 20.0 + i as f32 * ROW), Vec2::new(230.0, ROW - 8.0));
        for (i, (r, _)) in needs.iter().enumerate() {
            link(&painter, left(i).right_center(), centre.left_center(), resource_colour(rules, r));
        }
        for (i, (r, _)) in produce.iter().enumerate() {
            link(&painter, centre.right_center(), right(i).left_center(), resource_colour(rules, r));
        }
        painter.rect_filled(centre, 8.0, Color32::from_rgb(70, 45, 65));
        painter.rect_stroke(centre, 8.0, Stroke::new(1.5, Kind::Unit.colour()), egui::StrokeKind::Inside);
        painter.text(centre.center(), egui::Align2::CENTER_CENTER, &u.name, FontId::proportional(15.0), Color32::WHITE);
        painter.text(rect.left_top() + Vec2::new(8.0, 2.0), egui::Align2::LEFT_TOP, "needs", FontId::proportional(12.0), Color32::GRAY);
        painter.text(rect.right_top() + Vec2::new(-238.0, 2.0), egui::Align2::LEFT_TOP, "makes", FontId::proportional(12.0), Color32::GRAY);
        for (side, list, own, key) in [(0, &needs, &own_needs, "needs"), (1, &produce, &own_produce, "produce")] {
            for (i, (r, v)) in list.iter().enumerate() {
                let slot = if side == 0 { left(i) } else { right(i) };
                ui.scope_builder(egui::UiBuilder::new().max_rect(slot), |ui| {
                    ui.horizontal(|ui| {
                        let name = rules.resource(r).map_or(r.clone(), |k| rules.resources[k].name.clone());
                        if ui.add(egui::Button::new(RichText::new(name).color(resource_colour(rules, r))).min_size(Vec2::new(110.0, 0.0))).clicked() {
                            go = Some(Focus::Resource(r.clone()));
                        }
                        match own.get(r) {
                            Some(&x) if editable => {
                                let mut x = x;
                                if ui.add(egui::DragValue::new(&mut x).speed(0.05).range(0.0..=10_000.0).max_decimals(3)).changed() {
                                    edit = Some((key, r.clone(), Some(x)));
                                }
                                if ui.small_button("×").on_hover_text("Remove").clicked() {
                                    edit = Some((key, r.clone(), None));
                                }
                            }
                            _ => {
                                ui.label(RichText::new(format!("{v:.2}")).monospace());
                                ui.label(RichText::new("script").small().color(Color32::GRAY));
                            }
                        }
                    });
                });
            }
        }
        // The rows each moved the cursor; carry on below the whole graph.
        ui.advance_cursor_after_rect(rect);
        if editable {
            ui.separator();
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("add_res")
                    .selected_text(if self.add_resource.is_empty() { "resource…".to_string() } else { self.add_resource.clone() })
                    .show_ui(ui, |ui| {
                        for r in &rules.resources {
                            ui.selectable_value(&mut self.add_resource, r.id.clone(), &r.name);
                        }
                    });
                let pick = !self.add_resource.is_empty();
                if ui.add_enabled(pick, egui::Button::new("+ needs")).clicked() {
                    edit = Some(("needs", self.add_resource.clone(), Some(1.0)));
                }
                if ui.add_enabled(pick, egui::Button::new("+ makes")).clicked() {
                    edit = Some(("produce", self.add_resource.clone(), Some(1.0)));
                }
            });
        }
        if let Some((key, r, v)) = edit {
            if let Some(text) = set_unit_amount(&main, id, key, &r, v) {
                self.set_main(text);
            }
        }
        if editable {
            self.unit_layers_editor(ui, rules, id, &main, &mut go);
        }
        if let Some(f) = go {
            self.focus = Some(f);
        }
    }

    fn set_main(&mut self, text: String) {
        if let Some(src) = self.src.as_mut() {
            src.files[0].1 = text;
            self.rev += 1;
        }
    }

    fn start_creating(&mut self, what: NewThing, id: &str, name: &str) {
        self.creating = Some(what);
        self.new_id = id.to_string();
        self.new_name = name.to_string();
    }

    /// The map layers a building raises (`emits`) and needs (`requires`),
    /// editable.
    fn unit_layers_editor(&mut self, ui: &mut egui::Ui, rules: &Ruleset, id: &str, main: &str, go: &mut Option<Focus>) {
        let mut edit: Option<(&'static str, String, &'static str, Option<f32>)> = None;
        ui.separator();
        egui::Grid::new("unit_layers").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
            for (key, title, fields) in [("emits", "raises on the map", ["amount", "radius"]), ("requires", "needs on the map", ["min", "max"])] {
                ui.label(RichText::new(title).small().color(Color32::GRAY));
                ui.vertical(|ui| {
                    let list = unit_layers(main, id, key).unwrap_or_default();
                    if list.is_empty() {
                        ui.label(RichText::new("nothing").small().color(Color32::DARK_GRAY));
                    }
                    for (layer, (a, b)) in list {
                        ui.horizontal(|ui| {
                            let name = rules.layer(&layer).map_or(layer.clone(), |i| rules.layers[i].name.clone());
                            if node_button(ui, &name, Kind::Layer).clicked() {
                                *go = Some(Focus::Layer(layer.clone()));
                            }
                            for (field, v) in fields.iter().zip([a, b]) {
                                let label = match *field {
                                    "min" => "at least",
                                    "max" => "at most",
                                    f => f,
                                };
                                match v {
                                    Some(mut x) => {
                                        ui.label(RichText::new(label).small());
                                        let drag = if *field == "radius" {
                                            egui::DragValue::new(&mut x).speed(0.2).range(0.0..=64.0).max_decimals(0)
                                        } else {
                                            egui::DragValue::new(&mut x).speed(0.1).max_decimals(2)
                                        };
                                        if ui.add(drag).changed() {
                                            edit = Some((key, layer.clone(), field, Some(x)));
                                        }
                                    }
                                    None if key == "requires" => {
                                        if ui.small_button(format!("+ {label}")).clicked() {
                                            edit = Some((key, layer.clone(), field, Some(0.0)));
                                        }
                                    }
                                    None => {}
                                }
                            }
                            if ui.small_button("×").on_hover_text("Remove").clicked() {
                                edit = Some((key, layer.clone(), "", None));
                            }
                        });
                    }
                });
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("add_layer")
                .selected_text(if self.add_layer.is_empty() { "map layer…".to_string() } else { self.add_layer.clone() })
                .show_ui(ui, |ui| {
                    for l in &rules.layers {
                        ui.selectable_value(&mut self.add_layer, l.id.clone(), &l.name);
                    }
                });
            let pick = !self.add_layer.is_empty();
            if ui.add_enabled(pick, egui::Button::new("+ raises")).clicked() {
                edit = Some(("emits", self.add_layer.clone(), "amount", Some(1.0)));
            }
            if ui.add_enabled(pick, egui::Button::new("+ needs at least")).clicked() {
                edit = Some(("requires", self.add_layer.clone(), "min", Some(1.0)));
            }
            if ui.add_enabled(pick, egui::Button::new("+ needs at most")).clicked() {
                edit = Some(("requires", self.add_layer.clone(), "max", Some(10.0)));
            }
        });
        if let Some((key, layer, field, v)) = edit {
            let mut text = set_unit_layer(main, id, key, &layer, field, v);
            // A new emission reaches some way around the building.
            if let (Some(t), "emits", "amount", true) = (&text, key, field, unit_layers(main, id, key).is_some_and(|l| !l.contains_key(&layer))) {
                text = set_unit_layer(t, id, key, &layer, "radius", Some(4.0));
            }
            if let Some(t) = text {
                self.set_main(t);
            }
        }
    }

    /// A map layer: the buildings that raise it, and those that need it
    /// within bounds; its own numbers are editable.
    fn layer_graph(&mut self, ui: &mut egui::Ui, rules: &Ruleset, id: &str) {
        let Some(li) = rules.layer(id) else {
            self.focus = None;
            return;
        };
        let l = &rules.layers[li];
        ui.label(RichText::new(&l.name).heading().color(Kind::Layer.colour()));
        let main = self.src.as_ref().map(|s| s.files[0].1.clone()).unwrap_or_default();
        match layer_fields(&main, id) {
            Some(own) => {
                let mut edit = None;
                ui.horizontal(|ui| {
                    for (field, label, value, range, hint) in [
                        ("diffusion", "spreads", l.diffusion, 0.0..=1.0, "Share of a tile's value spread to its neighbours each tick"),
                        ("decay", "fades", l.decay, 0.0..=1.0, "Share of the value lost each tick"),
                        ("max", "full at", l.max, 0.0..=100_000.0, "Value shown at full colour on the map"),
                    ] {
                        let mut x = own.get(field).copied().unwrap_or(value);
                        ui.label(label).on_hover_text(hint);
                        if ui.add(egui::DragValue::new(&mut x).speed(if field == "max" { 1.0 } else { 0.01 }).range(range).max_decimals(3)).changed() {
                            edit = Some((field, x));
                        }
                    }
                });
                if let Some(t) = edit.and_then(|(f, x)| set_layer_field(&main, id, f, x)) {
                    self.set_main(t);
                }
            }
            None => {
                ui.label(RichText::new("Defined elsewhere: edit it in the text.").small().color(Color32::GRAY));
            }
        }
        let c = Kind::Layer.colour();
        let raisers: Vec<_> = rules
            .units
            .iter()
            .filter_map(|u| u.emits.get(id).map(|e| (u.name.clone(), format!("{:+.1} within {}", e.amount, e.radius), Focus::Unit(u.id.clone()), c)))
            .collect();
        let bound = |b: &opencity_core::rules::Bound| match (b.min, b.max) {
            (Some(a), Some(z)) => format!("{a}..{z}"),
            (Some(a), None) => format!("≥ {a}"),
            (None, Some(z)) => format!("≤ {z}"),
            (None, None) => String::new(),
        };
        let wanting: Vec<_> = rules.units.iter().filter_map(|u| u.requires.get(id).map(|b| (u.name.clone(), bound(b), Focus::Unit(u.id.clone()), c))).collect();
        columns(ui, &l.name, Kind::Layer, ["raised by", "needed by"], &raisers, &wanting, &mut self.focus);
    }

    /// The form for a new building, resource or map layer.
    fn creation_form(&mut self, ui: &mut egui::Ui, rules: &Ruleset) {
        let Some(mut what) = self.creating.clone() else { return };
        let title = match &what {
            NewThing::Unit { copy_of: Some(from), .. } => format!("New building, a copy of {from}"),
            NewThing::Unit { .. } => "New building".to_string(),
            NewThing::Resource { .. } => "New resource".to_string(),
            NewThing::Layer => "New map layer".to_string(),
        };
        let taken = |id: &str| {
            rules.units.iter().any(|u| u.id == id) || rules.resource(id).is_some() || rules.layer(id).is_some() || rules.networks.iter().any(|n| n.id == id)
        };
        let mut done = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new(title).strong());
            egui::Grid::new("new_thing").num_columns(2).show(ui, |ui| {
                ui.label("id");
                ui.add(egui::TextEdit::singleline(&mut self.new_id).desired_width(160.0).hint_text("lower_case_id"));
                ui.end_row();
                ui.label("name");
                ui.add(egui::TextEdit::singleline(&mut self.new_name).desired_width(220.0));
                ui.end_row();
                match &mut what {
                    NewThing::Unit { graphic, .. } => {
                        ui.label("model").on_hover_text("Graphic code of its model (see graphism.conf, or a skin's [models])");
                        let mut g = *graphic as u32;
                        if ui.add(egui::DragValue::new(&mut g).range(1..=u16::MAX as u32)).changed() {
                            *graphic = g as u16;
                        }
                        ui.end_row();
                    }
                    NewThing::Resource { transport, network } => {
                        ui.label("moves");
                        egui::ComboBox::from_id_salt("new_transport").selected_text(format!("{transport:?}").to_lowercase()).show_ui(ui, |ui| {
                            for t in [Transport::Agents, Transport::Grid, Transport::City, Transport::Pool, Transport::Local] {
                                let label = format!("{t:?}").to_lowercase();
                                ui.selectable_value(transport, t, label);
                            }
                        });
                        ui.end_row();
                        if matches!(transport, Transport::Agents | Transport::Grid) {
                            ui.label("on network");
                            egui::ComboBox::from_id_salt("new_network").selected_text(network.as_str()).show_ui(ui, |ui| {
                                for n in &rules.networks {
                                    ui.selectable_value(network, n.id.clone(), &n.name);
                                }
                            });
                            ui.end_row();
                        }
                    }
                    NewThing::Layer => {}
                }
            });
            let id = self.new_id.trim().to_string();
            let problem = if !valid_id(&id) {
                Some("The id takes lower case letters, digits and _.")
            } else if taken(&id) {
                Some("That id is taken.")
            } else {
                None
            };
            ui.horizontal(|ui| {
                if ui.add_enabled(problem.is_none(), egui::Button::new("Create")).clicked() {
                    let name = if self.new_name.trim().is_empty() { id.clone() } else { self.new_name.trim().to_string() };
                    let main = self.src.as_ref().map(|s| s.files[0].1.clone()).unwrap_or_default();
                    match add_definition(&main, &id, &name, &what) {
                        Some(t) => {
                            self.set_main(t);
                            self.pending = Some(id.clone());
                            self.note = match what {
                                NewThing::Unit { .. } => {
                                    format!("Added {id}. To build it, name it in a zone's densities or map a tool to it under [services].")
                                }
                                _ => format!("Added {id}."),
                            };
                            done = true;
                        }
                        None => self.note = format!("Could not add {id} to the rules file."),
                    }
                }
                if ui.button("Cancel").clicked() {
                    done = true;
                }
                if let (Some(p), false) = (problem, id.is_empty()) {
                    ui.label(RichText::new(p).small().color(Color32::from_rgb(240, 150, 100)));
                }
            });
        });
        self.creating = if done { None } else { Some(what) };
    }
}

const ROW: f32 = 34.0;

fn node_button(ui: &mut egui::Ui, name: &str, kind: Kind) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(name).color(kind.colour())).stroke(Stroke::new(1.0, kind.colour().gamma_multiply(0.5))).corner_radius(6.0))
}

fn link(painter: &egui::Painter, a: Pos2, b: Pos2, colour: Color32) {
    let dx = (b.x - a.x) * 0.5;
    let shape = egui::epaint::CubicBezierShape::from_points_stroke(
        [a, a + Vec2::new(dx, 0.0), b - Vec2::new(dx, 0.0), b],
        false,
        Color32::TRANSPARENT,
        Stroke::new(2.0, colour.gamma_multiply(0.8)),
    );
    painter.add(shape);
}

fn resource_colour(rules: &Ruleset, id: &str) -> Color32 {
    match rules.resource(id).and_then(|i| rules.resources[i].colour) {
        Some([r, g, b]) => Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8),
        None => Kind::Resource.colour(),
    }
}

fn transport_text(rules: &Ruleset, id: &str) -> String {
    let Some(i) = rules.resource(id) else { return String::new() };
    let r = &rules.resources[i];
    let how = match r.transport {
        Transport::Agents => "carried by agents",
        Transport::Grid => "through a grid",
        Transport::City => "through the treasury",
        Transport::Pool => "into a city-wide pool",
        Transport::Local => "kept where it is made",
    };
    let net = r.network.as_ref().map(|n| format!(" on {n}")).unwrap_or_default();
    let price = if r.price > 0.0 { format!(", traded at {}", r.price) } else { String::new() };
    format!("{how}{net}{price}")
}

/// Three columns: `left` → centre → `right`, each entry a button that
/// focuses it, links coloured `colour`.
#[allow(clippy::type_complexity)]
fn columns(
    ui: &mut egui::Ui,
    centre_name: &str,
    centre_kind: Kind,
    labels: [&str; 2],
    left: &[(String, String, Focus, Color32)],
    right: &[(String, String, Focus, Color32)],
    focus: &mut Option<Focus>,
) {
    let rows = left.len().max(right.len()).max(1);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(700.0), rows as f32 * ROW + 40.0), egui::Sense::hover());
        let centre = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 20.0 + (rows as f32 * ROW).min(320.0) / 2.0), Vec2::new(170.0, 54.0));
        let painter = ui.painter_at(rect);
        let slot = |side: usize, i: usize| {
            let x = if side == 0 { rect.left() + 8.0 } else { rect.right() - 238.0 };
            Rect::from_min_size(Pos2::new(x, rect.top() + 20.0 + i as f32 * ROW), Vec2::new(230.0, ROW - 8.0))
        };
        for (i, e) in left.iter().enumerate() {
            link(&painter, slot(0, i).right_center(), centre.left_center(), e.3);
        }
        for (i, e) in right.iter().enumerate() {
            link(&painter, centre.right_center(), slot(1, i).left_center(), e.3);
        }
        painter.rect_filled(centre, 8.0, Color32::from_rgb(40, 50, 70));
        painter.rect_stroke(centre, 8.0, Stroke::new(1.5, centre_kind.colour()), egui::StrokeKind::Inside);
        painter.text(centre.center(), egui::Align2::CENTER_CENTER, centre_name, FontId::proportional(15.0), Color32::WHITE);
        painter.text(rect.left_top() + Vec2::new(8.0, 2.0), egui::Align2::LEFT_TOP, labels[0], FontId::proportional(12.0), Color32::GRAY);
        painter.text(rect.right_top() + Vec2::new(-238.0, 2.0), egui::Align2::LEFT_TOP, labels[1], FontId::proportional(12.0), Color32::GRAY);
        for (side, list) in [(0, left), (1, right)] {
            for (i, (name, amount, f, colour)) in list.iter().enumerate() {
                ui.scope_builder(egui::UiBuilder::new().max_rect(slot(side, i)), |ui| {
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new(name).color(*colour)).min_size(Vec2::new(130.0, 0.0))).clicked() {
                            *focus = Some(f.clone());
                        }
                        ui.label(RichText::new(amount).monospace().small());
                    });
                });
            }
        }
        ui.advance_cursor_after_rect(rect);
    });
}

/// A resource: the buildings that make it and the ones that need it.
fn resource_graph(ui: &mut egui::Ui, rules: &Ruleset, id: &str, focus: &mut Option<Focus>) {
    let Some(ri) = rules.resource(id) else {
        *focus = None;
        return;
    };
    let r = &rules.resources[ri];
    ui.horizontal(|ui| {
        ui.label(RichText::new(&r.name).heading().color(resource_colour(rules, id)));
        ui.label(transport_text(rules, id));
        if let Some(n) = &r.network {
            if node_button(ui, n, Kind::Network).clicked() {
                *focus = Some(Focus::Network(n.clone()));
            }
        }
    });
    let c = resource_colour(rules, id);
    let makers: Vec<_> =
        rules.units.iter().filter_map(|u| u.produce.get(id).map(|v| (u.name.clone(), format!("{v:.2}"), Focus::Unit(u.id.clone()), c))).collect();
    let users: Vec<_> = rules.units.iter().filter_map(|u| u.needs.get(id).map(|v| (u.name.clone(), format!("{v:.2}"), Focus::Unit(u.id.clone()), c))).collect();
    columns(ui, &r.name, Kind::Resource, ["made by", "needed by"], &makers, &users, focus);
}

/// A network: what it is made of and what travels on it.
fn network_graph(ui: &mut egui::Ui, rules: &Ruleset, id: &str, focus: &mut Option<Focus>) {
    let Some(n) = rules.networks.iter().find(|n| n.id == id) else {
        *focus = None;
        return;
    };
    ui.label(RichText::new(&n.name).heading().color(Kind::Network.colour()));
    let mut parts = n.structures.join(", ");
    if n.through_buildings {
        parts.push_str(", and through buildings");
    }
    ui.label(format!("Made of: {parts}"));
    if !n.vehicles.is_empty() {
        ui.label(format!("Vehicles: {}", n.vehicles.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join(", ")));
    }
    let carried: Vec<_> = rules
        .resources
        .iter()
        .filter(|r| r.network.as_deref() == Some(id))
        .map(|r| (r.name.clone(), transport_text(rules, &r.id), Focus::Resource(r.id.clone()), resource_colour(rules, &r.id)))
        .collect();
    let others: Vec<_> =
        rules.networks.iter().filter(|o| o.id != id).map(|o| (o.name.clone(), String::new(), Focus::Network(o.id.clone()), Kind::Network.colour())).collect();
    columns(ui, &n.name, Kind::Network, ["other networks", "carries"], &others, &carried, focus);
}

// ------------------------------------------------------------ text helpers

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.' || c == '-'
}

/// The word that ends at char `at`: its start (in chars) and text.
fn word_before(text: &str, at: usize) -> (usize, String) {
    let chars: Vec<char> = text.chars().take(at).collect();
    let mut s = chars.len();
    while s > 0 && is_word(chars[s - 1]) {
        s -= 1;
    }
    (s, chars[s..].iter().collect())
}

fn byte_of_char(text: &str, c: usize) -> usize {
    text.char_indices().nth(c).map_or(text.len(), |(b, _)| b)
}

fn char_of_line(text: &str, line: usize) -> usize {
    let mut chars = 0;
    for (i, l) in text.split_inclusive('\n').enumerate() {
        if i + 1 >= line {
            return chars;
        }
        chars += l.chars().count();
    }
    chars
}

/// The TOML section the cursor is in (`unit`, `network.vehicle`…), and the
/// key of its line (empty before the `=`).
fn context(text: &str, at_byte: usize) -> (String, String) {
    let before = &text[..at_byte];
    let section = before
        .lines()
        .rev()
        .find_map(|l| {
            let t = l.trim();
            t.starts_with('[').then(|| t.trim_matches(|c| c == '[' || c == ']').trim().to_string())
        })
        .unwrap_or_default();
    let line = before.rsplit('\n').next().unwrap_or("");
    let key = line.split_once('=').map(|(k, _)| k.trim().to_string()).unwrap_or_default();
    (section, key)
}

/// The kinds a key takes, most likely first.
fn wanted(section: &str, key: &str, line: &str, script: bool) -> Vec<Kind> {
    use Kind::*;
    if script {
        let first = line.split_whitespace().next().unwrap_or("");
        return match first {
            "appliesTo" | "createUnit" => vec![Unit, Zone, Network, Keyword],
            "map" => vec![Layer, Keyword],
            "local" | "global" | "agent" | "test" | "log" | "condition" => vec![Resource, Keyword],
            "" => vec![Keyword],
            _ => vec![Keyword, Resource, Layer, Unit, Network],
        };
    }
    let k = key.rsplit('.').next().unwrap_or(key);
    match (section, k) {
        ("tech", "requires" | "excludes") | ("policy.stance", "requires") => vec![Tech],
        ("tech", "unlocks") => vec![Unit, Structure],
        ("tech" | "policy.stance", "bonus") => vec![Resource],
        (_, "produce" | "needs" | "sends" | "uses" | "resource" | "carries" | "population" | "parent" | "logs" | "cost") => vec![Resource],
        (_, "network") => vec![Network],
        (_, "structures" | "structure") => vec![Structure],
        (_, "requires" | "emits") => vec![Layer],
        (_, "units") | ("densities", _) => vec![Unit],
        (_, "zones") => vec![Zone],
        _ => vec![Resource, Unit, Layer, Network, Zone, Tech],
    }
}

/// Completions for `word` at char `at`.
fn suggestions(text: &str, at: usize, word: &str, script: bool, vocab: &Vocabulary) -> Vec<(String, Kind, String)> {
    let at_byte = byte_of_char(text, at);
    let (section, key) = context(text, at_byte);
    let line = text[..at_byte].rsplit('\n').next().unwrap_or("");
    let lw = word.to_lowercase();
    let mut out: Vec<(usize, String, Kind, String)> = Vec::new();
    // A TOML key being written: the fields of its section.
    if !script && !line.contains('=') && !line.trim_start().starts_with('[') {
        for f in fields(&section) {
            if f.starts_with(&lw) && *f != word {
                out.push((0, f.to_string(), Kind::Field, String::new()));
            }
        }
    } else {
        let kinds = wanted(&section, &key, line, script);
        for (id, (kind, name)) in &vocab.words {
            if let Some(rank) = kinds.iter().position(|k| k == kind) {
                if (id.to_lowercase().starts_with(&lw) || name.to_lowercase().starts_with(&lw)) && id != word {
                    out.push((rank, id.clone(), *kind, name.clone()));
                }
            }
        }
        // Structure names may also be ids of other kinds (`road`).
        if let Some(rank) = kinds.iter().position(|k| *k == Kind::Structure) {
            for st in STRUCTURES.iter().filter(|st| st.starts_with(&lw) && **st != word) {
                if !out.iter().any(|o| o.1 == *st) {
                    out.push((rank, st.to_string(), Kind::Structure, String::new()));
                }
            }
        }
        if script {
            let rank = kinds.iter().position(|k| *k == Kind::Keyword).unwrap_or(9);
            for k in KEYWORDS.iter().filter(|k| k.to_lowercase().starts_with(&lw) && **k != word) {
                out.push((rank, k.to_string(), Kind::Keyword, String::new()));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.len().cmp(&b.1.len())).then(a.1.cmp(&b.1)));
    out.into_iter().take(8).map(|(_, w, k, n)| (w, k, n)).collect()
}

/// Colours: comments, section headers, keys, strings, numbers, and every
/// id the rules define in the colour of its kind.
fn highlight(text: &str, script: bool, vocab: &Vocabulary) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font = FontId::monospace(13.0);
    let plain = Color32::from_gray(210);
    let mut put = |s: &str, c: Color32| job.append(s, 0.0, TextFormat::simple(font.clone(), c));
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || (script && trimmed.starts_with("//")) {
            put(line, Color32::from_gray(120));
            continue;
        }
        if !script && trimmed.starts_with('[') {
            put(line, Color32::from_rgb(230, 190, 100));
            continue;
        }
        // Up to a comment; the key before `=` (TOML).
        let (code, comment) = match line.find('#').or_else(|| if script { line.find("//") } else { None }) {
            Some(i) if !line[..i].contains('"') || line[..i].matches('"').count() % 2 == 0 => line.split_at(i),
            _ => (line, ""),
        };
        let mut rest = code;
        if !script {
            if let Some(eq) = rest.find('=') {
                let (k, v) = rest.split_at(eq);
                put(k, Kind::Field.colour());
                rest = v;
            }
        }
        let mut word = String::new();
        let mut in_str = false;
        let mut s = String::new();
        let flush_word = |word: &mut String, put: &mut dyn FnMut(&str, Color32)| {
            if word.is_empty() {
                return;
            }
            let c = if let Some(k) = vocab.kind(word) {
                k.colour()
            } else if word.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '-') {
                Color32::from_rgb(240, 170, 110)
            } else if script && KEYWORDS.contains(&word.as_str()) {
                Kind::Keyword.colour()
            } else if word == "true" || word == "false" {
                Color32::from_rgb(240, 170, 110)
            } else {
                plain
            };
            put(word, c);
            word.clear();
        };
        for ch in rest.chars() {
            if in_str {
                if ch == '"' {
                    // The string's content: an id in its kind's colour.
                    let c = vocab.kind(&s).map_or(Color32::from_rgb(160, 210, 140), |k| k.colour());
                    put(&s, c);
                    put("\"", Color32::from_rgb(160, 210, 140));
                    s.clear();
                    in_str = false;
                } else {
                    s.push(ch);
                }
                continue;
            }
            if is_word(ch) {
                word.push(ch);
                continue;
            }
            flush_word(&mut word, &mut put);
            if ch == '"' {
                put("\"", Color32::from_rgb(160, 210, 140));
                in_str = true;
            } else {
                let mut b = [0u8; 4];
                put(ch.encode_utf8(&mut b), plain);
            }
        }
        flush_word(&mut word, &mut put);
        if in_str {
            put(&s, Color32::from_rgb(160, 210, 140));
        }
        if !comment.is_empty() {
            put(comment, Color32::from_gray(120));
        }
    }
    job
}

/// Where an id is defined: `id = "x"` in the TOML files, or a script rule
/// that names it.
fn find_definition(src: &Sources, id: &str) -> Option<(usize, usize)> {
    let pat = format!("id = \"{id}\"");
    for (i, (_, text)) in src.files.iter().enumerate() {
        if let Some(n) = text.lines().position(|l| l.trim_start().starts_with(&pat)) {
            return Some((i, n + 1));
        }
    }
    for (i, (name, text)) in src.files.iter().enumerate().filter(|(_, f)| f.0.ends_with(".gbx")) {
        let _ = name;
        if let Some(n) = text.lines().position(|l| l.split_whitespace().any(|w| w == id)) {
            return Some((i, n + 1));
        }
    }
    None
}

/// A building's own `needs` and `produce` in the rules file, when it has a
/// `[[unit]]` there.
fn unit_tables(text: &str, id: &str) -> Option<(BTreeMap<String, f32>, BTreeMap<String, f32>)> {
    let doc: toml_edit::DocumentMut = text.parse().ok()?;
    let units = doc.get("unit")?.as_array_of_tables()?;
    let t = units.iter().find(|t| t.get("id").and_then(|v| v.as_str()) == Some(id))?;
    let read = |key: &str| -> BTreeMap<String, f32> {
        let mut m = BTreeMap::new();
        if let Some(tbl) = t.get(key).and_then(|v| v.as_table_like()) {
            for (k, v) in tbl.iter() {
                if let Some(x) = v.as_float().or_else(|| v.as_integer().map(|i| i as f64)) {
                    m.insert(k.to_string(), x as f32);
                }
            }
        }
        m
    };
    Some((read("needs"), read("produce")))
}

/// Sets (or with `None` removes) one amount of a building's `needs` or
/// `produce`, keeping the rest of the file as written.
fn set_unit_amount(text: &str, id: &str, key: &str, resource: &str, value: Option<f32>) -> Option<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().ok()?;
    let units = doc.get_mut("unit")?.as_array_of_tables_mut()?;
    let t = units.iter_mut().find(|t| t.get("id").and_then(|v| v.as_str()) == Some(id))?;
    if t.get(key).is_none() {
        value?;
        t.insert(key, toml_edit::value(toml_edit::InlineTable::new()));
    }
    let tbl = t.get_mut(key)?.as_table_like_mut()?;
    match value {
        Some(v) => {
            // Whole numbers stay integers; others keep three decimals.
            let v = (v as f64 * 1000.0).round() / 1000.0;
            let item = if v.fract() == 0.0 { toml_edit::value(v as i64) } else { toml_edit::value(v) };
            tbl.insert(resource, item);
        }
        None => {
            tbl.remove(resource);
        }
    }
    if let Some(it) = t.get_mut(key).and_then(|i| i.as_inline_table_mut()) {
        it.fmt();
    }
    Some(doc.to_string())
}

/// A building's `emits` or `requires` in the rules file: per layer, the
/// two numbers it sets (`amount`, `radius` for emits; `min`, `max` for
/// requires), when it has a `[[unit]]` there.
type LayerAmounts = BTreeMap<String, (Option<f32>, Option<f32>)>;

fn unit_layers(text: &str, id: &str, key: &str) -> Option<LayerAmounts> {
    let doc: toml_edit::DocumentMut = text.parse().ok()?;
    let units = doc.get("unit")?.as_array_of_tables()?;
    let t = units.iter().find(|t| t.get("id").and_then(|v| v.as_str()) == Some(id))?;
    let fields = if key == "emits" { ["amount", "radius"] } else { ["min", "max"] };
    let mut m = BTreeMap::new();
    if let Some(tbl) = t.get(key).and_then(|v| v.as_table_like()) {
        for (layer, v) in tbl.iter() {
            let num =
                |f: &str| v.as_table_like().and_then(|e| e.get(f)).and_then(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64))).map(|x| x as f32);
            m.insert(layer.to_string(), (num(fields[0]), num(fields[1])));
        }
    }
    Some(m)
}

fn number(v: f32) -> toml_edit::Value {
    let v = (v as f64 * 1000.0).round() / 1000.0;
    if v.fract() == 0.0 {
        (v as i64).into()
    } else {
        v.into()
    }
}

/// Sets one number of a building's `emits` or `requires` entry for a layer
/// (`None` removes the number; `field` empty removes the whole entry),
/// keeping the rest of the file as written.
fn set_unit_layer(text: &str, id: &str, key: &str, layer: &str, field: &str, value: Option<f32>) -> Option<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().ok()?;
    let units = doc.get_mut("unit")?.as_array_of_tables_mut()?;
    let t = units.iter_mut().find(|t| t.get("id").and_then(|v| v.as_str()) == Some(id))?;
    if t.get(key).is_none() {
        value?;
        t.insert(key, toml_edit::value(toml_edit::InlineTable::new()));
    }
    let tbl = t.get_mut(key)?.as_table_like_mut()?;
    if field.is_empty() {
        tbl.remove(layer);
    } else {
        if tbl.get(layer).is_none() {
            value?;
            tbl.insert(layer, toml_edit::Item::Value(toml_edit::Value::InlineTable(toml_edit::InlineTable::new())));
        }
        let entry = tbl.get_mut(layer)?.as_table_like_mut()?;
        match value {
            Some(v) => {
                entry.insert(field, toml_edit::Item::Value(number(v)));
            }
            None => {
                entry.remove(field);
            }
        }
        if let Some(e) = tbl.get_mut(layer).and_then(|i| i.as_value_mut()).and_then(|v| v.as_inline_table_mut()) {
            e.fmt();
        }
    }
    if let Some(it) = t.get_mut(key).and_then(|i| i.as_inline_table_mut()) {
        it.fmt();
    }
    Some(doc.to_string())
}

/// The numbers a `[[layer]]` sets in the rules file.
fn layer_fields(text: &str, id: &str) -> Option<BTreeMap<String, f32>> {
    let doc: toml_edit::DocumentMut = text.parse().ok()?;
    let t = doc.get("layer")?.as_array_of_tables()?.iter().find(|t| t.get("id").and_then(|v| v.as_str()) == Some(id))?;
    Some(
        ["diffusion", "decay", "max"]
            .iter()
            .filter_map(|f| t.get(f).and_then(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64))).map(|x| (f.to_string(), x as f32)))
            .collect(),
    )
}

fn set_layer_field(text: &str, id: &str, field: &str, value: f32) -> Option<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().ok()?;
    let t = doc.get_mut("layer")?.as_array_of_tables_mut()?.iter_mut().find(|t| t.get("id").and_then(|v| v.as_str()) == Some(id))?;
    t.insert(field, toml_edit::Item::Value(number(value)));
    Some(doc.to_string())
}

/// Something new to add to the rules file from the graph.
#[derive(Clone, Debug, PartialEq)]
enum NewThing {
    /// A building, blank or a copy of another (by id).
    Unit {
        graphic: u16,
        copy_of: Option<String>,
    },
    Resource {
        transport: Transport,
        network: String,
    },
    Layer,
}

/// Appends a `[[unit]]`, `[[resource]]` or `[[layer]]` to the rules file:
/// a copy of another building goes right after it, the rest at the end.
fn add_definition(text: &str, id: &str, name: &str, what: &NewThing) -> Option<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().ok()?;
    let array = match what {
        NewThing::Unit { .. } => "unit",
        NewThing::Resource { .. } => "resource",
        NewThing::Layer => "layer",
    };
    let exists = |doc: &toml_edit::DocumentMut| {
        doc.get(array).and_then(|a| a.as_array_of_tables()).is_some_and(|a| a.iter().any(|t| t.get("id").and_then(|v| v.as_str()) == Some(id)))
    };
    if id.is_empty() || exists(&doc) {
        return None;
    }
    let mut t = match what {
        NewThing::Unit { copy_of: Some(from), .. } => {
            let src = doc.get("unit")?.as_array_of_tables()?.iter().find(|t| t.get("id").and_then(|v| v.as_str()) == Some(from))?.clone();
            let mut t = src;
            t.decor_mut().clear();
            t
        }
        _ => {
            let mut t = toml_edit::Table::new();
            t.set_position(usize::MAX);
            t
        }
    };
    t.decor_mut().set_prefix("\n");
    t.insert("id", toml_edit::value(id));
    t.insert("name", toml_edit::value(name));
    match what {
        NewThing::Unit { graphic, copy_of } => {
            if copy_of.is_none() {
                t.insert("graphic", toml_edit::value(*graphic as i64));
                t.insert("needs", toml_edit::value(toml_edit::InlineTable::new()));
                t.insert("produce", toml_edit::value(toml_edit::InlineTable::new()));
            } else {
                t.insert("graphic", toml_edit::value(*graphic as i64));
            }
        }
        NewThing::Resource { transport, network } => {
            let tr = match transport {
                Transport::Agents => "agents",
                Transport::Grid => "grid",
                Transport::City => "city",
                Transport::Pool => "pool",
                Transport::Local => "local",
            };
            t.insert("transport", toml_edit::value(tr));
            if matches!(transport, Transport::Agents | Transport::Grid) {
                t.insert("network", toml_edit::value(network.as_str()));
            }
            if *transport == Transport::Agents {
                t.insert("max_trip", toml_edit::value(30));
            }
        }
        NewThing::Layer => {
            t.insert("decay", toml_edit::value(0.1));
            t.insert("max", toml_edit::value(100));
        }
    }
    if doc.get(array).is_none() {
        doc.insert(array, toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));
    }
    doc.get_mut(array)?.as_array_of_tables_mut()?.push(t);
    Some(doc.to_string())
}

/// Whether an id is fit for the rules: lower case letters, digits and `_`.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') && !id.starts_with(|c: char| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: &str = "# homes\n[[unit]]\nid = \"house\"   # small\nname = \"House\"\ngraphic = 1\nproduce = { residents = 4, workers = 2 }\nneeds = { power = 1 }\n\n[[unit]]\nid = \"shop\"\nname = \"Shop\"\ngraphic = 2\n";

    #[test]
    fn amounts_are_edited_in_place() {
        let t = set_unit_amount(UNIT, "house", "needs", "power", Some(2.5)).unwrap();
        assert!(t.contains("needs = { power = 2.5 }"), "{t}");
        assert!(t.contains("# homes") && t.contains("id = \"house\"   # small"), "comments and layout kept: {t}");
        let t = set_unit_amount(&t, "house", "produce", "workers", None).unwrap();
        assert!(t.contains("produce = { residents = 4 }"), "{t}");
        let t = set_unit_amount(&t, "shop", "needs", "workers", Some(3.0)).unwrap();
        let (needs, produce) = unit_tables(&t, "shop").unwrap();
        assert_eq!(needs.get("workers"), Some(&3.0));
        assert!(produce.is_empty());
        assert!(unit_tables(&t, "nowhere").is_none());
    }

    #[test]
    fn layers_and_new_definitions_are_edited_in_place() {
        let text = format!("{UNIT}\n[[layer]]\nid = \"smog\"   # air\nname = \"Smog\"\ndecay = 0.2\n");
        let t = set_unit_layer(&text, "house", "emits", "smog", "amount", Some(3.0)).unwrap();
        let t = set_unit_layer(&t, "house", "emits", "smog", "radius", Some(4.0)).unwrap();
        assert!(t.contains("emits = { smog = { amount = 3, radius = 4 } }"), "{t}");
        let t = set_unit_layer(&t, "shop", "requires", "smog", "max", Some(20.0)).unwrap();
        assert_eq!(unit_layers(&t, "shop", "requires").unwrap().get("smog"), Some(&(None, Some(20.0))));
        let t = set_unit_layer(&t, "house", "emits", "smog", "", None).unwrap();
        assert!(unit_layers(&t, "house", "emits").unwrap().is_empty(), "{t}");
        let t = set_layer_field(&t, "smog", "max", 50.0).unwrap();
        assert_eq!(layer_fields(&t, "smog").unwrap().get("max"), Some(&50.0));
        assert!(t.contains("id = \"smog\"   # air"), "comments kept: {t}");
        // A copy of a building follows it; new things go at the end.
        let t = add_definition(&t, "house_2", "Big house", &NewThing::Unit { graphic: 7, copy_of: Some("house".into()) }).unwrap();
        let (needs, produce) = unit_tables(&t, "house_2").unwrap();
        assert_eq!((needs.get("power"), produce.get("residents")), (Some(&1.0), Some(&4.0)), "{t}");
        assert!(t.find("id = \"house_2\"").unwrap() < t.find("id = \"shop\"").unwrap(), "{t}");
        let t = add_definition(&t, "fish", "Fish", &NewThing::Resource { transport: Transport::Agents, network: "road".into() }).unwrap();
        let t = add_definition(&t, "noise", "Noise", &NewThing::Layer).unwrap();
        assert!(t.trim_end().ends_with("max = 100"), "{t}");
        assert!(t.contains("[[resource]]\nid = \"fish\"\nname = \"Fish\"\ntransport = \"agents\"\nnetwork = \"road\""), "{t}");
        assert!(add_definition(&t, "fish", "Again", &NewThing::Layer).is_none() || layer_fields(&t, "fish").is_none());
        assert!(add_definition(&t, "house", "Twice", &NewThing::Unit { graphic: 1, copy_of: None }).is_none(), "ids stay unique");
        assert!(valid_id("tram_depot") && !valid_id("Tram") && !valid_id("1st") && !valid_id(""));
    }

    #[test]
    fn hints_follow_the_key() {
        let rules = {
            let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
            opencity_assets::rules::load(&dir, "glassbox").unwrap()
        };
        let v = Vocabulary::of(&rules);
        let text = "[[unit]]\nid = \"x\"\nneeds = { po";
        let h = suggestions(text, text.chars().count(), "po", false, &v);
        assert_eq!(h.first().map(|h| (h.0.as_str(), h.1)), Some(("power", Kind::Resource)), "{h:?}");
        let text = "[[network]]\nid = \"x\"\nstructures = [\"ro";
        let h = suggestions(text, text.chars().count(), "ro", false, &v);
        assert_eq!(h.first().map(|h| h.0.as_str()), Some("road"));
        let text = "[[unit]]\nid = \"x\"\npro";
        let h = suggestions(text, text.chars().count(), "pro", false, &v);
        assert_eq!(h.first().map(|h| (h.0.as_str(), h.1)), Some(("produce", Kind::Field)));
        let text = "unitRule a\n    appliesTo hou";
        let h = suggestions(text, text.chars().count(), "hou", true, &v);
        assert!(h.first().is_some_and(|h| h.1 == Kind::Unit && h.0.starts_with("house")), "{h:?}");
        let text = "unitRule a\n    ma";
        let h = suggestions(text, text.chars().count(), "ma", true, &v);
        assert!(h.iter().any(|h| h.0 == "map" && h.1 == Kind::Keyword), "{h:?}");
    }

    #[test]
    fn lines_and_words() {
        assert_eq!(char_of_line("a\nbb\nccc", 3), 5);
        assert_eq!(word_before("x = AirQuality.pm", 17), (4, "AirQuality.pm".to_string()));
        let src = Sources { id: "t".into(), files: vec![("rules/t.toml".into(), UNIT.into())], warnings: vec![] };
        assert_eq!(find_definition(&src, "shop"), Some((0, 10)));
    }
}
