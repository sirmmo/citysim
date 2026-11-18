//! Events the rules define (`[[event]]`): things that happen to a city,
//! some with a choice to make. A strike, a festival, a developer's offer
//! come up by themselves now and then; an `external` event waits to be set
//! off from outside (a game this city is part of, the Disasters menu, the
//! MCP server): a government falls, and its offices close.
//!
//! What an event or a choice does is an `effect`: money now or month by
//! month, points of approval, units made obsolete, and tools that build
//! something else from now on.

use serde::{Deserialize, Serialize};

use crate::glassbox::OBSOLETE_GRACE;
use crate::rules::{EffectDef, EventDef};
use crate::City;

/// Months a question waits for an answer before the last choice is taken.
pub const ANSWER_MONTHS: u8 = 3;
/// Months of quiet after an event before another can come up by itself.
pub const QUIET_MONTHS: u8 = 4;
/// The odds of an event in a month, once the quiet is over.
pub const ODDS: f32 = 0.25;
/// What decisions weigh on approval at most, either way.
pub const MOOD_MAX: f32 = 20.0;

/// An event waiting for the mayor's answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub id: String,
    pub months_left: u8,
    /// Thousands of residents when it came up: what its sums are times.
    pub scale: f32,
}

/// Money that comes or goes each month for a while.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub label: String,
    pub monthly: i64,
    pub months: u32,
}

/// Where the city's affairs stand (saved with the city).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Affairs {
    /// Events come up by themselves (off: only when set off from outside).
    pub on: bool,
    pub pending: Option<Pending>,
    /// Points of approval from recent decisions; they fade month by month.
    pub mood: f32,
    pub flows: Vec<Flow>,
    /// Months since the last event.
    pub quiet: u8,
    /// Every event that happened, oldest first.
    pub past: Vec<String>,
}

impl Default for Affairs {
    fn default() -> Self {
        Self { on: true, pending: None, mood: 0.0, flows: Vec::new(), quiet: 0, past: Vec::new() }
    }
}

impl Affairs {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A choice as the player sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Option_ {
    pub label: String,
    /// What it does, in words ("-$1,200 · approval +4"); for a gamble, both ways.
    pub does: String,
}

/// The event on the mayor's desk.
#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub id: String,
    pub name: String,
    pub text: String,
    pub months_left: u8,
    pub options: Vec<Option_>,
}

fn dollars(v: i64) -> String {
    let digits = v.abs().to_string();
    let mut s = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            s.push(',');
        }
        s.push(c);
    }
    format!("{}${s}", if v < 0 { "-" } else { "+" })
}

/// A sum of the rules (per thousand residents) for a city this size, to the nearest $10.
fn sum(per_thousand: f32, scale: f32) -> i64 {
    ((per_thousand * scale / 10.0).round() * 10.0) as i64
}

impl City {
    fn event_def(&self, id: &str) -> Option<EventDef> {
        self.rules()?.events.iter().find(|e| e.id == id).cloned()
    }

    fn unit_name(&self, id: &str) -> String {
        self.rules().and_then(|r| r.unit(id).map(|u| r.units[u].name.clone())).unwrap_or_else(|| id.to_string())
    }

    /// An effect in words.
    pub fn effect_words(&self, e: &EffectDef, scale: f32) -> String {
        let mut parts = Vec::new();
        if sum(e.money, scale) != 0 {
            parts.push(dollars(sum(e.money, scale)));
        }
        if sum(e.monthly, scale) != 0 && e.months > 0 {
            parts.push(format!("{} a month for {} months", dollars(sum(e.monthly, scale)), e.months));
        }
        if e.approval != 0.0 {
            parts.push(format!("approval {:+.0}", e.approval));
        }
        if !e.obsoletes.is_empty() {
            let names: Vec<String> = e.obsoletes.keys().map(|u| self.unit_name(u)).collect();
            let when = if e.grace == 0 { "at once".to_string() } else { format!("in {} months", e.grace) };
            parts.push(format!("closes {} {when}", names.join(", ")));
        }
        if !e.services.is_empty() {
            let names: Vec<String> = e.services.values().map(|u| self.unit_name(u)).collect();
            parts.push(format!("brings {}", names.join(", ")));
        }
        if parts.is_empty() {
            "nothing".to_string()
        } else {
            parts.join(" · ")
        }
    }

    /// The event waiting for an answer, with its choices in words.
    pub fn question(&self) -> Option<Question> {
        let p = self.affairs.pending.as_ref()?;
        let def = self.event_def(&p.id)?;
        let options = def
            .choices
            .iter()
            .map(|c| {
                let does = self.effect_words(&c.effect, p.scale);
                let does = if c.chance > 0.0 && c.chance < 1.0 {
                    format!("{:.0}%: {does}; otherwise {}", c.chance * 100.0, self.effect_words(&c.else_effect, p.scale))
                } else {
                    does
                };
                Option_ { label: c.label.clone(), does }
            })
            .collect();
        Some(Question { id: def.id, name: def.name, text: def.text, months_left: p.months_left, options })
    }

    /// A number in 0..1 that follows from the city's month and state, not
    /// from the simulation's dice: events never change how a seed plays out.
    fn chance(&self, salt: u64) -> f32 {
        let mut x =
            (self.month_index() as u64) << 32 ^ (self.population as u64) << 8 ^ (self.fund as u64).rotate_left(17) ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        x ^= x >> 30;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x ^= x >> 27;
        x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }

