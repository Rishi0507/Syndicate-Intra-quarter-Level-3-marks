import { useMemo, useState } from "react";
import type { Family, Universe } from "../lib/data";
import { INFERRED, MUTED } from "../lib/color";
import { fmtInt, fmtPct, fmtSigned } from "../lib/format";

type Column = "name" | "live" | "speed" | "bias" | "noise" | "overlap";

const COLUMNS: { id: Column; label: string; numeric: boolean }[] = [
  { id: "name", label: "Lender", numeric: false },
  { id: "live", label: "Loans", numeric: true },
  { id: "speed", label: "Same-quarter share of news", numeric: true },
  { id: "bias", label: "Marks against peers", numeric: true },
  { id: "noise", label: "Scatter, loan by loan", numeric: true },
  { id: "overlap", label: "Shared marks", numeric: true },
];

function value(f: Family, c: Column): number | string {
  switch (c) {
    case "name":
      return f.name;
    case "live":
      return f.live;
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
  const w = 84;
  const x = centred ? w / 2 + Math.min(0, share) * (w / 2) : 0;
  const len = Math.abs(share) * (centred ? w / 2 : w);
  return (
    <svg width={w} height={10} style={{ verticalAlign: "middle", marginLeft: 10 }} aria-hidden="true">
      <rect x={0} y={4} width={w} height={2} rx={1} fill="var(--grid)" />
      {centred && <rect x={w / 2 - 0.5} y={1} width={1} height={8} fill="var(--axis)" />}
      <rect x={x} y={2} width={Math.max(1.5, len)} height={6} rx={2} fill={color} />
    </svg>
  );
}

export function Lenders({ u }: { u: Universe }) {
  const [sort, setSort] = useState<{ by: Column; dir: 1 | -1 }>({ by: "live", dir: -1 });
  const rows = useMemo(() => {
    const out = u.meta.families.map((f, i) => ({ f, i }));
    out.sort((a, b) => {
      const va = value(a.f, sort.by);
      const vb = value(b.f, sort.by);
      const cmp = typeof va === "string" ? va.localeCompare(vb as string) : (va as number) - (vb as number);
      return cmp * sort.dir;
    });
    return out;
  }, [u, sort]);

  const maxBias = Math.max(...u.meta.families.map((f) => Math.abs(f.bias)), 0.01);
  const maxNoise = Math.max(...u.meta.families.map((f) => f.noise), 0.01);
  const measured = u.meta.families.filter((f) => f.moves >= 20);
  const slowest = measured.length ? Math.min(...measured.map((f) => 1 - f.phi)) : NaN;

  return (
    <div className="page">
      <section className="glass pane">
        <header>
          <h2>How each lender marks</h2>
          <p className="lede">
            Lenders that hold the same loan give the model something to compare. From those comparisons it estimates three habits for
            each manager: how much of the news on a troubled loan reaches its mark in the same quarter, where its marks sit against
            other holders', and how far they scatter from one loan to the next. Vehicles run by one manager are treated as a single
            lender, because one committee values the loan for all of them.
          </p>
          {!u.meta.lenders_named && (
            <p className="note" style={{ marginTop: 10 }}>
              Lenders are identified by code, numbered by the loans they hold. The figures describe how each lender's marks relate to
              other holders' marks of the same loans. They are statistical estimates from public filings, uncertain for lenders that
              share few loans, and they are not assessments of any manager's valuation policies or controls.
            </p>
          )}
        </header>
        <div className="stats" style={{ marginBottom: 22 }}>
          <div className="stat inset">
            <span>Lenders</span>
            <b>{fmtInt(u.meta.counts.families)}</b>
            <em>manager families</em>
          </div>
          <div className="stat inset">
            <span>Loans more than one lender holds</span>
            <b>{fmtInt(u.meta.counts.shared)}</b>
            <em>of {fmtInt(u.meta.counts.borrowers)} borrowers</em>
          </div>
          <div className="stat inset">
            <span>Lowest same-quarter share, well-observed lenders</span>
            <b>{fmtPct(slowest)}</b>
            <em>of news on a troubled loan marked in the quarter it arrives, at least</em>
          </div>
          <div className="stat inset">
            <span>Of a disagreement between lenders</span>
            <b>{fmtPct(u.meta.fit.carry)}</b>
            <em>is still there a quarter later</em>
          </div>
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
              {rows.map(({ f, i }) => (
                <tr key={i}>
                  <td>{f.name}</td>
                  <td className="r">{fmtInt(f.live)}</td>
                  <td className="r" style={{ whiteSpace: "nowrap" }}>
                    {fmtPct(1 - f.phi)}
                    <Meter share={1 - f.phi} color={f.moves >= 20 ? INFERRED : MUTED} />
                  </td>
                  <td className="r" style={{ whiteSpace: "nowrap" }}>
                    {fmtSigned(f.bias)}
                    <Meter share={f.bias / maxBias} centred color={f.overlap >= 30 ? INFERRED : MUTED} />
                  </td>
                  <td className="r" style={{ whiteSpace: "nowrap" }}>
                    {f.noise.toFixed(2)}
                    <Meter share={f.noise / maxNoise} color={f.overlap >= 30 ? INFERRED : MUTED} />
                  </td>
                  <td className="r">{fmtInt(f.overlap)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="note" style={{ marginTop: 16 }}>
          Reading the columns. The same-quarter share of news is the share of a change in a troubled loan's value that reaches the
          lender's mark in the quarter it happens, with the rest arriving a quarter later. It is measured from how much of what the
          other holders of a loan reported last quarter turns up in this lender's change this quarter. Part of that can be a trend
          in the loan's value rather than a late mark, so the share is a lower bound; the Evidence page measures the trend. Marks
          against peers is the typical difference, in points, between a lender's mark and other lenders' marks of the same loan on
          the same date, for performing loans. Scatter is how far its mark of one loan typically sits from the others' once that
          difference is removed, in points; most of it is a standing view of the loan and persists. Grey bars rest on few shared
          marks and are shrunk toward the average lender.
        </p>
      </section>
    </div>
  );
}
