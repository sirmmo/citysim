//! The GlassBox resource network drawn on the map as an overlay of its own
//! networks: what moves is followed along the streets, track, water and
//! grid that carry it (see [`opencity_core::netmap`]) and drawn per
//! stretch, coloured by resource, as thick as its flow, keeping to the
//! right of the way it travels.

use std::collections::BTreeSet;

use macroquad::color::hsl_to_rgb;
use macroquad::prelude::*;
use opencity_core::glassbox::Link;
use opencity_core::netmap::{Stretch, Way};
use opencity_core::rules::{Ruleset, Transport};
use opencity_core::City;

use crate::render::height_at;

/// What the network view shows.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowView {
    pub on: bool,
    /// Resources whose default visibility is flipped (by id). Agent
    /// resources show by default; grid resources (power) do not.
    pub toggled: BTreeSet<String>,
    /// Links carrying less than this per tick are hidden.
    pub min_amount: f32,
    /// Tint buildings that lack something they need.
    pub shortages: bool,
    /// Only the links of the inspected building.
    pub focus_only: bool,
}

impl Default for FlowView {
    fn default() -> Self {
        Self { on: false, toggled: BTreeSet::new(), min_amount: 0.05, shortages: true, focus_only: false }
    }
}

impl FlowView {
    pub fn shows(&self, rules: &Ruleset, resource: usize) -> bool {
        let r = &rules.resources[resource];
        let default = r.transport == Transport::Agents;
        default != self.toggled.contains(&r.id)
    }

    pub fn toggle(&mut self, id: &str) {
        if !self.toggled.remove(id) {
            self.toggled.insert(id.to_string());
        }
    }
}

/// Colour of a resource's links: the rules' colour, else a stable hue.
pub fn resource_colour(rules: &Ruleset, resource: usize) -> Color {
    match rules.resources.get(resource).and_then(|r| r.colour) {
        Some([r, g, b]) => Color::new(r, g, b, 1.0),
        None => {
            let h = rules.resources[resource].id.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
            hsl_to_rgb((h % 360) as f32 / 360.0, 0.75, 0.6)
        }
    }
}

/// One stretch ready to draw.
#[derive(Clone, Debug)]
pub struct Arc {
    pub stretch: Stretch,
    pub a: Vec3,
    pub b: Vec3,
    /// Bulge upwards at mid-way (only for links drawn straight across).
    pub height: f32,
    pub colour: Color,
    pub width: f32,
}

impl Arc {
    pub fn point(&self, t: f32) -> Vec3 {
        self.a.lerp(self.b, t) + Vec3::Y * self.height * 4.0 * t * (1.0 - t)
    }

    pub fn tiles(&self) -> u32 {
        self.stretch.tiles()
    }
}

/// Centre of the building whose origin is `o`, a little above the ground.
pub fn unit_centre(city: &City, o: (u32, u32)) -> Vec3 {
    let (sw, sl) = city.layer.structure_at(o.0, o.1).map_or((1, 1), |s| s.size(city.footprints()));
    let (x, z) = (o.0 as f32 + sw as f32 / 2.0, o.1 as f32 + sl as f32 / 2.0);
    vec3(x, height_at(&city.terrain, x, z) + 0.7, z)
}

/// Height of each way above the ground: lines hang high.
fn lift(way: Way) -> f32 {
    match way {
        Way::Street | Way::Walk => 0.12,
        Way::Rail => 0.2,
        Way::Water => 0.1,
        Way::Grid => 0.55,
    }
}

