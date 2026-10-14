// The surface of rest points of the information-liquidity equation.
//
// Each curve running across the surface is the answer to one question: for a
// loan of a given volatility, at what level of uncertainty does the market
// settle, as a function of how many clients ask for quotes? Where a curve
// doubles back there are two answers for the same flow, a lit market and a
// dark one, with a tipping point between them.

import { useEffect, useRef } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import type { Kernel, Market, RestPoint } from "../lib/kernel";
import { useStore } from "../state";
import { PALETTES } from "../theme";

const N_SD = 72;
const N_VOL = 30;
const SD_MAX = 2.6;
const VOL: [number, number] = [0.04, 0.34];
const LOG_A: [number, number] = [-1.6, 1.8];

const BOX = { x: 1.7, y: 1.0, z: 1.25 };

function place(logA: number, sd: number, vol: number): [number, number, number] {
  return [
    ((logA - LOG_A[0]) / (LOG_A[1] - LOG_A[0]) * 2 - 1) * BOX.x,
    ((sd / SD_MAX) * 2 - 1) * BOX.y,
    ((vol - VOL[0]) / (VOL[1] - VOL[0]) * 2 - 1) * BOX.z,
  ];
}

interface Props {
  kernel: Kernel;
  market: Market;
  rest: RestPoint[];
  height?: number;
}

