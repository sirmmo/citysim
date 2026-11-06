//! Declarative mapping from feature properties to OpenCity structures.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::GeoError;

/// What a feature becomes in the city.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Build {
    Residential,
    Commercial,
    Industrial,
    Road,
    PowerLine,
    CoalPlant,
    NuclearPlant,
    Park,
    Trees,
    Water,
    FireStation,
    Police,
    Hospital,
    School,
    Rail,
    Station,
    Harbour,
    WaterPipe,
    PumpingStation,
    WaterTower,
    WaterTreatment,
    Dump,
    Recycling,
    Incinerator,
    Tram,
    BusStop,
    TramStop,
    Metro,
    MetroStation,
    Airport,
    Seaport,
    Ignore,
}

impl Build {
    pub fn parse(s: &str) -> Option<Self> {
        use Build::*;
        Some(match s {
            "residential" => Residential,
            "commercial" => Commercial,
            "industrial" => Industrial,
            "road" => Road,
            "power_line" => PowerLine,
            "coal_plant" => CoalPlant,
            "nuclear_plant" => NuclearPlant,
            "park" => Park,
            "trees" => Trees,
            "water" => Water,
            "fire_station" => FireStation,
            "police" => Police,
            "hospital" => Hospital,
            "school" => School,
            "rail" => Rail,
            "station" => Station,
            "harbour" => Harbour,
            "water_pipe" => WaterPipe,
            "pumping_station" => PumpingStation,
            "water_tower" => WaterTower,
            "water_treatment" => WaterTreatment,
            "dump" => Dump,
            "recycling" => Recycling,
            "incinerator" => Incinerator,
            "tram" => Tram,
            "bus_stop" => BusStop,
            "tram_stop" => TramStop,
            "metro" => Metro,
            "metro_station" => MetroStation,
            "airport" => Airport,
            "seaport" => Seaport,
            "ignore" => Ignore,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        use Build::*;
        match self {
            Residential => "residential",
            Commercial => "commercial",
            Industrial => "industrial",
            Road => "road",
            PowerLine => "power_line",
            CoalPlant => "coal_plant",
            NuclearPlant => "nuclear_plant",
            Park => "park",
            Trees => "trees",
            Water => "water",
            FireStation => "fire_station",
            Police => "police",
            Hospital => "hospital",
            School => "school",
            Rail => "rail",
            Station => "station",
            Harbour => "harbour",
            WaterPipe => "water_pipe",
            PumpingStation => "pumping_station",
            WaterTower => "water_tower",
            WaterTreatment => "water_treatment",
            Dump => "dump",
            Recycling => "recycling",
            Incinerator => "incinerator",
            Tram => "tram",
            BusStop => "bus_stop",
            TramStop => "tram_stop",
            Metro => "metro",
            MetroStation => "metro_station",
            Airport => "airport",
            Seaport => "seaport",
            Ignore => "ignore",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum Values {
    One(String),
    Many(Vec<String>),
}

impl Values {
    fn matches(&self, v: &str) -> bool {
        match self {
            Values::One(s) => s == "*" || s == v,
            Values::Many(list) => list.iter().any(|s| s == "*" || s == v),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    when: BTreeMap<String, Values>,
    build: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRules {
    #[serde(default)]
    elevation: Vec<String>,
    #[serde(default)]
    start_date: Vec<String>,
    #[serde(default)]
    end_date: Vec<String>,
    #[serde(default)]
    levels: Vec<String>,
    #[serde(default)]
    rule: Vec<RawRule>,
}

#[derive(Clone, Debug)]
enum Target {
    Fixed(Build),
    /// Read the build kind from a property (round trip of exports).
    FromProperty(String),
}

#[derive(Clone, Debug)]
pub struct Rules {
    pub elevation: Vec<String>,
    pub start_date: Vec<String>,
    pub end_date: Vec<String>,
    pub levels: Vec<String>,
    rules: Vec<(BTreeMap<String, Values>, Target)>,
}

pub type Properties = BTreeMap<String, String>;

impl Rules {
    pub fn parse(text: &str) -> Result<Self, GeoError> {
        let raw: RawRules = toml::from_str(text).map_err(|e| GeoError::Rules(e.to_string()))?;
        let mut rules = Vec::new();
        for r in raw.rule {
            let target = match r.build.strip_prefix("from:") {
                Some(prop) => Target::FromProperty(prop.to_string()),
                None => Target::Fixed(Build::parse(&r.build).ok_or_else(|| GeoError::Rules(format!("unknown build kind {:?}", r.build)))?),
            };
            rules.push((r.when, target));
        }
        Ok(Self { elevation: raw.elevation, start_date: raw.start_date, end_date: raw.end_date, levels: raw.levels, rules })
    }

    /// The rules shipped with OpenCity (OSM / OpenHistoryMap tagging).
    pub fn builtin() -> Self {
        Self::parse(include_str!("default_rules.toml")).expect("built-in rules are valid")
    }

    pub fn classify(&self, props: &Properties) -> Option<Build> {
        for (when, target) in &self.rules {
            if when.iter().all(|(k, vals)| props.get(k).is_some_and(|v| vals.matches(v))) {
                return match target {
                    Target::Fixed(b) => Some(*b),
                    Target::FromProperty(p) => props.get(p).and_then(|v| Build::parse(v)),
                };
            }
        }
        None
    }

    pub fn first<'a>(&self, props: &'a Properties, keys: &[String]) -> Option<&'a str> {
        keys.iter().find_map(|k| props.get(k)).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(kv: &[(&str, &str)]) -> Properties {
        kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn builtin_rules_classify_osm_tags() {
        let r = Rules::builtin();
        assert_eq!(r.classify(&props(&[("building", "house")])), Some(Build::Residential));
        assert_eq!(r.classify(&props(&[("building", "yes"), ("shop", "bakery")])), Some(Build::Commercial));
        assert_eq!(r.classify(&props(&[("power", "plant"), ("plant:source", "nuclear")])), Some(Build::NuclearPlant));
        assert_eq!(r.classify(&props(&[("power", "plant"), ("plant:source", "coal")])), Some(Build::CoalPlant));
        assert_eq!(r.classify(&props(&[("highway", "residential")])), Some(Build::Road));
        assert_eq!(r.classify(&props(&[("highway", "footway")])), None);
        assert_eq!(r.classify(&props(&[("opencity:build", "park")])), Some(Build::Park));
    }

    #[test]
    fn custom_vocabulary() {
        let r = Rules::parse("[[rule]]\nwhen = { kind = \"tavern\" }\nbuild = \"commercial\"\n").unwrap();
        assert_eq!(r.classify(&props(&[("kind", "tavern")])), Some(Build::Commercial));
        assert!(Rules::parse("[[rule]]\nwhen = {}\nbuild = \"castle\"\n").is_err());
    }
}
