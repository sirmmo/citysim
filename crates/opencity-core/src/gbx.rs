//! The GlassBox rule language (`.gbx` scripts).
//!
//! Rules in the style Maxis presented for SimCity's GlassBox engine (GDC
//! 2012) and its extension for urban digital twins by Costagliola, Montanari
//! and Bellavista ("Interconnecting Urban Networks: a Novel Approach to
//! Digital Twins through GlassBox Adaptation"), compiled into a [`Ruleset`]:
//!
//! ```text
//! unitRule mustardFactory          # behaviour of a unit
//!     appliesTo factory_1          # unit id (defaults to the rule name)
//!     rate 10                      # every 10 ticks (amounts are averaged per tick)
//!     global Simoleans in 1        # from the treasury
//!     local Workers in 6           # needed
//!     agent Goods out 1            # produced and sent by agents
//!     map Pollution out 5 radius 3 # emitted into a map
//!     map LandValue atLeast 2      # condition on the map at the unit
//!     log -timestamp=Time Output 1 # Extended GlassBox metric
//! end
//!
//! mapRule growGrass                # every tile
//!     rate 200
//!     map Soil atLeast 20
//!     map Water in 10
//!     map Grass out 5
//! end
//!
//! zoneRule developHouses           # development
//!     appliesTo residential
//!     sample random -count 3
//!     test global Builders greater 5
//!     test map Forest is 0
//!     createUnit -id house_1
//! end
//!
//! agentRule carPollution           # agents moving on networks
//!     condition resource=Goods
//!     repeatAfter 10
//!     area -timestamp=Time -map=AirQuality pm10 10 5
//! end
//! ```
//!
//! Resources and maps that are used but not declared in the TOML file are
//! declared automatically: `agent` resources travel by agents on the network
//! named by `using` / `-using` (default `road`), `global` ones go through the
//! treasury, others stay local. Presentation statements (`successEvent`,
//! `onFail`) are accepted and ignored.

use crate::rules::{AgentRule, AreaEmission, Bound, Cmp, Emission, LayerDef, MapRule, ResourceDef, RulesError, Ruleset, Scope, Test, Transport, ZoneRule};

#[derive(Clone, Debug, PartialEq)]
struct Line {
    no: usize,
    words: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct Block {
    kind: String,
    name: String,
    lines: Vec<Line>,
}

fn tokenize(text: &str) -> Vec<Line> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let l = l.split('#').next().unwrap_or("").split("//").next().unwrap_or("");
            let words: Vec<String> = l.split_whitespace().map(str::to_string).collect();
            (!words.is_empty()).then_some(Line { no: i + 1, words })
        })
        .collect()
}

fn blocks(file: &str, text: &str) -> Result<Vec<Block>, RulesError> {
    let err = |no: usize, m: String| RulesError { rules: file.to_string(), message: format!("line {no}: {m}") };
    let mut out = Vec::new();
    let mut cur: Option<Block> = None;
    for line in tokenize(text) {
        let head = line.words[0].as_str();
        match (&mut cur, head) {
            (None, "rule" | "unitRule" | "mapRule" | "zoneRule" | "agentRule") => {
                let name = line.words.get(1).cloned().ok_or_else(|| err(line.no, format!("{head} needs a name")))?;
                cur = Some(Block { kind: head.to_string(), name, lines: Vec::new() });
            }
            (None, _) => return Err(err(line.no, format!("expected a rule, found {head:?}"))),
            (Some(_), "end") => out.push(cur.take().unwrap()),
            (Some(b), _) => b.lines.push(line),
        }
    }
    if let Some(b) = cur {
        return Err(err(0, format!("rule {} is missing `end`", b.name)));
    }
    Ok(out)
}

fn num(file: &str, line: &Line, i: usize) -> Result<f32, RulesError> {
    line.words.get(i).and_then(|w| w.parse::<f32>().ok()).ok_or_else(|| RulesError {
        rules: file.to_string(),
        message: format!("line {}: expected a number after {:?}", line.no, line.words[..i.min(line.words.len())].join(" ")),
    })
}

fn cmp(word: &str) -> Option<Cmp> {
    Some(match word {
        "atLeast" => Cmp::AtLeast,
        "atMost" => Cmp::AtMost,
        "greater" => Cmp::Greater,
        "less" => Cmp::Less,
        "is" => Cmp::Is,
        _ => return None,
    })
}

/// `-key=value` / `-key value` options of a statement.
fn option(words: &[String], key: &str) -> Option<String> {
    let flag = format!("-{key}");
    let mut it = words.iter();
    while let Some(w) = it.next() {
        if let Some(v) = w.strip_prefix(&format!("{flag}=")) {
            return Some(v.to_string());
        }
        if *w == flag {
            return it.next().cloned();
        }
    }
    None
}

