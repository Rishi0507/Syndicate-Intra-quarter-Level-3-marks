//! From an estimate with an error bar to a firm two-sided quote.
//!
//! A dealer asked to bid on a loan that has never traded charges for three
//! things, and the spread is their sum (the decomposition of Huang and Stoll):
//!
//! * making the market at all: the margin a dealer earns when clients are not
//!   infinitely price-sensitive;
//! * carrying inventory: the risk that the value moves while the position is
//!   held;
//! * being picked off: the chance that the client knows the value better
//!   than the dealer does.
//!
//! The first two come from the optimal market-making solution of Gueant,
//! Lehalle and Fernandez-Tapia for a dealer answering requests for quotes.
//! The third comes from the zero-profit condition of Glosten and Milgrom and
//! is where the filter's uncertainty enters: it is proportional to the
//! standard error of the mark. Halve the error bar and that part of the
//! spread halves with it.
//!
//! Everything here is in price points (par is 100) and trading days.

use crate::stats::{norm_cdf, norm_pdf};

/// The desk's situation and the market it faces. None of this can be
/// estimated from marks; these are assumptions, to be varied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Desk {
    /// Requests for quotes per day on each side that a zero spread would win.
    pub arrival: f64,
    /// How fast the chance of winning a request falls with the half-spread
    /// quoted, per point.
    pub decay: f64,
    /// The dealer's risk aversion, per point of profit-and-loss.
    pub gamma: f64,
    /// Share of clients who know the loan's value.
    pub informed: f64,
    /// Position held, in clips; positive is long.
    pub inventory: f64,
}

impl Default for Desk {
    fn default() -> Self {
        Desk { arrival: 0.2, decay: 1.5, gamma: 0.05, informed: 0.25, inventory: 0.0 }
    }
}

/// A two-sided quote and what it is made of.
#[derive(Clone, Copy, Debug, Default)]
pub struct Quote {
    pub mid: f64,
    pub bid: f64,
    pub ask: f64,
    /// Half the bid-ask spread at zero inventory, and its three parts.
    pub half_spread: f64,
    pub processing: f64,
    pub inventory_risk: f64,
    pub adverse_selection: f64,
    /// Shift of both sides against the position held.
    pub skew: f64,
    /// Expected trades per day on each side at this quote.
    pub bid_rate: f64,
    pub ask_rate: f64,
}

