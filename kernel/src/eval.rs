use crate::engine::Record;
use crate::json::Json;
use crate::stats::{norm_cdf, norm_inv};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Regime {
    Performing,
    Watch,
    Stressed,
}

pub const REGIMES: [Regime; 3] = [Regime::Performing, Regime::Watch, Regime::Stressed];

impl Regime {
    pub fn of(prev: f64) -> Regime {
        if prev >= 0.97 {
            Regime::Performing
        } else if prev >= 0.90 {
            Regime::Watch
        } else {
            Regime::Stressed
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Regime::Performing => "performing",
            Regime::Watch => "watch",
            Regime::Stressed => "stressed",
        }
    }
}

pub const RUNGS: [(&str, &str); 4] = [
    ("stale", "Last mark carried forward"),
    ("public_raw", "Rolled forward with public spreads and pull to par"),
    ("public", "With each lender's habits modelled"),
    ("peers", "With other lenders' marks of the same loan"),
];

pub const MODEL_RUNG: usize = 3;

pub const ABLATIONS: [(&str, &str); 1] = [("sector", "Adding sector repricing from other loans")];

fn point(r: &Record, rung: usize) -> f64 {
    match rung {
        0 => r.prev,
        1 => r.public_raw.0,
        2 => r.public.0,
        3 => r.peers.0,
        _ => r.sector.0,
    }
}

#[derive(Clone, Debug, Default)]
pub struct Score {
    pub n: usize,
    pub mae: f64,
    pub rmse: f64,
    pub skill: f64,
}

fn score(records: &[&Record], rung: usize) -> Score {
    if records.is_empty() {
        return Score::default();
    }
    let (mut ae, mut se, mut se0) = (0.0, 0.0, 0.0);
    for r in records {
        let e = r.price - point(r, rung);
        ae += e.abs();
        se += e * e;
        se0 += (r.price - r.prev).powi(2);
    }
    let n = records.len() as f64;
    Score { n: records.len(), mae: ae / n, rmse: (se / n).sqrt(), skill: if se0 > 0.0 { 1.0 - se / se0 } else { 0.0 } }
}

pub const LEVELS: [f64; 4] = [0.50, 0.80, 0.90, 0.95];

#[derive(Clone, Debug, Default)]
pub struct Coverage {
    pub n: usize,
    pub gaussian: [f64; 4],
    pub conformal: [f64; 4],
    pub width90: f64,
    pub naive_width90: f64,
    pub naive_cover90: f64,
    pub matched_width90: f64,
    pub matched_naive_width90: f64,
}