/// Words that are not `-options`.
fn plain(words: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut skip = false;
    for w in words {
        if skip {
            skip = false;
            continue;
        }
        if w.starts_with('-') {
            skip = !w.contains('=') && matches!(w.as_str(), "-sendTo" | "-via" | "-using" | "-count" | "-id" | "-map" | "-timestamp");
            continue;
        }
        out.push(w.as_str());
    }
    out
}

/// Compiles a GlassBox script into `rules`. Returns warnings for ignored statements.
pub fn compile(rules: &mut Ruleset, file: &str, text: &str) -> Result<Vec<String>, RulesError> {
    let mut warnings = Vec::new();
    let err = |no: usize, m: String| RulesError { rules: file.to_string(), message: format!("line {no}: {m}") };
    for b in blocks(file, text)? {
        let mut every = 1.0f32;
        let mut network: Option<String> = None;
        let mut applies: Option<String> = None;
        for l in &b.lines {
            match l.words[0].as_str() {
                "rate" | "repeatAfter" => every = num(file, l, 1)?.max(1.0),
                "appliesTo" => {
                    applies = l.words.get(1).cloned();
                    if let Some(i) = l.words.iter().position(|w| w == "using") {
                        network = l.words.get(i + 1).map(|n| n.to_lowercase());
                    }
                }
                "options" => {
                    if let Some(n) = option(&l.words, "using") {
                        network = Some(n.to_lowercase());
                    }
                }
                _ => {}
            }
        }
        let scale = 1.0 / every;
        match b.kind.as_str() {
            "rule" | "unitRule" => {
                let unit_id = applies.clone().unwrap_or_else(|| b.name.clone());
                let Some(ui) = rules.unit(&unit_id) else {
                    return Err(err(b.lines.first().map_or(0, |l| l.no), format!("{} {}: unknown unit {unit_id}", b.kind, b.name)));
                };
                for l in &b.lines {
                    let w: Vec<&str> = l.words.iter().map(String::as_str).collect();
                    match w.as_slice() {
                        ["rate" | "appliesTo" | "options", ..] => {}
                        [scope @ ("local" | "global" | "agent"), res, dir @ ("in" | "out"), ..] => {
                            let amount = num(file, l, 3)? * scale;
                            let transport = match *scope {
                                "global" => Transport::City,
                                "agent" => Transport::Agents,
                                _ => Transport::Local,
                            };
                            declare_resource(rules, res, transport, network.as_deref());
                            let unit = &mut rules.units[ui];
                            if *dir == "in" {
                                *unit.needs.entry(res.to_string()).or_default() += amount;
                            } else {
                                *unit.produce.entry(res.to_string()).or_default() += amount;
                                if *scope == "agent" && !unit.sends.iter().any(|s| s == res) {
                                    unit.sends.push(res.to_string());
                                }
                            }
                        }
                        // Plain `Money in 10` (Maxis' resource rule): local resource.
                        [res, dir @ ("in" | "out"), _amount] => {
                            let amount = num(file, l, 2)? * scale;
                            declare_resource(rules, res, Transport::Local, None);
                            let unit = &mut rules.units[ui];
                            if *dir == "in" {
                                *unit.needs.entry(res.to_string()).or_default() += amount;
                            } else {
                                *unit.produce.entry(res.to_string()).or_default() += amount;
                            }
                        }
                        ["map", layer, dir @ ("in" | "out"), ..] => {
                            let amount = num(file, l, 3)? * scale;
                            let radius = w.iter().position(|x| *x == "radius").and_then(|i| w.get(i + 1)).and_then(|r| r.parse().ok()).unwrap_or(0);
                            declare_layer(rules, layer);
                            let e = rules.units[ui].emits.entry(layer.to_string()).or_insert(Emission { amount: 0.0, radius });
                            e.amount += if *dir == "out" { amount } else { -amount };
                            e.radius = e.radius.max(radius);
                        }
                        ["map", layer, op, _] | ["test", "map", layer, op, _] if cmp(op).is_some() => {
                            let v = num(file, l, w.len() - 1)?;
                            declare_layer(rules, layer);
                            let b = rules.units[ui].requires.entry(layer.to_string()).or_insert(Bound { min: None, max: None });
                            match cmp(op).unwrap() {
                                Cmp::AtLeast | Cmp::Greater => b.min = Some(v),
                                Cmp::AtMost | Cmp::Less => b.max = Some(v),
                                Cmp::Is => {
                                    b.min = Some(v);
                                    b.max = Some(v);
                                }
                            }
                        }
                        ["log", ..] => {
                            let p = plain(&l.words[1..]);
                            let (Some(metric), Some(n)) = (p.first(), p.get(1).and_then(|n| n.parse::<f32>().ok())) else {
                                return Err(err(l.no, "log needs a metric and an amount".into()));
                            };
                            *rules.units[ui].logs.entry(metric.to_string()).or_default() += n * scale;
                        }
                        ["successEvent" | "onFail", ..] => warnings.push(format!("{file}:{}: `{}` ignored (presentation)", l.no, w[0])),
                        _ => return Err(err(l.no, format!("unknown statement in {}: {}", b.kind, l.words.join(" ")))),
                    }
                }
            }
            "mapRule" => {
                let mut r = MapRule { name: b.name.clone(), every: every as u32, conditions: Vec::new(), inputs: Vec::new(), outputs: Vec::new() };
                for l in &b.lines {
                    let w: Vec<&str> = l.words.iter().map(String::as_str).collect();
                    match w.as_slice() {
                        ["rate", ..] => {}
                        ["map", layer, dir @ ("in" | "out"), _] => {
                            declare_layer(rules, layer);
                            let v = num(file, l, 3)?;
                            if *dir == "in" {
                                r.inputs.push((layer.to_string(), v))
                            } else {
                                r.outputs.push((layer.to_string(), v))
                            }
                        }
                        ["map", layer, op, _] | ["test", "map", layer, op, _] if cmp(op).is_some() => {
                            declare_layer(rules, layer);
                            r.conditions.push(Test { scope: Scope::Map, id: layer.to_string(), cmp: cmp(op).unwrap(), value: num(file, l, w.len() - 1)? });
                        }
                        _ => return Err(err(l.no, format!("unknown statement in mapRule: {}", l.words.join(" ")))),
                    }
                }
                rules.map_rules.push(r);
            }
            "zoneRule" => {
                let mut r = ZoneRule { name: b.name.clone(), zone: applies.clone(), every: every as u32, sample: 1, tests: Vec::new(), create: String::new() };
                for l in &b.lines {
                    let w: Vec<&str> = l.words.iter().map(String::as_str).collect();
                    match w.as_slice() {
                        ["rate" | "appliesTo", ..] => {}
                        ["timeTrigger", _unit, n] => r.every = n.parse::<f32>().map_or(1, |v| v.max(1.0) as u32),
                        ["sample", ..] => r.sample = option(&l.words, "count").and_then(|c| c.parse().ok()).unwrap_or(1),
                        ["test", scope @ ("map" | "global"), id, op, _] if cmp(op).is_some() => {
                            if *scope == "map" {
                                declare_layer(rules, id);
                            }
                            let scope = if *scope == "map" { Scope::Map } else { Scope::Global };
                            r.tests.push(Test { scope, id: id.to_string(), cmp: cmp(op).unwrap(), value: num(file, l, 4)? });
                        }
                        ["createUnit", ..] => r.create = option(&l.words, "id").ok_or_else(|| err(l.no, "createUnit needs -id".into()))?,
                        _ => return Err(err(l.no, format!("unknown statement in zoneRule: {}", l.words.join(" ")))),
                    }
                }
                if r.create.is_empty() {
                    return Err(err(0, format!("zoneRule {}: no createUnit", r.name)));
                }
                rules.zone_rules.push(r);
            }
            "agentRule" => {
                let mut r = AgentRule { name: b.name.clone(), resource: None, every: every as u32, areas: Vec::new() };
                for l in &b.lines {
                    let w: Vec<&str> = l.words.iter().map(String::as_str).collect();
                    match w.as_slice() {
                        ["repeatAfter" | "rate", ..] => {}
                        ["condition", c] => r.resource = c.strip_prefix("resource=").map(str::to_string),
                        ["area", ..] => {
                            let map = option(&l.words, "map").ok_or_else(|| err(l.no, "area needs -map=".into()))?;
                            let p = plain(&l.words[1..]);
                            // `<measure> <amount> <radius>`: the map is `<Map>.<measure>`.
                            let (Some(measure), Some(amount), Some(radius)) =
                                (p.first(), p.get(1).and_then(|v| v.parse::<f32>().ok()), p.get(2).and_then(|v| v.parse::<u32>().ok()))
                            else {
                                return Err(err(l.no, "area needs <measure> <amount> <radius>".into()));
                            };
                            let layer = format!("{map}.{measure}");
                            declare_layer(rules, &layer);
                            r.areas.push(AreaEmission { layer, amount, radius });
                        }
                        _ => return Err(err(l.no, format!("unknown statement in agentRule: {}", l.words.join(" ")))),
                    }
                }
                rules.agent_rules.push(r);
            }
            other => return Err(err(0, format!("unknown rule kind {other}"))),
        }
    }
    Ok(warnings)
}

