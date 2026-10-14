//! Estimates every parameter of the model from the marks themselves.
//!
//! Nothing is hand-set. The estimation runs in six steps, each identified by
//! a different slice of the data:
//!
//! 1. How strongly marks follow public credit markets, and where they drift
//!    absent news: toward par for a performing loan, wherever the data say
//!    for a stressed one.
//! 2. How much of last quarter's news each lender had yet to put in its
//!    mark. A fair value cannot be forecast from its own past. So when what
//!    the other lenders reported last quarter predicts a lender's change this
//!    quarter, that lender is marking late. No public market is needed.
//! 3. Each lender's level bias, and how far its view of a loan sits from the
//!    other lenders' view of the same loan on the same date.
//! 4. How much of that distance is still there a quarter later.
//! 5. How much of the unexplained movement is shared within a sector.
//! 6. How loan-specific volatility grows as a loan becomes stressed.

use std::collections::HashMap;

use crate::data::{Obs, Panel};
use crate::filter::{slow, stress, view, Dynamics, MarkIn, V_REF};
use crate::stats::{mad_sigma, median, quantile, Ols};

/// How one manager family marks.
#[derive(Clone, Debug)]
pub struct FamilyFit {
    /// Share of its previous mark carried into the next, when the change
    /// touches stress. See `filter::slow`.
    pub phi: f64,
    /// Level bias against other lenders, in price.
    pub bias: f64,
    /// How far its view sits from fair value at par, beyond its bias, as a
    /// standard deviation in price.
    pub noise: f64,
    /// Observations behind phi, and behind bias and noise.
    pub n_moves: usize,
    pub n_overlap: usize,
}

/// What the estimation saw, for reporting.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub n_changes: usize,
    pub n_quarters: usize,
    /// Share of quarterly consensus variance explained by the public factor.
    pub r2_public: f64,
    /// Share of each public sensitivity kept after allowing for how few
    /// independent quarters it was estimated from.
    pub beta_kept: [f64; 2],
    /// Residual sum of squares when the public factor is assumed to reach
    /// marks with each candidate delay. Reported, not used: see step 2.
    pub phi_profile: Vec<(f64, f64)>,
    /// Smoothing measured from lead-lag between lenders: where the lender's
    /// own previous mark was already below the reference price, and where
    /// only the other lenders had the loan there. The two are pooled.
    pub phi_stressed: f64,
    pub phi_entering: f64,
    pub n_lead_lag: usize,
    /// Pairs of consecutive quarters behind the carry of disagreement, and
    /// the slope of a lender's gap on its own gap one and two quarters back.
    pub n_carry: usize,
    pub carry_lags: [f64; 2],
    /// Quarterly standard deviation of the sector shock.
    pub sector_q_sd: f64,
    /// Mean stress, quarterly loan-specific sd and count in each stress bin.
    pub idio_bins: Vec<(f64, f64, usize)>,
    pub noise_median: f64,
    pub days_per_quarter: f64,
}

#[derive(Clone, Debug)]
pub struct Fit {
    pub dynamics: Dynamics,
    /// Average share of a mark carried over from the previous quarter, on
    /// changes that touch stress.
    pub phi_bar: f64,
    /// Share of a lender's distance from fair value still there a quarter on.
    pub carry: f64,
    pub families: Vec<FamilyFit>,
    pub diag: Diagnostics,
}

/// No lender is taken to carry more of its old mark than this. Undoing
/// smoothing divides by what is left, and a small remainder turns valuation
/// noise into nonsense.
pub const PHI_MAX: f64 = 0.6;

impl Fit {
    /// Reads every mark at face value: no smoothing, and disagreement taken
    /// as fresh noise each quarter. This is the model without its account of
    /// how lenders behave, kept to measure what that account is worth.
    pub fn without_lender_habits(&self) -> Fit {
        let mut f = self.clone();
        for fam in f.families.iter_mut() {
            fam.phi = 0.0;
        }
        f.carry = 0.0;
        f
    }
}

/// Fills gaps in a level series forward and returns the day-over-day fall,
/// so a positive entry is a tightening of spreads.
pub fn shocks_from(level: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut filled = vec![f64::NAN; level.len()];
    let mut last = level.iter().copied().find(|x| x.is_finite()).unwrap_or(0.0);
    for (t, &x) in level.iter().enumerate() {
        if x.is_finite() {
            last = x;
        }
        filled[t] = last;
    }
    let mut shocks = vec![0.0; level.len()];
    for t in 1..level.len() {
        shocks[t] = -(filled[t] - filled[t - 1]);
    }
    (filled, shocks)
}

fn quarter_end(ym: i32) -> i32 {
    let (y, m) = (ym.div_euclid(12), ym.rem_euclid(12) + 1);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let d = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if leap => 29,
        _ => 28,
    };
    y * 10000 + m * 100 + d
}

