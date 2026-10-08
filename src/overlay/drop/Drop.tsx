import { useEffect, useRef } from "react";
import { ipc } from "../../lib/ipc";
import { createModel, gazeToward, stepModel, type Mood } from "./dropModel";
import { arcPath, clouds, drops, glints, ring, satellite } from "./effects";
import { eyePath } from "./eye";

export type { Mood };

interface Props {
  mood: Mood;
  /** Voice level 0..1 (already smoothed by the caller). */
  level: number;
  translate: boolean;
  reduced: boolean;
  /** Width in px (the pill's 48 by default). */
  size?: number;
  /** Look at the mouse pointer while it is over the page (settings logo). */
  follow?: boolean;
  /** Look this way (eye units), overriding the pointer — e.g. where the Drop is flying. */
  look?: { x: number; y: number } | null;
}

const BODY = "M50 14 C74 14 87 36 87 58 C87 77 71 90 50 90 C29 90 13 77 13 58 C13 36 26 14 50 14Z";
const STAR = "M0 -1 L.22 -.22 L1 0 L.22 .22 L0 1 L-.22 .22 L-1 0 L-.22 -.22Z";
const CLOUD = "M-12 4 C-16 4 -16 -3 -11 -3 C-11 -9 -3 -10 -1 -5 C1 -10 10 -9 10 -3 C15 -3 15 4 11 4Z";
const EYES: [number, number][] = [
  [38, 55],
  [62, 55],
];
const RINGS = 3;
const TAIL = 7;
const WHITE = [255, 255, 255];
const CYAN = [74, 216, 236];

/**
 * The «Капля» character (docs/design/drop-prototype.html). The SVG is rendered once;
 * every frame mutates attributes directly so React does not re-render at 60–120 fps.
 */
