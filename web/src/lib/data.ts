export interface Rung {
  id: string;
  label: string;
  n: number;
  mae: number;
  rmse: number;
  skill: number;
}

export interface Coverage {
  regime: string;
  n: number;
  levels: number[];
  gaussian: number[];
  conformal: number[];
  width90: number;
  naive_width90: number;
  naive_cover90: number;
  matched_width90: number;
  matched_naive_width90: number;
}

export interface Score {
  n: number;
  mae: number;
  rmse: number;
  skill: number;
}

export interface Ablation {
  id: string;
  label: string;
  kind: "removed" | "added";
  all: Score;
  followers: Score;
  rmse_change: number;
  followers_rmse_change: number;
  skill_change: number;
  followers_skill_change: number;
  worth_lo: number;
  worth_hi: number;
}

export interface RuleRow extends Rung {
  skill_lo: number;
  skill_hi: number;
  median_ae: number;
  closer: number;
  further: number;
  capped_skill: number;
  top_share: number | null;
}

export interface Versus {
  rows: RuleRow[];
  model_less_rule: number;
  model_less_rule_lo: number;
  model_less_rule_hi: number;
}

export interface LockGroup {
  n: number;
  stale_rmse: number;
  model_skill: number;
  rule_skill: number;
  model_mae: number;
  rule_mae: number;
}

export interface Signal {
  label: string;
  n: number;
  stale_mae: number;
  stale_rmse: number;
  mae: number;
  rmse: number;
  skill: number;
  rule_mae: number;
  rule_skill: number;
}

export interface Evaluation {
  n: number;
  quarters: number;
  all: Rung[];
  followers: Rung[];
  movers: Rung[];
  big_movers: Rung[];
  quiet: Rung[];
  ablations: Ablation[];
  versus: { all: Versus; followers: Versus };
  lockstep: { lockstep: LockGroup; independent: LockGroup };
  combine: { weights: [number, number][]; latest: number; min_forecasts: number };
  by_signal: Signal[];
  books?: Books;
  by_regime: { regime: string; ladder: Rung[]; pit: number[] }[];
  coverage: Coverage[];
  by_quarter: {
    ym: number;
    n: number;
    stale: number;
    model: number;
    rule: number;
    combined: number;
    followers: number;
    followers_stale: number;
    followers_model: number;
    followers_rule: number;
    followers_combined: number;
  }[];
  pit: number[];
}

export interface BookScore {
  n: number;
  skill: number;
  habits_skill: number;
  correlation: number;
  mean_abs_change: number;
  mean_abs_error: number;
  moved: number;
  direction_right: number;
  rule_skill: number;
  rule_direction_right: number;
  fell: number;
  majority_right: number;
}

export interface Books extends BookScore {
  min_loans: number;
  by_peer_share: (BookScore & { label: string })[];
  points: [number, number, number, number][];
}

export interface Vintage {
  ym: number;
  n: number;
  followers: number;
  stale: number;
  model: number;
  skill: number;
  followers_stale: number;
  followers_model: number;
  followers_skill: number;
  rule?: number;
  rule_skill?: number;
  followers_rule?: number;
  followers_rule_skill?: number;
  weight?: number;
  combined?: number;
  combined_skill?: number;
  followers_combined?: number;
  followers_combined_skill?: number;
}

export interface Audit {
  ingest?: Record<string, number | string>;
  vintage: Vintage[] | null;
}

export interface TruthRow {
  label: string;
  n: number;
  mae_model: number;
  mae_stale: number;
  rmse_model: number;
  rmse_stale: number;
  cover_gaussian: number[];
  cover: number[];
}

export interface Insight {
  shared_cells: number;
  catch_up?: { n: number; follow: number; continuation: number };
  dispersion: {
    regime: string;
    n: number;
    quantiles: number[];
    range: number[];
    share_over_1: number;
    share_over_5: number;
    share_over_10: number;
  }[];
  dispersion_by_quarter: { ym: number; n: number; median: number; p90: number }[];
  leader: { regime: string; n: number; rmse_low_mean_median_high: number[] }[];
  pull: { n: number; toward_low: number; toward_high: number } | null;
  warning: {
    n: number;
    markdown_points: number;
    base_rate: number;
    deciles: ({ range: number; rate: number } | null)[];
    auc_dispersion: number;
    auc_level: number;
    auc_dispersion_performing: number;
  };
}

export interface Family {
  name: string;
  phi: number;
  bias: number;
  noise: number;
  moves: number;
  overlap: number;
  marks: number;
  live: number;
  fit: [number, number, number];
}

