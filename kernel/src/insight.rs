//! Findings read straight off the marks, before any model is involved.
//!
//! These are the facts the model exists to explain: that lenders disagree
//! about the same loan, by how much, whether the disagreement warns of
//! trouble, and which lender's mark is the best guide to where the others
//! will go.

use std::collections::HashMap;

use crate::data::Panel;
use crate::json::Json;
use crate::stats::{quantile_sorted, Ols};

/// All families' marks of one loan on one date.
struct Cell {
    inst: usize,
    ym: i32,
    prices: Vec<f64>,
}

fn cells(panel: &Panel) -> Vec<Cell> {
    let mut out: Vec<Cell> = Vec::new();
    for o in &panel.obs {
        match out.last_mut() {
            Some(c) if c.inst == o.inst && c.ym == o.ym => c.prices.push(o.price),
            _ => out.push(Cell { inst: o.inst, ym: o.ym, prices: vec![o.price] }),
        }
    }
    out
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn min(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::INFINITY, f64::min)
}

fn max(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

fn median_of(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    quantile_sorted(&s, 0.5)
}

fn quantiles(mut v: Vec<f64>, ps: &[f64]) -> Vec<f64> {
    v.sort_by(|a, b| a.total_cmp(b));
    ps.iter().map(|&p| quantile_sorted(&v, p)).collect()
}

fn bucket(level: f64) -> usize {
    if level >= 0.97 {
        0
    } else if level >= 0.90 {
        1
    } else {
        2
    }
}

const BUCKETS: [&str; 3] = ["performing", "watch", "stressed"];

/// Area under the ROC curve of `score` as a predictor of `event`, by the
/// rank-sum identity. Ties share their average rank.
fn auc(pairs: &mut [(f64, bool)]) -> f64 {
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (mut rank_sum, mut pos, mut i) = (0.0, 0.0, 0);
    while i < pairs.len() {
        let mut j = i;
        while j < pairs.len() && pairs[j].0 == pairs[i].0 {
            j += 1;
        }
        let avg_rank = (i + j + 1) as f64 / 2.0;
        for p in &pairs[i..j] {
            if p.1 {
                rank_sum += avg_rank;
                pos += 1.0;
            }
        }
        i = j;
    }
    let neg = pairs.len() as f64 - pos;
    if pos == 0.0 || neg == 0.0 {
        return f64::NAN;
    }
    (rank_sum - pos * (pos + 1.0) / 2.0) / (pos * neg)
}

/// A fall in the consensus of at least this much within a quarter is counted
/// as a markdown event.
const MARKDOWN: f64 = 0.03;

pub fn report(panel: &Panel) -> Json {
    let cs = cells(panel);
    let index: HashMap<(usize, i32), usize> = cs.iter().enumerate().map(|(i, c)| ((c.inst, c.ym), i)).collect();
    let shared: Vec<&Cell> = cs.iter().filter(|c| c.prices.len() >= 2).collect();

    // ---- How much do lenders disagree? ------------------------------------
    let ps = [0.5, 0.75, 0.9, 0.95, 0.99];
    let mut by_bucket: Vec<Vec<f64>> = vec![Vec::new(); 3];
    for c in &shared {
        by_bucket[bucket(mean(&c.prices))].push(max(&c.prices) - min(&c.prices));
    }
    let dispersion = (0..3)
        .map(|b| {
            let v = &by_bucket[b];
            let n = v.len();
            let over = |x: f64| if n > 0 { v.iter().filter(|r| **r > x).count() as f64 / n as f64 } else { f64::NAN };
            let q = if n > 0 { quantiles(v.clone(), &ps) } else { vec![f64::NAN; ps.len()] };
            Json::obj()
                .with("regime", BUCKETS[b])
                .with("n", n)
                .with("quantiles", Json::nums(&ps, 2))
                .with("range", Json::nums(&q.iter().map(|x| x * 100.0).collect::<Vec<_>>(), 3))
                .with("share_over_1", Json::round(over(0.01), 4))
                .with("share_over_5", Json::round(over(0.05), 4))
                .with("share_over_10", Json::round(over(0.10), 4))
        })
        .collect::<Vec<_>>();

    // Median disagreement through time.
    let mut by_quarter: HashMap<i32, Vec<f64>> = HashMap::new();
    for c in &shared {
        by_quarter.entry(c.ym).or_default().push(max(&c.prices) - min(&c.prices));
    }
    let mut quarters: Vec<i32> = by_quarter.keys().copied().collect();
    quarters.sort_unstable();
    let through_time = quarters
        .iter()
        .map(|q| {
            let v = quantiles(by_quarter[q].clone(), &[0.5, 0.9]);
            Json::obj()
                .with("ym", *q)
                .with("n", by_quarter[q].len())
                .with("median", Json::round(v[0] * 100.0, 3))
                .with("p90", Json::round(v[1] * 100.0, 3))
        })
        .collect::<Vec<_>>();

    // ---- Whose mark leads? ------------------------------------------------
    //
    // For a loan several lenders hold, take this quarter's lowest, mean,
    // median and highest mark as four competing forecasts of next quarter's
    // consensus. If marks converge on the most cautious lender, the lowest
    // mark wins. The regression asks the same thing with both tails at once:
    // how much of the gap to the low mark, and to the high mark, does the
    // consensus close?
    let mut errs = [[0.0f64; 4]; 3];
    let mut counts = [0usize; 3];
    let mut ols = Ols::new(3);
    for c in &shared {
        let Some(&ni) = index.get(&(c.inst, c.ym + 3)) else { continue };
        let next = mean(&cs[ni].prices);
        let (lo, mid, med, hi) = (min(&c.prices), mean(&c.prices), median_of(&c.prices), max(&c.prices));
        let b = bucket(mid);
        for (k, f) in [lo, mid, med, hi].iter().enumerate() {
            errs[b][k] += (next - f).powi(2);
        }
        counts[b] += 1;
        ols.add(&[1.0, lo - mid, hi - mid], next - mid, 1.0);
    }
    let leader = (0..3)
        .map(|b| {
            let n = counts[b].max(1) as f64;
            let r: Vec<f64> = errs[b].iter().map(|e| (e / n).sqrt() * 100.0).collect();
            Json::obj().with("regime", BUCKETS[b]).with("n", counts[b]).with("rmse_low_mean_median_high", Json::nums(&r, 4))
        })
        .collect::<Vec<_>>();
    let pull = match ols.solve() {
        Some((b, _)) if ols.n >= 50.0 => Json::obj().with("n", ols.n as usize).with("toward_low", Json::round(b[1], 4)).with("toward_high", Json::round(b[2], 4)),
        _ => Json::Null,
    };

    // ---- Does disagreement warn of markdowns? -----------------------------
    //
    // Sort shared loans by how far apart their lenders are this quarter and
    // look at how often the consensus then falls by three points or more.
    // The level of the mark is the obvious rival predictor, so the same is
    // done for it, and disagreement is also tested among performing loans
    // alone, where the level says nothing.
    let mut events: Vec<(f64, f64, bool)> = Vec::new(); // range, level, markdown next quarter
    for c in &shared {
        let Some(&ni) = index.get(&(c.inst, c.ym + 3)) else { continue };
        let level = mean(&c.prices);
        events.push((max(&c.prices) - min(&c.prices), level, mean(&cs[ni].prices) - level <= -MARKDOWN));
    }
    let base_rate = if events.is_empty() { f64::NAN } else { events.iter().filter(|e| e.2).count() as f64 / events.len() as f64 };
    let mut sorted = events.clone();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let deciles = (0..10)
        .map(|d| {
            let (a, b) = (d * sorted.len() / 10, (d + 1) * sorted.len() / 10);
            let slice = &sorted[a..b.max(a)];
            if slice.is_empty() {
                return Json::Null;
            }
            let rate = slice.iter().filter(|e| e.2).count() as f64 / slice.len() as f64;
            let avg = slice.iter().map(|e| e.0).sum::<f64>() / slice.len() as f64;
            Json::obj().with("range", Json::round(avg * 100.0, 3)).with("rate", Json::round(rate, 5))
        })
        .collect::<Vec<_>>();
    let auc_range = auc(&mut events.iter().map(|e| (e.0, e.2)).collect::<Vec<_>>());
    let auc_level = auc(&mut events.iter().map(|e| (-e.1, e.2)).collect::<Vec<_>>());
    let auc_performing = auc(&mut events.iter().filter(|e| e.1 >= 0.97).map(|e| (e.0, e.2)).collect::<Vec<_>>());

    Json::obj()
        .with("shared_cells", shared.len())
        .with("dispersion", dispersion)
        .with("dispersion_by_quarter", through_time)
        .with("leader", leader)
        .with("pull", pull)
        .with(
            "warning",
            Json::obj()
                .with("n", events.len())
                .with("markdown_points", MARKDOWN * 100.0)
                .with("base_rate", Json::round(base_rate, 5))
                .with("deciles", deciles)
                .with("auc_dispersion", Json::round(auc_range, 4))
                .with("auc_level", Json::round(auc_level, 4))
                .with("auc_dispersion_performing", Json::round(auc_performing, 4)),
        )
}
