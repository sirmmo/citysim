//! A local MCP (Model Context Protocol) server on stdin/stdout.
//!
//! AI assistants (Claude Code, Claude Desktop, other MCP clients) start
//! `opencity-cli mcp` and call its tools to build, run and inspect a city:
//! one city lives in the server at a time. Messages are newline-delimited
//! JSON-RPC 2.0; logs go to stderr so stdout carries only protocol.
//!
//! ```sh
//! claude mcp add citysim -- opencity-cli mcp
//! claude mcp add citysim -e OPENCITY_DATA=/path/to/assets -- opencity-cli mcp --load town.ocsave.json
//! ```

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use clap::ValueEnum;
use opencity_core::{city::MS_PER_DAY, City, Status, Tool};
use opencity_mapgen::MapParams;
use serde_json::{json, Value};

use crate::{attach_rules, footprints, load_city, new_city_from, render, ReliefArg, ShapeArg, TreesArg, WaterArg};

const PROTOCOL: &str = "2025-06-18";

struct Server {
    data_dir: Option<PathBuf>,
    city: Option<City>,
}

/// Serves until stdin closes.
pub fn serve(data_dir: Option<PathBuf>, city: Option<City>) -> Result<()> {
    let mut server = Server { data_dir, city };
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => server.handle(&msg),
            Err(e) => Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(r) = reply {
            writeln!(out, "{r}")?;
            out.flush()?;
        }
    }
    Ok(())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

