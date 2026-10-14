use std::collections::HashMap;
use std::path::Path;

use crate::calib::Fit;
use crate::data::{Obs, Panel, Table};
use crate::engine::{Engine, MODEL};
use crate::eval::{Bands, Regime};
use crate::stats::norm_inv;

#[derive(Clone, Debug, Default)]
pub struct TruthScore {
    pub n: usize,
    pub mae_model: f64,
    pub rmse_model: f64,
    pub mae_stale: f64,
    pub rmse_stale: f64,
    pub cover_gaussian: [f64; 4],
    pub cover: [f64; 4],
}

pub struct TruthReport {
    pub all: TruthScore,
    pub by_staleness: Vec<(String, TruthScore)>,
}

#[derive(Default)]
struct Acc {
    n: usize,
    ae_model: f64,
    ae_stale: f64,
    se_model: f64,
    se_stale: f64,
    hit_gaussian: [usize; 4],
    hit: [usize; 4],
}

impl Acc {
    fn add(&mut self, truth: f64, mean: f64, sd: f64, stale: f64, bands: &Bands) {
        let e = truth - mean;
        self.n += 1;
        self.ae_model += e.abs();
        self.ae_stale += (truth - stale).abs();
        self.se_model += e * e;
        self.se_stale += (truth - stale).powi(2);
        if sd > 0.0 {
            let regime = Regime::of(mean);
            for (k, level) in [0.50, 0.80, 0.90, 0.95].iter().enumerate() {
                if e.abs() <= norm_inv(0.5 + level / 2.0) * sd {
                    self.hit_gaussian[k] += 1;
                }
                if e.abs() <= bands.half_width(regime, k, sd) {
                    self.hit[k] += 1;
                }
            }
        }
    }

    fn score(&self) -> TruthScore {
        if self.n == 0 {
            return TruthScore::default();
        }
        let n = self.n as f64;
        let (mut cover, mut cover_gaussian) = ([0.0; 4], [0.0; 4]);
        for k in 0..4 {
            cover[k] = self.hit[k] as f64 / n;
            cover_gaussian[k] = self.hit_gaussian[k] as f64 / n;
        }
        TruthScore {
            n: self.n,
            mae_model: self.ae_model / n,
            rmse_model: (self.se_model / n).sqrt(),
            mae_stale: self.ae_stale / n,
            rmse_stale: (self.se_stale / n).sqrt(),
            cover_gaussian,
            cover,
        }
    }
}

pub fn score(panel: &Panel, level: &[f64], fit: &Fit, bands: &Bands, dir: &Path) -> Result<TruthReport, String> {
    let t = Table::read(&dir.join("truth_prices.tsv"))?;
    let (ck, cc, cd, cp) = (t.col("key")?, t.col("class")?, t.col("date")?, t.col("price")?);
    let mut truth: HashMap<(String, String), Vec<(usize, f64)>> = HashMap::new();
    for r in 0..t.len() {
        let day = panel.day_index(t.int(r, cd) as i32);
        truth.entry((t.str(r, ck).to_string(), t.str(r, cc).to_string())).or_default().push((day, t.num(r, cp)));
    }

    let engine = Engine::new(panel, level);
    let insts: Vec<usize> = (0..panel.instruments.len()).collect();
    let end = panel.days.len() - 1;
    let runs = engine.run(fit, MODEL, &insts, &|_: &Obs| true, end, true);

    let edges = [0usize, 10, 30, 60, 100];
    let labels = ["0-9 days", "10-29 days", "30-59 days", "60-99 days"];
    let mut all = Acc::default();
    let mut buckets: Vec<Acc> = (0..labels.len()).map(|_| Acc::default()).collect();

    for r in &runs {
        let inst = &panel.instruments[r.inst];
        let key = (panel.borrowers[inst.borrower].key.clone(), inst.class.as_str().to_string());
        let Some(series) = truth.get(&key) else { continue };
        let last_asof = r.marks.iter().map(|m| m.asof_idx).max().unwrap_or(r.start);

        let mut filings: Vec<(usize, i32)> = r.marks.iter().map(|m| (m.filed_idx.max(m.asof_idx + 1), m.ym)).collect();
        filings.sort_unstable();

        for &(day, value) in series {
            if day < r.start || day > last_asof || day - r.start >= r.path.len() {
                continue;
            }
            let Some(&(last_filed, _)) = filings.iter().rev().find(|f| f.0 <= day) else { continue };
            let newest = filings.iter().filter(|f| f.0 <= day).map(|f| f.1).max().unwrap();
            let public: Vec<f64> = r
                .marks
                .iter()
                .filter(|m| m.ym == newest && m.filed_idx.max(m.asof_idx + 1) <= day)
                .map(|m| m.price)
                .collect();
            let stale = public.iter().sum::<f64>() / public.len() as f64;
            let p = r.path[day - r.start];
            all.add(value, p.mean, p.sd, stale, bands);
            let age = day - last_filed;
            if let Some(b) = (0..labels.len()).find(|&b| age >= edges[b] && age < edges[b + 1]) {
                buckets[b].add(value, p.mean, p.sd, stale, bands);
            }
        }
    }
    if all.n == 0 {
        return Err("no loan in the panel matches the answer key".into());
    }
    Ok(TruthReport {
        all: all.score(),
        by_staleness: labels.iter().zip(&buckets).map(|(l, b)| (l.to_string(), b.score())).collect(),
    })
}
