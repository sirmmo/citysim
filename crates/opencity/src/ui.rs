//! Toolbar, status bar, menu and query panel, drawn with the legacy GUI images.

use macroquad::prelude::*;
use opencity_core::{City, Status, TileInfo, Tool};

use crate::render::Renderer;

pub const BUTTON: f32 = 48.0;
const PAD: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Tool(Option<Tool>),
    Query,
    TogglePause,
    ToggleFast,
    OpenMenu,
    Menu(MenuItem),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    New,
    Load,
    Save,
    Quit,
    Close,
}

struct Button {
    icon: Texture2D,
    hover: Texture2D,
    action: Action,
    label: &'static str,
}

pub struct Ui {
    tools: Vec<Button>,
    menu: Vec<Button>,
    status_bar: Texture2D,
    query_icons: [(Texture2D, Texture2D, u8); 4],
    pub menu_open: bool,
}

const TOOLS: &[(&str, Action, &str)] = &[
    ("query", Action::Query, "Query (/)"),
    ("bulldozer", Action::Tool(Some(Tool::Destroy)), "Bulldozer (X)"),
    ("residential", Action::Tool(Some(Tool::ZoneResidential)), "Residential zone (R)"),
    ("commercial", Action::Tool(Some(Tool::ZoneCommercial)), "Commercial zone (C)"),
    ("industrial", Action::Tool(Some(Tool::ZoneIndustrial)), "Industrial zone (I)"),
    ("road", Action::Tool(Some(Tool::Road)), "Street (P)"),
    ("road", Action::Tool(Some(Tool::Highway)), "Highway"),
    ("road", Action::Tool(Some(Tool::PavedStreet)), "Paved street (P)"),
    ("road", Action::Tool(Some(Tool::DirtRoad)), "Dirt road"),
    ("road", Action::Tool(Some(Tool::Rail)), "Railway"),
    ("power_line", Action::Tool(Some(Tool::PowerLine)), "Power line (L)"),
    ("power_plant_coal", Action::Tool(Some(Tool::CoalPlant)), "Coal power plant"),
    ("power_plant_nuclear", Action::Tool(Some(Tool::NuclearPlant)), "Nuclear power plant"),
    ("park_city", Action::Tool(Some(Tool::Park)), "Park"),
    ("tree", Action::Tool(Some(Tool::Trees)), "Trees (T)"),
    ("fire", Action::Tool(Some(Tool::FireDept)), "Fire department"),
    ("police", Action::Tool(Some(Tool::PoliceDept)), "Police department"),
    ("hospital", Action::Tool(Some(Tool::Hospital)), "Hospital"),
    ("education", Action::Tool(Some(Tool::School)), "School"),
    ("government", Action::Tool(Some(Tool::Station)), "Railway station"),
    ("government", Action::Tool(Some(Tool::Harbour)), "Harbour"),
    ("lower", Action::Tool(Some(Tool::Canal)), "Canal"),
    ("raise", Action::Tool(Some(Tool::RaiseTerrain)), "Raise terrain (U)"),
    ("lower", Action::Tool(Some(Tool::LowerTerrain)), "Lower terrain (J)"),
];

impl Ui {
    pub fn new(r: &mut Renderer) -> Self {
        let mut button = |name: &str, action, label| Button {
            icon: r.ui_texture(&format!("graphism/gui/{name}.png")),
            hover: r.ui_texture(&format!("graphism/gui/{name}_over.png")),
            action,
            label,
        };
        let tools = TOOLS.iter().map(|&(n, a, l)| button(n, a, l)).collect();
        let menu = vec![
            button("main_menu_new", Action::Menu(MenuItem::New), "New city"),
            button("main_menu_load", Action::Menu(MenuItem::Load), "Load"),
            button("main_menu_save", Action::Menu(MenuItem::Save), "Save"),
            button("main_menu_quit", Action::Menu(MenuItem::Quit), "Quit"),
        ];
        let q = |name: &str, r: &mut Renderer| {
            (r.ui_texture(&format!("graphism/gui/query_{name}_on.png")), r.ui_texture(&format!("graphism/gui/query_{name}_off.png")))
        };
        let (ro, rf) = q("residential", r);
        let (co, cf) = q("commercial", r);
        let (io, if_) = q("industrial", r);
        let (eo, ef) = q("electricity", r);
        Self {
            tools,
            menu,
            status_bar: r.ui_texture("graphism/gui/main_status_bar.png"),
            query_icons: [(ro, rf, Status::RESIDENTIAL), (co, cf, Status::COMMERCIAL), (io, if_, Status::INDUSTRIAL), (eo, ef, Status::POWER)],
            menu_open: false,
        }
    }

