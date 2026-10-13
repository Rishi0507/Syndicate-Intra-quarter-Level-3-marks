// The market as a map: one tile for each loan, grouped by sector, sized by
// how much is lent and coloured by what the loan is estimated to be worth.
// It is the screen a traded market has and this one never did.

import { useEffect, useMemo, useRef, useState } from "react";
import { pathAt, type Universe } from "../lib/data";
import { priceRgb } from "../lib/color";
import { fmtPrice, fmtSize, sectorLabel } from "../lib/format";
import { bandHalfWidth } from "../lib/loan";
import { useStore } from "../state";
import { Tip, useWidth } from "./base";

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Tile extends Rect {
  loan: number;
  price: number;
  sd: number;
}

interface Block extends Rect {
  sector: number;
  tiles: Tile[];
}

/**
 * Lays weights, largest first, into a rectangle so that each gets an area in
 * proportion and stays as near square as it can (Bruls, Huizing and van Wijk).
 */
function squarify(weights: number[], rect: Rect): Rect[] {
  const total = weights.reduce((a, b) => a + b, 0);
  const out: Rect[] = new Array(weights.length);
  if (!(total > 0) || rect.w <= 0 || rect.h <= 0) {
    return out.fill({ x: rect.x, y: rect.y, w: 0, h: 0 });
  }
  const scale = (rect.w * rect.h) / total;
  let { x, y, w, h } = rect;
  let i = 0;
  while (i < weights.length) {
    const side = Math.min(w, h);
    let sum = 0;
    let lo = Infinity;
    let hi = 0;
    let best = Infinity;
    let j = i;
    while (j < weights.length) {
      const a = weights[j] * scale;
      const s = sum + a;
      const worst = Math.max((side * side * Math.max(hi, a)) / (s * s), (s * s) / (side * side * Math.min(lo, a)));
      if (worst > best) {
        break;
      }
      sum = s;
      lo = Math.min(lo, a);
      hi = Math.max(hi, a);
      best = worst;
      j++;
    }
    const thick = sum / side;
    let off = 0;
    for (let k = i; k < j; k++) {
      const len = (weights[k] * scale) / thick;
      out[k] = w >= h ? { x, y: y + off, w: thick, h: len } : { x: x + off, y, w: len, h: thick };
      off += len;
    }
    if (w >= h) {
      x += thick;
      w -= thick;
    } else {
      y += thick;
      h -= thick;
    }
    i = j;
  }
  return out;
}
