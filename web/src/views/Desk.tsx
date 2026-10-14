import { useMemo, useState } from "react";
import { Figure, Legend } from "../charts/base";
import { Cusp3D } from "../charts/Cusp3D";
import { Columns, Histogram, Lines } from "../charts/small";
import { INFERRED, MUTED, OBSERVED, SERIES } from "../lib/color";
import { fmtPct, fmtPrice } from "../lib/format";
import type { Desk as DeskParams, Kernel, Market } from "../lib/kernel";
import { DEFAULT_DESK, setState, useStore } from "../state";
import { PALETTES } from "../theme";

interface Loan {
  id: string;
  label: string;
  /** Standard deviation of one day's change in value, points. */
  vol: number;
  /** Noise of one round of marks taken together, points. */
  markSd: number;
  /** A typical error bar for the quote panel, points. */
  sd: number;
}

const LOANS: Loan[] = [
  { id: "performing", label: "Performing", vol: 0.04, markSd: 0.3, sd: 0.35 },
  { id: "watch", label: "Watch list", vol: 0.1, markSd: 0.6, sd: 0.9 },
  { id: "stressed", label: "Stressed", vol: 0.25, markSd: 1.0, sd: 1.8 },
];

const MARKS = [
  { id: 63, label: "Quarterly" },
  { id: 21, label: "Monthly" },
  { id: 1, label: "Daily" },
];

/** A competitive request-for-quote market: clients shop each quote hard. */
const TRAPPED: DeskParams = { arrival: 2, decay: 8, gamma: 0.02, informed: 0.45, inventory: 0 };

function Slider({
  label,
  hint,
  value,
  min,
  max,
  step,
  format,
  onChange,
}: {
  label: string;
  hint?: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
  onChange: (v: number) => void;
}) {
  const id = label.replace(/\W+/g, "-").toLowerCase();
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      <output htmlFor={id}>{format(value)}</output>
      <input id={id} type="range" min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} />
      {hint && <small>{hint}</small>}
    </div>
  );
}

