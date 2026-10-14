import { useMemo, useRef, useState } from "react";
import { BRAND } from "../brand";
import { MarketMap } from "../charts/MarketMap";
import { priceCss, priceGradient } from "../lib/color";
import { pathAt, type Universe } from "../lib/data";
import { classLabel, fmtDay, fmtInt, fmtPct, fmtPrice } from "../lib/format";
import type { Kernel } from "../lib/kernel";
import { setState, useStore } from "../state";
import { LoanSheet } from "./LoanSheet";

const QUARTER = 63;

export function Market({ u, kernel }: { u: Universe; kernel: Kernel }) {
  const day = useStore((s) => s.day);
  const selected = useStore((s) => s.selected);
  useStore((s) => s.theme);
  const sheet = useRef<HTMLDivElement>(null);
  const synthetic = u.meta.source === "synthetic";

  const featured = useMemo(() => {
    let best = 0;
    for (let i = 1; i < u.loans.name.length; i++) {
      const live = day >= u.loans.start[i] && day <= u.loans.last[i];
      const bestLive = day >= u.loans.start[best] && day <= u.loans.last[best];
      const better = u.loans.families[i] !== u.loans.families[best] ? u.loans.families[i] > u.loans.families[best] : (u.loans.size[i] ?? 0) > (u.loans.size[best] ?? 0);
      if (live && (!bestLive || better)) {
        best = i;
      }
    }
    return best;
  }, [u, day]);
  const loan = selected ?? featured;

  const movers = useMemo(() => {
    const out: { loan: number; price: number; change: number }[] = [];
    for (let i = 0; i < u.loans.name.length; i++) {
      if (u.loans.families[i] < 2) {
        continue;
      }
      const now = pathAt(u, i, day);
      const then = pathAt(u, i, day - QUARTER);
      if (now && then) {
        out.push({ loan: i, price: now.price, change: now.price - then.price });
      }
    }
    return out.sort((a, b) => a.change - b.change).slice(0, 5);
  }, [u, day]);

  const choose = (i: number) => {
    setState({ selected: i });
    sheet.current?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  };

  const followers = u.meta.eval.followers[u.meta.eval.followers.length - 1];
  const live = u.meta.counts.live;

  return (
    <div className="page">
      <section className="lead">
        <div className="pitch">
          <h1>A daily price for loans that never trade.</h1>
          <p>
            Private loans are valued four times a year, and the value is published weeks later. {BRAND} estimates what each one is
            worth on every day in between, says how sure it is, and turns that into a bid and an ask.
          </p>
          <Search u={u} onChoose={choose} />
          <div className="facts">
            <button onClick={() => setState({ view: "method" })}>
              <b>{fmtInt(u.meta.counts.loans)}</b>
              <span>loans priced, from {synthetic ? "simulated filings" : "what lenders file with the SEC"}</span>
            </button>
            <button onClick={() => setState({ view: "lenders" })}>
              <b>{fmtInt(u.meta.counts.families)}</b>
              <span>lenders, each with marking habits of its own</span>
            </button>
            <button onClick={() => setState({ view: "evidence" })}>
              <b>{fmtPct(followers.skill)}</b>
              <span>of forecast error removed once a second lender has reported</span>
            </button>
          </div>
          {movers.length > 0 && (
            <div className="movers glass">
              <h3>Marked down most in the last quarter</h3>
              <ol>
                {movers.map((m) => (
                  <li key={m.loan}>
                    <button onClick={() => choose(m.loan)}>
                      <span>
                        {u.loans.name[m.loan]}
                        <br />
                        <small>
                          {u.loans.families[m.loan]} lenders, {classLabel(u.meta.classes[u.loans.class[m.loan]]).toLowerCase()}
                        </small>
                      </span>
                      <span className="px num">
                        <i className="dot" style={{ background: priceCss(m.price) }} />
                        {fmtPrice(m.price, 1)}
                        <br />
                        <small className="chg">{m.change.toFixed(1)} points</small>
                      </span>
                    </button>
                  </li>
                ))}
              </ol>
            </div>
          )}
        </div>
        <div ref={sheet}>
          <LoanSheet u={u} kernel={kernel} loan={loan} />
        </div>
      </section>

      <section className="glass pane">
        <div className="map-head">
          <header>
            <h2>The whole market on {fmtDay(u.meta.days[day])}</h2>
            <p className="lede">
              Each tile is a loan, {fmtInt(live)} of them. The bigger the tile, the more is lent. The redder it is, the further
              below par the loan is priced. Choose one to see its price and the marks behind it.
            </p>
          </header>
          <div className="scale" aria-hidden="true">
            Estimated price, par is 100
            <i style={{ background: priceGradient() }} />
            <div>
              <span>60</span>
              <span>70</span>
              <span>80</span>
              <span>90</span>
              <span>100</span>
            </div>
          </div>
        </div>
        <MarketMap u={u} day={day} selected={loan} onSelect={choose} />
      </section>

      <p className="footer">
        {synthetic
          ? "This is a simulated market with a known answer, written in the SEC's file format to test the method. No real company, lender or mark appears in it."
          : "Prices are statistical estimates made from public filings with the SEC. They are not quotes, and nothing here is investment advice. Names are read by rule from free text, and some will be wrong."}
      </p>
    </div>
  );
}

