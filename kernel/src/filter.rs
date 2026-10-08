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
