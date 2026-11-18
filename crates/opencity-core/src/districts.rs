//! Neighbourhoods: the part of the city around each neighbourhood office
//! (any unit with a `district` radius). Each has a name, its own figures
//! and, if the mayor sets one, a policy of the rules
//! (`[[neighbourhood_policy]]`) that holds there only.

use serde::{Deserialize, Serialize};

use crate::City;

/// What the mayor set for a neighbourhood (kept with the city, by the
/// tile of its office).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct District {
    pub at: (u32, u32),
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
}

/// A neighbourhood as it stands today.
#[derive(Clone, Debug, PartialEq)]
pub struct DistrictInfo {
    /// Origin tile of its office.
    pub at: (u32, u32),
    pub name: String,
    /// The office's own name ("Neighbourhood office", "Ward house").
    pub office: String,
    pub policy: Option<String>,
    pub tiles: u32,
    pub buildings: u32,
    pub residents: f32,
    pub jobs: f32,
    /// How well its homes, shops and works have what they need, 0..1.
    pub satisfaction: f32,
    /// The value of its land and what its residents could complain of:
    /// (name, average over its tiles).
    pub maps: Vec<(String, f32)>,
}

/// An office that makes a neighbourhood: its origin tile, its centre,
/// how far it reaches and its unit's index.
type Office = ((u32, u32), (f32, f32), u32, usize);

const NAMES: [&str; 32] = [
    "Old Town",
    "Riverside",
    "Hillcrest",
    "Station Quarter",
    "Market Side",
    "Northgate",
    "Southfields",
    "Eastbank",
    "Westend",
    "The Meadows",
    "Mill Lane",
    "Churchfields",
    "Orchard Park",
    "Kingsway",
    "Canalside",
    "The Heights",
    "Greenfields",
    "Brickyard",
    "Lakeview",
    "Parkside",
    "The Crescent",
    "Foundry Row",
    "Harbour Hill",
    "Elm Grove",
    "Fairview",
    "Stonebridge",
    "The Commons",
    "Maple Court",
    "Windmill Hill",
    "Bellfield",
    "Ashgrove",
    "Longacre",
];

impl City {
    /// The offices that make a neighbourhood: (origin, centre, radius, unit index).
    fn district_offices(&self) -> Vec<Office> {
        let Some(rules) = self.rules() else { return Vec::new() };
        self.layer
            .structures()
            .filter_map(|((w, l), s)| {
                let u = s.unit? as usize;
                let radius = rules.units.get(u)?.district;
                let (sw, sl) = s.size(self.footprints());
                (radius > 0).then_some(((w, l), (w as f32 + sw as f32 / 2.0, l as f32 + sl as f32 / 2.0), radius, u))
            })
            .collect()
    }

    /// For every tile, the neighbourhood it lies in (the index of its
    /// office in [`City::districts`]): the nearest office that reaches it.
    pub fn district_map(&self) -> Vec<Option<u16>> {
        let (width, length) = (self.width(), self.length());
        let mut map: Vec<Option<u16>> = vec![None; (width * length) as usize];
        let mut best = vec![f32::INFINITY; map.len()];
        for (n, (_, (cx, cz), radius, _)) in self.district_offices().into_iter().enumerate() {
            let r = radius as i64;
            for l in ((cz as i64 - r).max(0))..=((cz as i64 + r).min(length as i64 - 1)) {
                for w in ((cx as i64 - r).max(0))..=((cx as i64 + r).min(width as i64 - 1)) {
                    let d = (w as f32 + 0.5 - cx).abs() + (l as f32 + 0.5 - cz).abs();
                    let i = (l as u32 * width + w as u32) as usize;
                    if d <= radius as f32 && d < best[i] {
                        best[i] = d;
                        map[i] = Some(n as u16);
                    }
                }
            }
        }
        map
    }

    /// The name a neighbourhood has until the mayor gives it one: by where
    /// its office stands, so it stays the same.
    fn default_district_name(at: (u32, u32), taken: &[String]) -> String {
        let start = (at.0 as usize * 7 + at.1 as usize * 13) % NAMES.len();
        (0..NAMES.len())
            .map(|k| NAMES[(start + k) % NAMES.len()].to_string())
            .find(|n| !taken.contains(n))
            .unwrap_or_else(|| format!("Neighbourhood {}, {}", at.0, at.1))
    }

