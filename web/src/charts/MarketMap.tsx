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

const weightOf = (size: number | null) => Math.max(size ?? 0.5, 0.5);

export function MarketMap({ u, day, selected, onSelect }: { u: Universe; day: number; selected: number | null; onSelect: (loan: number) => void }) {
  const theme = useStore((s) => s.theme);
  const [ref, width] = useWidth<HTMLDivElement>();
  const canvas = useRef<HTMLCanvasElement>(null);
  const [height, setHeight] = useState(0);
  const [hover, setHover] = useState<{ tile: Tile; x: number; y: number } | null>(null);

  useEffect(() => {
    setHeight(ref.current?.clientHeight ?? 0);
  }, [ref, width]);

  const live = useMemo(() => {
    const bySector = new Map<number, { loan: number; price: number; sd: number; weight: number }[]>();
    for (let i = 0; i < u.loans.name.length; i++) {
      const p = pathAt(u, i, day);
      if (!p) {
        continue;
      }
      const list = bySector.get(u.loans.sector[i]) ?? [];
      list.push({ loan: i, price: p.price, sd: p.sd, weight: weightOf(u.loans.size[i]) });
      bySector.set(u.loans.sector[i], list);
    }
    const sectors = [...bySector.entries()].map(([sector, list]) => {
      list.sort((a, b) => b.weight - a.weight);
      return { sector, list, weight: list.reduce((a, b) => a + b.weight, 0) };
    });
    sectors.sort((a, b) => b.weight - a.weight);
    return sectors;
  }, [u, day]);

  const blocks: Block[] = useMemo(() => {
    if (width <= 0 || height <= 0) {
      return [];
    }
    const outer = squarify(
      live.map((s) => s.weight),
      { x: 0, y: 0, w: width, h: height },
    );
    return live.map((s, k) => {
      const r = outer[k];
      const inner = { x: r.x + 2, y: r.y + 2, w: Math.max(0, r.w - 4), h: Math.max(0, r.h - 4) };
      const rects = squarify(
        s.list.map((l) => l.weight),
        inner,
      );
      return { ...r, sector: s.sector, tiles: s.list.map((l, i) => ({ ...rects[i], loan: l.loan, price: l.price, sd: l.sd })) };
    });
  }, [live, width, height]);

  useEffect(() => {
    const c = canvas.current;
    if (!c || blocks.length === 0) {
      return;
    }
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    c.width = Math.round(width * dpr);
    c.height = Math.round(height * dpr);
    const g = c.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, width, height);

    const css = getComputedStyle(document.documentElement);
    const ink = css.getPropertyValue("--ink-1").trim();
    const accent = css.getPropertyValue("--inferred").trim();
    const plate = css.getPropertyValue("--glass-solid").trim();
    const family = css.getPropertyValue("--font").trim();

    for (const b of blocks) {
      for (const t of b.tiles) {
        const [r, gr, bl] = priceRgb(t.price);
        g.fillStyle = `rgb(${Math.round(r * 255)} ${Math.round(gr * 255)} ${Math.round(bl * 255)})`;
        const gap = t.w > 3 && t.h > 3 ? 0.5 : 0;
        g.fillRect(t.x + gap, t.y + gap, t.w - 2 * gap, t.h - 2 * gap);
        if (t.w > 92 && t.h > 38) {
          const lum = 0.2126 * r + 0.7152 * gr + 0.0722 * bl;
          g.fillStyle = lum > 0.42 ? "#0f1a15" : "#f1f4ef";
          g.font = `500 12px ${family}`;
          let name = u.loans.name[t.loan];
          while (name.length > 3 && g.measureText(name).width > t.w - 14) {
            name = name.slice(0, -2);
          }
          if (name !== u.loans.name[t.loan]) {
            name = name.trimEnd() + "…";
          }
          g.fillText(name, t.x + 7, t.y + 17);
          g.font = `400 11.5px ${family}`;
          g.globalAlpha = 0.8;
          g.fillText(fmtPrice(t.price, 1), t.x + 7, t.y + 32);
          g.globalAlpha = 1;
        }
      }
    }
    g.font = `600 11.5px ${family}`;
    for (const b of blocks) {
      let label = sectorLabel(u.meta.sectors[b.sector]);
      for (const shorter of [label.split(" and ")[0], label.split(" ")[0]]) {
        if (b.w < g.measureText(label).width + 26) {
          label = shorter;
        }
      }
      const tw = g.measureText(label).width;
      if (b.w < tw + 26 || b.h < 30) {
        continue;
      }
      g.fillStyle = plate;
      g.beginPath();
      g.roundRect(b.x + 6, b.y + b.h - 26, tw + 14, 20, 6);
      g.fill();
      g.fillStyle = ink;
      g.fillText(label, b.x + 13, b.y + b.h - 12);
    }
    const ring = (t: Tile, color: string, lineWidth: number) => {
      g.strokeStyle = color;
      g.lineWidth = lineWidth;
      g.strokeRect(t.x + lineWidth / 2, t.y + lineWidth / 2, Math.max(1, t.w - lineWidth), Math.max(1, t.h - lineWidth));
    };
    for (const b of blocks) {
      for (const t of b.tiles) {
        if (t.loan === selected) {
          ring(t, accent, 2.5);
        }
      }
    }
    if (hover) {
      ring(hover.tile, ink, 1.5);
    }
  }, [blocks, width, height, selected, hover, theme, u]);

  const find = (x: number, y: number): Tile | null => {
    const b = blocks.find((k) => x >= k.x && x < k.x + k.w && y >= k.y && y < k.y + k.h);
    return b?.tiles.find((t) => x >= t.x && x < t.x + t.w && y >= t.y && y < t.y + t.h) ?? null;
  };
  const at = (e: React.PointerEvent | React.MouseEvent) => {
    const r = e.currentTarget.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };

  const shown = blocks.reduce((n, b) => n + b.tiles.length, 0);
  return (
    <div className="map inset" ref={ref}>
      <canvas
        ref={canvas}
        role="img"
        aria-label={`A map of ${shown} loans grouped by sector. Larger tiles are larger loans; redder tiles are priced further below par. Use the search box to reach any loan by name.`}
        onPointerMove={(e) => {
          const p = at(e);
          const tile = find(p.x, p.y);
          setHover(tile ? { tile, ...p } : null);
        }}
        onPointerLeave={() => setHover(null)}
        onClick={(e) => {
          const p = at(e);
          const tile = find(p.x, p.y);
          if (tile) {
            onSelect(tile.loan);
          }
        }}
      />
      {hover && (
        <Tip x={hover.x} y={hover.y} width={width}>
          <b>{u.loans.name[hover.tile.loan]}</b>
          <br />
          {fmtPrice(hover.tile.price)} ± {fmtPrice(bandHalfWidth(u, hover.tile.price, hover.tile.sd))}
          <br />
          {u.loans.families[hover.tile.loan]} {u.loans.families[hover.tile.loan] === 1 ? "lender" : "lenders"}, {fmtSize(u.loans.size[hover.tile.loan])} held
        </Tip>
      )}
    </div>
  );
}
