// The general-purpose charts: bars, columns, lines, calibration, histogram.

import { useState, type ReactNode } from "react";
import { clamp, line, linear, nice, ticks } from "../lib/scale";
import { Legend, Tip, useWidth, XAxis, YAxis, type Key } from "./base";

const SURFACE = "var(--chart-surface)";

/** Path of a bar with a rounded data end and a square foot at the baseline. */
function barPath(x: number, y: number, w: number, h: number, horizontal: boolean): string {
  const r = Math.min(4, horizontal ? w : h, (horizontal ? h : w) / 2);
  if (horizontal) {
    return `M${x},${y}h${w - r}a${r},${r} 0 0 1 ${r},${r}v${h - 2 * r}a${r},${r} 0 0 1 ${-r},${r}h${-(w - r)}Z`;
  }
  return `M${x},${y + h}v${-(h - r)}a${r},${r} 0 0 1 ${r},${-r}h${w - 2 * r}a${r},${r} 0 0 1 ${r},${r}v${h - r}Z`;
}

export interface BarRow {
  label: string;
  value: number;
  color: string;
  /** Text at the tip of the bar; defaults to the value. */
  tag?: string;
  detail?: ReactNode;
}

/** Horizontal bars from a shared baseline, labelled at the tip. */
export function HBars({ rows, format = (v) => v.toFixed(2), max, labelWidth = 210 }: { rows: BarRow[]; format?: (v: number) => string; max?: number; labelWidth?: number }) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const rowH = 34;
  const thick = 16;
  const lw = Math.min(labelWidth, width * 0.46);
  const top = max ?? Math.max(...rows.map((r) => r.value), 1e-9);
  const sx = linear([0, top], [lw, Math.max(lw + 10, width - 86)]);
  return (
    <div className="chart" ref={ref}>
      {width > 0 && (
        <svg height={rows.length * rowH + 4} role="img">
          <line className="axis-line" x1={lw} x2={lw} y1={0} y2={rows.length * rowH} />
          {rows.map((r, i) => {
            const y = i * rowH + (rowH - thick) / 2;
            const w = Math.max(2, sx(r.value) - lw);
            return (
              <g key={r.label} onPointerEnter={() => setHover(i)} onPointerLeave={() => setHover(null)}>
                <rect x={0} y={i * rowH} width={width} height={rowH} fill="transparent" />
                <text x={lw - 10} y={i * rowH + rowH / 2} dy="0.32em" textAnchor="end" className="label">
                  {r.label}
                </text>
                <path d={barPath(lw, y, w, thick, true)} fill={r.color} opacity={hover == null || hover === i ? 1 : 0.55} />
                <text x={lw + w + 8} y={i * rowH + rowH / 2} dy="0.32em" className="value">
                  {r.tag ?? format(r.value)}
                </text>
              </g>
            );
          })}
        </svg>
      )}
      {hover != null && rows[hover].detail && (
        <Tip x={lw + 20} y={hover * rowH + rowH} width={width}>
          {rows[hover].detail}
        </Tip>
      )}
    </div>
  );
}

export interface ColumnSeries {
  label: string;
  color: string;
  values: number[];
}

/** Vertical columns, one group per category, one column per series. */
export function Columns({
  categories,
  series,
  height = 190,
  format = (v) => v.toFixed(2),
  yLabel,
}: {
  categories: string[];
  series: ColumnSeries[];
  height?: number;
  format?: (v: number) => string;
  yLabel?: string;
}) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const m = { top: yLabel ? 20 : 8, right: 8, bottom: 26, left: 38 };
  const top = Math.max(...series.flatMap((s) => s.values.filter(Number.isFinite)), 1e-9);
  const sy = linear([0, top * 1.08], [height - m.bottom, m.top]);
  const band = (width - m.left - m.right) / Math.max(1, categories.length);
  const gap = 2;
  const colW = Math.min(24, (band * 0.62 - gap * (series.length - 1)) / series.length);
  const groupW = colW * series.length + gap * (series.length - 1);
  return (
    <div>
      {series.length > 1 && <Legend keys={series.map((s) => ({ label: s.label, color: s.color, kind: "swatch" }) as Key)} />}
      <div className="chart" ref={ref}>
        {width > 0 && (
          <svg height={height} role="img">
            {yLabel && (
              <text x={0} y={10} className="label">
                {yLabel}
              </text>
            )}
            <YAxis scale={sy} x0={m.left} x1={width - m.right} count={4} format={format} />
            <line className="axis-line" x1={m.left} x2={width - m.right} y1={sy(0)} y2={sy(0)} />
            {categories.map((c, i) => {
              const x0 = m.left + i * band + (band - groupW) / 2;
              return (
                <g key={c} onPointerEnter={() => setHover(i)} onPointerLeave={() => setHover(null)}>
                  <rect x={m.left + i * band} y={m.top} width={band} height={height - m.top} fill="transparent" />
                  {series.map((s, k) => {
                    const v = s.values[i];
                    if (!Number.isFinite(v)) {
                      return null;
                    }
                    const h = Math.max(1.5, sy(0) - sy(v));
                    return <path key={s.label} d={barPath(x0 + k * (colW + gap), sy(0) - h, colW, h, false)} fill={s.color} opacity={hover == null || hover === i ? 1 : 0.55} />;
                  })}
                  <text x={m.left + i * band + band / 2} y={height - m.bottom + 16} textAnchor="middle">
                    {c}
                  </text>
                </g>
              );
            })}
          </svg>
        )}
        {hover != null && (
          <Tip x={m.left + hover * band + band / 2} y={m.top + 8} width={width}>
            <b>{categories[hover]}</b>
            {series.map((s) => (
              <span key={s.label}>
                <br />
                <i style={{ background: s.color }} />
                {s.label} {format(s.values[hover])}
              </span>
            ))}
          </Tip>
        )}
      </div>
    </div>
  );
}

