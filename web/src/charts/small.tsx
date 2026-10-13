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
