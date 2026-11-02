//! First-run installation of the 0 A.D. culture skins.
//!
//! On the first launch the game downloads the 0 A.D. art it needs from the
//! official repository and converts it ([`opencity_zeroad::bake`]), in a
//! background thread. No Docker, git or Python is needed. The attempt is
//! recorded in `<data>/skins/.bake-0ad-attempted`, so a failure (no network)
//! is reported once instead of on every launch.

use std::path::Path;
use std::sync::mpsc::{self, Receiver};

const MARKER: &str = ".bake-0ad-attempted";

/// Outcome sent back by the background bake.
pub type Outcome = Result<String, String>;

fn already_baked(data_dir: &Path) -> bool {
    data_dir.join("skins").join("generated.toml").is_file()
}

/// Starts the bake if this looks like a first run (or `force`); returns a
/// receiver for its outcome.
pub fn start(data_dir: &Path, force: bool) -> Option<Receiver<Outcome>> {
    if !data_dir.join("zeroad").join("antiquity.toml").is_file() {
        return None;
    }
    let marker = data_dir.join("skins").join(MARKER);
    if !force && (already_baked(data_dir) || marker.exists()) {
        return None;
    }
    let _ = std::fs::write(&marker, "The 0 A.D. art was downloaded automatically on first launch.\nDelete this file (or pass --bake-0ad) to try again.\n");
    let dir = data_dir.to_path_buf();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        log::info!("first run: downloading 0 A.D. art");
        let outcome = match opencity_zeroad::bake(&dir, &opencity_zeroad::Options::default(), &|m| log::info!("bake-0ad: {m}")) {
            Ok(r) => Ok(format!("0 A.D. art installed ({} MB downloaded)", r.downloaded_bytes >> 20)),
            Err(e) => {
                log::warn!("bake-0ad failed: {e}");
                Err(format!("0 A.D. art not installed: {e}"))
            }
        };
        let _ = tx.send(outcome);
    });
    Some(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_without_mapping_and_when_already_attempted() {
        let data = std::env::temp_dir().join(format!("citysim-bake-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(data.join("skins")).unwrap();
        assert!(start(&data, true).is_none(), "no mapping file");
        std::fs::create_dir_all(data.join("zeroad")).unwrap();
        std::fs::write(data.join("zeroad/antiquity.toml"), "").unwrap();
        std::fs::write(data.join("skins/generated.toml"), "skins = []").unwrap();
        assert!(start(&data, false).is_none(), "already baked");
        std::fs::remove_file(data.join("skins/generated.toml")).unwrap();
        std::fs::write(data.join("skins").join(MARKER), "").unwrap();
        assert!(start(&data, false).is_none(), "already attempted");
        let rx = start(&data, true).expect("forced");
        assert!(rx.recv().unwrap().is_err(), "an empty mapping fails cleanly");
    }
}