fn exact_quantile(v: &mut [f64], level: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[(((v.len() as f64) * level).ceil() as usize).clamp(1, v.len()) - 1]
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bands {
    pub mult: [[f64; 4]; 3],
    pub floor: [f64; 3],
}

impl Bands {
    pub fn half_width(&self, regime: Regime, level: usize, sd: f64) -> f64 {
        self.mult[regime as usize][level] * (sd + self.floor[regime as usize])
    }
}

fn median_of(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    0.5 * (s[(s.len() - 1) / 2] + s[s.len() / 2])
}

pub fn coverage(records: &[Record]) -> Vec<(Regime, Coverage)> {
    let mut order: Vec<&Record> = records.iter().collect();
    order.sort_by_key(|r| r.ym);
    let mut quarters: Vec<i32> = order.iter().map(|r| r.ym).collect();
    quarters.dedup();

    const MIN_SCORES: usize = 60;
    let mut out = Vec::new();
    for regime in REGIMES {
        let mut past: Vec<(f64, f64)> = Vec::new();
        let mut moves: Vec<f64> = Vec::new();
        let mut c = Coverage::default();
        let (mut hit_g, mut hit_c) = ([0usize; 4], [0usize; 4]);
        let (mut width, mut naive_width, mut naive_hit) = (0.0, 0.0, 0usize);
        let (mut ratios, mut halves, mut moved) = (Vec::new(), Vec::new(), Vec::new());
        for &q in &quarters {
            let batch: Vec<&&Record> = order.iter().filter(|r| r.ym == q && Regime::of(r.prev) == regime).collect();
            if past.len() >= MIN_SCORES {
                let errors: Vec<f64> = past.iter().map(|p| p.0).collect();
                let floor = median_of(&errors);
                let mut scores: Vec<f64> = past.iter().map(|(e, sd)| e / (sd + floor)).collect();
                scores.sort_by(|a, b| a.total_cmp(b));
                let naive90 = conformal_quantile(&moves, 0.90);
                for r in &batch {
                    let (mean, sd) = r.model();
                    let e = (r.price - mean).abs();
                    for (k, &level) in LEVELS.iter().enumerate() {
                        if e <= norm_inv(0.5 + level / 2.0) * sd {
                            hit_g[k] += 1;
                        }
                        let half = conformal_quantile(&scores, level) * (sd + floor);
                        if e <= half {
                            hit_c[k] += 1;
                        }
                        if k == 2 {
                            width += 2.0 * half;
                            if half > 0.0 {
                                ratios.push(e / half);
                                halves.push(half);
                            }
                        }
                    }
                    moved.push((r.price - r.prev).abs());
                    naive_width += 2.0 * naive90;
                    if (r.price - r.prev).abs() <= naive90 {
                        naive_hit += 1;
                    }
                    c.n += 1;
                }
            }
            for r in &batch {
                let (mean, sd) = r.model();
                if sd > 0.0 {
                    past.push(((r.price - mean).abs(), sd));
                }
                moves.push((r.price - r.prev).abs());
            }
            moves.sort_by(|a, b| a.total_cmp(b));
        }
        if c.n > 0 {
            let n = c.n as f64;
            for k in 0..4 {
                c.gaussian[k] = hit_g[k] as f64 / n;
                c.conformal[k] = hit_c[k] as f64 / n;
            }
            c.width90 = width / n;
            c.naive_width90 = naive_width / n;
            c.naive_cover90 = naive_hit as f64 / n;
            let scale = exact_quantile(&mut ratios, 0.90);
            c.matched_width90 = 2.0 * scale * halves.iter().sum::<f64>() / halves.len().max(1) as f64;
            c.matched_naive_width90 = 2.0 * exact_quantile(&mut moved, 0.90);
        }
        out.push((regime, c));
    }
    out
}

pub fn bands(records: &[Record]) -> Bands {
    let pairs = |sel: &dyn Fn(&Record) -> bool| -> Vec<(f64, f64)> {
        records.iter().filter(|r| sel(r) && r.model().1 > 0.0).map(|r| ((r.price - r.model().0).abs(), r.model().1)).collect()
    };
    let pooled = pairs(&|_| true);
    let mut out = Bands { mult: [[0.0; 4]; 3], floor: [0.0; 3] };
    for (i, regime) in REGIMES.iter().enumerate() {
        let own = pairs(&|r| Regime::of(r.prev) == *regime);
        let used = if own.len() >= 200 { &own } else { &pooled };
        let errors: Vec<f64> = used.iter().map(|p| p.0).collect();
        let floor = median_of(&errors);
        let mut scores: Vec<f64> = used.iter().map(|(e, sd)| e / (sd + floor)).collect();
        scores.sort_by(|a, b| a.total_cmp(b));
        out.floor[i] = floor;
        for (k, &level) in LEVELS.iter().enumerate() {
            let q = conformal_quantile(&scores, level);
            out.mult[i][k] = if q.is_finite() { q } else { norm_inv(0.5 + level / 2.0) };
        }
    }
    out
}

pub fn conformal_quantile(sorted: &[f64], level: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return norm_inv(0.5 + level / 2.0);
    }
    let rank = ((n as f64 + 1.0) * level).ceil() as usize;
    if rank > n {
        return f64::INFINITY;
    }
    sorted[rank.max(1) - 1]
}

