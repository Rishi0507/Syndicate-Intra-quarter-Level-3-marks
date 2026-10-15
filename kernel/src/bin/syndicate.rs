use std::path::Path;
use std::process::exit;

use syndicate_kernel::calib::{self, Fit, PARTS};
use syndicate_kernel::data::{Obs, Panel};
use syndicate_kernel::engine::{self, Record};
use syndicate_kernel::eval;
use syndicate_kernel::export;
use syndicate_kernel::liquidity::Market;
use syndicate_kernel::quote::Desk;
use syndicate_kernel::truth;

const FACTOR: &str = "baa_spread";
const MIN_TRAIN: usize = 6;

fn die(msg: impl AsRef<str>) -> ! {
    eprintln!("syndicate: {}", msg.as_ref());
    exit(1)
}

fn load(dir: &str) -> (Panel, Vec<f64>) {
    let panel = Panel::load(Path::new(dir)).unwrap_or_else(|e| die(e));
    let level = panel.factor(FACTOR).unwrap_or_else(|| die(format!("factors.tsv has no {FACTOR} column"))).to_vec();
    eprintln!(
        "{} panel: {} loans, {} family marks, {} families, {} grid days ({} to {})",
        panel.source,
        panel.instruments.len(),
        panel.obs.len(),
        panel.families.len(),
        panel.days.len(),
        panel.days[0],
        panel.days[panel.days.len() - 1]
    );
    (panel, level)
}