export interface LineSeries {
  label: string;
  color: string;
  values: ArrayLike<number>;
  width?: number;
}

/** Lines over a shared x, with a crosshair that reads every series at once. */
export function Lines({
  x,
  series,
  height = 200,
  xTicks,
  xFormat,
  yFormat = (v) => v.toFixed(2),
  yDomain,
  marks,
  yLabel,
}: {
  x: ArrayLike<number>;
  series: LineSeries[];
  height?: number;
  xTicks: number[];
  xFormat: (v: number) => string;
  yFormat?: (v: number) => string;
  yDomain?: [number, number];
  /** Vertical ticks along the foot of the plot, such as trades. */
  marks?: { x: number; color: string }[];
  yLabel?: string;
}) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const m = { top: yLabel ? 20 : 8, right: 12, bottom: 26, left: 38 };
  const n = x.length;
  let lo = Infinity;
  let hi = -Infinity;
  for (const s of series) {
    for (let i = 0; i < n; i++) {
      const v = s.values[i];
      if (Number.isFinite(v)) {
        lo = Math.min(lo, v);
        hi = Math.max(hi, v);
      }
    }
  }
  const dom = yDomain ?? nice(Math.min(lo, 0) === 0 ? 0 : lo, hi, 0.08);
  const sx = linear([x[0], x[n - 1]], [m.left, Math.max(m.left + 10, width - m.right)]);
  const sy = linear(dom, [height - m.bottom, m.top]);
  const nearest = (px: number) => {
    const target = sx.invert(px);
    let best = 0;
    for (let i = 1; i < n; i++) {
      if (Math.abs(x[i] - target) < Math.abs(x[best] - target)) {
        best = i;
      }
    }
    return best;
  };
  return (
    <div>
      {series.length > 1 && <Legend keys={series.map((s) => ({ label: s.label, color: s.color }))} />}
      <div className="chart" ref={ref}>
        {width > 0 && (
          <svg
            height={height}
            role="img"
            onPointerMove={(e) => setHover(nearest(e.clientX - e.currentTarget.getBoundingClientRect().left))}
            onPointerLeave={() => setHover(null)}
          >
            {yLabel && (
              <text x={0} y={10} className="label">
                {yLabel}
              </text>
            )}
            <YAxis scale={sy} x0={m.left} x1={width - m.right} count={4} format={yFormat} values={ticks(dom, 4)} />
            <XAxis scale={sx} y={height - m.bottom} values={xTicks} format={xFormat} />
            {marks?.map((mk, i) => (
              <line key={i} x1={sx(mk.x)} x2={sx(mk.x)} y1={height - m.bottom - 7} y2={height - m.bottom} stroke={mk.color} strokeWidth={1.25} />
            ))}
            {series.map((s) => (
              <path key={s.label} d={line(x, s.values, sx, sy)} fill="none" stroke={s.color} strokeWidth={s.width ?? 2} strokeLinejoin="round" strokeLinecap="round" />
            ))}
            {hover != null && (
              <g pointerEvents="none">
                <line x1={sx(x[hover])} x2={sx(x[hover])} y1={m.top} y2={height - m.bottom} stroke="var(--ink-2)" strokeWidth={1} opacity={0.5} />
                {series.map((s) =>
                  Number.isFinite(s.values[hover]) ? (
                    <circle key={s.label} cx={sx(x[hover])} cy={sy(clamp(s.values[hover], dom[0], dom[1]))} r={4} fill={s.color} stroke={SURFACE} strokeWidth={2} />
                  ) : null,
                )}
              </g>
            )}
          </svg>
        )}
        {hover != null && (
          <Tip x={sx(x[hover])} y={m.top} width={width}>
            <b>{xFormat(x[hover])}</b>
            {series.map((s) => (
              <span key={s.label}>
                <br />
                <i style={{ background: s.color }} />
                {s.label} {Number.isFinite(s.values[hover]) ? yFormat(s.values[hover]) : "–"}
              </span>
            ))}
          </Tip>
        )}
      </div>
    </div>
  );
}

