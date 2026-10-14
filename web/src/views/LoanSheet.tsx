import { useEffect, useMemo, useState } from "react";
import { PriceChart, type AltBand } from "../charts/PriceChart";
import type { Universe } from "../lib/data";
import { priceCss, SERIES } from "../lib/color";
import { classLabel, fmtDay, fmtPrice, fmtQuarter, fmtSigned, fmtSize, sectorLabel } from "../lib/format";
import type { Kernel, Quote } from "../lib/kernel";
import { bandHalfWidth, computeLoan, REGIMES, regimeOf, type Print } from "../lib/loan";
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
  const desk = useStore((s) => s.desk);
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
  const dyn = u.meta.fit.dynamics;
  const dpq = u.meta.fit.days_per_quarter;

  const quote: Quote = useMemo(() => {
    const dailyVar = kernel.dailyVar(dyn, mean / 100, dpq) * 1e4;
    return kernel.quote(desk, mean, sd, dailyVar);
  }, [kernel, dyn, dpq, desk, mean, sd]);

  const afterPrint = Math.sqrt((sd * sd * printSd * printSd) / (sd * sd + printSd * printSd));
  const saved = 2 * kernel.adverseWidth(desk.informed) * (sd - afterPrint);

  const latestYm = Math.max(-Infinity, ...lc.marks.filter((m) => m.arrival <= at).map((m) => m.ym));
  const latest = lc.marks.filter((m) => m.ym === latestYm && m.arrival <= at).sort((a, b) => a.arrival - b.arrival);
  const pending = lc.marks.filter((m) => m.ym > latestYm && m.asof <= at).length;

  const parts = [
    { label: "Making the market", value: quote.processing, color: SERIES[0] },
    { label: "Carrying inventory", value: quote.inventoryRisk, color: SERIES[1] },
    { label: "Being picked off", value: quote.adverseSelection, color: SERIES[2] },
  ];

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
                    <td className="r">{m.late ? "–" : fmtPrice(m.peerMean)}</td>
                    <td className="r">{m.late ? "–" : fmtSigned(m.price - m.peerMean)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          <p className="note" style={{ marginTop: 8 }}>
            {latest.length > 8 ? `The first 8 of ${latest.length} lenders. ` : ""}Each forecast was made the day before that lender
            filed, from everything already public
            {pending > 0 ? `. ${pending} ${pending === 1 ? "mark" : "marks"} of the following quarter ${pending === 1 ? "is" : "are"} not yet filed.` : "."}
          </p>
        </div>

        <div className="section">
          <h3>A dealer's quote on {fmtDay(u.meta.days[at])}</h3>
          <div className="ticket inset">
            <div>
              <span>Bid</span>
              <b className="num">{fmtPrice(quote.bid)}</b>
            </div>
            <div className="mid">
              <span>Spread</span>
              <b className="num">{fmtPrice(2 * quote.halfSpread)}</b>
            </div>
            <div>
              <span>Ask</span>
              <b className="num">{fmtPrice(quote.ask)}</b>
            </div>
          </div>
          <div className="stack" role="img" aria-label="What the spread is made of">
            {parts.map((p) => (
              <i key={p.label} style={{ background: p.color, flex: Math.max(p.value, 1e-6) }} title={`${p.label}: ${fmtPrice(p.value)} points a side`} />
            ))}
          </div>
          <table>
            <tbody>
              {parts.map((p) => (
                <tr key={p.label}>
                  <td>
                    <i className="dot" style={{ background: p.color }} />
                    {p.label}
                  </td>
                  <td className="r">{fmtPrice(p.value)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="note" style={{ marginTop: 8 }}>
            Points on each side. The charge for being picked off is proportional to how unsure the price is: one trade would take{" "}
            {fmtPrice(saved)} points off the spread. The desk's own settings are assumptions.{" "}
            <a
              href="#/desk"
              onClick={(e) => {
                e.preventDefault();
                setState({ view: "desk" });
              }}
            >
              Change them
            </a>
            .
          </p>
        </div>
      </div>
    </article>
  );
}

function median(v: number[]): number {
  const s = [...v].sort((a, b) => a - b);
  return s.length ? s[s.length >> 1] : NaN;
}
