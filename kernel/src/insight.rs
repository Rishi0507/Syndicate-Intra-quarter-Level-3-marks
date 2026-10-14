use std::collections::HashMap;

use crate::data::Panel;
use crate::json::Json;
use crate::stats::{quantile_sorted, Ols};

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

const MARKDOWN: f64 = 0.03;

const PEER_MOVE: f64 = 0.01;
const STILL: f64 = 0.0025;
const CLIP: f64 = 0.10;

pub struct CatchUp {
    pub n: usize,
    pub follow: f64,
    pub continuation: f64,
}

pub fn catch_up(panel: &Panel) -> CatchUp {
    let mut price: HashMap<(usize, i32, usize), f64> = HashMap::new();
    for o in &panel.obs {
        price.insert((o.inst, o.ym, o.family), o.price);
    }
    let clip = |x: f64| x.clamp(-CLIP, CLIP);
    let (mut follow, mut cont, mut den, mut n) = (0.0, 0.0, 0.0, 0usize);
    for inst in 0..panel.instruments.len() {
        let obs = panel.obs_of(inst);
        for o in obs {
            let (t, f) = (o.ym, o.family);
            let (Some(&mid), Some(&old)) = (price.get(&(inst, t - 3, f)), price.get(&(inst, t - 6, f))) else { continue };
            if (mid - old).abs() >= STILL {
                continue;
            }
            let (mut last, mut now, mut k) = (0.0, 0.0, 0usize);
            for g in obs.iter().filter(|g| g.ym == t && g.family != f) {
                let (Some(&gm), Some(&go)) = (price.get(&(inst, t - 3, g.family)), price.get(&(inst, t - 6, g.family))) else { continue };
                last += gm - go;
                now += g.price - gm;
                k += 1;
            }
            if k == 0 {
                continue;
            }
            let (last, now) = (clip(last / k as f64), clip(now / k as f64));
            if last.abs() < PEER_MOVE {
                continue;
            }
            follow += clip(o.price - mid) * last;
            cont += now * last;
            den += last * last;
            n += 1;
        }
    }
    if den <= 0.0 {
        return CatchUp { n, follow: f64::NAN, continuation: f64::NAN };
    }
    CatchUp { n, follow: follow / den, continuation: cont / den }
}

pub fn report(panel: &Panel) -> Json {
    let cs = cells(panel);
    let index: HashMap<(usize, i32), usize> = cs.iter().enumerate().map(|(i, c)| ((c.inst, c.ym), i)).collect();
    let shared: Vec<&Cell> = cs.iter().filter(|c| c.prices.len() >= 2).collect();

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

    let mut events: Vec<(f64, f64, bool)> = Vec::new();
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

    let lag = catch_up(panel);
    Json::obj()
        .with("shared_cells", shared.len())
        .with(
            "catch_up",
            Json::obj().with("n", lag.n).with("follow", Json::round(lag.follow, 4)).with("continuation", Json::round(lag.continuation, 4)),
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auc_of_a_perfect_and_a_useless_score() {
        let mut perfect: Vec<(f64, bool)> = (0..100).map(|i| (i as f64, i >= 50)).collect();
        assert!((auc(&mut perfect) - 1.0).abs() < 1e-12);
        let mut inverted: Vec<(f64, bool)> = (0..100).map(|i| (i as f64, i < 50)).collect();
        assert!(auc(&mut inverted).abs() < 1e-12);
        let mut flat: Vec<(f64, bool)> = (0..100).map(|i| (1.0, i % 2 == 0)).collect();
        assert!((auc(&mut flat) - 0.5).abs() < 1e-12);
        assert!(auc(&mut [(1.0, true), (2.0, true)]).is_nan());
    }

    #[test]
    fn catch_up_separates_the_follower_from_the_trend() {
        use crate::data::{Class, Instrument, Obs};
        let mark = |family: usize, ym: i32, price: f64| Obs {
            inst: 0,
            family,
            asof: 0,
            ym,
            asof_idx: 0,
            filed_idx: 0,
            price,
            principal: 1.0,
            spread_bp: 0.0,
            original: true,
        };
        let obs = vec![mark(0, 0, 0.99), mark(1, 0, 0.99), mark(0, 3, 0.99), mark(1, 3, 0.97), mark(0, 6, 0.98), mark(1, 6, 0.965)];
        let panel = Panel {
            source: "test".into(),
            days: vec![20240101, 20240102],
            factor_names: Vec::new(),
            factors: Vec::new(),
            families: vec!["a".into(), "b".into()],
            sectors: vec!["other".into()],
            holders: Vec::new(),
            borrowers: Vec::new(),
            instruments: vec![Instrument { borrower: 0, class: Class::Senior, piece: 0 }],
            inst_obs: vec![(0, obs.len())],
            obs,
        };
        let lag = catch_up(&panel);
        assert_eq!(lag.n, 1);
        assert!((lag.follow - 0.5).abs() < 1e-9, "follow {}", lag.follow);
        assert!((lag.continuation - 0.25).abs() < 1e-9, "continuation {}", lag.continuation);
    }

    #[test]
    fn buckets_split_at_the_stress_thresholds() {
        assert_eq!(bucket(0.99), 0);
        assert_eq!(bucket(0.97), 0);
        assert_eq!(bucket(0.95), 1);
        assert_eq!(bucket(0.80), 2);
    }
}
