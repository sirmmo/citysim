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
