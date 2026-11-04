//! Scenarios and goals: how a game starts, and what wins or loses it.
//!
//! A scenario file (`<data>/scenarios/<id>.toml`) says how the city starts
//! (a generated map or a GeoJSON import, rules, skin, money) and lists its
//! goals. Each month the city measures every goal: a goal with a deadline
//! must be reached within its years, an `always` goal must hold the whole
//! game. Every goal reached (and every `always` goal holding) wins; a
//! deadline missed or an `always` goal broken loses.

use serde::{Deserialize, Serialize};

use crate::City;

/// A scenario file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub start: Start,
    #[serde(default, rename = "goal")]
    pub goals: Vec<GoalDef>,
}

/// How a scenario's city starts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Start {
    pub width: u32,
    pub length: u32,
    pub seed: u64,
    /// plain, hill, mountain.
    pub relief: String,
    /// dry, lake, coast.
    pub water: String,
    /// none, island, volcano, crater.
    pub shape: String,
    /// sparse, normal, dense.
    pub trees: String,
    /// An empty flat map instead (relief, water, shape and trees ignored).
    pub flat: bool,
    /// Build the city from a GeoJSON file instead (relative to the data folder).
    pub geojson: Option<String>,
    /// Keep only what existed on this date (GeoJSON imports).
    pub date: Option<String>,
    /// GlassBox rules id (none: classic rules).
    pub rules: Option<String>,
    /// Skin id the scenario is dressed in.
    pub skin: Option<String>,
    /// Starting money (default: as for any new city).
    pub fund: Option<i64>,
    /// Lay out the demo town.
    pub demo: bool,
    /// Start with every research project done.
    pub research_all: bool,
}

impl Default for Start {
    fn default() -> Self {
        Self {
            width: 64,
            length: 64,
            seed: 1,
            relief: "plain".into(),
            water: "lake".into(),
            shape: "none".into(),
            trees: "sparse".into(),
            flat: false,
            geojson: None,
            date: None,
            rules: None,
            skin: None,
            fund: None,
            demo: false,
            research_all: false,
        }
    }
}

/// One goal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalDef {
    pub name: String,
    /// What is measured: `population`, `fund`, `buildings` (developed),
    /// `research:<project>` (1 when done), `unit:<unit id>` (how many),
    /// `structure:<kind>` (station, harbour, school…: how many),
    /// `produced:<resource>` or `unmet:<resource>` (per tick).
    pub metric: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_least: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_most: Option<f64>,
    /// Years from the start to reach it (none: no deadline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within_years: Option<u32>,
    /// Must hold every month of the game (never in debt…).
    #[serde(default)]
    pub always: bool,
}

/// Where the game stands.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum GoalStatus {
    #[default]
    Playing,
    Won,
    /// Lost, and why.
    Lost(String),
}

/// A city's scenario and how its goals are going (saved with the city).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Goals {
    pub scenario: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub goals: Vec<GoalDef>,
    /// Year the scenario started.
    pub start_year: u32,
    /// Goals reached at some point (a deadline goal stays reached).
    pub reached: Vec<bool>,
    #[serde(default)]
    pub status: GoalStatus,
}

impl GoalDef {
    pub fn holds(&self, v: f64) -> bool {
        self.at_least.is_none_or(|t| v >= t) && self.at_most.is_none_or(|t| v <= t)
    }

    /// The target, for display: "≥ 5000", "≤ 0"…
    pub fn target(&self) -> String {
        match (self.at_least, self.at_most) {
            (Some(a), Some(b)) => format!("{a} to {b}"),
            (Some(a), None) => format!("≥ {a}"),
            (None, Some(b)) => format!("≤ {b}"),
            (None, None) => "—".into(),
        }
    }
}

impl City {
    /// Starts a scenario's goals now.
    pub fn set_goals(&mut self, s: &Scenario) {
        self.goals = Some(Goals {
            scenario: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            goals: s.goals.clone(),
            start_year: self.date.year,
            reached: vec![false; s.goals.len()],
            status: GoalStatus::Playing,
        });
    }

    /// The current value of a goal's metric.
    pub fn goal_value(&self, metric: &str) -> f64 {
        let rules = self.rules();
        let res = |id: &str| rules.and_then(|r| r.resource(id));
        let (kind, arg) = metric.split_once(':').unwrap_or((metric, ""));
        match kind {
            "population" => self.population as f64,
            "fund" => self.fund as f64,
            "buildings" => self.layer.structures().filter(|(_, s)| s.unit.is_some() || (s.code.is_zone() && s.level > 0)).count() as f64,
            "research" => self.glassbox.as_ref().is_some_and(|g| g.researched.contains(arg)) as u8 as f64,
            "unit" => {
                let Some(u) = rules.and_then(|r| r.unit(arg)) else { return 0.0 };
                self.layer.structures().filter(|(_, s)| s.unit == Some(u as u16)).count() as f64
            }
            "structure" => self.layer.structures().filter(|(_, s)| format!("{:?}", s.code).eq_ignore_ascii_case(arg)).count() as f64,
            "produced" => res(arg).and_then(|i| self.glassbox.as_ref().and_then(|g| g.balances.produced.get(i).copied())).unwrap_or(0.0) as f64,
            "unmet" => res(arg).and_then(|i| self.glassbox.as_ref().and_then(|g| g.balances.unmet.get(i).copied())).unwrap_or(0.0) as f64,
            _ => 0.0,
        }
    }

    /// Monthly check of the goals: reached, won or lost.
    pub(crate) fn check_goals(&mut self) {
        let Some(g) = self.goals.clone() else { return };
        if g.status != GoalStatus::Playing {
            return;
        }
        let years = self.date.year.saturating_sub(g.start_year);
        let mut reached = g.reached.clone();
        let mut lost = None;
        for (i, goal) in g.goals.iter().enumerate() {
            let ok = goal.holds(self.goal_value(&goal.metric));
            if goal.always {
                if !ok {
                    lost = Some(format!("{} failed", goal.name));
                }
                reached[i] = ok;
                continue;
            }
            if ok && !reached[i] {
                reached[i] = true;
                self.messages.push(format!("Goal reached: {}", goal.name));
            }
            if !reached[i] && goal.within_years.is_some_and(|y| years >= y) {
                lost = Some(format!("{}: out of time", goal.name));
            }
        }
        let status = match lost {
            Some(why) => {
                self.messages.push(format!("Scenario lost. {why}."));
                GoalStatus::Lost(why)
            }
            None if !reached.is_empty() && reached.iter().all(|&r| r) => {
                self.messages.push(format!("Scenario won: {}!", g.name));
                GoalStatus::Won
            }
            None => GoalStatus::Playing,
        };
        if let Some(goals) = self.goals.as_mut() {
            goals.reached = reached;
            goals.status = status;
        }
    }
}