/** Stated coverage against achieved coverage. Honest intervals lie on the diagonal. */
export function Calibration({ levels, series, size = 200, title }: { levels: number[]; series: { label: string; color: string; values: number[] }[]; size?: number; title?: string }) {
  const [hover, setHover] = useState<number | null>(null);
  const m = { top: title ? 22 : 8, right: 10, bottom: 30, left: 38 };
  const dom: [number, number] = [0.3, 1];
  const sx = linear(dom, [m.left, size - m.right]);
  const sy = linear(dom, [size - m.bottom, m.top]);
  const grid = [0.4, 0.6, 0.8, 1];
  return (
    <div className="chart" style={{ width: size }}>
      <svg width={size} height={size} role="img" onPointerLeave={() => setHover(null)}>
        {title && (
          <text x={m.left} y={12} className="label">
            {title}
          </text>
        )}
        {grid.map((g) => (
          <g key={g}>
            <line className="tick-line" x1={m.left} x2={size - m.right} y1={sy(g)} y2={sy(g)} />
            <text x={m.left - 8} y={sy(g)} dy="0.32em" textAnchor="end">
              {Math.round(g * 100)}
            </text>
            <text x={sx(g)} y={size - m.bottom + 16} textAnchor="middle">
              {Math.round(g * 100)}
            </text>
          </g>
        ))}
        <line className="axis-line" x1={sx(dom[0])} y1={sy(dom[0])} x2={sx(1)} y2={sy(1)} />
        {series.map((s) => (
          <g key={s.label}>
            <path d={line(levels, s.values, sx, sy)} fill="none" stroke={s.color} strokeWidth={2} strokeLinejoin="round" />
            {levels.map((l, i) => (
              <circle key={l} cx={sx(l)} cy={sy(clamp(s.values[i], dom[0], dom[1]))} r={4} fill={s.color} stroke={SURFACE} strokeWidth={2} />
            ))}
          </g>
        ))}
        {levels.map((l, i) => (
          <rect key={l} x={sx(l) - 14} y={m.top} width={28} height={size - m.top - m.bottom} fill="transparent" onPointerEnter={() => setHover(i)} />
        ))}
      </svg>
      {hover != null && (
        <Tip x={sx(levels[hover])} y={m.top} width={size}>
          <b>Stated {Math.round(levels[hover] * 100)}%</b>
          {series.map((s) => (
            <span key={s.label}>
              <br />
              <i style={{ background: s.color }} />
              {s.label} {(s.values[hover] * 100).toFixed(1)}%
            </span>
          ))}
        </Tip>
      )}
    </div>
  );
}

/** A distribution over evenly spaced bins from zero to `max`. */
export function Histogram({ bins, max, color, height = 130, xFormat = (v) => v.toFixed(1), xLabel }: { bins: number[]; max: number; color: string; height?: number; xFormat?: (v: number) => string; xLabel?: string }) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const m = { top: 6, right: 8, bottom: xLabel ? 40 : 24, left: 8 };
  const sx = linear([0, max], [m.left, Math.max(m.left + 10, width - m.right)]);
  const sy = linear([0, Math.max(...bins, 1e-9)], [height - m.bottom, m.top]);
  const bw = (sx(max) - sx(0)) / bins.length;
  return (
    <div className="chart" ref={ref}>
      {width > 0 && (
        <svg height={height} role="img" onPointerLeave={() => setHover(null)}>
          {bins.map((b, i) => {
            const h = sy(0) - sy(b);
            return (
              <g key={i} onPointerEnter={() => setHover(i)}>
                <rect x={sx(0) + i * bw} y={m.top} width={bw} height={height - m.top - m.bottom} fill="transparent" />
                {b > 0 && <rect x={sx(0) + i * bw + 1} y={sy(b)} width={Math.max(1, bw - 2)} height={Math.max(1, h)} rx={Math.min(2, bw / 2)} fill={color} opacity={hover == null || hover === i ? 1 : 0.55} />}
              </g>
            );
          })}
          <XAxis scale={sx} y={height - m.bottom} values={ticks([0, max], 5)} format={xFormat} />
          {xLabel && (
            <text x={sx(max / 2)} y={height - 6} textAnchor="middle" className="label">
              {xLabel}
            </text>
          )}
        </svg>
      )}
      {hover != null && (
        <Tip x={sx(0) + (hover + 0.5) * bw} y={m.top} width={width}>
          <b>
            {xFormat((hover * max) / bins.length)} to {xFormat(((hover + 1) * max) / bins.length)}
          </b>
          <br />
          {(bins[hover] * 100).toFixed(1)}% of days
        </Tip>
      )}
    </div>
  );
}
