//! The state-space model of a loan that does not trade.
//!
//! A loan's fair value `v` is never observed. It moves every day: with public
//! credit markets, with a shock shared by its sector, and with news of its
//! own. What is observed arrives late and rarely. A lender's quarter-end mark
//! measures the value on the reporting date but becomes public only when the
//! filing lands weeks afterwards, by which time the value has moved on.
//!
//! A measurement of the past is handled by keeping the past in the state. On
//! each reporting date the filter copies the current value into a slot. The
//! slot stays correlated with the present, so when a mark of that date finally
//! arrives, updating the slot also corrects today's estimate by exactly the
//! right amount. This is the textbook treatment of delayed measurements and
//! it is exact for a linear Gaussian model.
//!
//! The same device carries the sector shock. Each slot also freezes the
//! sector's cumulative move over the quarter, so marks on other loans in the
//! sector, once filed, inform this loan through that component alone.
//!
//! And it carries disagreement. Two lenders that mark the same loan a point
//! apart this quarter will mostly still be a point apart next quarter: a
//! lender's distance from fair value is a habit, not a fresh draw. If that
//! distance carries over at rate `a`, then a mark and the same lender's
//! previous one together measure `v_now - a * v_then` with only the fresh part
//! of the disagreement as noise. Both values are in the state, so this is an
//! ordinary linear measurement, and a lender's own history anchors its marks
//! without a single extra state per lender.
//!
//! Not every distance fades. Lenders ten points apart for two years are not
//! arguing about one loan: one holds the first-out piece and the other the
//! last-out, or one was repaid in a restructuring that left the other
//! behind. So only the core of a distance, up to a few of its own standard
//! deviations, fades at rate `a`; whatever lies beyond the core stays. A
//! lender far from the rest is then carried at a rate close to one, which
//! also bounds its say: it can pull the estimate toward itself only slowly,
//! and a majority that agrees will always outweigh it.

/// The reference price below which a loan is treated as stressed.
pub const V_REF: f64 = 0.97;

/// How far below the reference a price sits; zero for a performing loan.
///
/// Sensitivity, volatility and disagreement all scale with it. A price is
/// bounded below as well as above, and a loan marked at five cannot be
/// uncertain by twenty, so stress is never taken to exceed the price itself:
/// it peaks half way down and falls away again toward nothing.
#[inline]
pub fn stress(v: f64) -> f64 {
    (V_REF - v).max(0.0).min(v.max(0.0))
}

/// Slope of `stress` in the price.
#[inline]
fn stress_slope(v: f64) -> f64 {
    if v >= V_REF || v <= 0.0 {
        0.0
    } else if v < 0.5 * V_REF {
        1.0
    } else {
        -1.0
    }
}

/// Parameters of the latent value process. Prices are fractions of par and
/// time is in trading days.
#[derive(Clone, Debug, PartialEq)]
pub struct Dynamics {
    /// Price gain per unit fall in the public spread factor, for a loan at par.
    pub beta0: f64,
    /// Additional sensitivity per unit of stress.
    pub beta1: f64,
    /// Daily standard deviation of loan-specific news at par.
    pub idio0: f64,
    /// Additional loan-specific volatility per unit of stress.
    pub idio1: f64,
    /// Daily standard deviation of the sector shock at par.
    pub sector0: f64,
    /// Additional sector volatility per unit of stress.
    pub sector1: f64,
    /// How fast valuation noise grows with stress: sd = r * (1 + noise1 * stress).
    pub noise1: f64,
    /// Where a performing loan's mark settles, and the daily rates at which
    /// a performing loan is pulled back to it from below and from above. The
    /// two differ: a loan below par is waiting to be believed, a loan above
    /// it is waiting to be repaid.
    pub par: f64,
    pub pull: f64,
    pub pull_up: f64,
    /// Daily drift common to all loans.
    pub drift0: f64,
    /// Additional daily drift of a stressed loan: a level, and a slope per
    /// unit of stress. Stressed loans are not pulled to par; whether they
    /// recover or keep falling is left to the data.
    pub drift_s: f64,
    pub drift1: f64,
    /// Probability that a loan's value jumped during a quarter, and the
    /// standard deviation of the jump. Credit does not diffuse into default.
    pub jump_prob: f64,
    pub jump_sd: f64,
    /// Belief about a loan seen for the first time.
    pub prior_mean: f64,
    pub prior_sd: f64,
}

/// Number of parameters in `Dynamics`, and their order when flattened.
pub const DYNAMICS_LEN: usize = 17;

impl Dynamics {
    /// The parameters as a flat vector, in the order `from_slice` reads them.
    /// This is how they cross into the browser.
    pub fn to_vec(&self) -> Vec<f64> {
        vec![
            self.beta0,
            self.beta1,
            self.idio0,
            self.idio1,
            self.sector0,
            self.sector1,
            self.noise1,
            self.par,
            self.pull,
            self.drift0,
            self.drift_s,
            self.drift1,
            self.jump_prob,
            self.jump_sd,
            self.prior_mean,
            self.prior_sd,
            self.pull_up,
        ]
    }

    pub fn from_slice(v: &[f64]) -> Dynamics {
        Dynamics {
            beta0: v[0],
            beta1: v[1],
            idio0: v[2],
            idio1: v[3],
            sector0: v[4],
            sector1: v[5],
            noise1: v[6],
            par: v[7],
            pull: v[8],
            drift0: v[9],
            drift_s: v[10],
            drift1: v[11],
            jump_prob: v[12],
            jump_sd: v[13],
            prior_mean: v[14],
            prior_sd: v[15],
            pull_up: v[16],
        }
    }

    #[inline]
    pub fn beta(&self, v: f64) -> f64 {
        self.beta0 + self.beta1 * stress(v)
    }

    #[inline]
    pub fn idio_var(&self, v: f64) -> f64 {
        let s = self.idio0 + self.idio1 * stress(v);
        s * s
    }

    #[inline]
    pub fn sector_var(&self, v: f64) -> f64 {
        let s = self.sector0 + self.sector1 * stress(v);
        s * s
    }

    /// Total daily variance of the value process.
    #[inline]
    pub fn daily_var(&self, v: f64) -> f64 {
        self.idio_var(v) + self.sector_var(v)
    }

    #[inline]
    pub fn noise_scale(&self, v: f64) -> f64 {
        1.0 + self.noise1 * stress(v)
    }

    /// Posterior probability that a jump lies behind a surprise of size
    /// `innov` whose variance without a jump would be `s`.
    ///
    /// A Gaussian filter reads a ten-point fall in a mark as a twenty-sigma
    /// valuation error and discounts it. That is the wrong inference in
    /// credit, where large moves are mostly real. Comparing the likelihood of
    /// the surprise with and without a jump lets small surprises be filtered
    /// as noise and large ones be believed.
    pub fn jump_weight(&self, innov: f64, s: f64) -> f64 {
        if self.jump_prob <= 0.0 || self.jump_sd <= 0.0 || !(s > 0.0) || !innov.is_finite() {
            return 0.0;
        }
        let s1 = s + self.jump_sd * self.jump_sd;
        // Work with the log ratio so far-out surprises do not underflow.
        let log_ratio = (self.jump_prob / (1.0 - self.jump_prob)).ln() + 0.5 * (s / s1).ln()
            + 0.5 * innov * innov * (1.0 / s - 1.0 / s1);
        1.0 / (1.0 + (-log_ratio).exp())
    }

    /// Expected one-day change in value absent any shock. A performing loan
    /// is pulled to its settling level from either side. A stressed one is
    /// not; whether it recovers or keeps falling is left to the data.
    #[inline]
    pub fn drift(&self, v: f64) -> f64 {
        if v >= V_REF {
            let rate = if v < self.par { self.pull } else { self.pull_up };
            self.drift0 + rate * (self.par - v)
        } else {
            self.drift0 + self.drift_s + self.drift1 * stress(v)
        }
    }

    /// Derivative of the one-day map `v -> v + drift(v) + beta(v) * shock`.
    /// The map is piecewise linear in v, and its slope is what scales the
    /// covariance through a step.
    #[inline]
    pub fn slope(&self, v: f64, shock: f64) -> f64 {
        if v < V_REF {
            1.0 + stress_slope(v) * (self.drift1 + self.beta1 * shock)
        } else if v < self.par {
            1.0 - self.pull
        } else {
            1.0 - self.pull_up
        }
    }
}

