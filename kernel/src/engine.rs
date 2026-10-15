use std::collections::HashMap;

use crate::calib::{self, Fit, Part, PARTS};
use crate::data::{is_calendar_quarter, year_month, Obs, Panel};
use crate::filter::{self, Forecast, MarkIn, Point, Run, SectorIn};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variant {
    pub habits: bool,
    pub sector: bool,
    pub without: Option<Part>,
}

pub const MODEL: Variant = Variant { habits: true, sector: false, without: None };
pub const WITH_SECTOR: Variant = Variant { habits: true, sector: true, without: None };
pub const RAW: Variant = Variant { habits: false, sector: false, without: None };

pub const BATCH_DAYS: usize = 5;
pub const MIN_PEERS: usize = 8;
const CLAMP_SD: f64 = 2.5;

struct Surprise {
    inst: usize,
    weight: f64,
    value: f64,
}

pub struct LoanRun {
    pub inst: usize,
    pub start: usize,
    pub marks: Vec<MarkIn>,
    pub families: Vec<usize>,
    pub forecasts: Vec<Forecast>,
    pub feed: Vec<SectorIn>,
    pub said: Vec<(i32, usize, f64, f64)>,
    pub path: Vec<Point>,
}

#[derive(Clone, Copy, Debug)]
pub struct Batch {
    pub sector: usize,
    pub ym: i32,
    pub week: usize,
    pub weight: f64,
    pub weighted_sum: f64,
    pub n: usize,
    pub inflate: f64,
}

pub struct Engine<'a> {
    pub panel: &'a Panel,
    pub shocks: Vec<f64>,
    pub quarters: Vec<(usize, i32)>,
}

