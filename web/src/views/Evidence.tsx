import { useState } from "react";
import { Figure, Legend } from "../charts/base";
import { Calibration, Columns, HBars, Lines } from "../charts/small";
import { BRAND } from "../brand";
import type { Rung, Universe } from "../lib/data";
import { INFERRED, MUTED, RUNGS } from "../lib/color";
import { fmtInt, fmtPct, fmtQuarter } from "../lib/format";

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

function Ladder({ rungs }: { rungs: Rung[] }) {
  return (
    <HBars
      rows={rungs.map((r, i) => ({
        label: r.label,
        value: r.rmse,
        color: RUNGS[i],
        tag: i === 0 ? r.rmse.toFixed(2) : `${r.rmse.toFixed(2)}  (${(r.skill * 100).toFixed(0)}% of error removed)`,
        detail: (
          <>
            <b>{r.label}</b>
            <br />
            Root mean squared error {r.rmse.toFixed(3)} points
            <br />
            Mean absolute error {r.mae.toFixed(3)} points
            <br />
            {fmtInt(r.n)} forecasts
          </>
        ),
      }))}
      labelWidth={300}
    />
  );
}

function leaning(pull: { toward_low: number; toward_high: number }): string {
  const { toward_low: low, toward_high: high } = pull;
  if (low > 0.05 && low > high + 0.03) {
    return `Next quarter's average closes ${fmtPct(low, 0)} of the gap to the lowest mark: the most cautious lender tends to be early.`;
  }
  if (high > 0.05 && high > low + 0.03) {
    return `Next quarter's average closes ${fmtPct(high, 0)} of the gap to the highest mark: the most generous lender tends to be right.`;
  }
  return "Next quarter's average moves toward neither extreme: neither the most cautious lender nor the most generous one is early.";
}

function reading(rungs: Rung[]): string {
  const [, raw, habits, peers, sector] = rungs.map((r) => r.skill);
  const pct = (x: number) => `${Math.abs(x * 100).toFixed(0)}%`;
  const parts: string[] = [];
  if (raw < -0.02) {
    parts.push(
      `Taking marks at face value and rolling them with public spreads does worse than doing nothing, by ${pct(raw)}: a lender's distance from the rest is a habit, and a model that takes it for noise keeps pulling each mark toward the average.`,
    );
  } else {
    parts.push(`Rolling the last mark forward with public spreads and the pull to par removes ${pct(raw)} of the error.`);
  }
  if (habits - raw > 0.02) {
    parts.push(
      habits > 0.05
        ? `Modelling how each lender marks takes that to ${pct(habits)} removed.`
        : `Modelling how each lender marks repairs that, to about level with the last mark (${(habits * 100).toFixed(0)}%).`,
    );
  }
  if (peers - habits > 0.02) {
    parts.push(`Other lenders' marks of the same loan are where the information is: with them, ${pct(peers)} of the error is gone.`);
  }
  parts.push(
    sector - peers > 0.01
      ? `Repricing in the rest of the sector adds a further ${((sector - peers) * 100).toFixed(1)} points.`
      : "Repricing in the rest of the sector adds almost nothing on these forecasts.",
  );
  return parts.join(" ");
}