/// Reporting dates a loan can have outstanding at once. A mark is read
/// against the value a quarter before it, so the previous date must still be
/// held when the mark is filed; five slots keep it there for a loan whose
/// lenders are split between calendar and off-cycle fiscal quarters.
pub const MAX_SLOTS: usize = 5;
const DIM: usize = 2 + 2 * MAX_SLOTS;

/// Grid days after which an unfilled reporting date is given up on.
const SLOT_LIFE: usize = 140;

const V: usize = 0; // current value
const C: usize = 1; // sector shock accumulated since the last calendar quarter end

#[derive(Clone, Copy, Debug)]
struct Slot {
    ym: i32,
    opened: usize,
    /// Belief about the value on the reporting date, before any mark of it.
    prior_mean: f64,
    prior_var: f64,
    /// Variance of the sector shock accumulated over the quarter that ended
    /// on this date, as believed on the date.
    sector_var: f64,
    seen: u32,
    /// Belief on this date about the value a quarter earlier: mean, variance
    /// and covariance with the value on this date. The mean is NaN if that
    /// earlier date was not being tracked.
    back_mean: f64,
    back_var: f64,
    back_cov: f64,
}

const NO_SLOT: Slot =
    Slot { ym: 0, opened: 0, prior_mean: 0.0, prior_var: 0.0, sector_var: 0.0, seen: 0, back_mean: f64::NAN, back_var: 0.0, back_cov: 0.0 };

/// The joint Gaussian belief about a loan's value now and on its outstanding
/// reporting dates.
#[derive(Clone)]
pub struct State {
    x: [f64; DIM],
    p: [f64; DIM * DIM],
    slots: [Slot; MAX_SLOTS],
    n_slots: usize,
}

impl State {
    pub fn new(mean: f64, sd: f64) -> State {
        let mut s = State {
            x: [0.0; DIM],
            p: [0.0; DIM * DIM],
            slots: [NO_SLOT; MAX_SLOTS],
            n_slots: 0,
        };
        s.x[V] = mean;
        s.p[V * DIM + V] = sd * sd;
        s
    }

    #[inline]
    fn n(&self) -> usize {
        2 + 2 * self.n_slots
    }

    pub fn value(&self) -> f64 {
        self.x[V]
    }

    pub fn value_var(&self) -> f64 {
        self.p[V * DIM + V].max(0.0)
    }

    /// Advances one trading day. `shock` is the fall in the public spread
    /// factor over the day, so a positive shock lifts prices.
    pub fn predict(&mut self, shock: f64, d: &Dynamics) {
        let v = self.x[V];
        // Drift and sensitivity both depend on the price itself, so the map
        // is nonlinear in v. Its slope enters the covariance: a loan being
        // pulled to par sheds uncertainty, a stressed one gains it when
        // spreads gap.
        let slope = d.slope(v, shock);
        self.x[V] = (v + d.drift(v) + d.beta(v) * shock).clamp(0.0, 1.1);
        if slope != 1.0 {
            let n = self.n();
            for j in 0..n {
                self.p[V * DIM + j] *= slope;
                self.p[j * DIM + V] *= slope;
            }
        }
        let qi = d.idio_var(v);
        let qs = d.sector_var(v);
        self.p[V * DIM + V] += qi + qs;
        self.p[V * DIM + C] += qs;
        self.p[C * DIM + V] += qs;
        self.p[C * DIM + C] += qs;
    }

    /// Freezes today's value as the value on a reporting date. On a calendar
    /// quarter end the accumulated sector shock is frozen with it and a new
    /// accumulation starts.
    pub fn open_slot(&mut self, ym: i32, day: usize, calendar: bool) {
        if self.n_slots == MAX_SLOTS {
            self.drop_slot(0);
        }
        let back = self.slot_of(ym - 3).map(|s| 2 + 2 * s);
        let n = self.n();
        let (a, b) = (n, n + 1);
        self.x[a] = self.x[V];
        self.x[b] = self.x[C];
        for j in 0..n {
            self.p[a * DIM + j] = self.p[V * DIM + j];
            self.p[j * DIM + a] = self.p[j * DIM + V];
            self.p[b * DIM + j] = self.p[C * DIM + j];
            self.p[j * DIM + b] = self.p[j * DIM + C];
        }
        self.p[a * DIM + a] = self.p[V * DIM + V];
        self.p[a * DIM + b] = self.p[V * DIM + C];
        self.p[b * DIM + a] = self.p[C * DIM + V];
        self.p[b * DIM + b] = self.p[C * DIM + C];
        self.slots[self.n_slots] = Slot {
            ym,
            opened: day,
            prior_mean: self.x[V],
            prior_var: self.p[V * DIM + V],
            sector_var: self.p[C * DIM + C].max(0.0),
            seen: 0,
            back_mean: back.map_or(f64::NAN, |k| self.x[k]),
            back_var: back.map_or(0.0, |k| self.p[k * DIM + k].max(0.0)),
            back_cov: back.map_or(0.0, |k| self.p[V * DIM + k]),
        };
        self.n_slots += 1;
        if calendar {
            let n = self.n();
            self.x[C] = 0.0;
            for j in 0..n {
                self.p[C * DIM + j] = 0.0;
                self.p[j * DIM + C] = 0.0;
            }
        }
    }

    fn drop_slot(&mut self, s: usize) {
        let n = self.n();
        let cut = 2 + 2 * s;
        // Compact the state by deleting two adjacent dimensions.
        let keep: Vec<usize> = (0..n).filter(|&i| i != cut && i != cut + 1).collect();
        let mut x = [0.0; DIM];
        let mut p = [0.0; DIM * DIM];
        for (ni, &oi) in keep.iter().enumerate() {
            x[ni] = self.x[oi];
            for (nj, &oj) in keep.iter().enumerate() {
                p[ni * DIM + nj] = self.p[oi * DIM + oj];
            }
        }
        self.x = x;
        self.p = p;
        for k in s..self.n_slots - 1 {
            self.slots[k] = self.slots[k + 1];
        }
        self.n_slots -= 1;
    }

    /// Gives up on reporting dates that have been open too long.
    pub fn expire_slots(&mut self, day: usize) {
        while self.n_slots > 0 && day.saturating_sub(self.slots[0].opened) > SLOT_LIFE {
            self.drop_slot(0);
        }
    }

    fn slot_of(&self, ym: i32) -> Option<usize> {
        (0..self.n_slots).find(|&s| self.slots[s].ym == ym)
    }

    /// Belief about the value on a reporting date, given everything seen so
    /// far. `None` once the date has expired.
    pub fn slot_belief(&self, ym: i32) -> Option<Belief> {
        let s = self.slot_of(ym)?;
        let k = 2 + 2 * s;
        let sl = &self.slots[s];
        Some(Belief {
            mean: self.x[k],
            var: self.p[k * DIM + k].max(0.0),
            prior_mean: sl.prior_mean,
            prior_var: sl.prior_var.max(0.0),
            sector_var: sl.sector_var,
            seen: sl.seen,
        })
    }

    /// Where a mark points in the state: the slot of its date and, if the
    /// mark is anchored on the lender's previous view and that date is still
    /// held, the slot a quarter back.
    fn aim(&self, m: &MarkIn) -> Option<(usize, Option<usize>)> {
        let s = self.slot_of(m.ym)?;
        let back = if m.anchored() { self.slot_of(m.ym - 3) } else { None };
        Some((2 + 2 * s, back.map(|b| 2 + 2 * b)))
    }

    /// Mean and variance of `x[k] - a * x[back]`.
    fn combination(&self, k: usize, back: Option<usize>, a: f64) -> (f64, f64) {
        match back {
            Some(b) => (
                self.x[k] - a * self.x[b],
                (self.p[k * DIM + k] - 2.0 * a * self.p[k * DIM + b] + a * a * self.p[b * DIM + b]).max(0.0),
            ),
            None => (self.x[k], self.p[k * DIM + k].max(0.0)),
        }
    }

