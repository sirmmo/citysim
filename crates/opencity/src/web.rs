//! The browser build (`wasm32`). The data files arrive packed in one
//! download (`assets.pack`, written by `cargo run -p xtask -- web-dist`),
//! saves live in the browser's local storage, exports become downloads,
//! region servers are reached with `fetch` and the page's query string
//! stands in for the command line (`?demo&game-rules=glassbox`). The
//! JavaScript side is `web/citysim.js`.

use std::collections::HashMap;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use macroquad::prelude::*;
use sapp_jsutils::JsObject;

extern "C" {
    fn citysim_storage_get(key: JsObject) -> JsObject;
    fn citysim_storage_set(key: JsObject, value: JsObject) -> i32;
    fn citysim_storage_keys() -> JsObject;
    fn citysim_http_start(method: JsObject, url: JsObject, body: JsObject) -> u32;
    fn citysim_http_poll(id: u32) -> JsObject;
    fn citysim_query() -> JsObject;
    fn citysim_download(name: JsObject, text: JsObject);
}

/// Checked by the JS bundle against the plugin's `version`.
#[no_mangle]
pub extern "C" fn citysim_crate_version() -> u32 {
    2
}

/// The unpacked data files, by path relative to the data directory.
static FILES: Mutex<Option<HashMap<String, Vec<u8>>>> = Mutex::new(None);

/// `./assets/models/composed/../x.ac` → `models/x.ac`.
fn key(path: &Path) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let s = path.to_string_lossy().replace('\\', "/");
    for part in s.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    if parts.first() == Some(&"assets") {
        parts.remove(0);
    }
    parts.join("/")
}

/// The file reader installed once the pack is unpacked.
fn read(path: &Path) -> io::Result<Vec<u8>> {
    let files = FILES.lock().unwrap_or_else(|p| p.into_inner());
    files
        .as_ref()
        .and_then(|f| f.get(&key(path)))
        .cloned()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{} is not in the web pack", path.display())))
}

/// `assets.pack`: gzip of a text index (`CSPK1`, then `<size> <path>` lines,
/// then a blank line) followed by the files' bytes in index order.
fn unpack(gz: &[u8]) -> Result<HashMap<String, Vec<u8>>> {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(gz).read_to_end(&mut raw).context("decompressing")?;
    let head_end = raw.windows(2).position(|w| w == b"\n\n").context("no index")?;
    let index = std::str::from_utf8(&raw[..head_end]).context("index is not UTF-8")?;
    let mut lines = index.lines();
    if lines.next() != Some("CSPK1") {
        bail!("not a citysim pack");
    }
    let mut at = head_end + 2;
    let mut files = HashMap::new();
    for line in lines {
        let (len, path) = line.split_once(' ').context("bad index line")?;
        let len: usize = len.parse().context("bad size")?;
        files.insert(path.to_string(), raw.get(at..at + len).context("pack is truncated")?.to_vec());
        at += len;
    }
    Ok(files)
}

fn splash(what: &str, t0: f64) {
    clear_background(Color::from_rgba(16, 20, 28, 255));
    let dots = ".".repeat(((get_time() - t0) * 2.0) as usize % 4);
    let (w, h) = (screen_width(), screen_height());
    let title = measure_text("citysim", None, 48, 1.0);
    draw_text("citysim", (w - title.width) / 2.0, h / 2.0 - 20.0, 48.0, WHITE);
    let line = format!("{what}{dots}");
    let m = measure_text(what, None, 22, 1.0);
    draw_text(&line, (w - m.width) / 2.0, h / 2.0 + 24.0, 22.0, LIGHTGRAY);
}

/// Downloads and unpacks the data files, drawing a splash meanwhile, and
/// installs the reader that serves them.
pub async fn preload() -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    macroquad::miniquad::fs::load_file("assets.pack", move |r| {
        let _ = tx.send(r.map_err(|e| e.to_string()));
    });
    let t0 = get_time();
    let packed = loop {
        match rx.try_recv() {
            Ok(r) => break r.map_err(|e| anyhow::anyhow!("downloading assets.pack: {e}"))?,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                splash("Downloading the game data", t0);
                next_frame().await;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => bail!("the assets.pack download was dropped"),
        }
    };
    splash("Unpacking", t0);
    next_frame().await;
    let files = unpack(&packed).context("assets.pack")?;
    log::info!("web pack: {} files, {} MB", files.len(), files.values().map(Vec::len).sum::<usize>() >> 20);
    *FILES.lock().unwrap_or_else(|p| p.into_inner()) = Some(files);
    opencity_assets::set_file_reader(read);
    Ok(())
}

