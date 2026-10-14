use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::calib::{self, Fit};
use crate::data::{Obs, Panel, Table};
use crate::engine::{self, Engine, BATCH_DAYS, MIN_PEERS, MODEL};
use crate::eval::{self, LEVELS};
use crate::insight;
use crate::json::Json;
use crate::truth;

pub const PATH_STEP: usize = 5;
pub const AFTERLIFE: usize = 126;

pub const PRICE_LO: f64 = 0.30;
pub const PRICE_HI: f64 = 1.05;
pub const SD_HI_POINTS: f64 = 6.0;

fn price_code(v: f64) -> u8 {
    (((v - PRICE_LO) / (PRICE_HI - PRICE_LO)).clamp(0.0, 1.0) * 255.0).round() as u8
}

fn sd_code(sd: f64) -> u8 {
    ((sd * 100.0 / SD_HI_POINTS).clamp(0.0, 1.0).sqrt() * 255.0).round() as u8
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    fs::write(dir.join(name), bytes).map_err(|e| format!("{name}: {e}"))
}

fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn fit_json(fit: &Fit) -> Json {
    let d = &fit.diag;
    Json::obj()
        .with("dynamics", Json::Arr(fit.dynamics.to_vec().into_iter().map(Json::from).collect()))
        .with("phi_bar", Json::round(fit.phi_bar, 4))
        .with("phi_stressed", Json::round(d.phi_stressed, 4))
        .with("phi_entering", Json::round(d.phi_entering, 4))
        .with("carry", fit.carry)
        .with("n_lead_lag", d.n_lead_lag)
        .with("n_carry", d.n_carry)
        .with("carry_lags", Json::nums(&d.carry_lags, 4))
        .with("beta_kept", Json::nums(&d.beta_kept, 4))
        .with("days_per_quarter", d.days_per_quarter)
        .with("r2_public", Json::round(d.r2_public, 4))
        .with("sector_q_sd", Json::round(d.sector_q_sd * 100.0, 4))
        .with("noise_median", Json::round(d.noise_median * 100.0, 4))
        .with("n_changes", d.n_changes)
        .with("n_quarters", d.n_quarters)
        .with(
            "phi_profile",
            Json::Arr(d.phi_profile.iter().map(|(p, s)| Json::Arr(vec![Json::round(*p, 2), Json::from(*s)])).collect()),
        )
        .with(
            "idio_bins",
            Json::Arr(
                d.idio_bins
                    .iter()
                    .map(|(st, sd, n)| Json::Arr(vec![Json::round(st * 100.0, 3), Json::round(sd * 100.0, 4), Json::from(*n)]))
                    .collect(),
            ),
        )
}

fn skill(rows: &[(f64, f64, f64)]) -> (f64, f64, f64) {
    let n = rows.len().max(1) as f64;
    let se0: f64 = rows.iter().map(|(p, prev, _)| (p - prev).powi(2)).sum();
    let se: f64 = rows.iter().map(|(p, _, m)| (p - m).powi(2)).sum();
    ((se0 / n).sqrt(), (se / n).sqrt(), if se0 > 0.0 { 1.0 - se / se0 } else { 0.0 })
}

fn vintage_json(dir: &Path) -> Json {
    let Ok(entries) = fs::read_dir(dir) else { return Json::Null };
    let mut names: Vec<String> = entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        let Some((y, m)) = name.split_once('-') else { continue };
        let (Ok(y), Ok(m)) = (y.parse::<i32>(), m.parse::<i32>()) else { continue };
        let Ok(t) = Table::read(&dir.join(&name).join("records.tsv")) else { continue };
        let (Ok(cp), Ok(cv), Ok(cm), Ok(cs)) = (t.col("price"), t.col("prev"), t.col("model"), t.col("seen")) else { continue };
        let rows: Vec<(f64, f64, f64, bool)> = (0..t.len()).map(|r| (t.num(r, cp), t.num(r, cv), t.num(r, cm), t.int(r, cs) > 0)).collect();
        let all: Vec<(f64, f64, f64)> = rows.iter().map(|r| (r.0, r.1, r.2)).collect();
        let peers: Vec<(f64, f64, f64)> = rows.iter().filter(|r| r.3).map(|r| (r.0, r.1, r.2)).collect();
        let (stale, model, sk) = skill(&all);
        let (peer_stale, peer_model, peer_sk) = skill(&peers);
        out.push(
            Json::obj()
                .with("ym", y * 12 + m - 1)
                .with("n", all.len())
                .with("followers", peers.len())
                .with("stale", Json::round(stale * 100.0, 4))
                .with("model", Json::round(model * 100.0, 4))
                .with("skill", Json::round(sk, 4))
                .with("followers_stale", Json::round(peer_stale * 100.0, 4))
                .with("followers_model", Json::round(peer_model * 100.0, 4))
                .with("followers_skill", Json::round(peer_sk, 4)),
        );
    }
    if out.is_empty() {
        Json::Null
    } else {
        Json::Arr(out)
    }
}

