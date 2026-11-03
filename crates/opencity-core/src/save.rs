//! Saving and loading cities.
//!
//! The native format is versioned JSON. Legacy OpenCity 0.0.x save files
//! (a text stream of `\0`-separated numbers) can be imported.

use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use crate::{
    layer::{Rect, Status, Structure},
    City, Date, Footprints, GraphicCode, Record, StructureCode, Terrain,
};

pub const FORMAT: &str = "opencity-rs";
pub const VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid save file: {0}")]
    Format(#[from] serde_json::Error),
    #[error("unsupported save format {format:?} version {version}")]
    Unsupported { format: String, version: u32 },
    #[error("invalid legacy save file: {0}")]
    Legacy(String),
}

#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    format: String,
    version: u32,
    city: T,
}

pub fn save(city: &City, out: impl Write) -> Result<(), SaveError> {
    let env = Envelope { format: FORMAT.into(), version: VERSION, city };
    serde_json::to_writer(out, &env)?;
    Ok(())
}

pub fn load(input: impl Read, footprints: Footprints) -> Result<City, SaveError> {
    let env: Envelope<serde_json::Value> = serde_json::from_reader(input)?;
    if env.format != FORMAT || env.version > VERSION {
        return Err(SaveError::Unsupported { format: env.format, version: env.version });
    }
    let mut city: City = serde_json::from_value(env.city)?;
    city.set_footprints(footprints);
    Ok(city)
}

/// Imports a save file written by the C++ OpenCity 0.0.6/0.0.7.
pub fn import_legacy(data: &[u8], footprints: Footprints) -> Result<City, SaveError> {
    let bad = |m: &str| SaveError::Legacy(m.to_string());
    let text = String::from_utf8_lossy(data);
    let (signature, rest) = text.split_once('\n').ok_or_else(|| bad("missing signature line"))?;
    if !signature.starts_with("OpenCity") {
        return Err(bad("not an OpenCity save file"));
    }
    let mut tokens = rest.split(['\0', '\n', ' ']).filter(|t| !t.is_empty());
    let mut next = |what: &str| -> Result<i64, SaveError> {
        tokens
            .next()
            .ok_or_else(|| SaveError::Legacy(format!("truncated before {what}")))?
            .parse::<i64>()
            .map_err(|e| SaveError::Legacy(format!("{what}: {e}")))
    };

    let _version = next("version")?;
    let income = next("income")? as u32;
    let fund = next("fund")?;
    let population = next("population")? as u32;
    let date = Date { day: next("day")? as u32, month: next("month")? as u32, year: next("year")? as u32 };
    let (width, length) = (next("width")? as u32, next("length")? as u32);
    let mut history = Vec::new();
    for _ in 0..20 {
        let r = Record {
            fund: next("record fund")?,
            population: next("record population")? as u32,
            residential: next("record r")? as u32,
            commercial: next("record c")? as u32,
            industrial: next("record i")? as u32,
        };
        for _ in 0..3 {
            next("record w/e/g")?;
        }
        history.push(r);
    }
    let (mw, ml) = (next("map width")? as u32, next("map length")? as u32);
    if (mw, ml) != (width, length) {
        return Err(bad("map size differs from city size"));
    }
    let heights: Vec<i32> = (0..(width + 1) * (length + 1)).map(|_| next("height").map(|h| h as i32)).collect::<Result<_, _>>()?;
    let terrain = Terrain::from_corner_heights(width, length, &heights);
    let (lw, ll) = (next("layer width")? as u32, next("layer length")? as u32);
    if (lw, ll) != (width, length) {
        return Err(bad("layer size differs from city size"));
    }

    let mut city = City::new(terrain, None, footprints, 0);
    for l in 0..length {
        for w in 0..width {
            if next("tile flag")? == 0 {
                continue;
            }
            let ty = next("type")?;
            let graphic = GraphicCode(next("graphic code")? as u16);
            let code_id = next("structure code")? as u16;
            let _ty = next("type")?;
            let status = next("status")? as u8 & 0x7F;
            let level = next("level")?.clamp(0, 99) as u8;
            let mut traffic = 0;
            if ty == crate::StructureType::Path as i64 {
                next("neighbours")?;
                traffic = next("traffic")? as u8;
            }
            let code = StructureCode::from_legacy_id(code_id).ok_or_else(|| SaveError::Legacy(format!("unknown structure code {code_id}")))?;
            let (sw, sl) = city.footprints().size(graphic);
            let rect = Rect::sized(w, l, sw, sl);
            if !city.layer.in_bounds(rect) {
                return Err(bad("structure outside of the map"));
            }
            city.layer.place_block(rect, Structure { code, graphic, level, connections: 0, traffic, density: 0, unit: None, road: Default::default() });
            city.layer.tile_mut(w, l).unwrap().status = Status(status);
        }
    }
    // Simulator values follow but are recomputed below.
    city.layer.rebuild_connections();
    city.income = income;
    city.fund = fund;
    city.population = population;
    city.date = date;
    city.history = history.into();
    city.values = crate::sim::refresh_values(&city.layer);
    Ok(city)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;

    #[test]
    fn json_round_trip() {
        let mut c = City::new(Terrain::flat(10, 10), None, Footprints::builtin(), 3);
        c.apply_tool(Tool::Road, (0, 0), (5, 0)).unwrap();
        c.apply_tool(Tool::FireDept, (1, 2), (1, 2)).unwrap();
        let mut buf = Vec::new();
        save(&c, &mut buf).unwrap();
        let d = load(buf.as_slice(), Footprints::builtin()).unwrap();
        assert_eq!(c.layer, d.layer);
        assert_eq!(c.fund, d.fund);
    }

    #[test]
    fn imports_legacy_stream() {
        // A 2x1 city: a road tile and a level-3 residential zone.
        let mut v: Vec<i64> = vec![70]; // version
        v.extend([5, 14000, 3, 4, 2, 1, 2, 1]); // income fund pop day month year w l
        v.extend([0; 20 * 8]); // resource history
        v.extend([2, 1]); // map size
        v.extend([0; 6]); // 3x2 corner heights
        v.extend([2, 1]); // layer size
        v.extend([1, 8, 150, 20, 8, 0, 0, 0, 0]); // road: type gc sc type status level nbr traffic
        v.extend([1, 1, 10, 10, 1, 0x11, 3]); // zone: status R|E, level 3
        v.extend([0; 10]); // simulators
        let body: String = v.iter().map(|n| format!("{n}\0")).collect();
        let c = import_legacy(format!("OpenCity_0.0.7\n{body}").as_bytes(), Footprints::builtin()).unwrap();
        assert_eq!(c.fund, 14000);
        assert_eq!(c.date, Date { day: 4, month: 2, year: 1 });
        assert_eq!(c.layer.code_at(0, 0), Some(StructureCode::Road));
        assert_eq!(c.layer.structure_at(1, 0).unwrap().level, 3);
        assert_eq!(c.values.residential, 3);
    }
}