/// All lenders' marks of one loan on one date.
struct Cell {
    inst: usize,
    ym: i32,
    asof_idx: usize,
    lo: usize,
    hi: usize,
}

struct Entry {
    family: usize,
    price: f64,
    /// The same lender's price one and two quarters earlier, or NaN.
    prev: f64,
    prev2: f64,
    /// Price with the lender's smoothing undone.
    implied: f64,
}

struct Cells {
    cells: Vec<Cell>,
    entries: Vec<Entry>,
}

impl Cells {
    fn build(obs: &[&Obs]) -> Cells {
        let mut cells: Vec<Cell> = Vec::new();
        let mut entries: Vec<Entry> = Vec::with_capacity(obs.len());
        for o in obs {
            let fresh = match cells.last() {
                Some(c) => c.inst != o.inst || c.ym != o.ym,
                None => true,
            };
            if fresh {
                cells.push(Cell { inst: o.inst, ym: o.ym, asof_idx: o.asof_idx, lo: entries.len(), hi: entries.len() });
            }
            entries.push(Entry { family: o.family, price: o.price, prev: f64::NAN, prev2: f64::NAN, implied: o.price });
            cells.last_mut().unwrap().hi = entries.len();
        }
        let mut c = Cells { cells, entries };
        // Cells are in date order within a loan, so by the time a cell is
        // reached the one before it already knows its own previous prices.
        for ci in 0..c.cells.len() {
            if let Some(pi) = c.prev_cell(ci) {
                let (plo, phi) = (c.cells[pi].lo, c.cells[pi].hi);
                for e in c.cells[ci].lo..c.cells[ci].hi {
                    let fam = c.entries[e].family;
                    if let Some(p) = (plo..phi).find(|&k| c.entries[k].family == fam) {
                        c.entries[e].prev = c.entries[p].price;
                        c.entries[e].prev2 = c.entries[p].prev;
                    }
                }
            }
        }
        c
    }

    /// The same loan's cell exactly one quarter earlier. Lenders on different
    /// fiscal calendars interleave, so it is not always the adjacent cell.
    fn prev_cell(&self, ci: usize) -> Option<usize> {
        let c = &self.cells[ci];
        (ci.saturating_sub(4)..ci).rev().find(|&k| self.cells[k].inst == c.inst && self.cells[k].ym == c.ym - 3)
    }

    fn of(&self, ci: usize) -> &[Entry] {
        &self.entries[self.cells[ci].lo..self.cells[ci].hi]
    }
}

fn mean_by<F: Fn(&Entry) -> f64>(es: &[Entry], f: F) -> f64 {
    es.iter().map(f).sum::<f64>() / es.len() as f64
}

const LAGS: usize = 4;

