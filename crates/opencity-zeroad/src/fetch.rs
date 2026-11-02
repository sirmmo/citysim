//! Fetching files of the 0 A.D. repository at a pinned commit.
//!
//! Text files come from Gitea's `raw` endpoint and Git LFS files (meshes,
//! textures) from its `media` endpoint, one file at a time, so only what the
//! bake needs is downloaded. Everything is cached on disk; a local checkout
//! can be used instead of the network.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::BakeError;

pub const PUBLIC: &str = "binaries/data/mods/public";

pub enum Source {
    /// `https://gitea.wildfiregames.com/0ad/0ad` + commit, cached in `cache`.
    Remote { base: String, commit: String, cache: PathBuf, agent: ureq::Agent, downloaded: Mutex<u64> },
    /// A checked-out 0 A.D. repository.
    Local(PathBuf),
}

impl Source {
    pub fn remote(repo: &str, commit: &str, cache: &Path) -> Self {
        let base = repo.trim_end_matches(".git").trim_end_matches('/').to_string();
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(20))
            .timeout_read(std::time::Duration::from_secs(120))
            .user_agent("citysim-bake-0ad")
            .build();
        Self::Remote { base, commit: commit.to_string(), cache: cache.join(commit), agent, downloaded: Mutex::new(0) }
    }

    /// Bytes downloaded so far (remote sources).
    pub fn downloaded(&self) -> u64 {
        match self {
            Source::Remote { downloaded, .. } => *downloaded.lock().unwrap(),
            Source::Local(_) => 0,
        }
    }

    /// A text file (template, actor, variant), or `None` if it does not exist.
    pub fn text(&self, rel: &str) -> Result<Option<String>, BakeError> {
        Ok(self.get(rel, false)?.map(|b| String::from_utf8_lossy(&b).into_owned()))
    }

    /// A Git LFS file (mesh, texture), or `None` if it does not exist.
    pub fn binary(&self, rel: &str) -> Result<Option<Vec<u8>>, BakeError> {
        self.get(rel, true)
    }

    fn get(&self, rel: &str, lfs: bool) -> Result<Option<Vec<u8>>, BakeError> {
        let rel = rel.trim_start_matches('/');
        match self {
            Source::Local(root) => match std::fs::read(root.join(rel)) {
                Ok(b) => Ok(Some(b)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(BakeError::Io(root.join(rel), e)),
            },
            Source::Remote { base, commit, cache, agent, downloaded } => {
                let file = cache.join(rel);
                let missing = cache.join(format!("{rel}.missing"));
                if let Ok(b) = std::fs::read(&file) {
                    return Ok(Some(b));
                }
                if missing.exists() {
                    return Ok(None);
                }
                let url = format!("{base}/{}/commit/{commit}/{}", if lfs { "media" } else { "raw" }, encode(rel));
                let mut last = None;
                for attempt in 0..3 {
                    match agent.get(&url).call() {
                        Ok(resp) => {
                            let mut bytes = Vec::new();
                            resp.into_reader().take(256 << 20).read_to_end(&mut bytes).map_err(|e| BakeError::Http(url.clone(), e.to_string()))?;
                            if let Some(dir) = file.parent() {
                                std::fs::create_dir_all(dir).map_err(|e| BakeError::Io(dir.to_owned(), e))?;
                            }
                            std::fs::write(&file, &bytes).map_err(|e| BakeError::Io(file.clone(), e))?;
                            *downloaded.lock().unwrap() += bytes.len() as u64;
                            return Ok(Some(bytes));
                        }
                        Err(ureq::Error::Status(404, _)) => {
                            if let Some(dir) = missing.parent() {
                                let _ = std::fs::create_dir_all(dir);
                            }
                            let _ = std::fs::write(&missing, b"");
                            return Ok(None);
                        }
                        Err(e) => {
                            last = Some(e.to_string());
                            std::thread::sleep(std::time::Duration::from_millis(500 * (attempt + 1)));
                        }
                    }
                }
                Err(BakeError::Http(url, last.unwrap_or_default()))
            }
        }
    }

    /// Downloads many LFS files with a few parallel workers (cache warm-up).
    pub fn prefetch(&self, rels: &[String], progress: &(dyn Fn(String) + Sync)) -> Result<(), BakeError> {
        let next = Mutex::new(0usize);
        let error = Mutex::new(None);
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| loop {
                    let i = {
                        let mut n = next.lock().unwrap();
                        let i = *n;
                        *n += 1;
                        i
                    };
                    let Some(rel) = rels.get(i) else { break };
                    if let Err(e) = self.binary(rel) {
                        *error.lock().unwrap() = Some(e);
                        break;
                    }
                    if i % 20 == 0 {
                        progress(format!("downloading art {}/{} ({} MB)", i + 1, rels.len(), self.downloaded() >> 20));
                    }
                });
            }
        });
        match error.into_inner().unwrap() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// Percent-encodes a repository path for a URL (keeps `/`).
fn encode(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}
