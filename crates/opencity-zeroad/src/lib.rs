//! Bakes 0 A.D. art into citysim culture skins.
//!
//! Downloads only the actors, meshes and textures the mapping
//! (`assets/zeroad/antiquity.toml`) needs from the official 0 A.D.
//! repository at a pinned commit, over plain HTTPS (or reads a local
//! checkout), and converts them:
//!
//! * structure templates (with inheritance and mixins) give each culture's
//!   actors and building names;
//! * actors (variants, props) give meshes and textures;
//! * COLLADA meshes become Y-up AC3D models, props attached, scaled to the
//!   footprint of the OpenCity model they replace;
//! * textures become PNG with the culture's player colour.
//!
//! No Docker, git or Python is needed. 0 A.D. art is CC BY-SA 3.0.

mod collada;
mod data;
pub mod fetch;
mod xml;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::collada::{apply, Mat, IDENTITY};
use crate::data::ZeroAd;
use crate::fetch::{Source, PUBLIC};

#[derive(Debug, thiserror::Error)]
pub enum BakeError {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("download failed: {0}: {1}")]
    Http(String, String),
    #[error("invalid bake configuration: {0}")]
    Config(String),
}

#[derive(Deserialize)]
struct Config {
    source: SourceCfg,
    #[serde(default = "default_max_texture")]
    max_texture: u32,
    #[serde(default)]
    gaia: BTreeMap<String, Vec<String>>,
    bake: BakeCfg,
    models: BTreeMap<String, ModelSpec>,
}

fn default_max_texture() -> u32 {
    1024
}

#[derive(Deserialize)]
struct SourceCfg {
    repo: String,
    commit: String,
}

#[derive(Deserialize)]
struct BakeCfg {
    cultures: Vec<Culture>,
}

#[derive(Deserialize, Clone)]
struct Culture {
    civ: String,
    #[serde(default = "default_colour")]
    colour: [u8; 3],
}

fn default_colour() -> [u8; 3] {
    [150, 40, 30]
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ModelSpec {
    structure: Option<String>,
    actor: Option<String>,
    gaia: Option<String>,
    #[serde(default)]
    index: usize,
    look: Option<usize>,
}

/// Options for [`bake`].
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Where downloads are cached (default: the user cache directory).
    pub cache_dir: Option<PathBuf>,
    /// Read from a local 0 A.D. checkout instead of downloading.
    pub local_repo: Option<PathBuf>,
}

/// What was baked.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub cultures: Vec<(String, usize, usize)>,
    pub downloaded_bytes: u64,
}

/// OpenCity model footprints (tiles) for the codes we replace; must match assets/graphism.
const FOOTPRINTS: [(u16, (f64, f64)); 17] = [
    (16, (1.0, 2.0)),
    (18, (2.0, 2.0)),
    (19, (5.0, 1.0)),
    (24, (2.0, 1.0)),
    (25, (4.0, 4.0)),
    (26, (2.0, 2.0)),
    (27, (2.0, 2.0)),
    (28, (2.0, 2.0)),
    (29, (3.0, 3.0)),
    (38, (2.0, 2.0)),
    (39, (4.0, 2.0)),
    (300, (5.0, 3.0)),
    (303, (6.0, 6.0)),
    (350, (2.0, 2.0)),
    (360, (2.0, 2.0)),
    (370, (6.0, 3.0)),
    (390, (3.0, 3.0)),
];
const MARGIN: f64 = 0.92;
const MAX_PROP_DEPTH: usize = 3;
const SKIP_PROPS: [&str; 3] = ["props/structures/decals/", "props/special/common/plot_boundary", "props/special/common/garrison"];
const SKIP_POINTS: [&str; 5] = ["projectile", "garrison", "loaded", "fire", "smoke"];

fn footprint(code: u16) -> (f64, f64) {
    FOOTPRINTS.iter().find(|(c, _)| *c == code).map_or((1.0, 1.0), |(_, f)| *f)
}

