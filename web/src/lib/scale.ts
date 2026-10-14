export type Scale = ((x: number) => number) & { domain: [number, number]; range: [number, number]; invert(y: number): number };

export function linear(domain: [number, number], range: [number, number]): Scale {
  const [d0, d1] = domain;
  const [r0, r1] = range;
  const k = d1 === d0 ? 0 : (r1 - r0) / (d1 - d0);
  const f = ((x: number) => r0 + (x - d0) * k) as Scale;
  f.domain = domain;
  f.range = range;
  f.invert = (y: number) => (k === 0 ? d0 : d0 + (y - r0) / k);
  return f;
}

export function ticks(domain: [number, number], count = 5): number[] {
  const [lo, hi] = domain;
  const span = hi - lo;
  if (!(span > 0)) {
    return [lo];
  }
  const raw = span / count;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? 10 * mag;
  const out: number[] = [];
  for (let v = Math.ceil(lo / step) * step; v <= hi + step * 1e-9; v += step) {
    out.push(Math.abs(v) < step * 1e-9 ? 0 : v);
  }
  return out;
}

export function nice(lo: number, hi: number, pad = 0.06): [number, number] {
  if (!(hi > lo)) {
    return [lo - 1, hi + 1];
  }
  const m = (hi - lo) * pad;
  return [lo - m, hi + m];
}

export function line(xs: ArrayLike<number>, ys: ArrayLike<number>, sx: Scale, sy: Scale, from = 0, to = xs.length): string {
  let d = "";
  let pen = false;
  for (let i = from; i < to; i++) {
    const y = ys[i];
    if (!Number.isFinite(y)) {
      pen = false;
      continue;
    }
    d += `${pen ? "L" : "M"}${sx(xs[i]).toFixed(1)},${sy(y).toFixed(1)}`;
    pen = true;
  }
  return d;
}

export function band(xs: ArrayLike<number>, lo: ArrayLike<number>, hi: ArrayLike<number>, sx: Scale, sy: Scale): string {
  const n = xs.length;
  if (n === 0) {
    return "";
  }
  let d = "";
  for (let i = 0; i < n; i++) {
    d += `${i ? "L" : "M"}${sx(xs[i]).toFixed(1)},${sy(hi[i]).toFixed(1)}`;
  }
  for (let i = n - 1; i >= 0; i--) {
    d += `L${sx(xs[i]).toFixed(1)},${sy(lo[i]).toFixed(1)}`;
  }
  return d + "Z";
}

export function clamp(x: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, x));
}
