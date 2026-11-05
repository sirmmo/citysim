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
    if let Some(set) = r.policy_set.clone() {
        let text = std::fs::read_to_string(format!("{dir}/../policies/{set}.toml")).unwrap();
        r.add_policy_set(toml::from_str(&text).unwrap());
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
    // Road vehicles (trains and boats run apart).
    let kinds = |r: &Ruleset, id: &str| -> Vec<VehicleKind> {
        r.vehicles_for(r.resource(id).unwrap()).iter().filter(|v| !v.ways().sails() && !v.ways().allows(RoadKind::Rail)).map(|v| v.kind()).collect()
    };
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

#[test]
fn services_face_the_street_and_turn() {
    use opencity_core::layer::Structure;
    let mut c = City::new(Terrain::flat(24, 24), None, Footprints::builtin(), 1);
    // A street along the west side of where the school goes, one north of it.
    c.apply_tool(Tool::Road, (5, 2), (5, 20)).unwrap();
    c.apply_tool(Tool::School, (6, 8), (6, 8)).unwrap();
    let s = c.layer.structure_at(6, 8).unwrap().clone();
    assert_eq!(Structure::front(s.facing), (-1, 0), "the school looks onto the street to its west");
    // Turning swaps a long footprint and keeps the origin.
    let (w0, l0) = s.size(c.footprints());
    let f = c.turn_building(6, 8).unwrap();
    assert_eq!(f, (s.facing + 1) % 4);
    let t = c.layer.structure_at(6, 8).unwrap().clone();
    assert_eq!(t.size(c.footprints()), (l0, w0));
    assert!(c.layer.structure_at(6 + t.size(c.footprints()).0 - 1, 8 + t.size(c.footprints()).1 - 1).is_some());
    // Turned buildings survive a save.
    let mut saved = Vec::new();
    opencity_core::save::save(&c, &mut saved).unwrap();
    let back = opencity_core::save::load(saved.as_slice(), Footprints::builtin()).unwrap();
    assert_eq!(back.layer.structure_at(6, 8).unwrap().facing, t.facing);
    // Roads are not turned.
    assert!(c.turn_building(5, 3).is_err());
}

#[test]
fn taxes_and_policies_steer_the_city() {
    let r = rules();
    assert_eq!(r.policy_set.as_deref(), Some("modern"));
    assert!(r.tax("property").is_some() && r.policy("energy").is_some());
    let build = || {
        let mut c = City::new(Terrain::flat(48, 48), None, Footprints::builtin(), 7);
        c.use_rules(r.clone());
        c.research_all();
        c.apply_tool(Tool::CoalPlant, (2, 2), (2, 2)).unwrap();
        for l in [8, 12, 16, 20] {
            c.apply_tool(Tool::Road, (2, l), (44, l)).unwrap();
        }
        c.apply_tool(Tool::Road, (7, 2), (7, 8)).unwrap();
        c.apply_tool(Tool::ZoneResidential, (2, 9), (44, 11)).unwrap();
        c.apply_tool(Tool::ZoneIndustrial, (2, 13), (44, 15)).unwrap();
        c.apply_tool(Tool::ZoneCommercial, (2, 17), (44, 19)).unwrap();
        for _ in 0..300 {
            c.simulate_round();
        }
        c
    };
    let mut c = build();
    let taxes = r.resource("taxes").unwrap();
    {
        let gb = c.glassbox.as_mut().unwrap();
        assert_eq!(gb.tax_rate(&r, "property"), 9.0);
        // Rates stay within range.
        assert!(gb.set_tax_rate("property", 50.0));
        assert_eq!(gb.tax_rate(&r, "property"), 20.0);
        assert!(gb.set_tax_rate("property", 9.0) && gb.tax_rates.is_empty());
    }
    // Doubling every tax doubles the taxes made, and cuts demand.
    let mut doubled = c.clone();
    for t in ["property", "sales", "business"] {
        doubled.glassbox.as_mut().unwrap().set_tax_rate(t, 18.0);
    }
    c.simulate_round();
    doubled.simulate_round();
    let (a, b) = (c.glassbox.as_ref().unwrap(), doubled.glassbox.as_ref().unwrap());
    assert!((b.balances.produced[taxes] / a.balances.produced[taxes] - 2.0).abs() < 1e-3, "{} -> {}", a.balances.produced[taxes], b.balances.produced[taxes]);
    assert!(b.demand[0] < a.demand[0]);
    // A stance that needs research waits for it; an open one takes effect.
    let gb = c.glassbox.as_mut().unwrap();
    gb.researched.remove("fission");
    assert!(!gb.set_stance("energy", "atomic_age"));
    assert!(gb.set_stance("growth", "open_city"));
    assert_eq!(gb.stance(&r, "growth").unwrap().id, "open_city");
    assert_eq!(gb.stance(&r, "energy").unwrap().id, "balanced");
    let before = c.glassbox.as_ref().unwrap().demand[0];
    c.simulate_round();
    assert!(c.glassbox.as_ref().unwrap().demand[0] > before);
    // Saves keep rates and stances.
    c.glassbox.as_mut().unwrap().set_tax_rate("sales", 4.0);
    let mut saved = Vec::new();
    opencity_core::save::save(&c, &mut saved).unwrap();
    let back = opencity_core::save::load(saved.as_slice(), Footprints::builtin()).unwrap();
    let gb = back.glassbox.as_ref().unwrap();
    assert_eq!(gb.tax_rates.get("sales"), Some(&4.0));
    assert_eq!(gb.stances.get("growth").map(String::as_str), Some("open_city"));
}

#[test]
fn railways_link_towns_no_road_joins() {
    let r = rules();
    let build = |rail: bool| {
        let mut c = City::new(Terrain::flat(64, 24), None, Footprints::builtin(), 5);
        c.fund = 10_000_000;
        c.use_rules(r.clone());
        c.research_all();
        // West: houses. East: workshops and shops. No road between them.
        for (w1, w2, zone) in [(2, 16, Tool::ZoneResidential), (46, 60, Tool::ZoneIndustrial)] {
            c.apply_tool(Tool::Road, (w1, 8), (w2, 8)).unwrap();
            c.apply_tool(Tool::Road, (w1, 12), (w2, 12)).unwrap();
            c.apply_tool(Tool::Road, (w1, 8), (w1, 12)).unwrap();
            c.apply_tool(zone, (w1 + 1, 9), (w2, 11)).unwrap();
            // The plant touches the street, which carries its power.
            c.apply_tool(Tool::CoalPlant, (w1 + 2, 5), (w1 + 2, 5)).unwrap();
        }
        if rail {
            c.apply_tool(Tool::Rail, (5, 18), (58, 18)).unwrap();
            c.apply_tool(Tool::Station, (6, 16), (6, 16)).unwrap();
            c.apply_tool(Tool::Station, (50, 16), (50, 16)).unwrap();
            c.apply_tool(Tool::Road, (6, 13), (6, 15)).unwrap();
            c.apply_tool(Tool::Road, (50, 13), (50, 15)).unwrap();
        }
        for _ in 0..400 {
            c.simulate_round();
        }
        c
    };
    let crossing = |c: &City| -> f32 {
        let gb = c.glassbox.as_ref().unwrap();
        gb.flows.iter().filter(|(l, _)| (l.from.0 < 30) != (l.to.0 < 30)).map(|(_, v)| *v).sum()
    };
    let (without, with) = (build(false), build(true));
    let rail = opencity_core::rail::Railways::find(&with.layer, &with.terrain, with.footprints());
    assert_eq!(rail.stations.len(), 2, "both stations stand by the track");
    assert_eq!(crossing(&without), 0.0, "no road, no trips between the towns");
    assert!(crossing(&with) > 0.0, "the railway carries trips between the towns");
    // Cars never drive on the track; trains run between the stations.
    assert!(with.vehicles.iter().filter(|v| v.on_rail()).all(|v| v.mesh.as_deref().is_some_and(|m| m.contains("train"))));
}

#[test]
fn services_cover_what_they_can_drive_to_and_send_vehicles() {
    let r = rules();
    let mut c = City::new(Terrain::flat(48, 32), None, Footprints::builtin(), 3);
    c.fund = 10_000_000;
    c.use_rules(r.clone());
    c.research_all();
    // A street with the fire station and houses on it; a second street just
    // as near, but not joined to the first.
    c.apply_tool(Tool::Road, (2, 10), (40, 10)).unwrap();
    c.apply_tool(Tool::Road, (2, 16), (40, 16)).unwrap();
    c.apply_tool(Tool::CoalPlant, (2, 5), (2, 5)).unwrap();
    c.apply_tool(Tool::CoalPlant, (2, 17), (2, 17)).unwrap();
    c.apply_tool(Tool::FireDept, (20, 8), (20, 8)).unwrap();
    c.apply_tool(Tool::ZoneResidential, (8, 11), (34, 11)).unwrap();
    c.apply_tool(Tool::ZoneResidential, (8, 15), (34, 15)).unwrap();
    for _ in 0..200 {
        c.simulate_round();
    }
    let gb = c.glassbox.as_ref().unwrap();
    let fire = r.layer("fire_safety").unwrap();
    let at = |w: u32, l: u32| gb.layer_value(fire, w, l, c.width());
    // Same distance from the station as the crow flies, very different by road.
    assert!(at(21, 11) > 1.0, "on the station's street: {}", at(21, 11));
    assert!(at(21, 15) < at(21, 11) / 2.0, "the other street cannot be driven to: {} vs {}", at(21, 15), at(21, 11));
    // Fire trucks go out along the streets.
    let trucks = c.vehicles.iter().filter(|v| v.mesh.as_deref() == Some("kenney/car-kit/firetruck")).count();
    let mut seen = trucks;
    let mut c2 = c.clone();
    for _ in 0..100 {
        // Without real time passing vehicles never arrive: make room.
        c2.vehicles.clear();
        c2.simulate_round();
        seen += c2.vehicles.iter().filter(|v| v.mesh.as_deref() == Some("kenney/car-kit/firetruck")).count();
    }
    assert!(seen > 0, "no fire truck was sent");
}

/// How far from a street a building may stand (its walk to the network).
fn rules_reach(r: &opencity_core::rules::Ruleset) -> u32 {
    r.growth.road_access.max(1) * 2 + 3
}

#[test]
fn neighbouring_cities_trade_across_their_borders() {
    use opencity_core::region::Region;
    let r = rules();
    // `joined`: the border road meets the town's streets; `plant`: the town
    // makes its own power.
    let town = |zone: Tool, edge_west: bool, joined: bool, plant: bool| {
        let mut c = City::new(Terrain::flat(40, 24), None, Footprints::builtin(), 9);
        c.fund = 10_000_000;
        c.use_rules(r.clone());
        c.research_all();
        c.apply_tool(Tool::Road, (4, 8), (35, 8)).unwrap();
        c.apply_tool(Tool::Road, (4, 12), (35, 12)).unwrap();
        c.apply_tool(Tool::Road, (4, 8), (4, 12)).unwrap();
        c.apply_tool(zone, (5, 9), (33, 11)).unwrap();
        if plant {
            c.apply_tool(Tool::CoalPlant, (6, 5), (6, 5)).unwrap();
        }
        // A road out to the shared edge: the border crossing.
        let end = if joined { 4 } else { 2 };
        if edge_west {
            c.apply_tool(Tool::Road, (0, 10), (end, 10)).unwrap();
        } else {
            c.apply_tool(Tool::Road, (35, 10), (39, 10)).unwrap();
        }
        c
    };
    // West: a commuter town with workers and power to spare.
    let mut west = town(Tool::ZoneResidential, false, true, true);
    for _ in 0..300 {
        west.simulate_round();
    }
    let mut region = Region::new("Test", [2, 1], 40, 1, Some("glassbox".into()));
    region.summarize([0, 0], "Westville", "west.ocsave.json", &west);
    assert_eq!(region.cities[0].borders, [false, true, false, false]);
    let workers = r.resource("workers").unwrap();
    assert!(region.cities[0].surplus["workers"] > 1.0, "{:?}", region.cities[0].surplus);
    // East: workshops short of workers, trading month by month.
    let run = |trade: bool, joined: bool, plant: bool| {
        let mut east = town(Tool::ZoneIndustrial, true, joined, plant);
        let mut imported = vec![0.0; r.resources.len()];
        for t in 0..300 {
            if trade && t % 30 == 29 {
                let deals = region.trade([1, 0], &east);
                assert!(deals.iter().all(|d| d.gates.iter().all(|g| g.side == 3 && g.at == 10)), "{deals:?}");
                east.apply_trade(&deals);
            }
            east.simulate_round();
            let gb = east.glassbox.as_ref().unwrap();
            imported.iter_mut().zip(&gb.balances.imported).for_each(|(a, b)| *a += b);
        }
        // The network view: links out of the crossing's tile.
        let from_gate = east.glassbox.as_ref().unwrap().flows.keys().filter(|l| l.from == (0, 10) && l.resource == workers).count();
        (east, imported, from_gate)
    };
    let (alone, none, _) = run(false, true, true);
    let (trading, imported, from_gate) = run(true, true, true);
    assert!(none.iter().all(|v| *v == 0.0));
    assert!(imported[workers] > 0.0, "commuters came over the border");
    assert!(from_gate > 0, "commuters set out from the crossing");
    let gb = trading.glassbox.as_ref().unwrap();
    assert!(gb.gates.iter().any(|g| g.at == (0, 10) && g.import.contains_key("workers")), "{:?}", gb.gates);
    let fill = |c: &City| {
        let gb = c.glassbox.as_ref().unwrap();
        gb.units.values().map(|u| u.satisfaction).sum::<f32>() / gb.units.len().max(1) as f32
    };
    assert!(fill(&trading) > fill(&alone), "workshops are better staffed: {} vs {}", fill(&trading), fill(&alone));
    // The network view follows the streets: commuters leave the gate along
    // the border road, tile by tile, and every street stretch joins
    // neighbouring tiles.
    use opencity_core::netmap::Way;
    let links: Vec<_> = gb.flows.iter().map(|(k, v)| (*k, *v)).collect();
    let stretches = trading.network_stretches(&links);
    assert!(
        stretches.iter().any(|s| s.from == (0, 10) && s.to == (1, 10) && s.resource == workers && s.way == Way::Street),
        "commuters drive in along the border road"
    );
    assert!(stretches.iter().filter(|s| s.way == Way::Street).all(|s| s.tiles() <= rules_reach(&r) + 2), "street stretches are short steps");
    // A crossing the town's streets do not reach carries nothing.
    let (_, cut, _) = run(true, false, true);
    assert_eq!(cut[workers], 0.0, "no road from the crossing to the town");
    // Power comes over the border along the street into a town without a plant.
    let power = r.resource("power").unwrap();
    let (dark, unpowered, _) = run(true, true, false);
    assert!(unpowered[power] > 0.0, "power bought from the neighbour: {:?}", dark.glassbox.as_ref().unwrap().deficit[power]);
    let gb = dark.glassbox.as_ref().unwrap();
    let links: Vec<_> = gb.flows.iter().filter(|(k, _)| k.resource == power).map(|(k, v)| (*k, *v)).collect();
    let grid = dark.network_stretches(&links);
    assert!(grid.iter().any(|s| s.from == (0, 10) && s.way == Way::Grid), "power runs in from the gate through the grid: {:?}", &grid[..grid.len().min(5)]);
    // The region file round-trips.
    assert_eq!(Region::from_json(&region.to_json()).unwrap(), region);
}

#[test]
fn harbours_link_towns_across_a_lake() {
    let r = rules();
    let build = |harbours: bool| {
        let mut t = Terrain::flat(64, 24);
        for w in 22..=42 {
            for l in 0..=24 {
                t.set_corner(w, l, -1);
            }
        }
        let mut c = City::new(t, None, Footprints::builtin(), 5);
        c.fund = 10_000_000;
        c.use_rules(r.clone());
        c.research_all();
        for (w1, w2, zone) in [(2, 16, Tool::ZoneResidential), (47, 60, Tool::ZoneIndustrial)] {
            c.apply_tool(Tool::Road, (w1, 8), (w2, 8)).unwrap();
            c.apply_tool(Tool::Road, (w1, 12), (w2, 12)).unwrap();
            c.apply_tool(Tool::Road, (w1, 8), (w1, 12)).unwrap();
            c.apply_tool(zone, (w1 + 1, 9), (w2, 11)).unwrap();
            c.apply_tool(Tool::CoalPlant, (w1 + 2, 5), (w1 + 2, 5)).unwrap();
        }
        if harbours {
            c.apply_tool(Tool::Road, (16, 14), (20, 14)).unwrap();
            c.apply_tool(Tool::Road, (16, 12), (16, 14)).unwrap();
            c.apply_tool(Tool::Harbour, (19, 15), (19, 15)).unwrap();
            c.apply_tool(Tool::Road, (43, 14), (47, 14)).unwrap();
            c.apply_tool(Tool::Road, (47, 12), (47, 14)).unwrap();
            c.apply_tool(Tool::Harbour, (43, 15), (43, 15)).unwrap();
        }
        for _ in 0..400 {
            c.simulate_round();
        }
        c
    };
    let crossing = |c: &City| -> f32 {
        let gb = c.glassbox.as_ref().unwrap();
        gb.flows.iter().filter(|(l, _)| (l.from.0 < 30) != (l.to.0 < 30)).map(|(_, v)| *v).sum()
    };
    let (without, with) = (build(false), build(true));
    let water = opencity_core::rail::Railways::water(&with.layer, &with.terrain, with.footprints());
    assert_eq!(water.stations.len(), 2, "both harbours stand on the shore: {:?}", water.stations);
    assert_eq!(crossing(&without), 0.0);
    assert!(crossing(&with) > 0.0, "boats carry trips across the lake");
    assert!(with.vehicles.iter().filter(|v| v.water).all(|v| v.route.iter().all(|w| with.terrain.is_water(w.w, w.l))));
}

#[test]
fn canals_dig_water_through_empty_land() {
    let mut c = City::new(Terrain::flat(30, 20), None, Footprints::builtin(), 1);
    c.fund = 100_000;
    let before = c.fund;
    c.apply_tool(Tool::Canal, (2, 10), (20, 10)).unwrap();
    assert!((2..=20).all(|w| c.terrain.is_water(w, 10)));
    assert!(c.fund < before);
    // Not next to buildings.
    c.apply_tool(Tool::Road, (5, 15), (12, 15)).unwrap();
    assert!(c.apply_tool(Tool::Canal, (4, 14), (12, 14)).is_err());
}

#[test]
fn disasters_strike_and_coverage_protects() {
    let r = rules();
    assert!(r.disaster("fire").is_some() && r.disaster("flood").is_some_and(|d| d.near_water));
    let town = |station: bool| {
        let mut c = City::new(Terrain::flat(48, 32), None, Footprints::builtin(), 3);
        c.fund = 10_000_000;
        c.use_rules(r.clone());
        c.research_all();
        c.glassbox.as_mut().unwrap().disasters_off = true;
        c.apply_tool(Tool::Road, (2, 10), (40, 10)).unwrap();
        c.apply_tool(Tool::Road, (2, 14), (40, 14)).unwrap();
        c.apply_tool(Tool::Road, (2, 10), (2, 14)).unwrap();
        c.apply_tool(Tool::CoalPlant, (4, 5), (4, 5)).unwrap();
        c.apply_tool(Tool::ZoneResidential, (3, 11), (40, 13)).unwrap();
        if station {
            c.apply_tool(Tool::FireDept, (20, 8), (20, 8)).unwrap();
        }
        for _ in 0..300 {
            c.simulate_round();
        }
        c
    };
    // Switched off, nothing strikes by chance.
    let quiet = town(false);
    assert!(quiet.notices.is_empty());
    let losses = |c: &City| -> u32 {
        (0..12u64)
            .map(|i| {
                let mut c = c.clone();
                let gb = c.glassbox.as_mut().unwrap();
                let mut rng = <rand_pcg::Pcg32 as rand::SeedableRng>::seed_from_u64(i);
                gb.strike("fire", Some((21, 12)), &mut c.layer, &c.terrain, &Footprints::builtin(), &mut rng).unwrap().hit
            })
            .sum()
    };
    let (bare, covered) = (losses(&quiet), losses(&town(true)));
    assert!(bare > 0, "fires burn houses");
    assert!(covered < bare, "a fire station nearby saves houses: {covered} vs {bare}");
    // By hand: a notice, and the fire trucks roll.
    let mut c = town(true);
    let e = c.strike("fire").unwrap();
    assert_eq!(c.notices.last(), Some(&e));
    assert!(c.vehicles.iter().any(|v| v.mesh.as_deref() == Some("kenney/car-kit/firetruck")));
    // Floods need water nearby: none on this dry map.
    assert!(c.strike("flood").is_none());
}

#[test]
fn cousin_vinnie_brings_money_and_the_family() {
    let r = rules();
    let mut c = City::new(Terrain::flat(32, 24), None, Footprints::builtin(), 3);
    c.use_rules(r.clone());
    c.research_all();
    c.glassbox.as_mut().unwrap().disasters_off = true;
    let before = c.fund;
    assert!(c.cheat("give me money").is_none());
    assert!(c.cheat("CallCousinVinnie;").is_some());
    assert_eq!(c.fund, before + 1_000_000);
    assert!(c.mafia.as_ref().is_some_and(|m| m.heat == 1.0));
    // A month later the family collects.
    let fund = c.fund;
    for _ in 0..31 {
        c.update(opencity_core::city::MS_PER_DAY);
    }
    let m = c.mafia.as_ref().unwrap();
    assert!(m.collected > 0 && c.fund < fund, "protection money: {}", m.collected);
    assert!(c.messages.iter().any(|t| t.contains("protection money")));
    // Police stations drive them out over the months.
    c.fund = 10_000_000;
    c.apply_tool(Tool::Road, (2, 10), (30, 10)).unwrap();
    for x in [4, 10, 16, 22] {
        c.apply_tool(Tool::PoliceDept, (x, 11), (x, 11)).unwrap();
    }
    for _ in 0..(30 * 12) {
        c.update(opencity_core::city::MS_PER_DAY);
    }
    assert!(c.mafia.is_none(), "the family left: {:?}", c.mafia);
    // Saved and loaded, the family stays.
    c.cheat("callcousinvinnie");
    let mut saved = Vec::new();
    opencity_core::save::save(&c, &mut saved).unwrap();
    let back = opencity_core::save::load(saved.as_slice(), Footprints::builtin()).unwrap();
    assert!(back.mafia.is_some());
}

#[test]
fn the_budget_books_every_line_and_funding_matters() {
    let r = rules();
    let mut c = City::new(Terrain::flat(48, 32), None, Footprints::builtin(), 3);
    c.fund = 1_000_000;
    c.use_rules(r.clone());
    c.research_all();
    c.glassbox.as_mut().unwrap().disasters_off = true;
    c.apply_tool(Tool::Road, (2, 10), (40, 10)).unwrap();
    c.apply_tool(Tool::Road, (2, 14), (40, 14)).unwrap();
    c.apply_tool(Tool::Road, (2, 10), (2, 14)).unwrap();
    c.apply_tool(Tool::CoalPlant, (4, 5), (4, 5)).unwrap();
    c.apply_tool(Tool::ZoneResidential, (3, 11), (40, 13)).unwrap();
    c.apply_tool(Tool::FireDept, (20, 8), (20, 8)).unwrap();
    for _ in 0..(30 * 3) {
        c.update(opencity_core::city::MS_PER_DAY);
    }
    let last = c.last_ledger.clone();
    assert!(last.keys().any(|k| k.starts_with("Taxes: Property")), "{last:?}");
    assert!(last.keys().any(|k| k == "Upkeep: Fire station"), "{last:?}");
    // Loans: money now, monthly repayments booked.
    let fund = c.fund;
    let loan = c.take_loan(50_000).unwrap();
    assert_eq!(c.fund, fund + 50_000);
    assert!(loan.payment * loan.months_left as i64 > 50_000, "interest is charged");
    for _ in 0..31 {
        c.update(opencity_core::city::MS_PER_DAY);
    }
    assert!(c.last_ledger.get("Loans: repayments").is_some_and(|v| *v < 0.0));
    assert!(c.take_loan(1).is_ok() && c.take_loan(1).is_ok() && c.take_loan(1).is_err(), "three loans at most");
    // Funding: half-funded fire stations cost less and cover less.
    let fire = r.layer("fire_safety").unwrap();
    let mut half = c.clone();
    half.glassbox.as_mut().unwrap().set_funding("fire_station", 0.5);
    for _ in 0..60 {
        c.simulate_round();
        half.simulate_round();
    }
    let cover = |c: &City| c.glassbox.as_ref().unwrap().layer_value(fire, 21, 11, c.width());
    assert!(cover(&half) < cover(&c), "{} vs {}", cover(&half), cover(&c));
    let upkeep = |c: &City| c.glassbox.as_ref().unwrap().ledger.get("Upkeep: Fire station").copied().unwrap_or(0.0);
    assert!(upkeep(&half) > upkeep(&c), "half the cost: {} vs {}", upkeep(&half), upkeep(&c));
}

#[test]
fn goals_are_won_and_lost() {
    use opencity_core::goals::{GoalDef, GoalStatus, Scenario};
    let goal = |name: &str, metric: &str, at_least: f64, within: Option<u32>, always: bool| GoalDef {
        name: name.into(),
        metric: metric.into(),
        at_least: Some(at_least),
        at_most: None,
        within_years: within,
        always,
    };
    let scenario = |goals| Scenario { id: "t".into(), name: "Test".into(), goals, ..Default::default() };
    let month = |c: &mut City| {
        for _ in 0..31 {
            c.update(opencity_core::city::MS_PER_DAY);
        }
    };
    // Won: money in hand, and kept out of debt.
    let mut c = City::new(Terrain::flat(24, 24), None, Footprints::builtin(), 1);
    c.set_goals(&scenario(vec![goal("Rich", "fund", 1000.0, Some(2), false), goal("No debt", "fund", 0.0, None, true)]));
    month(&mut c);
    assert_eq!(c.goals.as_ref().unwrap().status, GoalStatus::Won);
    assert!(c.messages.iter().any(|m| m.contains("won")));
    // Lost: in debt.
    let mut c = City::new(Terrain::flat(24, 24), None, Footprints::builtin(), 1);
    c.set_goals(&scenario(vec![goal("Big", "population", 1e9, None, false), goal("No debt", "fund", 0.0, None, true)]));
    c.fund = -5;
    month(&mut c);
    assert!(matches!(c.goals.as_ref().unwrap().status, GoalStatus::Lost(_)));
    // Lost: out of time.
    let mut c = City::new(Terrain::flat(24, 24), None, Footprints::builtin(), 1);
    c.set_goals(&scenario(vec![goal("Big", "population", 1e9, Some(1), false)]));
    for _ in 0..13 {
        month(&mut c);
    }
    assert_eq!(c.goals.as_ref().unwrap().status, GoalStatus::Lost("Big: out of time".into()));
    // Saves keep the goals.
    let mut saved = Vec::new();
    opencity_core::save::save(&c, &mut saved).unwrap();
    assert!(opencity_core::save::load(saved.as_slice(), Footprints::builtin()).unwrap().goals.is_some());
}

#[test]
fn ideas_spread_across_a_region() {
    use opencity_core::region::{Region, RegionCity};
    let r = rules();
    assert!(r.resources[r.resource("ideas").unwrap()].shared);
    let mut region = Region::new("R", [2, 1], 32, 1, Some("glassbox".into()));
    let mut thinker = RegionCity { slot: [1, 0], name: "Athens".into(), ..Default::default() };
    thinker.culture.insert("ideas".into(), 20.0);
    region.cities.push(thinker);
    let mut c = City::new(Terrain::flat(24, 24), None, Footprints::builtin(), 1);
    c.use_rules(r.clone());
    let ideas = r.resource("ideas").unwrap();
    let inflow = region.culture_for([0, 0]);
    assert!((inflow["ideas"] - 2.0).abs() < 1e-6, "a tenth of the rest of the region");
    c.apply_culture(&inflow);
    for _ in 0..10 {
        c.simulate_round();
    }
    assert!(c.glassbox.as_ref().unwrap().pools[ideas] >= 19.0, "ideas arrived: {}", c.glassbox.as_ref().unwrap().pools[ideas]);
}

#[test]
fn seasons_and_weather_turn() {
    use opencity_core::city::{Season, Weather};
    let mut c = City::new(Terrain::flat(16, 16), None, Footprints::builtin(), 4);
    assert!((6.0..18.0).contains(&c.hour()), "a new city opens in daylight, not at {}", c.hour());
    let mut seen: Vec<(Season, Weather)> = Vec::new();
    for _ in 0..(30 * 24) {
        c.update(opencity_core::city::MS_PER_DAY);
        seen.push((c.season(), c.weather));
        assert!((0.0..24.0).contains(&c.hour()));
    }
    assert!(seen.iter().any(|(s, w)| *s == Season::Winter && *w == Weather::Snow), "snow in winter");
    assert!(!seen.iter().any(|(s, w)| *s == Season::Summer && *w == Weather::Snow), "never in summer");
    assert!(seen.iter().any(|(_, w)| *w == Weather::Rain));
    // Weather lasts a week.
    let changes = seen.windows(2).filter(|w| w[0].1 != w[1].1).count();
    assert!(changes < seen.len() / 5, "{changes} changes in {} days", seen.len());
}

#[test]
fn border_crossings_join_neighbouring_maps() {
    use opencity_core::region::{Crossing, Region};
    let mut west = City::new(Terrain::flat(32, 32), None, Footprints::builtin(), 3);
    west.fund = 1_000_000;
    west.apply_tool(Tool::Road, (20, 10), (31, 10)).unwrap();
    west.apply_tool(Tool::PowerLine, (25, 3), (31, 3)).unwrap();
    let out: Vec<_> = west.crossings().into_iter().filter(|c| c.side == 1).collect();
    assert_eq!(out, vec![Crossing { side: 1, at: 3, link: "power_line".into() }, Crossing { side: 1, at: 10, link: "street".into() },]);
    let mut region = Region::new("Test", [2, 1], 32, 1, None);
    region.summarize([0, 0], "Westville", "west.ocsave.json", &west);
    assert!(region.has_slot([0, 0], 1) && !region.has_slot([0, 0], 3) && !region.has_slot([0, 0], 0));
    // East sees them on its west edge, at the same rows.
    let incoming = region.incoming([1, 0]);
    assert_eq!(incoming.len(), 2);
    assert!(incoming.iter().all(|(from, c)| from == "Westville" && c.side == 3));
    let mut east = City::new(Terrain::flat(32, 32), None, Footprints::builtin(), 4);
    east.fund = 0;
    for (_, c) in &incoming {
        assert_eq!(east.connect_border(c), Ok(true), "{c:?}");
        assert_eq!(east.connect_border(c), Ok(false), "built once");
    }
    assert_eq!(east.fund, 0, "the neighbour's crossing is free");
    let back: Vec<_> = east.crossings().into_iter().map(|c| c.opposite()).collect();
    assert_eq!(back, out, "both maps have the same crossings");
    assert!(east.layer.structure_at(0, 10).is_some_and(|s| s.code == opencity_core::StructureCode::Road));
    // Bulldozed, a neighbour's crossing stays gone.
    east.fund = 1_000;
    east.apply_tool(Tool::Destroy, (0, 3), (0, 3)).unwrap();
    assert_eq!(east.connect_border(&incoming[0].1), Ok(false));
    assert!(east.layer.structure_at(0, 3).is_none());
    // A railway is not joined by a street.
    let rail = Crossing { side: 3, at: 10, link: "rail".into() };
    assert!(east.connect_border(&rail).is_err(), "the street is in the way");
}

#[test]
fn water_and_waste_serve_the_buildings() {
    let r = rules();
    let water = r.resource("water").unwrap();
    let sewage = r.resource("sewage").unwrap();
    let waste = r.resource("waste").unwrap();
    // The demo town has a pumping station, a treatment plant, a water tower
    // and a dump: the bigger buildings get water, sewage treatment and
    // rubbish trucks.
    let mut c = City::new(Terrain::flat(60, 60), None, Footprints::builtin(), 5);
    c.use_rules(r.clone());
    opencity_core::scenario::demo_town(&mut c);
    for code in [StructureCode::PumpingStation, StructureCode::WaterTreatment, StructureCode::WaterTower, StructureCode::Dump] {
        assert!(c.layer.structures().any(|(_, s)| s.code == code), "{code:?} placed");
    }
    let (mut got, mut made) = (vec![0.0f32; r.resources.len()], vec![0.0f32; r.resources.len()]);
    for _ in 0..800 {
        c.simulate_round();
        let gb = c.glassbox.as_ref().unwrap();
        got.iter_mut().zip(&gb.balances.delivered).for_each(|(a, b)| *a += b);
        made.iter_mut().zip(&gb.balances.produced).for_each(|(a, b)| *a += b);
    }
    // Low-density towns need water and rubbish collection; sewage treatment
    // waits for denser buildings, but the plant runs.
    assert!(got[water] > 0.0 && got[waste] > 0.0, "water {} waste {}", got[water], got[waste]);
    assert!(made[sewage] > 0.0, "the treatment plant works");
    let gb = c.glassbox.as_ref().unwrap();
    assert!(gb.flows.keys().any(|l| l.resource == waste), "rubbish trucks make rounds");

    // A pump off the streets reaches them only along a water pipe.
    let run = |pipe: bool| {
        let mut c = City::new(Terrain::flat(40, 24), None, Footprints::builtin(), 3);
        c.fund = 10_000_000;
        c.use_rules(r.clone());
        c.research_all();
        c.apply_tool(Tool::Road, (4, 8), (35, 8)).unwrap();
        c.apply_tool(Tool::Road, (4, 12), (35, 12)).unwrap();
        c.apply_tool(Tool::Road, (4, 8), (4, 12)).unwrap();
        c.apply_tool(Tool::ZoneResidential, (5, 9), (33, 11)).unwrap();
        c.apply_tool(Tool::CoalPlant, (6, 3), (6, 3)).unwrap();
        c.apply_tool(Tool::Road, (6, 6), (12, 6)).unwrap();
        c.apply_tool(Tool::Road, (12, 6), (12, 8)).unwrap();
        // The pump stands in a field to the south, with a road for its workers
        // that does not join the town's streets.
        c.apply_tool(Tool::PumpingStation, (20, 17), (20, 17)).unwrap();
        c.apply_tool(Tool::Road, (19, 19), (25, 19)).unwrap();
        if pipe {
            c.apply_tool(Tool::WaterPipe, (20, 13), (20, 16)).unwrap();
        }
        let mut water_in = 0.0;
        for _ in 0..300 {
            c.simulate_round();
            water_in += c.glassbox.as_ref().unwrap().balances.delivered[water];
        }
        water_in
    };
    let (dry, piped) = (run(false), run(true));
    assert_eq!(dry, 0.0, "no pipe, no water");
    assert!(piped > 0.0, "the pipe carries the pump's water to the town");
}