fn wanted_props(look: &data::Look) -> impl Iterator<Item = &(String, String)> {
    look.props.iter().filter(|(point, prop)| !SKIP_PROPS.iter().any(|s| prop.starts_with(s)) && !SKIP_POINTS.iter().any(|s| point.to_lowercase().contains(s)))
}

fn z_up_to_y_up(p: [f64; 3]) -> [f64; 3] {
    [p[0], p[2], -p[1]]
}

fn y_up_to_z_up(p: [f64; 3]) -> [f64; 3] {
    [p[0], -p[2], p[1]]
}

/// Default download cache: `$XDG_CACHE_HOME/citysim/0ad`, `~/.cache/...` or `%LOCALAPPDATA%\citysim\0ad`.
pub fn default_cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("citysim")
        .join("0ad")
}

struct Part {
    texture: Option<String>,
    positions: Vec<[f64; 3]>,
    uvs: Vec<[f64; 2]>,
}

struct Baker<'a> {
    z: &'a ZeroAd,
    out: PathBuf,
    colour: [f64; 3],
    max_texture: u32,
    textures: BTreeMap<(String, &'static str), Option<String>>,
}

impl Baker<'_> {
    fn texture(&mut self, rel: &str, mat: &'static str) -> Result<Option<String>, BakeError> {
        let key = (rel.to_string(), mat);
        if let Some(t) = self.textures.get(&key) {
            return Ok(t.clone());
        }
        let Some((_, bytes)) = self.z.texture_file(rel)? else {
            log::warn!("texture {rel} missing");
            self.textures.insert(key, None);
            return Ok(None);
        };
        let Ok(img) = image::load_from_memory(&bytes) else {
            log::warn!("texture {rel}: unreadable");
            self.textures.insert(key, None);
            return Ok(None);
        };
        let mut img = img.to_rgba8();
        let (w, h) = img.dimensions();
        if w.max(h) > self.max_texture {
            let k = self.max_texture as f64 / w.max(h) as f64;
            img = image::imageops::resize(
                &img,
                ((w as f64 * k).round() as u32).max(1),
                ((h as f64 * k).round() as u32).max(1),
                image::imageops::FilterType::Lanczos3,
            );
        }
        for p in img.pixels_mut() {
            let a = p[3] as f64 / 255.0;
            match mat {
                // Alpha marks where the player colour shows through.
                "player" => {
                    for c in 0..3 {
                        p[c] = (p[c] as f64 * a + self.colour[c] * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
                    }
                    p[3] = 255;
                }
                "alphatest" => {}
                _ => p[3] = 255,
            }
        }
        let stem = rel.rsplit_once('.').map_or(rel, |(s, _)| s).replace('/', "_");
        let name = format!("textures/{stem}_{mat}.png");
        let path = self.out.join(&name);
        img.save(&path).map_err(|e| BakeError::Io(path.clone(), std::io::Error::other(e.to_string())))?;
        self.textures.insert(key, Some(name.clone()));
        Ok(Some(name))
    }

    /// Flattened parts of an actor (props included), in Y-up space.
    fn actor(&mut self, name: &str, look: usize, depth: usize) -> Result<Vec<Part>, BakeError> {
        let Some(actor) = self.z.actor(name)? else { return Ok(Vec::new()) };
        if actor.looks.is_empty() {
            return Ok(Vec::new());
        }
        let lk = actor.looks[look % actor.looks.len()].clone();
        let Some(mesh_rel) = &lk.mesh else { return Ok(Vec::new()) };
        let tex = match lk.base_texture() {
            Some(t) => self.texture(t, actor.material)?,
            None => None,
        };
        let Some(bytes) = self.z.mesh(mesh_rel)? else {
            log::warn!("mesh {mesh_rel} missing");
            return Ok(Vec::new());
        };
        let Some(mesh) = collada::parse(&String::from_utf8_lossy(&bytes)) else {
            log::warn!("mesh {mesh_rel}: unreadable");
            return Ok(Vec::new());
        };
        let up = |p: [f64; 3]| if mesh.z_up { z_up_to_y_up(p) } else { p };
        let mut parts = vec![Part { texture: tex, positions: mesh.positions.iter().map(|p| up(*p)).collect(), uvs: mesh.uvs.clone() }];
        if depth < MAX_PROP_DEPTH {
            for (point, prop) in wanted_props(&lk).cloned().collect::<Vec<_>>() {
                let m: Mat = mesh.props.get(&point.to_lowercase()).copied().unwrap_or(IDENTITY);
                for mut child in self.actor(&prop, 0, depth + 1)? {
                    // Prop points are in the parent's own (Z-up) space.
                    child.positions = child.positions.iter().map(|p| up(apply(&m, if mesh.z_up { y_up_to_z_up(*p) } else { *p }))).collect();
                    parts.push(child);
                }
            }
        }
        parts.retain(|p| !p.positions.is_empty());
        Ok(parts)
    }

    fn write_ac(&self, parts: &[Part], (w, l): (f64, f64), dest: &Path) -> Result<(), BakeError> {
        let all = parts.iter().flat_map(|p| p.positions.iter());
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        for p in all {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        let s = (w * MARGIN / (hi[0] - lo[0]).max(1e-6)).min(l * MARGIN / (hi[2] - lo[2]).max(1e-6));
        let centre = [(lo[0] + hi[0]) / 2.0, lo[1], (lo[2] + hi[2]) / 2.0];
        let offset = [w / 2.0, 0.0, l / 2.0];
        let mut out = String::from("AC3Db\nMATERIAL \"white\" rgb 1 1 1  amb 0.2 0.2 0.2  emis 0 0 0  spec 0 0 0  shi 10  trans 0\nOBJECT world\n");
        let _ = writeln!(out, "kids {}", parts.len());
        for (i, part) in parts.iter().enumerate() {
            let _ = writeln!(out, "OBJECT poly\nname \"part{i}\"");
            if let Some(t) = &part.texture {
                let _ = writeln!(out, "texture \"../{t}\"");
            }
            let _ = writeln!(out, "numvert {}", part.positions.len());
            for p in &part.positions {
                let _ = writeln!(out, "{:.5} {:.5} {:.5}", (p[0] - centre[0]) * s + offset[0], (p[1] - centre[1]) * s, (p[2] - centre[2]) * s + offset[2]);
            }
            let n = part.positions.len() / 3;
            let _ = writeln!(out, "numsurf {n}");
            for t in 0..n {
                out.push_str("SURF 0x30\nmat 0\nrefs 3\n");
                for k in 0..3 {
                    let j = 3 * t + k;
                    let _ = writeln!(out, "{j} {:.5} {:.5}", part.uvs[j][0], part.uvs[j][1]);
                }
            }
            out.push_str("kids 0\n");
        }
        std::fs::write(dest, out).map_err(|e| BakeError::Io(dest.to_owned(), e))
    }
}

fn toml_str(v: &str) -> String {
    format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
}

/// (code, actor, look) for every model the mapping replaces for `civ`.
fn mapping(z: &ZeroAd, cfg: &Config, civ: &str) -> Result<Vec<(u16, String, usize)>, BakeError> {
    let mut out = Vec::new();
    for (code, spec) in &cfg.models {
        let code: u16 = code.parse().map_err(|_| BakeError::Config(format!("bad graphic code {code}")))?;
        let actor = if let Some(s) = &spec.structure {
            let template = format!("structures/{civ}/{s}");
            match z.template_text(&template, "VisualActor/Actor")? {
                Some(a) => a,
                None => {
                    log::info!("{code}: {civ} has no {s}, skipped");
                    continue;
                }
            }
        } else if let Some(g) = &spec.gaia {
            cfg.gaia.get(g).and_then(|l| l.get(spec.index)).cloned().ok_or_else(|| BakeError::Config(format!("{code}: no gaia {g}[{}]", spec.index)))?
        } else if let Some(a) = &spec.actor {
            a.clone()
        } else {
            return Err(BakeError::Config(format!("{code}: needs structure, gaia or actor")));
        };
        out.push((code, actor, spec.look.unwrap_or(code as usize)));
    }
    Ok(out)
}

/// Meshes and texture files an actor (with its props) uses.
fn needs(z: &ZeroAd, actor: &str, out: &mut BTreeSet<String>, depth: usize) -> Result<(), BakeError> {
    let Some(a) = z.actor(actor)? else { return Ok(()) };
    for lk in &a.looks {
        if let Some(m) = &lk.mesh {
            out.insert(format!("{PUBLIC}/art/meshes/{m}"));
        }
        if let Some(t) = lk.base_texture() {
            let stem = t.rsplit_once('.').map_or(t, |(s, _)| s);
            out.insert(format!("{PUBLIC}/art/textures/skins/{stem}.png"));
        }
        if depth < MAX_PROP_DEPTH {
            for (_, prop) in wanted_props(lk) {
                needs(z, prop, out, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn skin_toml(z: &ZeroAd, civ: &str, colour: [u8; 3]) -> Result<String, BakeError> {
    let spec = |role: &str, default: &str| -> Result<String, BakeError> {
        Ok(z.template_text(&format!("structures/{civ}/{role}"), "Identity/SpecificName")?.unwrap_or_else(|| default.to_string()))
    };
    let name = z.civ_name(civ)?;
    let labels = [
        ("residential", format!("{} · homes", spec("house", "Houses")?)),
        ("commercial", format!("{} · trade", spec("civil_centre", "Market")?)),
        ("industrial", format!("{} · fields & crafts", spec("farmstead", "Farmstead")?)),
        ("coal_plant", format!("{} · stronghold", spec("fortress", "Fortress")?)),
        ("police", format!("{} · garrison", spec("barracks", "Barracks")?)),
        ("fire_station", format!("{} · watch", spec("wall_tower", "Tower")?)),
        ("hospital", "Temple of healing".into()),
        ("school", "School".into()),
        ("park", "Garden".into()),
        ("trees", "Grove".into()),
        ("paved_street", "Paved road".into()),
        ("dirt_road", "Track".into()),
    ];
    let mut s = String::from("# Generated by opencity-cli bake-0ad\n");
    let _ = writeln!(s, "name = {}", toml_str(&format!("Antiquity: {name}")));
    let _ = writeln!(s, "description = {}", toml_str(&format!("{name} towns built from 0 A.D. art (CC BY-SA 3.0).")));
    s.push_str("era = \"Antiquity\"\n");
    let _ = writeln!(s, "culture = {}", toml_str(&name));
    s.push_str("disabled_tools = [\"nuclear_plant\", \"solar_plant\", \"fusion_plant\", \"power_line\", \"road\", \"highway\", \"rail\", \"station\"]\n\n[applies]\nyears = [-1000, 500]\n\n[look]\nsky = [0.70, 0.78, 0.88]\nsaturation = 0.95\nwater = [0.18, 0.40, 0.55, 0.60]\n");
    let _ = writeln!(s, "ui_accent = [{}, {}, {}]", colour[0], colour[1], colour[2]);
    s.push_str("ui_panel = [36, 28, 24]\nvehicles = false\n\n[labels]\n");
    for (k, v) in labels {
        let _ = writeln!(s, "{k} = {}", toml_str(&v));
    }
    Ok(s)
}

const ATTRIBUTION: &str = "Models and textures in this folder are converted from 0 A.D. by Wildfire Games
(https://play0ad.com), {repo} at commit {commit},
by citysim's bake (opencity-cli bake-0ad): COLLADA meshes converted to AC3D
(Y-up, props attached, scaled), textures re-encoded as PNG with the player colour applied.

0 A.D. art is licensed under Creative Commons Attribution-ShareAlike 3.0
(https://creativecommons.org/licenses/by-sa/3.0/). These derived files are shared
under the same licence.
";

/// Bakes every culture of `<data>/zeroad/antiquity.toml` into `<data>/skins`.
pub fn bake(data_dir: &Path, opts: &Options, progress: &(dyn Fn(String) + Sync)) -> Result<Report, BakeError> {
    let cfg_path = data_dir.join("zeroad").join("antiquity.toml");
    let text = std::fs::read_to_string(&cfg_path).map_err(|e| BakeError::Io(cfg_path.clone(), e))?;
    let cfg: Config = toml::from_str(&text).map_err(|e| BakeError::Config(e.to_string()))?;
    let source = match &opts.local_repo {
        Some(p) => Source::Local(p.clone()),
        None => Source::remote(&cfg.source.repo, &cfg.source.commit, &opts.cache_dir.clone().unwrap_or_else(default_cache_dir)),
    };
    let z = ZeroAd::new(source);
    progress("reading 0 A.D. templates and actors".into());
    let mut cultures = Vec::new();
    for c in &cfg.bake.cultures {
        if z.has_template(&format!("structures/{}/house", c.civ))? {
            cultures.push(c.clone());
        } else {
            log::warn!("{}: unknown civilisation, skipped", c.civ);
        }
    }
    let mut files = BTreeSet::new();
    let mut plans = Vec::new();
    for c in &cultures {
        let m = mapping(&z, &cfg, &c.civ)?;
        for (_, actor, _) in &m {
            needs(&z, actor, &mut files, 0)?;
        }
        plans.push((c.clone(), m));
    }
    let files: Vec<String> = files.into_iter().collect();
    z.source.prefetch(&files, progress)?;
    let skins = data_dir.join("skins");
    let mut report = Report::default();
    let mut baked = Vec::new();
    for (c, models) in plans {
        let name = z.civ_name(&c.civ)?;
        progress(format!("converting {name}"));
        let out = skins.join(format!("antiquity-{}", c.civ));
        let _ = std::fs::remove_dir_all(&out);
        for d in ["models", "textures"] {
            std::fs::create_dir_all(out.join(d)).map_err(|e| BakeError::Io(out.join(d), e))?;
        }
        let mut b = Baker { z: &z, out: out.clone(), colour: c.colour.map(|v| v as f64), max_texture: cfg.max_texture, textures: BTreeMap::new() };
        let mut entries = Vec::new();
        for (code, actor, look) in models {
            let parts = b.actor(&actor, look, 0)?;
            if parts.is_empty() {
                log::info!("{code}: {actor}: no mesh, skipped");
                continue;
            }
            let file = format!("models/{code}.ac");
            b.write_ac(&parts, footprint(code), &out.join(&file))?;
            entries.push(format!("{code} = \"{file}\""));
        }
        let write = |name: &str, text: String| std::fs::write(out.join(name), text).map_err(|e| BakeError::Io(out.join(name), e));
        write("models.toml", format!("# Generated by opencity-cli bake-0ad\n{}\n", entries.join("\n")))?;
        write("skin.toml", skin_toml(&z, &c.civ, c.colour)?)?;
        write("ATTRIBUTION.txt", ATTRIBUTION.replace("{repo}", &cfg.source.repo).replace("{commit}", &cfg.source.commit))?;
        let textures = b.textures.values().filter(|t| t.is_some()).count();
        report.cultures.push((name, entries.len(), textures));
        baked.push(format!("antiquity-{}", c.civ));
    }
    let list = baked.iter().map(|b| toml_str(b)).collect::<Vec<_>>().join(", ");
    let generated = skins.join("generated.toml");
    std::fs::write(&generated, format!("# Generated by opencity-cli bake-0ad: skins baked from 0 A.D. art.\nskins = [{list}]\n"))
        .map_err(|e| BakeError::Io(generated, e))?;
    report.downloaded_bytes = z.source.downloaded();
    Ok(report)
}