    /// Every neighbourhood with its figures, in the order of the map.
    pub fn districts(&self) -> Vec<DistrictInfo> {
        let offices = self.district_offices();
        let (Some(rules), Some(gb)) = (self.rules(), self.glassbox.as_ref()) else { return Vec::new() };
        if offices.is_empty() {
            return Vec::new();
        }
        let map = self.district_map();
        let width = self.width();
        // The land's value, then what residents complain of.
        let mut shown: Vec<(String, usize)> = rules.layer("land_value").map(|l| (rules.layers[l].name.clone(), l)).into_iter().collect();
        for c in &rules.complaints {
            if let Some(l) = rules.layer(&c.layer).filter(|l| !shown.iter().any(|(_, k)| k == l)) {
                shown.push((rules.layers[l].name.clone(), l));
            }
        }
        let mut names: Vec<String> = Vec::new();
        let mut out: Vec<DistrictInfo> = offices
            .iter()
            .map(|(at, _, _, u)| {
                let saved = self.districts.iter().find(|d| d.at == *at);
                let name = saved.and_then(|d| d.name.clone()).unwrap_or_else(|| Self::default_district_name(*at, &names));
                names.push(name.clone());
                let policy = saved.and_then(|d| d.policy.clone()).filter(|p| rules.district_policies.iter().any(|x| x.id == *p));
                DistrictInfo {
                    at: *at,
                    name,
                    office: rules.units[*u].name.clone(),
                    policy,
                    tiles: 0,
                    buildings: 0,
                    residents: 0.0,
                    jobs: 0.0,
                    satisfaction: 0.0,
                    maps: shown.iter().map(|(n, _)| (n.clone(), 0.0)).collect(),
                }
            })
            .collect();
        for (i, d) in map.iter().enumerate() {
            let Some(d) = d else { continue };
            let info = &mut out[*d as usize];
            info.tiles += 1;
            for (k, (_, l)) in shown.iter().enumerate() {
                info.maps[k].1 += gb.layers.get(*l).and_then(|v| v.get(i)).copied().unwrap_or(0.0);
            }
        }
        let mut weighed = vec![0.0f32; out.len()];
        for ((w, l), s) in self.layer.structures() {
            let (Some(u), Some(d)) = (s.unit.and_then(|u| rules.units.get(u as usize)), map[(l * width + w) as usize]) else { continue };
            let info = &mut out[d as usize];
            info.buildings += 1;
            info.residents += u.produce.get("residents").copied().unwrap_or(0.0);
            info.jobs += u.needs.get("workers").copied().unwrap_or(0.0);
            if rules.zone_of_unit(&u.id).is_some() {
                info.satisfaction += gb.unit_state(w, l, width).map_or(0.0, |st| st.satisfaction);
                weighed[d as usize] += 1.0;
            }
        }
        for (info, n) in out.iter_mut().zip(weighed) {
            info.satisfaction = if n > 0.0 { info.satisfaction / n } else { 0.0 };
            for m in &mut info.maps {
                m.1 /= info.tiles.max(1) as f32;
            }
        }
        out
    }

    /// The neighbourhood a tile lies in.
    pub fn district_at(&self, w: u32, l: u32) -> Option<DistrictInfo> {
        let d = (*self.district_map().get((l * self.width() + w) as usize)?)?;
        self.districts().into_iter().nth(d as usize)
    }

    fn district_mut(&mut self, at: (u32, u32)) -> Result<&mut District, String> {
        if !self.district_offices().iter().any(|(o, ..)| *o == at) {
            return Err(format!("no neighbourhood office at {}, {}", at.0, at.1));
        }
        if !self.districts.iter().any(|d| d.at == at) {
            self.districts.push(District { at, name: None, policy: None });
        }
        Ok(self.districts.iter_mut().find(|d| d.at == at).unwrap())
    }

    /// Names the neighbourhood of the office at a tile (an empty name
    /// gives it back the one it had by itself).
    pub fn rename_district(&mut self, at: (u32, u32), name: &str) -> Result<(), String> {
        let name: String = name.trim().chars().take(40).collect();
        self.district_mut(at)?.name = (!name.is_empty()).then_some(name);
        Ok(())
    }

    /// Sets the policy of a neighbourhood (`None`: no policy).
    pub fn set_district_policy(&mut self, at: (u32, u32), policy: Option<&str>) -> Result<(), String> {
        if let Some(p) = policy {
            if !self.rules().is_some_and(|r| r.district_policies.iter().any(|x| x.id == p)) {
                return Err(format!("no neighbourhood policy {p} in these rules"));
            }
        }
        self.district_mut(at)?.policy = policy.map(String::from);
        Ok(())
    }

    /// For every tile, the policy in force there (index + 1 into the
    /// rules' neighbourhood policies, 0 for none); empty when no
    /// neighbourhood has one.
    pub(crate) fn district_policy_map(&self) -> Vec<u8> {
        if self.districts.iter().all(|d| d.policy.is_none()) {
            return Vec::new();
        }
        let Some(rules) = self.rules() else { return Vec::new() };
        let by_office: Vec<u8> = self
            .district_offices()
            .iter()
            .map(|(at, ..)| {
                let id = self.districts.iter().find(|d| d.at == *at).and_then(|d| d.policy.as_deref());
                id.and_then(|id| rules.district_policies.iter().position(|p| p.id == id)).map_or(0, |i| i as u8 + 1)
            })
            .collect();
        if by_office.iter().all(|p| *p == 0) {
            return Vec::new();
        }
        self.district_map().into_iter().map(|d| d.map_or(0, |d| by_office[d as usize])).collect()
    }

    /// Month end: what the mayor set for offices that are gone is
    /// forgotten, and the policies are paid for.
    pub(crate) fn districts_month(&mut self) {
        if self.districts.is_empty() {
            return;
        }
        let standing: Vec<(u32, u32)> = self.district_offices().iter().map(|(at, ..)| *at).collect();
        self.districts.retain(|d| standing.contains(&d.at) && (d.name.is_some() || d.policy.is_some()));
        let Some(rules) = self.rules() else { return };
        let cost: f64 =
            self.districts.iter().filter_map(|d| rules.district_policies.iter().find(|p| Some(&p.id) == d.policy.as_ref())).map(|p| p.cost as f64).sum();
        if cost != 0.0 {
            self.fund -= cost.round() as i64;
            self.book("Neighbourhood policies", -cost);
        }
    }
}