export function Desk({ kernel }: { kernel: Kernel }) {
  const desk = useStore((s) => s.desk);
  const printSd = useStore((s) => s.printSd);
  const pal = PALETTES[useStore((s) => s.theme)];
  const [loanId, setLoanId] = useState("stressed");
  const [sd, setSd] = useState(1.8);
  const [markGap, setMarkGap] = useState(63);
  const [subsidy, setSubsidy] = useState(0);
  const [seed, setSeed] = useState(1);
  const loan = LOANS.find((l) => l.id === loanId)!;
  const setDesk = (patch: Partial<DeskParams>) => setState((s) => ({ desk: { ...s.desk, ...patch } }));

  const dailyVar = loan.vol * loan.vol;
  const quote = useMemo(() => kernel.quote(desk, 100, sd, dailyVar), [kernel, desk, sd, dailyVar]);
  const parts = [
    { label: "Making the market", value: quote.processing, color: SERIES[0] },
    { label: "Carrying inventory", value: quote.inventoryRisk, color: SERIES[1] },
    { label: "Being picked off", value: quote.adverseSelection, color: SERIES[2] },
  ];

  // The half-spread as the error bar grows: a straight line, whose slope is
  // the adverse selection width.
  const sweep = useMemo(() => {
    const xs = Array.from({ length: 41 }, (_, i) => (i * 3) / 40);
    return { xs, total: xs.map((x) => kernel.quote({ ...desk, inventory: 0 }, 100, x, dailyVar).halfSpread) };
  }, [kernel, desk, dailyVar]);

  const market: Market = useMemo(
    () => ({ desk: { ...desk, inventory: 0 }, dailyVar, markGap, markSd: loan.markSd, printSd, subsidy, subsidyAbove: 0.5 }),
    [desk, dailyVar, markGap, loan.markSd, printSd, subsidy],
  );

  const sim = useMemo(() => {
    const days = 756;
    const steps = kernel.simulate(market, loan.markSd, days, seed);
    const x = Array.from({ length: days }, (_, i) => i);
    const trades: { x: number; color: string }[] = [];
    steps.forEach((s, i) => {
      if (s.trades > 0) {
        trades.push({ x: i, color: SERIES[1] });
      }
    });
    return { x, sd: steps.map((s) => s.sd), spread: steps.map((s) => s.spread), trades, n: steps.reduce((a, s) => a + s.trades, 0) };
  }, [kernel, market, loan.markSd, seed]);

  const census = useMemo(() => kernel.census(market, 120, 7), [kernel, market]);
  const byMarks = useMemo(() => MARKS.map((m) => kernel.census({ ...market, markGap: m.id }, 120, 7)), [kernel, market]);
  const rest = useMemo(() => kernel.rest({ ...market, subsidy: 0 }), [kernel, market]);
  const trapped = rest.filter((r) => r.stable).length >= 2;

  return (
    <div className="page">
      <section className="glass pane">
        <header>
          <h2>From an error bar to a firm quote</h2>
          <p className="lede">
            A dealer asked to bid on a loan that has never traded charges for three things: making the market at all, carrying the
            position, and the chance that the client knows the loan better than the dealer does. The last is where the error bar
            enters. It is proportional to it.
          </p>
        </header>
        <div className="cols cols--wide">
          <div className="controls">
            <div className="seg" role="group" aria-label="Kind of loan" style={{ justifySelf: "start" }}>
              {LOANS.map((l) => (
                <button
                  key={l.id}
                  aria-pressed={loanId === l.id}
                  onClick={() => {
                    setLoanId(l.id);
                    setSd(l.sd);
                  }}
                >
                  {l.label}
                </button>
              ))}
            </div>
            <Slider label="Error bar on the estimate" hint="Standard error, in points. This is what the model supplies." value={sd} min={0} max={3} step={0.05} format={(v) => v.toFixed(2)} onChange={setSd} />
            <Slider label="Clients who know the loan's value" hint="Share of requests from informed counterparties." value={desk.informed} min={0} max={0.8} step={0.01} format={(v) => fmtPct(v)} onChange={(v) => setDesk({ informed: v })} />
            <Slider label="Requests for quotes" hint="Per day on each side, that a zero spread would win." value={desk.arrival} min={0.02} max={8} step={0.02} format={(v) => v.toFixed(2)} onChange={(v) => setDesk({ arrival: v })} />
            <Slider label="How hard clients shop the quote" hint="Fall in the win rate per point of half-spread." value={desk.decay} min={0.5} max={12} step={0.1} format={(v) => v.toFixed(1)} onChange={(v) => setDesk({ decay: v })} />
            <Slider label="Risk aversion" value={desk.gamma} min={0.005} max={0.3} step={0.005} format={(v) => v.toFixed(3)} onChange={(v) => setDesk({ gamma: v })} />
            <Slider label="Position held" hint="In clips. A long desk lowers both sides to sell." value={desk.inventory} min={-4} max={4} step={1} format={(v) => (v > 0 ? `long ${v}` : v < 0 ? `short ${-v}` : "flat")} onChange={(v) => setDesk({ inventory: v })} />
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              <button className="btn" onClick={() => setState({ desk: TRAPPED })}>
                Use a hard-shopped market
              </button>
              <button className="btn btn--quiet" onClick={() => setState({ desk: DEFAULT_DESK })}>
                Reset
              </button>
            </div>
          </div>
          <div>
            <div className="quote-row">
              <div className="inset">
                <b className="num">{fmtPrice(quote.bid)}</b>
                <span>Bid</span>
              </div>
              <div className="inset">
                <b className="num">{fmtPrice(quote.ask - quote.bid)}</b>
                <span>Spread, points</span>
              </div>
              <div className="inset">
                <b className="num">{fmtPrice(quote.ask)}</b>
                <span>Ask</span>
              </div>
            </div>
            <div className="stack" role="img" aria-label="What the half-spread is made of">
              {parts.map((p) => (
                <i key={p.label} style={{ background: p.color, flex: Math.max(p.value, 1e-6) }} />
              ))}
            </div>
            <Legend keys={parts.map((p) => ({ label: `${p.label} ${fmtPrice(p.value)}`, color: p.color, kind: "swatch" as const }))} />
            <div style={{ marginTop: 18 }}>
              <Figure
                title="Half-spread against the error bar"
                note={`Each point of standard error adds ${fmtPrice(kernel.adverseWidth(desk.informed))} points to each side of the quote at this share of informed clients. A print or a filing that halves the error bar takes that much back.`}
              >
                <Lines x={sweep.xs} series={[{ label: "Half-spread", color: INFERRED, values: sweep.total }]} xTicks={[0, 1, 2, 3]} xFormat={(v) => v.toFixed(2)} height={170} yLabel="Points, each side" />
              </Figure>
            </div>
          </div>
        </div>
      </section>

      <section className="glass pane">
        <header>
          <h2>A trade is also a measurement</h2>
          <p className="lede">
            Every print tells the market what the loan is worth. The error bar shrinks, the charge for being picked off shrinks with
            it, the quote tightens, and more clients trade. Run it backwards and it is a trap: no trades, no information, wide
            quotes, no trades. Below is one loan living through three years of that loop.
          </p>
        </header>
        <div className="cols cols--wide">
          <div className="controls">
            <div>
              <div className="note" style={{ marginBottom: 6 }}>
                The loan is marked
              </div>
              <div className="seg" role="group" aria-label="How often the loan is marked">
                {MARKS.map((m) => (
                  <button key={m.id} aria-pressed={markGap === m.id} onClick={() => setMarkGap(m.id)}>
                    {m.label}
                  </button>
                ))}
              </div>
            </div>
            <Slider label="Noise of a trade print" hint="How far a print can sit from value, in points." value={printSd} min={0.05} max={1.5} step={0.05} format={(v) => v.toFixed(2)} onChange={(v) => setState({ printSd: v })} />
            <Slider label="Support for loans gone dark" hint="Points of half-spread the dealer gives up whenever the fair half-spread is above half a point." value={subsidy} min={0} max={0.8} step={0.02} format={(v) => v.toFixed(2)} onChange={setSubsidy} />
            <div className="stats">
              <div className="stat inset">
                <span>Days it would trade at least monthly</span>
                <b>{fmtPct(census.tradeable)}</b>
              </div>
              <div className="stat inset">
                <span>Average spread</span>
                <b>{fmtPrice(census.meanSpread)}</b>
                <em>points</em>
              </div>
              <div className="stat inset">
                <span>Trades a year</span>
                <b>{census.tradesPerYear.toFixed(0)}</b>
              </div>
              <div className="stat inset">
                <span>Cost of support</span>
                <b>{fmtPrice(census.subsidyPerYear)}</b>
                <em>points per clip, a year</em>
              </div>
            </div>
            <p className="note">Long-run figures come from 120 simulated years of this market.</p>
          </div>
          <div>
            <Figure
              title="Error bar through three years"
              note={`${sim.n} trades in this history, each marked at the foot of the chart. The error bar climbs between observations and drops at each one.`}
            >
              <Lines
                x={sim.x}
                series={[{ label: "Standard error", color: INFERRED, values: sim.sd, width: 1.5 }]}
                xTicks={[0, 252, 504, 756]}
                xFormat={(v) => `year ${Math.floor(v / 252)}${v % 252 ? `, day ${v % 252}` : ""}`}
                marks={sim.trades}
                height={210}
                yLabel="Points"
              />
            </Figure>
            <button className="btn btn--quiet" style={{ marginTop: 6 }} onClick={() => setSeed((s) => s + 1)}>
              Draw another history
            </button>
          </div>
        </div>
        <div className="cols" style={{ marginTop: 26 }}>
          <Figure
            title="Share of days the loan is tradeable, by how often it is marked"
            note="Everything else held fixed. More frequent marks keep the error bar from growing far enough to shut trading down."
          >
            <Columns categories={MARKS.map((m) => m.label)} series={[{ label: "Tradeable", color: INFERRED, values: byMarks.map((c) => c.tradeable * 100) }]} format={(v) => `${v.toFixed(0)}%`} height={190} />
          </Figure>
          <Figure
            title="Average spread, by how often it is marked"
            note="In points. The saving is the adverse selection charge on the uncertainty that more frequent marks remove."
          >
            <Columns categories={MARKS.map((m) => m.label)} series={[{ label: "Spread", color: MUTED, values: byMarks.map((c) => c.meanSpread) }]} height={190} />
          </Figure>
          <Figure title="Where the error bar spends its time" note="The peak on the left is time spent trading and well known. The tail to the right is time spent in the dark, waiting for a filing.">
            <Histogram bins={census.hist} max={census.sdMax} color={INFERRED} height={166} xLabel="Standard error, points" />
          </Figure>
        </div>
      </section>

      <section className="glass pane">
        <header>
          <h2>Two regimes, one market</h2>
          <p className="lede">
            Average the arrivals away and the loop becomes a single equation for how the error bar changes. Its rest points form
            the surface below. Each curve across it belongs to a loan of one volatility and shows where its error bar settles as
            the flow of requests rises. Where a curve doubles back, the same flow supports two markets: one lit, one dark.
          </p>
        </header>
        <div className="cols cols--wide">
          <div>
            <Cusp3D kernel={kernel} market={market} rest={rest} />
            <Legend
              keys={[
                { label: "Settles here, well known", color: pal.lit },
                { label: "Settles here, in the dark", color: pal.dark },
                { label: "Tipping point", color: pal.tipping },
                { label: "This market", color: OBSERVED, kind: "dot" },
              ]}
            />
          </div>
          <div>
            <h3>{trapped ? "This market has both regimes" : "This market has one regime"}</h3>
            <table style={{ marginTop: 10 }}>
              <thead>
                <tr>
                  <th>Rest point</th>
                  <th className="r">Error bar</th>
                  <th className="r">Spread</th>
                  <th className="r">Trades a day</th>
                </tr>
              </thead>
              <tbody>
                {rest.map((r, i) => (
                  <tr key={i}>
                    <td>{r.stable ? (i === 0 && rest.length > 1 ? "Lit" : rest.length === 1 ? "Stable" : "Dark") : "Tipping point"}</td>
                    <td className="r">{fmtPrice(r.sd)}</td>
                    <td className="r">{fmtPrice(r.spread)}</td>
                    <td className="r">{r.rate < 0.01 ? r.rate.toFixed(4) : r.rate.toFixed(2)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="note" style={{ marginTop: 14 }}>
              {trapped
                ? "Same loan, same clients, two self-consistent markets. A loan in the dark one stays there until something measures it: a filing, or a trade someone was willing to do at a loss."
                : "With these assumptions the loan has a single resting state. Try the stressed loan in a hard-shopped market to see the surface fold."}
            </p>
            <p className="note" style={{ marginTop: 10 }}>
              The surface is a skeleton, not a forecast. It shows where two regimes exist, not how a loan moves between them: one
              print on a dark loan is often enough to relight it, which no smooth equation captures. The simulation above is the
              real process, and it lingers in each regime and jumps between them.
            </p>
            <p className="note" style={{ marginTop: 10 }}>
              One prediction follows that trade data could test. A filing cuts the error bar whatever the spread, so loans should
              trade most in the weeks after their lenders report.
            </p>
          </div>
        </div>
      </section>
    </div>
  );
}