    /// What the filter believes about the quantity a mark measures, now and
    /// as it stood on the reporting date. `None` once the date has expired.
    pub fn reading(&self, m: &MarkIn, d: &Dynamics) -> Option<Reading> {
        let (k, back) = self.aim(m)?;
        let sl = &self.slots[(k - 2) / 2];
        let tie = back.map(|b| m.tie(self.x[b], d));
        let (mean, var) = self.combination(k, back, tie.map_or(0.0, |t| t.a));
        // On the reporting date the earlier date was either tracked or not;
        // if it was not, the forecast made then could not have been anchored.
        let prior_tie = if back.is_some() && sl.back_mean.is_finite() { Some(m.tie(sl.back_mean, d)) } else { None };
        let (prior_mean, prior_var) = match prior_tie {
            Some(t) => (sl.prior_mean - t.a * sl.back_mean, (sl.prior_var - 2.0 * t.a * sl.back_cov + t.a * t.a * sl.back_var).max(0.0)),
            None => (sl.prior_mean, sl.prior_var.max(0.0)),
        };
        Some(Reading {
            mean,
            var,
            tie,
            level: self.x[k],
            prior_mean,
            prior_var,
            prior_tie,
            prior_level: sl.prior_mean,
            sector_var: sl.sector_var,
            seen: sl.seen,
        })
    }

    /// Conditions on a scalar measurement of one state component.
    fn update(&mut self, k: usize, y: f64, r2: f64) -> (f64, f64) {
        self.update_on(k, None, 0.0, y, r2)
    }

    /// Conditions on a measurement of `x[k] - a * x[back]` with noise
    /// variance `r2`. Returns the innovation and its variance.
    fn update_on(&mut self, k: usize, back: Option<usize>, a: f64, y: f64, r2: f64) -> (f64, f64) {
        let n = self.n();
        let (mean, var) = self.combination(k, back, a);
        let s = var + r2;
        let innov = y - mean;
        if !(s > 0.0) || !innov.is_finite() {
            return (innov, s);
        }
        // Covariance of every component with the measured combination.
        let mut row = [0.0; DIM];
        for i in 0..n {
            row[i] = self.p[i * DIM + k] - back.map_or(0.0, |b| a * self.p[i * DIM + b]);
        }
        for i in 0..n {
            let gain = row[i] / s;
            self.x[i] += gain * innov;
            for j in 0..n {
                self.p[i * DIM + j] -= gain * row[j];
            }
        }
        // Rounding breaks symmetry slowly; restore it so variances stay valid.
        for i in 0..n {
            for j in 0..i {
                let m = 0.5 * (self.p[i * DIM + j] + self.p[j * DIM + i]);
                self.p[i * DIM + j] = m;
                self.p[j * DIM + i] = m;
            }
            if self.p[i * DIM + i] < 0.0 {
                self.p[i * DIM + i] = 0.0;
            }
        }
        (innov, s)
    }

    /// A lender's mark. Returns false if its date is no longer tracked.
    pub fn observe_mark(&mut self, m: &MarkIn, d: &Dynamics) -> bool {
        let Some((k, back)) = self.aim(m) else { return false };
        let tie = back.map(|b| m.tie(self.x[b], d));
        let r = m.noise * d.noise_scale(self.x[k]);
        self.absorb(k, back, tie.map_or(0.0, |t| t.a), m.measured(tie), r * r * m.fresh(tie.is_some()), d);
        true
    }

    /// A direct reading, with noise variance `r2`, of the value on reporting
    /// date `ym`. Returns false if that date is no longer tracked.
    pub fn observe_date(&mut self, ym: i32, y: f64, r2: f64, d: &Dynamics) -> bool {
        let Some(s) = self.slot_of(ym) else { return false };
        self.absorb(2 + 2 * s, None, 0.0, y, r2, d);
        true
    }

    /// Absorbs a measurement of `x[k] - a * x[back]`.
    ///
    /// The first mark of a date is tested for a jump. The belief becomes a
    /// two-part mixture, "no jump" and "jump", each updated on the mark and
    /// weighted by its posterior probability, and is then collapsed back to
    /// one Gaussian with the mixture's mean and covariance. An ordinary mark
    /// leaves the jump branch with negligible weight; a large one hands it
    /// nearly all of it. The jump happened before the reporting date, so it
    /// is shared by that date, every later date and the present, and not by
    /// the date a quarter back.
    fn absorb(&mut self, k: usize, back: Option<usize>, a: f64, y: f64, r2: f64, d: &Dynamics) {
        let s = (k - 2) / 2;
        let w = if self.slots[s].seen == 0 {
            let (mean, var) = self.combination(k, back, a);
            d.jump_weight(y - mean, var + r2)
        } else {
            0.0
        };
        if w > 1e-9 {
            let mut jumped = self.clone();
            let add = d.jump_sd * d.jump_sd;
            let mut shared = [V; 1 + MAX_SLOTS];
            let mut m = 1;
            for later in s..self.n_slots {
                shared[m] = 2 + 2 * later;
                m += 1;
            }
            for &i in &shared[..m] {
                for &j in &shared[..m] {
                    jumped.p[i * DIM + j] += add;
                }
            }
            jumped.update_on(k, back, a, y, r2);
            self.update_on(k, back, a, y, r2);

            let n = self.n();
            let mut x = [0.0; DIM];
            for i in 0..n {
                x[i] = (1.0 - w) * self.x[i] + w * jumped.x[i];
            }
            for i in 0..n {
                for j in 0..n {
                    let quiet = self.p[i * DIM + j] + (self.x[i] - x[i]) * (self.x[j] - x[j]);
                    let loud = jumped.p[i * DIM + j] + (jumped.x[i] - x[i]) * (jumped.x[j] - x[j]);
                    self.p[i * DIM + j] = (1.0 - w) * quiet + w * loud;
                }
            }
            self.x = x;
        } else {
            self.update_on(k, back, a, y, r2);
        }
        self.slots[s].seen += 1;
    }

    /// An estimate of the sector's move over the quarter ending at `ym`,
    /// inferred from other loans.
    pub fn observe_sector(&mut self, ym: i32, y: f64, r2: f64) -> bool {
        match self.slot_of(ym) {
            Some(s) => {
                self.update(3 + 2 * s, y, r2);
                true
            }
            None => false,
        }
    }

    /// A direct observation of today's value, such as a trade print.
    pub fn observe_value(&mut self, y: f64, r2: f64) {
        self.update(V, y, r2);
    }
}

/// What the filter believes about a loan's value on a reporting date.
#[derive(Clone, Copy, Debug)]
pub struct Belief {
    pub mean: f64,
    pub var: f64,
    /// The belief on the reporting date itself, from public markets and
    /// earlier quarters only.
    pub prior_mean: f64,
    pub prior_var: f64,
    /// Variance of the sector shock over the quarter ending on this date.
    pub sector_var: f64,
    /// Marks of this date already absorbed.
    pub seen: u32,
}

/// How a mark is tied to the same lender's view a quarter earlier: the mark
/// measures `v_now - a * v_then`, once `a * anchor` is taken out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tie {
    pub a: f64,
}

/// A distance fades at the lender's carry while it is within this many of
/// its standard deviations. Beyond that it is carried whole.
pub const CORE_SDS: f64 = 3.0;

/// What the filter believes about the quantity one mark measures. For a
/// tied mark that is the value on its date less the carried share of the
/// value a quarter earlier; otherwise it is the value on its date.
#[derive(Clone, Copy, Debug)]
pub struct Reading {
    pub mean: f64,
    pub var: f64,
    pub tie: Option<Tie>,
    /// Value on the reporting date, which sets the scale of valuation noise.
    pub level: f64,
    /// The same on the reporting date itself, from public markets and
    /// earlier quarters only.
    pub prior_mean: f64,
    pub prior_var: f64,
    pub prior_tie: Option<Tie>,
    pub prior_level: f64,
    /// Variance of the sector shock over the quarter ending on this date.
    pub sector_var: f64,
    /// Marks of this date already absorbed.
    pub seen: u32,
}

/// Whether a lender's smoothing bears on a change between two prices. It
/// does once either end is below the reference price.
///
/// Lenders are slow about news, not about wobble. When the other holders of
/// a loan mark it down a point or more, a lender follows with about three
/// quarters of the move and the rest a quarter later. But the small changes
/// of a performing loan do not persist at all, and undoing smoothing there
/// would turn noise into momentum. News about a loan is, almost always, a
/// change that starts or ends under stress.
pub fn slow(a: f64, b: f64) -> bool {
    a < V_REF || b < V_REF
}

