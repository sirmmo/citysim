//! Ready-made layouts, used by the CLI, the game's `--demo` flag and tests.

use crate::{City, Tool};

/// Lays out a small powered town around the map centre: a road grid with
/// residential, commercial and industrial blocks, a coal plant and a power
/// line (or a granary, wells and paved streets under rules of the past).
/// Placements that do not fit the terrain are skipped.
pub fn demo_town(city: &mut City) {
    // A showcase: everything research would unlock is available.
    city.research_all();
    let (cw, cl) = (city.width() / 2, city.length() / 2);
    let width = city.width();
    let w0 = cw.saturating_sub(14);
    let l0 = cl.saturating_sub(10);
    let w1 = (w0 + 28).min(city.width() - 1);
    let l1 = (l0 + 20).min(city.length() - 1);
    // Streets the rules allow: asphalt, or paved main streets and dirt lanes before it.
    let allows = |city: &City, t: Tool| city.rules().is_none_or(|r| r.allows(t.id()));
    let modern = allows(city, Tool::Road);
    let (main, lane) = if modern { (Tool::Road, Tool::Road) } else { (Tool::PavedStreet, Tool::DirtRoad) };
    let highway = modern && allows(city, Tool::Highway) && w1 + 3 < width;
    let power = allows(city, Tool::PowerLine);
    // Level the site (and the highway's strip) at its median height (at
    // least dry land), like a player would.
    let east = if highway { w1 + 3 } else { w1 };
    let site = (w0, l0.saturating_sub(5), east, l1);
    let mut heights: Vec<i8> = (site.1..=site.3).flat_map(|l| (site.0..=site.2).map(move |w| (w, l))).map(|(w, l)| city.terrain.corner(w, l)).collect();
    heights.sort_unstable();
    let level = heights[heights.len() / 2].max(1);
    city.terrain.level_area(site.0, site.1, site.2, site.3, level);
    let mut try_tool = |tool: Tool, a: (u32, u32), b: (u32, u32)| {
        let _ = city.apply_tool(tool, a, b);
    };
    // Clear the site (roads cannot be laid over trees).
    try_tool(Tool::Destroy, (w0, l0.saturating_sub(5)), (east, l1));
    // Road grid: blocks of 4x3 tiles.
    for (i, l) in (l0..=l1).step_by(4).enumerate() {
        try_tool(if i % 2 == 0 { main } else { lane }, (w0, l), (w1, l));
    }
    for (i, w) in (w0..=w1).step_by(5).enumerate() {
        try_tool(if i % 3 == 0 { main } else { lane }, (w, l0), (w, l1));
    }
    // A highway along the east side, joined by the first and last streets.
    if highway {
        try_tool(Tool::Highway, (w1 + 3, l0.saturating_sub(4)), (w1 + 3, l1));
        try_tool(Tool::Road, (w1, l0), (w1 + 2, l0));
        try_tool(Tool::Road, (w1, l1), (w1 + 2, l1));
    }
    // Before piped water, wells in every other block.
    if !modern {
        for (i, w) in (w0..w1).step_by(5).enumerate() {
            for (_, l) in (l0..l1).step_by(4).enumerate().filter(|(j, _)| (i + j) % 2 == 0) {
                try_tool(Tool::Park, (w + 2, l + 2), (w + 2, l + 2));
            }
        }
    }
    // Mix R, C and I blocks so each zone has the others within reach.
    let tools = [Tool::ZoneResidential, Tool::ZoneCommercial, Tool::ZoneResidential, Tool::ZoneIndustrial];
    for (i, w) in (w0..w1).step_by(5).enumerate() {
        for (j, l) in (l0..l1).step_by(4).enumerate() {
            try_tool(tools[(i + 2 * j) % tools.len()], (w + 1, l + 1), ((w + 4).min(w1), (l + 3).min(l1)));
        }
    }
    // Power: a plant just north of the grid, wired to it.
    if l0 >= 5 {
        try_tool(Tool::CoalPlant, (w1.saturating_sub(5), l0 - 4), (0, 0));
        if power {
            try_tool(Tool::PowerLine, (w1, l0 - 4), (w1, l0 - 1));
        }
        // A road too, so workers can reach the plant (needed by GlassBox rules).
        try_tool(main, (w1.saturating_sub(5), l0 - 1), (w1 - 1, l0 - 1));
    }
    // Water and waste (GlassBox rules): a pumping station, a treatment
    // plant, a water tower and a dump along a street north of the grid.
    let gb = city.glassbox.is_some();
    let utilities = [(Tool::PumpingStation, 1, 3), (Tool::Dump, 4, 4), (Tool::WaterTreatment, 8, 4), (Tool::WaterTower, 12, 2)];
    if gb && l0 >= 5 && utilities.iter().any(|u| allows(city, u.0)) {
        try_tool_on(city, main, (w0, l0 - 1), (w0 + 13, l0 - 1));
        for (tool, dw, dl) in utilities {
            if allows(city, tool) {
                try_tool_on(city, tool, (w0 + dw, l0 - dl), (0, 0));
            }
        }
        // A water main out past the tower, across open land.
        if allows(city, Tool::WaterPipe) {
            try_tool_on(city, Tool::WaterPipe, (w0 + 13, l0 - 2), (w0 + 13, l0 - 6));
            try_tool_on(city, Tool::WaterPipe, (w0 + 13, l0 - 6), (w0 + 17, l0 - 6));
        }
    }
    try_tool_on(city, Tool::Park, (w0 + 1, l1.saturating_sub(1)), (w0 + 3, l1.saturating_sub(1)));
}

fn try_tool_on(city: &mut City, tool: Tool, a: (u32, u32), b: (u32, u32)) {
    let _ = city.apply_tool(tool, a, b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Footprints, Terrain};

    #[test]
    fn demo_town_grows_on_flat_land() {
        let mut c = City::new(Terrain::flat(60, 60), None, Footprints::builtin(), 5);
        demo_town(&mut c);
        assert!(c.layer.occupied() > 350, "{}", c.layer.occupied());
        for _ in 0..2000 {
            c.simulate_round();
        }
        let v = c.values;
        assert!(v.residential > 100 && v.commercial > 50 && v.industrial > 50, "{v:?}");
        assert!(c.values.power > 0);
        // Modern towns get a highway; streets are asphalt.
        assert!(c.layer.structures().any(|(_, s)| s.road == crate::RoadKind::Highway));
    }
}