/// Fits the model to every observation `keep` accepts. `level` is the public
/// spread factor on the panel's daily grid.
pub fn fit(panel: &Panel, level: &[f64], keep: &dyn Fn(&Obs) -> bool) -> Result<Fit, String> {
    let obs: Vec<&Obs> = panel.obs.iter().filter(|o| keep(o)).collect();
    if obs.len() < 200 {
        return Err(format!("only {} observations to calibrate on", obs.len()));
    }
    let n_fam = panel.families.len();
    let mut cells = Cells::build(&obs);
    let (filled, _) = shocks_from(level);
    let mut diag = Diagnostics::default();

    // ---- 1. Public beta, drift and pull to par ----------------------------
    struct Change {
        ym: i32,
        dc: f64,
        st: f64,
        /// Distance below the settling level, and above it.
        gap: f64,
        over: f64,
        x: [f64; LAGS],
        weight: f64,
    }
    // Where performing loans settle. A mark below it tends to drift back.
    let par = {
        let mut v: Vec<f64> =
            (0..cells.cells.len()).map(|ci| mean_by(cells.of(ci), |e| e.price)).filter(|c| *c >= V_REF).collect();
        if v.len() >= 20 {
            median(&mut v)
        } else {
            0.99
        }
    };
    let gap_of = |v: f64| if v >= V_REF { (par - v).max(0.0) } else { 0.0 };
    let over_of = |v: f64| if v >= V_REF { (v - par).max(0.0) } else { 0.0 };
    let mut changes: Vec<Change> = Vec::new();
    let mut quarters: Vec<i32> = Vec::new();
    for ci in 0..cells.cells.len() {
        let Some(pi) = cells.prev_cell(ci) else { continue };
        let c = &cells.cells[ci];
        let now = mean_by(cells.of(ci), |e| e.price);
        let was = mean_by(cells.of(pi), |e| e.price);
        let mut x = [0.0; LAGS];
        let mut hi = c.asof_idx;
        for (k, xk) in x.iter_mut().enumerate() {
            let lo_date = quarter_end(c.ym - 3 * (k as i32 + 1));
            if lo_date < panel.days[0] {
                break;
            }
            let lo = panel.day_index(lo_date);
            *xk = -(filled[hi] - filled[lo]);
            hi = lo;
        }
        changes.push(Change { ym: c.ym, dc: now - was, st: stress(was), gap: gap_of(was), over: over_of(was), x, weight: 1.0 });
        quarters.push(c.ym);
    }
    quarters.sort_unstable();
    quarters.dedup();
    diag.n_changes = changes.len();
    diag.n_quarters = quarters.len();
    if changes.len() < 100 {
        return Err("too few consecutive-quarter mark pairs to calibrate on".into());
    }

    // The public factor as it would reach a mark that carries share phi of
    // its predecessor: (1-phi) now, (1-phi) phi a quarter late, and so on.
    let lagged = |x: &[f64; LAGS], phi: f64| -> f64 {
        let mut z = 0.0;
        let mut w = 1.0 - phi;
        for xk in x {
            z += w * xk;
            w *= phi;
        }
        z
    };
    let row = |c: &Change, phi: f64| -> [f64; 7] {
        let z = lagged(&c.x, phi);
        [1.0, c.st, z, c.st * z, c.gap, if c.st > 0.0 { 1.0 } else { 0.0 }, c.over]
    };
    let regress = |changes: &[Change], phi: f64| -> Option<(Vec<f64>, f64)> {
        let mut o = Ols::new(7);
        for c in changes {
            o.add(&row(c, phi), c.dc, c.weight);
        }
        o.solve()
    };

    // Defaults are large, rare moves. One Huber pass keeps them from deciding
    // the sensitivity of the thousands of loans that did not default.
    if let Some((b, _)) = regress(&changes, 0.0) {
        let res: Vec<f64> = changes.iter().map(|c| c.dc - row(c, 0.0).iter().zip(&b).map(|(x, b)| x * b).sum::<f64>()).collect();
        let scale = mad_sigma(&res).max(1e-5);
        for (c, r) in changes.iter_mut().zip(&res) {
            c.weight = (2.5 * scale / r.abs().max(1e-12)).min(1.0);
        }
    }

    // Public spreads are outside any valuation committee's control, so the
    // delay with which they reach marks would measure smoothing directly, if
    // they reached marks at all. On private loans they barely do, and the
    // profile over delays is too flat to choose from. It is kept as a
    // diagnostic; smoothing is measured in step 2 instead.
    let mut phi = 0.0;
    while phi <= 0.8 + 1e-9 {
        if let Some((_, sse)) = regress(&changes, phi) {
            diag.phi_profile.push((phi, sse));
        }
        phi += 0.02;
    }

    // Every loan in a quarter sees the same move in public spreads, so the
    // sensitivity rests on as many independent observations as there are
    // quarters, not rows. Leaving each quarter out in turn shows how far the
    // estimate moves, and it is shrunk toward zero by that much: a
    // sensitivity that one quarter can overturn is not carried into forecasts.
    let mut all = Ols::new(7);
    let mut by_quarter: HashMap<i32, Ols> = HashMap::new();
    for c in &changes {
        let r = row(c, 0.0);
        all.add(&r, c.dc, c.weight);
        by_quarter.entry(c.ym).or_insert_with(|| Ols::new(7)).add(&r, c.dc, c.weight);
    }
    let raw = all.solve().map(|(b, _)| b).unwrap_or_else(|| vec![0.0; 7]);
    let left_out: Vec<Vec<f64>> = by_quarter.values().filter_map(|q| all.without(q).solve().map(|(b, _)| b)).collect();
    let g = left_out.len() as f64;
    if left_out.len() >= 3 {
        for (j, k) in [(0usize, 2usize), (1, 3)] {
            let mean = left_out.iter().map(|b| b[k]).sum::<f64>() / g;
            let var = (g - 1.0) / g * left_out.iter().map(|b| (b[k] - mean).powi(2)).sum::<f64>();
            if raw[k] > 0.0 {
                diag.beta_kept[j] = (1.0 - var / (raw[k] * raw[k])).max(0.0);
            }
        }
    }
    let (beta0, beta1) = (raw[2].max(0.0) * diag.beta_kept[0], raw[3].max(0.0) * diag.beta_kept[1]);

    // With the sensitivity settled, what remains of the average change is
    // drift: a level, a pull toward par from below and another from above,
    // and a level and slope under stress.
    let drift = {
        let mut o = Ols::new(5);
        for c in &changes {
            let stressed = if c.st > 0.0 { 1.0 } else { 0.0 };
            o.add(&[1.0, c.gap, stressed, c.st, c.over], c.dc - (beta0 + beta1 * c.st) * c.x[0], c.weight);
        }
        o.solve().map(|(b, _)| b).unwrap_or_else(|| vec![0.0; 5])
    };
    // A negative pull would push performing loans away from par for ever.
    let pull_q = drift[1].clamp(0.0, 0.95);
    let pull_up_q = (-drift[4]).clamp(0.0, 0.95);
    let base_at = |v: f64| {
        let st = stress(v);
        drift[0] + pull_q * gap_of(v) - pull_up_q * over_of(v) + if st > 0.0 { drift[2] + drift[3] * st } else { 0.0 }
    };
    let base_of = |c: &Change| drift[0] + pull_q * c.gap - pull_up_q * c.over + if c.st > 0.0 { drift[2] + drift[3] * c.st } else { 0.0 };
    {
        let (mut sw, mut swy, mut swyy, mut sse) = (0.0, 0.0, 0.0, 0.0);
        for c in &changes {
            let fit = (beta0 + beta1 * c.st) * c.x[0];
            let base = base_of(c);
            sw += c.weight;
            swy += c.weight * (c.dc - base);
            swyy += c.weight * (c.dc - base).powi(2);
            sse += c.weight * (c.dc - base - fit).powi(2);
        }
        let sst = swyy - swy * swy / sw;
        diag.r2_public = if sst > 0.0 { (1.0 - sse / sst).max(0.0) } else { 0.0 };
    }

    // ---- 2. Smoothing, from lead and lag between lenders -------------------
    //
    // Write a lender's report as phi of its old mark plus (1 - phi) of its
    // new view. If views follow fair value and fair value cannot be forecast,
    // then the only way last quarter's news can show up in this quarter's
    // change is through the old mark, in proportion phi. Last quarter's news
    // is read off the other lenders, whose valuation errors are their own:
    //
    //   phi = Cov(my change now, others' change last quarter)
    //       / Cov(my change last quarter, others' change last quarter)
    //
    // Changes are taken net of drift and pull to par, which are predictable
    // without being late, and net of what public spreads did, which is not
    // news about the loan and need not be unforecastable. Only quarters in
    // which the others moved by a point or more are used, where the
    // comparison is about news and not noise, and only loans that someone
    // had below the reference price a quarter ago: the lender itself, or
    // the others. That is the set of changes smoothing is applied to.
    const MOVE_MIN: f64 = 0.01;
    const MOVE_CLIP: f64 = 0.10;
    const PRIOR_MOVES: f64 = 100.0;
    const MIN_MOVES: usize = 50;
    let news = |now: f64, was: f64, spreads: f64| {
        (now - was - base_at(was) - (beta0 + beta1 * stress(was)) * spreads).clamp(-MOVE_CLIP, MOVE_CLIP)
    };
    // The fall in public spreads over the quarter to a cell's date, and over
    // the quarter before that.
    let spread_falls = |c: &Cell| -> [f64; 2] {
        let mut out = [0.0; 2];
        let mut hi = c.asof_idx;
        for (k, o) in out.iter_mut().enumerate() {
            let lo_date = quarter_end(c.ym - 3 * (k as i32 + 1));
            if lo_date < panel.days[0] {
                break;
            }
            let lo = panel.day_index(lo_date);
            *o = -(filled[hi] - filled[lo]);
            hi = lo;
        }
        out
    };
    let mut num = vec![0.0; n_fam];
    let mut den = vec![0.0; n_fam];
    let mut n_moves = vec![0usize; n_fam];
    // Pooled over lenders: own previous mark below the reference, or only
    // the others' marks.
    let (mut rnum, mut rden, mut rn) = ([0.0; 2], [0.0; 2], [0usize; 2]);
    for ci in 0..cells.cells.len() {
        let Some(pi) = cells.prev_cell(ci) else { continue };
        let before = cells.of(pi);
        let [this_quarter, last_quarter] = spread_falls(&cells.cells[ci]);
        for e in cells.of(ci) {
            if !e.prev.is_finite() || !e.prev2.is_finite() {
                continue;
            }
            let (mut sum, mut k, mut lowest) = (0.0, 0usize, f64::INFINITY);
            for o in before {
                if o.family != e.family && o.prev.is_finite() {
                    sum += news(o.price, o.prev, last_quarter);
                    k += 1;
                    lowest = lowest.min(o.price);
                }
            }
            if k == 0 || !slow(e.prev, lowest) {
                continue;
            }
            let x = sum / k as f64;
            if x.abs() < MOVE_MIN {
                continue;
            }
            let (now, then) = (news(e.price, e.prev, this_quarter), news(e.prev, e.prev2, last_quarter));
            let r = if e.prev < V_REF { 0 } else { 1 };
            rnum[r] += now * x;
            rden[r] += then * x;
            rn[r] += 1;
            num[e.family] += now * x;
            den[e.family] += then * x;
            n_moves[e.family] += 1;
        }
    }
    let share = |num: f64, den: f64, n: usize| if n >= MIN_MOVES && den > 0.0 { (num / den).clamp(0.0, PHI_MAX) } else { 0.0 };
    let n_all = rn[0] + rn[1];
    let phi_all = share(rnum[0] + rnum[1], rden[0] + rden[1], n_all);
    diag.phi_stressed = share(rnum[0], rden[0], rn[0]);
    diag.phi_entering = share(rnum[1], rden[1], rn[1]);
    diag.n_lead_lag = n_all;
    // A lender is taken to be like the rest until its own shared moves say
    // otherwise. The prior is worth a hundred of them.
    let typical_den = if n_all > 0 { (rden[0] + rden[1]) / n_all as f64 } else { 0.0 };
    let mut families: Vec<FamilyFit> = (0..n_fam)
        .map(|f| {
            let prior = PRIOR_MOVES * typical_den;
            let own = if typical_den > 0.0 && den[f] + prior > 0.0 { (num[f] + prior * phi_all) / (den[f] + prior) } else { phi_all };
            FamilyFit { phi: own.clamp(0.0, PHI_MAX), bias: 0.0, noise: 0.0, n_moves: n_moves[f], n_overlap: 0 }
        })
        .collect();

    // ---- 3. Bias and distance, on de-smoothed marks ------------------------
    for e in cells.entries.iter_mut() {
        e.implied = view(e.price, e.prev, families[e.family].phi);
    }
    let mut gaps: Vec<Vec<f64>> = vec![Vec::new(); n_fam];
    for ci in 0..cells.cells.len() {
        let es = cells.of(ci);
        if es.len() < 2 {
            continue;
        }
        let total: f64 = es.iter().map(|e| e.implied).sum();
        let consensus = total / es.len() as f64;
        if consensus < V_REF {
            continue;
        }
        for e in es {
            gaps[e.family].push(e.implied - (total - e.implied) / (es.len() - 1) as f64);
        }
    }
    for f in 0..n_fam {
        let n = gaps[f].len();
        families[f].n_overlap = n;
        if n >= 5 {
            let med = median(&mut gaps[f]);
            families[f].bias = med * n as f64 / (n as f64 + 30.0);
        }
    }
    // A gap between one lender and the mean of the others mixes both sides'
    // distance from fair value. Unpick it by fixed-point iteration over
    //   Var[gap] = r_f^2 + sum_{j != f} r_j^2 / (n-1)^2.
    // `dist` keeps each entry's gap for step 4.
    let mut dist = vec![f64::NAN; cells.entries.len()];
    let mut sq: Vec<Vec<(f64, Vec<usize>)>> = vec![Vec::new(); n_fam];
    for ci in 0..cells.cells.len() {
        let es = cells.of(ci);
        if es.len() < 2 {
            continue;
        }
        let adj: Vec<f64> = es.iter().map(|e| e.implied - families[e.family].bias).collect();
        let total: f64 = adj.iter().sum();
        if total / es.len() as f64 <= V_REF {
            continue;
        }
        for (k, e) in es.iter().enumerate() {
            let gap = adj[k] - (total - adj[k]) / (es.len() - 1) as f64;
            let others = es.iter().enumerate().filter(|(j, _)| *j != k).map(|(_, o)| o.family).collect();
            sq[e.family].push((gap, others));
            dist[cells.cells[ci].lo + k] = gap;
        }
    }
    let mut pooled_gaps: Vec<f64> = sq.iter().flatten().map(|(g, _)| *g).collect();
    // Two lenders at equal distance r differ by r * sqrt(2).
    let typical = if pooled_gaps.is_empty() { 0.003 } else { mad_sigma(&pooled_gaps) / std::f64::consts::SQRT_2 };
    let typical = typical.max(0.0005);
    let gap_scale = if pooled_gaps.is_empty() { typical } else { mad_sigma(&pooled_gaps).max(1e-6) };
    pooled_gaps.clear();
    // Gaps have a tight core and a long tail, so their scale is taken from
    // the middle. The others' share must then be taken from the middle too:
    // averaged, a few erratic lenders would outweigh the whole core and
    // leave a careful lender with no distance of its own at all. Nor is any
    // lender taken to sit closer to fair value than half the typical: a
    // lender believed four times as exact as its peers would outvote
    // sixteen of them, and on the day it holds a different tranche it does.
    let floor = 0.5 * typical;
    let mut noise = vec![typical; n_fam];
    for _ in 0..8 {
        let mut next = noise.clone();
        for f in 0..n_fam {
            if sq[f].len() < 8 {
                next[f] = typical;
                continue;
            }
            let g: Vec<f64> = sq[f].iter().map(|(g, _)| *g).collect();
            let total_var = mad_sigma(&g).powi(2);
            let mut others: Vec<f64> =
                sq[f].iter().map(|(_, o)| o.iter().map(|&j| noise[j] * noise[j]).sum::<f64>() / (o.len() * o.len()) as f64).collect();
            let own = (total_var - median(&mut others)).max(0.0).sqrt();
            // Shrink toward the typical lender in proportion to evidence.
            let w = sq[f].len() as f64 / (sq[f].len() as f64 + 20.0);
            next[f] = (w * own + (1.0 - w) * typical).max(floor);
        }
        noise = next;
    }
    for f in 0..n_fam {
        families[f].noise = noise[f];
    }
    diag.noise_median = median(&mut noise.clone());

    // ---- 4. How much of the distance carries --------------------------------
    //
    // A lender's gap to the others has two parts. One is a standing view,
    // which fades slowly. The other is this quarter's wobble, gone by the
    // next. Regress the gap on itself one quarter back and the wobble dilutes
    // the slope; regress it two quarters back and the same dilution appears,
    // with the standing view faded twice. The ratio of the two slopes is
    // therefore the rate at which the standing view fades on its own, and
    // that is the share a lender is taken to carry. Near zero, disagreement
    // is fresh noise each quarter. Near one, it is a difference of opinion
    // that outlasts the year, and a lender's next mark is better forecast
    // from its own last one than from what everyone else thinks. Too little
    // persistence to measure is none.
    const CARRY_MIN: f64 = 0.10;
    const CARRY_MAX: f64 = 0.95;
    let carry = {
        // Slopes over ordinary gaps. Gaps beyond three of their own
        // deviations are left out as regressors: they are differences of
        // another kind, handled by the filter's own rule. What an ordinary
        // gap turns into is capped at twice that, so that one default does
        // not set the slope.
        let core = 3.0 * gap_scale;
        let mut sums = [(0.0, 0.0, 0usize); 2];
        for ci in 0..cells.cells.len() {
            let Some(pi) = cells.prev_cell(ci) else { continue };
            let back = [Some(pi), cells.prev_cell(pi)];
            let lo = cells.cells[ci].lo;
            for (k, e) in cells.of(ci).iter().enumerate() {
                let now = dist[lo + k];
                if !now.is_finite() {
                    continue;
                }
                for (lag, cell) in back.iter().enumerate() {
                    let Some(bi) = *cell else { continue };
                    let Some(p) = cells.of(bi).iter().position(|o| o.family == e.family) else { continue };
                    let was = dist[cells.cells[bi].lo + p];
                    if !was.is_finite() || was.abs() > core {
                        continue;
                    }
                    sums[lag].0 += was * now.clamp(-2.0 * core, 2.0 * core);
                    sums[lag].1 += was * was;
                    sums[lag].2 += 1;
                }
            }
        }
        let slope = |s: (f64, f64, usize)| if s.2 >= 200 && s.1 > 0.0 { s.0 / s.1 } else { 0.0 };
        let (one, two) = (slope(sums[0]), slope(sums[1]));
        diag.n_carry = sums[0].2;
        diag.carry_lags = [one, two];
        if one < CARRY_MIN {
            0.0
        } else if two > 0.0 {
            // No less than the one-quarter slope itself.
            (two / one).clamp(one.min(CARRY_MAX), CARRY_MAX)
        } else {
            one.min(CARRY_MAX)
        }
    };

    // How the distance grows with stress: disagreement on a stressed loan,
    // relative to what the lenders involved would show at par.
    let (mut snum, mut sden) = (0.0, 0.0);
    for ci in 0..cells.cells.len() {
        let es = cells.of(ci);
        if es.len() < 2 {
            continue;
        }
        let adj: Vec<f64> = es.iter().map(|e| e.implied - families[e.family].bias).collect();
        let mean = adj.iter().sum::<f64>() / adj.len() as f64;
        let st = stress(mean);
        if st <= 0.0 {
            continue;
        }
        let ss: f64 = adj.iter().map(|a| (a - mean).powi(2)).sum::<f64>() / (adj.len() - 1) as f64;
        let at_par: f64 = es.iter().map(|e| families[e.family].noise.powi(2)).sum::<f64>() / es.len() as f64;
        // sqrt(ss / at_par) estimates 1 + noise1 * st; cap single cells so one
        // disputed default does not set the slope.
        let ratio = (ss / at_par).sqrt().min(60.0);
        snum += st * (ratio - 1.0);
        sden += st * st;
    }
    let noise1 = if sden > 0.0 { (snum / sden).clamp(0.0, 60.0) } else { 0.0 };

    // ---- 5 and 6. Sector and loan-specific volatility ----------------------
    //
    // Work with the consensus of de-smoothed, de-biased marks. Its quarterly
    // change, less drift and the public-market part, splits into a component
    // shared by a sector in a quarter and a remainder specific to the loan.
    struct Resid {
        group: (usize, i32),
        st: f64,
        u: f64,
        noise_var: f64,
    }
    let consensus = |ci: usize| -> f64 {
        let es = cells.of(ci);
        es.iter().map(|e| e.implied - families[e.family].bias).sum::<f64>() / es.len() as f64
    };
    // Variance the lenders' own distances add to a change in the consensus.
    // What a lender carries from one quarter to the next cancels in the
    // change; only what it redraws, and any change of cast, remains.
    let change_noise = |ci: usize, pi: usize| -> f64 {
        let (now, was) = (cells.of(ci), cells.of(pi));
        let own = |es: &[Entry]| es.iter().map(|e| families[e.family].noise.powi(2)).sum::<f64>() / (es.len() * es.len()) as f64;
        let stayed: f64 = now.iter().filter(|e| was.iter().any(|w| w.family == e.family)).map(|e| families[e.family].noise.powi(2)).sum();
        own(now) + own(was) - 2.0 * carry * stayed / (now.len() * was.len()) as f64
    };
    let mut resid: Vec<Resid> = Vec::new();
    let mut span_days = Vec::new();
    for ci in 0..cells.cells.len() {
        let Some(pi) = cells.prev_cell(ci) else { continue };
        let c = &cells.cells[ci];
        let (now, was) = (consensus(ci), consensus(pi));
        let st = stress(was);
        let x0 = -(filled[c.asof_idx] - filled[cells.cells[pi].asof_idx]);
        let scale = (1.0 + noise1 * st).powi(2);
        resid.push(Resid {
            group: (panel.borrowers[panel.instruments[c.inst].borrower].sector, c.ym),
            st,
            u: now - was - base_at(was) - (beta0 + beta1 * st) * x0,
            noise_var: change_noise(ci, pi).max(0.0) * scale,
        });
        span_days.push((c.asof_idx - cells.cells[pi].asof_idx) as f64);
    }
    let dpq = if span_days.is_empty() { 63.0 } else { median(&mut span_days).max(40.0) };
    diag.days_per_quarter = dpq;

    // Sector shock: variance of sector-quarter means among performing loans,
    // less what sampling alone would produce.
    let mut groups: HashMap<(usize, i32), Vec<f64>> = HashMap::new();
    for r in resid.iter().filter(|r| r.st == 0.0) {
        groups.entry(r.group).or_default().push(r.u);
    }
    let performing: Vec<f64> = resid.iter().filter(|r| r.st == 0.0).map(|r| r.u).collect();
    // Scale from the middle of the distribution. Real marks have a tight core
    // and a long tail of credit events; a variance, even a winsorised one,
    // would let a few defaults set the volatility of every loan that did not.
    // The tail is described separately, as jumps.
    let within = if performing.len() > 10 { mad_sigma(&performing).powi(2) } else { 1e-5 };
    let mut means = Vec::new();
    let mut inv_n = 0.0;
    for g in groups.values().filter(|g| g.len() >= 8) {
        let mut v = g.clone();
        means.push(median(&mut v));
        inv_n += 1.0 / g.len() as f64;
    }
    let sector_q_var = if means.len() >= 6 {
        // The median of n draws has variance about (pi / 2) / n times theirs.
        let sampling = within * std::f64::consts::FRAC_PI_2 * inv_n / means.len() as f64;
        (mad_sigma(&means).powi(2) - sampling).max(0.0)
    } else {
        0.0
    };
    diag.sector_q_sd = sector_q_var.sqrt();
    let group_mean: HashMap<(usize, i32), f64> = groups
        .iter()
        .filter(|(_, g)| g.len() >= 8)
        .map(|(k, g)| {
            let mut v = g.clone();
            (*k, median(&mut v))
        })
        .collect();

    // Loan-specific volatility by stress bin, net of sector and noise.
    let edges = [0.0, 1e-12, 0.03, 0.08, 0.20, f64::INFINITY];
    let mut bins: Vec<(f64, f64, usize)> = Vec::new();
    for w in edges.windows(2) {
        let sel: Vec<&Resid> = resid.iter().filter(|r| r.st >= w[0] && r.st < w[1]).collect();
        if sel.len() < 30 {
            continue;
        }
        let u: Vec<f64> = sel.iter().map(|r| r.u - group_mean.get(&r.group).copied().unwrap_or(0.0)).collect();
        let nv = sel.iter().map(|r| r.noise_var).sum::<f64>() / sel.len() as f64;
        let core = mad_sigma(&u).powi(2);
        // Valuation noise cannot explain more than the core moves it sits
        // inside; where the noise estimate is larger, split the difference.
        let var = (core - nv.min(0.5 * core)).max(1e-10);
        let st = sel.iter().map(|r| r.st).sum::<f64>() / sel.len() as f64;
        bins.push((st, var.sqrt(), sel.len()));
    }
    // Daily sd = idio0 + idio1 * stress, fitted through the bins by weighted
    // least squares. The performing bin anchors the intercept.
    let (idio0, idio1) = {
        let q = dpq.sqrt();
        let base = bins.iter().find(|b| b.0 == 0.0).map(|b| b.1 / q).unwrap_or(0.0006);
        let (mut a, mut b) = (0.0, 0.0);
        for (st, sd, n) in bins.iter().filter(|b| b.0 > 0.0) {
            let w = (*n as f64).sqrt();
            a += w * st * (sd / q - base);
            b += w * st * st;
        }
        (base.max(1e-5), if b > 0.0 { (a / b).max(0.0) } else { 0.0 })
    };
    diag.idio_bins = bins;

    // Jumps: moves of a performing loan too large to be diffusion. Their
    // frequency is the share of residuals beyond five core deviations, and
    // their size a robust scale of those residuals, so that a handful of
    // total write-offs do not set the size of the typical event.
    let (jump_prob, jump_sd) = {
        let u: Vec<f64> =
            resid.iter().filter(|r| r.st == 0.0).map(|r| r.u - group_mean.get(&r.group).copied().unwrap_or(0.0)).collect();
        let core = mad_sigma(&u).max(1e-5);
        let mut tail: Vec<f64> = u.iter().map(|x| x.abs()).filter(|x| *x > 5.0 * core).collect();
        if u.len() >= 200 && tail.len() >= 5 {
            let p = (tail.len() as f64 / u.len() as f64).clamp(0.001, 0.1);
            let typical = 1.482_602_218_505_602 * median(&mut tail);
            (p, typical.clamp(5.0 * core, 0.30))
        } else {
            (0.0, 0.0)
        }
    };

    // A loan seen for the first time: where do first marks sit?
    let mut firsts = Vec::new();
    let first_ym = cells.cells.iter().map(|c| c.ym).min().unwrap_or(0);
    let mut last_inst = usize::MAX;
    for ci in 0..cells.cells.len() {
        let c = &cells.cells[ci];
        if c.inst != last_inst && c.ym > first_ym {
            firsts.push(mean_by(cells.of(ci), |e| e.price));
        }
        last_inst = c.inst;
    }
    let (prior_mean, prior_sd) = if firsts.len() >= 30 {
        let lo = quantile(&mut firsts, 0.05);
        let med = quantile(&mut firsts, 0.5);
        // Spread of the lower tail: a new position can be an old, troubled loan.
        (med, ((med - lo) / 1.645).max(0.01))
    } else {
        (0.985, 0.03)
    };

    let dynamics = Dynamics {
        beta0,
        beta1,
        idio0,
        idio1,
        sector0: (sector_q_var / dpq).sqrt(),
        sector1: 0.0,
        noise1,
        par,
        // A quarterly pull of k compounds from a daily one of 1 - (1-k)^(1/n).
        pull: 1.0 - (1.0 - pull_q).powf(1.0 / dpq),
        pull_up: 1.0 - (1.0 - pull_up_q).powf(1.0 / dpq),
        drift0: drift[0] / dpq,
        drift_s: drift[2] / dpq,
        drift1: drift[3] / dpq,
        jump_prob,
        jump_sd,
        prior_mean,
        prior_sd,
    };
    Ok(Fit { dynamics, phi_bar: phi_all, carry, families, diag })
}

