// The ground of the page, engraved the way banknotes and share certificates
// are. Each band is one wave drawn round a circle several times over, every
// copy a fraction of a wavelength on from the last, so the lines cross into
// a braid. Bands of different pitch nest inside one another.

import { useMemo } from "react";

interface Band {
  radius: number;
  depth: number;
  waves: number;
  strands: number;
}

function braid(cx: number, cy: number, b: Band): string[] {
  const steps = Math.max(240, b.waves * 16);
  const paths: string[] = [];
  for (let j = 0; j < b.strands; j++) {
    const phase = (j / b.strands) * 2 * Math.PI;
    let d = "";
    for (let s = 0; s <= steps; s++) {
      const t = (s / steps) * 2 * Math.PI;
      const r = b.radius + b.depth * Math.sin(b.waves * t + phase);
      d += `${s === 0 ? "M" : "L"}${(cx + r * Math.cos(t)).toFixed(1)} ${(cy + r * Math.sin(t)).toFixed(1)}`;
    }
    paths.push(d + "Z");
  }
  return paths;
}

const ROSETTE: Band[] = [
  { radius: 560, depth: 34, waves: 60, strands: 6 },
  { radius: 474, depth: 44, waves: 36, strands: 8 },
  { radius: 366, depth: 52, waves: 24, strands: 8 },
  { radius: 248, depth: 50, waves: 16, strands: 8 },
  { radius: 138, depth: 44, waves: 10, strands: 8 },
];

export function Engraving() {
  const paths = useMemo(
    () => [
      ...ROSETTE.flatMap((b) => braid(1420, 60, b)),
      ...ROSETTE.slice(2).flatMap((b) => braid(90, 1010, { ...b, radius: b.radius * 0.9 })),
    ],
    [],
  );
  return (
    <div className="ground" aria-hidden="true">
      <svg viewBox="0 0 1600 1000" preserveAspectRatio="xMidYMid slice">
        {paths.map((d, i) => (
          <path key={i} d={d} />
        ))}
      </svg>
    </div>
  );
}