pub fn pit(records: &[&Record]) -> [f64; 10] {
    let mut h = [0.0; 10];
    if records.is_empty() {
        return h;
    }
    for r in records {
        let (mean, sd) = r.model();
        let u = norm_cdf((r.price - mean) / sd);
        h[((u * 10.0) as usize).min(9)] += 1.0;
    }
    for x in h.iter_mut() {
        *x /= records.len() as f64;
    }
    h
}

fn score_json(s: &Score) -> Json {
    Json::obj()
        .with("n", s.n)
        .with("mae", Json::round(s.mae * 100.0, 4))
        .with("rmse", Json::round(s.rmse * 100.0, 4))
        .with("skill", Json::round(s.skill, 4))
}

fn ladder(records: &[&Record]) -> Json {
    Json::Arr(
        RUNGS
            .iter()
            .enumerate()
            .map(|(k, (id, label))| score_json(&score(records, k)).with("id", *id).with("label", *label))
            .collect(),
    )
}

pub const SIGNAL_EDGES: [f64; 5] = [0.0, 0.0025, 0.01, 0.03, f64::INFINITY];

fn signal_row(label: &str, records: &[&Record]) -> Json {
    let stale = score(records, 0);
    let model = score(records, MODEL_RUNG);
    Json::obj()
        .with("label", label)
        .with("n", records.len())
        .with("stale_mae", Json::round(stale.mae * 100.0, 4))
        .with("stale_rmse", Json::round(stale.rmse * 100.0, 4))
        .with("mae", Json::round(model.mae * 100.0, 4))
        .with("rmse", Json::round(model.rmse * 100.0, 4))
        .with("skill", Json::round(model.skill, 4))
}

fn by_signal(records: &[Record]) -> Json {
    let moved = |r: &Record| (r.model().0 - r.prev).abs();
    let labels = ["Under 0.25 points", "0.25 to 1 point", "1 to 3 points", "3 points or more"];
    let mut rows: Vec<Json> = (0..labels.len())
        .map(|k| {
            let sel: Vec<&Record> = records.iter().filter(|r| moved(r) >= SIGNAL_EDGES[k] && moved(r) < SIGNAL_EDGES[k + 1]).collect();
            signal_row(labels[k], &sel)
        })
        .collect();
    let peer_moved: Vec<&Record> = records.iter().filter(|r| r.seen > 0 && moved(r) >= 0.01).collect();
    rows.push(signal_row("A peer has filed and the estimate moved 1 point or more", &peer_moved));
    Json::Arr(rows)
}

pub const MIN_BOOK: usize = 20;

#[derive(Clone, Debug, Default)]
pub struct Book {
    pub family: usize,
    pub quarter: i32,
    pub loans: usize,
    pub actual: f64,
    pub model: f64,
    pub habits: f64,
    pub peer_share: f64,
}

pub fn books(records: &[Record]) -> Vec<Book> {
    let mut groups: std::collections::BTreeMap<(usize, i32), Vec<&Record>> = std::collections::BTreeMap::new();
    for r in records {
        groups.entry((r.family, r.ym.div_euclid(3))).or_default().push(r);
    }
    groups
        .into_iter()
        .filter_map(|((family, quarter), rs)| {
            let w: f64 = rs.iter().map(|r| r.principal).sum();
            if rs.len() < MIN_BOOK || w <= 0.0 {
                return None;
            }
            let avg = |f: &dyn Fn(&Record) -> f64| rs.iter().map(|r| r.principal * f(r)).sum::<f64>() / w;
            Some(Book {
                family,
                quarter,
                loans: rs.len(),
                actual: avg(&|r| r.price - r.prev),
                model: avg(&|r| r.model().0 - r.prev),
                habits: avg(&|r| r.public.0 - r.prev),
                peer_share: avg(&|r| if r.seen > 0 { 1.0 } else { 0.0 }),
            })
        })
        .collect()
}