/// The adverse selection half-spread in units of the mark's standard error.
///
/// A dealer quoting `d` standard errors from its estimate breaks even when
/// what it earns from uninformed clients equals what it loses to informed
/// ones, who trade only when the quote is wrong in their favour:
///
/// ```text
/// (1 - pi) / 2 * d = pi * (phi(d) - d * Phi(-d))
/// ```
///
/// The right side is the expected overshoot of a standard normal beyond `d`.
/// The left rises and the right falls in `d`, so the root is unique.
pub fn adverse_selection_width(informed: f64) -> f64 {
    let pi = informed.clamp(0.0, 0.999);
    if pi <= 0.0 {
        return 0.0;
    }
    let gap = |d: f64| pi * (norm_pdf(d) - d * norm_cdf(-d)) - 0.5 * (1.0 - pi) * d;
    let (mut lo, mut hi) = (0.0, 1.0);
    while gap(hi) > 0.0 {
        hi *= 2.0;
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if gap(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// The inventory term of the Gueant-Lehalle-Fernandez-Tapia quotes: how far
/// one clip of inventory moves each side, and twice the risk premium in the
/// half-spread at zero inventory.
pub fn inventory_width(desk: &Desk, daily_var: f64) -> f64 {
    let (a, k, g) = (desk.arrival.max(1e-9), desk.decay.max(1e-9), desk.gamma.max(1e-9));
    (daily_var * g / (2.0 * k * a) * (1.0 + g / k).powf(1.0 + k / g)).sqrt()
}

/// The margin a dealer keeps when clients are not perfectly price-sensitive.
/// It tends to `1 / decay` as risk aversion vanishes.
pub fn processing_width(desk: &Desk) -> f64 {
    let (k, g) = (desk.decay.max(1e-9), desk.gamma.max(1e-9));
    (1.0 + g / k).ln() / g
}

/// Quotes a loan.
///
/// * `mid`: the filter's estimate of value.
/// * `sd_now`: its standard error today.
/// * `daily_var`: variance of one day's change in value, including the
///   expected contribution of jumps.
pub fn quote(desk: &Desk, mid: f64, sd_now: f64, daily_var: f64) -> Quote {
    let processing = processing_width(desk);
    let xi = inventory_width(desk, daily_var);
    let inventory_risk = 0.5 * xi;
    let adverse_selection = adverse_selection_width(desk.informed) * sd_now;
    let half_spread = processing + inventory_risk + adverse_selection;
    let skew = desk.inventory * xi;
    let (bid_edge, ask_edge) = (half_spread + skew, half_spread - skew);
    Quote {
        mid,
        bid: mid - bid_edge,
        ask: mid + ask_edge,
        half_spread,
        processing,
        inventory_risk,
        adverse_selection,
        skew,
        bid_rate: desk.arrival * (-desk.decay * bid_edge).exp(),
        ask_rate: desk.arrival * (-desk.decay * ask_edge).exp(),
    }
}

/// How much one trade print tightens the quote. A print is an observation of
/// value with noise `print_sd`; it cuts the mark's variance from `p` to
/// `p * r / (p + r)`, and the adverse selection half-spread falls in step.
pub fn print_value(desk: &Desk, sd_now: f64, print_sd: f64) -> (f64, f64) {
    let (p, r) = (sd_now * sd_now, print_sd * print_sd);
    let after = if p + r > 0.0 { (p * r / (p + r)).sqrt() } else { 0.0 };
    (after, adverse_selection_width(desk.informed) * (sd_now - after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adverse_selection_solves_its_equation() {
        for &pi in &[0.05, 0.25, 0.5, 0.9] {
            let d = adverse_selection_width(pi);
            let lhs = 0.5 * (1.0 - pi) * d;
            let rhs = pi * (norm_pdf(d) - d * norm_cdf(-d));
            assert!((lhs - rhs).abs() < 1e-12, "pi = {pi}");
        }
        assert_eq!(adverse_selection_width(0.0), 0.0);
        // More informed clients, wider quote.
        assert!(adverse_selection_width(0.5) > adverse_selection_width(0.25));
        // For a small informed share the root is close to 2 * pi * phi(0).
        let small = adverse_selection_width(0.01);
        assert!((small / (2.0 * 0.01 * norm_pdf(0.0)) - 1.0).abs() < 0.03);
    }

    #[test]
    fn quote_is_symmetric_when_flat_and_skews_with_inventory() {
        let flat = quote(&Desk::default(), 97.0, 0.8, 0.01);
        assert!((flat.mid - flat.bid - (flat.ask - flat.mid)).abs() < 1e-12);
        assert!((flat.half_spread - (flat.processing + flat.inventory_risk + flat.adverse_selection)).abs() < 1e-12);
        assert!((flat.bid_rate - flat.ask_rate).abs() < 1e-12);

        // A long desk lowers both sides: it wants to sell and not to buy.
        let long = quote(&Desk { inventory: 2.0, ..Desk::default() }, 97.0, 0.8, 0.01);
        assert!(long.bid < flat.bid && long.ask < flat.ask);
        assert!(long.ask_rate > flat.ask_rate && long.bid_rate < flat.bid_rate);
        // The spread itself does not change with inventory, only its centre.
        assert!(((long.ask - long.bid) - (flat.ask - flat.bid)).abs() < 1e-12);
    }

    #[test]
    fn spread_widens_with_uncertainty_and_volatility() {
        let d = Desk::default();
        assert!(quote(&d, 97.0, 1.6, 0.01).half_spread > quote(&d, 97.0, 0.8, 0.01).half_spread);
        assert!(quote(&d, 97.0, 0.8, 0.04).half_spread > quote(&d, 97.0, 0.8, 0.01).half_spread);
        // With no uncertainty about value the adverse selection part is gone.
        assert_eq!(quote(&d, 97.0, 0.0, 0.01).adverse_selection, 0.0);
    }

    #[test]
    fn processing_margin_tends_to_one_over_decay() {
        let d = Desk { gamma: 1e-7, decay: 2.0, ..Desk::default() };
        assert!((processing_width(&d) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_print_tightens_the_quote() {
        let d = Desk::default();
        let (after, saved) = print_value(&d, 1.2, 0.25);
        assert!(after < 0.25 && after > 0.24);
        assert!((saved - adverse_selection_width(d.informed) * (1.2 - after)).abs() < 1e-12);
        // A print cannot make things worse, and a perfect one removes all doubt.
        assert!(print_value(&d, 1.2, 0.0).0 == 0.0);
    }
}
