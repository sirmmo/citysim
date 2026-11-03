//! The shipped GlassBox rules produce a working economy.

use std::sync::Arc;

use opencity_core::{rules::Ruleset, City, Footprints, StructureCode, Terrain, Tool};

fn rules() -> Arc<Ruleset> {
    let dir = format!("{}/../../assets/rules", env!("CARGO_MANIFEST_DIR"));
    let mut r: Ruleset = toml::from_str(&std::fs::read_to_string(format!("{dir}/glassbox.toml")).unwrap()).unwrap();
    for script in r.scripts.clone() {
        opencity_core::gbx::compile(&mut r, &script, &std::fs::read_to_string(format!("{dir}/{script}")).unwrap()).unwrap();
    }
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
    // GlassBox script: metrics are logged with timestamps, traffic dirties the air.
    assert!(gb.metrics.get("Households").copied().unwrap_or(0.0) > 1.0, "{:?}", gb.metrics);
    assert!(gb.metric_history.len() > 100 && gb.metric_history.back().unwrap().0 == gb.tick);
    let air = r.layer("AirQuality.pm10").unwrap();
    assert!((0..48).flat_map(|l| (0..48).map(move |x| (x, l))).any(|(x, l)| gb.layer_value(air, x, l, w) > 0.5));
    assert!(!r.agent_rules.is_empty() && !r.map_rules.is_empty() && !r.zone_rules.is_empty());
    // Resource kinds: both kinds of car fill parking slots; their trips ask
    // other networks of the homes (fuel by road, power from the grid).
    for id in ["car.gasoline", "car.electric", "fuel"] {
        assert!(gb.balances.delivered[res(id)] > 0.0, "no {id} delivered");
    }
    assert!((gb.balances.delivered[res("car")] - gb.balances.delivered[res("car.gasoline")] - gb.balances.delivered[res("car.electric")]).abs() < 0.01);
    let homes: Vec<_> = c
        .layer
        .structures()
        .filter(|(_, s)| s.code == StructureCode::Residential)
        .filter_map(|(p, s)| Some((r.units[s.unit? as usize].needs.get("power").copied().unwrap_or(0.0), gb.unit_state(p.0, p.1, w)?)))
        .collect();
    assert!(homes.iter().any(|(own, st)| st.needs[res("power")] > own + 0.01), "electric cars should charge at home");
    assert!(homes.iter().any(|(_, st)| st.needs[res("fuel")] > 0.0), "petrol cars should need fuel");
    // The resource network: road links for agents, grid links from the plant.
    for id in ["workers", "goods", "power"] {
        assert!(gb.flows.keys().any(|k| k.resource == res(id)), "no {id} links");
    }
    assert!(gb.flows.iter().all(|(k, v)| *v > 0.0 && k.from != k.to));
    assert!(gb.flows.keys().filter(|k| k.resource == res("power")).all(|k| k.from == (2, 2)));
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

/// High-density zones grow multi-tile buildings that swallow their
/// neighbours during a growth pass; this used to panic in `set_unit`.
#[test]
fn dense_growth_over_neighbours_does_not_panic() {
    for seed in 0..6 {
        let mut c = City::new(Terrain::flat(40, 40), None, Footprints::builtin(), seed);
        c.use_rules(rules());
        c.fund = 1_000_000;
        c.apply_tool(Tool::CoalPlant, (2, 2), (2, 2)).unwrap();
        c.apply_tool(Tool::NuclearPlant, (30, 0), (30, 0)).unwrap();
        for l in [9, 15, 21, 27] {
            c.apply_tool(Tool::Road, (2, l), (38, l)).unwrap();
        }
        c.apply_tool(Tool::Road, (2, 7), (38, 7)).unwrap();
        c.apply_tool(Tool::Road, (20, 7), (20, 27)).unwrap();
        c.apply_tool(Tool::FireDept, (10, 28), (10, 28)).unwrap();
        c.apply_tool(Tool::PoliceDept, (14, 28), (14, 28)).unwrap();
        c.apply_tool(Tool::Hospital, (24, 28), (24, 28)).unwrap();
        c.zone_density = 2;
        c.apply_tool(Tool::ZoneResidential, (2, 10), (19, 14)).unwrap();
        c.apply_tool(Tool::ZoneCommercial, (21, 10), (38, 14)).unwrap();
        c.zone_density = 1;
        c.apply_tool(Tool::ZoneIndustrial, (2, 22), (38, 26)).unwrap();
        c.apply_tool(Tool::ZoneResidential, (2, 16), (38, 20)).unwrap();
        c.fund = 1_000_000;
        for _ in 0..800 {
            c.simulate_round();
        }
        // Every unit is on an origin tile.
        for ((w, l), s) in c.layer.structures() {
            if s.unit.is_some() {
                assert_eq!(c.layer.origin_of(w, l), Some((w, l)));
            }
        }
    }
}

/// Buildings must not flicker: once the town has grown, plots stop being
/// built and abandoned over and over.
#[test]
fn growth_settles_without_flicker() {
    let mut c = City::new(Terrain::flat(48, 48), None, Footprints::builtin(), 7);
    c.use_rules(rules());
    c.apply_tool(Tool::CoalPlant, (2, 2), (2, 2)).unwrap();
    // Enough power that the town is limited by its plots, not its grid.
    c.apply_tool(Tool::NuclearPlant, (30, 0), (30, 0)).unwrap();
    c.apply_tool(Tool::Road, (32, 6), (32, 8)).unwrap();
    for l in [8, 12, 16, 20, 24] {
        c.apply_tool(Tool::Road, (2, l), (44, l)).unwrap();
    }
    c.apply_tool(Tool::Road, (7, 2), (7, 8)).unwrap();
    c.apply_tool(Tool::ZoneResidential, (2, 9), (44, 11)).unwrap();
    c.apply_tool(Tool::ZoneIndustrial, (2, 13), (44, 15)).unwrap();
    c.zone_density = 1;
    c.apply_tool(Tool::ZoneResidential, (2, 17), (20, 19)).unwrap();
    c.zone_density = 0;
    c.apply_tool(Tool::ZoneCommercial, (21, 17), (44, 19)).unwrap();
    let snapshot = |c: &City| c.layer.structures().filter_map(|(p, s)| Some((p, s.unit?))).collect::<std::collections::BTreeMap<_, _>>();
    for _ in 0..1200 {
        c.simulate_round();
    }
    let mut before = snapshot(&c);
    let mut changes = 0;
    for _ in 0..300 {
        c.simulate_round();
        let now = snapshot(&c);
        changes += now.iter().filter(|(p, u)| before.get(p) != Some(u)).count() + before.keys().filter(|p| !now.contains_key(p)).count();
        before = now;
    }
    assert!(before.len() > 300, "the town shrank to {}", before.len());
    assert!(changes < 15, "{changes} building changes in 300 settled ticks");
}