fn book_score(bs: &[&Book]) -> Json {
    let n = bs.len();
    let sst: f64 = bs.iter().map(|b| b.actual * b.actual).sum();
    let sse: f64 = bs.iter().map(|b| (b.actual - b.model).powi(2)).sum();
    let ssh: f64 = bs.iter().map(|b| (b.actual - b.habits).powi(2)).sum();
    let moved: Vec<&&Book> = bs.iter().filter(|b| b.actual.abs() >= 0.005).collect();
    let right = moved.iter().filter(|b| b.actual * b.model > 0.0).count();
    let mean = |f: &dyn Fn(&Book) -> f64| bs.iter().map(|b| f(b)).sum::<f64>() / n.max(1) as f64;
    let (ma, mm) = (mean(&|b| b.actual), mean(&|b| b.model));
    let cov: f64 = bs.iter().map(|b| (b.actual - ma) * (b.model - mm)).sum();
    let va: f64 = bs.iter().map(|b| (b.actual - ma).powi(2)).sum();
    let vm: f64 = bs.iter().map(|b| (b.model - mm).powi(2)).sum();
    Json::obj()
        .with("n", n)
        .with("skill", Json::round(if sst > 0.0 { 1.0 - sse / sst } else { f64::NAN }, 4))
        .with("habits_skill", Json::round(if sst > 0.0 { 1.0 - ssh / sst } else { f64::NAN }, 4))
        .with("correlation", Json::round(if va > 0.0 && vm > 0.0 { cov / (va * vm).sqrt() } else { f64::NAN }, 4))
        .with("mean_abs_change", Json::round(mean(&|b| b.actual.abs()) * 100.0, 4))
        .with("mean_abs_error", Json::round(mean(&|b| (b.actual - b.model).abs()) * 100.0, 4))
        .with("moved", moved.len())
        .with("direction_right", right)
}

pub fn book_report(records: &[Record]) -> Json {
    let all = books(records);
    let refs: Vec<&Book> = all.iter().collect();
    let bins: [(f64, f64, &str); 3] = [(0.0, 0.2, "Under 20%"), (0.2, 0.4, "20% to 40%"), (0.4, 1.01, "40% or more")];
    let by_share = bins
        .iter()
        .map(|&(lo, hi, label)| {
            let sel: Vec<&Book> = all.iter().filter(|b| b.peer_share >= lo && b.peer_share < hi).collect();
            book_score(&sel).with("label", label)
        })
        .collect::<Vec<_>>();
    let points = all
        .iter()
        .map(|b| Json::Arr(vec![Json::round(b.actual * 100.0, 3), Json::round(b.model * 100.0, 3), Json::round(b.peer_share, 3), Json::from(b.loans)]))
        .collect::<Vec<_>>();
    book_score(&refs).with("min_loans", MIN_BOOK).with("by_peer_share", by_share).with("points", points)
}

fn ablations(records: &[&Record]) -> Json {
    let model = score(records, MODEL_RUNG);
    Json::Arr(
        ABLATIONS
            .iter()
            .enumerate()
            .map(|(k, (id, label))| {
                let s = score(records, RUNGS.len() + k);
                score_json(&s)
                    .with("id", *id)
                    .with("label", *label)
                    .with("rmse_change", Json::round(if model.rmse > 0.0 { s.rmse / model.rmse - 1.0 } else { 0.0 }, 6))
            })
            .collect(),
    )
}