/// The view of a loan behind a reported price, for a lender that carries
/// share `phi` of its previous price `prev` into each new one: the report is
/// `phi * prev + (1 - phi) * view`, so the view lies beyond the report, on
/// the far side from the previous price.
///
/// The view is kept between nothing and par. A loan is repaid at par, so a
/// mark that leaps from 45 to 100 is a restructuring, or new paper in place
/// of old, and not a lender half way to a view of 120.
pub fn view(price: f64, prev: f64, phi: f64) -> f64 {
    if !(prev.is_finite() && phi > 0.0 && slow(prev, price)) {
        return price;
    }
    let raw = (price - phi * prev) / (1.0 - phi);
    raw.clamp(price.min(0.0), price.max(1.0))
}

/// A mark as the filter consumes it.
#[derive(Clone, Copy, Debug)]
pub struct MarkIn {
    pub ym: i32,
    pub asof_idx: usize,
    pub filed_idx: usize,
    /// The reported price.
    pub price: f64,
    /// The same lender's price one quarter earlier, or NaN.
    pub prev: f64,
    /// What that earlier mark implied about fair value, or NaN.
    pub anchor: f64,
    /// Share of last quarter's mark carried into this one by the lender.
    pub phi: f64,
    /// The lender's level bias.
    pub bias: f64,
    /// How far the lender's view sits from fair value at par, beyond its
    /// bias, as a standard deviation.
    pub noise: f64,
    /// Share of that distance still there a quarter later.
    pub carry: f64,
    /// Which lender this is, to tell one lender's marks from another's.
    pub lender: u32,
}

impl MarkIn {
    /// Undoes the lender's smoothing and bias, returning what the mark says
    /// about fair value. A lender that reports `phi` of its old mark plus
    /// `1 - phi` of its new view has revealed the new view.
    pub fn implied(&self) -> f64 {
        view(self.price, self.prev, self.phi) - self.bias
    }

    /// Whether the mark can be read against the lender's previous view.
    pub fn anchored(&self) -> bool {
        self.carry > 0.0 && self.anchor.is_finite()
    }

    /// How the mark is tied to the lender's previous view, given the value
    /// believed for that earlier date. Within the core the distance fades at
    /// the carry. Beyond it only the core fades and the excess stays, so the
    /// share of the whole distance that is carried rises toward one.
    pub fn tie(&self, value_then: f64, d: &Dynamics) -> Tie {
        let distance = (self.anchor - value_then).abs();
        let core = CORE_SDS * self.noise * d.noise_scale(value_then);
        if distance <= core {
            Tie { a: self.carry }
        } else {
            Tie { a: 1.0 - (1.0 - self.carry) * core / distance }
        }
    }

    /// The measurement the mark amounts to. Tied, what the lender carried
    /// from its previous view is taken out, leaving the value now less a
    /// share of the value then, plus the fresh part of its disagreement.
    pub fn measured(&self, tie: Option<Tie>) -> f64 {
        match tie {
            Some(t) => self.implied() - t.a * self.anchor,
            None => self.implied(),
        }
    }

    /// Share of the lender's distance that is fresh in a tied mark.
    pub fn fresh(&self, tied: bool) -> f64 {
        if tied {
            1.0 - self.carry * self.carry
        } else {
            1.0
        }
    }

    /// Predicts the reported price from a belief about the measured
    /// quantity, returning mean and standard deviation. `level` is the value
    /// on the reporting date, which scales the noise.
    pub fn predict(&self, mean: f64, var: f64, tie: Option<Tie>, level: f64, d: &Dynamics) -> (f64, f64) {
        let r = self.noise * d.noise_scale(level);
        let view = match tie {
            Some(t) => mean + t.a * self.anchor,
            None => mean,
        };
        let sd = (var + r * r * self.fresh(tie.is_some())).sqrt();
        // What the lender would report if it marked without delay.
        let target = view + self.bias;
        if self.prev.is_finite() && self.phi > 0.0 && slow(self.prev, target) {
            (self.phi * self.prev + (1.0 - self.phi) * target, (1.0 - self.phi) * sd)
        } else {
            (target, sd)
        }
    }
}

/// An estimate of a sector's move over a quarter, available from some day.
#[derive(Clone, Copy, Debug)]
pub struct SectorIn {
    pub ym: i32,
    pub avail_idx: usize,
    pub value: f64,
    pub var: f64,
}

/// A direct observation of current value.
#[derive(Clone, Copy, Debug)]
pub struct PrintIn {
    pub idx: usize,
    pub price: f64,
    pub var: f64,
}

/// The filter's forecast of one mark, made twice: once on the reporting date
/// from public information alone, and once just before the mark was filed,
/// after every other lender's mark of that date already public.
#[derive(Clone, Copy, Debug)]
pub struct Forecast {
    /// Index into the marks passed to `run`.
    pub mark: usize,
    pub public_mean: f64,
    pub public_sd: f64,
    pub peer_mean: f64,
    pub peer_sd: f64,
    /// Other lenders' marks of the same date seen before this one.
    pub seen: u32,
    /// What the mark measures, less the public forecast of it: the surprise
    /// this loan delivered to anyone watching public markets.
    pub surprise: f64,
    /// Variance of that surprise, and the part of it due to the sector.
    pub surprise_var: f64,
    pub sector_var: f64,
    /// Grid day the mark became usable.
    pub arrival: usize,
    /// The mark arrived after its reporting date had expired.
    pub late: bool,
}

/// One day of the real-time estimate.
#[derive(Clone, Copy, Debug, Default)]
pub struct Point {
    pub mean: f64,
    pub sd: f64,
}

/// Inputs to one loan's filter run. Marks must be sorted by `filed_idx`.
pub struct Run<'a> {
    /// Fall in the public spread factor on each grid day.
    pub shocks: &'a [f64],
    /// First and last grid day of the run, inclusive.
    pub start: usize,
    pub end: usize,
    /// Calendar quarter ends inside the run, as (grid day, year-month). A
    /// slot opens on each whether or not this loan has a mark of that date
    /// yet, so that other loans' marks can inform it as they are filed.
    pub quarters: &'a [(usize, i32)],
    pub marks: &'a [MarkIn],
    pub sector: &'a [SectorIn],
    pub prints: &'a [PrintIn],
}

