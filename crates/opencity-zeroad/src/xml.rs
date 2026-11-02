//! A small owned XML tree with 0 A.D.'s template inheritance rules.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Elem {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub text: String,
    pub children: Vec<Elem>,
}

impl Elem {
    pub fn parse(text: &str) -> Option<Elem> {
        let doc = roxmltree::Document::parse(text).ok()?;
        Some(convert(doc.root_element()))
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    pub fn child(&self, tag: &str) -> Option<&Elem> {
        self.children.iter().find(|c| c.tag == tag)
    }

    pub fn children_named<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Elem> {
        self.children.iter().filter(move |c| c.tag == tag)
    }

    /// Descends `A/B/C`.
    pub fn find(&self, path: &str) -> Option<&Elem> {
        path.split('/').try_fold(self, |e, part| e.child(part))
    }

    pub fn text_at(&self, path: &str) -> Option<String> {
        self.find(path).map(|e| e.text.trim().to_string()).filter(|t| !t.is_empty())
    }

    fn set_attr(&mut self, k: &str, v: &str) {
        match self.attrs.iter_mut().find(|(key, _)| key == k) {
            Some(a) => a.1 = v.to_string(),
            None => self.attrs.push((k.to_string(), v.to_string())),
        }
    }

    /// Layers `src` onto `self` (0 A.D. template semantics, simplified).
    pub fn merge(&mut self, src: &Elem) {
        for (k, v) in &src.attrs {
            if !matches!(k.as_str(), "parent" | "replace" | "merge" | "op" | "filtered" | "disable") {
                self.set_attr(k, v);
            }
        }
        if !src.text.trim().is_empty() {
            self.text = src.text.clone();
        }
        for child in &src.children {
            let existing = self.children.iter().position(|c| c.tag == child.tag);
            if child.attr("disable").is_some() {
                if let Some(i) = existing {
                    self.children.remove(i);
                }
                continue;
            }
            match existing {
                Some(i) if child.attr("replace").is_none() => self.children[i].merge(child),
                _ => {
                    if let Some(i) = existing {
                        self.children.remove(i);
                    } else if child.attr("merge").is_some() {
                        continue;
                    }
                    self.children.push(child.clone());
                }
            }
        }
    }
}

fn convert(node: roxmltree::Node) -> Elem {
    Elem {
        tag: node.tag_name().name().to_string(),
        attrs: node.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect(),
        text: node.children().filter(|c| c.is_text()).map(|c| c.text().unwrap_or("")).collect::<String>(),
        children: node.children().filter(|c| c.is_element()).map(convert).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_follows_template_rules() {
        let mut base =
            Elem::parse("<E><Identity><Civ>gaia</Civ><GenericName>House</GenericName></Identity><Health><Max>100</Max></Health><Cost/></E>").unwrap();
        let over = Elem::parse("<E parent=\"x\"><Identity><Civ>rome</Civ><SpecificName>Domus</SpecificName></Identity><Health disable=\"\"/><Cost replace=\"\"><Wood>5</Wood></Cost></E>").unwrap();
        base.merge(&over);
        assert_eq!(base.text_at("Identity/Civ").as_deref(), Some("rome"));
        assert_eq!(base.text_at("Identity/GenericName").as_deref(), Some("House"));
        assert_eq!(base.text_at("Identity/SpecificName").as_deref(), Some("Domus"));
        assert!(base.find("Health").is_none());
        assert_eq!(base.text_at("Cost/Wood").as_deref(), Some("5"));
        assert!(base.attr("parent").is_none());
    }
}
