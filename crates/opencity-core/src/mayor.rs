//! The mayor's standing with the city: an approval rating, and the vote
//! that ends a term that went badly.
//!
//! Each month the city works out what people make of their mayor: how well
//! the buildings' needs are met, less what debt, people leaving, recent
//! disasters and the family in town cost. The rating moves a fifth of the
//! way there. In a city of [`VOTERS`] people or more, a rating that stays
//! under [`OUST_BELOW`] for [`OUST_MONTHS`] months in a row ends in a vote
//! of no confidence: the mayor is out, and the city stands still until the
//! player decides what next (see [`City::stay_in_office`]).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::{City, Date, Status, StructureCode};

/// Approval under this share counts against the mayor.
pub const OUST_BELOW: f32 = 0.30;
/// Months in a row under it before the vote.
pub const OUST_MONTHS: u8 = 12;
/// Smaller places than this have no vote.
pub const VOTERS: u32 = 500;

/// News kept for the player to read again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct News {
    pub date: Date,
    pub text: String,
}

/// News the city keeps.
pub const NEWS_KEPT: usize = 80;

/// Something residents complain about, as it stands now.
#[derive(Clone, Debug, PartialEq)]
pub struct Complaint {
    pub name: String,
    /// Share of residents living with it, 0..1.
    pub share: f32,
    /// Approval it costs now, in points out of 100.
    pub points: f32,
    /// The home that has it worst.
    pub at: Option<(u32, u32)>,
    pub advice: String,
}

/// Something the mayor should look at.
#[derive(Clone, Debug, PartialEq)]
pub struct Advice {
    pub text: String,
    /// A place to look, when there is one.
    pub at: Option<(u32, u32)>,
}

pub(crate) fn buildings(n: u32) -> String {
    if n == 1 {
        "1 building".to_string()
    } else {
        format!("{n} buildings")
    }
}

/// The mayor's standing (saved with the city).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mayor {
    /// Share of the city behind the mayor, 0..1.
    pub approval: f32,
    /// Months in a row under [`OUST_BELOW`].
    pub low_months: u8,
    /// The city can vote its mayor out (off: mayor for life).
    pub elections: bool,
    /// Points lost to recent disasters; they fade month by month.
    pub shock: f32,
    /// Population at the end of each of the last 36 months.
    pub past: VecDeque<u32>,
    /// Voted out on this day: time stands still and nothing can be built.
    pub ousted: Option<Date>,
}

impl Default for Mayor {
    fn default() -> Self {
        Self { approval: 0.6, low_months: 0, elections: true, shock: 0.0, past: VecDeque::new(), ousted: None }
    }
}

impl Mayor {
    /// Months left before the vote, once the rating counts against the mayor.
    pub fn months_left(&self) -> Option<u8> {
        (self.low_months > 0).then(|| OUST_MONTHS.saturating_sub(self.low_months))
    }
}

impl City {
    /// What the city makes of its mayor right now, in points out of 100:
    /// what each thing adds or takes away, the first being where it starts.
    pub fn approval_factors(&self) -> Vec<(String, f32)> {
        let mut v = Vec::new();
        let width = self.width();
        let zones = self.layer.structures().filter(|(_, s)| s.code.is_zone());
        match &self.glassbox {
            Some(gb) => {
                // Homes count double: it is the people living there who vote.
                let (mut sum, mut weight) = (0.0f32, 0.0f32);
                for (p, s) in zones.filter(|(_, s)| s.unit.is_some()) {
                    let Some(st) = gb.units.get(&(p.1 * width + p.0)) else { continue };
                    let homes = if s.code == StructureCode::Residential { 2.0 } else { 1.0 };
                    let w = homes * (1.0 + s.level as f32);
                    sum += st.satisfaction * w;
                    weight += w;
                }
                if weight > 0.0 {
                    v.push(("Needs met in homes, shops and works".to_string(), sum / weight * 100.0));
                } else if self.best_of_late() > 0 {
                    v.push(("Nobody is left".to_string(), 0.0));
                } else {
                    v.push(("A new mayor's welcome".to_string(), 60.0));
                }
            }
            None => {
                let (mut n, mut lit) = (0u32, 0u32);
                for (p, _) in zones {
                    n += 1;
                    lit += self.layer.tile(p.0, p.1).is_some_and(|t| t.status.has(Status::POWER)) as u32;
                }
                if n > 0 {
                    v.push(("Zones with power".to_string(), 30.0 + 50.0 * lit as f32 / n as f32));
                } else {
                    v.push(("A new mayor's welcome".to_string(), 60.0));
                }
                if self.values.power < 0 {
                    v.push(("Power cuts".to_string(), -15.0));
                }
            }
        }
        if self.fund < 0 {
            v.push(("The city is in debt".to_string(), -10.0));
        }
        let before = self.best_of_late();
        // A few moving away is no news; one in twenty is.
        if (self.population as f32) < before as f32 * 0.95 {
            let gone = (before - self.population) as f32 / before as f32;
            v.push(("People have left these last years".to_string(), -(gone * 100.0).min(40.0)));
        }
        if let Some(gb) = self.glassbox.as_ref().filter(|gb| gb.homeless >= 1.0 && self.population > 0) {
            let share = gb.homeless / (self.population as f32 + gb.homeless);
            v.push(("People without a home".to_string(), -(share * 200.0).min(20.0)));
        }
        for c in self.complaints().into_iter().filter(|c| c.points >= 0.5) {
            v.push((format!("{} ({:.0}% of residents)", c.name, c.share * 100.0), -c.points));
        }
        if self.mayor.shock >= 0.5 {
            v.push(("Recent disasters".to_string(), -self.mayor.shock));
        }
        if let Some(m) = &self.mafia {
            v.push(("The family is in town".to_string(), -(m.heat * 15.0).min(30.0)));
        }
        v
    }

