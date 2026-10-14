// Keeps the address bar in step with what is on screen, so a view or a loan
// can be linked to: #/desk, #/evidence, #/loan/412.

import { getState, setState, type State, type View } from "./state";

const VIEWS: View[] = ["market", "lenders", "desk", "evidence", "method"];

function parse(hash: string, loans: number): Partial<State> | null {
  const parts = hash.replace(/^#\/?/, "").split("/");
  if (parts[0] === "loan") {
    const loan = Number(parts[1]);
    return Number.isInteger(loan) && loan >= 0 && loan < loans ? { view: "market", selected: loan } : null;
  }
  return VIEWS.includes(parts[0] as View) ? { view: parts[0] as View } : null;
}

function format(s: State): string {
  if (s.view === "market") {
    return s.selected != null ? `#/loan/${s.selected}` : "#/market";
  }
  return `#/${s.view}`;
}

/** Applies the current address, then follows it. Returns a function that stops following. */
export function followAddress(loans: number, subscribe: (l: () => void) => () => void): () => void {
  const read = () => {
    const patch = parse(window.location.hash, loans);
    if (patch) {
      setState(patch);
    }
  };
  read();
  const stop = subscribe(() => {
    const want = format(getState());
    if (window.location.hash !== want) {
      window.history.replaceState(null, "", want);
    }
  });
  window.addEventListener("hashchange", read);
  return () => {
    stop();
    window.removeEventListener("hashchange", read);
  };
}