export function Cusp3D({ kernel, market, rest, height = 380 }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const labels = useRef<HTMLDivElement>(null);
  const stage = useRef<{ scene: THREE.Scene; surface: THREE.Group; markers: THREE.Group; frame: THREE.LineSegments; render: () => void } | null>(null);
  const theme = useStore((s) => s.theme);
  const pal = PALETTES[theme];

  // The scene is built once; the surface and markers are rebuilt on change.
  useEffect(() => {
    const el = host.current!;
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setClearColor(0x000000, 0);
    el.appendChild(renderer.domElement);
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(36, 1, 0.1, 50);
    camera.position.set(3.3, 1.5, 4.3);
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.enablePan = false;
    controls.minDistance = 3;
    controls.maxDistance = 10;

    // The box the surface lives in, drawn as its three back edges and floor.
    const frame = new THREE.LineSegments(
      new THREE.EdgesGeometry(new THREE.BoxGeometry(2 * BOX.x, 2 * BOX.y, 2 * BOX.z)),
      new THREE.LineBasicMaterial({ color: 0xbec6f0, transparent: true, opacity: 0.14 }),
    );
    scene.add(frame);
    const surface = new THREE.Group();
    const markers = new THREE.Group();
    scene.add(surface, markers);

    const axis = [
      { text: "more clients asking", at: [0, -BOX.y - 0.16, BOX.z + 0.1] },
      { text: "less is known", at: [-BOX.x - 0.12, 0.2, BOX.z + 0.1] },
      { text: "more volatile loan", at: [BOX.x + 0.14, -BOX.y - 0.1, 0] },
    ].map((a) => {
      const d = document.createElement("div");
      d.className = "sky-label";
      d.style.transform = "translate(-50%, -50%)";
      d.textContent = a.text;
      labels.current!.appendChild(d);
      return { d, v: new THREE.Vector3(...(a.at as [number, number, number])) };
    });

    let raf = 0;
    let dirty = true;
    const render = () => {
      dirty = true;
    };
    const tick = () => {
      raf = requestAnimationFrame(tick);
      if (controls.update() || dirty) {
        dirty = false;
        renderer.render(scene, camera);
        for (const { d, v } of axis) {
          const p = v.clone().project(camera);
          d.style.left = `${(p.x * 0.5 + 0.5) * el.clientWidth}px`;
          d.style.top = `${(-p.y * 0.5 + 0.5) * el.clientHeight}px`;
        }
      }
    };
    const resize = () => {
      const w = el.clientWidth || 1;
      const h = el.clientHeight || 1;
      renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
      renderer.setSize(w, h, false);
      renderer.domElement.style.width = "100%";
      renderer.domElement.style.height = "100%";
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      dirty = true;
    };
    const ro = new ResizeObserver(resize);
    ro.observe(el);
    resize();
    tick();
    stage.current = { scene, surface, markers, frame, render };
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      controls.dispose();
      renderer.dispose();
      renderer.domElement.remove();
      axis.forEach((a) => a.d.remove());
      stage.current = null;
    };
  }, []);

  // Everything about the market except flow and volatility shapes the surface.
  const { desk, markGap, markSd, printSd, subsidy, subsidyAbove } = market;
  const shapeKey = [desk.decay, desk.gamma, desk.informed, markGap, markSd, printSd, subsidy, subsidyAbove].join("|");

  useEffect(() => {
    const st = stage.current;
    if (!st) {
      return;
    }
    st.surface.clear();
    const frame = st.frame.material as THREE.LineBasicMaterial;
    frame.color.setHex(pal.frame);
    frame.opacity = pal.frameOpacity;
    const LIT = new THREE.Color(pal.lit);
    const DARK = new THREE.Color(pal.dark);
    const TIP = new THREE.Color(pal.tipping);
    const sds = Float64Array.from({ length: N_SD }, (_, i) => 0.04 * (SD_MAX / 0.04) ** (i / (N_SD - 1)));
    const pos = new Float32Array(N_VOL * N_SD * 3);
    const col = new Float32Array(N_VOL * N_SD * 3);
    const ok = new Uint8Array(N_VOL * N_SD);
    const lines: number[] = [];
    const lineCol: number[] = [];
    for (let j = 0; j < N_VOL; j++) {
      const vol = VOL[0] + ((VOL[1] - VOL[0]) * j) / (N_VOL - 1);
      const arrival = kernel.manifold({ ...market, dailyVar: vol * vol }, sds);
      for (let i = 0; i < N_SD; i++) {
        const k = j * N_SD + i;
        const logA = Math.log10(arrival[i]);
        ok[k] = Number.isFinite(logA) && logA >= LOG_A[0] && logA <= LOG_A[1] ? 1 : 0;
        pos.set(place(Math.min(LOG_A[1], Math.max(LOG_A[0], Number.isFinite(logA) ? logA : LOG_A[0])), sds[i], vol), 3 * k);
        // A rest point is stable where more uncertainty would need less flow
        // to hold it: where the curve falls as it rises.
        const prev = arrival[Math.max(0, i - 1)];
        const next = arrival[Math.min(N_SD - 1, i + 1)];
        const stable = next < prev;
        const c = stable ? LIT.clone().lerp(DARK, Math.min(1, sds[i] / 1.6)) : TIP;
        col.set([c.r, c.g, c.b], 3 * k);
      }
      for (let i = 0; i + 1 < N_SD; i++) {
        const a = j * N_SD + i;
        if (ok[a] && ok[a + 1]) {
          lines.push(pos[3 * a], pos[3 * a + 1], pos[3 * a + 2], pos[3 * a + 3], pos[3 * a + 4], pos[3 * a + 5]);
          lineCol.push(col[3 * a], col[3 * a + 1], col[3 * a + 2], col[3 * a + 3], col[3 * a + 4], col[3 * a + 5]);
        }
      }
    }
    const index: number[] = [];
    for (let j = 0; j + 1 < N_VOL; j++) {
      for (let i = 0; i + 1 < N_SD; i++) {
        const a = j * N_SD + i;
        const b = a + 1;
        const c = a + N_SD;
        const d = c + 1;
        if (ok[a] && ok[b] && ok[c] && ok[d]) {
          index.push(a, b, c, b, d, c);
        }
      }
    }
    const geom = new THREE.BufferGeometry();
    geom.setAttribute("position", new THREE.BufferAttribute(pos, 3));
    geom.setAttribute("color", new THREE.BufferAttribute(col, 3));
    geom.setIndex(index);
    st.surface.add(
      new THREE.Mesh(geom, new THREE.MeshBasicMaterial({ vertexColors: true, transparent: true, opacity: pal.surfaceOpacity, side: THREE.DoubleSide, depthWrite: false })),
    );
    const lgeom = new THREE.BufferGeometry();
    lgeom.setAttribute("position", new THREE.BufferAttribute(new Float32Array(lines), 3));
    lgeom.setAttribute("color", new THREE.BufferAttribute(new Float32Array(lineCol), 3));
    st.surface.add(new THREE.LineSegments(lgeom, new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.85 })));
    st.render();
    return () => {
      geom.dispose();
      lgeom.dispose();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kernel, shapeKey, theme]);

  // The market being studied: a vertical line at its flow and volatility, and
  // a bead wherever that line meets the surface.
  useEffect(() => {
    const st = stage.current;
    if (!st) {
      return;
    }
    st.markers.clear();
    const vol = Math.sqrt(market.dailyVar);
    const logA = Math.log10(Math.max(1e-9, market.desk.arrival));
    const inside = vol >= VOL[0] && vol <= VOL[1] && logA >= LOG_A[0] && logA <= LOG_A[1];
    if (inside) {
      const foot = place(logA, 0, vol);
      const head = place(logA, SD_MAX, vol);
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.BufferAttribute(new Float32Array([...foot, ...head]), 3));
      st.markers.add(new THREE.Line(g, new THREE.LineBasicMaterial({ color: pal.ink, transparent: true, opacity: 0.45 })));
      for (const r of rest) {
        if (r.sd > SD_MAX) {
          continue;
        }
        const bead = new THREE.Mesh(
          new THREE.SphereGeometry(r.stable ? 0.055 : 0.04, 20, 14),
          new THREE.MeshBasicMaterial({ color: r.stable ? pal.ink : new THREE.Color(pal.tipping).getHex() }),
        );
        bead.position.set(...place(logA, r.sd, vol));
        st.markers.add(bead);
      }
    }
    st.render();
  }, [market.dailyVar, market.desk.arrival, rest, theme]);

  return (
    <div style={{ position: "relative", height }}>
      <div ref={host} style={{ position: "absolute", inset: 0, cursor: "grab" }} aria-label="Surface of rest points; drag to turn it" role="img" />
      <div ref={labels} style={{ position: "absolute", inset: 0, pointerEvents: "none" }} />
    </div>
  );
}
