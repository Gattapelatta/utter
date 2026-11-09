import { useEffect, useRef } from "react";
import { ipc } from "../../lib/ipc";
import { createModel, gazeToward, stepModel, type Mood } from "./dropModel";
import { arcPath, clouds, drops, glints, ring, satellite } from "./effects";
import { eyePath } from "./eye";

export type { Mood };

/** The voice level 0..1, or where to read it each frame (the overlay: no React redraw per level). */
export type Level = number | (() => number);

interface Props {
  mood: Mood;
  /** Voice level 0..1 (already smoothed by the caller). */
  level: Level;
  translate: boolean;
  /** The «Задача» recording: its own colour (spec 2026-11-09 §3.7). */
  task?: boolean;
  reduced: boolean;
  /** Width in px (the pill's 48 by default). */
  size?: number;
  /** Look at the mouse pointer while it is over the page (settings logo). */
  follow?: boolean;
  /** Look this way (eye units), overriding the pointer — e.g. where the Drop is flying. */
  look?: { x: number; y: number } | null;
  /**
   * Hold the last drawn frame (the model keeps running): while the settings window flies the
   * Drop, a repaint every frame made WebKit miss frames and the flight stuttered.
   */
  frozen?: boolean;
  /** Living in the notch's ear while idle (§4.2a). */
  rest?: boolean;
  /**
   * Where the eyes look at first (handed over from the Drop in the notch). A function is read
   * when the character mounts — after the ear Drop, leaving in the same commit, has said where
   * its eyes really are.
   */
  startGaze?: { x: number; y: number } | (() => { x: number; y: number });
  /** Called whenever the character comes to rest, with where it is looking. */
  onSettle?: (gaze: { x: number; y: number }) => void;
}

export const BODY = "M50 14 C74 14 87 36 87 58 C87 77 71 90 50 90 C29 90 13 77 13 58 C13 36 26 14 50 14Z";
const STAR = "M0 -1 L.22 -.22 L1 0 L.22 .22 L0 1 L-.22 .22 L-1 0 L-.22 -.22Z";
const CLOUD = "M-12 4 C-16 4 -16 -3 -11 -3 C-11 -9 -3 -10 -1 -5 C1 -10 10 -9 10 -3 C15 -3 15 4 11 4Z";
/** An Archimedean spiral of two turns around (x, y), radius 5. */
function spiral(x: number, y: number): string {
  const pts: string[] = [];
  for (let i = 0; i <= 40; i++) {
    const a = (i / 40) * Math.PI * 4;
    const r = (i / 40) * 5;
    pts.push(`${(x + Math.cos(a) * r).toFixed(2)} ${(y + Math.sin(a) * r).toFixed(2)}`);
  }
  return `M${pts.join(" L")}`;
}

export const EYES: [number, number][] = [
  [38, 55],
  [62, 55],
];
const RINGS = 3;
const TAIL = 7;
const WHITE = [255, 255, 255];
const CYAN = [74, 216, 236];
/** The task mode: the Figma logo's violet #A259FF (chosen by the user 2026-11-10). */
const VIOLET = [162, 89, 255];

/**
 * The «Капля» character (docs/design/drop-prototype.html). The SVG is rendered once;
 * every frame mutates attributes directly so React does not re-render at 60–120 fps.
 */
