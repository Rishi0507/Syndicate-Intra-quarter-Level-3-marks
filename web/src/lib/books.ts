import { pathAt, type Universe } from "./data";

export interface BookNow {
  loans: number;
  principal: number;
  mark: number;
  drift: number;
  asof: number;
}

const SAME_FILING = 25;

export function booksAt(u: Universe, day: number): BookNow[] {
  const nf = u.meta.families.length;
  const latest = new Array<number>(nf).fill(-1);
  for (let k = 0; k < u.marksI.length / 3; k++) {
    if (u.marksI[3 * k + 2] <= day) {
      const f = u.marksI[3 * k];
      latest[f] = Math.max(latest[f], u.marksI[3 * k + 1]);
    }
  }
  const acc = Array.from({ length: nf }, () => ({ loans: 0, principal: 0, value: 0, moved: 0 }));
  for (let loan = 0; loan < u.loans.name.length; loan++) {
    if (day < u.loans.start[loan] || day > u.loans.last[loan]) {
      continue;
    }
    const now = pathAt(u, loan, day);
    if (!now) {
      continue;
    }
    const mine = new Map<number, { asof: number; arrival: number; price: number; principal: number }>();
    for (let k = u.loans.mark_start[loan]; k < u.loans.mark_start[loan + 1]; k++) {
      const f = u.marksI[3 * k];
      const asof = u.marksI[3 * k + 1];
      const arrival = u.marksI[3 * k + 2];
      if (arrival > day || asof < latest[f] - SAME_FILING) {
        continue;
      }
      const have = mine.get(f);
      if (!have || asof > have.asof) {
        mine.set(f, { asof, arrival, price: u.marksF[5 * k], principal: u.marksF[5 * k + 2] });
      }
    }
    for (const [f, m] of mine) {
      const then = pathAt(u, loan, Math.max(m.arrival, u.loans.start[loan]));
      if (!then || !(m.principal > 0)) {
        continue;
      }
      const a = acc[f];
      a.loans += 1;
      a.principal += m.principal;
      a.value += m.principal * m.price * 100;
      a.moved += m.principal * (now.price - then.price);
    }
  }
  return acc.map((a, f) => ({
    loans: a.loans,
    principal: a.principal,
    mark: a.principal > 0 ? a.value / a.principal : NaN,
    drift: a.principal > 0 ? a.moved / a.principal : NaN,
    asof: latest[f],
  }));
}