impl Server {
    /// Answers a request; notifications (no id) get no answer.
    fn handle(&mut self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let id = id?;
        let result = match method {
            "initialize" => {
                let version = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL);
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "citysim", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": "citysim: a 3D city simulation. Start with new_city (or load_city), build with apply_tool, \
                        run time with simulate, and read the city with city_summary and inspect_tile. Tiles are (w, l) from the \
                        north-west corner. GlassBox rules (\"glassbox\", \"pre-industrial\") add resources, research, taxes and policies."
                })
            }
            "ping" => json!({}),
            "tools/list" => json!({ "tools": tools() }),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                match self.call(name, &args) {
                    Ok(v) => json!({ "content": [{ "type": "text", "text": text(&v) }], "isError": false }),
                    Err(e) => json!({ "content": [{ "type": "text", "text": format!("{e:#}") }], "isError": true }),
                }
            }
            _ => return Some(error(id, -32601, &format!("unknown method {method}"))),
        };
        Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    fn city(&mut self) -> Result<&mut City> {
        self.city.as_mut().ok_or_else(|| anyhow!("no city yet: call new_city or load_city first"))
    }

    fn call(&mut self, name: &str, a: &Value) -> Result<Value> {
        match name {
            "new_city" if a.get("scenario").is_some() => {
                let id = str_arg(a, "scenario")?;
                let dir = opencity_assets::find_data_dir(self.data_dir.as_deref())?;
                let s = opencity_assets::scenario::load(&dir, id)?;
                self.city = Some(opencity_assets::scenario::build(&dir, &s, footprints(self.data_dir.as_deref()))?);
                Ok(json!({ "scenario": s.name, "description": s.description, "summary": self.summary() }))
            }
            "new_city" => {
                let enum_arg = |key: &str, default: &str| a.get(key).and_then(Value::as_str).unwrap_or(default).to_string();
                let params = MapParams {
                    width: num(a, "width").unwrap_or(64.0) as u32,
                    length: num(a, "length").unwrap_or(64.0) as u32,
                    seed: num(a, "seed").unwrap_or(1.0) as u64,
                    relief: ReliefArg::from_str(&enum_arg("relief", "plain"), true).map_err(|e| anyhow!(e))?.into(),
                    water: WaterArg::from_str(&enum_arg("water", "lake"), true).map_err(|e| anyhow!(e))?.into(),
                    shape: ShapeArg::from_str(&enum_arg("shape", "none"), true).map_err(|e| anyhow!(e))?.into(),
                    trees: TreesArg::from_str(&enum_arg("trees", "sparse"), true).map_err(|e| anyhow!(e))?.into(),
                    heightmap: None,
                };
                if !(8..=512).contains(&params.width) || !(8..=512).contains(&params.length) {
                    bail!("width and length must be 8 to 512 tiles");
                }
                let demo = a.get("demo").and_then(Value::as_bool).unwrap_or(false);
                let rules = a.get("rules").and_then(Value::as_str);
                let fp = footprints(self.data_dir.as_deref());
                let mut city = if a.get("flat").and_then(Value::as_bool).unwrap_or(false) {
                    City::new(opencity_core::Terrain::flat(params.width, params.length), None, fp, params.seed)
                } else {
                    new_city_from(&params, false, fp)?
                };
                attach_rules(&mut city, rules, self.data_dir.as_deref())?;
                if demo {
                    opencity_core::scenario::demo_town(&mut city);
                }
                self.city = Some(city);
                Ok(self.summary())
            }
            "load_city" => {
                let path = PathBuf::from(str_arg(a, "path")?);
                let mut city = load_city(&path, footprints(self.data_dir.as_deref()))?;
                let rules = city.glassbox.as_ref().map(|g| g.rules_id.clone());
                attach_rules(&mut city, rules.as_deref(), self.data_dir.as_deref())?;
                self.city = Some(city);
                Ok(self.summary())
            }
            "save_city" => {
                let path = PathBuf::from(str_arg(a, "path")?);
                let city = self.city()?;
                let f = std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
                opencity_core::save::save(city, std::io::BufWriter::new(f))?;
                Ok(json!({ "saved": path }))
            }
            "city_summary" => {
                self.city()?;
                Ok(self.summary())
            }
            "simulate" => {
                let days = num(a, "days").unwrap_or(0.0) + num(a, "months").unwrap_or(0.0) * 30.0 + num(a, "years").unwrap_or(0.0) * 360.0;
                let days = if days <= 0.0 { 30.0 } else { days.min(360.0 * 50.0) } as u32;
                let city = self.city()?;
                for _ in 0..days {
                    city.update(MS_PER_DAY);
                }
                Ok(json!({ "simulated_days": days, "summary": self.summary() }))
            }
            "apply_tool" => {
                let tool = tool_by_id(str_arg(a, "tool")?)?;
                let from = point(a, "from")?;
                let to = point(a, "to").unwrap_or(from);
                if let Some(d) = num(a, "density") {
                    self.city()?.zone_density = (d as u8).min(2);
                }
                let city = self.city()?;
                if city.rules().is_some_and(|r| !r.allows(tool.id())) {
                    bail!("these rules do not offer the {} tool", tool.id());
                }
                let outcome = city.apply_tool(tool, from, to).map_err(|e| anyhow!("{e}"))?;
                Ok(json!({ "tool": tool.id(), "cost": outcome.cost, "fund": city.fund }))
            }
            "inspect_tile" => {
                let (w, l) = point(a, "tile")?;
                inspect(self.city()?, w, l)
            }
            "turn_building" => {
                let (w, l) = point(a, "tile")?;
                let facing = self.city()?.turn_building(w, l).map_err(|e| anyhow!("{e}"))?;
                Ok(json!({ "facing": facing }))
            }
            "list_tools" => {
                let city = self.city()?;
                Ok(Value::Array(
                    Tool::ALL
                        .iter()
                        .filter(|t| city.rules().is_none_or(|r| r.allows(t.id())) && **t != Tool::TestBuilding)
                        .map(|t| {
                            json!({
                                "id": t.id(),
                                "area": t.is_area(),
                                "locked_by": city.tool_locked_by(*t),
                            })
                        })
                        .collect(),
                ))
            }
            "research" => {
                let city = self.city()?;
                let gb = city.glassbox.as_mut().context("research needs GlassBox rules")?;
                if let Some(t) = a.get("project") {
                    let t = t.as_str().filter(|s| !s.is_empty());
                    if !gb.set_research(t) {
                        bail!("cannot research {}: unknown, done, ruled out, or its requirements are not done", t.unwrap_or(""));
                    }
                }
                let rules = gb.rules().cloned().context("no rules")?;
                Ok(json!({
                    "current": gb.research,
                    "projects": rules.techs.iter().map(|t| json!({
                        "id": t.id, "name": t.name, "category": t.category, "tier": t.tier,
                        "cost": t.cost, "requires": t.requires, "unlocks": t.unlocks, "bonus": t.bonus, "excludes": t.excludes,
                        "state": if gb.researched.contains(&t.id) { "done" } else if gb.research.as_deref() == Some(t.id.as_str()) { "in progress" }
                            else if gb.ruled_out(&rules, &t.id) { "ruled out" } else if gb.can_research(&rules, &t.id) { "available" } else { "locked" },
                        "progress": gb.research_progress(&rules, &t.id),
                    })).collect::<Vec<_>>(),
                }))
            }
            "policies" => {
                let city = self.city()?;
                let gb = city.glassbox.as_mut().context("policies need GlassBox rules")?;
                if let Some(taxes) = a.get("taxes").and_then(Value::as_object) {
                    for (t, r) in taxes {
                        let rate = r.as_f64().with_context(|| format!("tax {t}: rate must be a number"))? as f32;
                        if !gb.set_tax_rate(t, rate) {
                            bail!("unknown tax {t}");
                        }
                    }
                }
                if let Some(stances) = a.get("stances").and_then(Value::as_object) {
                    for (p, s) in stances {
                        let s = s.as_str().with_context(|| format!("policy {p}: stance must be a string"))?;
                        if !gb.set_stance(p, s) {
                            bail!("cannot take stance {s} on {p}: unknown, or it needs research");
                        }
                    }
                }
                let rules = gb.rules().cloned().context("no rules")?;
                Ok(json!({
                    "taxes": rules.taxes.iter().map(|t| json!({
                        "id": t.id, "name": t.name, "zones": t.zones, "rate": gb.tax_rate(&rules, &t.id), "default": t.rate, "range": t.range,
                    })).collect::<Vec<_>>(),
                    "policies": rules.policies.iter().map(|p| json!({
                        "id": p.id, "name": p.name, "stance": gb.stance(&rules, &p.id).map(|s| s.id.clone()),
                        "stances": p.stances.iter().map(|s| json!({
                            "id": s.id, "name": s.name, "description": s.description, "open": gb.stance_open(s), "requires": s.requires,
                            "bonus": s.bonus, "demand": s.demand, "satisfaction": s.satisfaction, "cost": s.cost,
                        })).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                }))
            }
            "disaster" => {
                let city = self.city()?;
                if let Some(on) = a.get("chance").and_then(Value::as_bool) {
                    city.glassbox.as_mut().context("disasters need GlassBox rules")?.disasters_off = !on;
                }
                let strike = a.get("strike").and_then(Value::as_str).map(str::to_string);
                let event = match strike {
                    Some(id) => Some(city.strike(&id).with_context(|| format!("{id}: no such disaster, or nothing it could strike"))?),
                    None => None,
                };
                let rules = city.rules().cloned();
                Ok(json!({
                    "chance": city.glassbox.as_ref().is_some_and(|g| !g.disasters_off),
                    "struck": event.map(|e| json!({ "id": e.id, "name": e.name, "at": e.at, "hit": e.hit })),
                    "disasters": rules.map(|r| r.disasters.iter().map(|d| json!({ "id": d.id, "name": d.name, "every": d.every, "radius": d.radius, "protection": d.protection })).collect::<Vec<_>>()),
                }))
            }
            "budget" => {
                let city = self.city()?;
                if let Some(f) = a.get("funding").and_then(Value::as_object) {
                    let gb = city.glassbox.as_mut().context("funding needs GlassBox rules")?;
                    for (unit, pct) in f {
                        gb.set_funding(unit, pct.as_f64().context("funding is a percentage")? as f32 / 100.0);
                    }
                }
                if let Some(amount) = num(a, "borrow") {
                    city.take_loan(amount as i64).map_err(|e| anyhow!(e))?;
                }
                Ok(json!({
                    "fund": city.fund,
                    "last_month": city.last_ledger,
                    "this_month": city.budget(),
                    "loans": city.loans.iter().map(|l| json!({ "amount": l.amount, "payment": l.payment, "months_left": l.months_left })).collect::<Vec<_>>(),
                    "funding": city.glassbox.as_ref().map(|g| g.funding.clone()),
                }))
            }
            "goals" => {
                let city = self.city()?;
                let g = city.goals.as_ref().context("no scenario: start one with new_city { scenario }")?;
                Ok(json!({
                    "scenario": g.name,
                    "status": format!("{:?}", g.status),
                    "years_played": city.date.year.saturating_sub(g.start_year),
                    "goals": g.goals.iter().enumerate().map(|(i, goal)| json!({
                        "name": goal.name, "metric": goal.metric, "now": city.goal_value(&goal.metric), "target": goal.target(),
                        "within_years": goal.within_years, "always": goal.always, "reached": g.reached.get(i),
                    })).collect::<Vec<_>>(),
                }))
            }
            "cheat" => {
                let code = str_arg(a, "code")?.to_string();
                let city = self.city()?;
                let said = city.cheat(&code).context("nothing happens")?;
                Ok(json!({ "said": said, "fund": city.fund, "mafia": city.mafia.as_ref().map(|m| json!({ "heat": m.heat, "collected": m.collected })) }))
            }
            "render_map" => {
                let path = PathBuf::from(str_arg(a, "path")?);
                let scale = num(a, "scale").unwrap_or(8.0).clamp(1.0, 32.0) as u32;
                render(self.city()?, scale).save(&path)?;
                Ok(json!({ "wrote": path }))
            }
            "export_geojson" => {
                let path = PathBuf::from(str_arg(a, "path")?);
                let json = opencity_geo::export(self.city()?, &opencity_geo::ExportOptions::default());
                std::fs::write(&path, serde_json::to_string(&json)?)?;
                Ok(json!({ "wrote": path, "features": json["features"].as_array().map_or(0, Vec::len) }))
            }
            _ => bail!("unknown tool {name}"),
        }
    }

    /// The state of the city, as the city panel shows it.
    fn summary(&self) -> Value {
        let Some(c) = &self.city else { return json!(null) };
        let v = c.values;
        let mut s = json!({
            "size": [c.width(), c.length()],
            "date": c.date.to_string(),
            "fund": c.fund,
            "population": c.population,
            "zones": { "residential": v.residential, "commercial": v.commercial, "industrial": v.industrial },
            "power": { "used": v.power_capacity - v.power, "capacity": v.power_capacity },
            "vehicles": c.vehicles.len(),
            "rules": c.glassbox.as_ref().map_or("classic".to_string(), |g| g.rules_id.clone()),
        });
        if let (Some(gb), Some(rules)) = (&c.glassbox, c.rules()) {
            let b = &gb.balances;
            s["demand"] = rules.zones.iter().zip(&gb.demand).map(|(z, d)| (z.id.clone(), json!(d))).collect();
            s["resources"] = rules
                .resources
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    (
                        r.id.clone(),
                        json!({
                            "produced": b.produced.get(i), "delivered": b.delivered.get(i), "unmet": b.unmet.get(i),
                            "pool": (gb.pools.get(i).copied().unwrap_or(0.0) > 0.0).then(|| gb.pools[i]),
                        }),
                    )
                })
                .collect();
            s["research"] = json!({ "current": gb.research, "done": gb.researched });
            s["taxes"] = rules.taxes.iter().map(|t| (t.id.clone(), json!(gb.tax_rate(rules, &t.id)))).collect();
            s["stances"] = rules.policies.iter().map(|p| (p.id.clone(), json!(gb.stance(rules, &p.id).map(|x| x.id.clone())))).collect();
        }
        s
    }
}