export function Drop({ mood, level, translate, task = false, reduced, size = 48, follow = false, look = null, frozen = false, rest = false, startGaze, onSettle }: Props) {
  const svg = useRef<SVGSVGElement>(null);
  // The model only knows "coloured or not"; which colour is the Drop's own business.
  const tinted = translate || task;
  const input = useRef({ mood, level, translate: tinted, task, reduced, look, frozen, rest, onSettle });
  input.current = { mood, level, translate: tinted, task, reduced, look, frozen, rest, onSettle };
  const start = useRef(startGaze);
  const pointer = useRef<{ x: number; y: number } | null>(null);
  /** Restarts a sleeping frame loop (set by the loop's effect). */
  const wake = useRef(() => {});

  // Anything that changes what the character does must not wait for the sleeping loop.
  useEffect(() => wake.current(), [mood, tinted, task, reduced, look?.x, look?.y, frozen, rest]);

  useEffect(() => {
    if (!follow) return;
    const move = (e: PointerEvent) => {
      pointer.current = { x: e.clientX, y: e.clientY };
      wake.current();
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
    const [dizzy] = q<SVGGElement>("[data-dizzy]");
    const [eyeL, eyeR] = q<SVGPathElement>("[data-eye]");
    const [glow] = q<SVGCircleElement>("[data-glow]");
    const stops = q<SVGStopElement>("stop");
    const rings = q<SVGPathElement>("[data-ring]");
    const satBack = q<SVGCircleElement>("[data-sat-back]");
    const satFront = q<SVGCircleElement>("[data-sat-front]");
    const glintEls = q<SVGPathElement>("[data-glint]");
    const cloudEls = q<SVGGElement>("[data-cloud]");
    const dropEls = q<SVGLineElement>("[data-drop]");

    // Writes only what changed: at rest only the eyes move, and rewriting every ring, star and
    // gradient stop each frame cost ~2 ms of WebKit work per frame (5 % CPU in the notch).
    const written = new WeakMap<Element, Record<string, string>>();
    const put = (el: Element, name: string, value: string) => {
      let w = written.get(el);
      if (!w) written.set(el, (w = {}));
      if (w[name] === value) return;
      w[name] = value;
      el.setAttribute(name, value);
    };
    const s = start.current;
    const model = createModel(performance.now(), Math.random, typeof s === "function" ? s() : s);
    let raf = 0;
    let sleep: ReturnType<typeof setTimeout> | 0 = 0;
    wake.current = () => {
      if (!sleep) return;
      clearTimeout(sleep);
      sleep = 0;
      raf = requestAnimationFrame(frame);
    };
    /** After this frame: the next one, or — when nothing moves — a nap until the next move. */
    const next = (f: { settled: boolean; wakeAt: number; ex: number; ey: number }, now: number) => {
      if (f.settled && !pointer.current) {
        input.current.onSettle?.({ x: f.ex, y: f.ey });
        sleep = setTimeout(() => {
          sleep = 0;
          raf = requestAnimationFrame(frame);
        }, Math.max(16, Math.min(f.wakeAt - now, 60_000)));
      } else {
        raf = requestAnimationFrame(frame);
      }
    };
    // Smoothness probe (§8.4): frame intervals while listening, reported when it ends.
    let lastFrame = 0;
    let intervals: number[] = [];
    let wasListening = false;
    // Idle budget (§8.4): frames drawn per minute while resting in the notch.
    let restFrames = 0;
    let restSince = 0;

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
      if (cur.rest) {
        if (!restSince) restSince = now;
        restFrames++;
        if (now - restSince >= 60_000) {
          ipc.uiLog("info", `timing: drop frames while resting per_min=${restFrames}`).catch(() => {});
          restFrames = 0;
          restSince = now;
        }
      } else {
        restSince = 0;
        restFrames = 0;
      }

      let gaze = cur.look;
      if (!gaze && pointer.current) {
        const r = root.getBoundingClientRect();
        gaze = gazeToward(pointer.current.x - (r.left + r.width / 2), pointer.current.y - (r.top + r.height * 0.55));
      }
      const f = stepModel(model, { ...cur, gaze, level: typeof cur.level === "function" ? cur.level() : cur.level }, now);
      if (cur.frozen) {
        next(f, now);
        return;
      }
      put(body, "transform", `rotate(${f.rot.toFixed(3)} 50 90) translate(0 ${f.y.toFixed(3)})`);
      put(eyes, "transform", `translate(${f.ex.toFixed(3)} ${f.ey.toFixed(3)})`);
      put(eyes, "opacity", (1 - f.dizzy).toFixed(3));
      put(dizzy, "opacity", f.dizzy.toFixed(3));
      put(eyeL, "d", eyePath(EYES[0][0], EYES[0][1], f.lid, f.smile));
      put(eyeR, "d", eyePath(EYES[1][0], EYES[1][1], f.lid, f.smile));

      const tint = cur.task ? VIOLET : CYAN;
      const rgb = `rgb(${WHITE.map((c, i) => Math.round(c + (tint[i] - c) * f.cool)).join(",")})`;
      put(glow, "opacity", f.glow.toFixed(3));
      for (const s of stops) put(s, "stop-color", rgb);
      rings.forEach((r, i) => {
        const g = ring((f.flow * 0.8 + i / RINGS) % 1, f.env, f.rings);
        const opacity = g.opacity.toFixed(3);
        put(r, "opacity", opacity);
        // An invisible ring keeps drifting outwards; redrawing its path would only cost frames.
        if (opacity !== "0.000") put(r, "d", arcPath(g.r));
        put(r, "stroke", rgb);
      });

      const good = f.good;
      satellite(good).forEach((d, i) => {
        const [on, off] = d.front ? [satFront[i], satBack[i]] : [satBack[i], satFront[i]];
        put(on, "cx", d.x.toFixed(2));
        put(on, "cy", d.y.toFixed(2));
        put(on, "r", d.r.toFixed(2));
        put(on, "opacity", (Number.isFinite(good) ? d.opacity : 0).toFixed(3));
        put(off, "opacity", "0");
      });
      glints(good).forEach((g, i) => {
        put(glintEls[i], "transform", `translate(${g.x} ${g.y}) rotate(${g.rotate.toFixed(2)}) scale(${g.scale.toFixed(3)})`);
        put(glintEls[i], "opacity", (Number.isFinite(good) ? g.opacity : 0).toFixed(3));
      });
      const bad = Number.isFinite(f.bad) ? f.bad : 99;
      clouds(bad).forEach((c, i) => {
        put(cloudEls[i], "transform", `translate(${c.x.toFixed(2)} ${c.y.toFixed(2)}) scale(${c.scale})`);
        put(cloudEls[i], "opacity", c.opacity.toFixed(3));
      });
      drops(bad).forEach((d, i) => {
        put(dropEls[i], "transform", `translate(0 ${d.dy.toFixed(2)})`);
        put(dropEls[i], "opacity", d.opacity.toFixed(3));
      });
      next(f, now);
    };
    raf = requestAnimationFrame(frame);
    return () => {
      cancelAnimationFrame(raf);
      if (sleep) clearTimeout(sleep);
      wake.current = () => {};
    };
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
        {/* The easter egg: spiral eyes and blush (spec 2026-11-09 §3.6). */}
        <g data-dizzy opacity="0">
          {EYES.map(([x, y]) => (
            <path key={x} d={spiral(x, y)} fill="none" style={{ stroke: "var(--drop-eye, #000)" }} strokeWidth="1.6" strokeLinecap="round" />
          ))}
          {EYES.map(([x, y]) => (
            <ellipse key={`b${x}`} cx={x + (x < 50 ? -4 : 4)} cy={y + 9} rx="4.5" ry="2.6" fill="#ff8a9a" opacity="0.75" />
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
