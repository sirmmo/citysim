//! Holdings: a city run for another game. OpenTactics hands a city it owns
//! to a friend with a link (`…/citysim/play/#holding=<packed JSON>`); the
//! friend builds it here, at most `day × pace` months of city time, and
//! answers with a report link (`<return>#report=<packed JSON>`) that sets
//! what the city pays in OpenTactics. The contract is OpenTactics'
//! `HOLDINGS.md` (version 1).
//!
//! Both links pack JSON the same way: UTF-8, compressed with raw deflate,
//! base64url without padding. A link made without compression says
//! `holdingjson=` / `reportjson=` and is only base64url.

use std::io::{Read, Write};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

/// Months of city time per OpenTactics day, unless the link says otherwise.
pub const DEFAULT_PACE: u32 = 3;
/// The monthly tax revenue that adds 100 to `income_pct`.
pub const TAX_FOR_FULL_BONUS: f64 = 400.0;

/// What OpenTactics asks for (the holding link).
#[derive(Clone, Debug, PartialEq)]
pub struct Holding {
    pub kind: String,
    pub game: String,
    pub map: String,
    pub tile: [i64; 2],
    pub name: String,
    pub owner: i64,
    pub army: String,
    pub day: u32,
    pub pace: u32,
    /// The 5×5 neighbourhood of the tile, north row first.
    pub around: Vec<String>,
    /// Where the report goes.
    pub return_url: String,
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url without padding.
pub fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// base64url, with or without padding (plain base64's `+` and `/` too).
pub fn base64url_decode(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.trim().trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => bail!("{:?} is not base64url", c as char),
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

/// JSON packed for a link: raw deflate, then base64url.
pub fn pack(json: &str) -> String {
    let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    // Writing to memory cannot fail.
    let _ = z.write_all(json.as_bytes());
    base64url_encode(&z.finish().unwrap_or_default())
}

/// The JSON a link carries as `#<name>=…` (packed) or `#<name>json=…`.
/// Takes a whole address, its fragment (with or without `#`) or the JSON
/// itself.
pub fn unpack(text: &str, name: &str) -> Result<Value> {
    let text = text.trim();
    if text.starts_with('{') {
        return serde_json::from_str(text).context("bad JSON");
    }
    let fragment = text.split_once('#').map_or(text, |(_, f)| f);
    for part in fragment.split('&') {
        let Some((k, v)) = part.split_once('=') else { continue };
        let json = if k == name {
            let mut s = String::new();
            flate2::read::DeflateDecoder::new(&base64url_decode(v)?[..]).read_to_string(&mut s).context("cannot decompress the link")?;
            s
        } else if k == format!("{name}json") {
            String::from_utf8(base64url_decode(v)?).context("the link is not UTF-8")?
        } else {
            continue;
        };
        return serde_json::from_str(&json).context("the link's JSON is broken");
    }
    bail!("no #{name}= in the link")
}

impl Holding {
    /// Reads a holding link (see [`unpack`]).
    pub fn from_link(text: &str) -> Result<Self> {
        let v = unpack(text, "holding")?;
        if v["v"].as_u64().unwrap_or(1) > 1 {
            log::warn!("holding link version {} is newer than 1; reading what is known", v["v"]);
        }
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        let kind = s("kind");
        if !kind.is_empty() && kind != "city" {
            bail!("this link is for a {kind}, not a city: open it in OpenFactor");
        }
        let game = s("game");
        let tile = match v["tile"].as_array().map(|t| t.iter().map(Value::as_i64).collect::<Vec<_>>()) {
            Some(t) if t.len() == 2 && t.iter().all(Option::is_some) => [t[0].unwrap_or(0), t[1].unwrap_or(0)],
            _ => bail!("the link has no tile"),
        };
        if game.is_empty() {
            bail!("the link names no game");
        }
        let name = Some(s("name")).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("{} {},{}", s("map"), tile[0], tile[1]));
        Ok(Self {
            kind: "city".into(),
            game,
            map: s("map"),
            tile,
            name,
            owner: v["owner"].as_i64().unwrap_or(0),
            army: s("army"),
            day: v["day"].as_u64().unwrap_or(1) as u32,
            pace: v["pace"].as_u64().filter(|p| *p > 0).map_or(DEFAULT_PACE, |p| p as u32),
            around: v["around"].as_array().map(|r| r.iter().filter_map(|r| r.as_str().map(str::to_string)).collect()).unwrap_or_default(),
            return_url: Some(s("return")).filter(|r| !r.is_empty()).unwrap_or_else(|| "https://ingmmo.com/opentactics/".into()),
        })
    }

    /// What identifies the holding: the game and the tile.
    pub fn key(&self) -> String {
        format!("{}@{},{}", self.game, self.tile[0], self.tile[1])
    }

    /// The save file (in the browser, the storage key) of the holding.
    pub fn save_name(&self) -> String {
        let game: String = self.game.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(40).collect();
        format!("holding-{game}-{}-{}.ocsave.json", self.tile[0], self.tile[1])
    }

    /// The map seed: the same land for the same holding.
    pub fn seed(&self) -> u64 {
        // FNV-1a.
        self.key().bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
    }
}

/// Months of city time allowed by the OpenTactics day.
pub fn months_allowed(day: u32, pace: u32) -> u32 {
    day.saturating_mul(pace)
}

/// Whether the city has used all the time it is allowed.
pub fn at_limit(months_run: u32, day: u32, pace: u32) -> bool {
    months_run >= months_allowed(day, pace)
}

/// Percent of the property's base income: 50, plus 100 for each $400 of
/// tax revenue a month, rounded down.
pub fn income_pct(taxes_per_month: f64) -> u32 {
    50 + (100.0 * taxes_per_month.max(0.0) / TAX_FOR_FULL_BONUS).floor() as u32
}

/// 4210 → "4,210".
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The line the commander sees.
pub fn summary(population: u32, taxes_per_month: f64) -> String {
    format!("Population {}, tax ${}/month", thousands(population as u64), thousands(taxes_per_month.max(0.0).round() as u64))
}

/// The report (section 2 of the contract).
pub fn report(h: &Holding, name: &str, population: u32, taxes_per_month: f64) -> Value {
    json!({
        "v": 1,
        "kind": "city",
        "game": h.game,
        "tile": h.tile,
        "owner": h.owner,
        "day": h.day,
        "income_pct": income_pct(taxes_per_month),
        "discount_pct": 0,
        "name": name,
        "summary": summary(population, taxes_per_month),
        "stats": { "population": population, "tax_per_month": taxes_per_month.max(0.0).round() as u64 },
    })
}

/// The address that takes the report to OpenTactics.
pub fn report_link(h: &Holding, report: &Value) -> String {
    let base = h.return_url.split('#').next().unwrap_or_default();
    format!("{base}#report={}", pack(&report.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link() -> Value {
        json!({
            "v": 1, "kind": "city", "terrain": "city", "builds": [], "game": "3f9a01c2", "map": "Twin Rivers",
            "tile": [5, 3], "name": "Twin Rivers 5,3", "owner": 0, "army": "Red Star", "color": "#d04040",
            "day": 7, "pace": 3, "around": ["..f~~", ".=c~~", "..=..", "f^...", "....."],
            "return": "https://ingmmo.com/opentactics/"
        })
    }

    #[test]
    fn base64url_round_trips() {
        for len in 0..20 {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37 + 250) as u8).collect();
            let text = base64url_encode(&bytes);
            assert!(!text.contains(['=', '+', '/']), "{text}");
            assert_eq!(base64url_decode(&text).unwrap(), bytes);
        }
        assert_eq!(base64url_encode(b"hello"), "aGVsbG8");
        assert_eq!(base64url_decode("aGVsbG8=").unwrap(), b"hello");
        assert!(base64url_decode("a b").is_err());
    }

    #[test]
    fn holding_links_unpack_packed_or_not() {
        let json = link().to_string();
        let packed = format!("https://ingmmo.com/citysim/play/#holding={}", pack(&json));
        let plain = format!("#holdingjson={}", base64url_encode(json.as_bytes()));
        for text in [packed.as_str(), &packed[packed.find('#').unwrap() + 1..], plain.as_str(), json.as_str()] {
            let h = Holding::from_link(text).unwrap();
            assert_eq!((h.game.as_str(), h.tile, h.day, h.pace), ("3f9a01c2", [5, 3], 7, 3));
            assert_eq!(h.name, "Twin Rivers 5,3");
            assert_eq!(h.around[1], ".=c~~");
            assert_eq!(h.key(), "3f9a01c2@5,3");
            assert_eq!(h.save_name(), "holding-3f9a01c2-5-3.ocsave.json");
        }
        assert!(Holding::from_link("https://ingmmo.com/citysim/play/").is_err());
        assert!(Holding::from_link("#holding=!!!").is_err());
        let mut factory = link();
        factory["kind"] = json!("factory");
        assert!(Holding::from_link(&factory.to_string()).is_err());
    }

    #[test]
    fn missing_fields_take_defaults() {
        let h = Holding::from_link(r#"{"game": "g", "map": "M", "tile": [1, 2]}"#).unwrap();
        assert_eq!((h.day, h.pace, h.name.as_str()), (1, DEFAULT_PACE, "M 1,2"));
        assert_eq!(h.return_url, "https://ingmmo.com/opentactics/");
    }

    #[test]
    fn income_follows_the_taxes() {
        assert_eq!(income_pct(0.0), 50);
        assert_eq!(income_pct(-20.0), 50);
        assert_eq!(income_pct(380.0), 145);
        assert_eq!(income_pct(399.9), 149);
        assert_eq!(income_pct(400.0), 150);
        assert_eq!(income_pct(1000.0), 300);
    }

    #[test]
    fn pace_limits_the_months() {
        assert_eq!(months_allowed(7, 3), 21);
        assert!(!at_limit(20, 7, 3));
        assert!(at_limit(21, 7, 3));
        assert!(at_limit(0, 0, 3));
        assert!(!at_limit(21, 8, 3), "a newer day allows more");
    }

    #[test]
    fn reports_pack_into_a_link() {
        let h = Holding::from_link(&link().to_string()).unwrap();
        let r = report(&h, "New Bologna", 4210, 380.4);
        assert_eq!(r["summary"], "Population 4,210, tax $380/month");
        assert_eq!(r["income_pct"], 145);
        assert_eq!(r["stats"]["population"], 4210);
        assert_eq!((r["day"].as_u64(), r["owner"].as_i64(), r["discount_pct"].as_u64()), (Some(7), Some(0), Some(0)));
        let url = report_link(&h, &r);
        assert!(url.starts_with("https://ingmmo.com/opentactics/#report="), "{url}");
        assert_eq!(unpack(&url, "report").unwrap(), r);
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