fn inspect(city: &City, w: u32, l: u32) -> Result<Value> {
    if w >= city.width() || l >= city.length() {
        bail!("tile ({w}, {l}) is outside the {} x {} map", city.width(), city.length());
    }
    let mut out = json!({ "tile": [w, l], "ground_height": city.terrain.tile_min(w, l) });
    let Some(info) = city.query(w, l) else {
        out["empty"] = json!(true);
        return Ok(out);
    };
    let s = city.layer.structure_at(w, l);
    let origin = city.layer.origin_of(w, l).unwrap_or((w, l));
    out["structure"] = json!(format!("{:?}", info.code));
    out["origin"] = json!(origin);
    out["level"] = json!(info.level);
    out["traffic"] = json!(info.traffic);
    out["status"] = json!({
        "residents": info.status.has(Status::RESIDENTIAL), "shops": info.status.has(Status::COMMERCIAL),
        "jobs": info.status.has(Status::INDUSTRIAL), "power": info.status.has(Status::POWER), "road": info.status.has(Status::ROAD),
    });
    if let Some(s) = s {
        out["size"] = json!(s.size(city.footprints()));
        out["facing"] = json!(s.facing);
        if s.code == opencity_core::StructureCode::Road {
            out["road"] = json!(format!("{:?}", s.road));
        }
    }
    if let (Some(gb), Some(rules)) = (&city.glassbox, city.rules()) {
        if let Some(u) = s.and_then(|s| s.unit) {
            let def = &rules.units[u as usize];
            out["unit"] = json!({ "id": def.id, "name": def.name, "produce": def.produce, "needs": def.needs });
            if let Some(st) = gb.units.get(&(origin.1 * city.width() + origin.0)) {
                out["satisfaction"] = json!(st.satisfaction);
            }
        }
        out["layers"] = rules.layers.iter().enumerate().map(|(i, layer)| (layer.id.clone(), json!(gb.layer_value(i, w, l, city.width())))).collect();
    }
    Ok(out)
}

