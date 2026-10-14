import { lazy, Suspense, useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { BRAND } from "./brand";
import { Engraving } from "./charts/Engraving";
import { loadUniverse, type Universe } from "./lib/data";
import { fmtMonth } from "./lib/format";
import { Kernel } from "./lib/kernel";
import { followAddress } from "./route";
import { setState, subscribe, useStore, type View } from "./state";
import { toggleTheme } from "./theme";
import { Evidence } from "./views/Evidence";
import { Lenders } from "./views/Lenders";
import { Market } from "./views/Market";

// The method page brings the maths typesetter with it and the desk brings
// the 3D library; each is loaded when asked for.
const Method = lazy(() => import("./views/Method").then((m) => ({ default: m.Method })));
const Desk = lazy(() => import("./views/Desk").then((m) => ({ default: m.Desk })));

const TABS: { id: View; label: string }[] = [
  { id: "market", label: "Market" },
  { id: "lenders", label: "Lenders" },
  { id: "desk", label: "Desk" },
  { id: "evidence", label: "Evidence" },
  { id: "method", label: "Method" },
];

/** How long the outgoing view takes to leave, and the incoming one to settle. Matched in the stylesheet. */
const LEAVE_MS = 170;
const ARRIVE_MS = 640;

const order = (v: View) => TABS.findIndex((t) => t.id === v);

interface Loaded {
  u: Universe;
  kernel: Kernel;
}

/**
 * The tabs, with one pill that slides to whichever is current. Its position
 * is measured from the buttons, so it follows the text as fonts load and the
 * window resizes.
 */
function Tabs({ view }: { view: View }) {
  const row = useRef<HTMLDivElement>(null);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const [pill, setPill] = useState<{ x: number; w: number } | null>(null);
  // The pill appears in place; only later moves are animated.
  const [settled, setSettled] = useState(false);

  useLayoutEffect(() => {
    const measure = () => {
      const b = buttons.current[order(view)];
      if (b) {
        setPill({ x: b.offsetLeft, w: b.offsetWidth });
      }
    };
    measure();
    const ro = new ResizeObserver(measure);
    if (row.current) {
      ro.observe(row.current);
    }
    void document.fonts?.ready.then(measure);
    buttons.current[order(view)]?.scrollIntoView({ block: "nearest", inline: "nearest" });
    return () => ro.disconnect();
  }, [view]);

  useEffect(() => {
    if (pill && !settled) {
      const id = requestAnimationFrame(() => setSettled(true));
      return () => cancelAnimationFrame(id);
    }
  }, [pill, settled]);

  return (
    <div className="tabs" ref={row}>
      {pill && <span className="tab-pill" style={{ width: pill.w, transform: `translateX(${pill.x}px)`, transition: settled ? undefined : "none" }} />}
      {TABS.map((t, i) => (
        <button
          key={t.id}
          ref={(el) => {
            buttons.current[i] = el;
          }}
          className="tab"
          aria-current={view === t.id ? "page" : undefined}
          onClick={() => setState({ view: t.id })}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}

/**
 * The latest day on which most of the market is still reported. Public
 * spreads are published daily and filings arrive months late, so the last
 * day of the grid shows only the few lenders that have reported since.
 */
function latestFullDay(u: Universe): number {
  const n = u.meta.days.length;
  const change = new Int32Array(n + 1);
  for (let i = 0; i < u.loans.start.length; i++) {
    change[u.loans.start[i]] += 1;
    change[u.loans.last[i] + 1] -= 1;
  }
  const live = new Int32Array(n);
  let peak = 0;
  for (let t = 0, c = 0; t < n; t++) {
    c += change[t];
    live[t] = c;
    peak = Math.max(peak, c);
  }
  for (let t = n - 1; t >= 0; t--) {
    if (live[t] >= 0.6 * peak) {
      return t;
    }
  }
  return n - 1;
}

export function App() {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [error, setError] = useState<string | null>(null);
  const view = useStore((s) => s.view);
  const theme = useStore((s) => s.theme);

  // The view on screen trails the one asked for by the time it takes the old
  // one to leave. The tab pill follows the request at once.
  const [shown, setShown] = useState<View>(view);
  const [phase, setPhase] = useState<"idle" | "out" | "in">("idle");
  const [dir, setDir] = useState(1);

  useEffect(() => {
    if (view === shown) {
      // Asked to come back before the old view had finished leaving.
      setPhase((p) => (p === "out" ? "in" : p));
      return;
    }
    setDir(order(view) > order(shown) ? 1 : -1);
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setShown(view);
      setPhase("idle");
      return;
    }
    setPhase("out");
    const id = window.setTimeout(() => {
      setShown(view);
      setPhase("in");
    }, LEAVE_MS);
    return () => window.clearTimeout(id);
  }, [view, shown]);

  useEffect(() => {
    if (phase !== "in") {
      return;
    }
    const id = window.setTimeout(() => setPhase("idle"), ARRIVE_MS);
    return () => window.clearTimeout(id);
  }, [phase, shown]);

  useEffect(() => {
    let live = true;
    Promise.all([loadUniverse(), Kernel.load()])
      .then(([u, kernel]) => {
        if (live) {
          setState({ day: latestFullDay(u) });
          setLoaded({ u, kernel });
        }
      })
      .catch((e: unknown) => live && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      live = false;
    };
  }, []);

  // Once the data is in, let the address choose the view and the loan.
  useEffect(() => {
    if (!loaded) {
      return;
    }
    return followAddress(loaded.u.loans.name.length, subscribe);
  }, [loaded]);

  if (error) {
    return (
      <>
        <Engraving />
        <div className="loading">
          <p>
            The data could not be loaded: {error}. Run the pipeline to produce <code>web/public/data</code> and the kernel module,
            then reload.
          </p>
        </div>
      </>
    );
  }
  if (!loaded) {
    return (
      <>
        <Engraving />
        <div className="loading">
          <p>Loading the market</p>
        </div>
      </>
    );
  }
  const { u, kernel } = loaded;
  const synthetic = u.meta.source === "synthetic";
  const light = theme === "light";

  return (
    <>
      <Engraving />
      <div className="shell">
        <nav className="nav glass" aria-label="Sections">
          <button className="wordmark" onClick={() => setState({ view: "market" })} aria-label={`${BRAND}, go to the market`}>
            {/* A mark with its error bar. */}
            <svg viewBox="0 0 32 32" aria-hidden="true">
              <path d="M16 4v24M10 4h12M10 28h12" fill="none" stroke="var(--inferred)" strokeWidth="2.4" strokeLinecap="round" />
              <circle cx="16" cy="16" r="5" fill="var(--ink-1)" />
            </svg>
            <span>{BRAND.toUpperCase()}</span>
          </button>
          <Tabs view={view} />
          <div className="nav-end">
            <span
              className={`source${synthetic ? "" : " source--real"}`}
              title={synthetic ? "A simulated market with a known answer. No real company, lender or mark." : "SEC BDC data sets, as filed."}
            >
              <i />
              {synthetic ? "Simulated market" : "SEC filings"}
              <b style={{ fontWeight: 400, color: "var(--ink-3)" }}>to {fmtMonth(u.meta.days[u.meta.days.length - 1])}</b>
            </span>
            <button className="icon-btn theme-btn" onClick={toggleTheme} aria-label={light ? "Switch to dark mode" : "Switch to light mode"} title={light ? "Dark mode" : "Light mode"}>
              {light ? (
                <svg viewBox="0 0 16 16" aria-hidden="true">
                  <path d="M13.2 10.1A5.6 5.6 0 0 1 5.9 2.8a.5.5 0 0 0-.66-.62 6.6 6.6 0 1 0 8.58 8.58.5.5 0 0 0-.62-.66Z" />
                </svg>
              ) : (
                <svg viewBox="0 0 16 16" aria-hidden="true">
                  <circle cx="8" cy="8" r="3.1" />
                  <path
                    d="M8 .9v1.8M8 13.3v1.8M.9 8h1.8M13.3 8h1.8M2.98 2.98l1.27 1.27M11.75 11.75l1.27 1.27M2.98 13.02l1.27-1.27M11.75 4.25l1.27-1.27"
                    fill="none"
                    stroke="var(--ink-1)"
                    strokeWidth="1.4"
                    strokeLinecap="round"
                  />
                </svg>
              )}
            </button>
          </div>
        </nav>
        <main key={shown} className="view view--scroll" data-phase={phase} style={{ "--dir": dir } as CSSProperties}>
          {shown === "market" && <Market u={u} kernel={kernel} />}
          {shown === "lenders" && <Lenders u={u} />}
          {shown === "evidence" && <Evidence u={u} />}
          <Suspense fallback={<div className="page note">Loading</div>}>
            {shown === "desk" && <Desk kernel={kernel} />}
            {shown === "method" && <Method u={u} />}
          </Suspense>
        </main>
      </div>
    </>
  );
}
