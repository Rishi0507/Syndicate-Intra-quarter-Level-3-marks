//! Why some loans can be made to trade and others cannot: the feedback
//! between information and liquidity.
//!
//! A trade is also a measurement. Each print tells the market what the loan
//! is worth, the estimate's error shrinks, the dealer's adverse selection
//! charge shrinks with it, the quote tightens, and more clients trade. Run it
//! backwards and it is a trap: no trades, no information, wide quotes, no
//! trades.
//!
//! # The process
//!
//! Let `P` be the variance of the market's estimate of a loan's value.
//! Between observations it grows at `sigma^2` a day, the rate at which the
//! value itself diffuses. Two things cut it:
//!
//! * marks, on a fixed schedule, whatever the spread;
//! * trades, arriving at a rate `2 A exp(-k * half_spread(P))` that falls
//!   exponentially in the half-spread, which itself rises with `sqrt(P)`.
//!
//! An observation with noise variance `r` takes `P` to `P r / (P + r)`. This
//! is a piecewise-deterministic Markov process and it is simulated exactly.
//!
//! # The skeleton
//!
//! Replace the arrivals by their average effect and the process becomes an
//! ordinary differential equation,
//!
//! ```text
//! dP/dt = sigma^2 - mu * P^2 / (P + r_m) - lambda(P) * P^2 / (P + r_t)
//! ```
//!
//! whose trade term is hump-shaped in `P`: with little uncertainty there is
//! little left to learn, with much the quote is too wide to trade on. Where
//! the hump clears `sigma^2` the equation has three rest points, two of them
//! stable: a lit one, tight and traded, and a dark one, held up by marks
//! alone. This is a fold, and over two parameters a cusp.
//!
//! The skeleton shows where two regimes exist. It does not describe how a
//! loan moves between them, because a single print on a dark loan is not a
//! small nudge: it is often enough, on its own, to relight it. The real
//! process is metastable. It lingers in each regime and jumps between them,
//! and the right questions are how long it spends in each and what moves
//! that balance. Those are answered by simulation, in `Market::census`.
//!
//! Units are price points and trading days throughout.

use crate::quote::{adverse_selection_width, inventory_width, processing_width, Desk};
use crate::stats::Rng;

/// A trade a month: the threshold for calling a loan tradeable on a day.
pub const TRADEABLE_RATE: f64 = 1.0 / 21.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Market {
    pub desk: Desk,
    /// Daily variance of the loan's value.
    pub daily_var: f64,
    /// Trading days between rounds of marks, and the noise of a round taken
    /// together (one lender's noise over the square root of their number).
    pub mark_gap: f64,
    pub mark_sd: f64,
    /// Noise of a trade print as a reading of value.
    pub print_sd: f64,
    /// Half-spread the dealer gives up to draw flow, and the half-spread
    /// above which it does so. Support is for loans that have gone dark.
    pub subsidy: f64,
    pub subsidy_above: f64,
}

impl Market {
    /// The half-spread at zero inventory when the estimate's variance is `p`,
    /// before any subsidy.
    pub fn fair_half_spread(&self, p: f64) -> f64 {
        processing_width(&self.desk)
            + 0.5 * inventory_width(&self.desk, self.daily_var)
            + adverse_selection_width(self.desk.informed) * p.max(0.0).sqrt()
    }

    /// The half-spread actually quoted.
    pub fn half_spread(&self, p: f64) -> f64 {
        let fair = self.fair_half_spread(p);
        if self.subsidy > 0.0 && fair > self.subsidy_above {
            (fair - self.subsidy).max(0.0)
        } else {
            fair
        }
    }

    /// Trades per day, both sides.
    pub fn trade_rate(&self, p: f64) -> f64 {
        2.0 * self.desk.arrival * (-self.desk.decay * self.half_spread(p)).exp()
    }

    fn gain(p: f64, r: f64) -> f64 {
        if p + r > 0.0 {
            p * p / (p + r)
        } else {
            0.0
        }
    }

    /// Rate of change of the estimate's variance in the skeleton.
    pub fn drift(&self, p: f64) -> f64 {
        let marks = Market::gain(p, self.mark_sd * self.mark_sd) / self.mark_gap.max(1e-9);
        let trades = self.trade_rate(p) * Market::gain(p, self.print_sd * self.print_sd);
        self.daily_var - marks - trades
    }

    /// Every rest point of the skeleton with variance in `(lo, hi)`, each
    /// with whether it is stable. Found by scanning a logarithmic grid for
    /// sign changes of the drift and bisecting; a root where the drift goes
    /// from positive to negative is stable.
    pub fn rest_points(&self, lo: f64, hi: f64) -> Vec<RestPoint> {
        const STEPS: usize = 2000;
        let ratio = (hi / lo).powf(1.0 / STEPS as f64);
        let mut out = Vec::new();
        let (mut a, mut fa) = (lo, self.drift(lo));
        for _ in 0..STEPS {
            let b = a * ratio;
            let fb = self.drift(b);
            if fa == 0.0 || fa * fb < 0.0 {
                let (mut x0, mut x1, f0) = (a, b, fa);
                for _ in 0..70 {
                    let m = 0.5 * (x0 + x1);
                    if self.drift(m) * f0 > 0.0 {
                        x0 = m;
                    } else {
                        x1 = m;
                    }
                }
                let p = 0.5 * (x0 + x1);
                out.push(RestPoint { variance: p, half_spread: self.half_spread(p), trade_rate: self.trade_rate(p), stable: fa > 0.0 });
            }
            a = b;
            fa = fb;
        }
        out
    }

