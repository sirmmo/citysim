//! Ready-made layouts, used by the CLI, the game's `--demo` flag and tests.

use crate::{City, Tool};

/// Lays out a small powered town around the map centre: a road grid with
/// residential, commercial and industrial blocks, a coal plant and a power
/// line. Placements that do not fit the terrain are skipped.
pub fn demo_town(city: &mut City) {
    let (cw, cl) = (city.width() / 2, city.length() / 2);
    let w0 = cw.saturating_sub(14);
    let l0 = cl.saturating_sub(10);
    let w1 = (w0 + 28).min(city.width() - 1);
    let l1 = (l0 + 20).min(city.length() - 1);
    // Level the site at its median height (at least dry land), like a player would.
    let site = (w0, l0.saturating_sub(5), w1, l1);
    let mut heights: Vec<i8> = (site.1..=site.3).flat_map(|l| (site.0..=site.2).map(move |w| (w, l))).map(|(w, l)| city.terrain.corner(w, l)).collect();
    heights.sort_unstable();
    let level = heights[heights.len() / 2].max(1);
    city.terrain.level_area(site.0, site.1, site.2, site.3, level);
    let mut try_tool = |tool: Tool, a: (u32, u32), b: (u32, u32)| {
        let _ = city.apply_tool(tool, a, b);
    };
    // Clear the site (roads cannot be laid over trees).
    try_tool(Tool::Destroy, (w0, l0.saturating_sub(5)), (w1, l1));
    // Road grid: blocks of 4x3 tiles.
    for l in (l0..=l1).step_by(4) {
        try_tool(Tool::Road, (w0, l), (w1, l));
    }
    for w in (w0..=w1).step_by(5) {
        try_tool(Tool::Road, (w, l0), (w, l1));
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
        try_tool(Tool::PowerLine, (w1, l0 - 4), (w1, l0 - 1));
    }
    try_tool(Tool::Park, (w0 + 1, l1.saturating_sub(1)), (w0 + 3, l1.saturating_sub(1)));
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
    }
}
