import { useEffect, useRef, type MutableRefObject } from "react";
import { BODY, EYES } from "./Drop";
import { LOOK_IDLE } from "./dropModel";
import { eyePath } from "./eye";
import { blinkFrames, gazeMove, gazeProgress, gazeTransform, smileFrames, type Gaze } from "./restMotion";

interface Props {
  /** Where the eyes look when it appears (handed over by the full Drop, so nothing jumps). */
  start: Gaze;
  reduced: boolean;
  /** Where it is looking, for handing back to the full Drop — set to where the eyes really are as it leaves. */
  gazeRef?: MutableRefObject<Gaze>;
}

/** Eye centre height in px of the 48 × 50 Drop (viewBox from y 14, 76 units tall). */
const EYE_Y_PX = ((EYES[0][1] - 14) * 50) / 76;
const LAYER = { position: "absolute", inset: 0, willChange: "transform, opacity" } as const;

/**
 * The Drop living in the notch's ear (§4.2a), moved by the compositor: a still body and an eye
 * layer that macOS animates along the character's own spring curves. The full Drop redrew its
 * SVG from script for every frame of every look — 3 % CPU just to idle; this one draws none.
 */
export function RestDrop({ start, reduced, gazeRef }: Props) {
  const gaze = useRef<HTMLDivElement>(null);
  const open = useRef<SVGSVGElement>(null);
  const smile = useRef<SVGSVGElement>(null);
  const startAt = useRef(start);

  useEffect(() => {
    const g = gaze.current;
    const o = open.current;
    const s = smile.current;
    if (!g || !o || !s) return;
    const timers = new Set<ReturnType<typeof setTimeout>>();
    // One animation per kind of move, replayed each time: a new Animation every few seconds is
    // kept by WebKit long after it ends (a bare WKWebView grew 34 MB in 3 minutes; replaying
    // one stayed flat).
    const live = new Map<string, Animation>();
    const later = (ms: number, fn: () => void) => {
      const t = setTimeout(() => {
        timers.delete(t);
        fn();
      }, ms);
      timers.add(t);
    };
    const play = (kind: string, el: Element, keyframes: Keyframe[], duration: number, changes: boolean) => {
      if (typeof el.animate !== "function") return;
      const a = live.get(kind);
      if (!a) {
        live.set(kind, el.animate(keyframes, { duration }));
        return;
      }
      const effect = a.effect as KeyframeEffect | null;
      if (changes) effect?.setKeyframes(keyframes);
      effect?.updateTiming({ duration });
      a.currentTime = 0;
      a.play();
    };
    let at = startAt.current;
    /** The look in progress, to know where the eyes are if a recording interrupts it. */
    let moving: { from: Gaze; to: Gaze; since: number } | null = null;

    const look = () => {
      const [x, y] = LOOK_IDLE[Math.floor(Math.random() * LOOK_IDLE.length)];
      const to = { x, y };
      const { keyframes, duration } = gazeMove(at, to);
      g.style.transform = gazeTransform(to);
      play("gaze", g, keyframes, duration, true);
      moving = { from: at, to, since: Date.now() };
      at = to;
      if (gazeRef) gazeRef.current = to;
      later(2000 + Math.random() * 3000, look);
    };
    const blink = () => {
      const { keyframes, duration } = blinkFrames();
      play("blink", o, keyframes, duration, false);
      later(2800 + Math.random() * 2600, blink);
    };
    const smileOnce = () => {
      const f = smileFrames();
      play("smile-open", o, f.open, f.duration, false);
      play("smile", s, f.smile, f.duration, false);
      later(30_000 + Math.random() * 30_000, smileOnce);
    };

    if (gazeRef) gazeRef.current = at;
    if (!reduced) {
      later(2000 + Math.random() * 3000, look);
      later(30_000 + Math.random() * 30_000, smileOnce);
    }
    later(2800 + Math.random() * 2600, blink);
    return () => {
      if (gazeRef && moving) {
        const p = gazeProgress(Date.now() - moving.since);
        const { from, to } = moving;
        gazeRef.current = { x: from.x + (to.x - from.x) * p, y: from.y + (to.y - from.y) * p };
      }
      for (const t of timers) clearTimeout(t);
      for (const a of live.values()) a.cancel();
    };
  }, [reduced, gazeRef]);

  const eyes = (smiling: number) =>
    EYES.map(([x, y]) => <path key={x} d={eyePath(x, y, 0, smiling)} style={{ fill: "var(--drop-eye, #000)", stroke: "var(--drop-eye, #000)" }} strokeWidth="1.6" strokeLinejoin="round" />);

  return (
    <div data-rest-drop style={{ position: "relative", width: 48, height: 50 }} aria-hidden>
      <svg viewBox="13 14 74 76" width={48} height={50} style={{ position: "absolute", inset: 0 }}>
        <path d={BODY} style={{ fill: "var(--drop-body, #fff)" }} />
      </svg>
      <div data-rest-gaze ref={gaze} style={{ ...LAYER, transform: gazeTransform(start) }}>
        <svg data-rest-open ref={open} viewBox="13 14 74 76" width={48} height={50} style={{ ...LAYER, transformOrigin: `24px ${EYE_Y_PX}px` }}>
          {eyes(0)}
        </svg>
        <svg data-rest-smile ref={smile} viewBox="13 14 74 76" width={48} height={50} style={{ ...LAYER, opacity: 0 }}>
          {eyes(1)}
        </svg>
      </div>
    </div>
  );
}
