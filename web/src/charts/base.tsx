// Shared pieces of every chart: sizing, axes, legends and the hover tooltip.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { ticks, type Scale } from "../lib/scale";

/** Width of an element, tracked as it changes. */
export function useWidth<T extends HTMLElement>(): [React.RefObject<T>, number] {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el) {
      return;
    }
    const ro = new ResizeObserver(([e]) => setWidth(Math.round(e.contentRect.width)));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  return [ref, width];
}

export function YAxis({
  scale,
  x0,
  x1,
  count = 4,
  format = (v: number) => String(v),
  values,
}: {
  scale: Scale;
  x0: number;
  x1: number;
  count?: number;
  format?: (v: number) => string;
  values?: number[];
}) {
  return (
    <g>
      {(values ?? ticks(scale.domain, count)).map((v) => (
        <g key={v} transform={`translate(0,${scale(v).toFixed(1)})`}>
          <line className="tick-line" x1={x0} x2={x1} />
          <text x={x0 - 8} dy="0.32em" textAnchor="end">
            {format(v)}
          </text>
        </g>
      ))}
    </g>
  );
}

export function XAxis({
  scale,
  y,
  values,
  format,
  anchor = "middle",
}: {
  scale: Scale;
  y: number;
  values: number[];
  format: (v: number) => string;
  anchor?: "start" | "middle" | "end";
}) {
  return (
    <g>
      <line className="axis-line" x1={scale.range[0]} x2={scale.range[1]} y1={y} y2={y} />
      {values.map((v) => (
        <text key={v} x={scale(v)} y={y + 16} textAnchor={anchor}>
          {format(v)}
        </text>
      ))}
    </g>
  );
}

export interface Key {
  label: string;
  color: string;
  kind?: "line" | "dot" | "band" | "swatch";
}

export function Legend({ keys }: { keys: Key[] }) {
  return (
    <div className="legend">
      {keys.map((k) => (
        <span key={k.label}>
          <i className={k.kind && k.kind !== "line" ? k.kind : undefined} style={{ color: k.color, background: k.color }} />
          {k.label}
        </span>
      ))}
    </div>
  );
}

/** A tooltip positioned inside its chart, flipped away from the right edge. */
export function Tip({ x, y, width, children }: { x: number; y: number; width: number; children: ReactNode }) {
  const flip = x > width * 0.6;
  return (
    <div
      className="tip"
      role="status"
      style={{
        left: flip ? undefined : x + 14,
        right: flip ? width - x + 14 : undefined,
        top: Math.max(0, y - 10),
      }}
    >
      {children}
    </div>
  );
}

/** The figure a chart sits in: an optional title, the plot, and a note. */
export function Figure({ title, note, children }: { title?: ReactNode; note?: ReactNode; children: ReactNode }) {
  return (
    <figure>
      {title && <h3 style={{ marginBottom: 10 }}>{title}</h3>}
      {children}
      {note && (
        <figcaption className="note" style={{ marginTop: 10 }}>
          {note}
        </figcaption>
      )}
    </figure>
  );
}