impl<'a> Engine<'a> {
    pub fn new(panel: &'a Panel, level: &[f64]) -> Engine<'a> {
        let (_, shocks) = calib::shocks_from(level);
        let mut quarters = Vec::new();
        for (t, &d) in panel.days.iter().enumerate() {
            let last_of_month = t + 1 == panel.days.len() || panel.days[t + 1] / 100 != d / 100;
            let complete = t + 1 < panel.days.len();
            if last_of_month && complete && is_calendar_quarter(d) {
                quarters.push((t, year_month(d)));
            }
        }
        Engine { panel, shocks, quarters }
    }

    pub fn run(
        &self,
        fit: &Fit,
        variant: Variant,
        insts: &[usize],
        keep: &dyn Fn(&Obs) -> bool,
        end: usize,
        paths: bool,
    ) -> Vec<LoanRun> {
        self.run_with_book(fit, variant, insts, keep, end, paths).0
    }

    pub fn run_with_book(
        &self,
        fit: &Fit,
        variant: Variant,
        insts: &[usize],
        keep: &dyn Fn(&Obs) -> bool,
        end: usize,
        paths: bool,
    ) -> (Vec<LoanRun>, Vec<Batch>) {
        let fit = if variant.habits { fit.clone() } else { fit.without_lender_habits() };
        let fit = match variant.without {
            Some(part) => fit.without(part),
            None => fit,
        };
        let d = &fit.dynamics;

        let prepare = |inst: usize| -> Option<(Vec<MarkIn>, Vec<usize>, usize)> {
            let obs: Vec<Obs> = self.panel.obs_of(inst).iter().filter(|o| keep(o)).cloned().collect();
            let start = obs.iter().map(|o| o.asof_idx).min()?;
            let fams = obs.iter().map(|o| o.family).collect();
            Some((calib::marks_for(&obs, &fit), fams, start))
        };

        let sweep = |r: &mut LoanRun, keep_path: bool| {
            r.forecasts.clear();
            r.path.clear();
            filter::run(
                &Run {
                    shocks: &self.shocks,
                    start: r.start,
                    end,
                    quarters: &self.quarters,
                    marks: &r.marks,
                    sector: &r.feed,
                    prints: &[],
                },
                d,
                &mut r.forecasts,
                if keep_path { Some(&mut r.path) } else { None },
            );
        };

        let hears_sector = variant.sector && d.sector0 > 0.0;
        let mut runs: Vec<LoanRun> = Vec::with_capacity(insts.len());
        for &inst in insts {
            let Some((marks, families, start)) = prepare(inst) else { continue };
            if start > end {
                continue;
            }
            let mut r =
                LoanRun { inst, start, marks, families, forecasts: Vec::new(), feed: Vec::new(), said: Vec::new(), path: Vec::new() };
            sweep(&mut r, paths && !hears_sector);
            runs.push(r);
        }
        if !hears_sector {
            return (runs, Vec::new());
        }

        let mut batches: HashMap<(usize, i32, usize), Vec<Surprise>> = HashMap::new();
        for r in &runs {
            let sector = self.panel.borrowers[self.panel.instruments[r.inst].borrower].sector;
            let mut seen_ym: Vec<i32> = Vec::new();
            for f in &r.forecasts {
                let m = &r.marks[f.mark];
                if f.late || !f.surprise.is_finite() || seen_ym.contains(&m.ym) {
                    continue;
                }
                seen_ym.push(m.ym);
                if !matches!(m.ym.rem_euclid(12), 2 | 5 | 8 | 11) {
                    continue;
                }
                let own = (f.surprise_var - f.sector_var).max(1e-10);
                let limit = CLAMP_SD * f.surprise_var.max(1e-10).sqrt();
                batches.entry((sector, m.ym, f.arrival / BATCH_DAYS)).or_default().push(Surprise {
                    inst: r.inst,
                    weight: 1.0 / own,
                    value: f.surprise.clamp(-limit, limit),
                });
            }
        }
        let mut by_sector: HashMap<(usize, i32), Vec<(usize, f64, f64, usize, f64)>> = HashMap::new();
        let mut own: HashMap<(usize, i32), (usize, f64, f64)> = HashMap::new();
        for ((sector, ym, week), list) in &batches {
            let w: f64 = list.iter().map(|s| s.weight).sum();
            let wv: f64 = list.iter().map(|s| s.weight * s.value).sum();
            let mean = wv / w;
            let scatter: f64 = list.iter().map(|s| s.weight * (s.value - mean).powi(2)).sum();
            let inflate = if list.len() > 1 { (scatter / (list.len() - 1) as f64).max(1.0) } else { 1.0 };
            by_sector.entry((*sector, *ym)).or_default().push((*week, w, wv, list.len(), inflate));
            for s in list {
                own.insert((s.inst, *ym), (*week, s.weight, s.weight * s.value));
            }
        }
        for v in by_sector.values_mut() {
            v.sort_by_key(|b| b.0);
        }

        for r in runs.iter_mut() {
            let sector = self.panel.borrowers[self.panel.instruments[r.inst].borrower].sector;
            let mut feed: Vec<SectorIn> = Vec::new();
            for &(_, ym) in &self.quarters {
                let Some(list) = by_sector.get(&(sector, ym)) else { continue };
                let mine = own.get(&(r.inst, ym));
                for &(week, w, wv, n, inflate) in list {
                    let (mut w, mut wv, mut n) = (w, wv, n);
                    if let Some(&(my_week, mw, mwv)) = mine {
                        if my_week == week {
                            w -= mw;
                            wv -= mwv;
                            n -= 1;
                        }
                    }
                    if n < MIN_PEERS || w <= 0.0 {
                        continue;
                    }
                    feed.push(SectorIn { ym, avail_idx: (week + 1) * BATCH_DAYS, value: wv / w, var: inflate / w });
                }
            }
            r.feed = feed;
            r.said = self
                .quarters
                .iter()
                .filter_map(|&(_, ym)| own.get(&(r.inst, ym)).map(|&(week, w, wv)| (ym, week, w, wv)))
                .collect();
            sweep(r, paths);
        }
        let mut book: Vec<Batch> = by_sector
            .iter()
            .flat_map(|(&(sector, ym), list)| {
                list.iter().map(move |&(week, weight, weighted_sum, n, inflate)| Batch { sector, ym, week, weight, weighted_sum, n, inflate })
            })
            .collect();
        book.sort_by_key(|b| (b.sector, b.ym, b.week));
        (runs, book)
    }
}

pub const N_PARTS: usize = PARTS.len();

#[derive(Clone, Debug)]
pub struct Record {
    pub inst: usize,
    pub family: usize,
    pub ym: i32,
    pub price: f64,
    pub prev: f64,
    pub public_raw: (f64, f64),
    pub public: (f64, f64),
    pub peers: (f64, f64),
    pub sector: (f64, f64),
    pub parts: [f64; N_PARTS],
    pub copy: f64,
    pub rule: f64,
    pub combined: f64,
    pub lockstep: bool,
    pub seen: u32,
    pub principal: f64,
    pub filed: usize,
}

impl Record {
    pub fn model(&self) -> (f64, f64) {
        self.peers
    }
}

pub const COMBINE_MIN: usize = 500;

pub fn rule_weight(rows: &[(f64, f64, f64)]) -> f64 {
    if rows.len() < COMBINE_MIN {
        return 0.0;
    }
    let (mut num, mut den) = (0.0, 0.0);
    for &(price, model, rule) in rows {
        num += (price - model) * (rule - model);
        den += (rule - model).powi(2);
    }
    if den > 0.0 {
        (num / den).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn quarter_of(r: &Record) -> i32 {
    r.ym.div_euclid(3)
}

pub fn combine_weights(records: &[Record]) -> Vec<(i32, f64)> {
    let mut quarters: Vec<i32> = records.iter().map(quarter_of).collect();
    quarters.sort_unstable();
    quarters.dedup();
    quarters
        .into_iter()
        .map(|q| {
            let earlier: Vec<(f64, f64, f64)> =
                records.iter().filter(|r| quarter_of(r) < q && r.seen > 0).map(|r| (r.price, r.peers.0, r.rule)).collect();
            (3 * q + 2, rule_weight(&earlier))
        })
        .collect()
}

pub fn combine(records: &mut [Record]) {
    let weights = combine_weights(records);
    for r in records.iter_mut() {
        let q = 3 * quarter_of(r) + 2;
        let w = weights.iter().find(|(ym, _)| *ym == q).map_or(0.0, |(_, w)| *w);
        r.combined = if r.seen > 0 { r.peers.0 + w * (r.rule - r.peers.0) } else { r.peers.0 };
    }
}

pub const SAME_MARK: f64 = 0.0005;
pub const LOCKSTEP_MOVE: f64 = 0.005;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    pub copy: f64,
    pub rule: f64,
    pub lockstep: bool,
}

fn arrival(o: &Obs) -> usize {
    o.filed_idx.max(o.asof_idx + 1)
}

pub fn rules(obs: &[&Obs], own: &Obs, prev: f64, t: usize) -> Rules {
    let public = |o: &&&Obs| arrival(o) < t;
    let mark_of = |family: usize, ym: i32| obs.iter().filter(public).find(|o| o.family == family && o.ym == ym).map(|o| o.price);
    let peers: Vec<&Obs> = obs.iter().filter(public).filter(|o| o.family != own.family && o.ym == own.ym).copied().collect();
    let copy = if peers.is_empty() { f64::NAN } else { peers.iter().map(|o| o.price).sum::<f64>() / peers.len() as f64 };
    let moves: Vec<f64> = peers.iter().filter_map(|o| mark_of(o.family, o.ym - 3).map(|p| o.price - p)).collect();
    let rule = if moves.is_empty() || !prev.is_finite() { prev } else { prev + moves.iter().sum::<f64>() / moves.len() as f64 };
    let lockstep = obs.iter().filter(public).filter(|o| o.family == own.family && o.ym < own.ym).any(|mine| {
        let moved = mark_of(own.family, mine.ym - 3).is_some_and(|p| (mine.price - p).abs() >= LOCKSTEP_MOVE);
        moved && obs.iter().filter(public).any(|o| o.family != own.family && o.ym == mine.ym && (o.price - mine.price).abs() <= SAME_MARK)
    });
    Rules { copy, rule, lockstep }
}

pub fn backtest(panel: &Panel, level: &[f64], min_train: usize, progress: impl FnMut(i32, usize, &Fit)) -> Result<Vec<Record>, String> {
    backtest_quarters(panel, level, min_train, None, progress)
}

pub fn backtest_quarters(
    panel: &Panel,
    level: &[f64],
    min_train: usize,
    only: Option<i32>,
    mut progress: impl FnMut(i32, usize, &Fit),
) -> Result<Vec<Record>, String> {
    let engine = Engine::new(panel, level);
    let mut have: Vec<i32> = panel.obs.iter().map(|o| o.ym).filter(|ym| matches!(ym.rem_euclid(12), 2 | 5 | 8 | 11)).collect();
    have.sort_unstable();
    have.dedup();
    if have.len() <= min_train {
        return Err(format!("{} quarters of marks; need more than {min_train} to backtest", have.len()));
    }
    let asof_of: HashMap<i32, usize> = engine.quarters.iter().map(|&(t, ym)| (ym, t)).collect();
    let last_grid = panel.days.len() - 1;

    let mut records = Vec::new();
    for k in min_train..have.len() {
        let test_ym = have[k];
        let prev_ym = have[k - 1];
        if only.is_some_and(|q| q != test_ym) {
            continue;
        }
        let Some(&cut) = asof_of.get(&test_ym) else { continue };

        let fit = calib::fit(panel, level, &|o: &Obs| o.ym < test_ym && o.filed_idx < cut)?;

        let target = |o: &Obs| o.ym > prev_ym && o.ym <= test_ym;
        let insts: Vec<usize> = (0..panel.instruments.len()).filter(|&i| panel.obs_of(i).iter().any(|o| target(o))).collect();
        let end = panel.obs.iter().filter(|o| target(o)).map(|o| o.filed_idx.max(o.asof_idx + 1)).max().unwrap_or(cut).min(last_grid);
        let keep = |o: &Obs| o.ym <= test_ym;

        let raw = engine.run(&fit, RAW, &insts, &keep, end, false);
        let solo = engine.run(&fit, MODEL, &insts, &keep, end, false);
        let full = engine.run(&fit, WITH_SECTOR, &insts, &keep, end, false);
        let parts: Vec<Vec<LoanRun>> =
            PARTS.iter().map(|&(part, _, _)| engine.run(&fit, Variant { without: Some(part), ..MODEL }, &insts, &keep, end, false)).collect();

        let mut n = 0;
        for (k, b) in solo.iter().enumerate() {
            let (a, c) = (&raw[k], &full[k]);
            let kept: Vec<&Obs> = panel.obs_of(b.inst).iter().filter(|o| keep(o)).collect();
            for (j, fb) in b.forecasts.iter().enumerate() {
                let (fa, fc) = (&a.forecasts[j], &c.forecasts[j]);
                let m = &b.marks[fb.mark];
                if m.ym <= prev_ym || m.ym > test_ym || fb.late || !m.prev.is_finite() {
                    continue;
                }
                let simple = rules(&kept, kept[fb.mark], m.prev, fb.arrival);
                records.push(Record {
                    inst: b.inst,
                    family: b.families[fb.mark],
                    ym: m.ym,
                    price: m.price,
                    prev: m.prev,
                    public_raw: (fa.public_mean, fa.public_sd),
                    public: (fb.public_mean, fb.public_sd),
                    peers: (fb.peer_mean, fb.peer_sd),
                    sector: (fc.peer_mean, fc.peer_sd),
                    parts: std::array::from_fn(|p| parts[p][k].forecasts[j].peer_mean),
                    copy: simple.copy,
                    rule: simple.rule,
                    combined: fb.peer_mean,
                    lockstep: simple.lockstep,
                    seen: fb.seen,
                    principal: kept.get(fb.mark).map_or(0.0, |o| o.principal),
                    filed: fb.arrival,
                });
                n += 1;
            }
        }
        progress(test_ym, n, &fit);
    }
    combine(&mut records);
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(family: usize, ym: i32, price: f64, filed_idx: usize) -> Obs {
        Obs { inst: 0, family, asof: 0, ym, asof_idx: 0, filed_idx, price, principal: 1.0, spread_bp: 0.0, original: true }
    }

    #[test]
    fn rules_use_only_marks_public_before_the_forecast() {
        let marks = [
            obs(0, 10, 0.98, 10),
            obs(1, 10, 0.97, 12),
            obs(0, 13, 0.93, 100),
            obs(1, 13, 0.92, 90),
            obs(2, 13, 0.50, 100),
        ];
        let refs: Vec<&Obs> = marks.iter().collect();
        let r = rules(&refs, &marks[2], 0.98, 100);
        assert!((r.copy - 0.92).abs() < 1e-12, "a mark filed the same day is not yet public: {}", r.copy);
        assert!((r.rule - 0.93).abs() < 1e-12, "own last mark moved by the peer's change: {}", r.rule);
        assert!(!r.lockstep);
        let alone = rules(&refs, &marks[2], 0.98, 90);
        assert!(alone.copy.is_nan() && alone.rule == 0.98);
    }

    #[test]
    fn lockstep_needs_an_earlier_move_matched_exactly() {
        let marks = [obs(0, 7, 0.99, 5), obs(0, 10, 0.97, 10), obs(1, 10, 0.9702, 12), obs(0, 13, 0.95, 100)];
        let refs: Vec<&Obs> = marks.iter().collect();
        assert!(rules(&refs, &marks[3], 0.97, 100).lockstep);
        let apart = [obs(0, 7, 0.99, 5), obs(0, 10, 0.97, 10), obs(1, 10, 0.9690, 12), obs(0, 13, 0.95, 100)];
        let refs: Vec<&Obs> = apart.iter().collect();
        assert!(!rules(&refs, &apart[3], 0.97, 100).lockstep);
        let still = [obs(0, 7, 0.97, 5), obs(0, 10, 0.97, 10), obs(1, 10, 0.97, 12), obs(0, 13, 0.95, 100)];
        let refs: Vec<&Obs> = still.iter().collect();
        assert!(!rules(&refs, &still[3], 0.97, 100).lockstep, "matching at an unchanged mark is not lockstep");
    }

    fn record(ym: i32, price: f64, model: f64, rule: f64) -> Record {
        Record {
            inst: 0,
            family: 0,
            ym,
            price,
            prev: 1.0,
            public_raw: (model, 0.01),
            public: (model, 0.01),
            peers: (model, 0.01),
            sector: (model, 0.01),
            parts: [model; N_PARTS],
            copy: f64::NAN,
            rule,
            combined: model,
            lockstep: false,
            seen: 1,
            principal: 1.0,
            filed: 0,
        }
    }

    #[test]
    fn combination_weights_use_earlier_quarters_only() {
        let mut rs: Vec<Record> = (0..COMBINE_MIN).map(|_| record(2, 0.90, 0.95, 0.90)).collect();
        rs.extend((0..COMBINE_MIN).map(|_| record(5, 0.96, 0.96, 0.80)));
        combine(&mut rs);
        let w = combine_weights(&rs);
        assert_eq!(w, vec![(2, 0.0), (5, 1.0)]);
        assert!(rs[0].combined == 0.95, "the first quarter has nothing earlier to learn from");
        assert!((rs[COMBINE_MIN].combined - 0.80).abs() < 1e-12, "the second quarter uses the first quarter's weight");
    }

    #[test]
    fn rule_weight_is_least_squares_and_bounded() {
        let half: Vec<(f64, f64, f64)> = (0..COMBINE_MIN).map(|_| (0.95, 0.96, 0.94)).collect();
        assert!((rule_weight(&half) - 0.5).abs() < 1e-12);
        let away: Vec<(f64, f64, f64)> = (0..COMBINE_MIN).map(|_| (0.97, 0.96, 0.94)).collect();
        assert_eq!(rule_weight(&away), 0.0);
        assert_eq!(rule_weight(&half[..10]), 0.0);
    }
}