    fn toolbar_rect(i: usize) -> Rect {
        let cols = 2;
        Rect::new(PAD + (i % cols) as f32 * (BUTTON + PAD), 80.0 + (i / cols) as f32 * (BUTTON + PAD), BUTTON, BUTTON)
    }

    /// Whether the mouse is over a UI element (so the map should ignore it).
    pub fn has_mouse(&self) -> bool {
        let (x, y) = mouse_position();
        let last = Self::toolbar_rect(self.tools.len() - 1);
        let sw = screen_width();
        let status = x > (sw - 512.0) / 2.0 && x < (sw + 512.0) / 2.0 + 130.0 && y < 80.0;
        self.menu_open || (x < last.x + BUTTON + PAD * 2.0 && y < last.y + BUTTON + PAD) || status
    }

    pub fn draw(&self, city: &City, active: Option<Action>, message: Option<&str>) -> Option<Action> {
        let mouse: Vec2 = mouse_position().into();
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        let mut action = None;
        let mut tooltip = None;

        // Status bar (top centre).
        let sw = screen_width();
        let bar = Rect::new((sw - 512.0) / 2.0, 4.0, 512.0, 64.0);
        draw_texture_ex(&self.status_bar, bar.x, bar.y, WHITE, DrawTextureParams { dest_size: Some(vec2(bar.w, bar.h)), ..Default::default() });
        let v = city.values;
        let text = format!("$ {}   pop {}   {}", city.fund, city.population, city.date);
        draw_text(&text, bar.x + 20.0, bar.y + 26.0, 24.0, if city.fund < 0 { RED } else { WHITE });
        let total = (v.residential + v.commercial + v.industrial).max(1) as f32;
        let bars = [
            ("R", v.residential as f32 / total, GREEN),
            ("C", v.commercial as f32 / total, SKYBLUE),
            ("I", v.industrial as f32 / total, GOLD),
            ("E", if v.power_capacity > 0 { v.power.max(0) as f32 / v.power_capacity as f32 } else { 0.0 }, YELLOW),
        ];
        for (i, (label, frac, color)) in bars.iter().enumerate() {
            let x = bar.x + 20.0 + i as f32 * 120.0;
            draw_text(label, x, bar.y + 52.0, 20.0, WHITE);
            draw_rectangle(x + 16.0, bar.y + 40.0, 90.0, 12.0, Color::new(0.0, 0.0, 0.0, 0.5));
            draw_rectangle(x + 16.0, bar.y + 40.0, 90.0 * frac.clamp(0.0, 1.0), 12.0, *color);
        }
        // Speed buttons.
        let speed_label = match city.speed {
            opencity_core::Speed::Paused => "|| paused",
            opencity_core::Speed::Normal => "> normal",
            opencity_core::Speed::Fast => ">> fast",
        };
        let speed = Rect::new(bar.x + bar.w + 8.0, bar.y + 8.0, 110.0, 30.0);
        draw_rectangle(speed.x, speed.y, speed.w, speed.h, Color::new(0.0, 0.0, 0.0, 0.6));
        draw_text(speed_label, speed.x + 8.0, speed.y + 21.0, 22.0, WHITE);
        if speed.contains(mouse) {
            tooltip = Some("Space: pause, F: fast");
            if clicked {
                action = Some(Action::TogglePause);
            }
        }
        // Menu button (needed on touch screens without an Esc key).
        let menu = Rect::new(speed.x, speed.y + speed.h + 6.0, speed.w, speed.h);
        draw_rectangle(menu.x, menu.y, menu.w, menu.h, Color::new(0.0, 0.0, 0.0, 0.6));
        draw_text("= menu", menu.x + 8.0, menu.y + 21.0, 22.0, WHITE);
        if menu.contains(mouse) {
            tooltip = Some("Esc: menu");
            if clicked && !self.menu_open {
                action = Some(Action::OpenMenu);
            }
        }

        // Toolbar (left).
        for (i, b) in self.tools.iter().enumerate() {
            let r = Self::toolbar_rect(i);
            let hover = r.contains(mouse);
            let selected = active == Some(b.action);
            if selected {
                draw_rectangle(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0, Color::new(1.0, 1.0, 1.0, 0.35));
            }
            let tex = if hover || selected { &b.hover } else { &b.icon };
            draw_texture_ex(tex, r.x, r.y, WHITE, DrawTextureParams { dest_size: Some(vec2(r.w, r.h)), ..Default::default() });
            if hover {
                tooltip = Some(b.label);
                if clicked {
                    action = Some(b.action);
                }
            }
        }

        if self.menu_open {
            draw_rectangle(0.0, 0.0, sw, screen_height(), Color::new(0.0, 0.0, 0.1, 0.6));
            let size = 128.0;
            let total = self.menu.len() as f32 * (size + 20.0);
            for (i, b) in self.menu.iter().enumerate() {
                let r = Rect::new((sw - total) / 2.0 + i as f32 * (size + 20.0), screen_height() / 2.0 - size / 2.0, size, size);
                let hover = r.contains(mouse);
                draw_texture_ex(
                    if hover { &b.hover } else { &b.icon },
                    r.x,
                    r.y,
                    WHITE,
                    DrawTextureParams { dest_size: Some(vec2(size, size)), ..Default::default() },
                );
                if hover {
                    tooltip = Some(b.label);
                    if clicked {
                        action = Some(b.action);
                    }
                }
            }
            if clicked && action.is_none() {
                action = Some(Action::Menu(MenuItem::Close));
            }
        }

        if let Some(t) = tooltip {
            draw_text_box(t, mouse + vec2(16.0, 24.0));
        }
        if let Some(m) = message {
            let dims = measure_text(m, None, 24, 1.0);
            draw_text_box(m, vec2((sw - dims.width) / 2.0, screen_height() - 40.0));
        }
        action
    }