fn js_string(v: JsObject) -> Option<String> {
    if v.is_nil() || v.is_undefined() {
        return None;
    }
    let mut s = String::new();
    v.to_string(&mut s);
    Some(s)
}

/// A value kept in the browser's local storage.
pub fn storage_read(key: &str) -> io::Result<String> {
    let v = unsafe { citysim_storage_get(JsObject::string(key)) };
    js_string(v).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("nothing is saved as {key} in this browser")))
}

pub fn storage_write(key: &str, value: &str) -> io::Result<()> {
    match unsafe { citysim_storage_set(JsObject::string(key), JsObject::string(value)) } {
        1 => Ok(()),
        _ => Err(io::Error::other("the browser refused to store it (storage full?)")),
    }
}

/// Every key in local storage.
pub fn storage_keys() -> Vec<String> {
    js_string(unsafe { citysim_storage_keys() }).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Offers a text file for download.
pub fn download(name: &str, text: &str) {
    unsafe { citysim_download(JsObject::string(name), JsObject::string(text)) }
}

/// An HTTP request in flight; the browser answers it between frames.
#[derive(Debug)]
pub struct Request(u32);

/// Starts a request; an empty body sends none.
pub fn http_start(method: &str, url: &str, body: &str) -> Request {
    Request(unsafe { citysim_http_start(JsObject::string(method), JsObject::string(url), JsObject::string(body)) })
}

/// The answer, once it is there: the status code and the body, or the
/// reason the request never got through.
pub fn http_poll(r: &Request) -> Option<Result<(u16, String)>> {
    let text = js_string(unsafe { citysim_http_poll(r.0) })?;
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return Some(Err(anyhow::anyhow!("bad reply from the page: {e}"))),
    };
    let status = v["status"].as_u64().unwrap_or(0) as u16;
    let body = v["body"].as_str().unwrap_or_default().to_string();
    Some(if status == 0 { Err(anyhow::anyhow!("{body}")) } else { Ok((status, body)) })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(b) => {
                    out.push(b);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The command line, from the page's query string: `?demo&seed=7` is
/// `opencity --demo --seed 7`.
pub fn args() -> Vec<String> {
    let query = js_string(unsafe { citysim_query() }).unwrap_or_default();
    let mut out = vec!["opencity".to_string()];
    for part in query.trim_start_matches('?').split('&').filter(|p| !p.is_empty()) {
        let (k, v) = match part.split_once('=') {
            Some((k, v)) => (k, Some(v)),
            None => (part, None),
        };
        out.push(format!("--{}", percent_decode(k)));
        if let Some(v) = v {
            out.push(percent_decode(v));
        }
    }
    out
}

struct ConsoleLog;

impl log::Log for ConsoleLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        match r.level() {
            log::Level::Error => macroquad::miniquad::error!("{}", r.args()),
            log::Level::Warn => macroquad::miniquad::warn!("{}", r.args()),
            _ => macroquad::miniquad::info!("{}", r.args()),
        }
    }
    fn flush(&self) {}
}

/// Sends `log` output to the browser console.
pub fn init_log() {
    let _ = log::set_logger(&ConsoleLog);
    log::set_max_level(log::LevelFilter::Info);
    std::panic::set_hook(Box::new(|info| macroquad::miniquad::error!("{info}")));
}

/// Random bytes for `rand`: the browser build has no OS source, so a
/// splitmix64 stream seeded from the clock stands in. The game itself
/// seeds its generators explicitly.
static RANDOM: Mutex<u64> = Mutex::new(0);

fn web_random(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    let mut state = RANDOM.lock().unwrap_or_else(|p| p.into_inner());
    if *state == 0 {
        *state = (macroquad::miniquad::date::now() * 1e6) as u64 ^ 0x9E37_79B9_7F4A_7C15;
    }
    for chunk in buf.chunks_mut(8) {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        chunk.copy_from_slice(&z.to_le_bytes()[..chunk.len()]);
    }
    Ok(())
}

getrandom::register_custom_getrandom!(web_random);

#[cfg(test)]
mod tests {
    #[test]
    fn query_strings_become_arguments() {
        assert_eq!(super::percent_decode("a%20b+c%zz"), "a b c%zz");
    }

    #[test]
    fn pack_keys_are_relative_to_the_data_dir() {
        assert_eq!(super::key(std::path::Path::new("./assets/models/composed/../kenney/a.obj")), "models/kenney/a.obj");
        assert_eq!(super::key(std::path::Path::new("assets/graphism.conf")), "graphism.conf");
    }
}
