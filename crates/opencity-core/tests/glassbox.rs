//! The shipped GlassBox rules produce a working economy.

use std::sync::Arc;

use opencity_core::{rules::Ruleset, City, Footprints, StructureCode, Terrain, Tool};

fn rules() -> Arc<Ruleset> {
    let path = format!("{}/../../assets/rules/glassbox.toml", env!("CARGO_MANIFEST_DIR"));
    let r: Ruleset = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    r.validate().unwrap();
    Arc::new(r)
}

fn units(c: &City, code: StructureCode) -> usize {
    c.layer.structures().filter(|(_, s)| s.code == code && s.unit.is_some()).count()
}

#[test]
fn shipped_rules_are_valid() {
    let r = rules();
    assert!(r.units.len() >= 30);
    assert_eq!(r.zones.len(), 3);
    assert!(r.zones.iter().all(|z| z.densities.len() == 3));
}

#[test]
fn a_town_develops_from_resource_flows() {
    let mut c = City::new(Terrain::flat(48, 48), None, Footprints::builtin(), 7);
    c.use_rules(rules());
    c.apply_tool(Tool::CoalPlant, (2, 2), (2, 2)).unwrap();
    // Roads every 4 rows, zones between them.
    for l in [8, 12, 16, 20, 24] {
        c.apply_tool(Tool::Road, (2, l), (44, l)).unwrap();
    }
    c.apply_tool(Tool::Road, (7, 2), (7, 8)).unwrap(); // plant to the grid
    c.apply_tool(Tool::ZoneResidential, (2, 9), (44, 11)).unwrap();
    c.apply_tool(Tool::ZoneIndustrial, (2, 13), (44, 15)).unwrap();
    c.zone_density = 1;
    c.apply_tool(Tool::ZoneResidential, (2, 17), (20, 19)).unwrap();
    c.zone_density = 0;
    c.apply_tool(Tool::ZoneCommercial, (21, 17), (44, 19)).unwrap();
    c.apply_tool(Tool::FireDept, (10, 21), (10, 21)).unwrap();
    let fund0 = c.fund;
    for _ in 0..600 {
        c.simulate_round();
    }
    let gb = c.glassbox.as_ref().unwrap();
    let r = gb.rules().unwrap().clone();
    let res = |id: &str| r.resource(id).unwrap();
    println!("demand {:?}", gb.demand);
    println!("produced {:?}\ndelivered {:?}", gb.balances.produced, gb.balances.delivered);
    let (houses, shops, factories) = (units(&c, StructureCode::Residential), units(&c, StructureCode::Commercial), units(&c, StructureCode::Industrial));
    println!("houses {houses} shops {shops} factories {factories} population {} fund {} -> {}", c.population, fund0, c.fund);
    assert!(houses > 20, "houses {houses}");
    assert!(factories > 5, "factories {factories}");
    assert!(shops > 3, "shops {shops}");
    assert!(gb.balances.delivered[res("power")] > 0.0);
    assert!(gb.balances.delivered[res("workers")] > 0.0);
    assert!(gb.balances.delivered[res("goods")] > 0.0);
    assert!(c.population > 100);
    // Medium density plots grow medium-density buildings.
    assert!(c.layer.structures().any(|(_, s)| s.code == StructureCode::Residential && s.density == 1 && s.unit.is_some()));
    // Pollution near industry, fire coverage around the station.
    let w = c.width();
    assert!(gb.layer_value(r.layer("pollution").unwrap(), 20, 14, w) > 0.5);
    assert!(gb.layer_value(r.layer("fire_safety").unwrap(), 10, 21, w) > 1.0);
    // A save keeps the engine state; rules are re-attached on load.
    let mut buf = Vec::new();
    opencity_core::save::save(&c, &mut buf).unwrap();
    let mut d = opencity_core::save::load(buf.as_slice(), Footprints::builtin()).unwrap();
    assert!(d.glassbox.is_some() && d.rules().is_none());
    d.use_rules(rules());
    d.simulate_round();
    assert!(units(&d, StructureCode::Residential) > 0);
}

#[test]
fn without_power_nothing_thrives() {
    let mut c = City::new(Terrain::flat(30, 30), None, Footprints::builtin(), 3);
    c.use_rules(rules());
    c.apply_tool(Tool::Road, (2, 10), (28, 10)).unwrap();
    c.apply_tool(Tool::ZoneResidential, (2, 11), (28, 12)).unwrap();
    for _ in 0..300 {
        c.simulate_round();
    }
    let gb = c.glassbox.as_ref().unwrap();
    assert!(gb.units.values().all(|u| u.satisfaction < 0.5));
}