/// The stretches to draw. `focus` is the inspected tile (any tile of a building).
pub fn arcs(city: &City, view: &FlowView, focus: Option<(u32, u32)>) -> Vec<Arc> {
    let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) else { return Vec::new() };
    let focus = focus.and_then(|(w, l)| city.layer.origin_of(w, l));
    let shown: Vec<usize> = (0..rules.resources.len()).filter(|&r| view.shows(rules, r)).collect();
    let links: Vec<(Link, f32)> = gb
        .flows
        .iter()
        .filter(|(k, _)| shown.contains(&k.resource))
        .filter(|(k, _)| !view.focus_only || focus.is_none_or(|f| k.from == f || k.to == f))
        .map(|(k, v)| (*k, *v))
        .collect();
    let stretches: Vec<Stretch> = city.network_stretches(&links).into_iter().filter(|s| s.amount >= view.min_amount).collect();
    let mut biggest = vec![0f32; rules.resources.len()];
    for s in &stretches {
        biggest[s.resource] = biggest[s.resource].max(s.amount);
    }
    let point = |p: (u32, u32), way: Way| {
        // A building's own tile: its centre; a network tile: the tile's middle.
        let unit = city.layer.structure_at(p.0, p.1).is_some_and(|s| s.unit.is_some()) && city.layer.origin_of(p.0, p.1) == Some(p);
        if unit && way != Way::Grid {
            let c = unit_centre(city, p);
            return vec3(c.x, c.y - 0.7 + lift(way), c.z);
        }
        let (x, z) = (p.0 as f32 + 0.5, p.1 as f32 + 0.5);
        vec3(x, height_at(&city.terrain, x, z) + lift(way), z)
    };
    let mut out: Vec<Arc> = stretches
        .iter()
        .map(|s| {
            let (a, b) = (point(s.from, s.way), point(s.to, s.way));
            let d = b - a;
            let flat = vec3(-d.z, 0.0, d.x).normalize_or_zero();
            let slot = shown.iter().position(|&r| r == s.resource).unwrap_or(0) as f32;
            // Each resource its own lane; what travels keeps to the right
            // (power has no direction of travel to keep to).
            let lane = if s.way == Way::Grid { (slot - (shown.len() as f32 - 1.0) / 2.0) * 0.06 } else { 0.1 + slot * 0.06 };
            let straight = s.way == Way::Walk && s.tiles() > 3;
            Arc {
                stretch: *s,
                a: a + flat * lane,
                b: b + flat * lane,
                height: if straight { (0.4 + d.length() * 0.12).min(5.0) } else { 0.0 },
                colour: resource_colour(rules, s.resource),
                width: 0.02 + 0.09 * (s.amount / biggest[s.resource].max(1e-6)).sqrt(),
            }
        })
        .collect();
    // Thin stretches first, so busy ones are drawn on top.
    out.sort_by(|x, y| x.width.total_cmp(&y.width).then(x.stretch.from.cmp(&y.stretch.from)));
    out
}

/// What a set of arcs was built from: tick, view, focus, layer revision.
pub type ArcsKey = (u64, FlowView, Option<(u32, u32)>, u64);

/// Samples per arc, for drawing and picking.
pub const SEGMENTS: usize = 16;

/// The arc under the mouse (screen pixels), if any.
pub fn pick(arcs: &[Arc], cam: &Camera3D, mouse: Vec2) -> Option<usize> {
    let m = cam.matrix();
    let (sw, sh) = (screen_width(), screen_height());
    let project = |p: Vec3| {
        let c = m * p.extend(1.0);
        (c.w > 0.0).then(|| vec2((c.x / c.w + 1.0) * 0.5 * sw, (1.0 - c.y / c.w) * 0.5 * sh))
    };
    let mut best: Option<(usize, f32)> = None;
    for (i, arc) in arcs.iter().enumerate() {
        let pts: Vec<Option<Vec2>> = (0..=SEGMENTS).map(|s| project(arc.point(s as f32 / SEGMENTS as f32))).collect();
        for w in pts.windows(2) {
            let (Some(p), Some(q)) = (w[0], w[1]) else { continue };
            let pq = q - p;
            let t = ((mouse - p).dot(pq) / pq.length_squared().max(1e-6)).clamp(0.0, 1.0);
            let d = (p + pq * t).distance(mouse);
            if d < 7.0 && best.is_none_or(|(_, bd)| d <= bd) {
                best = Some((i, d));
            }
        }
    }
    best.map(|(i, _)| i)
}
