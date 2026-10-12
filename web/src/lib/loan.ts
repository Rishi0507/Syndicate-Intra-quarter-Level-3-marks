// Assembles one loan's inputs from the exported tables and asks the kernel
// for its curve. The result matches what the native program computes.

import { yearMonth, type Universe } from "./data";
import { MARK_LEN, type Curve, type Kernel } from "./kernel";

export interface LoanMark {
  family: number;
  /** Grid day of the reporting date and of the filing. */
  asof: number;
  arrival: number;
  ym: number;
  /** Reported price and the same lender's price a quarter earlier, in points. */
  price: number;
  prev: number;
  /** Principal held, $ millions. */
  principal: number;
  /** What the mark implies about fair value once the lender's smoothing and bias are removed, in points. */
  implied: number;
  /** Forecast of this mark from public information alone, and just before it was filed. */
  publicMean: number;
  publicSd: number;
  peerMean: number;
  peerSd: number;
  seen: number;
  late: boolean;
}

export interface LoanCurve {
  loan: number;
  start: number;
  end: number;
  curve: Curve;
  marks: LoanMark[];
}

export interface Print {
  day: number;
  /** Points. */
  price: number;
  sd: number;
}

/** The price below which a loan is treated as stressed, as a fraction of par. */
const V_REF = 0.97;

/**
 * The view of a loan behind a reported price, for a lender that carries share
 * `phi` of its previous price into each new one. Smoothing bears only on a
 * change that starts or ends below the reference price, and a view stays
 * between nothing and par. Mirrors `filter::view` in the kernel.
 */
export function viewOf(price: number, prev: number, phi: number): number {
  if (!(Number.isFinite(prev) && phi > 0 && (prev < V_REF || price < V_REF))) {
    return price;
  }
  const raw = (price - phi * prev) / (1 - phi);
  return Math.min(Math.max(raw, Math.min(price, 0)), Math.max(price, 1));
}

/** Marks of one loan as rows of the binary tables. */
export function marksOf(u: Universe, loan: number) {
  const out: { family: number; asof: number; arrival: number; price: number; prev: number; principal: number; sw: number; swv: number }[] = [];
  for (let k = u.loans.mark_start[loan]; k < u.loans.mark_start[loan + 1]; k++) {
    out.push({
      family: u.marksI[3 * k],
      asof: u.marksI[3 * k + 1],
      arrival: u.marksI[3 * k + 2],
      price: u.marksF[5 * k],
      prev: u.marksF[5 * k + 1],
      principal: u.marksF[5 * k + 2],
      sw: u.marksF[5 * k + 3],
      swv: u.marksF[5 * k + 4],
    });
  }
  return out;
}

/**
 * What a loan hears from its sector: each weekly batch of other loans'
 * surprises, with the loan's own contribution taken back out.
 */
export function sectorFeed(u: Universe, loan: number, rows: ReturnType<typeof marksOf>): number[] {
  const { batch_days, min_peers } = u.meta.engine;
  const bySector = u.batches.get(u.loans.sector[loan]);
  const feed: number[] = [];
  if (!bySector) {
    return feed;
  }
  const said = new Map<number, { week: number; w: number; wv: number }>();
  for (const m of rows) {
    if (m.sw > 0) {
      said.set(yearMonth(u.meta.days[m.asof]), { week: Math.floor(m.arrival / batch_days), w: m.sw, wv: m.swv });
    }
  }
  for (const [, ym] of u.meta.quarters) {
    const list = bySector.get(ym);
    if (!list) {
      continue;
    }
    const mine = said.get(ym);
    for (const b of list) {
      let { w, wv, n } = b;
      if (mine && mine.week === b.week) {
        w -= mine.w;
        wv -= mine.wv;
        n -= 1;
      }
      if (n < min_peers || w <= 0) {
        continue;
      }
      feed.push(ym, (b.week + 1) * batch_days, wv / w, b.inflate / w);
    }
  }
  return feed;
}

export function computeLoan(u: Universe, kernel: Kernel, loan: number, prints: Print[] = []): LoanCurve {
  const rows = marksOf(u, loan);
  const start = u.loans.start[loan];
  const end = u.loans.last[loan];

  // A lender's previous price is in the table. The one before that, which
  // its previous view depends on, is the previous price of its previous mark.
  const yms = rows.map((m) => yearMonth(u.meta.days[m.asof]));
  const before = new Map<string, number>();
  rows.forEach((m, i) => before.set(`${m.family}:${yms[i]}`, m.prev));
  const implied = rows.map((m) => viewOf(m.price, m.prev, u.meta.families[m.family].fit[0]) - u.meta.families[m.family].fit[1]);

  const marks = new Float64Array(MARK_LEN * rows.length);
  rows.forEach((m, i) => {
    const [phi, bias, noise] = u.meta.families[m.family].fit;
    const earlier = before.get(`${m.family}:${yms[i] - 3}`);
    const anchor = Number.isFinite(m.prev) ? viewOf(m.prev, earlier ?? NaN, phi) - bias : NaN;
    marks.set([yms[i], m.asof, m.arrival, m.price, m.prev, anchor, phi, bias, noise, u.meta.fit.carry, m.family], MARK_LEN * i);
  });
  const printRows = prints.flatMap((p) => [p.day, p.price / 100, (p.sd / 100) ** 2]);

  const curve = kernel.curve({
    dynamics: u.meta.fit.dynamics,
    shocks: u.shocks,
    quarters: u.meta.quarters.flat(),
    marks,
    sector: sectorFeed(u, loan, rows),
    prints: printRows,
    start,
    end,
  });

  const out: LoanMark[] = rows.map((m, i) => {
    const f = curve.forecasts.subarray(6 * i, 6 * i + 6);
    return {
      family: m.family,
      asof: m.asof,
      arrival: m.arrival,
      ym: yms[i],
      price: m.price * 100,
      prev: m.prev * 100,
      principal: m.principal,
      implied: implied[i] * 100,
      publicMean: f[0] * 100,
      publicSd: f[1] * 100,
      peerMean: f[2] * 100,
      peerSd: f[3] * 100,
      seen: f[4],
      late: f[5] === 1,
    };
  });
  return { loan, start, end, curve, marks: out };
}

/** Which regime a price sits in, as an index into the multiplier table. */
export function regimeOf(points: number): 0 | 1 | 2 {
  return points >= 97 ? 0 : points >= 90 ? 1 : 2;
}

export const REGIMES = ["Performing", "Watch", "Stressed"] as const;

/**
 * Half-width of the calibrated band at a confidence level, in points, for a
 * standard deviation in points. The model's standard deviation says which
 * estimates are surer than others. It is added to the typical forecast error
 * of the regime, which is the part of credit risk no model sees coming, and
 * the sum is scaled by a multiplier learned from forecasting marks out of
 * sample. That is what gives the band honest coverage.
 */
export function bandHalfWidth(u: Universe, points: number, sd: number, level = 0.9): number {
  const k = u.meta.levels.indexOf(level);
  const regime = regimeOf(points);
  return u.meta.multipliers[regime][k < 0 ? 2 : k] * (sd + 100 * u.meta.floors[regime]);
}
