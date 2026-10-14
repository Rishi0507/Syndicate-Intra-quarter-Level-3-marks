use std::collections::HashMap;

use crate::calib::{self, Fit};
use crate::data::{is_calendar_quarter, year_month, Obs, Panel};
use crate::filter::{self, Forecast, MarkIn, Point, Run, SectorIn};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variant {
    pub habits: bool,
    pub sector: bool,
}

pub const FULL: Variant = Variant { habits: true, sector: true };

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
    pub full: (f64, f64),
    pub seen: u32,
}

pub fn backtest(panel: &Panel, level: &[f64], min_train: usize, mut progress: impl FnMut(i32, usize, &Fit)) -> Result<Vec<Record>, String> {
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
        let Some(&cut) = asof_of.get(&test_ym) else { continue };

        let fit = calib::fit(panel, level, &|o: &Obs| o.ym < test_ym && o.filed_idx < cut)?;

        let target = |o: &Obs| o.ym > prev_ym && o.ym <= test_ym;
        let insts: Vec<usize> = (0..panel.instruments.len()).filter(|&i| panel.obs_of(i).iter().any(|o| target(o))).collect();
        let end = panel.obs.iter().filter(|o| target(o)).map(|o| o.filed_idx.max(o.asof_idx + 1)).max().unwrap_or(cut).min(last_grid);
        let keep = |o: &Obs| o.ym <= test_ym;

        let raw = engine.run(&fit, Variant { habits: false, sector: false }, &insts, &keep, end, false);
        let solo = engine.run(&fit, Variant { habits: true, sector: false }, &insts, &keep, end, false);
        let full = engine.run(&fit, FULL, &insts, &keep, end, false);

        let mut n = 0;
        for ((a, b), c) in raw.iter().zip(&solo).zip(&full) {
            for ((fa, fb), fc) in a.forecasts.iter().zip(&b.forecasts).zip(&c.forecasts) {
                let m = &b.marks[fb.mark];
                if m.ym <= prev_ym || m.ym > test_ym || fb.late || !m.prev.is_finite() {
                    continue;
                }
                records.push(Record {
                    inst: b.inst,
                    family: b.families[fb.mark],
                    ym: m.ym,
                    price: m.price,
                    prev: m.prev,
                    public_raw: (fa.public_mean, fa.public_sd),
                    public: (fb.public_mean, fb.public_sd),
                    peers: (fb.peer_mean, fb.peer_sd),
                    full: (fc.peer_mean, fc.peer_sd),
                    seen: fb.seen,
                });
                n += 1;
            }
        }
        progress(test_ym, n, &fit);
    }
    Ok(records)
}
