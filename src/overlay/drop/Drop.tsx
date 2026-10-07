import { useEffect, useRef } from "react";
import { clamp01, clouds, drops, easeInOut, glints, satellite } from "./effects";
import { eyePath } from "./eye";
import { spring, stepSpring, type Spring } from "./springs";

/** What the character is doing (spec §4.2). */
export type Mood = "idle" | "listening" | "thinking" | "happy" | "sad";

interface Props {
  mood: Mood;
  /** Voice level 0..1 (already smoothed by the caller). */
  level: number;
  translate: boolean;
  reduced: boolean;
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
const LOOK_IDLE: [number, number][] = [[0, 0], [-8, 0], [8, 0], [-6, -5], [6, -5], [0, -5.5], [-8, 2], [8, 2]];
const LOOK_THINK: [number, number][] = [[-6, -5.5], [6, -5.5], [0, -6], [-8, -2], [8, -2]];
const WHITE = [255, 255, 255];
const CYAN = [74, 216, 236];

type Springs = Record<"close" | "blink" | "smile" | "y" | "rot" | "ex" | "ey" | "env" | "fx" | "glow" | "cool", Spring>;

const springs = (): Springs => ({
  close: spring(0, 7),
  blink: spring(0, 22),
  smile: spring(0, 7),
  y: spring(0, 6),
  rot: spring(0, 6),
  ex: spring(0, 5.5),
  ey: spring(0, 5.5),
  env: spring(0, 4.5),
  fx: spring(0, 4),
  glow: spring(0.75, 4.5),
  cool: spring(0, 4),
});

/**
 * The «Капля» character (docs/design/drop-prototype.html). The SVG is rendered once;
 * every frame mutates attributes directly so React does not re-render at 60–120 fps.
 */
export function Drop({ mood, level, translate, reduced }: Props) {
  const svg = useRef<SVGSVGElement>(null);
  const input = useRef({ mood, level, translate, reduced });
  input.current = { mood, level, translate, reduced };

  useEffect(() => {
    const root = svg.current;
    if (!root) return;
    const q = <T extends Element>(sel: string) => [...root.querySelectorAll<T>(sel)];
    const [body] = q<SVGGElement>("[data-body]");
    const [eyes] = q<SVGGElement>("[data-eyes]");
    const [eyeL, eyeR] = q<SVGPathElement>("[data-eye]");
    const [glow] = q<SVGCircleElement>("[data-glow]");
    const stops = q<SVGStopElement>("stop");
    const rings = q<SVGCircleElement>("[data-ring]");
    const satBack = q<SVGCircleElement>("[data-sat-back]");
    const satFront = q<SVGCircleElement>("[data-sat-front]");
    const glintEls = q<SVGPathElement>("[data-glint]");
    const cloudEls = q<SVGGElement>("[data-cloud]");
    const dropEls = q<SVGLineElement>("[data-drop]");

    const P = springs();
    let mood: Mood | null = null;
    let moodAt = 0;
    let phase = 0;
    let flow = 0;
    let nextLook = 0;
    let nextBlink = performance.now() + 2600;
    let blinkUntil = 0;
    let last = performance.now();
    let raf = 0;

    const enter = (m: Mood, now: number) => {
      mood = m;
      moodAt = now;
      const S = P;
      S.smile.t = 0;
      S.ex.t = 0;
      S.ey.t = 0;
      S.y.t = 0;
      S.glow.t = 0.75;
      if (m === "listening") S.close.t = 1;
      if (m === "thinking" || m === "idle") {
        S.close.t = 0;
        nextLook = now + 500;
      }
      if (m === "happy") {
        S.close.t = 0;
        S.smile.t = 1;
        S.y.t = -3;
      }
      if (m === "sad") {
        S.close.t = 1;
        S.ey.t = 4.5;
        S.y.t = 2.5;
        S.glow.t = 0.35;
      }
    };

    const frame = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      const cur = input.current;
      if (cur.mood !== mood) enter(cur.mood, now);
      const since = (now - moodAt) / 1000;
      const still = cur.reduced;

      P.env.t = mood === "listening" && !still ? cur.level : 0;
      P.fx.t = mood === "listening" && !still ? 1 : 0;
      P.cool.t = cur.translate ? 1 : 0;
      if (mood === "listening") {
        phase += dt * Math.PI * 2 * 0.4;
        const e = P.env.x;
        P.rot.t = still ? 0 : Math.sin(phase) * (2 + e * 5);
        P.y.t = still ? 0 : -e * 2.2;
        P.glow.t = 0.75 + e * 0.25;
      } else {
        P.rot.t = 0;
        if (mood === "happy") {
          if (since > 0.9) P.y.t = 0;
          if (since > 2.0) P.smile.t = 0;
        }
        if ((mood === "idle" || mood === "thinking") && !still && now > nextLook) {
          const L = mood === "thinking" ? LOOK_THINK : LOOK_IDLE;
          const [x, y] = L[Math.floor(Math.random() * L.length)];
          P.ex.t = x;
          P.ey.t = y;
          nextLook = now + 1400 + Math.random() * 1800;
        }
        if (mood === "idle" && now > nextBlink) {
          P.blink.t = 1;
          blinkUntil = now + 90;
          nextBlink = now + 2800 + Math.random() * 2600;
        }
      }
      if (blinkUntil && now > blinkUntil) {
        P.blink.t = 0;
        blinkUntil = 0;
      }
      for (const s of Object.values(P)) stepSpring(s, dt);

      body.setAttribute("transform", `rotate(${P.rot.x.toFixed(3)} 50 90) translate(0 ${P.y.x.toFixed(3)})`);
      eyes.setAttribute("transform", `translate(${P.ex.x.toFixed(3)} ${P.ey.x.toFixed(3)})`);
      const lid = Math.max(clamp01(P.close.x), clamp01(P.blink.x));
      const smile = clamp01(P.smile.x);
      eyeL.setAttribute("d", eyePath(EYES[0][0], EYES[0][1], lid, smile));
      eyeR.setAttribute("d", eyePath(EYES[1][0], EYES[1][1], lid, smile));

      const w = clamp01(P.cool.x);
      const rgb = `rgb(${WHITE.map((c, i) => Math.round(c + (CYAN[i] - c) * w)).join(",")})`;
      glow.setAttribute("opacity", Math.max(0, P.glow.x).toFixed(3));
      for (const s of stops) s.setAttribute("stop-color", rgb);
      flow += dt * (0.3 + P.env.x * 0.45);
      const fxA = clamp01(P.fx.x);
      rings.forEach((r, i) => {
        const p = (flow * 0.8 + i / RINGS) % 1;
        const o = fxA * Math.pow(1 - p, 1.6) * Math.min(1, p * 6) * (0.12 + P.env.x * 0.45);
        r.setAttribute("r", (38 + easeInOut(p) * 28).toFixed(2));
        r.setAttribute("opacity", o.toFixed(3));
        r.setAttribute("stroke", rgb);
      });

      const good = mood === "happy" && !still ? since : Infinity;
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
      const bad = mood === "sad" && !still ? since : Infinity;
      clouds(Number.isFinite(bad) ? bad : 99).forEach((c, i) => {
        cloudEls[i].setAttribute("transform", `translate(${c.x.toFixed(2)} ${c.y.toFixed(2)}) scale(${c.scale})`);
        cloudEls[i].setAttribute("opacity", c.opacity.toFixed(3));
      });
      drops(Number.isFinite(bad) ? bad : 99).forEach((d, i) => {
        dropEls[i].setAttribute("transform", `translate(0 ${d.dy.toFixed(2)})`);
        dropEls[i].setAttribute("opacity", d.opacity.toFixed(3));
      });
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <svg ref={svg} viewBox="13 14 74 76" width="58" height="60" overflow="visible" aria-hidden className="shrink-0">
      <defs>
        <radialGradient id="drop-glow" cx="50%" cy="50%" r="50%">
          <stop offset="0" stopColor="#fff" stopOpacity=".55" />
          <stop offset=".45" stopColor="#fff" stopOpacity=".16" />
          <stop offset="1" stopColor="#fff" stopOpacity="0" />
        </radialGradient>
      </defs>
      {Array.from({ length: RINGS }, (_, i) => (
        <circle key={i} data-ring cx="50" cy="54" r="38" fill="none" strokeWidth="1.4" opacity="0" />
      ))}
      {Array.from({ length: TAIL }, (_, i) => (
        <circle key={i} data-sat-back fill="#fff" opacity="0" />
      ))}
      <g data-body>
        <circle data-glow cx="50" cy="55" r="58" fill="url(#drop-glow)" />
        <path d={BODY} fill="#fff" />
        <g data-eyes>
          {EYES.map(([x, y]) => (
            <path key={x} data-eye d={eyePath(x, y, 0, 0)} fill="#000" stroke="#000" strokeWidth="1.6" strokeLinejoin="round" />
          ))}
        </g>
      </g>
      {Array.from({ length: TAIL }, (_, i) => (
        <circle key={i} data-sat-front fill="#fff" opacity="0" />
      ))}
      {[0, 1, 2].map((i) => (
        <path key={i} data-glint d={STAR} fill="#fff" opacity="0" />
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