pub fn report(records: &[Record]) -> Json {
    let all: Vec<&Record> = records.iter().collect();
    let followers: Vec<&Record> = records.iter().filter(|r| r.seen > 0).collect();
    let movers: Vec<&Record> = records.iter().filter(|r| (r.price - r.prev).abs() >= 0.01).collect();
    let big_movers: Vec<&Record> = records.iter().filter(|r| (r.price - r.prev).abs() >= 0.03).collect();
    let quiet: Vec<&Record> = records.iter().filter(|r| (r.price - r.prev).abs() < 0.01).collect();

    let mut by_regime = Vec::new();
    for regime in REGIMES {
        let sel: Vec<&Record> = records.iter().filter(|r| Regime::of(r.prev) == regime).collect();
        by_regime.push(Json::obj().with("regime", regime.label()).with("ladder", ladder(&sel)).with("pit", Json::nums(&pit(&sel), 4)));
    }

    let cov = coverage(records)
        .into_iter()
        .map(|(regime, c)| {
            Json::obj()
                .with("regime", regime.label())
                .with("n", c.n)
                .with("levels", Json::nums(&LEVELS, 2))
                .with("gaussian", Json::nums(&c.gaussian, 4))
                .with("conformal", Json::nums(&c.conformal, 4))
                .with("width90", Json::round(c.width90 * 100.0, 4))
                .with("naive_width90", Json::round(c.naive_width90 * 100.0, 4))
                .with("naive_cover90", Json::round(c.naive_cover90, 4))
                .with("matched_width90", Json::round(c.matched_width90 * 100.0, 4))
                .with("matched_naive_width90", Json::round(c.matched_naive_width90 * 100.0, 4))
        })
        .collect::<Vec<_>>();

    let calendar = |ym: i32| ym + (2 - ym.rem_euclid(3));
    let mut quarters: Vec<i32> = records.iter().map(|r| calendar(r.ym)).collect();
    quarters.sort_unstable();
    quarters.dedup();
    let by_quarter = quarters
        .iter()
        .map(|&q| {
            let sel: Vec<&Record> = records.iter().filter(|r| calendar(r.ym) == q).collect();
            let peers: Vec<&Record> = sel.iter().copied().filter(|r| r.seen > 0).collect();
            Json::obj()
                .with("ym", q)
                .with("n", sel.len())
                .with("stale", Json::round(score(&sel, 0).rmse * 100.0, 4))
                .with("model", Json::round(score(&sel, MODEL_RUNG).rmse * 100.0, 4))
                .with("followers", peers.len())
                .with("followers_stale", Json::round(score(&peers, 0).rmse * 100.0, 4))
                .with("followers_model", Json::round(score(&peers, MODEL_RUNG).rmse * 100.0, 4))
        })
        .collect::<Vec<_>>();

    Json::obj()
        .with("n", records.len())
        .with("quarters", quarters.len())
        .with("all", ladder(&all))
        .with("followers", ladder(&followers))
        .with("movers", ladder(&movers))
        .with("big_movers", ladder(&big_movers))
        .with("quiet", ladder(&quiet))
        .with("ablations", ablations(&all))
        .with("by_signal", by_signal(records))
        .with("books", book_report(records))
        .with("by_regime", by_regime)
        .with("coverage", cov)
        .with("by_quarter", by_quarter)
        .with("pit", Json::nums(&pit(&all), 4))
}