    /// The most people the city had at a month's end these last three years.
    fn best_of_late(&self) -> u32 {
        self.mayor.past.iter().copied().max().unwrap_or(0)
    }

    /// What residents complain about where they live (the rules'
    /// `[[complaint]]`s), each with the share of residents living with it.
    pub fn complaints(&self) -> Vec<Complaint> {
        let (Some(gb), Some(rules)) = (&self.glassbox, self.rules()) else { return Vec::new() };
        if rules.complaints.is_empty() {
            return Vec::new();
        }
        let width = self.width();
        let pop = rules.population.as_deref();
        let homes: Vec<((u32, u32), f32)> = self
            .layer
            .structures()
            .filter(|(_, s)| s.code == StructureCode::Residential)
            .filter_map(|(p, s)| {
                let def = rules.units.get(s.unit? as usize)?;
                Some((p, pop.and_then(|r| def.produce.get(r)).copied().unwrap_or(1.0)))
            })
            .collect();
        let all: f32 = homes.iter().map(|(_, n)| n).sum();
        if all <= 0.0 {
            return Vec::new();
        }
        rules
            .complaints
            .iter()
            .filter_map(|c| {
                let li = rules.layer(&c.layer)?;
                // How far past the line a home is; the worst one is shown.
                let (mut with, mut worst) = (0.0f32, None::<(f32, (u32, u32))>);
                for (p, n) in &homes {
                    let v = gb.layer_value(li, p.0, p.1, width);
                    let past = c.above.map(|a| v - a).or(c.below.map(|b| b - v)).unwrap_or(0.0);
                    if past > 0.0 {
                        with += n;
                        if worst.is_none_or(|(w, _)| past > w) {
                            worst = Some((past, *p));
                        }
                    }
                }
                let share = with / all;
                Some(Complaint { name: c.name.clone(), share, points: share * c.points, at: worst.map(|(_, p)| p), advice: c.advice.clone() })
            })
            .collect()
    }

    /// What the mayor should look at now, most pressing first.
    pub fn advice(&self) -> Vec<Advice> {
        let mut v = Vec::new();
        let mut say = |text: String, at: Option<(u32, u32)>| v.push(Advice { text, at });
        if self.ousted() {
            say("You have been voted out of office.".into(), None);
        } else if let Some(left) = self.mayor.months_left() {
            say(
                format!(
                    "Approval is at {:.0}%: the council votes you out in {left} months unless it is back above {:.0}%.",
                    self.mayor.approval * 100.0,
                    OUST_BELOW * 100.0
                ),
                None,
            );
        }
        if self.fund < 0 {
            say("The city is in debt: nothing can be built. Raise taxes, cut funding or take a loan (Budget).".into(), None);
        }
        if let Some(m) = &self.mafia {
            say(format!("The family is in town (hold {:.0}%): police stations drive them out.", m.heat * 100.0 / 3.0), None);
        }
        if let (Some(gb), Some(rules)) = (&self.glassbox, self.rules()) {
            use crate::rules::Transport;
            let b = &gb.balances;
            for (i, r) in rules.resources.iter().enumerate() {
                if matches!(r.transport, Transport::City | Transport::Pool | Transport::Local) || r.parent.is_some() {
                    continue;
                }
                let (got, missing) = (b.delivered.get(i).copied().unwrap_or(0.0), b.unmet.get(i).copied().unwrap_or(0.0));
                let wanted = got + missing;
                if wanted >= 5.0 && missing > wanted * 0.15 {
                    say(format!("{} short: {got:.0} of {wanted:.0} wanted are delivered.", r.name), None);
                }
            }
            if rules.people.is_some() {
                if gb.jobless > 0.1 {
                    say(format!("{:.0}% of workers have no job: zone works and shops, or crime follows.", gb.jobless * 100.0), None);
                }
                if gb.homeless >= 5.0 {
                    say(format!("{:.0} people have no home: zone homes where there is room, or they leave.", gb.homeless), None);
                }
            }
            let mut complaints = self.complaints();
            complaints.sort_by(|a, b| b.points.total_cmp(&a.points));
            for c in complaints.into_iter().filter(|c| c.share >= 0.15) {
                let advice = if c.advice.is_empty() { String::new() } else { format!(" {}", c.advice) };
                say(format!("{}: {:.0}% of residents live with it.{advice}", c.name, c.share * 100.0), c.at);
            }
            // Plants that can fail, run on too little.
            for d in rules.disasters.iter().filter(|d| d.source.is_some()) {
                let id = d.source.as_deref().unwrap();
                let Some(u) = rules.unit(id) else { continue };
                let at = self.layer.structures().find(|(_, s)| s.unit == Some(u as u16)).map(|(p, _)| p);
                if at.is_some() && gb.funding_of(id) < 0.95 {
                    say(
                        format!("The {} runs on {:.0}% of its budget: an accident is likelier.", rules.units[u].name.to_lowercase(), gb.funding_of(id) * 100.0),
                        at,
                    );
                }
            }
            if let Some((z, d)) = rules.zones.iter().zip(&gb.demand).max_by(|a, b| a.1.total_cmp(b.1)).filter(|(_, d)| **d > 25.0) {
                say(format!("{} land is wanted (demand {d:.0}): zone more of it.", z.name), None);
            }
        } else if self.values.power < 0 {
            say("Power is short: build another plant.".into(), None);
        }
        v
    }

