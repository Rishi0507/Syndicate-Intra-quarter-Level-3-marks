import { getState, setState } from "./state";

export type Theme = "dark" | "light";

const KEY = "syndicate-theme";

export interface CanvasPalette {
  price: [number, [number, number, number]][];
  ink: number;
  frame: number;
  frameOpacity: number;
  surfaceOpacity: number;
  lit: string;
  dark: string;
  tipping: string;
}

export const PALETTES: Record<Theme, CanvasPalette> = {
  light: {
    price: [
      [60, [0x7a, 0x14, 0x20]],
      [75, [0xa8, 0x3a, 0x3a]],
      [85, [0xc9, 0x6a, 0x5a]],
      [92, [0xdc, 0x9a, 0x85]],
      [97, [0xe2, 0xc6, 0xb8]],
      [100, [0xcd, 0xd6, 0xcf]],
    ],
    ink: 0x0f1a15,
    frame: 0x3b4942,
    frameOpacity: 0.22,
    surfaceOpacity: 0.16,
    lit: "#b07200",
    dark: "#2a78d6",
    tipping: "#b3261e",
  },
  dark: {
    price: [
      [60, [0xef, 0x5a, 0x4a]],
      [75, [0xcc, 0x4a, 0x3f]],
      [85, [0xa8, 0x43, 0x3a]],
      [92, [0x7a, 0x3d, 0x33]],
      [97, [0x4a, 0x3a, 0x30]],
      [100, [0x26, 0x34, 0x2e]],
    ],
    ink: 0xedf1ec,
    frame: 0xb1bcb4,
    frameOpacity: 0.16,
    surfaceOpacity: 0.2,
    lit: "#c98500",
    dark: "#256abf",
    tipping: "#ef6a5c",
  },
};

export function savedTheme(): Theme {
  try {
    return window.localStorage.getItem(KEY) === "dark" ? "dark" : "light";
  } catch {
    return "light";
  }
}

function apply(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  document.querySelector('meta[name="theme-color"]')?.setAttribute("content", theme === "light" ? "#e7ebe5" : "#0c1310");
  document.querySelector('meta[name="color-scheme"]')?.setAttribute("content", theme);
  setState({ theme });
}

type WithTransitions = Document & { startViewTransition?: (update: () => void) => unknown };

export function setTheme(theme: Theme): void {
  try {
    window.localStorage.setItem(KEY, theme);
  } catch {
  }
  const doc = document as WithTransitions;
  const calm = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (doc.startViewTransition && !calm) {
    doc.startViewTransition(() => apply(theme));
  } else {
    apply(theme);
  }
}

export function toggleTheme(): void {
  setTheme(getState().theme === "dark" ? "light" : "dark");
}
