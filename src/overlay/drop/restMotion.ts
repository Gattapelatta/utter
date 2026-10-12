import { springEasing } from "../../settings/onboarding/flight";
import { spring, stepSpring } from "./springs";

/** One eye unit of the character, in px of the 48 px wide Drop (viewBox 74 units wide). */
export const UNIT_PX = 48 / 74;

/** The character's springs (§5.4): gaze ω 5.5, blink ω 22, smile ω 7 — critically damped. */
const GAZE_OMEGA = 5.5;
const BLINK_OMEGA = 22;
const SMILE_OMEGA = 7;
/** How long a blink holds the lids shut and a smile holds, as in `dropModel`. */
const BLINK_HOLD_MS = 90;
const SMILE_HOLD_MS = 1600;

export type Gaze = { x: number; y: number };

export const gazeTransform = (g: Gaze) => `translate(${g.x * UNIT_PX}px, ${g.y * UNIT_PX}px)`;

/** A look from one place to another, as one compositor animation on the spring's curve. */
export function gazeMove(from: Gaze, to: Gaze): { keyframes: Keyframe[]; easing: string; duration: number } {
  const { easing, duration } = springEasing({ response: (2 * Math.PI) / GAZE_OMEGA, damping: 1 });
  return { keyframes: [{ transform: gazeTransform(from) }, { transform: gazeTransform(to) }], easing, duration };
}

/** Samples a 0→1→0 spring move: up for `holdMs`, then back, until it rests. */
function pulse(omega: number, holdMs: number, sampleMs: number): { values: number[]; duration: number } {
  const s = spring(0, omega);
  s.t = 1;
  const dt = 1 / 240;
  const values = [0];
  let t = 0;
  let next = sampleMs;
  while (t < 10_000) {
    stepSpring(s, dt);
    t += dt * 1000;
    if (t >= holdMs) s.t = 0;
    const resting = t > holdMs && Math.abs(s.x) < 0.002 && Math.abs(s.v) < 0.01;
    if (t >= next || resting) {
      values.push(resting ? 0 : Math.min(1, Math.max(0, s.x)));
      next += sampleMs;
    }
    if (resting) break;
  }
  return { values, duration: Math.round(t) };
}

const offsets = (n: number) => Array.from({ length: n }, (_, i) => (i === n - 1 ? 1 : +(i / (n - 1)).toFixed(5)));

/** A blink: the open eyes squeeze vertically about their centre and open again. */
export function blinkFrames(): { keyframes: { offset: number; transform: string }[]; duration: number } {
  const { values, duration } = pulse(BLINK_OMEGA, BLINK_HOLD_MS, 1000 / 120);
  const at = offsets(values.length);
  return {
    keyframes: values.map((lid, i) => ({ offset: at[i], transform: `scaleY(${+(1 - 0.9 * lid).toFixed(4)})` })),
    duration,
  };
}

/** A smile: the open eyes cross into "^ ^" and back; the two layers' opacities always add up to 1. */
export function smileFrames(): { open: { offset: number; opacity: number }[]; smile: { offset: number; opacity: number }[]; duration: number } {
  const { values, duration } = pulse(SMILE_OMEGA, SMILE_HOLD_MS, 1000 / 60);
  const at = offsets(values.length);
  const smile = values.map((v, i) => ({ offset: at[i], opacity: +v.toFixed(4) }));
  return { open: smile.map((k) => ({ offset: k.offset, opacity: 1 - k.opacity })), smile, duration };
}