/// `RoadNetwork`, `Road`, `road_network` -> `road`.
fn network_id(name: &str) -> String {
    let n = name.to_lowercase().replace(['_', '-'], "");
    n.strip_suffix("network").filter(|s| !s.is_empty()).unwrap_or(&n).to_string()
}

fn declare_resource(rules: &mut Ruleset, id: &str, transport: Transport, network: Option<&str>) {
    let network = match transport {
        Transport::Agents => Some(network.map(network_id).unwrap_or_else(|| "road".into())),
        _ => None,
    };
    if let Some(i) = rules.resource(id) {
        // A resource first seen as local becomes agent- or treasury-borne when a rule moves it so.
        let r = &mut rules.resources[i];
        if r.transport == Transport::Local && transport != Transport::Local {
            r.transport = transport;
            r.network = network;
        }
        return;
    }
    rules.resources.push(ResourceDef { id: id.to_string(), name: id.to_string(), transport, network, max_trip: 40 });
}

fn declare_layer(rules: &mut Ruleset, id: &str) {
    if rules.layer(id).is_some() {
        return;
    }
    rules.layers.push(LayerDef { id: id.to_string(), name: id.to_string(), diffusion: 0.1, decay: 0.1, colour: [0.8, 0.5, 0.9], max: 50.0 });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Ruleset {
        serde_json::from_value(serde_json::json!({
            "id": "t", "name": "t",
            "network": [{ "id": "road", "name": "Roads", "structures": ["road"] }],
            "unit": [
                { "id": "mustardFactory", "name": "Mustard factory", "graphic": 31 },
                { "id": "house_1", "name": "House", "graphic": 11 }
            ],
            "zone": [{ "id": "residential", "name": "R", "structure": "residential",
                       "densities": [{ "id": "low", "name": "Low", "units": ["house_1"] }] }]
        }))
        .unwrap()
    }

    #[test]
    fn compiles_the_published_examples() {
        let mut r = base();
        let script = r#"
            # Maxis, GDC 2012
            unitRule mustardFactory
                rate 10
                global Simoleans in 1
                local YellowMustard in 6
                local EmptyBottle in 1
                local BottleOfMustard out 1
                map Pollution out 5
                successEvent effect smokePuff
                onFail buyMoreMustard
            end
            mapRule growGrass
                rate 200
                map Soil atLeast 20
                map water in 10
                map Nutrients in 1
                map Grass out 5
            end
            zoneRule developHouses
                appliesTo residential
                timeTrigger Day 0.5
                sample random -count 3
                test global Builders greater 5
                test map Forest is 0
                createUnit -id house_1
            end
            unitRule goToWork
                appliesTo house_1
                options -sendTo Work -via Car -using Road
                local People in 2
                agent People out 2
            end
            # Costagliola, Montanari, Bellavista: Extended GlassBox
            unitRule spireTrace
                appliesTo house_1 via Car using RoadNetwork
                local Car in 1
                agent Car out 1
                log -timestamp=Time Vehicles 1
            end
            agentRule carPollution
                condition resource=Car
                repeatAfter 10
                area -timestamp=Time -map=AirQuality pm10 10 5
                area -timestamp=Time -map=NoiseMap noise 8 10
            end
        "#;
        let warnings = compile(&mut r, "t.gbx", script).unwrap();
        assert_eq!(warnings.len(), 2);
        let f = &r.units[r.unit("mustardFactory").unwrap()];
        assert_eq!(f.needs["YellowMustard"], 0.6);
        assert_eq!(f.produce["BottleOfMustard"], 0.1);
        assert_eq!(f.emits["Pollution"].amount, 0.5);
        assert_eq!(r.resources[r.resource("Simoleans").unwrap()].transport, Transport::City);
        let h = &r.units[r.unit("house_1").unwrap()];
        assert!(h.sends.contains(&"People".to_string()));
        assert_eq!(h.logs["Vehicles"], 1.0);
        assert_eq!(r.resources[r.resource("People").unwrap()].network.as_deref(), Some("road"));
        assert_eq!(r.map_rules[0].every, 200);
        assert_eq!(r.zone_rules[0].sample, 3);
        assert_eq!(r.agent_rules[0].areas[1].layer, "NoiseMap.noise");
        assert_eq!(r.resources[r.resource("Car").unwrap()].network.as_deref(), Some("road"));
        r.validate().unwrap();
    }

    #[test]
    fn reports_errors_with_lines() {
        let mut r = base();
        let e = compile(&mut r, "x.gbx", "unitRule mustardFactory\n  frobnicate 3\nend\n").unwrap_err();
        assert!(e.message.contains("line 2"), "{e}");
        assert!(compile(&mut r, "x.gbx", "unitRule nobody\n rate 1\nend\n").is_err());
        assert!(compile(&mut r, "x.gbx", "unitRule mustardFactory\n rate 1\n").is_err());
    }
}
