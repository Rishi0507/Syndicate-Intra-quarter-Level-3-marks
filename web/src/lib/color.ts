// Price as a colour.
//
// One hue, as a sequential scale should be. A loan at par is the colour of
// the page and draws no attention; the further below par it is marked, the
// deeper the red. The stops live with the theme, because canvases need them.

import { getState } from "../state";
import { PALETTES } from "../theme";

/** RGB in 0..1 for a price in points, in the current theme. */
export function priceRgb(points: number): [number, number, number] {
  const stops = PALETTES[getState().theme].price;
  if (points <= stops[0][0]) {
    return stops[0][1].map((c) => c / 255) as [number, number, number];
  }
  for (let i = 1; i < stops.length; i++) {
    const [p1, c1] = stops[i];
    if (points <= p1) {
      const [p0, c0] = stops[i - 1];
      const t = (points - p0) / (p1 - p0);
      return [0, 1, 2].map((k) => (c0[k] + t * (c1[k] - c0[k])) / 255) as [number, number, number];
    }
  }
  return stops[stops.length - 1][1].map((c) => c / 255) as [number, number, number];
}

export function priceCss(points: number): string {
  const [r, g, b] = priceRgb(points);
  return `rgb(${Math.round(r * 255)} ${Math.round(g * 255)} ${Math.round(b * 255)})`;
}

/** The price scale as a CSS gradient, lowest price on the left. */
export function priceGradient(): string {
  const stops = PALETTES[getState().theme].price;
  const [lo, hi] = [stops[0][0], stops[stops.length - 1][0]];
  return `linear-gradient(90deg, ${stops.map(([p, c]) => `rgb(${c.join(" ")}) ${(((p - lo) / (hi - lo)) * 100).toFixed(1)}%`).join(", ")})`;
}

// Chart colours are CSS custom properties, so every chart follows the theme
// without being told. What was observed and what was inferred keep their
// meaning in both: marks in the page's ink, the model in gilt.
export const SERIES = ["var(--series-1)", "var(--series-2)", "var(--series-3)"] as const;
export const RUNGS = ["var(--rung-0)", "var(--rung-1)", "var(--rung-2)", "var(--rung-3)", "var(--rung-4)"] as const;
export const INFERRED = "var(--inferred)";
export const OBSERVED = "var(--observed)";
export const MUTED = "var(--ink-3)";