pub fn print_ladder(title: &str, records: &[&Record]) {
    println!("\n  {title}  (n = {})", records.len());
    println!("  {:<46} {:>8} {:>8} {:>8}", "", "MAE", "RMSE", "skill");
    for (k, (_, label)) in RUNGS.iter().enumerate() {
        let s = score(records, k);
        println!("  {:<46} {:>8.3} {:>8.3} {:>7.1}%", label, s.mae * 100.0, s.rmse * 100.0, s.skill * 100.0);
    }
    for (k, (_, label)) in ABLATIONS.iter().enumerate() {
        let s = score(records, RUNGS.len() + k);
        println!("  {:<46} {:>8.3} {:>8.3} {:>7.1}%   not adopted", label, s.mae * 100.0, s.rmse * 100.0, s.skill * 100.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Rng;

    fn rec(ym: i32, price: f64, prev: f64, mean: f64, sd: f64) -> Record {
        Record {
            inst: 0,
            family: 0,
            ym,
            price,
            prev,
            public_raw: (mean, sd),
            public: (mean, sd),
            peers: (mean, sd),
            sector: (mean, sd),
            seen: 1,
            principal: 1.0,
            filed: 0,
        }
    }

    #[test]
    fn conformal_quantile_is_the_finite_sample_rank() {
        let s: Vec<f64> = (1..=9).map(|x| x as f64).collect();
        assert_eq!(conformal_quantile(&s, 0.9), 9.0);
        assert!(conformal_quantile(&s, 0.95).is_infinite());
        assert_eq!(conformal_quantile(&s, 0.5), 5.0);
    }

    #[test]
    fn conformal_repairs_fat_tails() {
        let mut rng = Rng::new(5);
        let mut records = Vec::new();
        for q in 0..12 {
            for _ in 0..400 {
                let t = rng.normal() / ((rng.normal().powi(2) + rng.normal().powi(2) + rng.normal().powi(2)) / 3.0).sqrt();
                let e = 0.01 * t / 3f64.sqrt();
                records.push(rec(q * 3, 0.99 + e, 0.99, 0.99, 0.005));
            }
        }
        let cov = coverage(&records);
        let c = &cov[0].1;
        assert!(c.n > 3000);
        assert!(c.gaussian[2] < 0.86, "gaussian 90% coverage {}", c.gaussian[2]);
        for (k, level) in LEVELS.iter().enumerate() {
            assert!((c.conformal[k] - level).abs() < 0.02, "level {level}: {}", c.conformal[k]);
        }
    }

    #[test]
    fn matched_widths_hold_ninety_percent() {
        let mut rng = Rng::new(9);
        let mut records = Vec::new();
        for q in 0..10 {
            for _ in 0..500 {
                let e = 0.01 * rng.normal();
                records.push(rec(q * 3, 0.98 + e, 0.98, 0.98, 0.004));
            }
        }
        let c = &coverage(&records)[0].1;
        assert!(c.matched_width90 > 0.0 && c.matched_naive_width90 > 0.0);
        let half = c.matched_width90 / 2.0;
        let inside = records.iter().filter(|r| (r.price - r.model().0).abs() <= half + 1e-12).count() as f64 / records.len() as f64;
        assert!((inside - 0.9).abs() < 0.03, "inside {inside}");
        assert!((c.matched_width90 - c.matched_naive_width90).abs() < 0.002);
    }

    #[test]
    fn signal_groups_use_the_forecast_not_the_outcome() {
        let rs = vec![rec(0, 0.90, 0.99, 0.95, 0.01), rec(0, 0.99, 0.99, 0.989, 0.01)];
        let report = by_signal(&rs).to_string();
        assert!(report.contains("\"label\":\"Under 0.25 points\",\"n\":1"), "{report}");
        assert!(report.contains("\"label\":\"3 points or more\",\"n\":1"), "{report}");
    }

    #[test]
    fn books_weight_loans_by_principal() {
        let mut rs: Vec<Record> = (0..MIN_BOOK).map(|_| rec(3, 0.99, 0.99, 0.99, 0.01)).collect();
        rs[0] = Record { price: 0.89, principal: 3.0, sector: (0.94, 0.01), peers: (0.94, 0.01), ..rec(3, 0.89, 0.99, 0.94, 0.01) };
        let b = &books(&rs)[0];
        let w = 3.0 + (MIN_BOOK - 1) as f64;
        assert!((b.actual - (-0.10 * 3.0 / w)).abs() < 1e-12);
        assert!((b.model - (-0.05 * 3.0 / w)).abs() < 1e-12);
        assert!(books(&rs[1..]).is_empty());
    }

    #[test]
    fn skill_is_relative_to_stale() {
        let rs = vec![rec(0, 0.95, 0.99, 0.96, 0.01), rec(0, 0.97, 0.99, 0.97, 0.01)];
        let refs: Vec<&Record> = rs.iter().collect();
        let stale = score(&refs, 0);
        let model = score(&refs, MODEL_RUNG);
        assert_eq!(stale.skill, 0.0);
        assert!(model.skill > 0.9 && model.rmse < stale.rmse);
    }
}