/// Runs the real-time filter for one loan from `start` to `end` inclusive.
/// Forecasts are appended in filing order; if `path` is given it receives the
/// estimate at the close of every day.
pub fn run(r: &Run, d: &Dynamics, forecasts: &mut Vec<Forecast>, mut path: Option<&mut Vec<Point>>) -> State {
    let mut st = State::new(d.prior_mean, d.prior_sd);

    // Reporting dates this loan has marks for, in time order.
    let mut dates: Vec<(usize, i32)> = r.marks.iter().map(|m| (m.asof_idx, m.ym)).collect();
    dates.extend(r.quarters.iter().filter(|q| q.0 >= r.start && q.0 <= r.end));
    dates.sort_unstable();
    dates.dedup_by_key(|d| d.1);
    let mut next_date = 0;

    let mut order: Vec<usize> = (0..r.marks.len()).collect();
    // A mark cannot be known before the day after its reporting date.
    let arrival = |m: &MarkIn| m.filed_idx.max(m.asof_idx + 1);
    order.sort_by_key(|&i| (arrival(&r.marks[i]), i));
    let mut next_mark = 0;

    let mut sector: Vec<&SectorIn> = r.sector.iter().collect();
    sector.sort_by_key(|s| s.avail_idx);
    let mut next_sector = 0;

    let mut prints: Vec<&PrintIn> = r.prints.iter().collect();
    prints.sort_by_key(|p| p.idx);
    let mut next_print = 0;

    for t in r.start..=r.end {
        if t > r.start {
            st.predict(r.shocks[t], d);
        }
        st.expire_slots(t);
        while next_date < dates.len() && dates[next_date].0 <= t {
            let ym = dates[next_date].1;
            st.open_slot(ym, t, matches!(ym.rem_euclid(12), 2 | 5 | 8 | 11));
            next_date += 1;
        }

        while next_sector < sector.len() && sector[next_sector].avail_idx <= t {
            let s = sector[next_sector];
            st.observe_sector(s.ym, s.value, s.var);
            next_sector += 1;
        }

        // Everything filed on one day is forecast from the same information
        // before any of it is absorbed, so same-day filers cannot inform each
        // other's forecasts.
        let first = next_mark;
        while next_mark < order.len() && arrival(&r.marks[order[next_mark]]) <= t {
            let i = order[next_mark];
            let m = &r.marks[i];
            let f = match st.reading(m, d) {
                Some(b) => {
                    let (pm, ps) = m.predict(b.prior_mean, b.prior_var, b.prior_tie, b.prior_level, d);
                    let (xm, xs) = m.predict(b.mean, b.var, b.tie, b.level, d);
                    let noise = m.noise * d.noise_scale(b.prior_level);
                    let fresh = m.fresh(b.prior_tie.is_some());
                    Forecast {
                        mark: i,
                        public_mean: pm,
                        public_sd: ps,
                        peer_mean: xm,
                        peer_sd: xs,
                        seen: b.seen,
                        surprise: m.measured(b.prior_tie) - b.prior_mean,
                        surprise_var: b.prior_var + noise * noise * fresh,
                        sector_var: b.sector_var,
                        arrival: t,
                        late: false,
                    }
                }
                None => Forecast {
                    mark: i,
                    public_mean: f64::NAN,
                    public_sd: f64::NAN,
                    peer_mean: f64::NAN,
                    peer_sd: f64::NAN,
                    seen: 0,
                    surprise: f64::NAN,
                    surprise_var: f64::NAN,
                    sector_var: 0.0,
                    arrival: t,
                    late: true,
                },
            };
            forecasts.push(f);
            next_mark += 1;
        }
        for &i in &order[first..next_mark] {
            // The lender's noise sits on its view of fair value, which is
            // exactly what de-smoothing recovers, so it carries over as is.
            st.observe_mark(&r.marks[i], d);
        }

        while next_print < prints.len() && prints[next_print].idx <= t {
            let p = prints[next_print];
            st.observe_value(p.price, p.var);
            next_print += 1;
        }

        if let Some(out) = path.as_deref_mut() {
            out.push(Point { mean: st.value(), sd: st.value_var().sqrt() });
        }
    }
    st
}

/// The hindsight estimate: the value on every day given every mark ever
/// filed, by a forward filter and a Rauch-Tung-Striebel backward pass.
///
/// With all marks in hand their publication delay no longer matters, so each
/// one is placed on its reporting date. Prints are direct observations of
/// value on their own day.
///
/// Where lenders' disagreement carries between quarters, each lender's
/// distance from fair value is estimated alongside the value itself. Where it
/// does not, that distance is plain noise and the state is the value alone.
pub fn smooth(shocks: &[f64], start: usize, end: usize, marks: &[MarkIn], prints: &[PrintIn], d: &Dynamics) -> Vec<Point> {
    if marks.iter().any(|m| m.carry > 0.0) {
        smooth_carried(shocks, start, end, marks, prints, d)
    } else {
        smooth_plain(shocks, start, end, marks, prints, d)
    }
}

/// Inverts the symmetric positive definite `m` by `m` matrix `a` in place by
/// Gauss-Jordan elimination with pivoting. Returns false if it is singular.
fn invert(a: &mut [f64], m: usize, work: &mut Vec<f64>) -> bool {
    work.clear();
    work.resize(m * m, 0.0);
    for i in 0..m {
        work[i * m + i] = 1.0;
    }
    for c in 0..m {
        let mut piv = c;
        for r in c + 1..m {
            if a[r * m + c].abs() > a[piv * m + c].abs() {
                piv = r;
            }
        }
        if !(a[piv * m + c].abs() > 1e-300) {
            return false;
        }
        if piv != c {
            for j in 0..m {
                a.swap(c * m + j, piv * m + j);
                work.swap(c * m + j, piv * m + j);
            }
        }
        let inv = 1.0 / a[c * m + c];
        for j in 0..m {
            a[c * m + j] *= inv;
            work[c * m + j] *= inv;
        }
        for r in 0..m {
            if r != c {
                let f = a[r * m + c];
                if f != 0.0 {
                    for j in 0..m {
                        a[r * m + j] -= f * a[c * m + j];
                        work[r * m + j] -= f * work[c * m + j];
                    }
                }
            }
        }
    }
    a.copy_from_slice(work);
    true
}

