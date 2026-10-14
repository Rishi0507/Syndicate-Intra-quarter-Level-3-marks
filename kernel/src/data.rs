use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct Table {
    cols: HashMap<String, usize>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn read(path: &Path) -> Result<Table, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Table::parse(&text))
    }

    pub fn parse(text: &str) -> Table {
        let mut lines = text.lines();
        let header = lines.next().unwrap_or("");
        let cols = header.split('\t').enumerate().map(|(i, h)| (h.trim().to_string(), i)).collect();
        let rows = lines
            .filter(|l| !l.is_empty())
            .map(|l| l.split('\t').map(|s| s.to_string()).collect())
            .collect();
        Table { cols, rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn col(&self, name: &str) -> Result<usize, String> {
        self.cols.get(name).copied().ok_or_else(|| format!("missing column {name:?}"))
    }

    pub fn has(&self, name: &str) -> bool {
        self.cols.contains_key(name)
    }

    pub fn str(&self, row: usize, col: usize) -> &str {
        self.rows[row].get(col).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn num(&self, row: usize, col: usize) -> f64 {
        self.str(row, col).trim().parse().unwrap_or(f64::NAN)
    }

    pub fn int(&self, row: usize, col: usize) -> i64 {
        self.str(row, col).trim().parse().unwrap_or(0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Senior,
    Junior,
    Unclassified,
}

impl Class {
    pub fn parse(s: &str) -> Class {
        match s {
            "senior" => Class::Senior,
            "junior" => Class::Junior,
            _ => Class::Unclassified,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Class::Senior => "senior",
            Class::Junior => "junior",
            Class::Unclassified => "unclassified",
        }
    }
}

pub struct Holder {
    pub name: String,
    pub family: usize,
}

pub struct Borrower {
    pub name: String,
    pub key: String,
    pub sector: usize,
}

pub struct Instrument {
    pub borrower: usize,
    pub class: Class,
    pub piece: usize,
}

#[derive(Clone, Debug)]
pub struct Obs {
    pub inst: usize,
    pub family: usize,
    pub asof: i32,
    pub ym: i32,
    pub asof_idx: usize,
    pub filed_idx: usize,
    pub price: f64,
    pub principal: f64,
    pub spread_bp: f64,
    pub original: bool,
}

pub struct Panel {
    pub source: String,
    pub days: Vec<i32>,
    pub factor_names: Vec<String>,
    pub factors: Vec<Vec<f64>>,
    pub families: Vec<String>,
    pub sectors: Vec<String>,
    pub holders: Vec<Holder>,
    pub borrowers: Vec<Borrower>,
    pub instruments: Vec<Instrument>,
    pub obs: Vec<Obs>,
    pub inst_obs: Vec<(usize, usize)>,
}

pub fn year_month(d: i32) -> i32 {
    (d / 10000) * 12 + (d / 100 % 100) - 1
}

pub fn is_calendar_quarter(d: i32) -> bool {
    matches!(d / 100 % 100, 3 | 6 | 9 | 12)
}

pub fn serial(d: i32) -> i64 {
    let (y, m, dd) = ((d / 10000) as i64, (d / 100 % 100) as i64, (d % 100) as i64);
    let (y, m) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    365 * y + y / 4 - y / 100 + y / 400 + (153 * (m - 3) + 2) / 5 + dd
}

impl Panel {
    pub fn day_index(&self, date: i32) -> usize {
        match self.days.binary_search(&date) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        }
    }

    pub fn factor(&self, name: &str) -> Option<&[f64]> {
        self.factor_names.iter().position(|n| n == name).map(|k| self.factors[k].as_slice())
    }

    pub fn load(dir: &Path) -> Result<Panel, String> {
        let source = fs::read_to_string(dir.join("source.json"))
            .ok()
            .and_then(|s| {
                let i = s.find("\"kind\"")?;
                let rest = &s[i + 6..];
                let a = rest.find('"')?;
                let b = rest[a + 1..].find('"')?;
                Some(rest[a + 1..a + 1 + b].to_string())
            })
            .unwrap_or_else(|| "unknown".to_string());

        let ft = Table::read(&dir.join("factors.tsv"))?;
        let dcol = ft.col("date")?;
        let mut names: Vec<(String, usize)> =
            ft.cols.iter().filter(|(n, _)| n.as_str() != "date").map(|(n, &i)| (n.clone(), i)).collect();
        names.sort_by_key(|(_, i)| *i);
        let days: Vec<i32> = (0..ft.len()).map(|r| ft.int(r, dcol) as i32).collect();
        if days.len() < 2 {
            return Err("factors.tsv has no rows".into());
        }
        if days.windows(2).any(|w| w[0] >= w[1]) {
            return Err("factors.tsv dates are not strictly increasing".into());
        }
        let factors = names.iter().map(|(_, c)| (0..ft.len()).map(|r| ft.num(r, *c)).collect()).collect();
        let factor_names = names.into_iter().map(|(n, _)| n).collect();

        let ht = Table::read(&dir.join("holders.tsv"))?;
        let (hname, hfam) = (ht.col("name")?, ht.col("family")?);
        let mut families: Vec<String> = Vec::new();
        let mut fam_idx: HashMap<String, usize> = HashMap::new();
        let mut holders = Vec::with_capacity(ht.len());
        for r in 0..ht.len() {
            let f = ht.str(r, hfam).to_string();
            let fi = *fam_idx.entry(f.clone()).or_insert_with(|| {
                families.push(f);
                families.len() - 1
            });
            holders.push(Holder { name: ht.str(r, hname).to_string(), family: fi });
        }

        let bt = Table::read(&dir.join("borrowers.tsv"))?;
        let (bname, bkey, bsec) = (bt.col("name")?, bt.col("key")?, bt.col("sector")?);
        let mut sectors: Vec<String> = Vec::new();
        let mut sec_idx: HashMap<String, usize> = HashMap::new();
        let mut borrowers = Vec::with_capacity(bt.len());
        for r in 0..bt.len() {
            let s = bt.str(r, bsec).to_string();
            let si = *sec_idx.entry(s.clone()).or_insert_with(|| {
                sectors.push(s);
                sectors.len() - 1
            });
            borrowers.push(Borrower { name: bt.str(r, bname).to_string(), key: bt.str(r, bkey).to_string(), sector: si });
        }

        let it = Table::read(&dir.join("instruments.tsv"))?;
        let (ibor, icls) = (it.col("borrower")?, it.col("class")?);
        let ipiece = it.col("piece").ok();
        let instruments: Vec<Instrument> = (0..it.len())
            .map(|r| Instrument {
                borrower: it.int(r, ibor) as usize,
                class: Class::parse(it.str(r, icls)),
                piece: ipiece.map_or(0, |c| it.int(r, c) as usize),
            })
            .collect();

        let mt = Table::read(&dir.join("marks.tsv"))?;
        let c_inst = mt.col("instrument")?;
        let c_hold = mt.col("holder")?;
        let c_asof = mt.col("asof")?;
        let c_filed = mt.col("filed")?;
        let c_orig = mt.col("original")?;
        let c_prin = mt.col("principal")?;
        let c_fv = mt.col("fair_value")?;
        let c_spr = mt.col("spread_bp")?;

        let mut panel = Panel {
            source,
            days,
            factor_names,
            factors,
            families,
            sectors,
            holders,
            borrowers,
            instruments,
            obs: Vec::new(),
            inst_obs: Vec::new(),
        };

        struct Acc {
            principal: f64,
            fv: f64,
            spread_w: f64,
            filed: i32,
            original: bool,
        }
        let mut acc: HashMap<(usize, i32, usize), Acc> = HashMap::new();
        for r in 0..mt.len() {
            let inst = mt.int(r, c_inst) as usize;
            let holder = mt.int(r, c_hold) as usize;
            if inst >= panel.instruments.len() || holder >= panel.holders.len() {
                return Err(format!("marks.tsv row {r} references an unknown instrument or holder"));
            }
            let fam = panel.holders[holder].family;
            let asof = mt.int(r, c_asof) as i32;
            let filed = mt.int(r, c_filed) as i32;
            let principal = mt.num(r, c_prin);
            let fv = mt.num(r, c_fv);
            if !(principal > 0.0) || !fv.is_finite() {
                continue;
            }
            let spread = mt.num(r, c_spr);
            let e = acc.entry((inst, asof, fam)).or_insert(Acc {
                principal: 0.0,
                fv: 0.0,
                spread_w: 0.0,
                filed,
                original: true,
            });
            e.principal += principal;
            e.fv += fv;
            if spread.is_finite() {
                e.spread_w += spread * principal;
            }
            e.filed = e.filed.min(filed);
            e.original &= mt.int(r, c_orig) == 1;
        }
        let mut keys: Vec<(usize, i32, usize)> = acc.keys().copied().collect();
        keys.sort_unstable();
        panel.obs.reserve(keys.len());
        for k in keys {
            let a = &acc[&k];
            let (inst, asof, family) = k;
            panel.obs.push(Obs {
                inst,
                family,
                asof,
                ym: year_month(asof),
                asof_idx: panel.day_index(asof),
                filed_idx: panel.day_index(a.filed),
                price: a.fv / a.principal,
                principal: a.principal,
                spread_bp: a.spread_w / a.principal,
                original: a.original,
            });
        }
        panel.inst_obs = vec![(0, 0); panel.instruments.len()];
        let mut start = 0;
        while start < panel.obs.len() {
            let inst = panel.obs[start].inst;
            let mut end = start;
            while end < panel.obs.len() && panel.obs[end].inst == inst {
                end += 1;
            }
            panel.inst_obs[inst] = (start, end);
            start = end;
        }
        Ok(panel)
    }

    pub fn obs_of(&self, inst: usize) -> &[Obs] {
        let (a, b) = self.inst_obs[inst];
        &self.obs[a..b]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_parses_named_columns() {
        let t = Table::parse("a\tb\tc\n1\t2.5\tx\n\n3\t\ty\n");
        assert_eq!(t.len(), 2);
        assert_eq!(t.int(0, t.col("a").unwrap()), 1);
        assert_eq!(t.num(0, t.col("b").unwrap()), 2.5);
        assert!(t.num(1, t.col("b").unwrap()).is_nan());
        assert_eq!(t.str(1, t.col("c").unwrap()), "y");
        assert!(t.col("z").is_err());
    }

    #[test]
    fn serial_counts_days() {
        assert_eq!(serial(20240301) - serial(20240228), 2);
        assert_eq!(serial(20230301) - serial(20230228), 1);
        assert_eq!(serial(20260101) - serial(20250101), 365);
    }

    #[test]
    fn quarters_are_three_months_apart() {
        assert_eq!(year_month(20240331) - year_month(20231231), 3);
        assert!(is_calendar_quarter(20240630) && !is_calendar_quarter(20240531));
    }
}
