//! First-run installation of the 0 A.D. culture skins.
//!
//! From a source checkout, the first launch runs `scripts/bake-0ad.sh` in the
//! background. It needs Docker, and converts art from the original 0 A.D.
//! repository. The attempt is
//! recorded in `<data>/skins/.bake-0ad-attempted`, so a missing prerequisite is
//! reported once instead of on every launch. Release packages ship the script
//! (and `download-0ad-art.*` launchers) too; without Docker the attempt fails
//! once with a message.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};

const MARKER: &str = ".bake-0ad-attempted";

/// Outcome sent back by the background bake.
pub type Outcome = Result<String, String>;

fn script(data_dir: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "bake-0ad.cmd" } else { "bake-0ad.sh" };
    let s = data_dir.join("..").join("scripts").join(name);
    s.is_file().then_some(s)
}

fn command(script: &Path) -> Command {
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(script);
        c
    } else {
        let mut c = Command::new("bash");
        c.arg(script);
        c
    }
}

fn already_baked(data_dir: &Path) -> bool {
    data_dir.join("skins").join("generated.toml").is_file()
}

/// Starts the bake if this looks like a first run (or `force`); returns a
/// receiver for its outcome.
pub fn start(data_dir: &Path, force: bool) -> Option<Receiver<Outcome>> {
    let script = script(data_dir)?;
    let marker = data_dir.join("skins").join(MARKER);
    if !force && (already_baked(data_dir) || marker.exists()) {
        return None;
    }
    let _ = std::fs::write(&marker, "scripts/bake-0ad.sh was run automatically on first launch.\nDelete this file (or pass --bake-0ad) to try again.\n");
    let root = data_dir.join("..");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        log::info!("first run: installing 0 A.D. art with {}", script.display());
        let result = command(&script).current_dir(&root).output();
        let outcome = match result {
            Ok(out) if out.status.success() => {
                log::info!("bake-0ad finished:\n{}", String::from_utf8_lossy(&out.stdout));
                Ok("0 A.D. art installed".to_string())
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr);
                let reason = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("unknown error").to_string();
                log::warn!("bake-0ad failed:\n{err}");
                Err(format!("0 A.D. art not installed: {reason}"))
            }
            Err(e) => Err(format!("0 A.D. art not installed: cannot run {} ({e})", script.display())),
        };
        let _ = tx.send(outcome);
    });
    Some(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkout(script_body: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("citysim-bake-{}-{}", std::process::id(), script_body.len()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("assets/skins")).unwrap();
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(root.join("scripts/bake-0ad.sh"), script_body).unwrap();
        root.join("assets")
    }

    #[test]
    fn runs_once_and_reports() {
        let data = checkout("echo 'skins = []' > assets/skins/generated.toml\n");
        let rx = start(&data, false).expect("first run starts the bake");
        assert_eq!(rx.recv().unwrap(), Ok("0 A.D. art installed".to_string()));
        assert!(data.join("skins/generated.toml").is_file());
        assert!(start(&data, false).is_none(), "not again once baked");
        assert!(start(&data, true).is_some(), "--bake-0ad forces it");
    }

    #[test]
    fn failure_is_reported_once() {
        let data = checkout("echo 'no manifest.json in /art' >&2\nexit 1\n");
        let rx = start(&data, false).unwrap();
        assert_eq!(rx.recv().unwrap(), Err("0 A.D. art not installed: no manifest.json in /art".to_string()));
        assert!(start(&data, false).is_none(), "the marker stops retries");
    }

    #[test]
    fn packaged_builds_skip() {
        let data = std::env::temp_dir().join("citysim-bake-none/assets");
        std::fs::create_dir_all(&data).unwrap();
        assert!(start(&data, true).is_none());
    }
}
