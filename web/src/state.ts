import { useSyncExternalStore } from "react";
import type { Desk } from "./lib/kernel";
import type { Theme } from "./theme";

export type View = "market" | "lenders" | "evidence" | "method" | "desk";

export interface State {
  view: View;
  theme: Theme;
  day: number;
  selected: number | null;
  desk: Desk;
  printSd: number;
}

export const DEFAULT_DESK: Desk = { arrival: 0.2, decay: 1.5, gamma: 0.05, informed: 0.25, inventory: 0 };

const initialTheme: Theme = typeof document !== "undefined" && document.documentElement.dataset.theme === "dark" ? "dark" : "light";

let state: State = {
  view: "market",
  theme: initialTheme,
  day: 0,
  selected: null,
  desk: DEFAULT_DESK,
  printSd: 0.25,
};

const listeners = new Set<() => void>();

export function getState(): State {
  return state;
}

export function setState(patch: Partial<State> | ((s: State) => Partial<State>)): void {
  const next = typeof patch === "function" ? patch(state) : patch;
  let changed = false;
  for (const k of Object.keys(next) as (keyof State)[]) {
    if (state[k] !== next[k]) {
      changed = true;
      break;
    }
  }
  if (!changed) {
    return;
  }
  state = { ...state, ...next };
  listeners.forEach((l) => l());
}

export function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function useStore<T>(select: (s: State) => T): T {
  return useSyncExternalStore(subscribe, () => select(state));
}