export interface Meta {
  source: "synthetic" | "sec" | string;
  lenders_named?: boolean;
  counts: { loans: number; borrowers: number; marks: number; families: number; days: number; live: number; shared: number };
  days: number[];
  quarters: [number, number][];
  factor: { name: string; level: number[] };
  sectors: string[];
  classes: string[];
  families: Family[];
  fit: {
    dynamics: number[];
    phi_bar: number;
    phi_stressed: number;
    phi_entering: number;
    n_lead_lag: number;
    carry: number;
    carry_lags: [number, number];
    n_carry: number;
    beta_kept: [number, number];
    days_per_quarter: number;
    r2_public: number;
    sector_q_sd: number;
    noise_median: number;
    n_changes: number;
    n_quarters: number;
    phi_profile: [number, number][];
    idio_bins: [number, number, number][];
  };
  levels: number[];
  multipliers: number[][];
  floors: number[];
  engine: {
    batch_days: number;
    min_peers: number;
    path_step: number;
    price_lo: number;
    price_hi: number;
    sd_hi_points: number;
    min_train: number;
  };
  batches: [number, number, number, number, number, number, number][];
  arrivals: number[];
  mean_sd: (number | null)[];
  eval: Evaluation;
  truth: TruthRow[] | null;
  insight: Insight;
  audit?: Audit;
}

export interface Loans {
  name: string[];
  sector: number[];
  class: number[];
  start: number[];
  last: number[];
  families: number[];
  size: (number | null)[];
  now_mean: (number | null)[];
  now_sd: (number | null)[];
  live: number[];
  mark_start: number[];
  path_start: number[];
}

export interface Universe {
  meta: Meta;
  loans: Loans;
  marksI: Uint16Array;
  marksF: Float32Array;
  path: Uint8Array;
  shocks: Float64Array;
  time: number[];
  batches: Map<number, Map<number, { week: number; w: number; wv: number; n: number; inflate: number }[]>>;
}

async function fetchOk(url: string): Promise<Response> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`${url}: ${res.status} ${res.statusText}`);
  }
  return res;
}

export function dayToTime(d: number): number {
  return Date.UTC(Math.floor(d / 10000), (Math.floor(d / 100) % 100) - 1, d % 100);
}

export function assembleUniverse(meta: Meta, loans: Loans, marksI: ArrayBuffer, marksF: ArrayBuffer, path: ArrayBuffer): Universe {
  const level = meta.factor.level;
  const shocks = new Float64Array(level.length);
  for (let t = 1; t < level.length; t++) {
    shocks[t] = -(level[t] - level[t - 1]);
  }

  const batches: Universe["batches"] = new Map();
  for (const [sector, ym, week, w, wv, n, inflate] of meta.batches) {
    let bySector = batches.get(sector);
    if (!bySector) {
      batches.set(sector, (bySector = new Map()));
    }
    let list = bySector.get(ym);
    if (!list) {
      bySector.set(ym, (list = []));
    }
    list.push({ week, w, wv, n, inflate });
  }

  return {
    meta,
    loans,
    marksI: new Uint16Array(marksI),
    marksF: new Float32Array(marksF),
    path: new Uint8Array(path),
    shocks,
    time: meta.days.map(dayToTime),
    batches,
  };
}

export async function loadUniverse(base = "./data/"): Promise<Universe> {
  const [meta, loans, mi, mf, path] = await Promise.all([
    fetchOk(base + "meta.json").then((r) => r.json() as Promise<Meta>),
    fetchOk(base + "loans.json").then((r) => r.json() as Promise<Loans>),
    fetchOk(base + "marks_i.bin").then((r) => r.arrayBuffer()),
    fetchOk(base + "marks_f.bin").then((r) => r.arrayBuffer()),
    fetchOk(base + "path.bin").then((r) => r.arrayBuffer()),
  ]);
  return assembleUniverse(meta, loans, mi, mf, path);
}

export function yearMonth(d: number): number {
  return Math.floor(d / 10000) * 12 + (Math.floor(d / 100) % 100) - 1;
}

export function pathAt(u: Universe, loan: number, day: number): { price: number; sd: number } | null {
  const start = u.loans.start[loan];
  if (day < start || day > u.loans.last[loan]) {
    return null;
  }
  const e = u.meta.engine;
  const first = u.loans.path_start[loan];
  const count = u.loans.path_start[loan + 1] - first;
  if (count === 0) {
    return null;
  }
  const k = Math.min(count - 1, Math.floor((day - start) / e.path_step));
  const pc = u.path[2 * (first + k)];
  const sc = u.path[2 * (first + k) + 1];
  const price = (e.price_lo + (pc / 255) * (e.price_hi - e.price_lo)) * 100;
  const sd = (sc / 255) ** 2 * e.sd_hi_points;
  return { price, sd };
}