    fn apply_effect(&mut self, label: &str, e: &EffectDef, scale: f32) {
        let now = sum(e.money, scale);
        if now != 0 {
            self.fund += now;
            self.book("Events", now as f64);
        }
        let monthly = sum(e.monthly, scale);
        if monthly != 0 && e.months > 0 {
            self.affairs.flows.push(Flow { label: label.to_string(), monthly, months: e.months });
        }
        self.affairs.mood = (self.affairs.mood + e.approval).clamp(-MOOD_MAX, MOOD_MAX);
        if let Some(gb) = self.glassbox.as_mut() {
            let due = gb.tick + e.grace as u64 * OBSOLETE_GRACE / 12;
            for (unit, becomes) in &e.obsoletes {
                gb.obsolete.entry(unit.clone()).or_insert(due);
                gb.retired.insert(unit.clone(), (becomes.clone(), label.to_string()));
            }
            gb.services.extend(e.services.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }

    /// Makes an event of the rules happen now, whatever its conditions:
    /// what it does is done, and its question (if it has one) is put to
    /// the mayor. A question still open is answered with its last choice first.
    pub fn trigger_event(&mut self, id: &str) -> Result<(), String> {
        let def = self.event_def(id).ok_or_else(|| format!("no event {id} in these rules"))?;
        if let Some(q) = self.question() {
            self.decide(q.options.len().saturating_sub(1))?;
        }
        let scale = (self.population.max(500) as f32) / 1000.0;
        self.messages.push(format!("{}: {}", def.name, def.text));
        self.apply_effect(&def.name, &def.effect, scale);
        self.affairs.past.push(def.id.clone());
        self.affairs.quiet = 0;
        if !def.choices.is_empty() {
            self.affairs.pending = Some(Pending { id: def.id, months_left: ANSWER_MONTHS, scale });
        }
        Ok(())
    }

    /// Answers the open question with one of its choices; returns what
    /// came of it.
    pub fn decide(&mut self, choice: usize) -> Result<String, String> {
        let p = self.affairs.pending.clone().ok_or("nothing is waiting for a decision")?;
        let Some(def) = self.event_def(&p.id) else {
            self.affairs.pending = None;
            return Err(format!("these rules no longer have the event {}", p.id));
        };
        let c = def.choices.get(choice).ok_or_else(|| format!("{} has {} choices", def.name, def.choices.len()))?.clone();
        let lucky = c.chance <= 0.0 || c.chance >= 1.0 || self.chance(choice as u64 + 1) < c.chance;
        let (effect, text) = if lucky { (&c.effect, &c.text) } else { (&c.else_effect, &c.else_text) };
        self.affairs.pending = None;
        self.apply_effect(&def.name, effect, p.scale);
        let said = if text.is_empty() { format!("{}: {}.", def.name, c.label) } else { format!("{}: {text}", def.name) };
        self.messages.push(said.clone());
        Ok(said)
    }

    /// The events that could come up by themselves this month.
    fn candidates(&self) -> Vec<EventDef> {
        let Some(rules) = self.rules() else { return Vec::new() };
        let standing = |unit: &str| rules.unit(unit).is_some_and(|u| self.layer.structures().any(|(_, s)| s.unit == Some(u as u16)));
        rules
            .events
            .iter()
            .filter(|e| !e.external && e.weight > 0.0 && self.population >= e.min_population)
            .filter(|e| !(e.once && self.affairs.past.contains(&e.id)) && self.affairs.past.last() != Some(&e.id))
            .filter(|e| e.requires.is_empty() || e.requires.iter().any(|u| standing(u)))
            .cloned()
            .collect()
    }

    /// Month end: money of past decisions comes and goes, their weight on
    /// approval fades, an unanswered question is settled, and now and then
    /// something new comes up.
    pub(crate) fn affairs_month(&mut self) {
        let mut flows = std::mem::take(&mut self.affairs.flows);
        for f in &mut flows {
            self.fund += f.monthly;
            self.book(&format!("Events: {}", f.label), f.monthly as f64);
            f.months -= 1;
        }
        flows.retain(|f| f.months > 0);
        self.affairs.flows = flows;
        self.affairs.mood *= 0.9;
        if self.affairs.mood.abs() < 0.05 {
            self.affairs.mood = 0.0;
        }
        self.affairs.quiet = self.affairs.quiet.saturating_add(1);
        if let Some(p) = self.affairs.pending.as_mut() {
            p.months_left = p.months_left.saturating_sub(1);
            if p.months_left == 0 {
                let last = self.question().map_or(0, |q| q.options.len().saturating_sub(1));
                if self.decide(last).is_err() {
                    self.affairs.pending = None;
                }
            }
            return;
        }
        if !self.affairs.on || self.affairs.quiet < QUIET_MONTHS || self.ousted() || self.chance(0) >= ODDS {
            return;
        }
        let all = self.candidates();
        let total: f32 = all.iter().map(|e| e.weight).sum();
        if total <= 0.0 {
            return;
        }
        let mut pick = self.chance(7) * total;
        let chosen = all.iter().find(|e| {
            pick -= e.weight;
            pick < 0.0
        });
        if let Some(id) = chosen.or(all.last()).map(|e| e.id.clone()) {
            let _ = self.trigger_event(&id);
        }
    }

    /// The unit a service tool builds today: what the rules say, unless an
    /// event has put something else in its place.
    pub fn service(&self, tool_id: &str) -> Option<&crate::rules::UnitDef> {
        let rules = self.rules()?;
        let id = self.glassbox.as_ref().and_then(|g| g.services.get(tool_id)).or_else(|| rules.services.get(tool_id))?;
        rules.unit(id).map(|u| &rules.units[u])
    }
}
