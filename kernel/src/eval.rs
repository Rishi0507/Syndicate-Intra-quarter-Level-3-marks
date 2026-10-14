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

pub const RUNGS: [(&str, &str); 5] = [
    ("stale", "Last mark carried forward"),
    ("public_raw", "Rolled forward with public spreads and pull to par"),
    ("public", "With each lender's habits modelled"),
    ("peers", "With other lenders' marks of the same loan"),
    ("full", "With sector repricing from other loans"),
];

fn point(r: &Record, rung: usize) -> f64 {
    match rung {
        0 => r.prev,
        1 => r.public_raw.0,
        2 => r.public.0,
        3 => r.peers.0,
        _ => r.full.0,
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
        for &q in &quarters {
            let batch: Vec<&&Record> = order.iter().filter(|r| r.ym == q && Regime::of(r.prev) == regime).collect();
            if past.len() >= MIN_SCORES {
                let errors: Vec<f64> = past.iter().map(|p| p.0).collect();
                let floor = median_of(&errors);
                let mut scores: Vec<f64> = past.iter().map(|(e, sd)| e / (sd + floor)).collect();
                scores.sort_by(|a, b| a.total_cmp(b));
                let naive90 = conformal_quantile(&moves, 0.90);
                for r in &batch {
                    let (mean, sd) = r.full;
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
                        }
                    }
                    naive_width += 2.0 * naive90;
                    if (r.price - r.prev).abs() <= naive90 {
                        naive_hit += 1;
                    }
                    c.n += 1;
                }
            }
            for r in &batch {
                let (mean, sd) = r.full;
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
        }
        out.push((regime, c));
    }
    out
}

pub fn bands(records: &[Record]) -> Bands {
    let pairs = |sel: &dyn Fn(&Record) -> bool| -> Vec<(f64, f64)> {
        records.iter().filter(|r| sel(r) && r.full.1 > 0.0).map(|r| ((r.price - r.full.0).abs(), r.full.1)).collect()
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
        let (mean, sd) = r.full;
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

pub fn report(records: &[Record]) -> Json {
    let all: Vec<&Record> = records.iter().collect();
    let followers: Vec<&Record> = records.iter().filter(|r| r.seen > 0).collect();
    let movers: Vec<&Record> = records.iter().filter(|r| (r.price - r.prev).abs() >= 0.01).collect();

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
            Json::obj()
                .with("ym", q)
                .with("n", sel.len())
                .with("stale", Json::round(score(&sel, 0).rmse * 100.0, 4))
                .with("full", Json::round(score(&sel, 4).rmse * 100.0, 4))
        })
        .collect::<Vec<_>>();

    Json::obj()
        .with("n", records.len())
        .with("quarters", quarters.len())
        .with("all", ladder(&all))
        .with("followers", ladder(&followers))
        .with("movers", ladder(&movers))
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Rng;

    fn rec(ym: i32, price: f64, prev: f64, mean: f64, sd: f64) -> Record {
        Record { inst: 0, family: 0, ym, price, prev, public_raw: (mean, sd), public: (mean, sd), peers: (mean, sd), full: (mean, sd), seen: 1 }
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
    fn skill_is_relative_to_stale() {
        let rs = vec![rec(0, 0.95, 0.99, 0.96, 0.01), rec(0, 0.97, 0.99, 0.97, 0.01)];
        let refs: Vec<&Record> = rs.iter().collect();
        let stale = score(&refs, 0);
        let model = score(&refs, 4);
        assert_eq!(stale.skill, 0.0);
        assert!(model.skill > 0.9 && model.rmse < stale.rmse);
    }
}