fn show_fit(fit: &Fit) {
    let d = &fit.dynamics;
    let q = fit.diag.days_per_quarter.sqrt();
    println!("  public beta at par        {:>8.4}   price per 1.00 of spread", d.beta0);
    println!("  public beta per stress    {:>8.4}", d.beta1);
    println!("  share of each kept        {:>8.2}   and {:.2}, after leaving out each quarter in turn", fit.diag.beta_kept[0], fit.diag.beta_kept[1]);
    println!("  public R2                 {:>8.4}", fit.diag.r2_public);
    println!("  smoothing under stress    {:>8.3}   share of last mark carried over, from {} shared moves", fit.phi_bar, fit.diag.n_lead_lag);
    println!(
        "  own mark already below 97 {:>8.3}   and {:.3} where only the other lenders had it there",
        fit.diag.phi_stressed, fit.diag.phi_entering
    );
    println!(
        "  disagreement carried      {:>8.3}   a quarter on; gap on its own past {:.3} at one quarter, {:.3} at two, {} pairs",
        fit.carry, fit.diag.carry_lags[0], fit.diag.carry_lags[1], fit.diag.n_carry
    );
    println!("  settling level            {:>8.2}", d.par * 100.0);
    println!(
        "  pull to it, per quarter   {:>8.3}   from below, {:.3} from above",
        1.0 - (1.0 - d.pull).powf(fit.diag.days_per_quarter),
        1.0 - (1.0 - d.pull_up).powf(fit.diag.days_per_quarter)
    );
    println!("  drift, per quarter        {:>8.3}   points", d.drift0 * q * q * 100.0);
    println!("  stressed drift            {:>8.3}   points, {:+.3} per point of stress", d.drift_s * q * q * 100.0, d.drift1 * q * q);
    println!("  jumps                     {:>8.3}   per loan-quarter, sd {:.2} points", d.jump_prob, d.jump_sd * 100.0);
    println!("  loan vol at par           {:>8.3}   points per quarter", d.idio0 * q * 100.0);
    println!("  loan vol per stress       {:>8.3}", d.idio1 * q * 100.0);
    println!("  sector vol                {:>8.3}   points per quarter", fit.diag.sector_q_sd * 100.0);
    println!("  lender distance, median   {:>8.3}   points", fit.diag.noise_median * 100.0);
    println!("  noise growth with stress  {:>8.2}", d.noise1);
    println!("  new-loan prior            {:>8.2} +/- {:.2}", d.prior_mean * 100.0, d.prior_sd * 100.0);
    println!("  changes / quarters used   {:>8} / {}", fit.diag.n_changes, fit.diag.n_quarters);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        die("usage: syndicate <fit|backtest|truth> <build-dir> [--out file] [--records file] [--only yyyy-mm]\n       syndicate export <build-dir> <out-dir> [--named]\n       syndicate rest <eight market parameters>");
    }
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).cloned();

    match args[1].as_str() {
        "fit" => {
            let (panel, level) = load(&args[2]);
            let fit = calib::fit(&panel, &level, &|_: &Obs| true).unwrap_or_else(|e| die(e));
            println!("\nFull-sample fit");
            show_fit(&fit);
            let best = fit.diag.phi_profile.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
            let profile: Vec<String> = fit
                .diag
                .phi_profile
                .iter()
                .step_by(5)
                .map(|(phi, sse)| format!("{:.1}:{:+.2}%", phi, (sse / best - 1.0) * 100.0))
                .collect();
            println!("  smoothing profile         {}", profile.join("  "));
            let lag = syndicate_kernel::insight::catch_up(&panel);
            println!(
                "  after peers move alone    {:>8.3}   of their move reaches the lender next quarter; peers add {:.3} more, from {} cases",
                lag.follow, lag.continuation, lag.n
            );
            println!("\n  {:<28} {:>6} {:>8} {:>8} {:>7} {:>7}", "family", "phi", "bias", "noise", "moves", "overlap");
            for (name, f) in panel.families.iter().zip(&fit.families) {
                println!(
                    "  {:<28} {:>6.2} {:>8.3} {:>8.3} {:>7} {:>7}",
                    name.chars().take(28).collect::<String>(),
                    f.phi,
                    f.bias * 100.0,
                    f.noise * 100.0,
                    f.n_moves,
                    f.n_overlap
                );
            }
        }
        "backtest" => {
            let (panel, level) = load(&args[2]);
            let only = args.iter().position(|a| a == "--only").and_then(|i| args.get(i + 1)).map(|q| {
                let (y, m) = q.split_once('-').unwrap_or_else(|| die("--only takes a quarter-end month, yyyy-mm"));
                let (y, m): (i32, i32) = (y.parse().unwrap_or_else(|_| die("bad year")), m.parse().unwrap_or_else(|_| die("bad month")));
                y * 12 + m - 1
            });
            let records: Vec<Record> = engine::backtest_quarters(&panel, &level, MIN_TRAIN, only, |ym, n, fit| {
                eprintln!(
                    "  {}-{:02}  {:>6} forecasts   beta {:.4}  phi {:.2}/{:.2}  carry {:.2}  pull {:.2}  sector vol {:.3}",
                    ym / 12,
                    ym % 12 + 1,
                    n,
                    fit.dynamics.beta0,
                    fit.diag.phi_entering,
                    fit.diag.phi_stressed,
                    fit.carry,
                    1.0 - (1.0 - fit.dynamics.pull).powf(fit.diag.days_per_quarter),
                    fit.diag.sector_q_sd * 100.0
                );
            })
            .unwrap_or_else(|e| die(e));

            let all: Vec<&Record> = records.iter().collect();
            eval::print_ladder("All forecasts, price points", &all);
            let followers: Vec<&Record> = records.iter().filter(|r| r.seen > 0).collect();
            eval::print_ladder("Another lender had already filed", &followers);
            eval::print_rules("Against simple rules, all forecasts", &all);
            eval::print_rules("Against simple rules, another lender had already filed", &followers);
            for flag in [true, false] {
                let sel: Vec<&Record> = followers.iter().copied().filter(|r| r.lockstep == flag).collect();
                eval::print_rules(
                    if flag { "Peer filed, lender's marks of the loan have matched another's exactly" } else { "Peer filed, marks never matched exactly" },
                    &sel,
                );
            }
            eval::print_ablations(&all, &followers);
            for regime in eval::REGIMES {
                let sel: Vec<&Record> = records.iter().filter(|r| eval::Regime::of(r.prev) == regime).collect();
                eval::print_ladder(&format!("Regime: {}", regime.label()), &sel);
            }
            println!("\n  Interval coverage, walk-forward");
            println!("  {:<12} {:>7}  {:<32} {:<32} {:>9} {:>9}", "regime", "n", "gaussian 50/80/90/95", "conformal 50/80/90/95", "width90", "naive90");
            for (regime, c) in eval::coverage(&records) {
                let fmt = |v: &[f64; 4]| v.iter().map(|x| format!("{:.3}", x)).collect::<Vec<_>>().join(" / ");
                println!(
                    "  {:<12} {:>7}  {:<32} {:<32} {:>9.3} {:>9.3}",
                    regime.label(),
                    c.n,
                    fmt(&c.gaussian),
                    fmt(&c.conformal),
                    c.width90 * 100.0,
                    c.naive_width90 * 100.0
                );
            }
            let books = eval::books(&records);
            if !books.is_empty() {
                let sst: f64 = books.iter().map(|b| b.actual * b.actual).sum();
                let sse: f64 = books.iter().map(|b| (b.actual - b.model).powi(2)).sum();
                let ssr: f64 = books.iter().map(|b| (b.actual - b.rule).powi(2)).sum();
                let moved: Vec<&eval::Book> = books.iter().filter(|b| b.actual.abs() >= 0.005).collect();
                let fell = moved.iter().filter(|b| b.actual < 0.0).count();
                println!(
                    "\n  Lender books, {} or more loans: {} books, {:.1}% of the squared change anticipated (rule {:.1}%), direction right in {} of {} that moved half a point (rule {}, always the commoner direction {})",
                    eval::MIN_BOOK,
                    books.len(),
                    (1.0 - sse / sst) * 100.0,
                    (1.0 - ssr / sst) * 100.0,
                    moved.iter().filter(|b| b.actual * b.model > 0.0).count(),
                    moved.len(),
                    moved.iter().filter(|b| b.actual * b.rule > 0.0).count(),
                    fell.max(moved.len() - fell)
                );
            }
            if let Some(path) = out {
                std::fs::write(&path, eval::report(&records).to_string()).unwrap_or_else(|e| die(format!("{path}: {e}")));
                eprintln!("wrote {path}");
            }
            if let Some(path) = args.iter().position(|a| a == "--records").and_then(|i| args.get(i + 1)) {
                let mut rows = String::from("inst\tfamily\tym\tprice\tprev\tpublic_raw\tpublic\tmodel\tsd\tsector\tcopy\trule\tcombined\tlockstep\tseen\tprincipal\tfiled");
                for (_, id, _) in &PARTS {
                    rows.push_str(&format!("\twithout_{id}"));
                }
                rows.push('\n');
                for r in &records {
                    rows.push_str(&format!(
                        "{}\t{}\t{}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{}\t{}\t{:.0}\t{}",
                        r.inst,
                        r.family,
                        r.ym,
                        r.price,
                        r.prev,
                        r.public_raw.0,
                        r.public.0,
                        r.peers.0,
                        r.peers.1,
                        r.sector.0,
                        r.copy,
                        r.rule,
                        r.combined,
                        u8::from(r.lockstep),
                        r.seen,
                        r.principal,
                        r.filed
                    ));
                    for x in &r.parts {
                        rows.push_str(&format!("\t{x:.6}"));
                    }
                    rows.push('\n');
                }
                std::fs::write(path, rows).unwrap_or_else(|e| die(format!("{path}: {e}")));
                eprintln!("wrote {path}");
            }
        }
        "export" => {
            let Some(dest) = args.get(3) else { die("export needs an output directory") };
            let named = args.iter().any(|a| a == "--named");
            let summary = export::export(Path::new(&args[2]), Path::new(dest), FACTOR, MIN_TRAIN, named, &mut |msg| eprintln!("  {msg}"))
                .unwrap_or_else(|e| die(e));
            eprintln!("{} loans, {} marks, {} KB in {dest}", summary.loans, summary.marks, summary.bytes / 1024);
        }
        "rest" => {
            let v: Vec<f64> = args[2..].iter().filter_map(|a| a.parse().ok()).collect();
            if v.len() != 8 {
                die("rest needs eight numbers");
            }
            let m = Market {
                desk: Desk { arrival: v[0], decay: v[1], gamma: v[2], informed: v[3], inventory: 0.0 },
                daily_var: v[4],
                mark_gap: v[5],
                mark_sd: v[6],
                print_sd: v[7],
                subsidy: 0.0,
                subsidy_above: 0.0,
            };
            for r in m.rest_points(1e-6, 1e4) {
                println!(
                    "  sd {:>7.3}  spread {:>7.3}  trades/day {:>8.4}  {}",
                    r.variance.sqrt(),
                    2.0 * r.half_spread,
                    r.trade_rate,
                    if r.stable { "stable" } else { "tipping point" }
                );
            }
            for (label, gap) in [("as given", m.mark_gap), ("monthly", 21.0), ("daily", 1.0)] {
                let c = Market { mark_gap: gap, ..m }.census(200, 1);
                println!(
                    "  marks {:<9} tradeable {:>5.1}%  spread mean {:.2} median {:.2}  trades/yr {:.1}",
                    label,
                    c.tradeable * 100.0,
                    c.mean_spread,
                    c.median_spread,
                    c.trades_per_year
                );
            }
        }
        "truth" => {
            let (panel, level) = load(&args[2]);
            if panel.source != "synthetic" {
                die("truth is only known for the synthetic universe");
            }
            let fit = calib::fit(&panel, &level, &|_: &Obs| true).unwrap_or_else(|e| die(e));
            let records = engine::backtest(&panel, &level, MIN_TRAIN, |_, _, _| {}).unwrap_or_else(|e| die(e));
            let mult = eval::bands(&records);
            let rep = truth::score(&panel, &level, &fit, &mult, Path::new(&args[2])).unwrap_or_else(|e| die(e));
            println!("\n  Daily estimate against true value at month ends, price points");
            println!(
                "  {:<12} {:>6}  {:>13}  {:>13}   {:<29} {}",
                "since filing", "n", "MAE mod/stale", "RMSE mod/stale", "gaussian 50/80/90/95", "calibrated 50/80/90/95"
            );
            let join = |c: &[f64; 4]| c.iter().map(|x| format!("{x:.3}")).collect::<Vec<_>>().join(" / ");
            let row = |label: &str, s: &truth::TruthScore| {
                println!(
                    "  {:<12} {:>6}  {:>6.3}/{:<6.3}  {:>6.3}/{:<6.3}   {:<29} {}",
                    label,
                    s.n,
                    s.mae_model * 100.0,
                    s.mae_stale * 100.0,
                    s.rmse_model * 100.0,
                    s.rmse_stale * 100.0,
                    join(&s.cover_gaussian),
                    join(&s.cover)
                );
            };
            for (label, s) in &rep.by_staleness {
                row(label, s);
            }
            row("all", &rep.all);
        }
        other => die(format!("unknown command {other:?}")),
    }
}
