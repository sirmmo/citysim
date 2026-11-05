//! GlassBox rules files (`<data>/rules/*.toml`).

use std::path::Path;
use std::sync::Arc;

use opencity_core::rules::{PolicySet, Ruleset, TechTree};
use serde::Deserialize;

use crate::{read_text, AssetError};

#[derive(Deserialize)]
struct Index {
    rules: Vec<String>,
}

/// Loads and validates one rules file by id.
pub fn load(data_dir: &Path, id: &str) -> Result<Arc<Ruleset>, AssetError> {
    let src = Sources::read(data_dir, id)?;
    let rules = src.build().map_err(|p| AssetError::Parse { path: data_dir.join(p.path()), line: p.line, message: p.message })?;
    for w in &src.warnings {
        log::info!("{w}");
    }
    Ok(Arc::new(rules))
}

/// The text of a rules set: the rules file, its GlassBox scripts, its tech
/// tree and its policies, as the rules editor edits them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sources {
    pub id: String,
    /// Files by path relative to the data folder (`rules/glassbox.toml`,
    /// `rules/glassbox.gbx`, `techs/modern.toml`…), the rules file first.
    pub files: Vec<(String, String)>,
    /// Warnings of the last build (ignored script statements).
    pub warnings: Vec<String>,
}

/// What stops a rules set from building, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    /// Index in [`Sources::files`].
    pub file: usize,
    pub name: String,
    /// 1-based; 0 when unknown.
    pub line: usize,
    pub message: String,
}

impl Problem {
    pub fn path(&self) -> &str {
        &self.name
    }
}

impl Sources {
    /// Reads a rules set and every file it names.
    pub fn read(data_dir: &Path, id: &str) -> Result<Self, AssetError> {
        let main = format!("rules/{id}.toml");
        let text = read_text(&data_dir.join(&main))?;
        let mut src = Sources { id: id.to_string(), files: vec![(main, text)], warnings: Vec::new() };
        let mut names = Vec::new();
        if let Ok(header) = toml::from_str::<Names>(&src.files[0].1) {
            names.extend(header.scripts.iter().map(|s| format!("rules/{s}")));
            names.extend(header.tech_tree.map(|t| format!("techs/{t}.toml")));
            names.extend(header.policy_set.map(|p| format!("policies/{p}.toml")));
        }
        for name in names {
            let text = read_text(&data_dir.join(&name))?;
            src.files.push((name, text));
        }
        Ok(src)
    }

    /// Writes every file back to the data folder.
    pub fn write(&self, data_dir: &Path) -> std::io::Result<()> {
        for (name, text) in &self.files {
            let path = data_dir.join(name);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, text)?;
        }
        Ok(())
    }

    fn text(&self, name: &str) -> Option<(usize, &str)> {
        self.files.iter().position(|(n, _)| n == name).map(|i| (i, self.files[i].1.as_str()))
    }

    /// Builds and validates the rules from the texts as they are.
    pub fn build(&self) -> Result<Ruleset, Problem> {
        self.build_with_warnings().map(|(r, _)| r)
    }

    /// The rules and the scripts' warnings.
    pub fn build_with_warnings(&self) -> Result<(Ruleset, Vec<String>), Problem> {
        let problem =
            |file: usize, line: usize, message: String| Problem { file, name: self.files.get(file).map_or(String::new(), |f| f.0.clone()), line, message };
        let toml_problem = |file: usize, text: &str, e: toml::de::Error| {
            let line =
                e.span().map_or(0, |s| text[..s.start.min(text.len())].lines().count().max(1) + usize::from(text[..s.start.min(text.len())].ends_with('\n')));
            problem(file, line, e.message().to_string())
        };
        let (_, main) = self.files.first().map(|(n, t)| (n.as_str(), t.as_str())).ok_or_else(|| problem(0, 0, "no rules file".into()))?;
        let mut rules: Ruleset = toml::from_str(main).map_err(|e| toml_problem(0, main, e))?;
        let mut warnings = Vec::new();
        for script in rules.scripts.clone() {
            let name = format!("rules/{script}");
            let (i, text) = self.text(&name).ok_or_else(|| problem(0, 0, format!("script {script} not found")))?;
            let w = opencity_core::gbx::compile(&mut rules, &script, text).map_err(|e| {
                // "line N: message"
                let (line, message) = e
                    .message
                    .strip_prefix("line ")
                    .and_then(|m| m.split_once(": "))
                    .and_then(|(n, m)| Some((n.parse().ok()?, m.to_string())))
                    .unwrap_or((0, e.message.clone()));
                problem(i, line, message)
            })?;
            warnings.extend(w);
        }
        if let Some(tree) = rules.tech_tree.clone() {
            let name = format!("techs/{tree}.toml");
            let (i, text) = self.text(&name).ok_or_else(|| problem(0, 0, format!("tech tree {tree} not found")))?;
            rules.add_tech_tree(toml::from_str(text).map_err(|e| toml_problem(i, text, e))?);
        }
        if let Some(set) = rules.policy_set.clone() {
            let name = format!("policies/{set}.toml");
            let (i, text) = self.text(&name).ok_or_else(|| problem(0, 0, format!("policies {set} not found")))?;
            rules.add_policy_set(toml::from_str(text).map_err(|e| toml_problem(i, text, e))?);
        }
        rules.validate().map_err(|e| problem(0, 0, e.message))?;
        Ok((rules, warnings))
    }
}

