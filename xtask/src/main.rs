//! Build helpers.
//!
//! `android-shim <dir>`: writes a copy of the game's crates whose manifests
//! the Cargo 1.61 embedded in cargo-quad-apk can parse (no workspace
//! inheritance, lockfile v3). See scripts/android.sh.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use toml::{Table, Value};

/// Dependencies unused on Android whose manifests Cargo 1.61 cannot resolve.
const DROP_ON_ANDROID: &[&str] = &["env_logger"];

/// Declared MSRV of the shim crates; see `android_shim`.
const SHIM_RUST_VERSION: &str = "1.79";

/// Crates the Android game needs, in dependency order.
const CRATES: &[&str] = &["opencity-core", "opencity-mapgen", "opencity-assets", "opencity-geo", "opencity"];

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [task, out] if task == "android-shim" => android_shim(Path::new(out)),
        _ => bail!("usage: cargo run -p xtask -- android-shim <out-dir>"),
    }
}

fn read_toml(p: &Path) -> Result<Table> {
    Ok(fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?.parse::<Table>()?)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

fn is_inherited(v: &Value) -> bool {
    v.as_table().and_then(|t| t.get("workspace")).and_then(Value::as_bool) == Some(true)
}

fn android_shim(out: &Path) -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let ws_manifest = read_toml(&root.join("Cargo.toml"))?;
    let ws = ws_manifest.get("workspace").and_then(Value::as_table).context("no [workspace]")?;
    let ws_package = ws.get("package").and_then(Value::as_table).cloned().unwrap_or_default();
    let ws_deps = ws.get("dependencies").and_then(Value::as_table).cloned().unwrap_or_default();

    if out.exists() {
        fs::remove_dir_all(out)?;
    }
    for name in CRATES {
        let dir = root.join("crates").join(name);
        let mut manifest = read_toml(&dir.join("Cargo.toml"))?;
        let package = manifest.get_mut("package").and_then(Value::as_table_mut).context("no [package]")?;
        for (key, value) in package.iter_mut() {
            if is_inherited(value) {
                *value = ws_package.get(key).cloned().with_context(|| format!("{name}: workspace has no package.{key}"))?;
            }
        }
        package.insert("edition".into(), "2021".into());
        // Lets the MSRV-aware resolver pick dependency versions that predate
        // edition 2024 (which Cargo 1.61 cannot parse). rustc 1.88 still compiles.
        package.insert("rust-version".into(), SHIM_RUST_VERSION.into());
        if let Some(deps) = manifest.get_mut("dependencies").and_then(Value::as_table_mut) {
            deps.retain(|dep, _| !DROP_ON_ANDROID.contains(&dep));
            for (dep, value) in deps.iter_mut() {
                if !is_inherited(value) {
                    continue;
                }
                let mut resolved = match ws_deps.get(dep).with_context(|| format!("{name}: workspace has no dependency {dep}"))? {
                    Value::String(v) => {
                        let mut t = Table::new();
                        t.insert("version".into(), v.clone().into());
                        t
                    }
                    Value::Table(t) => t.clone(),
                    other => bail!("unexpected workspace dependency {dep} = {other}"),
                };
                if let Some(Value::String(path)) = resolved.get("path") {
                    // Workspace paths are relative to the root; make them relative to the crate.
                    let crate_dir = Path::new(path).file_name().unwrap().to_string_lossy().into_owned();
                    resolved.insert("path".into(), format!("../{crate_dir}").into());
                }
                if let Some(local) = value.as_table() {
                    for (k, v) in local {
                        if k != "workspace" {
                            resolved.insert(k.clone(), v.clone());
                        }
                    }
                }
                *value = Value::Table(resolved);
            }
        }
        let dest = out.join("crates").join(name);
        fs::create_dir_all(&dest)?;
        fs::write(dest.join("Cargo.toml"), toml::to_string(&manifest)?)?;
        copy_dir(&dir.join("src"), &dest.join("src"))?;
        if dir.join("android").exists() {
            copy_dir(&dir.join("android"), &dest.join("android"))?;
        }
    }

    let mut root_manifest = Table::new();
    let mut workspace = Table::new();
    workspace.insert("members".into(), Value::Array(CRATES.iter().map(|c| Value::from(format!("crates/{c}"))).collect()));
    workspace.insert("resolver".into(), "2".into());
    root_manifest.insert("workspace".into(), Value::Table(workspace));
    if let Some(profile) = ws_manifest.get("profile") {
        root_manifest.insert("profile".into(), profile.clone());
    }
    fs::write(out.join("Cargo.toml"), toml::to_string(&root_manifest)?)?;

    copy_dir(&root.join("assets"), &out.join("assets"))?;
    // Fresh lockfile: dependency versions compatible with Rust 1.79, written in
    // lockfile format v3 (chosen by Cargo from that MSRV).
    let status = std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["generate-lockfile", "--quiet"])
        .env("CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS", "fallback")
        .current_dir(out)
        .status()?;
    if !status.success() {
        bail!("cargo generate-lockfile failed in {}", out.display());
    }
    println!("android shim written to {}", out.display());
    Ok(())
}
