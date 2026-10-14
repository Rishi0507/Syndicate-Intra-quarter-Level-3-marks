use crate::quote::{adverse_selection_width, inventory_width, processing_width, Desk};
use crate::stats::Rng;

pub const TRADEABLE_RATE: f64 = 1.0 / 21.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Market {
    pub desk: Desk,
    pub daily_var: f64,
    pub mark_gap: f64,
    pub mark_sd: f64,
    pub print_sd: f64,
    pub subsidy: f64,
    pub subsidy_above: f64,
}

impl Market {
    pub fn fair_half_spread(&self, p: f64) -> f64 {
        processing_width(&self.desk)
            + 0.5 * inventory_width(&self.desk, self.daily_var)
            + adverse_selection_width(self.desk.informed) * p.max(0.0).sqrt()
    }

    pub fn half_spread(&self, p: f64) -> f64 {
        let fair = self.fair_half_spread(p);
        if self.subsidy > 0.0 && fair > self.subsidy_above {
            (fair - self.subsidy).max(0.0)
        } else {
            fair
        }
    }

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

    pub fn drift(&self, p: f64) -> f64 {
        let marks = Market::gain(p, self.mark_sd * self.mark_sd) / self.mark_gap.max(1e-9);
        let trades = self.trade_rate(p) * Market::gain(p, self.print_sd * self.print_sd);
        self.daily_var - marks - trades
    }

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

    pub fn arrival_at_rest(&self, p: f64) -> f64 {
        let marks = Market::gain(p, self.mark_sd * self.mark_sd) / self.mark_gap.max(1e-9);
        let per_unit = 2.0 * (-self.desk.decay * self.half_spread(p)).exp() * Market::gain(p, self.print_sd * self.print_sd);
        if per_unit > 0.0 {
            (self.daily_var - marks) / per_unit
        } else {
            f64::INFINITY
        }
    }

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
    pub sd: f64,
    pub spread: f64,
    pub rate: f64,
    pub trades: u32,
    pub subsidised: u32,
    pub marked: bool,
}

#[derive(Clone, Debug)]
pub struct Census {
    pub tradeable: f64,
    pub mean_spread: f64,
    pub median_spread: f64,
    pub trades_per_year: f64,
    pub subsidy_per_year: f64,
    pub sd_max: f64,
    pub sd_hist: Vec<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn bistable() -> Market {
        Market {
            desk: Desk { arrival: 2.0, decay: 8.0, gamma: 0.02, informed: 0.45, inventory: 0.0 },
            daily_var: 0.0625,
            mark_gap: 63.0,
            mark_sd: 1.0,
            print_sd: 0.35,
            subsidy: 0.0,
            subsidy_above: 0.0,
        }
    }

    #[test]
    fn rest_points_alternate_and_solve_the_equation() {
        let m = bistable();
        let rest = m.rest_points(1e-6, 1e4);
        assert_eq!(rest.len(), 3, "{rest:?}");
        assert!(rest[0].stable && !rest[1].stable && rest[2].stable);
        for r in &rest {
            assert!(m.drift(r.variance).abs() < 1e-9, "{r:?}");
        }
        assert!(rest[0].half_spread < 0.5 * rest[2].half_spread);
        assert!(rest[0].trade_rate > 20.0 * rest[2].trade_rate);
    }

    #[test]
    fn manifold_agrees_with_the_rest_points() {
        let m = bistable();
        for r in m.rest_points(1e-6, 1e4) {
            let a = m.arrival_at_rest(r.variance);
            assert!((a / m.desk.arrival - 1.0).abs() < 1e-6, "{r:?} -> {a}");
        }
    }

    #[test]
    fn the_fold_closes_at_both_ends() {
        let m = bistable();
        let busy = Market { desk: Desk { arrival: 60.0, ..m.desk }, ..m };
        let rest = busy.rest_points(1e-6, 1e4);
        assert!(rest.len() == 1 && rest[0].stable && rest[0].trade_rate > 1.0);
        let dead = Market { desk: Desk { arrival: 0.01, ..m.desk }, ..m };
        let rest = dead.rest_points(1e-6, 1e4);
        assert!(rest.len() == 1 && rest[0].stable && rest[0].trade_rate < 0.01);
    }

    #[test]
    fn simulation_is_reproducible_and_observations_only_reduce_variance() {
        let m = bistable();
        let a = m.simulate(1.0, 500, 9);
        let b = m.simulate(1.0, 500, 9);
        assert!(a.iter().zip(&b).all(|(x, y)| x.sd == y.sd && x.trades == y.trades));
        for w in a.windows(2) {
            if w[1].trades == 0 && !w[1].marked {
                assert!((w[1].sd * w[1].sd - w[0].sd * w[0].sd - m.daily_var).abs() < 1e-9);
            } else {
                assert!(w[1].sd * w[1].sd < w[0].sd * w[0].sd + m.daily_var);
            }
        }
    }

    #[test]
    fn marks_flow_and_subsidy_each_light_the_market() {
        let m = bistable();
        let base = m.census(200, 4);
        assert!(base.tradeable > 0.05 && base.tradeable < 0.95, "base case should sit between regimes: {}", base.tradeable);

        let monthly = Market { mark_gap: 21.0, ..m }.census(200, 4);
        let daily = Market { mark_gap: 1.0, ..m }.census(200, 4);
        assert!(monthly.tradeable > base.tradeable + 0.05, "monthly {} vs quarterly {}", monthly.tradeable, base.tradeable);
        assert!(daily.tradeable > monthly.tradeable, "daily {} vs monthly {}", daily.tradeable, monthly.tradeable);
        assert!(daily.mean_spread < monthly.mean_spread && monthly.mean_spread < base.mean_spread);

        let busier = Market { desk: Desk { arrival: 2.0 * m.desk.arrival, ..m.desk }, ..m }.census(200, 4);
        assert!(busier.tradeable > base.tradeable + 0.05);

        let helped = Market { subsidy: 0.3, subsidy_above: 0.5, ..m }.census(200, 4);
        assert!(helped.tradeable > base.tradeable + 0.05, "helped {} vs base {}", helped.tradeable, base.tradeable);
        assert!(helped.subsidy_per_year > 0.0 && base.subsidy_per_year == 0.0);

        let hist: f64 = base.sd_hist.iter().sum();
        assert!((hist - 1.0).abs() < 1e-9);
    }
}