/// The files a rules file names (read before the rest is valid).
#[derive(Deserialize)]
struct Names {
    #[serde(default)]
    scripts: Vec<String>,
    #[serde(default)]
    tech_tree: Option<String>,
    #[serde(default)]
    policy_set: Option<String>,
}

/// Reads a tech tree (`<data>/techs/<id>.toml`).
pub fn load_tech_tree(data_dir: &Path, id: &str) -> Result<TechTree, AssetError> {
    let path = data_dir.join("techs").join(format!("{id}.toml"));
    toml::from_str(&read_text(&path)?).map_err(|source| AssetError::Settings { path, source })
}

/// Reads a policy file (`<data>/policies/<id>.toml`).
pub fn load_policy_set(data_dir: &Path, id: &str) -> Result<PolicySet, AssetError> {
    let path = data_dir.join("policies").join(format!("{id}.toml"));
    toml::from_str(&read_text(&path)?).map_err(|source| AssetError::Settings { path, source })
}

/// Every rules set listed in `<data>/rules/index.toml`.
pub fn load_all(data_dir: &Path) -> Vec<Arc<Ruleset>> {
    let Ok(text) = read_text(&data_dir.join("rules").join("index.toml")) else { return Vec::new() };
    let Ok(index) = toml::from_str::<Index>(&text) else {
        log::warn!("invalid rules/index.toml");
        return Vec::new();
    };
    index.rules.iter().filter_map(|id| load(data_dir, id).map_err(|e| log::warn!("rules {id}: {e}")).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_rules_load() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let all = load_all(&dir);
        let ids: Vec<&str> = all.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["glassbox", "pre-industrial"]);
    }

    #[test]
    fn sources_build_and_point_at_problems() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let src = Sources::read(&dir, "glassbox").unwrap();
        assert_eq!(src.files[0].0, "rules/glassbox.toml");
        assert!(src.files.iter().any(|f| f.0 == "rules/glassbox.gbx"));
        assert!(src.files.iter().any(|f| f.0.starts_with("techs/")));
        assert_eq!(src.build().unwrap().id, "glassbox");
        // A TOML mistake: the line it is on.
        let mut bad = src.clone();
        bad.files[0].1 = bad.files[0].1.replacen("[[unit]]\nid = \"house_1\"", "[[unit]]\nid = \"house_1\"\nbogus = 3", 1);
        let p = bad.build().unwrap_err();
        let line = bad.files[0].1.lines().position(|l| l.starts_with("bogus")).unwrap() + 1;
        assert_eq!((p.file, p.line), (0, line), "{p:?}");
        // A script mistake: its file and line.
        let mut bad = src.clone();
        let gbx = bad.files.iter().position(|f| f.0.ends_with(".gbx")).unwrap();
        bad.files[gbx].1.push_str("\nunitRule broken\n    rate many\nend\n");
        let p = bad.build().unwrap_err();
        assert_eq!(p.file, gbx);
        assert_eq!(p.line, bad.files[gbx].1.lines().count() - 1, "{p:?}");
        // A dangling reference: caught by validation.
        let mut bad = src;
        bad.files[0].1 = bad.files[0].1.replacen("needs = { power = 1 }", "needs = { powr = 1 }", 1);
        assert!(bad.build().unwrap_err().message.contains("powr"));
    }
}