export function Evidence({ u }: { u: Universe }) {
  const ev = u.meta.eval;
  const ins = u.meta.insight;
  const synthetic = u.meta.source === "synthetic";
  const subsets: { id: string; label: string; rungs: Rung[] }[] = [
    { id: "all", label: "All forecasts", rungs: ev.all },
    { id: "followers", label: "Another lender had filed", rungs: ev.followers },
    { id: "movers", label: "Marks that moved a point or more", rungs: ev.movers },
    ...ev.by_regime.map((r) => ({ id: r.regime, label: cap(r.regime), rungs: r.ladder })),
  ];
  const [subset, setSubset] = useState("all");
  const chosen = subsets.find((s) => s.id === subset)!;
  const full = ev.all[ev.all.length - 1];
  const followers = ev.followers[ev.followers.length - 1];
  const perf = ev.coverage.find((c) => c.regime === "performing") ?? ev.coverage[0];
  const truthAll = u.meta.truth?.find((t) => t.label === "all");
  const regimes = ev.coverage.map((c) => cap(c.regime));
  const warn = ins.warning;

  return (
    <div className="page">
      <section className="glass pane">
        <header>
          <h2>Does it work</h2>
          <p className="lede">
            Every figure here is out of sample. For each of {ev.quarters} quarters the model was fitted only on marks filed before
            that quarter ended, then asked to forecast each mark of it the day before the lender filed. {fmtInt(ev.n)} forecasts
            were scored against what lenders went on to report.
          </p>
          {synthetic && (
            <p className="note" style={{ marginTop: 10, color: "var(--ink-2)" }}>
              These results are on a simulated market with a known answer, built in the SEC's own file format. They show the
              method recovers the truth where the truth is known. They are not findings about any real lender or loan.
            </p>
          )}
        </header>
        <div className="stats">
          <div className="stat inset">
            <span>Forecast error removed, against carrying the last mark forward</span>
            <b>{fmtPct(full.skill)}</b>
            <em>of squared error, all forecasts</em>
          </div>
          <div className="stat inset">
            <span>When another lender had already filed on the loan</span>
            <b>{fmtPct(followers.skill)}</b>
            <em>{fmtInt(followers.n)} forecasts</em>
          </div>
          <div className="stat inset">
            <span>Outcomes inside the 90% band, performing loans</span>
            <b>{fmtPct(perf.conformal[2], 1)}</b>
            <em>
              band {fmtPct(1 - perf.width90 / perf.naive_width90)} narrower than a naive one
            </em>
          </div>
          {truthAll && (
            <div className="stat inset">
              <span>Typical error against the true value, between marks</span>
              <b>{truthAll.mae_model.toFixed(2)}</b>
              <em>points, against {truthAll.mae_stale.toFixed(2)} for stale marks</em>
            </div>
          )}
        </div>
      </section>

      <section className="glass pane">
        <header>
          <h2>What each source of information is worth</h2>
          <p className="lede">
            A ladder. Each rung adds one source of information to the rung above it and is scored on the same forecasts, so the
            gain from each can be read off on its own. Shorter bars are better.
          </p>
        </header>
        <div className="seg" role="group" aria-label="Which forecasts" style={{ flexWrap: "wrap", marginBottom: 14 }}>
          {subsets.map((s) => (
            <button key={s.id} aria-pressed={subset === s.id} onClick={() => setSubset(s.id)}>
              {s.label}
            </button>
          ))}
        </div>
        <Figure note={`Root mean squared forecast error in price points, ${fmtInt(chosen.rungs[0].n)} forecasts. Par is 100.`}>
          <Ladder rungs={chosen.rungs} />
        </Figure>
        <p className="note" style={{ marginTop: 12 }}>
          {reading(chosen.rungs)}
        </p>
        <details style={{ marginTop: 14 }}>
          <summary className="note" style={{ cursor: "pointer" }}>
            Show as a table
          </summary>
          <table style={{ marginTop: 10 }}>
            <thead>
              <tr>
                <th>Information used</th>
                <th className="r">MAE</th>
                <th className="r">RMSE</th>
                <th className="r">Error removed</th>
              </tr>
            </thead>
            <tbody>
              {chosen.rungs.map((r) => (
                <tr key={r.id}>
                  <td>{r.label}</td>
                  <td className="r">{r.mae.toFixed(3)}</td>
                  <td className="r">{r.rmse.toFixed(3)}</td>
                  <td className="r">{fmtPct(r.skill, 1)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      </section>

      <section className="glass pane">
        <header>
          <h2>Is the error bar honest</h2>
          <p className="lede">
            A band claimed to hold 90% of outcomes should hold 90% of them. The model's own Gaussian bands are too narrow for
            credit, where a loan can fall ten points in a quarter. Widening them by the errors of earlier quarters repairs most
            of that without assuming any distribution. What is left is the quarter that was unlike the ones before it.
          </p>
        </header>
        <Legend
          keys={[
            { label: "Calibrated band", color: INFERRED },
            { label: "Gaussian band", color: MUTED },
          ]}
        />
        <div style={{ display: "flex", flexWrap: "wrap", gap: 18 }}>
          {ev.coverage.map((c) => (
            <Calibration
              key={c.regime}
              title={`${cap(c.regime)}, ${fmtInt(c.n)} forecasts`}
              levels={c.levels}
              series={[
                { label: "Gaussian", color: MUTED, values: c.gaussian },
                { label: "Calibrated", color: INFERRED, values: c.conformal },
              ]}
              size={230}
            />
          ))}
        </div>
        <p className="note" style={{ marginTop: 10 }}>
          Stated coverage across, achieved coverage up, both in percent. A band is honest where its line lies on the diagonal.
        </p>
        <div className="cols" style={{ marginTop: 26 }}>
          <Figure
            title="Width of a 90% band"
            note="In points. The naive band is the last mark plus or minus the historical range of quarterly moves, built to the same coverage. A narrower honest band is a tighter quote."
          >
            <Columns
              categories={regimes}
              series={[
                { label: "Last mark and history", color: MUTED, values: ev.coverage.map((c) => c.naive_width90) },
                { label: BRAND, color: INFERRED, values: ev.coverage.map((c) => c.width90) },
              ]}
              format={(v) => v.toFixed(1)}
            />
          </Figure>
          <Figure title="Forecast error, quarter by quarter" note="Root mean squared error in points for each quarter forecast. The model is refitted before each one.">
            <Lines
              x={ev.by_quarter.map((q) => q.ym)}
              series={[
                { label: "Last mark carried forward", color: MUTED, values: ev.by_quarter.map((q) => q.stale) },
                { label: BRAND, color: INFERRED, values: ev.by_quarter.map((q) => q.full) },
              ]}
              xTicks={ev.by_quarter.filter((q) => q.ym % 12 === 2).map((q) => q.ym)}
              xFormat={fmtQuarter}
              height={206}
            />
          </Figure>
        </div>
      </section>

      {u.meta.truth && truthAll && (
        <section className="glass pane">
          <header>
            <h2>Against the true value</h2>
            <p className="lede">
              Forecasting marks is the only test real data allows. The simulated market allows a harder one: every loan's true
              value is written down for every month end, including the months no lender reports. This is the estimate between
              marks, scored against it.
            </p>
          </header>
          <div className="cols">
            <Figure
              title="Typical error, by days since a filing last reached the market"
              note="Mean absolute error in points. The stale alternative is the average of the most recent marks on file, which is what a holder has without a model."
            >
              <Columns
                categories={u.meta.truth.filter((t) => t.label !== "all").map((t) => t.label.replace(" days", ""))}
                series={[
                  { label: "Stale marks", color: MUTED, values: u.meta.truth.filter((t) => t.label !== "all").map((t) => t.mae_stale) },
                  { label: BRAND, color: INFERRED, values: u.meta.truth.filter((t) => t.label !== "all").map((t) => t.mae_model) },
                ]}
              />
            </Figure>
            <Figure
              title="Do the bands hold the true value"
              note={`The bands are calibrated on marks, the only thing that can be checked on real data, and then drawn around a value no one observes. Here the 90% band holds the truth ${fmtPct(truthAll.cover[2], 1)} of the time: close, and a little narrow.`}
            >
              <Calibration
                levels={u.meta.levels}
                series={[
                  { label: "Gaussian", color: MUTED, values: truthAll.cover_gaussian },
                  { label: "Calibrated", color: INFERRED, values: truthAll.cover },
                ]}
                size={230}
              />
            </Figure>
          </div>
        </section>
      )}

      <section className="glass pane">
        <header>
          <h2>What the marks say before any model</h2>
          <p className="lede">
            Three facts read straight off the filings, on the {fmtInt(ins.shared_cells)} occasions when more than one lender
            reported the same loan on the same date.
          </p>
        </header>
        <div className="cols">
          <Figure
            title="How far apart lenders are on the same loan"
            note="Highest mark less lowest, in points, at the median and the 90th percentile of shared loans."
          >
            <Columns
              categories={ins.dispersion.map((d) => cap(d.regime))}
              series={[
                { label: "Median", color: MUTED, values: ins.dispersion.map((d) => d.range[0]) },
                { label: "90th percentile", color: INFERRED, values: ins.dispersion.map((d) => d.range[2]) },
              ]}
              format={(v) => v.toFixed(1)}
            />
          </Figure>
          <Figure
            title="Whose mark is the best guide to next quarter"
            note={
              `Error of each as a forecast of next quarter's average mark, performing loans, in points.${ins.pull ? ` ${leaning(ins.pull)}` : ""}`
            }
          >
            <Columns
              categories={["Lowest", "Average", "Median", "Highest"]}
              series={[{ label: "Error", color: INFERRED, values: ins.leader[0]?.rmse_low_mean_median_high ?? [] }]}
            />
          </Figure>
          <Figure
            title="Does disagreement warn of a markdown"
            note={`Share of shared loans whose average mark fell ${warn.markdown_points} points or more the next quarter, by tenth of disagreement this quarter. Across all of them the rate is ${fmtPct(warn.base_rate, 1)}. As a ranking, disagreement scores ${warn.auc_dispersion.toFixed(2)} and the level of the mark ${warn.auc_level.toFixed(2)}, where 0.5 is chance. Among performing loans alone disagreement scores ${warn.auc_dispersion_performing.toFixed(2)}${warn.auc_dispersion_performing < 0.55 ? ": it marks out loans already in trouble, which are both disputed and fragile, and does not see trouble coming" : ""}.`}
          >
            <Columns
              categories={warn.deciles.map((_, i) => String(i + 1))}
              series={[{ label: "Markdown rate", color: INFERRED, values: warn.deciles.map((d) => (d ? d.rate * 100 : NaN)) }]}
              format={(v) => `${v.toFixed(1)}%`}
            />
          </Figure>
        </div>
        {synthetic && (
          <p className="note" style={{ marginTop: 16 }}>
            In the simulated market lenders' errors are symmetric and unrelated to credit quality, so the average mark should
            lead and disagreement should warn of nothing. That is what these show. On real filings the same three charts are
            open questions.
          </p>
        )}
      </section>
    </div>
  );
}