    /// Query panel, legacy style: one on/off icon per service.
    pub fn draw_query(&self, at: Vec2, info: Option<(String, TileInfo)>) {
        let (w, h) = (4.0 * 52.0 + 12.0, 110.0);
        draw_rectangle(at.x, at.y, w, h, Color::new(0.0, 0.0, 0.0, 0.7));
        match info {
            None => {
                draw_text("empty land", at.x + 10.0, at.y + 30.0, 22.0, WHITE);
            }
            Some((name, i)) => {
                draw_text(&format!("{name}  (level {}, traffic {})", i.level, i.traffic), at.x + 8.0, at.y + 22.0, 18.0, WHITE);
                for (k, (on, off, bit)) in self.query_icons.iter().enumerate() {
                    let tex = if i.status.has(*bit) { on } else { off };
                    draw_texture_ex(
                        tex,
                        at.x + 6.0 + k as f32 * 52.0,
                        at.y + 34.0,
                        WHITE,
                        DrawTextureParams { dest_size: Some(vec2(48.0, 48.0)), ..Default::default() },
                    );
                }
            }
        }
    }
}

fn draw_text_box(text: &str, at: Vec2) {
    let d = measure_text(text, None, 22, 1.0);
    draw_rectangle(at.x - 6.0, at.y - d.offset_y - 6.0, d.width + 12.0, d.height + 12.0, Color::new(0.0, 0.0, 0.0, 0.75));
    draw_text(text, at.x, at.y, 22.0, WHITE);
}
