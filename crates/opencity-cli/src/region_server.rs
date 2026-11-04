//! A region server: several players, each with a town of one region.
//!
//! A tiny HTTP/1.1 service on the standard library, keeping the region
//! file (`region.json`, see `opencity_core::region`) in a folder. Players
//! claim a slot, then send their town's summary every month and read the
//! others'; trade and shared culture are worked out by each game from the
//! summaries, exactly as in a region played alone.
//!
//! * `GET /region`: the region, every town's summary and player.
//! * `POST /claim` `{"slot": [x, y], "player": "name"}`: claims a free slot
//!   (or the player's own again) and returns `{"token": …}`.
//! * `PUT /city` `{"token": …, "summary": {…}}`: the town's latest summary.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use opencity_core::region::{Region, RegionCity};
use serde_json::{json, Value};

pub struct State {
    dir: PathBuf,
    region: Region,
    /// Token -> slot.
    tokens: BTreeMap<String, [u32; 2]>,
}

impl State {
    fn save(&self) -> Result<()> {
        std::fs::write(self.dir.join("region.json"), self.region.to_json())?;
        std::fs::write(self.dir.join("tokens.json"), serde_json::to_string_pretty(&self.tokens)?)?;
        Ok(())
    }
}

/// Opens (or creates) the region in `dir`.
pub fn open(dir: &Path, create: impl FnOnce() -> Region) -> Result<Arc<Mutex<State>>> {
    std::fs::create_dir_all(dir)?;
    let region = match std::fs::read_to_string(dir.join("region.json")) {
        Ok(text) => Region::from_json(&text).context("parsing region.json")?,
        Err(_) => create(),
    };
    let tokens = std::fs::read_to_string(dir.join("tokens.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let state = State { dir: dir.to_owned(), region, tokens };
    state.save()?;
    Ok(Arc::new(Mutex::new(state)))
}

/// Serves until the process ends.
pub fn serve(listener: TcpListener, state: Arc<Mutex<State>>) -> Result<()> {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let state = state.clone();
        std::thread::spawn(move || {
            if let Err(e) = handle(stream, &state) {
                log::warn!("region server: {e:#}");
            }
        });
    }
    Ok(())
}

fn respond(mut s: &TcpStream, code: u16, body: &Value) -> Result<()> {
    let text = body.to_string();
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Error",
    };
    write!(
        s,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{text}",
        text.len()
    )?;
    Ok(())
}

fn handle(stream: TcpStream, state: &Mutex<State>) -> Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
    let mut length = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0; length.min(1 << 20)];
    reader.read_exact(&mut body)?;
    let body: Value = if body.is_empty() { Value::Null } else { serde_json::from_slice(&body).unwrap_or(Value::Null) };
    let mut st = state.lock().unwrap();
    match (method.as_str(), path.as_str()) {
        ("GET", "/region") => respond(&stream, 200, &serde_json::to_value(&st.region)?),
        ("GET", "/health") => respond(&stream, 200, &json!({ "ok": true })),
        ("POST", "/claim") => {
            let (Some(slot), Some(player)) =
                (body.get("slot").and_then(|s| serde_json::from_value::<[u32; 2]>(s.clone()).ok()), body.get("player").and_then(Value::as_str))
            else {
                return respond(&stream, 400, &json!({ "error": "needs slot [x, y] and player" }));
            };
            if slot[0] >= st.region.size[0] || slot[1] >= st.region.size[1] {
                return respond(&stream, 400, &json!({ "error": "no such slot" }));
            }
            if let Some(c) = st.region.city(slot) {
                if c.player.as_deref().is_some_and(|p| p != player) {
                    return respond(&stream, 409, &json!({ "error": format!("{} plays that town", c.player.clone().unwrap_or_default()) }));
                }
            }
            let token = format!("{:016x}{:016x}", rand_u64(), rand_u64());
            st.tokens.retain(|_, s| *s != slot);
            st.tokens.insert(token.clone(), slot);
            if st.region.city(slot).is_none() {
                // Unnamed until the player's game names the town.
                st.region.cities.push(RegionCity { slot, ..Default::default() });
            }
            if let Some(c) = st.region.cities.iter_mut().find(|c| c.slot == slot) {
                c.player = Some(player.to_string());
            }
            st.save()?;
            respond(&stream, 200, &json!({ "token": token, "slot": slot, "region": st.region }))
        }
        ("PUT", "/city") => {
            let Some(slot) = body.get("token").and_then(Value::as_str).and_then(|t| st.tokens.get(t).copied()) else {
                return respond(&stream, 403, &json!({ "error": "unknown token: claim a slot first" }));
            };
            let Some(mut summary) = body.get("summary").and_then(|s| serde_json::from_value::<RegionCity>(s.clone()).ok()) else {
                return respond(&stream, 400, &json!({ "error": "needs a summary" }));
            };
            summary.slot = slot;
            summary.player = st.region.city(slot).and_then(|c| c.player.clone());
            match st.region.cities.iter_mut().find(|c| c.slot == slot) {
                Some(c) => *c = summary,
                None => st.region.cities.push(summary),
            }
            st.save()?;
            respond(&stream, 200, &json!({ "ok": true }))
        }
        _ => respond(&stream, 404, &json!({ "error": "unknown route" })),
    }
}

/// Unpredictable enough for tokens among friends (not cryptographic).
fn rand_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(addr: &str, method: &str, path: &str, body: &Value) -> (u16, Value) {
        let mut s = TcpStream::connect(addr).unwrap();
        let text = body.to_string();
        write!(s, "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{text}", text.len()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = out.split("\r\n\r\n").nth(1).unwrap_or("null");
        (code, serde_json::from_str(body).unwrap())
    }

    #[test]
    fn players_claim_towns_and_share_summaries() {
        let dir = std::env::temp_dir().join(format!("citysim-region-{}", rand_u64()));
        let state = open(&dir, || Region::new("Net", [2, 1], 32, 3, Some("glassbox".into()))).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        std::thread::spawn(move || serve(listener, state));
        let (code, r) = call(&addr, "POST", "/claim", &json!({ "slot": [0, 0], "player": "ada" }));
        assert_eq!(code, 200, "{r}");
        let token = r["token"].as_str().unwrap().to_string();
        // Someone else cannot take Ada's town; the next one is free.
        assert_eq!(call(&addr, "POST", "/claim", &json!({ "slot": [0, 0], "player": "bob" })).0, 409);
        assert_eq!(call(&addr, "POST", "/claim", &json!({ "slot": [1, 0], "player": "bob" })).0, 200);
        let summary = json!({ "slot": [9, 9], "name": "Adaville", "file": "", "population": 1200, "culture": { "ideas": 4.0 } });
        assert_eq!(call(&addr, "PUT", "/city", &json!({ "token": token, "summary": summary })).0, 200);
        assert_eq!(call(&addr, "PUT", "/city", &json!({ "token": "nope", "summary": summary })).0, 403);
        let (_, region) = call(&addr, "GET", "/region", &Value::Null);
        let region: Region = serde_json::from_value(region).unwrap();
        let ada = region.city([0, 0]).unwrap();
        assert_eq!((ada.name.as_str(), ada.population, ada.player.as_deref()), ("Adaville", 1200, Some("ada")));
        // Bob's town takes in a share of Ada's ideas.
        assert!((region.culture_for([1, 0])["ideas"] - 0.4).abs() < 1e-6);
        assert!(dir.join("region.json").is_file() && dir.join("tokens.json").is_file());
        let _ = std::fs::remove_dir_all(dir);
    }
}
