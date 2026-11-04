//! The shipped GlassBox rules produce a working economy.

use std::sync::Arc;

use opencity_core::{rules::Ruleset, City, Footprints, RoadKind, StructureCode, Terrain, Tool};

fn rules() -> Arc<Ruleset> {
    rules_named("glassbox")
}

fn rules_named(id: &str) -> Arc<Ruleset> {
    let dir = format!("{}/../../assets/rules", env!("CARGO_MANIFEST_DIR"));
    let mut r: Ruleset = toml::from_str(&std::fs::read_to_string(format!("{dir}/{id}.toml")).unwrap()).unwrap();
    for script in r.scripts.clone() {
        opencity_core::gbx::compile(&mut r, &script, &std::fs::read_to_string(format!("{dir}/{script}")).unwrap()).unwrap();
    }
    if let Some(tree) = r.tech_tree.clone() {
        let text = std::fs::read_to_string(format!("{dir}/../techs/{tree}.toml")).unwrap();
        r.add_tech_tree(toml::from_str(&text).unwrap());
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
        c.research_all();
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
    c.research_all();
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

/// A walled town of the past: paved and dirt streets, a granary, wells, carts.
#[test]
fn a_pre_industrial_town_lives_on_food_and_carts() {
    let r = rules_named("pre-industrial");
    assert!(!r.allows("power_line") && !r.allows("road") && r.allows("dirt_road"));
    assert!(r.fits_year(1200) && !r.fits_year(1900));
    assert!(!rules().fits_year(1200));
    let mut c = City::new(Terrain::flat(48, 48), None, Footprints::builtin(), 11);
    c.use_rules(r.clone());
    c.research_all();
    c.apply_tool(Tool::CoalPlant, (2, 2), (2, 2)).unwrap(); // the granary
    for l in [8, 16, 24] {
        c.apply_tool(Tool::PavedStreet, (2, l), (44, l)).unwrap();
    }
    for l in [12, 20] {
        c.apply_tool(Tool::DirtRoad, (2, l), (44, l)).unwrap();
    }
    c.apply_tool(Tool::DirtRoad, (7, 2), (7, 8)).unwrap();
    for w in [2, 23, 44] {
        c.apply_tool(Tool::PavedStreet, (w, 8), (w, 24)).unwrap();
    }
    c.apply_tool(Tool::ZoneResidential, (2, 9), (44, 11)).unwrap();
    c.apply_tool(Tool::ZoneIndustrial, (2, 13), (30, 15)).unwrap();
    c.zone_density = 1;
    c.apply_tool(Tool::ZoneIndustrial, (31, 13), (44, 15)).unwrap();
    c.zone_density = 0;
    c.apply_tool(Tool::ZoneCommercial, (21, 17), (44, 19)).unwrap();
    c.zone_density = 1;
    c.apply_tool(Tool::ZoneResidential, (2, 17), (20, 19)).unwrap();
    for w in [6, 16, 26, 36] {
        c.apply_tool(Tool::Park, (w, 21), (w, 21)).unwrap(); // wells
    }
    c.apply_tool(Tool::FireDept, (10, 22), (10, 22)).unwrap();
    assert_eq!(c.layer.structure_at(4, 12).unwrap().road, RoadKind::DirtRoad);
    for _ in 0..800 {
        c.simulate_round();
    }
    let gb = c.glassbox.as_ref().unwrap();
    let res = |id: &str| r.resource(id).unwrap();
    let (houses, shops, crafts) = (units(&c, StructureCode::Residential), units(&c, StructureCode::Commercial), units(&c, StructureCode::Industrial));
    println!("houses {houses} shops {shops} crafts {crafts} population {}", c.population);
    println!("delivered {:?}", gb.balances.delivered);
    assert!(houses > 20, "houses {houses}");
    assert!(crafts > 5, "crafts {crafts}");
    assert!(shops > 3, "shops {shops}");
    for id in ["food", "workers", "cart", "cart.handcart", "cart.horse", "fodder"] {
        assert!(gb.balances.delivered[res(id)] > 0.0, "{id} delivered");
    }
    // Water from the wells lets houses grow past the first level.
    let watered = c.layer.structures().filter(|(_, s)| s.unit.is_some_and(|u| r.units[u as usize].requires.contains_key("water"))).count();
    assert!(watered > 0, "no house needing water was built");
}

/// Plots along a highway alone do not develop: buildings cannot open onto it.
#[test]
fn highways_give_no_access() {
    let mut c = City::new(Terrain::flat(40, 30), None, Footprints::builtin(), 5);
    c.use_rules(rules());
    c.research_all();
    c.apply_tool(Tool::NuclearPlant, (2, 2), (2, 2)).unwrap();
    c.apply_tool(Tool::Highway, (2, 14), (38, 14)).unwrap();
    c.apply_tool(Tool::ZoneResidential, (2, 15), (38, 16)).unwrap();
    for _ in 0..200 {
        c.simulate_round();
    }
    assert_eq!(units(&c, StructureCode::Residential), 0);
    // A street next to them opens the plots.
    c.apply_tool(Tool::Road, (2, 17), (38, 17)).unwrap();
    for _ in 0..200 {
        c.simulate_round();
    }
    assert!(units(&c, StructureCode::Residential) > 0);
}

#[test]
fn rules_choose_the_vehicles_of_each_age() {
    use opencity_core::traffic::VehicleKind;
    let old = rules_named("pre-industrial");
    let kinds = |r: &Ruleset, id: &str| -> Vec<VehicleKind> { r.vehicles_for(r.resource(id).unwrap()).iter().map(|v| v.kind()).collect() };
    // No cars before 1800: people walk, carts carry the loads.
    for id in ["workers", "shoppers", "food", "goods", "cart", "cart.horse", "fodder"] {
        let k = kinds(&old, id);
        assert!(!k.is_empty() && k.iter().all(|k| k.graphic().is_none()), "{id}: {k:?}");
    }
    assert_eq!(kinds(&old, "workers"), [VehicleKind::Walker]);
    assert_eq!(kinds(&old, "cart.handcart"), [VehicleKind::Handcart]);
    assert!(old.vehicles_for(old.resource("goods").unwrap()).iter().all(|v| !v.ways().allows(RoadKind::Highway)));
    let new = rules_named("glassbox");
    assert!(kinds(&new, "goods").contains(&VehicleKind::Pickup));
    assert!(kinds(&new, "car.electric").contains(&VehicleKind::Car));

    // A bad model or way is refused.
    let mut bad = (*old).clone();
    bad.networks[0].vehicles[0].model = "spaceship".into();
    assert!(bad.validate().is_err());
    let mut bad = (*old).clone();
    bad.networks[0].vehicles[0].ways = vec!["canal".into()];
    assert!(bad.validate().is_err());

    // The medieval demo town only ever shows walkers and carts.
    let mut c = City::new(Terrain::flat(60, 60), None, Footprints::builtin(), 5);
    c.use_rules(old);
    opencity_core::scenario::demo_town(&mut c);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..600 {
        c.simulate_round();
        for v in &c.vehicles {
            assert!(v.kind.graphic().is_none() && v.top_speed.is_some_and(|t| t <= 1.0), "{:?}", v.kind);
            seen.insert(v.kind.id());
        }
        c.vehicles.clear();
    }
    assert!(seen.contains("walker") && seen.len() >= 2, "{seen:?}");
}

/// Ideas from cafés and restaurants become research points at schools, and
/// research unlocks tools and buildings.
#[test]
fn ideas_become_research_that_unlocks() {
    let r = rules();
    let mut c = City::new(Terrain::flat(48, 48), None, Footprints::builtin(), 7);
    c.use_rules(r.clone());
    // Locked from the start: the highway tool, nuclear plants, and the top
    // residential levels.
    assert!(matches!(c.apply_tool(Tool::Highway, (2, 30), (20, 30)), Err(opencity_core::city::ToolError::Locked(_))));
    assert_eq!(c.tool_locked_by(Tool::NuclearPlant).as_deref(), Some("Nuclear fission"));
    {
        let gb = c.glassbox.as_mut().unwrap();
        assert!(!gb.unlocked(&r, "tower_2") && gb.unlocked(&r, "tower_1"));
        assert!(!gb.can_research(&r, "fission"), "needs steel-frame towers first");
        assert!(gb.set_research(Some("motorways")));
    }
    c.apply_tool(Tool::CoalPlant, (2, 2), (2, 2)).unwrap();
    c.apply_tool(Tool::School, (10, 2), (10, 2)).unwrap();
    for l in [8, 12, 16, 20] {
        c.apply_tool(Tool::Road, (2, l), (44, l)).unwrap();
    }
    c.apply_tool(Tool::Road, (7, 2), (7, 8)).unwrap();
    c.apply_tool(Tool::Road, (13, 2), (13, 8)).unwrap();
    c.apply_tool(Tool::ZoneResidential, (2, 9), (44, 11)).unwrap();
    c.apply_tool(Tool::ZoneIndustrial, (2, 13), (44, 15)).unwrap();
    c.apply_tool(Tool::ZoneCommercial, (2, 17), (44, 19)).unwrap();
    let mut done_at = None;
    for t in 0..2500 {
        c.simulate_round();
        if done_at.is_none() && c.glassbox.as_ref().unwrap().researched.contains("motorways") {
            done_at = Some(t);
        }
    }
    let gb = c.glassbox.as_ref().unwrap();
    let ideas = r.resource("ideas").unwrap();
    let research = r.resource("research").unwrap();
    println!("ideas made {} research made {} done at {done_at:?}", gb.balances.produced[ideas], gb.balances.produced[research]);
    assert!(gb.balances.produced[ideas] > 0.0 && gb.balances.produced[research] > 0.0);
    assert!(done_at.is_some(), "motorways never researched");
    assert!(gb.research.is_none() && gb.pools[research] > 0.0, "points keep pooling once the project is done");
    c.apply_tool(Tool::Highway, (2, 30), (20, 30)).unwrap();
    // Nothing research has not unlocked grew.
    let tower2 = r.unit("tower_2").unwrap() as u16;
    assert!(c.layer.structures().all(|(_, s)| s.unit != Some(tower2)));
    // Saves keep the research.
    let mut saved = Vec::new();
    opencity_core::save::save(&c, &mut saved).unwrap();
    let back = opencity_core::save::load(saved.as_slice(), Footprints::builtin()).unwrap();
    assert!(back.glassbox.as_ref().unwrap().researched.contains("motorways"));
}

#[test]
fn tech_trees_choose_a_charter_and_pay_bonuses() {
    // The projects come from techs/pre-industrial.toml, outside the rules.
    let r = rules_named("pre-industrial");
    assert_eq!(r.tech_tree.as_deref(), Some("pre-industrial"));
    assert!(r.tech("free_market").is_some() && r.tech("printing").is_some());
    let mut c = City::new(Terrain::flat(32, 32), None, Footprints::builtin(), 3);
    c.use_rules(r.clone());
    let gb = c.glassbox.as_mut().unwrap();
    assert!(gb.can_research(&r, "free_market") && gb.can_research(&r, "guild_charter"));
    assert!(!gb.unlocked(&r, "market_square") && !gb.unlocked(&r, "craft_hall"));
    // Choosing the guild charter rules the free market out for good.
    gb.researched.insert("guild_charter".into());
    assert!(gb.unlocked(&r, "craft_hall") && !gb.unlocked(&r, "market_square"));
    assert!(!gb.can_research(&r, "free_market") && gb.ruled_out(&r, "free_market"));
    assert!(!gb.set_research(Some("free_market")));
    assert!((gb.research_bonus(&r)[r.resource("goods").unwrap()] - 0.2).abs() < 1e-6);
    // Researching everything still picks one charter: the first listed.
    let mut d = City::new(Terrain::flat(32, 32), None, Footprints::builtin(), 3);
    d.use_rules(r.clone());
    d.research_all();
    let done = &d.glassbox.as_ref().unwrap().researched;
    assert!(done.contains("free_market") && !done.contains("guild_charter") && done.contains("banking"));
}

#[test]
fn research_bonuses_raise_production() {
    let r = rules();
    let mut c = City::new(Terrain::flat(48, 48), None, Footprints::builtin(), 7);
    c.use_rules(r.clone());
    c.apply_tool(Tool::CoalPlant, (2, 2), (2, 2)).unwrap();
    for l in [8, 12, 16] {
        c.apply_tool(Tool::Road, (2, l), (44, l)).unwrap();
    }
    c.apply_tool(Tool::Road, (7, 2), (7, 8)).unwrap();
    c.apply_tool(Tool::ZoneResidential, (2, 9), (44, 11)).unwrap();
    c.apply_tool(Tool::ZoneIndustrial, (2, 13), (44, 15)).unwrap();
    for _ in 0..300 {
        c.simulate_round();
    }
    let goods = r.resource("goods").unwrap();
    let mut boosted = c.clone();
    boosted.glassbox.as_mut().unwrap().researched.insert("industrial_policy".into());
    c.simulate_round();
    boosted.simulate_round();
    let (a, b) = (c.glassbox.as_ref().unwrap().balances.produced[goods], boosted.glassbox.as_ref().unwrap().balances.produced[goods]);
    assert!(a > 0.0, "no goods made");
    assert!((b / a - 1.2).abs() < 1e-3, "goods {a} -> {b}");
}
