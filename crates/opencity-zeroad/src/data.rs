//! 0 A.D. simulation templates and art actors, read from a [`Source`].

use std::cell::RefCell;
use std::collections::HashMap;

use crate::fetch::{Source, PUBLIC};
use crate::xml::Elem;
use crate::BakeError;

/// Variants that are not the idle look of a building or tree.
const SKIP_VARIANTS: [&str; 13] = ["destruction", "death", "dead", "garrison", "fire", "burn", "ruin", "carry", "gather", "attack", "packed", "winter", "snow"];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub mesh: Option<String>,
    pub textures: Vec<(String, String)>,
    /// (attach point, actor)
    pub props: Vec<(String, String)>,
}

impl Look {
    pub fn base_texture(&self) -> Option<&str> {
        self.textures.iter().find(|(n, _)| n == "baseTex").or(self.textures.first()).map(|(_, f)| f.as_str())
    }

    fn apply(&mut self, v: &Elem, data: &ZeroAd) -> Result<(), BakeError> {
        if let Some(file) = v.attr("file") {
            if let Some(inc) = data.xml(&format!("{PUBLIC}/art/variants/{file}"))? {
                self.apply(&inc, data)?;
            }
        }
        if let Some(m) = v.child("mesh").map(|m| m.text.trim().to_string()).filter(|m| !m.is_empty()) {
            self.mesh = Some(m);
        }
        if let Some(t) = v.child("textures") {
            for tex in t.children_named("texture") {
                if let Some(file) = tex.attr("file") {
                    let name = tex.attr("name").unwrap_or("baseTex").to_string();
                    self.textures.retain(|(n, _)| *n != name);
                    self.textures.push((name, file.to_string()));
                }
            }
        }
        if let Some(p) = v.child("props") {
            let new: Vec<(String, String)> =
                p.children_named("prop").map(|p| (p.attr("attachpoint").unwrap_or("root").to_string(), p.attr("actor").unwrap_or("").to_string())).collect();
            self.props.retain(|(point, _)| !new.iter().any(|(n, _)| n == point));
            self.props.extend(new.into_iter().filter(|(_, a)| !a.is_empty()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Actor {
    pub looks: Vec<Look>,
    /// "player", "alphatest" or "opaque".
    pub material: &'static str,
}

pub struct ZeroAd {
    pub source: Source,
    templates: RefCell<HashMap<String, Option<Elem>>>,
    xml_cache: RefCell<HashMap<String, Option<Elem>>>,
}

fn frequency(v: &Elem) -> f32 {
    v.attr("frequency").and_then(|f| f.parse().ok()).unwrap_or(0.0)
}

fn idle(vs: &[&Elem]) -> Vec<Elem> {
    let ok: Vec<&Elem> = vs.iter().copied().filter(|v| !SKIP_VARIANTS.iter().any(|s| v.attr("name").unwrap_or("").to_lowercase().contains(s))).collect();
    let weighted: Vec<Elem> = ok.iter().filter(|v| frequency(v) > 0.0).map(|v| (*v).clone()).collect();
    if !weighted.is_empty() {
        weighted
    } else if let Some(v) = ok.first() {
        vec![(*v).clone()]
    } else {
        vs.first().map(|v| vec![(*v).clone()]).unwrap_or_default()
    }
}

pub fn material_kind(material: &str) -> &'static str {
    let m = material.to_lowercase();
    if m.starts_with("player_trans") {
        "player"
    } else if m.contains("trans") && !m.starts_with("no_trans") {
        "alphatest"
    } else {
        "opaque"
    }
}

impl ZeroAd {
    pub fn new(source: Source) -> Self {
        Self { source, templates: RefCell::new(HashMap::new()), xml_cache: RefCell::new(HashMap::new()) }
    }

    fn xml(&self, rel: &str) -> Result<Option<Elem>, BakeError> {
        if let Some(e) = self.xml_cache.borrow().get(rel) {
            return Ok(e.clone());
        }
        let e = self.source.text(rel)?.and_then(|t| Elem::parse(&t));
        self.xml_cache.borrow_mut().insert(rel.to_string(), e.clone());
        Ok(e)
    }

    fn raw_template(&self, name: &str) -> Result<Option<Elem>, BakeError> {
        self.xml(&format!("{PUBLIC}/simulation/templates/{name}.xml"))
    }

    pub fn has_template(&self, name: &str) -> Result<bool, BakeError> {
        Ok(self.raw_template(name)?.is_some())
    }

    /// Template with its parents (and `a|b` mixins) merged in.
    pub fn template(&self, name: &str) -> Result<Option<Elem>, BakeError> {
        if let Some(t) = self.templates.borrow().get(name) {
            return Ok(t.clone());
        }
        let Some(node) = self.raw_template(name)? else {
            self.templates.borrow_mut().insert(name.to_string(), None);
            return Ok(None);
        };
        let resolved = match node.attr("parent") {
            None => node,
            Some(parent) => {
                let names: Vec<&str> = parent.split('|').collect();
                let last = self.resolve_name(names[names.len() - 1])?;
                let mut base = self.template(&last)?.unwrap_or_default();
                for mixin in names[..names.len() - 1].iter().rev() {
                    let m = self.resolve_name(mixin)?;
                    if let Some(t) = self.template(&m)? {
                        base.merge(&t);
                    }
                }
                base.merge(&node);
                base
            }
        };
        self.templates.borrow_mut().insert(name.to_string(), Some(resolved.clone()));
        Ok(Some(resolved))
    }

    fn resolve_name(&self, name: &str) -> Result<String, BakeError> {
        if self.has_template(name)? || !self.has_template(&format!("mixins/{name}"))? {
            Ok(name.to_string())
        } else {
            Ok(format!("mixins/{name}"))
        }
    }

    pub fn template_text(&self, name: &str, path: &str) -> Result<Option<String>, BakeError> {
        Ok(self.template(name)?.and_then(|t| t.text_at(path)))
    }

    pub fn civ_name(&self, civ: &str) -> Result<String, BakeError> {
        Ok(self.template_text(&format!("special/players/{civ}"), "Identity/GenericName")?.unwrap_or_else(|| civ.to_string()))
    }

    /// Up to four idle looks of an actor.
    pub fn actor(&self, name: &str) -> Result<Option<Actor>, BakeError> {
        let file = if name.ends_with(".xml") { name.to_string() } else { format!("{name}.xml") };
        let Some(mut root) = self.xml(&format!("{PUBLIC}/art/actors/{file}"))? else { return Ok(None) };
        if root.tag == "qualitylevels" {
            if let Some(a) = root.children_named("actor").last().cloned() {
                root = a;
            }
        }
        let choices: Vec<Vec<Elem>> =
            root.children_named("group").map(|g| g.children_named("variant").collect::<Vec<_>>()).filter(|v| !v.is_empty()).map(|v| idle(&v)).collect();
        if choices.is_empty() {
            return Ok(None);
        }
        let vary = choices.iter().position(|c| c.len() > 1);
        let n = vary.map_or(1, |i| choices[i].len().min(4));
        let mut looks = Vec::new();
        for k in 0..n {
            let mut look = Look::default();
            for (i, c) in choices.iter().enumerate() {
                let v = if Some(i) == vary { &c[k] } else { c.iter().max_by(|a, b| frequency(a).total_cmp(&frequency(b))).unwrap() };
                look.apply(v, self)?;
            }
            looks.push(look);
        }
        let material = root.text_at("material").unwrap_or_else(|| "default.xml".into());
        Ok(Some(Actor { looks, material: material_kind(&material) }))
    }

    /// Repository path of a texture (`.png`, `.dds` or `.tga`), if any exists.
    pub fn texture_file(&self, rel: &str) -> Result<Option<(String, Vec<u8>)>, BakeError> {
        let stem = rel.rsplit_once('.').map_or(rel, |(s, _)| s);
        let mut exts: Vec<&str> = Vec::new();
        if let Some((_, e)) = rel.rsplit_once('.') {
            exts.push(e);
        }
        exts.extend(["png", "dds", "tga"]);
        for ext in exts {
            let path = format!("{PUBLIC}/art/textures/skins/{stem}.{ext}");
            if let Some(b) = self.source.binary(&path)? {
                return Ok(Some((path, b)));
            }
        }
        Ok(None)
    }

    pub fn mesh(&self, rel: &str) -> Result<Option<Vec<u8>>, BakeError> {
        self.source.binary(&format!("{PUBLIC}/art/meshes/{rel}"))
    }
}