function Search({ u, onChoose }: { u: Universe; onChoose: (loan: number) => void }) {
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const [cursor, setCursor] = useState(0);

  const results = useMemo(() => {
    const needle = q.trim().toLowerCase();
    if (needle.length < 2) {
      return [];
    }
    const starts: number[] = [];
    const contains: number[] = [];
    for (let i = 0; i < u.loans.name.length && starts.length < 8; i++) {
      const at = u.loans.name[i].toLowerCase().indexOf(needle);
      if (at === 0) {
        starts.push(i);
      } else if (at > 0 && contains.length < 8) {
        contains.push(i);
      }
    }
    return [...starts, ...contains].slice(0, 8);
  }, [u, q]);

  const choose = (loan: number) => {
    onChoose(loan);
    setOpen(false);
    setQ("");
  };

  return (
    <div className="search glass">
      <svg viewBox="0 0 16 16" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true">
        <circle cx="7" cy="7" r="4.6" />
        <path d="M10.6 10.6 14 14" />
      </svg>
      <input
        type="search"
        placeholder="Find a borrower"
        aria-label="Find a borrower"
        value={q}
        onChange={(e) => {
          setQ(e.target.value);
          setOpen(true);
          setCursor(0);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => window.setTimeout(() => setOpen(false), 150)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            setCursor((c) => Math.min(results.length - 1, c + 1));
            e.preventDefault();
          } else if (e.key === "ArrowUp") {
            setCursor((c) => Math.max(0, c - 1));
            e.preventDefault();
          } else if (e.key === "Enter" && results[cursor] != null) {
            choose(results[cursor]);
          } else if (e.key === "Escape") {
            setOpen(false);
          }
        }}
      />
      {open && q.trim().length >= 2 && (
        <ul className="results glass glass--solid" role="listbox">
          {results.length === 0 && <li style={{ padding: "8px 10px", color: "var(--ink-3)", fontSize: "var(--fs-1)" }}>No borrower matches “{q.trim()}”.</li>}
          {results.map((i, k) => (
            <li key={i}>
              <button role="option" aria-selected={k === cursor} onPointerDown={(e) => e.preventDefault()} onClick={() => choose(i)}>
                <span>
                  {u.loans.name[i]}
                  <br />
                  <small>{classLabel(u.meta.classes[u.loans.class[i]])}</small>
                </span>
                {u.loans.now_mean[i] != null && (
                  <span className="num">
                    <i className="dot" style={{ background: priceCss(u.loans.now_mean[i]!) }} />
                    {fmtPrice(u.loans.now_mean[i]!, 1)}
                  </span>
                )}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
