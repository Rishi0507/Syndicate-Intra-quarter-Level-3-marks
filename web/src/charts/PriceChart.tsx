import { useId, useMemo, useState } from "react";
import type { Universe } from "../lib/data";
import { INFERRED, MUTED, OBSERVED, SERIES } from "../lib/color";
import { fmtDay, fmtPrice } from "../lib/format";
import { bandHalfWidth, type LoanCurve, type Print } from "../lib/loan";
import { band, clamp, line, linear, nice } from "../lib/scale";
import { Legend, Tip, useWidth, YAxis } from "./base";

export interface AltBand {
  label: string;
  mean: ArrayLike<number>;
  sd: ArrayLike<number>;
}

interface Props {
  u: Universe;
  lc: LoanCurve;
  day: number;
  prints?: Print[];
  alt?: AltBand | null;
  height?: number;
  onPick?: (day: number, price: number) => void;
}

export function PriceChart({ u, lc, day, prints = [], alt = null, height = 250, onPick }: Props) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const clip = useId();
  const m = { top: 10, right: 12, bottom: 24, left: 36 };
  const factorH = 46;

  const s = useMemo(() => {
    const n = lc.end - lc.start + 1;
    const xs = Float64Array.from({ length: n }, (_, i) => lc.start + i);
    const mean = new Float64Array(n);
    const lo = new Float64Array(n);
    const hi = new Float64Array(n);
    const hind = new Float64Array(n);
    const firstSeen = Math.min(...lc.marks.map((mk) => mk.arrival), lc.end) - lc.start;
    let min = Infinity;
    let max = -Infinity;
    for (let i = 0; i < n; i++) {
      mean[i] = lc.curve.mean[i] * 100;
      hind[i] = lc.curve.hindMean[i] * 100;
      const h = bandHalfWidth(u, mean[i], lc.curve.sd[i] * 100);
      lo[i] = Math.max(0, mean[i] - h);
      hi[i] = mean[i] + h;
      if (i >= firstSeen) {
        min = Math.min(min, lo[i], hind[i]);
        max = Math.max(max, hi[i], hind[i]);
      }
    }
    for (const mk of lc.marks) {
      min = Math.min(min, mk.price);
      max = Math.max(max, mk.price);
    }
    let altLo: Float64Array | null = null;
    let altHi: Float64Array | null = null;
    if (alt) {
      altLo = new Float64Array(n);
      altHi = new Float64Array(n);
      for (let i = 0; i < n; i++) {
        const c = alt.mean[i] * 100;
        const h = bandHalfWidth(u, c, alt.sd[i] * 100);
        altLo[i] = c - h;
        altHi[i] = c + h;
      }
    }
    return { n, xs, mean, lo, hi, hind, altLo, altHi, domain: nice(Math.max(0, min), Math.min(max, 108), 0.08) };
  }, [u, lc, alt]);

  if (width === 0) {
    return <div className="chart" ref={ref} style={{ height: height + factorH }} />;
  }

  const sx = linear([lc.start, lc.end], [m.left, width - m.right]);
  const sy = linear(s.domain, [height - m.bottom, m.top]);
  const clampY = (v: number) => clamp(v, s.domain[0], s.domain[1]);
  const quarters = u.meta.quarters.filter(([t]) => t >= lc.start && t <= lc.end);
  const yearTicks = quarters.filter(([, ym]) => ym % 12 === 11);

  const level = u.meta.factor.level;
  let fmin = Infinity;
  let fmax = -Infinity;
  for (let t = lc.start; t <= lc.end; t++) {
    fmin = Math.min(fmin, level[t]);
    fmax = Math.max(fmax, level[t]);
  }
  const sf = linear(nice(fmin, fmax, 0.15), [height + factorH - 6, height + 8]);

  const at = hover == null ? null : clamp(Math.round(hover), lc.start, lc.end);
  const i = at == null ? -1 : at - lc.start;
  const filedHere = at == null ? [] : lc.marks.filter((mk) => mk.arrival === at);

  const move = (e: React.PointerEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    setHover(sx.invert(e.clientX - r.left));
  };

  const keys = [
    { label: "Estimate", color: INFERRED },
    { label: "90% band", color: INFERRED, kind: "band" as const },
    { label: "With hindsight", color: MUTED },
    { label: "Lender marks", color: OBSERVED, kind: "dot" as const },
    ...(prints.length ? [{ label: "Trades", color: SERIES[1], kind: "dot" as const }] : []),
    ...(alt ? [{ label: alt.label, color: SERIES[0] }] : []),
  ];

  return (
    <div>
      <Legend keys={keys} />
      <div className="chart" ref={ref}>
        <svg
          height={height + factorH}
          role="img"
          aria-label={`Daily estimate of ${u.loans.name[lc.loan]} with its 90% band and the marks lenders filed`}
          onPointerMove={move}
          onPointerLeave={() => setHover(null)}
          onClick={(e) => {
            if (!onPick) {
              return;
            }
            const r = e.currentTarget.getBoundingClientRect();
            const d = clamp(Math.round(sx.invert(e.clientX - r.left)), lc.start, lc.end);
            onPick(d, sy.invert(e.clientY - r.top));
          }}
          style={{ cursor: onPick ? "crosshair" : "default" }}
        >
          <YAxis scale={sy} x0={m.left} x1={width - m.right} count={4} format={(v) => v.toFixed(v % 1 ? 1 : 0)} />
          {quarters.map(([t]) => (
            <line key={t} className="tick-line" x1={sx(t)} x2={sx(t)} y1={m.top} y2={height - m.bottom} />
          ))}
          <line className="axis-line" x1={m.left} x2={width - m.right} y1={height - m.bottom} y2={height - m.bottom} />
          {yearTicks.map(([t, ym]) => (
            <text key={t} x={sx(t)} y={height - m.bottom + 16} textAnchor="middle">
              {Math.floor(ym / 12) + 1}
            </text>
          ))}

          <defs>
            <clipPath id={clip}>
              <rect x={m.left} y={m.top} width={Math.max(0, width - m.left - m.right)} height={height - m.top - m.bottom} />
            </clipPath>
          </defs>
          <g clipPath={`url(#${clip})`}>
          <path d={band(s.xs, s.lo.map(clampY), s.hi.map(clampY), sx, sy)} fill={INFERRED} opacity={0.17} />
          {s.altLo && s.altHi && (
            <>
              <path d={line(s.xs, s.altLo.map(clampY), sx, sy)} fill="none" stroke={SERIES[0]} strokeWidth={1.25} />
              <path d={line(s.xs, s.altHi.map(clampY), sx, sy)} fill="none" stroke={SERIES[0]} strokeWidth={1.25} />
            </>
          )}
          <path d={line(s.xs, s.hind, sx, sy)} fill="none" stroke={MUTED} strokeWidth={1.25} strokeLinejoin="round" />
          <path d={line(s.xs, s.mean, sx, sy)} fill="none" stroke={INFERRED} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />

          {lc.marks.map((mk, k) => (
            <g key={k}>
              <line x1={sx(mk.asof)} x2={sx(mk.arrival)} y1={sy(mk.price)} y2={sy(mk.price)} stroke={OBSERVED} strokeWidth={1} opacity={0.28} />
              <line x1={sx(mk.arrival)} x2={sx(mk.arrival)} y1={sy(mk.price) - 3} y2={sy(mk.price) + 3} stroke={OBSERVED} strokeWidth={1} opacity={0.5} />
              <circle cx={sx(mk.asof)} cy={sy(mk.price)} r={4} fill={OBSERVED} stroke="var(--chart-surface)" strokeWidth={2} />
            </g>
          ))}
          {prints.map((p, k) => (
            <rect
              key={k}
              x={sx(p.day) - 4}
              y={sy(p.price) - 4}
              width={8}
              height={8}
              transform={`rotate(45 ${sx(p.day)} ${sy(p.price)})`}
              fill={SERIES[1]}
              stroke="var(--chart-surface)"
              strokeWidth={2}
            />
          ))}

          {day >= lc.start && day <= lc.end && (
            <line x1={sx(day)} x2={sx(day)} y1={m.top} y2={height - m.bottom} stroke={OBSERVED} strokeWidth={1} opacity={0.35} />
          )}
          </g>

          <text x={m.left} y={height + 6} className="label">
            {u.meta.factor.name === "baa_spread" ? "Baa spread, %" : u.meta.factor.name}
          </text>
          <path d={line(s.xs, level.slice(lc.start, lc.end + 1), sx, sf)} fill="none" stroke={MUTED} strokeWidth={1.5} />
          <text x={width - m.right} y={sf(level[lc.end]) - 6} textAnchor="end" className="value">
            {level[lc.end].toFixed(2)}
          </text>

          {at != null && (
            <g pointerEvents="none">
              <line x1={sx(at)} x2={sx(at)} y1={m.top} y2={height + factorH - 6} stroke="var(--ink-2)" strokeWidth={1} opacity={0.5} />
              <circle cx={sx(at)} cy={sy(clampY(s.mean[i]))} r={4} fill={INFERRED} stroke="var(--chart-surface)" strokeWidth={2} />
            </g>
          )}
        </svg>
        {at != null && (
          <Tip x={sx(at)} y={sy(clampY(s.hi[i]))} width={width}>
            <b>{fmtDay(u.meta.days[at])}</b>
            <br />
            <i style={{ background: INFERRED }} />
            Estimate {fmtPrice(s.mean[i])} ± {fmtPrice((s.hi[i] - s.lo[i]) / 2)}
            <br />
            <i style={{ background: MUTED }} />
            With hindsight {fmtPrice(s.hind[i])}
            {filedHere.map((mk, k) => (
              <span key={k}>
                <br />
                <i style={{ background: OBSERVED }} />
                {u.meta.families[mk.family].name} filed {fmtPrice(mk.price)}
              </span>
            ))}
          </Tip>
        )}
      </div>
    </div>
  );
}