fn audit_json(build: &Path) -> Json {
    let mut out = Json::obj();
    if let Ok(t) = Table::read(&build.join("audit.tsv")) {
        if let (Ok(cm), Ok(cv)) = (t.col("measure"), t.col("value")) {
            let mut facts = Json::obj();
            for r in 0..t.len() {
                let v = t.num(r, cv);
                facts = facts.with(t.str(r, cm), if v.is_finite() { Json::from(v) } else { Json::from(t.str(r, cv)) });
            }
            out = out.with("ingest", facts);
        }
    }
    let vintage = build.parent().map(|p| p.join("vintage")).map(|d| vintage_json(&d)).unwrap_or(Json::Null);
    out.with("vintage", vintage)
}

pub fn lender_codes(live: &[usize], marks: &[usize], names: &[String]) -> Vec<String> {
    let mut order: Vec<usize> = (0..names.len()).collect();
    order.sort_by(|&a, &b| live[b].cmp(&live[a]).then(marks[b].cmp(&marks[a])).then(names[a].cmp(&names[b])));
    let width = names.len().to_string().len().max(2);
    let mut codes = vec![String::new(); names.len()];
    for (rank, &i) in order.iter().enumerate() {
        codes[i] = format!("Lender {:0width$}", rank + 1, width = width);
    }
    codes
}

pub struct Summary {
    pub loans: usize,
    pub marks: usize,
    pub bytes: usize,
}

