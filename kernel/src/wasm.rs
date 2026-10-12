//! The interface the browser calls.
//!
//! Plain C functions over the module's linear memory: the page allocates a
//! buffer with `px_alloc`, writes 64-bit floats into it, calls a function and
//! reads floats back. No generated glue, no serialisation. Every function
//! here is a thin wrapper around code that the native tests cover, so a
//! number on screen is a number that was tested.
//!
//! Row layouts, all `f64`:
//!
//! | array     | columns                                                        |
//! |-----------|----------------------------------------------------------------|
//! | dynamics  | the sixteen fields of `Dynamics`, in `Dynamics::to_vec` order   |
//! | quarters  | grid day, year-month                                           |
//! | marks     | year-month, as-of day, filed day, price, prev, phi, bias, noise |
//! | sector    | year-month, available day, value, variance                     |
//! | prints    | day, price, variance                                           |
//! | desk      | arrival, decay, gamma, informed, inventory                     |
//! | market    | desk (5), daily var, mark gap, mark sd, print sd, subsidy, subsidy above |

use std::slice;

use crate::filter::{self, Dynamics, MarkIn, PrintIn, Run, SectorIn, DYNAMICS_LEN};
use crate::liquidity::Market;
use crate::quote::{self, Desk};

/// Allocates `len` 64-bit floats and returns the address.
#[no_mangle]
pub extern "C" fn px_alloc(len: usize) -> *mut f64 {
    let mut v = Vec::<f64>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Releases a buffer from `px_alloc`. `len` must be the length it was
/// allocated with.
///
/// # Safety
/// `ptr` must come from `px_alloc(len)` and not have been freed.
#[no_mangle]
pub unsafe extern "C" fn px_free(ptr: *mut f64, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

unsafe fn floats<'a>(ptr: *const f64, len: usize) -> &'a [f64] {
    if len == 0 || ptr.is_null() {
        &[]
    } else {
        slice::from_raw_parts(ptr, len)
    }
}

fn desk_from(v: &[f64]) -> Desk {
    Desk { arrival: v[0], decay: v[1], gamma: v[2], informed: v[3], inventory: v[4] }
}

const MARKET_LEN: usize = 11;

fn market_from(v: &[f64]) -> Market {
    Market {
        desk: desk_from(v),
        daily_var: v[5],
        mark_gap: v[6],
        mark_sd: v[7],
        print_sd: v[8],
        subsidy: v[9],
        subsidy_above: v[10],
    }
}

/// Floats per mark as they cross from the browser: year-month, reporting
/// day, filing day, price, the lender's previous price, what that previous
/// mark implied about fair value, then the lender's smoothing, bias, distance
/// and carry, and its number.
pub const MARK_LEN: usize = 11;

fn mark_from(m: &[f64]) -> MarkIn {
    MarkIn {
        ym: m[0] as i32,
        asof_idx: m[1] as usize,
        filed_idx: m[2] as usize,
        price: m[3],
        prev: m[4],
        anchor: m[5],
        phi: m[6],
        bias: m[7],
        noise: m[8],
        carry: m[9],
        lender: m[10] as u32,
    }
}

/// Filters and smooths one loan.
///
/// Writes four series of `end - start + 1` values to `out`, one after the
/// other: real-time mean, real-time standard deviation, hindsight mean,
/// hindsight standard deviation. Writes six values per mark to `forecasts`,
/// in the order the marks were given: public mean and sd, syndicate mean and
/// sd, how many other marks of that date were already in, and 1 if the mark
/// arrived too late to use. Returns 0 on success.
///
/// # Safety
/// Every pointer must address at least as many floats as its length says.
#[no_mangle]
pub unsafe extern "C" fn px_curve(
    dynamics: *const f64,
    shocks: *const f64,
    n_days: usize,
    quarters: *const f64,
    n_quarters: usize,
    marks: *const f64,
    n_marks: usize,
    sector: *const f64,
    n_sector: usize,
    prints: *const f64,
    n_prints: usize,
    start: usize,
    end: usize,
    out: *mut f64,
    forecasts: *mut f64,
) -> i32 {
    if end < start || end >= n_days {
        return 1;
    }
    let d = Dynamics::from_slice(floats(dynamics, DYNAMICS_LEN));
    let shocks = floats(shocks, n_days);
    let quarters: Vec<(usize, i32)> = floats(quarters, 2 * n_quarters).chunks_exact(2).map(|q| (q[0] as usize, q[1] as i32)).collect();
    let marks: Vec<MarkIn> = floats(marks, MARK_LEN * n_marks).chunks_exact(MARK_LEN).map(mark_from).collect();
    let sector: Vec<SectorIn> = floats(sector, 4 * n_sector)
        .chunks_exact(4)
        .map(|s| SectorIn { ym: s[0] as i32, avail_idx: s[1] as usize, value: s[2], var: s[3] })
        .collect();
    let prints: Vec<PrintIn> =
        floats(prints, 3 * n_prints).chunks_exact(3).map(|p| PrintIn { idx: p[0] as usize, price: p[1], var: p[2] }).collect();

    let mut fc = Vec::with_capacity(marks.len());
    let mut path = Vec::with_capacity(end - start + 1);
    filter::run(
        &Run { shocks, start, end, quarters: &quarters, marks: &marks, sector: &sector, prints: &prints },
        &d,
        &mut fc,
        Some(&mut path),
    );
    let smooth = filter::smooth(shocks, start, end, &marks, &prints, &d);

    let n = end - start + 1;
    let out = slice::from_raw_parts_mut(out, 4 * n);
    for k in 0..n {
        out[k] = path[k].mean;
        out[n + k] = path[k].sd;
        out[2 * n + k] = smooth[k].mean;
        out[3 * n + k] = smooth[k].sd;
    }
    if !forecasts.is_null() && n_marks > 0 {
        let rows = slice::from_raw_parts_mut(forecasts, 6 * n_marks);
        rows.fill(f64::NAN);
        for f in &fc {
            let r = &mut rows[6 * f.mark..6 * f.mark + 6];
            r[0] = f.public_mean;
            r[1] = f.public_sd;
            r[2] = f.peer_mean;
            r[3] = f.peer_sd;
            r[4] = f.seen as f64;
            r[5] = if f.late { 1.0 } else { 0.0 };
        }
    }
    0
}

/// Daily variance of value at price `v`, including the expected contribution
/// of jumps over a quarter of `days_per_quarter` days. Prices are fractions
/// of par, as everywhere in the filter.
///
/// # Safety
/// `dynamics` must address seventeen floats.
#[no_mangle]
pub unsafe extern "C" fn px_daily_var(dynamics: *const f64, v: f64, days_per_quarter: f64) -> f64 {
    let d = Dynamics::from_slice(floats(dynamics, DYNAMICS_LEN));
    d.daily_var(v) + d.jump_prob * d.jump_sd * d.jump_sd / days_per_quarter.max(1.0)
}

/// Quotes a loan. Writes ten values to `out`: mid, bid, ask, half-spread,
/// processing, inventory risk, adverse selection, skew, bid rate, ask rate.
/// Units are price points.
///
/// # Safety
/// `desk` must address five floats and `out` ten.
#[no_mangle]
pub unsafe extern "C" fn px_quote(desk: *const f64, mid: f64, sd_now: f64, daily_var: f64, out: *mut f64) {
    let q = quote::quote(&desk_from(floats(desk, 5)), mid, sd_now, daily_var);
    let out = slice::from_raw_parts_mut(out, 10);
    out.copy_from_slice(&[
        q.mid,
        q.bid,
        q.ask,
        q.half_spread,
        q.processing,
        q.inventory_risk,
        q.adverse_selection,
        q.skew,
        q.bid_rate,
        q.ask_rate,
    ]);
}

/// The adverse selection half-spread per unit of standard error.
#[no_mangle]
pub extern "C" fn px_adverse_width(informed: f64) -> f64 {
    quote::adverse_selection_width(informed)
}

/// Rest points of the skeleton. Writes up to `max` rows of four values:
/// standard error, full spread, trades per day, 1 if stable. Returns the
/// number written.
///
/// # Safety
/// `market` must address eleven floats and `out` four times `max`.
#[no_mangle]
pub unsafe extern "C" fn px_rest(market: *const f64, out: *mut f64, max: usize) -> usize {
    let m = market_from(floats(market, MARKET_LEN));
    let rest = m.rest_points(1e-6, 1e4);
    let out = slice::from_raw_parts_mut(out, 4 * max);
    let n = rest.len().min(max);
    for (i, r) in rest.iter().take(n).enumerate() {
        out[4 * i] = r.variance.sqrt();
        out[4 * i + 1] = 2.0 * r.half_spread;
        out[4 * i + 2] = r.trade_rate;
        out[4 * i + 3] = if r.stable { 1.0 } else { 0.0 };
    }
    n
}

/// For each standard error in `sd`, the arrival rate at which it is a rest
/// point of the skeleton, with the market's own arrival rate ignored.
///
/// # Safety
/// `market` must address eleven floats; `sd` and `out` must address `n`.
#[no_mangle]
pub unsafe extern "C" fn px_manifold(market: *const f64, sd: *const f64, n: usize, out: *mut f64) {
    let m = market_from(floats(market, MARKET_LEN));
    let sd = floats(sd, n);
    let out = slice::from_raw_parts_mut(out, n);
    for i in 0..n {
        out[i] = m.arrival_at_rest(sd[i] * sd[i]);
    }
}

/// One simulated history. Writes `days` rows of six values: standard error,
/// full spread, trade rate, trades, subsidised trades, 1 if marks landed.
///
/// # Safety
/// `market` must address eleven floats and `out` six times `days`.
#[no_mangle]
pub unsafe extern "C" fn px_simulate(market: *const f64, sd0: f64, days: usize, seed: f64, out: *mut f64) {
    let m = market_from(floats(market, MARKET_LEN));
    let steps = m.simulate(sd0 * sd0, days, seed as u64);
    let out = slice::from_raw_parts_mut(out, 6 * days);
    for (i, s) in steps.iter().enumerate() {
        out[6 * i] = s.sd;
        out[6 * i + 1] = s.spread;
        out[6 * i + 2] = s.rate;
        out[6 * i + 3] = s.trades as f64;
        out[6 * i + 4] = s.subsidised as f64;
        out[6 * i + 5] = if s.marked { 1.0 } else { 0.0 };
    }
}

/// Number of histogram bins `px_census` writes after its six summary values.
pub const CENSUS_BINS: usize = 40;

/// Long-run behaviour from one simulated history of `years` years. Writes
/// six summary values (share of days tradeable, mean spread, median spread,
/// trades per year, subsidy per year, largest standard error) followed by a
/// forty-bin histogram of the standard error over zero to that largest value.
///
/// # Safety
/// `market` must address eleven floats and `out` forty-six.
#[no_mangle]
pub unsafe extern "C" fn px_census(market: *const f64, years: usize, seed: f64, out: *mut f64) {
    let m = market_from(floats(market, MARKET_LEN));
    let c = m.census(years.max(1), seed as u64);
    let out = slice::from_raw_parts_mut(out, 6 + CENSUS_BINS);
    out[0] = c.tradeable;
    out[1] = c.mean_spread;
    out[2] = c.median_spread;
    out[3] = c.trades_per_year;
    out[4] = c.subsidy_per_year;
    out[5] = c.sd_max;
    out[6..].copy_from_slice(&c.sd_hist);
}