    /// The request-for-quote arrival rate at which variance `p` is a rest
    /// point of the skeleton. The rest points over a range of arrival rates
    /// form a surface; this gives it explicitly as a graph over `p`, with no
    /// root finding. Where it turns back on itself, two regimes coexist.
    pub fn arrival_at_rest(&self, p: f64) -> f64 {
        let marks = Market::gain(p, self.mark_sd * self.mark_sd) / self.mark_gap.max(1e-9);
        let per_unit = 2.0 * (-self.desk.decay * self.half_spread(p)).exp() * Market::gain(p, self.print_sd * self.print_sd);
        if per_unit > 0.0 {
            (self.daily_var - marks) / per_unit
        } else {
            f64::INFINITY
        }
    }

    /// One random history, day by day, starting from variance `p0`.
    pub fn simulate(&self, p0: f64, days: usize, seed: u64) -> Vec<Step> {
        let mut rng = Rng::new(seed);
        let gap = self.mark_gap.round().max(1.0) as usize;
        let (r_m, r_t) = (self.mark_sd * self.mark_sd, self.print_sd * self.print_sd);
        let mut p = p0;
        let mut out = Vec::with_capacity(days);
        for day in 0..days {
            p += self.daily_var;
            let marked = day % gap == gap - 1;
            if marked {
                p = p * r_m / (p + r_m);
            }
            // Trades within the day. Each one changes the rate for the next,
            // so arrivals are drawn one at a time.
            let mut trades = 0u32;
            let mut subsidised = 0u32;
            let mut t = rng.exponential() / self.trade_rate(p).max(1e-12);
            while t < 1.0 && trades < 50 {
                if self.subsidy > 0.0 && self.fair_half_spread(p) > self.subsidy_above {
                    subsidised += 1;
                }
                p = p * r_t / (p + r_t);
                trades += 1;
                t += rng.exponential() / self.trade_rate(p).max(1e-12);
            }
            out.push(Step { sd: p.sqrt(), spread: 2.0 * self.half_spread(p), rate: self.trade_rate(p), trades, subsidised, marked });
        }
        out
    }

    /// Long-run behaviour of the process, from one long simulated history.
    pub fn census(&self, years: usize, seed: u64) -> Census {
        const YEAR: usize = 252;
        const BINS: usize = 40;
        let burn = 2 * YEAR;
        let steps = self.simulate(self.mark_sd * self.mark_sd, burn + years * YEAR, seed);
        let live = &steps[burn..];
        let n = live.len() as f64;

        let mut spreads: Vec<f64> = live.iter().map(|s| s.spread).collect();
        spreads.sort_by(|a, b| a.total_cmp(b));
        let top = live.iter().map(|s| s.sd).fold(0.0, f64::max).max(1e-9);
        let mut hist = vec![0.0; BINS];
        for s in live {
            hist[((s.sd / top * BINS as f64) as usize).min(BINS - 1)] += 1.0 / n;
        }
        Census {
            tradeable: live.iter().filter(|s| s.rate >= TRADEABLE_RATE).count() as f64 / n,
            mean_spread: spreads.iter().sum::<f64>() / n,
            median_spread: spreads[spreads.len() / 2],
            trades_per_year: live.iter().map(|s| s.trades as f64).sum::<f64>() / years as f64,
            subsidy_per_year: live.iter().map(|s| s.subsidised as f64).sum::<f64>() * self.subsidy / years as f64,
            sd_max: top,
            sd_hist: hist,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RestPoint {
    pub variance: f64,
    pub half_spread: f64,
    pub trade_rate: f64,
    pub stable: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Step {
    /// Standard error of the estimate at the close.
    pub sd: f64,
    /// Full bid-ask spread quoted at the close.
    pub spread: f64,
    /// Trades per day expected at that spread.
    pub rate: f64,
    pub trades: u32,
    pub subsidised: u32,
    pub marked: bool,
}

#[derive(Clone, Debug)]
pub struct Census {
    /// Share of days on which the loan would be expected to trade at least
    /// monthly at the quoted spread.
    pub tradeable: f64,
    pub mean_spread: f64,
    pub median_spread: f64,
    pub trades_per_year: f64,
    /// Points per clip given up to subsidies each year.
    pub subsidy_per_year: f64,
    /// Distribution of the estimate's standard error over `[0, sd_max]`.
    pub sd_max: f64,
    pub sd_hist: Vec<f64>,
}
