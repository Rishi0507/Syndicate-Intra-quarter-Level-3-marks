import { useMemo, useState } from "react";
import { Figure } from "../charts/base";
import { Scatter } from "../charts/small";
import { BRAND } from "../brand";
import { booksAt, type BookNow } from "../lib/books";
import type { Family, Universe } from "../lib/data";
import { INFERRED, MUTED } from "../lib/color";
import { fmtDay, fmtInt, fmtPct, fmtSigned, fmtSize } from "../lib/format";
import { useStore } from "../state";

type Column = "name" | "live" | "book" | "drift" | "speed" | "bias" | "noise" | "overlap";

const COLUMNS: { id: Column; label: string; numeric: boolean }[] = [
  { id: "name", label: "Lender", numeric: false },
  { id: "live", label: "Loans", numeric: true },
  { id: "book", label: "Debt book", numeric: true },
  { id: "drift", label: "Since its last filing", numeric: true },
  { id: "speed", label: "Same-quarter share of news", numeric: true },
  { id: "bias", label: "Marks against peers", numeric: true },
  { id: "noise", label: "Scatter, loan by loan", numeric: true },
  { id: "overlap", label: "Shared marks", numeric: true },
];

const WELL_OBSERVED = 30;

function value(f: Family, b: BookNow | undefined, c: Column): number | string {
  switch (c) {
    case "name":
      return f.name;
    case "live":
      return f.live;
    case "book":
      return b?.principal ?? 0;
    case "drift":
      return Number.isFinite(b?.drift) ? b!.drift : -Infinity;
    case "speed":
      return 1 - f.phi;
    case "bias":
      return f.bias;
    case "noise":
      return f.noise;
    case "overlap":
      return f.overlap;
  }
}

function Meter({ share, centred = false, color = INFERRED }: { share: number; centred?: boolean; color?: string }) {
  const w = 72;
  const s = Math.max(-1, Math.min(1, Number.isFinite(share) ? share : 0));
  const x = centred ? w / 2 + Math.min(0, s) * (w / 2) : 0;
  const len = Math.abs(s) * (centred ? w / 2 : w);
  return (
    <svg width={w} height={10} style={{ verticalAlign: "middle", marginLeft: 10 }} aria-hidden="true">
      <rect x={0} y={4} width={w} height={2} rx={1} fill="var(--grid)" />
      {centred && <rect x={w / 2 - 0.5} y={1} width={1} height={8} fill="var(--axis)" />}
      <rect x={x} y={2} width={Math.max(1.5, len)} height={6} rx={2} fill={color} />
    </svg>
  );
}