/// The smoother with one extra state per lender: how far its view of the
/// loan sits from fair value. Between a lender's marks that distance stands
/// still; from one quarter's mark to the next it keeps the carried share and
/// redraws the rest. A mark then reads value plus distance exactly.
fn smooth_carried(shocks: &[f64], start: usize, end: usize, marks: &[MarkIn], prints: &[PrintIn], d: &Dynamics) -> Vec<Point> {
    // A mark is an exact reading of value plus distance. A floor on its
    // variance, a thousandth of a point, keeps the algebra well conditioned.
    const EXACT: f64 = 1e-10;
    let days = end + 1 - start;
    let in_range = |m: &MarkIn| m.asof_idx >= start && m.asof_idx <= end;
    let mut ids: Vec<u32> = marks.iter().filter(|m| in_range(m)).map(|m| m.lender).collect();
    ids.sort_unstable();
    ids.dedup();
    let n = 1 + ids.len();
    let col = |lender: u32| 1 + ids.binary_search(&lender).unwrap_or(0);

    let mut prints_by_day: Vec<Vec<usize>> = vec![Vec::new(); days];
    for (i, p) in prints.iter().enumerate() {
        if p.idx >= start && p.idx <= end {
            prints_by_day[p.idx - start].push(i);
        }
    }
    let mut by_day: Vec<Vec<usize>> = vec![Vec::new(); days];
    for (i, m) in marks.iter().enumerate() {
        if in_range(m) {
            by_day[m.asof_idx - start].push(i);
        }
    }

    // Conditions the state on a reading of `x[0] + x[c]`, or of `x[0]` alone
    // when `c` is zero.
    let update = |x: &mut [f64], p: &mut [f64], c: usize, y: f64, r2: f64| {
        let mut row = vec![0.0; n];
        for i in 0..n {
            row[i] = p[i * n] + if c > 0 { p[i * n + c] } else { 0.0 };
        }
        let mean = x[0] + if c > 0 { x[c] } else { 0.0 };
        let s = row[0] + if c > 0 { row[c] } else { 0.0 } + r2;
        if !(s > 0.0) {
            return;
        }
        let innov = y - mean;
        for i in 0..n {
            let gain = row[i] / s;
            x[i] += gain * innov;
            for j in 0..n {
                p[i * n + j] -= gain * row[j];
            }
        }
        for i in 0..n {
            for j in 0..i {
                let m = 0.5 * (p[i * n + j] + p[j * n + i]);
                p[i * n + j] = m;
                p[j * n + i] = m;
            }
            if p[i * n + i] < 0.0 {
                p[i * n + i] = 0.0;
            }
        }
    };

    // As in the plain smoother, a first pass finds the quarters that held a
    // jump and spreads the variance to reach the mark over their days.
    let mut extra = vec![0.0; days];
    let (mut xp, mut pp) = (vec![0.0; days * n], vec![0.0; days * n * n]); // predicted
    let (mut xf, mut pf) = (vec![0.0; days * n], vec![0.0; days * n * n]); // filtered
    // Each day's transition is diagonal: the slope of the value map, and for
    // a lender marking that day the share of its distance carried over.
    let mut step = vec![1.0; days * n];
    for pass in 0..2 {
        let mut x = vec![0.0; n];
        let mut p = vec![0.0; n * n];
        x[0] = d.prior_mean;
        p[0] = d.prior_sd * d.prior_sd;
        let mut last_ym = vec![i32::MIN; n];
        let mut last_obs = 0;
        let mut q = vec![0.0; n];
        for k in 0..days {
            let t = &mut step[k * n..(k + 1) * n];
            t.fill(1.0);
            q.fill(0.0);
            if k > 0 {
                let shock = shocks[start + k];
                t[0] = d.slope(x[0], shock);
                q[0] = d.daily_var(x[0]) + extra[k];
                x[0] = (x[0] + d.drift(x[0]) + d.beta(x[0]) * shock).clamp(0.0, 1.1);
            }
            for &i in &by_day[k] {
                let mk = &marks[i];
                let c = col(mk.lender);
                let r = mk.noise * d.noise_scale(x[0]);
                let carry = if mk.carry > 0.0 && last_ym[c] == mk.ym - 3 { mk.carry.min(0.999) } else { 0.0 };
                q[c] = r * r * (1.0 - carry * carry);
                last_ym[c] = mk.ym;
                // Within the core the distance fades; beyond it, only the
                // core does and the rest stands.
                let core = CORE_SDS * r;
                t[c] = if carry > 0.0 && x[c].abs() > core { 1.0 - (1.0 - carry) * core / x[c].abs() } else { carry };
            }
            for i in 1..n {
                x[i] *= t[i];
            }
            for i in 0..n {
                for j in 0..n {
                    p[i * n + j] *= t[i] * t[j];
                }
                p[i * n + i] += q[i];
            }
            xp[k * n..(k + 1) * n].copy_from_slice(&x);
            pp[k * n * n..(k + 1) * n * n].copy_from_slice(&p);

            for (j, &i) in by_day[k].iter().enumerate() {
                let mk = &marks[i];
                let c = col(mk.lender);
                if pass == 0 && j == 0 && k > last_obs {
                    let innov = mk.implied() - (x[0] + x[c]);
                    let s = p[0] + 2.0 * p[c] + p[c * n + c] + EXACT;
                    if d.jump_weight(innov, s) > 0.5 {
                        let per_day = (innov * innov - s).max(0.0) / (k - last_obs) as f64;
                        for e in extra[last_obs + 1..=k].iter_mut() {
                            *e = per_day;
                        }
                    }
                }
                update(&mut x, &mut p, c, mk.implied(), EXACT);
            }
            for &i in &prints_by_day[k] {
                update(&mut x, &mut p, 0, prints[i].price, prints[i].var);
            }
            if !by_day[k].is_empty() || !prints_by_day[k].is_empty() {
                last_obs = k;
            }
            xf[k * n..(k + 1) * n].copy_from_slice(&x);
            pf[k * n * n..(k + 1) * n * n].copy_from_slice(&p);
        }
        if pass == 0 && extra.iter().all(|e| *e == 0.0) {
            break;
        }
    }

    // Backward pass. A lender not yet seen has no distance to speak of: its
    // row and column are zero, and it is left out of the inversion.
    let mut out = vec![Point::default(); days];
    let mut xs = xf[(days - 1) * n..].to_vec();
    let mut ps = pf[(days - 1) * n * n..].to_vec();
    out[days - 1] = Point { mean: xs[0], sd: ps[0].max(0.0).sqrt() };
    let (mut inv, mut work, mut gain, mut tmp) = (Vec::new(), Vec::new(), vec![0.0; n * n], vec![0.0; n * n]);
    let mut active: Vec<usize> = Vec::with_capacity(n);
    for k in (0..days - 1).rev() {
        let t = &step[(k + 1) * n..(k + 2) * n];
        let pnext = &pp[(k + 1) * n * n..(k + 2) * n * n];
        let pk = &pf[k * n * n..(k + 1) * n * n];
        active.clear();
        active.extend((0..n).filter(|&i| pnext[i * n + i] > 0.0));
        let m = active.len();
        inv.clear();
        inv.resize(m * m, 0.0);
        for (a, &i) in active.iter().enumerate() {
            for (b, &j) in active.iter().enumerate() {
                inv[a * m + b] = pnext[i * n + j];
            }
        }
        gain.fill(0.0);
        if m > 0 && invert(&mut inv, m, &mut work) {
            // gain = P_k T' inv(P_next), on the active columns.
            for i in 0..n {
                for (b, &j) in active.iter().enumerate() {
                    let mut g = 0.0;
                    for (a, &l) in active.iter().enumerate() {
                        g += pk[i * n + l] * t[l] * inv[a * m + b];
                    }
                    gain[i * n + j] = g;
                }
            }
        }
        let mut next_x = xf[k * n..(k + 1) * n].to_vec();
        for i in 0..n {
            for j in 0..n {
                next_x[i] += gain[i * n + j] * (xs[j] - xp[(k + 1) * n + j]);
            }
        }
        // P_k + gain (P_smooth - P_next) gain'
        for i in 0..n {
            for j in 0..n {
                let mut v = 0.0;
                for l in 0..n {
                    v += gain[i * n + l] * (ps[l * n + j] - pnext[l * n + j]);
                }
                tmp[i * n + j] = v;
            }
        }
        let mut next_p = pk.to_vec();
        for i in 0..n {
            for j in 0..n {
                let mut v = 0.0;
                for l in 0..n {
                    v += tmp[i * n + l] * gain[j * n + l];
                }
                next_p[i * n + j] += v;
            }
        }
        xs = next_x;
        ps = next_p;
        out[k] = Point { mean: xs[0], sd: ps[0].max(0.0).sqrt() };
    }
    out
}

