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
