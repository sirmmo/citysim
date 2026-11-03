//! The GlassBox resource network drawn on the map: one arc per link
//! (sender to receiver), coloured by resource, as thick as its flow.

use std::collections::BTreeSet;

use macroquad::color::hsl_to_rgb;
use macroquad::prelude::*;
use opencity_core::glassbox::Link;
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

/// One link ready to draw.
#[derive(Clone, Debug)]
pub struct Arc {
    pub link: Link,
    pub amount: f32,
    pub a: Vec3,
    pub b: Vec3,
    /// Sideways shift at mid-arc, so links of different resources between
    /// the same two buildings do not overlap.
    pub side: Vec3,
    pub height: f32,
    pub colour: Color,
    pub width: f32,
}

impl Arc {
    pub fn point(&self, t: f32) -> Vec3 {
        let s = (t * std::f32::consts::PI).sin();
        self.a.lerp(self.b, t) + self.side * s + Vec3::Y * self.height * 4.0 * t * (1.0 - t)
    }

    pub fn tiles(&self) -> u32 {
        self.link.from.0.abs_diff(self.link.to.0) + self.link.from.1.abs_diff(self.link.to.1)
    }
}

/// Centre of the building whose origin is `o`, a little above the ground.
pub fn unit_centre(city: &City, o: (u32, u32)) -> Vec3 {
    let (sw, sl) = city.layer.structure_at(o.0, o.1).map_or((1, 1), |s| city.footprints().size(s.graphic));
    let (x, z) = (o.0 as f32 + sw as f32 / 2.0, o.1 as f32 + sl as f32 / 2.0);
    vec3(x, height_at(&city.terrain, x, z) + 0.7, z)
}

/// The arcs to draw. `focus` is the inspected tile (any tile of a building).
pub fn arcs(city: &City, view: &FlowView, focus: Option<(u32, u32)>) -> Vec<Arc> {
    let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) else { return Vec::new() };
    let focus = focus.and_then(|(w, l)| city.layer.origin_of(w, l));
    let mut biggest = vec![0f32; rules.resources.len()];
    for (k, v) in &gb.flows {
        biggest[k.resource] = biggest[k.resource].max(*v);
    }
    let shown: Vec<usize> = (0..rules.resources.len()).filter(|&r| view.shows(rules, r)).collect();
    let mut out: Vec<Arc> = gb
        .flows
        .iter()
        .filter(|(k, v)| **v >= view.min_amount && shown.contains(&k.resource))
        .filter(|(k, _)| !view.focus_only || focus.is_none_or(|f| k.from == f || k.to == f))
        .map(|(k, v)| {
            let (a, b) = (unit_centre(city, k.from), unit_centre(city, k.to));
            let d = b - a;
            let flat = vec3(d.z, 0.0, -d.x).normalize_or_zero();
            let slot = shown.iter().position(|&r| r == k.resource).unwrap_or(0) as f32 - (shown.len() as f32 - 1.0) / 2.0;
            let len = d.length();
            Arc {
                link: *k,
                amount: *v,
                a,
                b,
                side: flat * slot * 0.12,
                height: (0.4 + len * 0.12).min(5.0),
                colour: resource_colour(rules, k.resource),
                width: 0.025 + 0.11 * (v / biggest[k.resource].max(1e-6)).sqrt(),
            }
        })
        .collect();
    // Thin links first, so busy ones are drawn on top.
    out.sort_by(|x, y| x.width.total_cmp(&y.width).then(x.link.cmp(&y.link)));
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