fn smooth_plain(shocks: &[f64], start: usize, end: usize, marks: &[MarkIn], prints: &[PrintIn], d: &Dynamics) -> Vec<Point> {
    let n = end + 1 - start;
    let mut prints_by_day: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, p) in prints.iter().enumerate() {
        if p.idx >= start && p.idx <= end {
            prints_by_day[p.idx - start].push(i);
        }
    }
    let mut by_day: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, m) in marks.iter().enumerate() {
        if m.asof_idx >= start && m.asof_idx <= end {
            by_day[m.asof_idx - start].push(i);
        }
    }

    // Extra variance per day from jumps. A first pass finds the quarters
    // that more likely than not contained a jump. Enough variance to reach
    // the mark is then spread over the days since the previous one, because
    // nothing says when in the quarter the jump happened. The second pass is
    // the one that is smoothed.
    let mut extra = vec![0.0; n];
    let (mut mp, mut pp) = (vec![0.0; n], vec![0.0; n]); // predicted
    let (mut mf, mut pf) = (vec![0.0; n], vec![0.0; n]); // filtered
    let mut slope = vec![1.0; n];
    for pass in 0..2 {
        let (mut m, mut p) = (d.prior_mean, d.prior_sd * d.prior_sd);
        let mut last_obs = 0;
        for k in 0..n {
            if k > 0 {
                let shock = shocks[start + k];
                let a = d.slope(m, shock);
                let q = d.daily_var(m) + extra[k];
                m = (m + d.drift(m) + d.beta(m) * shock).clamp(0.0, 1.1);
                p = a * a * p + q;
                slope[k] = a;
            }
            mp[k] = m;
            pp[k] = p;
            for (j, &i) in by_day[k].iter().enumerate() {
                let mk = &marks[i];
                let noise = mk.noise * d.noise_scale(m);
                let r2 = noise * noise;
                if pass == 0 && j == 0 && k > last_obs {
                    let innov = mk.implied() - m;
                    if d.jump_weight(innov, p + r2) > 0.5 {
                        let per_day = (innov * innov - (p + r2)).max(0.0) / (k - last_obs) as f64;
                        for e in extra[last_obs + 1..=k].iter_mut() {
                            *e = per_day;
                        }
                    }
                }
                let s = p + r2;
                if s > 0.0 {
                    let gain = p / s;
                    m += gain * (mk.implied() - m);
                    p *= 1.0 - gain;
                }
            }
            for &i in &prints_by_day[k] {
                let s = p + prints[i].var;
                if s > 0.0 {
                    let gain = p / s;
                    m += gain * (prints[i].price - m);
                    p *= 1.0 - gain;
                }
            }
            if !by_day[k].is_empty() || !prints_by_day[k].is_empty() {
                last_obs = k;
            }
            mf[k] = m;
            pf[k] = p;
        }
        if pass == 0 && extra.iter().all(|e| *e == 0.0) {
            break;
        }
    }

    let mut out = vec![Point::default(); n];
    let (mut ms, mut ps) = (mf[n - 1], pf[n - 1]);
    out[n - 1] = Point { mean: ms, sd: ps.max(0.0).sqrt() };
    for k in (0..n - 1).rev() {
        let g = if pp[k + 1] > 0.0 { pf[k] * slope[k + 1] / pp[k + 1] } else { 0.0 };
        ms = mf[k] + g * (ms - mp[k + 1]);
        ps = pf[k] + g * g * (ps - pp[k + 1]);
        out[k] = Point { mean: ms, sd: ps.max(0.0).sqrt() };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Rng;

    fn dynamics() -> Dynamics {
        Dynamics {
            beta0: 0.012,
            beta1: 0.10,
            idio0: 0.0006,
            idio1: 0.012,
            sector0: 0.0003,
            sector1: 0.0015,
            noise1: 8.0,
            par: 0.992,
            pull: 0.008,
            pull_up: 0.0,
            drift0: 0.0,
            drift_s: 0.0001,
            drift1: -0.0005,
            jump_prob: 0.0,
            jump_sd: 0.0,
            prior_mean: 0.985,
            prior_sd: 0.03,
        }
    }

    /// With no shocks, no stress and one direct observation, the filter must
    /// reproduce the scalar Kalman update exactly.
    #[test]
    fn single_update_matches_closed_form() {
        let mut st = State::new(0.99, 0.02);
        st.observe_value(0.97, 0.01 * 0.01);
        let (p, r) = (0.02f64 * 0.02, 0.01f64 * 0.01);
        let gain = p / (p + r);
        assert!((st.value() - (0.99 + gain * (0.97 - 0.99))).abs() < 1e-15);
        assert!((st.value_var() - p * (1.0 - gain)).abs() < 1e-15);
    }

    /// A mark of a past date must move today's estimate by the covariance
    /// between then and now, not by the full surprise.
    #[test]
    fn delayed_mark_updates_the_present_through_the_slot() {
        let d = Dynamics { beta1: 0.0, idio1: 0.0, sector0: 0.0, sector1: 0.0, noise1: 0.0, pull: 0.0, pull_up: 0.0, drift_s: 0.0, drift1: 0.0, ..dynamics() };
        let mut st = State::new(0.99, 0.01);
        st.open_slot(100, 0, false);
        let days = 30;
        for _ in 0..days {
            st.predict(0.0, &d);
        }
        let p0 = 0.01f64 * 0.01;
        let q = d.idio0 * d.idio0 * days as f64;
        // Before the mark: today's variance has grown, the slot's has not.
        assert!((st.value_var() - (p0 + q)).abs() < 1e-15);
        let r2 = 0.002f64 * 0.002;
        let y = 0.96;
        st.observe_date(100, y, r2, &d);
        // Cov(v_now, v_then) = p0, so the gain on today is p0 / (p0 + r2).
        let gain = p0 / (p0 + r2);
        assert!((st.value() - (0.99 + gain * (y - 0.99))).abs() < 1e-14);
        assert!((st.value_var() - (p0 + q - gain * p0)).abs() < 1e-15);
        // What remains is the slot's posterior plus everything since.
        assert!(st.value_var() > q);
    }

    /// Simulates the model the filter assumes and measures its honesty:
    /// the variance of its standardised forecast errors, which should be one,
    /// and the ratio of its actual to its stated error about today's value,
    /// which should also be one.
    fn honesty(d: &Dynamics, seed: u64) -> (f64, f64) {
        let mut rng = Rng::new(seed);
        let days = 900;
        let quarter = 63;
        let lag = 32;
        let noise = 0.004;

        let (mut z2, mut nz) = (0.0, 0.0);
        let (mut e2, mut p_sum) = (0.0, 0.0);
        for _ in 0..400 {
            let shocks: Vec<f64> = (0..days).map(|_| 0.02 * rng.normal()).collect();
            let mut v = vec![0.0; days];
            v[0] = d.prior_mean + d.prior_sd * rng.normal();
            for t in 1..days {
                let sd = d.daily_var(v[t - 1]).sqrt();
                v[t] = (v[t - 1] + d.drift(v[t - 1]) + d.beta(v[t - 1]) * shocks[t] + sd * rng.normal()).clamp(0.0, 1.1);
            }
            let mut marks = Vec::new();
            let mut ym = 0;
            let mut t = 0;
            while t + lag < days {
                for fam in 0..2 {
                    let r = noise * d.noise_scale(v[t]);
                    marks.push(MarkIn {
                        ym,
                        asof_idx: t,
                        filed_idx: t + lag + 6 * fam,
                        price: v[t] + r * rng.normal(),
                        prev: f64::NAN,
                        anchor: f64::NAN,
                        phi: 0.0,
                        bias: 0.0,
                        noise,
                        carry: 0.0,
                        lender: fam as u32,
                    });
                }
                ym += 3;
                t += quarter;
            }
            let mut fc = Vec::new();
            let mut path = Vec::new();
            run(
                &Run { shocks: &shocks, start: 0, end: days - 1, quarters: &[], marks: &marks, sector: &[], prints: &[] },
                d,
                &mut fc,
                Some(&mut path),
            );
            for f in &fc {
                let m = &marks[f.mark];
                if m.ym == 0 {
                    continue; // the prior is exact by construction; skip it
                }
                let z = (m.price - f.peer_mean) / f.peer_sd;
                z2 += z * z;
                nz += 1.0;
            }
            for t in (quarter..days).step_by(7) {
                let e = v[t] - path[t].mean;
                e2 += e * e;
                p_sum += path[t].sd * path[t].sd;
            }
        }
        (z2 / nz, e2 / p_sum)
    }

    /// In the linear Gaussian case the filter is exact, so both honesty
    /// measures must equal one up to Monte Carlo error.
    #[test]
    fn filter_is_exact_on_the_linear_model() {
        let d = Dynamics { beta1: 0.0, idio1: 0.0, sector1: 0.0, noise1: 0.0, pull: 0.0, pull_up: 0.0, drift_s: 0.0, drift1: 0.0, ..dynamics() };
        let (zvar, ratio) = honesty(&d, 11);
        assert!((zvar - 1.0).abs() < 0.04, "standardised forecast variance {zvar}");
        assert!((ratio - 1.0).abs() < 0.04, "actual over stated error variance {ratio}");
    }

    /// With stress effects on, value dynamics are piecewise linear and the
    /// filter linearises around its own estimate. It should stay close to
    /// honest; this bounds how much the approximation costs.
    #[test]
    fn filter_stays_honest_with_stress_effects() {
        let (zvar, ratio) = honesty(&dynamics(), 11);
        assert!((zvar - 1.0).abs() < 0.15, "standardised forecast variance {zvar}");
        assert!((ratio - 1.0).abs() < 0.15, "actual over stated error variance {ratio}");
    }

    /// A ten-point fall reported by one lender. Without a jump model the
    /// filter treats most of it as valuation error; with one it believes it.
    #[test]
    fn jumps_are_believed() {
        let quiet = Dynamics { beta1: 0.0, idio1: 0.0, sector0: 0.0, sector1: 0.0, noise1: 0.0, pull: 0.0, pull_up: 0.0, drift_s: 0.0, drift1: 0.0, ..dynamics() };
        let jumpy = Dynamics { jump_prob: 0.01, jump_sd: 0.08, ..quiet.clone() };
        let after = |d: &Dynamics| {
            let mut st = State::new(0.99, 0.003);
            st.open_slot(100, 0, false);
            for _ in 0..30 {
                st.predict(0.0, d);
            }
            st.observe_date(100, 0.89, 0.003 * 0.003, d);
            (st.value(), st.value_var().sqrt())
        };
        let (v_quiet, _) = after(&quiet);
        let (v_jumpy, sd_jumpy) = after(&jumpy);
        assert!(v_quiet > 0.93, "without jumps the fall is mostly discounted: {v_quiet}");
        assert!(v_jumpy < 0.895, "with jumps the fall is believed: {v_jumpy}");
        assert!(sd_jumpy < 0.01, "and one mark still pins the new level: {sd_jumpy}");

        // An ordinary surprise must be left alone.
        let small = |d: &Dynamics| {
            let mut st = State::new(0.99, 0.003);
            st.open_slot(100, 0, false);
            st.observe_date(100, 0.987, 0.003 * 0.003, d);
            st.value()
        };
        assert!((small(&quiet) - small(&jumpy)).abs() < 2e-4);
    }

    /// The smoother knows the future, so it can never be less certain than
    /// the filter, and at the final day the two must agree. Both statements
    /// are exact only for the linear model, so stress effects are switched
    /// off here; with them on, the two passes linearise around slightly
    /// different means.
    #[test]
    fn smoother_tightens_the_filter() {
        let d = Dynamics { beta1: 0.0, idio1: 0.0, sector0: 0.0, sector1: 0.0, noise1: 0.0, pull: 0.0, pull_up: 0.0, drift_s: 0.0, drift1: 0.0, ..dynamics() };
        let mut rng = Rng::new(3);
        let days = 400;
        let shocks: Vec<f64> = (0..days).map(|_| 0.02 * rng.normal()).collect();
        let marks: Vec<MarkIn> = (0..6)
            .map(|q| MarkIn {
                ym: 3 * q,
                asof_idx: 63 * q as usize,
                filed_idx: 63 * q as usize + 1,
                price: 0.985 - 0.004 * q as f64,
                prev: f64::NAN,
                anchor: f64::NAN,
                phi: 0.0,
                bias: 0.0,
                noise: 0.003,
                carry: 0.0,
                lender: 0,
            })
            .collect();
        let sm = smooth(&shocks, 0, days - 1, &marks, &[], &d);
        let mut path = Vec::new();
        run(
            &Run { shocks: &shocks, start: 0, end: days - 1, quarters: &[], marks: &marks, sector: &[], prints: &[] },
            &d,
            &mut Vec::new(),
            Some(&mut path),
        );
        for t in 0..days {
            assert!(sm[t].sd <= path[t].sd + 1e-9, "day {t}: smoothed {} > filtered {}", sm[t].sd, path[t].sd);
        }
        assert!((sm[days - 1].mean - path[days - 1].mean).abs() < 1e-9);
        assert!((sm[days - 1].sd - path[days - 1].sd).abs() < 1e-9);
    }

    /// With nothing carried, a lender's distance is redrawn every quarter,
    /// which is plain valuation noise. The two smoothers must then agree.
    /// Noise that grows with stress is switched off: the plain smoother
    /// scales each mark's noise after absorbing the one before it, the
    /// carried one scales all of a day's marks together.
    #[test]
    fn carried_smoother_reduces_to_the_plain_one() {
        let d = Dynamics { noise1: 0.0, ..dynamics() };
        let mut rng = Rng::new(5);
        let days = 400;
        let shocks: Vec<f64> = (0..days).map(|_| 0.02 * rng.normal()).collect();
        let mut marks = Vec::new();
        for q in 0..6 {
            for lender in 0..2u32 {
                marks.push(MarkIn {
                    ym: 3 * q,
                    asof_idx: 20 + 63 * q as usize,
                    filed_idx: 50 + 63 * q as usize,
                    price: 0.985 - 0.006 * q as f64 + 0.004 * rng.normal(),
                    prev: f64::NAN,
                    anchor: f64::NAN,
                    phi: 0.0,
                    bias: 0.0,
                    noise: 0.004,
                    carry: 0.0,
                    lender,
                });
            }
        }
        let prints = [PrintIn { idx: 300, price: 0.95, var: 1e-5 }];
        let plain = smooth_plain(&shocks, 0, days - 1, &marks, &prints, &d);
        let carried = smooth_carried(&shocks, 0, days - 1, &marks, &prints, &d);
        for t in 0..days {
            assert!((plain[t].mean - carried[t].mean).abs() < 2e-6, "day {t}: {} against {}", plain[t].mean, carried[t].mean);
            assert!((plain[t].sd - carried[t].sd).abs() < 2e-6, "day {t}: {} against {}", plain[t].sd, carried[t].sd);
        }
    }

    /// Two lenders a point apart, quarter after quarter, with value moving
    /// underneath. Read as fresh noise, each mark is averaged away and the
    /// forecast of either lender's next mark sits between them. Read as a
    /// standing disagreement, each lender is forecast at its own level.
    #[test]
    fn standing_disagreement_is_carried_into_forecasts() {
        let d = Dynamics { beta1: 0.0, idio1: 0.0, sector0: 0.0, sector1: 0.0, noise1: 0.0, pull: 0.0, pull_up: 0.0, drift_s: 0.0, drift1: 0.0, ..dynamics() };
        let days = 500;
        let shocks = vec![0.0; days];
        let build = |carry: f64| -> Vec<MarkIn> {
            let mut marks = Vec::new();
            for q in 0..7 {
                let value = 0.98 - 0.002 * q as f64;
                for lender in 0..2u32 {
                    let off = if lender == 0 { 0.005 } else { -0.005 };
                    let price = value + off;
                    let prev = if q > 0 { value + 0.002 + off } else { f64::NAN };
                    marks.push(MarkIn {
                        ym: 3 * q,
                        asof_idx: 10 + 63 * q as usize,
                        filed_idx: 40 + 63 * q as usize + 5 * lender as usize,
                        price,
                        prev,
                        anchor: prev,
                        phi: 0.0,
                        bias: 0.0,
                        noise: 0.005,
                        carry,
                        lender,
                    });
                }
            }
            marks
        };
        let error = |carry: f64| -> f64 {
            let marks = build(carry);
            let mut fc = Vec::new();
            run(&Run { shocks: &shocks, start: 0, end: days - 1, quarters: &[], marks: &marks, sector: &[], prints: &[] }, &d, &mut fc, None);
            let late: Vec<&Forecast> = fc.iter().filter(|f| marks[f.mark].ym >= 9).collect();
            late.iter().map(|f| (marks[f.mark].price - f.peer_mean).abs()).sum::<f64>() / late.len() as f64
        };
        let (fresh, carried) = (error(0.0), error(0.9));
        assert!(fresh > 0.003, "as noise, each lender is pulled to the middle: {fresh}");
        assert!(carried < 0.0015, "as a habit, each is forecast where it stands: {carried}");

        // The hindsight value sits between the two lenders either way.
        let marks = build(0.9);
        let sm = smooth(&shocks, 0, days - 1, &marks, &[], &d);
        let at = 10 + 63 * 4;
        assert!((sm[at].mean - (0.98 - 0.008)).abs() < 0.002, "smoothed value {}", sm[at].mean);
    }

    /// A view never leaves the range a loan can be worth.
    #[test]
    fn views_stay_between_nothing_and_par() {
        assert!((view(0.60, 1.00, 0.25) - (0.60 - 0.25) / 0.75).abs() < 1e-15);
        assert_eq!(view(1.00, 0.45, 0.25), 1.00);
        assert_eq!(view(1.02, 0.99, 0.25), 1.02);
        assert_eq!(view(0.10, 1.00, 0.25), 0.0);
        assert_eq!(view(0.93, f64::NAN, 0.25), 0.93);
        assert_eq!(view(0.93, 0.99, 0.0), 0.93);
    }

    /// De-smoothing must invert smoothing: a lender who reports a blend of
    /// old mark and new view has an implied view equal to the new view.
    #[test]
    fn implied_value_inverts_smoothing() {
        let (view, prev, phi, bias) = (0.93, 0.99, 0.4, 0.002);
        let reported = phi * prev + (1.0 - phi) * (view + bias);
        let m =
            MarkIn { ym: 0, asof_idx: 0, filed_idx: 1, price: reported, prev, anchor: f64::NAN, phi, bias, noise: 0.003, carry: 0.0, lender: 0 };
        assert!((m.implied() - view).abs() < 1e-14);
        let d = Dynamics { noise1: 0.0, ..dynamics() };
        let (mean, sd) = m.predict(view, 0.0, None, view, &d);
        assert!((mean - reported).abs() < 1e-14);
        assert!((sd - (1.0 - phi) * 0.003).abs() < 1e-14);
    }

    #[test]
    fn dynamics_survive_flattening() {
        let d = dynamics();
        let v = d.to_vec();
        assert_eq!(v.len(), DYNAMICS_LEN);
        assert_eq!(Dynamics::from_slice(&v), d);
    }

    /// More than MAX_SLOTS reporting dates outstanding must not corrupt state.
    #[test]
    fn slots_overflow_cleanly() {
        let d = dynamics();
        let mut st = State::new(0.99, 0.01);
        for k in 0..8 {
            st.open_slot(k, k as usize, k % 3 == 0);
            st.predict(0.01, &d);
        }
        assert!(st.slot_belief(0).is_none() && st.slot_belief(2).is_none() && st.slot_belief(7).is_some());
        assert!(st.value_var().is_finite() && st.value_var() > 0.0);
    }
}
