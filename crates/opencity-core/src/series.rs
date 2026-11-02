//! Columnar time series of a GlassBox run, for graphs and CSV export.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Ticks kept; older samples are dropped.
pub const SERIES_CAP: usize = 2000;

/// Named series sampled at the same ticks. A series that appears later is
/// back-filled with NaN, so all columns stay aligned with `ticks`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Series {
    pub names: Vec<String>,
    pub ticks: VecDeque<u64>,
    /// Gaps (NaN) are stored as `null`, which JSON can represent.
    #[serde(with = "gaps")]
    pub columns: Vec<VecDeque<f32>>,
}

mod gaps {
    use std::collections::VecDeque;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(cols: &[VecDeque<f32>], s: S) -> Result<S::Ok, S::Error> {
        let v: Vec<Vec<Option<f32>>> = cols.iter().map(|c| c.iter().map(|x| x.is_finite().then_some(*x)).collect()).collect();
        v.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<VecDeque<f32>>, D::Error> {
        let v: Vec<Vec<Option<f32>>> = Vec::deserialize(d)?;
        Ok(v.into_iter().map(|c| c.into_iter().map(|x| x.unwrap_or(f32::NAN)).collect()).collect())
    }
}

impl Series {
    /// Appends one sample per named value at `tick`; series missing from
    /// `values` get NaN at this tick.
    pub fn push<'a>(&mut self, tick: u64, values: impl IntoIterator<Item = (&'a str, f32)>) {
        let n = self.ticks.len();
        self.ticks.push_back(tick);
        for c in &mut self.columns {
            c.push_back(f32::NAN);
        }
        for (name, v) in values {
            let i = match self.names.iter().position(|x| x == name) {
                Some(i) => i,
                None => {
                    self.names.push(name.to_string());
                    let mut col = VecDeque::from(vec![f32::NAN; n + 1]);
                    col.reserve(SERIES_CAP.saturating_sub(n));
                    self.columns.push(col);
                    self.names.len() - 1
                }
            };
            *self.columns[i].back_mut().unwrap() = v;
        }
        while self.ticks.len() > SERIES_CAP {
            self.ticks.pop_front();
            for c in &mut self.columns {
                c.pop_front();
            }
        }
    }

    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ticks.is_empty()
    }

    pub fn column(&self, name: &str) -> Option<&VecDeque<f32>> {
        self.names.iter().position(|x| x == name).map(|i| &self.columns[i])
    }

    /// CSV with a `tick` column, then the named series (all when `only` is empty).
    pub fn to_csv(&self, only: &[&str]) -> String {
        let cols: Vec<usize> = (0..self.names.len()).filter(|&i| only.is_empty() || only.contains(&self.names[i].as_str())).collect();
        let mut out = String::from("tick");
        for &i in &cols {
            out.push(',');
            if self.names[i].contains([',', '"']) {
                out.push_str(&format!("\"{}\"", self.names[i].replace('"', "\"\"")));
            } else {
                out.push_str(&self.names[i]);
            }
        }
        out.push('\n');
        for (row, tick) in self.ticks.iter().enumerate() {
            out.push_str(&tick.to_string());
            for &i in &cols {
                let v = self.columns[i][row];
                out.push(',');
                if v.is_finite() {
                    out.push_str(&format!("{v:.3}"));
                }
            }
            out.push('\n');
        }
        out
    }
}

/// Summary statistics of a window of samples (NaN skipped).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub last: f32,
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    pub std_dev: f32,
    /// Least-squares slope, per 100 ticks.
    pub trend: f32,
    /// Ticks in the window where the value changed sign of its step:
    /// a measure of oscillation (0 = monotone).
    pub reversals: u32,
}

impl Stats {
    pub fn of(ticks: impl IntoIterator<Item = u64>, values: impl IntoIterator<Item = f32>) -> Option<Stats> {
        let pts: Vec<(f64, f64)> = ticks.into_iter().zip(values).filter(|(_, v)| v.is_finite()).map(|(t, v)| (t as f64, v as f64)).collect();
        let (&(_, last), n) = (pts.last()?, pts.len() as f64);
        let mean = pts.iter().map(|p| p.1).sum::<f64>() / n;
        let tm = pts.iter().map(|p| p.0).sum::<f64>() / n;
        let var = pts.iter().map(|p| (p.1 - mean).powi(2)).sum::<f64>() / n;
        let stt = pts.iter().map(|p| (p.0 - tm).powi(2)).sum::<f64>();
        let sty = pts.iter().map(|p| (p.0 - tm) * (p.1 - mean)).sum::<f64>();
        let mut reversals = 0;
        let mut dir = 0.0f64;
        for w in pts.windows(2) {
            let d = w[1].1 - w[0].1;
            if d.abs() > 1e-6 {
                if dir != 0.0 && d.signum() != dir {
                    reversals += 1;
                }
                dir = d.signum();
            }
        }
        Some(Stats {
            last: last as f32,
            min: pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min) as f32,
            max: pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max) as f32,
            mean: mean as f32,
            std_dev: var.sqrt() as f32,
            trend: if stt > 0.0 { (sty / stt * 100.0) as f32 } else { 0.0 },
            reversals,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_series_are_backfilled_and_capped() {
        let mut s = Series::default();
        s.push(1, [("a", 1.0)]);
        s.push(2, [("a", 2.0), ("b", 5.0)]);
        assert_eq!(s.column("b").unwrap().len(), 2);
        assert!(s.column("b").unwrap()[0].is_nan());
        assert_eq!(s.to_csv(&[]), "tick,a,b\n1,1.000,\n2,2.000,5.000\n");
        for t in 3..(SERIES_CAP as u64 + 10) {
            s.push(t, [("a", t as f32)]);
        }
        assert_eq!(s.len(), SERIES_CAP);
        assert!(s.columns.iter().all(|c| c.len() == SERIES_CAP));
    }

    #[test]
    fn gaps_survive_json() {
        let mut s = Series::default();
        s.push(1, [("a", 1.0)]);
        s.push(2, [("b", 2.0)]);
        let back: Series = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.to_csv(&[]), s.to_csv(&[]));
        assert!(back.column("a").unwrap()[1].is_nan());
    }

    #[test]
    fn stats_measure_trend_and_oscillation() {
        let st = Stats::of(0..10, (0..10).map(|i| i as f32)).unwrap();
        assert!((st.trend - 100.0).abs() < 1e-3 && st.reversals == 0 && st.max == 9.0);
        let flick = Stats::of(0..10, (0..10).map(|i| (i % 2) as f32)).unwrap();
        assert_eq!(flick.reversals, 8);
    }
}