/// Turns one loan's observations into filter inputs, attaching each lender's
/// fitted parameters and its own previous marks.
pub fn marks_for(obs: &[Obs], fit: &Fit) -> Vec<MarkIn> {
    let price = |family: usize, ym: i32| obs.iter().find(|p| p.family == family && p.ym == ym).map_or(f64::NAN, |p| p.price);
    obs.iter()
        .map(|o| {
            let f = &fit.families[o.family];
            let prev = price(o.family, o.ym - 3);
            // What the lender's previous mark implied about fair value, with
            // the smoothing that applied to that mark undone.
            let anchor = if prev.is_finite() {
                view(prev, price(o.family, o.ym - 6), f.phi) - f.bias
            } else {
                f64::NAN
            };
            MarkIn {
                ym: o.ym,
                asof_idx: o.asof_idx,
                filed_idx: o.filed_idx,
                price: o.price,
                prev,
                anchor,
                phi: f.phi,
                bias: f.bias,
                noise: f.noise,
                carry: fit.carry,
                lender: o.family as u32,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shocks_fill_gaps_forward() {
        let (filled, shocks) = shocks_from(&[f64::NAN, 2.0, f64::NAN, 2.5, 2.4]);
        assert_eq!(filled, vec![2.0, 2.0, 2.0, 2.5, 2.4]);
        assert_eq!(shocks[0], 0.0);
        assert_eq!(shocks[2], 0.0);
        assert!((shocks[3] + 0.5).abs() < 1e-12);
        assert!((shocks[4] - 0.1).abs() < 1e-12);
    }

    #[test]
    fn quarter_ends_are_month_ends() {
        assert_eq!(quarter_end(crate::data::year_month(20240331)), 20240331);
        assert_eq!(quarter_end(crate::data::year_month(20240229)), 20240229);
        assert_eq!(quarter_end(crate::data::year_month(20230228)), 20230228);
        assert_eq!(quarter_end(crate::data::year_month(20251231) + 3), 20260331);
    }
}
