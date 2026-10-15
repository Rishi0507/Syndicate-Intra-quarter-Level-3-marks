import { useState } from "react";
import { Figure, Legend } from "../charts/base";
import { Calibration, Columns, HBars, Lines } from "../charts/small";
import { BRAND } from "../brand";
import type { Ablation, LockGroup, Rung, RuleRow, Universe, Versus } from "../lib/data";
import { INFERRED, MUTED, RUNGS } from "../lib/color";
import { fmtInt, fmtPct, fmtQuarter } from "../lib/format";

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

function versusStale(skill: number): string {
  const pct = `${Math.abs(skill * 100).toFixed(0)}%`;
  return skill >= 0 ? `${pct} less squared error` : `${pct} more squared error`;
}

function Ladder({ rungs }: { rungs: Rung[] }) {
  return (
    <HBars
      rows={rungs.map((r, i) => ({
        label: r.label,
        value: r.rmse,
        color: RUNGS[i],
        tag: i === 0 ? r.rmse.toFixed(2) : `${r.rmse.toFixed(2)}  (${versusStale(r.skill)})`,
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

const signedPts = (x: number) => {
  const v = Math.abs(x * 100).toFixed(1);
  return v === "0.0" ? "0.0" : `${x > 0 ? "+" : "−"}${v}`;
};
const listOf = (xs: string[]) => (xs.length < 2 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`);
const span = (lo: number, hi: number) => `${fmtPct(lo)} to ${fmtPct(hi)}`;

function RuleTable({ v }: { v: Versus }) {
  return (
    <div style={{ overflowX: "auto" }}>
      <table>
        <thead>
          <tr>
            <th>Forecast</th>
            <th className="r">MAE</th>
            <th className="r">Median error</th>
            <th className="r">Squared error removed</th>
            <th className="r">90% interval</th>
            <th className="r">Errors capped at 5 points</th>
            <th className="r">Closer than the last mark</th>
            <th className="r">Further</th>
          </tr>
        </thead>
        <tbody>
          {v.rows.map((r: RuleRow) => (
            <tr key={r.id}>
              <td>{r.label}</td>
              <td className="r">{r.mae.toFixed(2)}</td>
              <td className="r">{r.median_ae.toFixed(2)}</td>
              <td className="r">{r.id === "stale" ? "" : fmtPct(r.skill, 1)}</td>
              <td className="r">{r.id === "stale" ? "" : span(r.skill_lo, r.skill_hi)}</td>
              <td className="r">{r.id === "stale" ? "" : fmtPct(r.capped_skill, 1)}</td>
              <td className="r">{r.id === "stale" ? "" : fmtPct(r.closer)}</td>
              <td className="r">{r.id === "stale" ? "" : fmtPct(r.further)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function verdict(v: Versus): string {
  const d = v.model_less_rule;
  const lo = v.model_less_rule_lo;
  const hi = v.model_less_rule_hi;
  const gap = `${signedPts(d)} points of squared error removed (90% interval ${signedPts(lo)} to ${signedPts(hi)})`;
  if (lo > 0) {
    return `The model is ahead of the rule by ${gap}.`;
  }
  if (hi < 0) {
    return `The rule is ahead of the model: ${gap}.`;
  }
  return `The model and the rule cannot be told apart here: ${gap}.`;
}

function concentration(model: RuleRow | undefined): string {
  if (!model || model.top_share == null || !Number.isFinite(model.top_share)) {
    return "";
  }
  const top = model.top_share;
  return ` The 1% of forecasts the model improved most account for ${fmtPct(top)} of its net gain${top > 1 ? ": the other 99%, taken together, are slightly worse than the last mark" : ""}.`;
}

function AblationTable({ rows, model, peers }: { rows: Ablation[]; model: number; peers: number }) {
  return (
    <div style={{ overflowX: "auto" }}>
      <table>
        <thead>
          <tr>
            <th>Variant</th>
            <th className="r">Squared error removed, all forecasts</th>
            <th className="r">Change</th>
            <th className="r">90% interval of the change</th>
            <th className="r">Another lender had filed</th>
            <th className="r">Change</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>The model as fitted</td>
            <td className="r">{fmtPct(model, 1)}</td>
            <td className="r"></td>
            <td className="r"></td>
            <td className="r">{fmtPct(peers, 1)}</td>
            <td className="r"></td>
          </tr>
          {rows.map((a) => (
            <tr key={a.id}>
              <td>
                {a.kind === "removed" ? "Without" : "With"} {a.label.charAt(0).toLowerCase() + a.label.slice(1)}
              </td>
              <td className="r">{fmtPct(a.all.skill, 1)}</td>
              <td className="r">{signedPts(a.skill_change)}</td>
              <td className="r">
                {signedPts(-a.worth_hi)} to {signedPts(-a.worth_lo)}
              </td>
              <td className="r">{fmtPct(a.followers.skill, 1)}</td>
              <td className="r">{signedPts(a.followers_skill_change)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ablationReading(rows: Ablation[]): string {
  const removed = rows.filter((a) => a.kind === "removed");
  if (!removed.length) {
    return "";
  }
  const helps = removed.filter((a) => a.worth_lo > 0).sort((a, b) => a.skill_change - b.skill_change);
  const idle = removed.filter((a) => a.worth_lo <= 0 && a.worth_hi >= 0);
  const hurts = removed.filter((a) => a.worth_hi < 0);
  const name = (a: Ablation) => a.label.charAt(0).toLowerCase() + a.label.slice(1);
  const parts: string[] = [];
  if (helps.length) {
    parts.push(
      `${helps[0].label} carries the model: without it, ${fmtPct(-helps[0].skill_change, 0)} of squared error removed is lost, ${fmtPct(-helps[0].followers_skill_change, 0)} where another lender had filed.`,
    );
    if (helps.length > 1) {
      const rest = helps.slice(1);
      const gains = rest.map((a) => -a.skill_change * 100);
      const range = `${Math.min(...gains).toFixed(1)} to ${Math.max(...gains).toFixed(1)}`;
      parts.push(`${listOf(rest.map(name))} add ${range} points each.`.replace(/^./, (c) => c.toUpperCase()));
    }
  }
  if (idle.length) {
    parts.push(`${listOf(idle.map(name))} change nothing measurable on these quarters.`.replace(/^./, (c) => c.toUpperCase()));
  }
  if (hurts.length) {
    parts.push(`Removing ${listOf(hurts.map(name))} would help slightly; the parts were fixed before this test and are left as they are.`);
  }
  return parts.join(" ");
}

function LockTable({ lock }: { lock: { lockstep: LockGroup; independent: LockGroup } }) {
  const rows: [string, LockGroup][] = [
    ["Marks have matched another lender's exactly before", lock.lockstep],
    ["Marks have never matched exactly", lock.independent],
  ];
  return (
    <table style={{ marginTop: 10 }}>
      <thead>
        <tr>
          <th>Another lender had filed, and</th>
          <th className="r">Forecasts</th>
          <th className="r">Removed, model</th>
          <th className="r">Removed, rule</th>
          <th className="r">MAE, model</th>
          <th className="r">MAE, rule</th>
        </tr>
      </thead>
      <tbody>
        {rows.map(([label, g]) => (
          <tr key={label}>
            <td>{label}</td>
            <td className="r">{fmtInt(g.n)}</td>
            <td className="r">{fmtPct(g.model_skill)}</td>
            <td className="r">{fmtPct(g.rule_skill)}</td>
            <td className="r">{g.model_mae.toFixed(2)}</td>
            <td className="r">{g.rule_mae.toFixed(2)}</td>
          </tr>
        ))}
      </tbody>
    </table>
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
  const [, raw, habits, peers] = rungs.map((r) => r.skill);
  const pct = (x: number) => `${Math.abs(x * 100).toFixed(0)}%`;
  const parts: string[] = [];
  if (raw < -0.02) {
    parts.push(
      `Taking marks at face value and rolling them with public spreads does worse than doing nothing, with ${pct(raw)} more squared error: a lender's distance from the rest is a habit, and a model that takes it for noise keeps pulling each mark toward the average.`,
    );
  } else {
    parts.push(`Rolling the last mark forward with public spreads and the pull to par removes ${pct(raw)} of the squared error.`);
  }
  if (habits - raw > 0.02) {
    parts.push(
      habits > 0.05
        ? `Modelling how each lender marks takes that to ${pct(habits)} removed.`
        : `Modelling how each lender marks repairs that, to about level with the last mark.`,
    );
  }
  if (peers - habits > 0.02) {
    parts.push(`Other lenders' marks of the same loan are where the information is: with them, ${pct(peers)} of the squared error is gone.`);
  }
  return parts.join(" ");
}

export function Evidence({ u }: { u: Universe }) {
  const ev = u.meta.eval;
  const ins = u.meta.insight;
  const audit = u.meta.audit;
  const synthetic = u.meta.source === "synthetic";
  const subsets: { id: string; label: string; rungs: Rung[] }[] = [
    { id: "all", label: "All forecasts", rungs: ev.all },
    { id: "followers", label: "Another lender had filed", rungs: ev.followers },
    { id: "movers", label: "Mark moved 1 point or more", rungs: ev.movers },
    { id: "big", label: "Mark moved 3 points or more", rungs: ev.big_movers },
    { id: "quiet", label: "Mark moved less than 1 point", rungs: ev.quiet },
    ...ev.by_regime.map((r) => ({ id: r.regime, label: cap(r.regime), rungs: r.ladder })),
  ];
  const [subset, setSubset] = useState("all");
  const [against, setAgainst] = useState<"followers" | "all">("followers");
  const chosen = subsets.find((s) => s.id === subset)!;
  const model = ev.all[ev.all.length - 1];
  const followers = ev.followers[ev.followers.length - 1];
  const versus = ev.versus;
  const pick = (v: Versus | undefined, id: string) => v?.rows.find((r) => r.id === id);
  const peerModel = pick(versus?.followers, "model");
  const peerRule = pick(versus?.followers, "rule");
  const peerCombined = pick(versus?.followers, "combined");
  const allRule = pick(versus?.all, "rule");
  const bins = ev.by_signal.slice(0, -1);
  const perf = ev.coverage.find((c) => c.regime === "performing") ?? ev.coverage[0];
  const truthAll = u.meta.truth?.find((t) => t.label === "all");
  const regimes = ev.coverage.map((c) => cap(c.regime));
  const warn = ins.warning;
  const lag = ins.catch_up;
  const facts = audit?.ingest;
  const vintage = audit?.vintage ?? null;
  const last = ev.by_quarter[ev.by_quarter.length - 1];
  const lastSkill = last ? 1 - (last.model / last.stale) ** 2 : NaN;
  const vintageOf = (ym: number) => vintage?.find((v) => v.ym === ym);
  const pooled = (rows: { n: number; stale: number; model: number }[]) => {
    const se0 = rows.reduce((a, r) => a + r.n * r.stale ** 2, 0);
    const se = rows.reduce((a, r) => a + r.n * r.model ** 2, 0);
    return se0 > 0 ? 1 - se / se0 : NaN;
  };
  const both = ev.by_quarter.filter((q) => vintageOf(q.ym));
  const fullAll = pooled(both);
  const vintAll = pooled(both.map((q) => vintageOf(q.ym)!));
  const fullPeers = pooled(both.map((q) => ({ n: q.followers, stale: q.followers_stale, model: q.followers_model })));
  const vintPeers = pooled(both.map((q) => vintageOf(q.ym)!).map((v) => ({ n: v.followers, stale: v.followers_stale, model: v.followers_model })));
  const vintRows = both.map((q) => vintageOf(q.ym)!);
  const hasRule = vintRows.length > 0 && vintRows.every((v) => v.followers_rule != null && v.followers_combined != null);
  const vintPeerRule = hasRule ? pooled(vintRows.map((v) => ({ n: v.followers, stale: v.followers_stale, model: v.followers_rule! }))) : NaN;
  const vintPeerComb = hasRule ? pooled(vintRows.map((v) => ({ n: v.followers, stale: v.followers_stale, model: v.followers_combined! }))) : NaN;
  const vintAllRule = hasRule ? pooled(vintRows.map((v) => ({ n: v.n, stale: v.stale, model: v.rule! }))) : NaN;
  const vintAllComb = hasRule ? pooled(vintRows.map((v) => ({ n: v.n, stale: v.stale, model: v.combined! }))) : NaN;
  const fullSkills = ev.by_quarter.map((q) => 1 - (q.model / q.stale) ** 2);
  const vintSkills = ev.by_quarter.map((q) => vintageOf(q.ym)?.skill ?? NaN);
  const restated = facts && Number(facts.restated_compared) > 0 ? facts : null;

  return (
    <div className="page">
      <section className="glass pane">
        <header>
          <h2>Does it work</h2>
          <p className="lede">
            Every figure here is out of sample. For each of {ev.quarters} quarters the model was fitted only on marks filed before
            that quarter ended, then asked to forecast each mark of it the day before the lender filed. {fmtInt(ev.n)} forecasts
            were scored against the marks lenders first reported. The target is the mark a lender will report, not an independent
            fair value.
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
            <span>When another lender had already filed on the loan</span>
            <b>{fmtPct(followers.skill)}</b>
            <em>
              of squared error removed{peerModel ? ` (90% interval ${span(peerModel.skill_lo, peerModel.skill_hi)})` : ""}, {fmtInt(followers.n)}{" "}
              forecasts, {fmtPct(followers.n / ev.n)} of all.
              {peerRule && peerCombined
                ? ` The simple rule removes ${fmtPct(peerRule.skill)}; the model and the rule combined, ${fmtPct(peerCombined.skill)}.`
                : ""}
            </em>
          </div>
          <div className="stat inset">
            <span>All forecasts</span>
            <b>{fmtPct(model.skill)}</b>
            <em>
              of squared error removed
              {allRule && versus
                ? `, against ${fmtPct(allRule.skill)} for the simple rule (difference ${signedPts(versus.all.model_less_rule)} points, 90% interval ${signedPts(versus.all.model_less_rule_lo)} to ${signedPts(versus.all.model_less_rule_hi)}). The lead comes from large moves on loans no other lender has filed yet${allRule.mae < model.mae ? "; on the typical forecast the rule is closer" : ""}.`
                : "; where no peer has filed, the estimate stays close to the last mark."}
            </em>
          </div>
          <div className="stat inset">
            <span>Outcomes inside the 90% band, performing loans</span>
            <b>{fmtPct(perf.conformal[2], 1)}</b>
            <em>
              naive band {fmtPct(perf.naive_cover90, 1)}; at equal coverage {fmtPct(1 - perf.matched_width90 / perf.matched_naive_width90)} narrower
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

      {versus && (
        <section className="glass pane">
          <header>
            <h2>Against simple rules</h2>
            <p className="lede">
              A model is worth having only if it beats what an analyst could do by hand with the same filings. Two rules use the
              same public marks: copy what the other lenders filed for the quarter, or move the lender's own last mark by the
              change the other lenders reported. Every row is scored on the same forecasts. Intervals come from redrawing whole
              loans 1,000 times, so a loan with many marks cannot dominate.
            </p>
          </header>
          <div className="seg" role="group" aria-label="Which forecasts" style={{ marginBottom: 14 }}>
            <button aria-pressed={against === "followers"} onClick={() => setAgainst("followers")}>
              Another lender had filed
            </button>
            <button aria-pressed={against === "all"} onClick={() => setAgainst("all")}>
              All forecasts
            </button>
          </div>
          <Figure note="Squared error removed against carrying the lender's last mark forward, in percent. Longer bars are better.">
            <HBars
              rows={versus[against].rows
                .filter((r) => r.id !== "stale")
                .map((r, i) => ({
                  label: r.label,
                  value: Math.max(0, r.skill * 100),
                  color: RUNGS[i + 1],
                  tag: `${fmtPct(r.skill, 1)}  (${span(r.skill_lo, r.skill_hi)})`,
                }))}
              format={(v) => `${v.toFixed(0)}%`}
              labelWidth={300}
            />
          </Figure>
          <RuleTable v={versus[against]} />
          <p className="note" style={{ marginTop: 12 }}>
            {verdict(versus[against])}
            {concentration(pick(versus[against], "model"))} Errors are in points; "closer" and "further" count forecasts more
            than 0.05 points nearer to or further from the outcome than the last mark.
          </p>
          <p className="note" style={{ marginTop: 8 }}>
            The combined forecast moves the model toward the rule by a weight learned each quarter from the earlier quarters
            only{ev.combine ? ` (latest ${ev.combine.latest.toFixed(2)})` : ""}, and is used where another lender had filed. It was
            added after this comparison was first run, so the next quarter to be filed is its first clean test.
          </p>
          {ev.lockstep && ev.lockstep.lockstep.n > 0 && (
            <>
              <h3 style={{ marginTop: 22 }}>Shared prices</h3>
              <p className="note">
                On {fmtInt(ev.lockstep.lockstep.n)} of the {fmtInt(followers.n)} forecasts where another lender had filed, the
                lender's marks of that loan had already matched another holder's to within 0.05 points after a move. That points to
                a shared price source, such as a dealer quote or a common valuation agent, rather than an independent view. The
                flag uses only earlier quarters. Forecasts are much easier there, so the headline figure mixes two kinds of loan.
              </p>
              <LockTable lock={ev.lockstep} />
            </>
          )}
        </section>
      )}

      {bins.length > 0 && (
        <section className="glass pane">
          <header>
            <h2>Where the estimate earns its keep</h2>
            <p className="lede">
              Forecasts grouped by how far the estimate had moved from the lender's own last mark on the day the forecast was made.
              The grouping uses only what was known in advance, so it is a fair test. Most of the time the estimate barely moves
              and is no worse than the last mark. When it does move, the move is usually right.
            </p>
          </header>
          <div className="cols">
            <Figure title="Typical error, by how far the estimate had moved" note="Mean absolute error in points, with par at 100.">
              <Columns
                categories={bins.map((b) => b.label.replace(" points", "").replace(" point", "").replace("Under ", "< ").replace(" or more", "+"))}
                series={[
                  { label: "Last mark carried forward", color: MUTED, values: bins.map((b) => b.stale_mae) },
                  { label: BRAND, color: INFERRED, values: bins.map((b) => b.mae) },
                ]}
                format={(v) => v.toFixed(2)}
              />
            </Figure>
            <div style={{ overflowX: "auto" }}>
              <table>
                <thead>
                  <tr>
                    <th>Estimate had moved</th>
                    <th className="r">Forecasts</th>
                    <th className="r">MAE, last mark</th>
                    <th className="r">MAE, {BRAND}</th>
                    <th className="r">MAE, rule</th>
                    <th className="r">Squared error removed</th>
                  </tr>
                </thead>
                <tbody>
                  {ev.by_signal.map((b) => (
                    <tr key={b.label}>
                      <td>{b.label}</td>
                      <td className="r">{fmtInt(b.n)}</td>
                      <td className="r">{b.stale_mae.toFixed(2)}</td>
                      <td className="r">{b.mae.toFixed(2)}</td>
                      <td className="r">{b.rule_mae != null ? b.rule_mae.toFixed(2) : ""}</td>
                      <td className="r">{fmtPct(b.skill)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </section>
      )}

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
          {(subset === "movers" || subset === "big" || subset === "quiet") &&
            " Grouping by the move that actually happened favours whichever forecast predicted it: on marks that did not move, the last mark is right by construction. The grouping above, by what was known in advance, is the fair comparison."}
        </p>
        <p className="note" style={{ marginTop: 8 }}>
          The ladder adds sources of information in one order. The parts of the model are tested one at a time in the next section.
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
                <th className="r">Squared error against the last mark</th>
              </tr>
            </thead>
            <tbody>
              {chosen.rungs.map((r, i) => (
                <tr key={r.id}>
                  <td>{r.label}</td>
                  <td className="r">{r.mae.toFixed(3)}</td>
                  <td className="r">{r.rmse.toFixed(3)}</td>
                  <td className="r">{i === 0 ? "" : versusStale(r.skill)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      </section>

      {ev.ablations.length > 0 && ev.ablations[0].all && (
        <section className="glass pane">
          <header>
            <h2>What each part of the model is worth</h2>
            <p className="lede">
              Each part is switched off in turn, with everything else left as fitted, and the backtest is run again on the same
              forecasts. A negative change means the part helps. Sector repricing is the reverse test: a part that was tried,
              added to the model.
            </p>
          </header>
          <AblationTable rows={ev.ablations} model={model.skill} peers={followers.skill} />
          <p className="note" style={{ marginTop: 12 }}>
            {ablationReading(ev.ablations)} Sector repricing helps in the simulated market, where sectors share shocks by design,
            and changes nothing on the filings, so the model leaves it out.
          </p>
        </section>
      )}

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
            title="Width of a 90% band, at equal coverage"
            note={`In points. The naive band is the last mark plus or minus the historical range of quarterly moves. Bands that miss their stated coverage by different amounts cannot be compared directly, so both are scaled here to hold exactly 90% of outcomes. As stated, the calibrated band held ${ev.coverage.map((c) => fmtPct(c.conformal[2], 1)).join(", ")} and the naive band ${ev.coverage.map((c) => fmtPct(c.naive_cover90, 1)).join(", ")}, for ${regimes.join(", ").toLowerCase()} loans.`}
          >
            <Columns
              categories={regimes}
              series={[
                { label: "Last mark and history", color: MUTED, values: ev.coverage.map((c) => c.matched_naive_width90) },
                { label: BRAND, color: INFERRED, values: ev.coverage.map((c) => c.matched_width90) },
              ]}
              format={(v) => v.toFixed(1)}
            />
          </Figure>
          <Figure title="Forecast error, quarter by quarter" note="Root mean squared error in points for each quarter forecast. The model is refitted before each one.">
            <Lines
              x={ev.by_quarter.map((q) => q.ym)}
              series={[
                { label: "Last mark carried forward", color: MUTED, values: ev.by_quarter.map((q) => q.stale) },
                ...(ev.by_quarter[0]?.rule != null ? [{ label: "Simple rule", color: RUNGS[2], values: ev.by_quarter.map((q) => q.rule) }] : []),
                { label: BRAND, color: INFERRED, values: ev.by_quarter.map((q) => q.model) },
              ]}
              xTicks={ev.by_quarter.filter((q) => q.ym % 12 === 2).map((q) => q.ym)}
              xFormat={fmtQuarter}
              height={206}
            />
          </Figure>
        </div>
      </section>

      {!synthetic && (restated || vintage) && (
        <section className="glass pane">
          <header>
            <h2>Could the results see the future</h2>
            <p className="lede">
              A backtest can leak information from after the forecast date in three ways: through marks that were later restated,
              through cleaning and matching rules that saw later filings, and through design choices made while looking at the
              results. Each is checked here.
            </p>
          </header>
          <div className="stats" style={{ marginBottom: 22 }}>
            {restated && (
              <div className="stat inset">
                <span>Later restatements</span>
                <b>{fmtPct(Number(restated.restated_over_tenth_point) / Number(restated.restated_compared), 1)}</b>
                <em>
                  of {fmtInt(Number(restated.restated_compared))} marks repeated in a later filing changed by more than 0.1 points.
                  Forecasts are scored against the first-reported mark.
                </em>
              </div>
            )}
            {vintage && both.length > 0 && (
              <>
                <div className="stat inset">
                  <span>All forecasts, panel rebuilt as it stood at the time</span>
                  <b>
                    {fmtPct(fullAll)} → {fmtPct(vintAll)}
                  </b>
                  <em>of squared error removed, full panel against the panel as it stood, {both.length} quarters</em>
                </div>
                <div className="stat inset">
                  <span>Another lender had filed, panel as it stood</span>
                  <b>
                    {fmtPct(fullPeers)} → {fmtPct(vintPeers)}
                  </b>
                  <em>
                    of squared error removed; the lower figure is the one to expect in real time.
                    {hasRule ? ` On the same panels the simple rule removes ${fmtPct(vintPeerRule)} and the combined forecast ${fmtPct(vintPeerComb)}.` : ""}
                  </em>
                </div>
              </>
            )}
            {last && (
              <div className="stat inset">
                <span>{fmtQuarter(last.ym)}, added after the model was fixed</span>
                <b>{fmtPct(lastSkill)}</b>
                <em>
                  of squared error removed, {fmtInt(last.n)} forecasts from reports filed in August 2026. The comparison with simple
                  rules and the combined forecast came later, so for them the next quarter is the first clean test.
                </em>
              </div>
            )}
          </div>
          {vintage && (
            <Figure
              title="Squared error removed, quarter by quarter"
              note="The panel as it stood is rebuilt from every row filed by 105 days after the quarter ended, so borrower matching and the splitting of loans into pieces see nothing later. Borrowers matched later can still be linked in the full panel, which gives the model more shared loans."
            >
              <Columns
                categories={ev.by_quarter.map((q) => fmtQuarter(q.ym).replace(" 20", " '"))}
                series={[
                  { label: "Full panel", color: MUTED, values: fullSkills.map((x) => x * 100) },
                  { label: "Panel as it stood", color: INFERRED, values: vintSkills.map((x) => x * 100) },
                ]}
                format={(v) => `${v.toFixed(0)}%`}
              />
            </Figure>
          )}
          {vintage && hasRule && (
            <div style={{ marginTop: 26 }}>
              <Figure
                title="Another lender had filed, panel as it stood"
                note={`Squared error removed in each quarter. Pooled: model ${fmtPct(vintPeers)}, simple rule ${fmtPct(vintPeerRule)}, combined ${fmtPct(vintPeerComb)}. Over all forecasts: model ${fmtPct(vintAll)}, rule ${fmtPct(vintAllRule)}, combined ${fmtPct(vintAllComb)}. The combination's weight in each quarter is learned from the earlier quarters' panels only; the first quarter has none and uses the model alone.`}
              >
                <Columns
                  categories={vintRows.map((v) => fmtQuarter(v.ym).replace(" 20", " '"))}
                  series={[
                    { label: "Simple rule", color: MUTED, values: vintRows.map((v) => (v.followers_rule_skill ?? NaN) * 100) },
                    { label: BRAND, color: INFERRED, values: vintRows.map((v) => v.followers_skill * 100) },
                    { label: "Combined", color: RUNGS[4], values: vintRows.map((v) => (v.followers_combined_skill ?? NaN) * 100) },
                  ]}
                  format={(v) => `${v.toFixed(0)}%`}
                />
              </Figure>
            </div>
          )}
        </section>
      )}

      {u.meta.truth && truthAll && (
        <section className="glass pane">
          <header>
            <h2>Against the true value</h2>
            <p className="lede">
              Forecasting marks is the only test real data allows. The simulated market allows a check of the software: every
              loan's true value is written down for every month end, including the months no lender reports. The simulation is
              built with the model's own structure, so this verifies the code, not the model.
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
            Facts read straight off the filings, on the {fmtInt(ins.shared_cells)} occasions when more than one lender reported the
            same loan on the same date.
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
          {lag && Number.isFinite(lag.follow) && (
            <Figure
              title="Late, or a trend"
              note={`After the other holders move a loan by a point or more in a quarter and a lender does not, ${fmtPct(lag.follow)} of their move reaches that lender's mark the next quarter, and the other holders move a further ${fmtPct(lag.continuation)} the same way (${fmtInt(lag.n)} cases). Only the difference is unambiguously a late lender; the rest is indistinguishable from a trend in value. Lateness figures on the Lenders page are therefore upper bounds.`}
            >
              <Columns
                categories={["The lender", "The other holders"]}
                series={[{ label: "Share of the earlier move, next quarter", color: INFERRED, values: [lag.follow * 100, lag.continuation * 100] }]}
                format={(v) => `${v.toFixed(0)}%`}
              />
            </Figure>
          )}
        </div>
        {synthetic && (
          <p className="note" style={{ marginTop: 16 }}>
            In the simulated market lenders' errors are symmetric and unrelated to credit quality, so the average mark should
            lead and disagreement should warn of nothing. That is what these show. On real filings the same charts are open
            questions.
          </p>
        )}
      </section>
    </div>
  );
}