    /// Keeps a line of news for the player to read again.
    pub fn report(&mut self, text: String) {
        // Said twice (by hand and by the day's round): kept once.
        if self.news.iter().rev().take(3).any(|n| n.date == self.date && n.text == text) {
            return;
        }
        self.news.push_back(News { date: self.date, text });
        while self.news.len() > NEWS_KEPT {
            self.news.pop_front();
        }
    }

    /// Files what was said since `(messages, notices)` were this long.
    pub(crate) fn keep_news(&mut self, since: (usize, usize)) {
        let said: Vec<String> = self.messages.iter().skip(since.0).cloned().collect();
        let struck: Vec<String> =
            self.notices.iter().skip(since.1).map(|e| format!("{} at {}, {}: {} hit.", e.name, e.at.0, e.at.1, buildings(e.hit))).collect();
        for text in struck.into_iter().chain(said) {
            self.report(text);
        }
    }

    /// Where the approval rating is heading, 0..1.
    pub fn approval_target(&self) -> f32 {
        (self.approval_factors().iter().map(|(_, p)| p).sum::<f32>() / 100.0).clamp(0.0, 1.0)
    }

    /// A disaster struck: the city holds it against the mayor for a while,
    /// the more the more was lost (and most of all land lost for years).
    pub(crate) fn shaken(&mut self, e: &crate::glassbox::DisasterEvent) {
        let fallout = self.rules().and_then(|r| r.disaster(&e.id)).is_some_and(|d| d.fallout.is_some());
        let points = if fallout { 25.0 } else { (e.hit as f32).min(8.0) };
        self.mayor.shock = (self.mayor.shock + points).min(40.0);
    }

    /// Month end: the rating moves toward what people think now, and a
    /// year of it under the line ends in a vote.
    pub(crate) fn mayor_month(&mut self) {
        let target = self.approval_target();
        let m = &mut self.mayor;
        m.approval += (target - m.approval) * 0.2;
        m.shock *= 0.85;
        m.past.push_back(self.population);
        while m.past.len() > 36 {
            m.past.pop_front();
        }
        // Those who left still have their say.
        let voters = m.past.iter().copied().max().unwrap_or(0);
        if !m.elections || voters < VOTERS {
            m.low_months = 0;
            return;
        }
        let pct = (m.approval * 100.0).round();
        if m.approval >= OUST_BELOW {
            if m.low_months > 0 && m.low_months <= 2 {
                self.messages.push(format!("Approval is back at {pct}%: the council lets the matter rest."));
            }
            m.low_months = m.low_months.saturating_sub(2);
            return;
        }
        m.low_months += 1;
        let left = OUST_MONTHS.saturating_sub(m.low_months);
        if left == 0 {
            m.ousted = Some(self.date);
            self.messages.push(format!("Vote of no confidence: with {pct}% behind you, the city has voted you out of office."));
        } else if m.low_months == 1 || left <= 3 || left == 6 {
            let months = if left == 1 { "1 month".to_string() } else { format!("{left} months") };
            self.messages.push(format!(
                "Only {pct}% of the city is behind you. The council votes you out in {months} unless approval is back above {:.0}%.",
                OUST_BELOW * 100.0
            ));
        }
    }

    /// The mayor is out: the city waits for the player.
    pub fn ousted(&self) -> bool {
        self.mayor.ousted.is_some()
    }

    /// After being voted out: back in office after a new campaign (the
    /// rating starts over and the city can vote again), or as mayor for
    /// life, with no more votes.
    pub fn stay_in_office(&mut self, for_life: bool) {
        let m = &mut self.mayor;
        m.ousted = None;
        m.low_months = 0;
        if for_life {
            m.elections = false;
        } else {
            m.approval = m.approval.max(0.5);
            m.shock = 0.0;
        }
    }
}