export function Lenders({ u }: { u: Universe }) {
  const day = useStore((s) => s.day);
  const [sort, setSort] = useState<{ by: Column; dir: 1 | -1 }>({ by: "live", dir: -1 });
  const [all, setAll] = useState(false);
  const books = useMemo(() => booksAt(u, day), [u, day]);
  const rows = useMemo(() => {
    const out = u.meta.families.map((f, i) => ({ f, i, b: books[i] })).filter((r) => all || r.f.overlap >= WELL_OBSERVED);
    out.sort((a, b) => {
      const va = value(a.f, a.b, sort.by);
      const vb = value(b.f, b.b, sort.by);
      const cmp = typeof va === "string" ? va.localeCompare(vb as string) : (va as number) - (vb as number);
      return cmp * sort.dir;
    });
    return out;
  }, [u, books, sort, all]);

  const shown = u.meta.families.filter((f) => all || f.overlap >= WELL_OBSERVED);
  const maxBias = Math.max(...shown.map((f) => Math.abs(f.bias)), 0.01);
  const maxNoise = Math.max(...shown.map((f) => f.noise), 0.01);
  const maxDrift = Math.max(...books.filter((b) => b.loans >= 5).map((b) => Math.abs(b.drift)).filter(Number.isFinite), 0.05);
  const measured = u.meta.families.filter((f) => f.moves >= 20);
  const slowest = measured.length ? Math.min(...measured.map((f) => 1 - f.phi)) : NaN;
  const hidden = u.meta.families.length - shown.length;
  const bk = u.meta.eval.books;
  const lim = useMemo(() => {
    if (!bk || bk.points.length === 0) {
      return 2;
    }
    const mags = bk.points.map((p) => Math.max(Math.abs(p[0]), Math.abs(p[1]))).sort((a, b) => a - b);
    const p95 = mags[Math.floor(0.95 * (mags.length - 1))];
    return Math.max(1, Math.min(4, Math.ceil(p95 * 1.4 * 2) / 2));
  }, [bk]);

  return (
    <div className="page">
      <section className="glass pane">
        <header>
          <h2>How each lender marks</h2>
          <p className="lede">
            Lenders that hold the same loan give the model something to compare. From those comparisons it estimates three habits for
            each manager: how much of the news on a troubled loan reaches its mark in the same quarter, where its marks sit against
            other holders', and how far they scatter from one loan to the next. It also reprices each lender's debt book every day
            from what other holders have filed since.
          </p>
          {!u.meta.lenders_named && (
            <p className="note" style={{ marginTop: 10 }}>
              Lenders are identified by code, numbered by the loans they hold. Vehicles run by one manager are one lender, because one
              committee values the loan for all of them. The figures describe how each lender's marks relate to other holders' marks
              of the same loans. They are statistical estimates from public filings, uncertain for lenders that share few loans, and
              they are not assessments of any manager's valuation policies or controls.
            </p>
          )}
        </header>
        <div className="stats" style={{ marginBottom: 22 }}>
          {bk && (
            <div className="stat inset">
              <span>Each lender's quarter, called the day before it files</span>
              <b>{fmtPct(bk.skill)}</b>
              <em>
                of the squared change in its reported debt book anticipated, {fmtInt(bk.n)} lender-quarters
              </em>
            </div>
          )}
          {bk && (
            <div className="stat inset">
              <span>Direction of the change</span>
              <b>
                {fmtInt(bk.direction_right)} of {fmtInt(bk.moved)}
              </b>
              <em>
                books that moved half a point or more were called the right way
                {bk.majority_right != null
                  ? `; ${fmtInt(bk.fell)} of them fell, so always calling a ${bk.fell * 2 >= bk.moved ? "fall" : "rise"} gets ${fmtInt(bk.majority_right)}`
                  : ""}
              </em>
            </div>
          )}
          <div className="stat inset">
            <span>Of a disagreement between lenders</span>
            <b>{fmtPct(u.meta.fit.carry)}</b>
            <em>is still there a quarter later</em>
          </div>
          <div className="stat inset">
            <span>Lowest same-quarter share, well-observed lenders</span>
            <b>{fmtPct(slowest)}</b>
            <em>of news on a troubled loan marked in the quarter it arrives, at least</em>
          </div>
        </div>

        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", flexWrap: "wrap", gap: 10, marginBottom: 6 }}>
          <p className="note">
            Debt books on {fmtDay(u.meta.days[day])}: principal of the loans in each lender's latest filing, and how far the
            estimate of those loans has moved since that filing was published, in points, weighted by principal.
          </p>
          <label className="note" style={{ display: "flex", gap: 8, alignItems: "center", cursor: "pointer", whiteSpace: "nowrap" }}>
            <input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} />
            Show {fmtInt(hidden)} lenders with few shared marks
          </label>
        </div>
        <div style={{ overflowX: "auto" }}>
          <table>
            <thead>
              <tr>
                {COLUMNS.map((c) => (
                  <th key={c.id} className={c.numeric ? "r" : undefined} aria-sort={sort.by === c.id ? (sort.dir === 1 ? "ascending" : "descending") : undefined}>
                    <button onClick={() => setSort((s) => ({ by: c.id, dir: s.by === c.id ? (-s.dir as 1 | -1) : c.numeric ? -1 : 1 }))}>
                      {c.label}
                      {sort.by === c.id ? (sort.dir === 1 ? " ↑" : " ↓") : ""}
                    </button>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map(({ f, i, b }) => (
                <tr key={i}>
                  <td>{f.name}</td>
                  <td className="r">{fmtInt(f.live)}</td>
                  <td className="r">{b && b.loans > 0 ? fmtSize(b.principal) : "–"}</td>
                  <td className="r" style={{ whiteSpace: "nowrap" }}>
                    {b && b.loans >= 5 ? fmtSigned(b.drift) : "–"}
                    <Meter share={b && b.loans >= 5 ? b.drift / maxDrift : 0} centred color={b && b.drift < 0 ? "var(--down)" : INFERRED} />
                  </td>
                  <td className="r" style={{ whiteSpace: "nowrap" }}>
                    {fmtPct(1 - f.phi)}
                    <Meter share={1 - f.phi} color={f.moves >= 20 ? INFERRED : MUTED} />
                  </td>
                  <td className="r" style={{ whiteSpace: "nowrap" }}>
                    {fmtSigned(f.bias)}
                    <Meter share={f.bias / maxBias} centred color={f.overlap >= WELL_OBSERVED ? INFERRED : MUTED} />
                  </td>
                  <td className="r" style={{ whiteSpace: "nowrap" }}>
                    {f.noise.toFixed(2)}
                    <Meter share={f.noise / maxNoise} color={f.overlap >= WELL_OBSERVED ? INFERRED : MUTED} />
                  </td>
                  <td className="r">{fmtInt(f.overlap)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="note" style={{ marginTop: 16 }}>
          Reading the columns. Since its last filing is the principal-weighted move in the estimate of a lender's loans between the
          day its latest filing was published and the day shown; it is where the lender's own marks would go if it followed the
          other holders, before its own distance from them. The same-quarter share of news is the share of a change in a troubled
          loan's value that reaches the lender's mark in the quarter it happens, measured from how much of what the other holders
          reported last quarter turns up in this lender's change this quarter. Part of that can be a trend in the loan's value
          rather than a late mark, so the share is a lower bound. Marks against peers is the typical difference, in points, between
          a lender's mark and other lenders' marks of the same loan on the same date, for performing loans. Scatter is how far its
          mark of one loan typically sits from the others' once that difference is removed, in points; most of it is a standing
          view of the loan and persists. Grey bars rest on few shared marks and are shrunk toward the average lender.
        </p>
      </section>

      {bk && bk.n > 0 && (
        <section className="glass pane">
          <header>
            <h2>Calling a lender's quarter before it files</h2>
            <p className="lede">
              Each point is one lender's continuing debt book in one quarter, with {bk.min_loans} or more loans. Across is the change
              {" "}{BRAND} expected the day before the lender filed, from every mark already public; up is the change the lender
              reported. Both are principal-weighted averages, in points. The more of a book other holders had already marked, the
              more of the change was known in advance.
            </p>
          </header>
          <div className="cols cols--wide">
            <Figure note={`Points on the diagonal were called exactly; points beyond ${lim} points are drawn at the edge. Correlation ${bk.correlation.toFixed(2)}. Amber points are books of which other holders had already marked 40% or more; grey points, less.`}>
              <Scatter
                points={bk.points.map(([actual, model, share]) => ({ x: model, y: actual, color: share >= 0.4 ? INFERRED : MUTED, r: 3 }))}
                limit={lim}
                xLabel={`Expected by ${BRAND}, points`}
                yLabel="Reported change, points"
              />
            </Figure>
            <div style={{ overflowX: "auto" }}>
              <table>
                <thead>
                  <tr>
                    <th>Book already marked by others</th>
                    <th className="r">Books</th>
                    <th className="r">Change anticipated</th>
                    <th className="r">Direction right</th>
                    <th className="r">Base rate</th>
                  </tr>
                </thead>
                <tbody>
                  {bk.by_peer_share.map((r) => (
                    <tr key={r.label}>
                      <td>{r.label}</td>
                      <td className="r">{fmtInt(r.n)}</td>
                      <td className="r">{fmtPct(r.skill)}</td>
                      <td className="r">
                        {fmtInt(r.direction_right)} of {fmtInt(r.moved)}
                      </td>
                      <td className="r">{r.majority_right != null ? fmtInt(r.majority_right) : ""}</td>
                    </tr>
                  ))}
                  <tr>
                    <td>
                      <b>All</b>
                    </td>
                    <td className="r">{fmtInt(bk.n)}</td>
                    <td className="r">{fmtPct(bk.skill)}</td>
                    <td className="r">
                      {fmtInt(bk.direction_right)} of {fmtInt(bk.moved)}
                    </td>
                    <td className="r">{bk.majority_right != null ? fmtInt(bk.majority_right) : ""}</td>
                  </tr>
                </tbody>
              </table>
              <p className="note" style={{ marginTop: 10 }}>
                Squared change anticipated is one less the ratio of squared errors to squared changes, against a forecast of no
                change. Rolling each book forward with public spreads, the pull to par and each lender's own habits, without other
                holders' marks, anticipates {fmtPct(bk.habits_skill)}.
                {bk.rule_skill != null
                  ? ` The simple rule, each loan's last mark moved by the other holders' change and left alone where no one else has filed, anticipates ${fmtPct(bk.rule_skill)} and calls the direction of ${fmtInt(bk.rule_direction_right)} books. Most books fell in these quarters: the base rate is the number of books called right by always calling the commoner direction, so the direction alone says little and the share of the change anticipated is the better guide.`
                  : ""}{" "}
                The typical book moved {bk.mean_abs_change.toFixed(2)} points
                in a quarter and was called to within {bk.mean_abs_error.toFixed(2)}.
              </p>
            </div>
          </div>
        </section>
      )}
    </div>
  );
}
