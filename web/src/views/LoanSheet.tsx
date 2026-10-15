import { useEffect, useMemo, useState } from "react";
import { PriceChart, type AltBand } from "../charts/PriceChart";
import type { Universe } from "../lib/data";
import { INFERRED, priceCss } from "../lib/color";
import { classLabel, fmtDay, fmtPrice, fmtQuarter, fmtSigned, fmtSize, sectorLabel } from "../lib/format";
import type { Kernel } from "../lib/kernel";
import { bandHalfWidth, computeLoan, REGIMES, regimeOf, type LoanMark, type Print } from "../lib/loan";
import { clamp } from "../lib/scale";
import { setState, useStore } from "../state";

type Schedule = "filed" | "monthly" | "daily";

const SCHEDULES: { id: Schedule; label: string; gap: number }[] = [
  { id: "filed", label: "As filed", gap: 0 },
  { id: "monthly", label: "Monthly", gap: 21 },
  { id: "daily", label: "Daily", gap: 1 },
];

export function LoanSheet({ u, kernel, loan }: { u: Universe; kernel: Kernel; loan: number }) {
  const day = useStore((s) => s.day);
  const printSd = useStore((s) => s.printSd);
  useStore((s) => s.theme);
  const [prints, setPrints] = useState<Print[]>([]);
  const [adding, setAdding] = useState(false);
  const [schedule, setSchedule] = useState<Schedule>("filed");

  useEffect(() => {
    setPrints([]);
    setAdding(false);
    setSchedule("filed");
  }, [loan]);

  const lc = useMemo(() => computeLoan(u, kernel, loan, prints), [u, kernel, loan, prints]);

  const alt: AltBand | null = useMemo(() => {
    const gap = SCHEDULES.find((s) => s.id === schedule)!.gap;
    if (gap === 0 || lc.marks.length === 0) {
      return null;
    }
    const noise = median(lc.marks.map((m) => u.meta.families[m.family].noise));
    const extra: Print[] = [];
    for (let t = lc.start + gap; t <= lc.end; t += gap) {
      extra.push({ day: t, price: lc.curve.hindMean[t - lc.start] * 100, sd: noise });
    }
    const c = computeLoan(u, kernel, loan, [...prints, ...extra]).curve;
    return { label: `90% band if marked ${schedule}`, mean: c.mean, sd: c.sd };
  }, [u, kernel, loan, lc, prints, schedule]);

  const at = clamp(day, lc.start, lc.end);
  const i = at - lc.start;
  const mean = lc.curve.mean[i] * 100;
  const sd = lc.curve.sd[i] * 100;
  const half = bandHalfWidth(u, mean, sd);

  const latestYm = Math.max(-Infinity, ...lc.marks.filter((m) => m.arrival <= at).map((m) => m.ym));
  const latest = lc.marks.filter((m) => m.ym === latestYm && m.arrival <= at).sort((a, b) => a.arrival - b.arrival);
  const pending = lc.marks.filter((m) => m.ym > latestYm && m.asof <= at).length;

  const holders = useMemo(() => {
    const byFamily = new Map<number, LoanMark>();
    for (const m of lc.marks) {
      if (m.arrival > at) {
        continue;
      }
      const have = byFamily.get(m.family);
      if (!have || m.asof > have.asof) {
        byFamily.set(m.family, m);
      }
    }
    const newest = Math.max(-Infinity, ...[...byFamily.values()].map((m) => m.ym));
    return [...byFamily.values()].filter((m) => m.ym >= newest - 3).sort((a, b) => b.price - a.price);
  }, [lc, at]);

  const cls = u.meta.classes[u.loans.class[loan]];
  const outside = day < lc.start || day > lc.end;

  return (
    <article className="glass loan" aria-live="polite">
      <div className="loan-head">
        <div>
          <h2>{u.loans.name[loan]}</h2>
          <p>
            {classLabel(cls)}, {sectorLabel(u.meta.sectors[u.loans.sector[loan]]).toLowerCase()}. {u.loans.families[loan]}{" "}
            {u.loans.families[loan] === 1 ? "lender reports" : "lenders report"} it, {fmtSize(u.loans.size[loan])} held.
          </p>
        </div>
        <div className="estimate">
          <b className="num">{fmtPrice(mean)}</b>
          <span className="num">{half > mean ? `0 to ${fmtPrice(mean + half)}` : `± ${fmtPrice(half)}`}</span>
        </div>
      </div>
      <p className="note" style={{ marginTop: -8 }}>
        <i className="dot" style={{ background: priceCss(mean) }} />
        {REGIMES[regimeOf(mean)]}. The estimate, and the range that should hold the price nine times in ten, on {fmtDay(u.meta.days[at])}
        {outside ? ", the last day on which the loan was reported" : ""}.
      </p>

      <div>
        <PriceChart
          u={u}
          lc={lc}
          day={at}
          prints={prints}
          alt={alt}
          onPick={adding ? (d, price) => setPrints((p) => [...p, { day: d, price, sd: printSd }]) : undefined}
        />
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8, alignItems: "center", marginTop: 12 }}>
          <div className="seg" role="group" aria-label="How often the loan is marked">
            {SCHEDULES.map((s) => (
              <button key={s.id} aria-pressed={schedule === s.id} onClick={() => setSchedule(s.id)}>
                {s.label}
              </button>
            ))}
          </div>
          <button className="btn" aria-pressed={adding} onClick={() => setAdding((a) => !a)}>
            {adding ? "Stop adding trades" : "Add a trade"}
          </button>
          {prints.length > 0 && (
            <button className="btn btn--quiet" onClick={() => setPrints([])}>
              Remove trades
            </button>
          )}
        </div>
        <p className="note" style={{ marginTop: 8 }}>
          {adding
            ? "Click the chart where a trade happened. The estimate and its range are recomputed with it."
            : schedule === "filed"
              ? "Dots are lenders' marks on the date they describe; each hairline runs to the day the mark was published. The range can only tighten when a filing lands."
              : `The blue outline is the range this loan would have had if it were marked ${schedule} and published at once.`}
        </p>
      </div>

      <div className="loan-foot">
        <div className="section">
          <h3>{latest.length ? `Marks of ${fmtQuarter(latestYm)}, in the order they were filed` : "No marks public yet on this day"}</h3>
          {latest.length > 0 && (
            <table>
              <thead>
                <tr>
                  <th>Lender</th>
                  <th className="r">Mark</th>
                  <th className="r">Forecast</th>
                  <th className="r">Surprise</th>
                </tr>
              </thead>
              <tbody>
                {latest.slice(0, 8).map((m, k) => (
                  <tr key={k}>
                    <td>{u.meta.families[m.family].name}</td>
                    <td className="r">{fmtPrice(m.price)}</td>
                    <td className="r">{m.late ? "–" : fmtPrice(m.combined)}</td>
                    <td className="r">{m.late ? "–" : fmtSigned(m.price - m.combined)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          <p className="note" style={{ marginTop: 8 }}>
            {latest.length > 8 ? `The first 8 of ${latest.length} lenders. ` : ""}Each forecast was made the day before that lender
            filed, from everything already public. Where another lender had filed, the model is averaged with the lender's own last
            mark moved by the other lenders' change, with a weight learned from earlier quarters
            {pending > 0 ? `. ${pending} ${pending === 1 ? "mark" : "marks"} of the following quarter ${pending === 1 ? "is" : "are"} not yet filed.` : "."}
          </p>
        </div>

        <div className="section">
          <h3>Where each holder marks it</h3>
          {holders.length > 0 ? (
            <>
              <HolderStrip marks={holders} mean={mean} half={half} />
              <table>
                <thead>
                  <tr>
                    <th>Lender</th>
                    <th className="r">Latest mark</th>
                    <th className="r">Against the estimate</th>
                  </tr>
                </thead>
                <tbody>
                  {holders.slice(0, 8).map((m) => (
                    <tr key={m.family}>
                      <td style={{ whiteSpace: "nowrap" }}>{u.meta.families[m.family].name}</td>
                      <td className="r">{fmtPrice(m.price)}</td>
                      <td className="r">{fmtSigned(m.price - mean)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          ) : (
            <p className="note">No lender has filed a mark of this loan by this day.</p>
          )}
          <p className="note" style={{ marginTop: 8 }}>
            {holders.length > 8 ? `The 8 highest of ${holders.length} lenders. ` : ""}Each current lender's most recent filed mark
            against the estimate on this day; the shaded range is the 90% band. A lender's distance from the others tends to persist, and the
            estimate allows for it. How the band would carry into a dealer's spread is explored on the{" "}
            <a
              href="#/desk"
              onClick={(e) => {
                e.preventDefault();
                setState({ view: "desk" });
              }}
            >
              Desk page
            </a>
            .
          </p>
        </div>
      </div>
    </article>
  );
}

function HolderStrip({ marks, mean, half }: { marks: LoanMark[]; mean: number; half: number }) {
  const w = 320;
  const h = 34;
  const prices = marks.map((m) => m.price);
  const lo = Math.min(...prices, mean - half) - 0.5;
  const hi = Math.max(...prices, mean + half) + 0.5;
  const x = (v: number) => 8 + ((v - lo) / (hi - lo)) * (w - 16);
  const b0 = x(Math.max(lo, mean - half));
  const b1 = x(Math.min(hi, mean + half));
  return (
    <div className="chart" style={{ marginBottom: 6 }}>
      <svg viewBox={`0 0 ${w} ${h}`} height={h} role="img" aria-label="Lenders' latest marks against the estimate and its band">
        <rect x={b0} y={8} width={Math.max(1, b1 - b0)} height={14} rx={4} fill={INFERRED} opacity={0.17} />
        <line x1={8} x2={w - 8} y1={15} y2={15} stroke="var(--axis)" strokeWidth={1} />
        {prices.map((p, i) => (
          <circle key={i} cx={x(p)} cy={15} r={4} fill={priceCss(p)} stroke="var(--chart-surface)" strokeWidth={1.2} />
        ))}
        <line x1={x(mean)} x2={x(mean)} y1={4} y2={26} stroke={INFERRED} strokeWidth={2} />
        <text x={8} y={33}>
          {lo.toFixed(1)}
        </text>
        <text x={w - 8} y={33} textAnchor="end">
          {hi.toFixed(1)}
        </text>
      </svg>
    </div>
  );
}

function median(v: number[]): number {
  const s = [...v].sort((a, b) => a - b);
  return s.length ? s[s.length >> 1] : NaN;
}