export function Drop({ mood, level, translate, reduced, size = 48, follow = false, look = null }: Props) {
  const svg = useRef<SVGSVGElement>(null);
  const input = useRef({ mood, level, translate, reduced, look });
  input.current = { mood, level, translate, reduced, look };
  const pointer = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    if (!follow) return;
    const move = (e: PointerEvent) => {
      pointer.current = { x: e.clientX, y: e.clientY };
    };
    const leave = () => {
      pointer.current = null;
    };
    const root = document.documentElement;
    document.addEventListener("pointermove", move);
    root.addEventListener("mouseleave", leave);
    return () => {
      document.removeEventListener("pointermove", move);
      root.removeEventListener("mouseleave", leave);
    };
  }, [follow]);

  useEffect(() => {
    const root = svg.current;
    if (!root) return;
    const q = <T extends Element>(sel: string) => [...root.querySelectorAll<T>(sel)];
    const [body] = q<SVGGElement>("[data-body]");
    const [eyes] = q<SVGGElement>("[data-eyes]");
    const [eyeL, eyeR] = q<SVGPathElement>("[data-eye]");
    const [glow] = q<SVGCircleElement>("[data-glow]");
    const stops = q<SVGStopElement>("stop");
    const rings = q<SVGPathElement>("[data-ring]");
    const satBack = q<SVGCircleElement>("[data-sat-back]");
    const satFront = q<SVGCircleElement>("[data-sat-front]");
    const glintEls = q<SVGPathElement>("[data-glint]");
    const cloudEls = q<SVGGElement>("[data-cloud]");
    const dropEls = q<SVGLineElement>("[data-drop]");

    const model = createModel(performance.now());
    let raf = 0;
    // Smoothness probe (§8.4): frame intervals while listening, reported when it ends.
    let lastFrame = 0;
    let intervals: number[] = [];
    let wasListening = false;

    const frame = (now: number) => {
      const cur = input.current;
      const listening = cur.mood === "listening";
      if (listening && lastFrame) intervals.push(now - lastFrame);
      if (wasListening && !listening && intervals.length > 10) {
        const sorted = [...intervals].sort((a, b) => a - b);
        const avg = sorted.reduce((a, b) => a + b, 0) / sorted.length;
        ipc.overlayFrames(avg, sorted[Math.floor(sorted.length * 0.95)], sorted[sorted.length - 1], sorted.length).catch(() => {});
        intervals = [];
      }
      wasListening = listening;
      lastFrame = now;

      let gaze = cur.look;
      if (!gaze && pointer.current) {
        const r = root.getBoundingClientRect();
        gaze = gazeToward(pointer.current.x - (r.left + r.width / 2), pointer.current.y - (r.top + r.height * 0.55));
      }
      const f = stepModel(model, { ...cur, gaze }, now);
      body.setAttribute("transform", `rotate(${f.rot.toFixed(3)} 50 90) translate(0 ${f.y.toFixed(3)})`);
      eyes.setAttribute("transform", `translate(${f.ex.toFixed(3)} ${f.ey.toFixed(3)})`);
      eyeL.setAttribute("d", eyePath(EYES[0][0], EYES[0][1], f.lid, f.smile));
      eyeR.setAttribute("d", eyePath(EYES[1][0], EYES[1][1], f.lid, f.smile));

      const rgb = `rgb(${WHITE.map((c, i) => Math.round(c + (CYAN[i] - c) * f.cool)).join(",")})`;
      glow.setAttribute("opacity", f.glow.toFixed(3));
      for (const s of stops) s.setAttribute("stop-color", rgb);
      rings.forEach((r, i) => {
        const g = ring((f.flow * 0.8 + i / RINGS) % 1, f.env, f.rings);
        r.setAttribute("d", arcPath(g.r));
        r.setAttribute("opacity", g.opacity.toFixed(3));
        r.setAttribute("stroke", rgb);
      });

      const good = f.good;
      satellite(good).forEach((d, i) => {
        const [on, off] = d.front ? [satFront[i], satBack[i]] : [satBack[i], satFront[i]];
        on.setAttribute("cx", d.x.toFixed(2));
        on.setAttribute("cy", d.y.toFixed(2));
        on.setAttribute("r", d.r.toFixed(2));
        on.setAttribute("opacity", (Number.isFinite(good) ? d.opacity : 0).toFixed(3));
        off.setAttribute("opacity", "0");
      });
      glints(good).forEach((g, i) => {
        glintEls[i].setAttribute("transform", `translate(${g.x} ${g.y}) rotate(${g.rotate.toFixed(2)}) scale(${g.scale.toFixed(3)})`);
        glintEls[i].setAttribute("opacity", (Number.isFinite(good) ? g.opacity : 0).toFixed(3));
      });
      const bad = Number.isFinite(f.bad) ? f.bad : 99;
      clouds(bad).forEach((c, i) => {
        cloudEls[i].setAttribute("transform", `translate(${c.x.toFixed(2)} ${c.y.toFixed(2)}) scale(${c.scale})`);
        cloudEls[i].setAttribute("opacity", c.opacity.toFixed(3));
      });
      drops(bad).forEach((d, i) => {
        dropEls[i].setAttribute("transform", `translate(0 ${d.dy.toFixed(2)})`);
        dropEls[i].setAttribute("opacity", d.opacity.toFixed(3));
      });
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <svg ref={svg} viewBox="13 14 74 76" width={size} height={Math.round((size * 50) / 48)} overflow="visible" aria-hidden className="shrink-0">
      <defs>
        <radialGradient id="drop-glow" cx="50%" cy="50%" r="50%">
          {/* Tighter and softer than the prototype: a wide glow made the island look grey next to the notch. */}
          <stop offset="0" stopColor="#fff" stopOpacity=".4" />
          <stop offset=".5" stopColor="#fff" stopOpacity=".07" />
          <stop offset="1" stopColor="#fff" stopOpacity="0" />
        </radialGradient>
      </defs>
      {Array.from({ length: RINGS }, (_, i) => (
        <path key={i} data-ring d={arcPath(38)} fill="none" strokeWidth="2.2" strokeLinecap="round" opacity="0" />
      ))}
      {Array.from({ length: TAIL }, (_, i) => (
        <circle key={i} data-sat-back style={{ fill: "var(--drop-body, #fff)" }} opacity="0" />
      ))}
      <g data-body>
        <circle data-glow cx="50" cy="55" r="48" fill="url(#drop-glow)" />
        <path d={BODY} style={{ fill: "var(--drop-body, #fff)" }} />
        <g data-eyes>
          {EYES.map(([x, y]) => (
            <path
              key={x}
              data-eye
              d={eyePath(x, y, 0, 0)}
              style={{ fill: "var(--drop-eye, #000)", stroke: "var(--drop-eye, #000)" }}
              strokeWidth="1.6"
              strokeLinejoin="round"
            />
          ))}
        </g>
      </g>
      {Array.from({ length: TAIL }, (_, i) => (
        <circle key={i} data-sat-front style={{ fill: "var(--drop-body, #fff)" }} opacity="0" />
      ))}
      {[0, 1, 2].map((i) => (
        <path key={i} data-glint d={STAR} style={{ fill: "var(--drop-body, #fff)" }} opacity="0" />
      ))}
      {[0, 1, 2].map((i) => (
        <g key={i} data-cloud opacity="0">
          <path d={CLOUD} fill="#9aa3ad" />
          {i === 1 &&
            [-4, 4].map((x) => (
              <line key={x} data-drop x1={x} y1="6" x2={x - 0.5} y2="9" stroke="#9aa3ad" strokeWidth="1.3" strokeLinecap="round" opacity="0" />
            ))}
        </g>
      ))}
    </svg>
  );
}