fn text(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn num(a: &Value, key: &str) -> Option<f64> {
    a.get(key).and_then(Value::as_f64)
}

fn str_arg<'a>(a: &'a Value, key: &str) -> Result<&'a str> {
    a.get(key).and_then(Value::as_str).with_context(|| format!("missing string argument `{key}`"))
}

fn point(a: &Value, key: &str) -> Result<(u32, u32)> {
    let p = a.get(key).and_then(Value::as_array).with_context(|| format!("`{key}` must be [w, l]"))?;
    match (p.first().and_then(Value::as_u64), p.get(1).and_then(Value::as_u64)) {
        (Some(w), Some(l)) => Ok((w as u32, l as u32)),
        _ => bail!("`{key}` must be [w, l]"),
    }
}

fn tool_by_id(id: &str) -> Result<Tool> {
    Tool::ALL.iter().copied().find(|t| t.id() == id).ok_or_else(|| {
        let ids: Vec<&str> = Tool::ALL.iter().map(|t| t.id()).collect();
        anyhow!("unknown tool {id}; tools: {}", ids.join(", "))
    })
}

/// The tools the server offers, with their input schemas.
fn tools() -> Value {
    let point = json!({ "type": "array", "items": { "type": "integer", "minimum": 0 }, "minItems": 2, "maxItems": 2 });
    let path = json!({ "type": "string", "description": "File path on this machine" });
    let tool = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({ "name": name, "description": description,
                "inputSchema": { "type": "object", "properties": properties, "required": required } })
    };
    json!([
        tool("new_city", "Generate a new city (replaces the open one). Returns its summary.", json!({
            "width": { "type": "integer", "minimum": 8, "maximum": 512, "default": 64 },
            "length": { "type": "integer", "minimum": 8, "maximum": 512, "default": 64 },
            "seed": { "type": "integer", "default": 1 },
            "relief": { "enum": ["plain", "hill", "mountain"] },
            "water": { "enum": ["dry", "lake", "coast"] },
            "shape": { "enum": ["none", "island", "volcano", "crater"] },
            "trees": { "enum": ["sparse", "normal", "dense"] },
            "rules": { "type": "string", "description": "GlassBox rules id: \"glassbox\" (modern) or \"pre-industrial\"; omit for classic OpenCity rules" },
            "flat": { "type": "boolean", "description": "A flat, empty map with no water or trees (relief, water, shape and trees are ignored)" },
            "demo": { "type": "boolean", "description": "Lay out the demo town (roads, zones, power, services)" },
            "scenario": { "type": "string", "description": "Start a scenario instead (first-town, rail-baron, bologna-1600…): its map, rules and goals" }
        }), &[]),
        tool("load_city", "Open a saved city (.ocsave.json).", json!({ "path": path }), &["path"]),
        tool("save_city", "Save the open city (.ocsave.json).", json!({ "path": path }), &["path"]),
        tool("city_summary", "Date, funds, population, zones, power; under GlassBox rules also demand, resources, research, taxes and stances.", json!({}), &[]),
        tool("simulate", "Run the simulation forward (default 30 days; at most 50 years per call). Returns the new summary.", json!({
            "days": { "type": "integer", "minimum": 0 }, "months": { "type": "integer", "minimum": 0 }, "years": { "type": "integer", "minimum": 0 }
        }), &[]),
        tool("list_tools", "The building tools these rules offer (ids for apply_tool), and the research each locked one waits for.", json!({}), &[]),
        tool("apply_tool", "Use a building tool between two corner tiles: zones and roads span from..to; plants and services are placed at `from` and turn to face the street. Costs money.", json!({
            "tool": { "type": "string", "description": "Tool id from list_tools, e.g. residential, commercial, industrial, road, coal_plant, school, bulldozer" },
            "from": point.clone(), "to": point.clone(),
            "density": { "type": "integer", "minimum": 0, "maximum": 2, "description": "Zone density under GlassBox rules: 0 low, 1 medium, 2 high" }
        }), &["tool", "from"]),
        tool("inspect_tile", "What is on a tile: structure, level, status, size, facing; under GlassBox rules the building, its needs, satisfaction and map layers.", json!({ "tile": point.clone() }), &["tile"]),
        tool("turn_building", "Turn the plant or service on a tile a quarter clockwise.", json!({ "tile": point.clone() }), &["tile"]),
        tool("research", "List research projects with their state; pass `project` to research it (empty string stops).", json!({
            "project": { "type": "string" }
        }), &[]),
        tool("policies", "List taxes and policies; pass `taxes` ({id: rate %}) and/or `stances` ({policy: stance}) to change them.", json!({
            "taxes": { "type": "object", "additionalProperties": { "type": "number" } },
            "stances": { "type": "object", "additionalProperties": { "type": "string" } }
        }), &[]),
        tool("disaster", "List disasters; `strike` sets one off by hand (fire, flood, epidemic…), `chance` switches chance disasters on or off.", json!({
            "strike": { "type": "string" }, "chance": { "type": "boolean" }
        }), &[]),
        tool("budget", "Money by budget line (last month and this month so far), loans and service funding. `funding` ({unit id: percent 0-150}) changes funding; `borrow` takes a loan (6%/year over five years).", json!({
            "funding": { "type": "object", "additionalProperties": { "type": "number" } }, "borrow": { "type": "integer", "minimum": 1 }
        }), &[]),
        tool("goals", "The scenario's goals: how far each has come, deadlines, and whether the game is won or lost.", json!({}), &[]),
        tool("cheat", "Type a cheat code, as in SimCity 3000's cheat box. Some of them have consequences.", json!({ "code": { "type": "string" } }), &["code"]),
        tool("render_map", "Write a top-down PNG of the city.", json!({ "path": path, "scale": { "type": "integer", "minimum": 1, "maximum": 32, "default": 8 } }), &["path"]),
        tool("export_geojson", "Write the city as GeoJSON.", json!({ "path": path }), &["path"])
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(s: &mut Server, id: u64, method: &str, params: Value) -> Value {
        s.handle(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).unwrap()
    }

    fn tool_call(s: &mut Server, name: &str, args: Value) -> (bool, Value) {
        let r = call(s, 9, "tools/call", json!({ "name": name, "arguments": args }));
        let text = r["result"]["content"][0]["text"].as_str().unwrap().to_string();
        (r["result"]["isError"].as_bool().unwrap(), serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    #[test]
    fn builds_and_runs_a_city_over_mcp() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut s = Server { data_dir: Some(data), city: None };
        let init = call(&mut s, 1, "initialize", json!({ "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } }));
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert!(s.handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let list = call(&mut s, 2, "tools/list", json!({}));
        assert!(list["result"]["tools"].as_array().unwrap().len() >= 12);
        // Without a city, tools say so.
        let (err, _) = tool_call(&mut s, "city_summary", json!({}));
        assert!(err);
        let (err, sum) = tool_call(&mut s, "new_city", json!({ "width": 32, "length": 32, "flat": true, "rules": "glassbox" }));
        assert!(!err, "{sum}");
        assert_eq!(sum["rules"], "glassbox");
        let (err, r) = tool_call(&mut s, "apply_tool", json!({ "tool": "road", "from": [2, 10], "to": [28, 10] }));
        assert!(!err, "{r}");
        let (err, r) = tool_call(&mut s, "apply_tool", json!({ "tool": "highway", "from": [2, 20], "to": [28, 20] }));
        assert!(err && r.as_str().unwrap().contains("research"), "{r}");
        let (err, _) = tool_call(&mut s, "apply_tool", json!({ "tool": "residential", "from": [2, 11], "to": [28, 12] }));
        assert!(!err);
        let (err, tile) = tool_call(&mut s, "inspect_tile", json!({ "tile": [3, 10] }));
        assert!(!err && tile["structure"] == "Road", "{tile}");
        let (err, r) = tool_call(&mut s, "policies", json!({ "taxes": { "property": 12 }, "stances": { "growth": "open_city" } }));
        assert!(!err, "{r}");
        let (err, r) = tool_call(&mut s, "research", json!({ "project": "motorways" }));
        assert!(!err && r["current"] == "motorways", "{r}");
        let (err, r) = tool_call(&mut s, "simulate", json!({ "days": 5 }));
        assert!(!err && r["simulated_days"] == 5, "{r}");
        assert_eq!(r["summary"]["taxes"]["property"], 12.0);
        assert_eq!(call(&mut s, 3, "nope", json!({}))["error"]["code"], -32601);
    }
}