pub fn export(build: &Path, out: &Path, factor: &str, min_train: usize, named: bool, log: &mut dyn FnMut(&str)) -> Result<Summary, String> {
    let panel = Panel::load(build)?;
    let level = panel.factor(factor).ok_or_else(|| format!("factors.tsv has no {factor} column"))?.to_vec();
    if panel.days.len() > u16::MAX as usize {
        return Err("grid is too long for 16-bit day indices".into());
    }
    if panel.families.len() > u16::MAX as usize {
        return Err("too many families for 16-bit indices".into());
    }
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;

    log("fitting");
    let fit = calib::fit(&panel, &level, &|_: &Obs| true)?;
    log("backtesting");
    let records = engine::backtest(&panel, &level, min_train, |_, _, _| {})?;
    let bands = eval::bands(&records);

    log("filtering every loan");
    let eng = Engine::new(&panel, &level);
    let grid_end = panel.days.len() - 1;
    let insts: Vec<usize> = (0..panel.instruments.len()).collect();
    let (runs, book) = eng.run_with_book(&fit, MODEL, &insts, &|_: &Obs| true, grid_end, true);

    let truth = if panel.source == "synthetic" && build.join("truth_prices.tsv").exists() {
        log("scoring against the answer key");
        Some(truth::score(&panel, &level, &fit, &bands, build)?)
    } else {
        None
    };

    let mut lenders_of: Vec<Vec<usize>> = vec![Vec::new(); panel.borrowers.len()];
    for o in &panel.obs {
        let b = panel.instruments[o.inst].borrower;
        if !lenders_of[b].contains(&o.family) {
            lenders_of[b].push(o.family);
        }
    }
    let shared = lenders_of.iter().filter(|l| l.len() >= 2).count();

    let latest_ym = panel.obs.iter().map(|o| o.ym).max().unwrap_or(0);
    let run_of: HashMap<usize, usize> = runs.iter().enumerate().map(|(i, r)| (r.inst, i)).collect();

    let n = panel.instruments.len();
    let (mut name, mut sector, mut class) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    let (mut start, mut last, mut fams, mut size) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut now_mean, mut now_sd, mut live) = (Vec::new(), Vec::new(), Vec::new());
    let mut mark_start: Vec<Json> = vec![Json::from(0usize)];
    let mut path_start: Vec<Json> = vec![Json::from(0usize)];
    let mut marks_i: Vec<u16> = Vec::new();
    let mut marks_f: Vec<f32> = Vec::new();
    let mut path: Vec<u8> = Vec::new();
    let mut n_marks = 0usize;

    let mut arrivals = vec![0u32; panel.days.len()];
    let mut sd_sum = vec![0.0f64; panel.days.len()];
    let mut sd_n = vec![0u32; panel.days.len()];
    let mut family_live = vec![0usize; panel.families.len()];
    let mut family_marks = vec![0usize; panel.families.len()];

    for inst in 0..n {
        let ins = &panel.instruments[inst];
        let b = &panel.borrowers[ins.borrower];
        name.push(if ins.piece > 0 { Json::from(format!("{} (loan {})", b.name, ins.piece + 1)) } else { Json::from(b.name.as_str()) });
        sector.push(Json::from(b.sector));
        class.push(Json::from(ins.class as usize));

        let Some(&ri) = run_of.get(&inst) else {
            start.push(Json::from(0usize));
            last.push(Json::from(0usize));
            fams.push(Json::from(0usize));
            size.push(Json::Null);
            now_mean.push(Json::Null);
            now_sd.push(Json::Null);
            live.push(Json::from(0usize));
            mark_start.push(Json::from(n_marks));
            path_start.push(Json::from(path.len() / 2));
            continue;
        };
        let r = &runs[ri];
        let obs = panel.obs_of(inst);
        let last_asof = obs.iter().map(|o| o.asof_idx).max().unwrap_or(r.start);
        let last_ym = obs.iter().map(|o| o.ym).max().unwrap_or(0);
        let life_end = (last_asof + AFTERLIFE).min(grid_end);
        let is_live = last_ym >= latest_ym - 3;

        let mut distinct: Vec<usize> = obs.iter().map(|o| o.family).collect();
        distinct.sort_unstable();
        distinct.dedup();
        let mut by_date: HashMap<i32, f64> = HashMap::new();
        for o in obs.iter() {
            *by_date.entry(o.ym).or_default() += o.principal;
        }
        let latest_size = by_date.values().copied().fold(0.0, f64::max);

        start.push(Json::from(r.start));
        last.push(Json::from(life_end));
        fams.push(Json::from(distinct.len()));
        size.push(Json::round(latest_size / 1e6, 2));
        let at_end = r.path[(life_end - r.start).min(r.path.len() - 1)];
        now_mean.push(Json::round(at_end.mean * 100.0, 3));
        now_sd.push(Json::round(at_end.sd * 100.0, 3));
        live.push(Json::from(is_live as usize));
        if is_live {
            for f in &distinct {
                family_live[*f] += 1;
            }
        }

        for (k, m) in r.marks.iter().enumerate() {
            let arrival = m.filed_idx.max(m.asof_idx + 1).min(grid_end);
            marks_i.extend([r.families[k] as u16, m.asof_idx as u16, arrival as u16]);
            let week = arrival / BATCH_DAYS;
            let said = r.said.iter().find(|s| s.0 == m.ym && s.1 == week);
            let first_of_ym = !r.marks.iter().enumerate().any(|(j, e)| {
                j != k && e.ym == m.ym && (e.filed_idx.max(e.asof_idx + 1), j) < (m.filed_idx.max(m.asof_idx + 1), k)
            });
            let (sw, swv) = match said {
                Some(&(_, _, w, wv)) if first_of_ym => (w, wv),
                _ => (0.0, 0.0),
            };
            marks_f.extend([m.price as f32, m.prev as f32, (obs[k].principal / 1e6) as f32, sw as f32, swv as f32]);
            arrivals[arrival] += 1;
            family_marks[r.families[k]] += 1;
            n_marks += 1;
        }
        mark_start.push(Json::from(n_marks));

        let mut t = r.start;
        while t <= life_end {
            let pt = r.path[t - r.start];
            path.extend([price_code(pt.mean), sd_code(pt.sd)]);
            t += PATH_STEP;
        }
        path_start.push(Json::from(path.len() / 2));
        for t in r.start..=life_end {
            sd_sum[t] += r.path[t - r.start].sd;
            sd_n[t] += 1;
        }
    }

    let loans = Json::obj()
        .with("name", name)
        .with("sector", sector)
        .with("class", class)
        .with("start", start)
        .with("last", last)
        .with("families", fams)
        .with("size", size)
        .with("now_mean", now_mean)
        .with("now_sd", now_sd)
        .with("live", live)
        .with("mark_start", mark_start)
        .with("path_start", path_start);

    let (filled, _) = calib::shocks_from(&level);
    let shown = if named { panel.families.clone() } else { lender_codes(&family_live, &family_marks, &panel.families) };
    let families = shown
        .iter()
        .enumerate()
        .map(|(i, fname)| {
            let f = &fit.families[i];
            Json::obj()
                .with("name", fname.as_str())
                .with("phi", Json::round(f.phi, 4))
                .with("bias", Json::round(f.bias * 100.0, 4))
                .with("noise", Json::round(f.noise * 100.0, 4))
                .with("moves", f.n_moves)
                .with("overlap", f.n_overlap)
                .with("marks", family_marks[i])
                .with("live", family_live[i])
                .with("fit", Json::Arr(vec![Json::from(f.phi), Json::from(f.bias), Json::from(f.noise)]))
        })
        .collect::<Vec<_>>();

    let truth_json = match &truth {
        Some(t) => {
            let row = |label: &str, s: &truth::TruthScore| {
                Json::obj()
                    .with("label", label)
                    .with("n", s.n)
                    .with("mae_model", Json::round(s.mae_model * 100.0, 4))
                    .with("mae_stale", Json::round(s.mae_stale * 100.0, 4))
                    .with("rmse_model", Json::round(s.rmse_model * 100.0, 4))
                    .with("rmse_stale", Json::round(s.rmse_stale * 100.0, 4))
                    .with("cover_gaussian", Json::nums(&s.cover_gaussian, 4))
                    .with("cover", Json::nums(&s.cover, 4))
            };
            let mut rows: Vec<Json> = t.by_staleness.iter().map(|(l, s)| row(l, s)).collect();
            rows.push(row("all", &t.all));
            Json::Arr(rows)
        }
        None => Json::Null,
    };

    let meta = Json::obj()
        .with("source", panel.source.as_str())
        .with("lenders_named", named)
        .with(
            "counts",
            Json::obj()
                .with("loans", n)
                .with("borrowers", panel.borrowers.len())
                .with("marks", n_marks)
                .with("families", panel.families.len())
                .with("days", panel.days.len())
                .with("live", runs.iter().filter(|r| panel.obs_of(r.inst).iter().any(|o| o.ym >= latest_ym - 3)).count())
                .with("shared", shared),
        )
        .with("days", Json::Arr(panel.days.iter().map(|d| Json::from(*d)).collect()))
        .with("quarters", Json::Arr(eng.quarters.iter().map(|&(t, ym)| Json::Arr(vec![Json::from(t), Json::from(ym)])).collect()))
        .with("factor", Json::obj().with("name", factor).with("level", Json::nums(&filled, 4)))
        .with("sectors", Json::Arr(panel.sectors.iter().map(|s| Json::from(s.as_str())).collect()))
        .with("classes", Json::Arr(vec!["senior".into(), "junior".into(), "unclassified".into()]))
        .with("families", families)
        .with("fit", fit_json(&fit))
        .with("levels", Json::nums(&LEVELS, 2))
        .with("multipliers", Json::Arr(bands.mult.iter().map(|m| Json::nums(m, 4)).collect()))
        .with("floors", Json::nums(&bands.floor, 6))
        .with(
            "engine",
            Json::obj()
                .with("batch_days", BATCH_DAYS)
                .with("min_peers", MIN_PEERS)
                .with("path_step", PATH_STEP)
                .with("price_lo", PRICE_LO)
                .with("price_hi", PRICE_HI)
                .with("sd_hi_points", SD_HI_POINTS)
                .with("min_train", min_train),
        )
        .with(
            "batches",
            Json::Arr(
                book.iter()
                    .map(|b| {
                        Json::Arr(vec![
                            Json::from(b.sector),
                            Json::from(b.ym),
                            Json::from(b.week),
                            Json::from(b.weight),
                            Json::from(b.weighted_sum),
                            Json::from(b.n),
                            Json::from(b.inflate),
                        ])
                    })
                    .collect(),
            ),
        )
        .with("arrivals", Json::Arr(arrivals.iter().map(|a| Json::from(*a as usize)).collect()))
        .with(
            "mean_sd",
            Json::Arr(
                sd_sum
                    .iter()
                    .zip(&sd_n)
                    .map(|(s, c)| if *c > 0 { Json::round(s / *c as f64 * 100.0, 4) } else { Json::Null })
                    .collect(),
            ),
        )
        .with("eval", eval::report(&records))
        .with("truth", truth_json)
        .with("insight", insight::report(&panel))
        .with("audit", audit_json(build));

    {
        let mut order: Vec<usize> = (0..runs.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse((runs[i].marks.len(), runs[i].inst)));
        let reference = Json::Arr(
            order
                .iter()
                .take(12)
                .map(|&i| {
                    let r = &runs[i];
                    let end = (panel.obs_of(r.inst).iter().map(|o| o.asof_idx).max().unwrap_or(r.start) + AFTERLIFE).min(grid_end);
                    let slice = &r.path[..=end - r.start];
                    Json::obj()
                        .with("loan", r.inst)
                        .with("start", r.start)
                        .with("mean", Json::Arr(slice.iter().map(|p| Json::from(p.mean)).collect()))
                        .with("sd", Json::Arr(slice.iter().map(|p| Json::from(p.sd)).collect()))
                })
                .collect(),
        );
        fs::write(build.join("reference.json"), reference.to_string()).map_err(|e| format!("reference.json: {e}"))?;
    }

    let files: [(&str, Vec<u8>); 5] = [
        ("meta.json", meta.to_string().into_bytes()),
        ("loans.json", loans.to_string().into_bytes()),
        ("marks_i.bin", u16s(&marks_i)),
        ("marks_f.bin", f32s(&marks_f)),
        ("path.bin", path),
    ];
    let mut bytes = 0;
    for (fname, data) in &files {
        write(out, fname, data)?;
        bytes += data.len();
        log(&format!("wrote {fname} ({} KB)", data.len() / 1024));
    }
    Ok(Summary { loans: n, marks: n_marks, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_span_their_ranges() {
        assert_eq!(price_code(PRICE_LO), 0);
        assert_eq!(price_code(PRICE_HI), 255);
        assert_eq!(price_code(0.0), 0);
        assert_eq!(price_code(2.0), 255);
        assert_eq!(sd_code(0.0), 0);
        assert_eq!(sd_code(SD_HI_POINTS / 100.0), 255);
        assert!(sd_code(0.003) > 50);
    }

    #[test]
    fn lender_codes_rank_by_size_and_hide_names() {
        let names: Vec<String> = ["Gamma", "Alpha", "Beta"].iter().map(|s| s.to_string()).collect();
        let codes = lender_codes(&[5, 9, 5], &[10, 1, 20], &names);
        assert_eq!(codes, vec!["Lender 03", "Lender 01", "Lender 02"]);
        assert!(codes.iter().all(|c| !names.iter().any(|n| c.contains(n.as_str()))));
    }

    #[test]
    fn binary_tables_are_little_endian() {
        assert_eq!(u16s(&[0x0102]), vec![0x02, 0x01]);
        assert_eq!(f32s(&[1.0]), 1.0f32.to_le_bytes().to_vec());
    }
}
