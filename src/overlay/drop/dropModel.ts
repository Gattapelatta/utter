import { spring, stepSpring, type Spring } from "./springs";

/** What the character is doing (spec §4.2). */
export type Mood = "idle" | "listening" | "thinking" | "happy" | "sad";

export interface ModelInput {
  mood: Mood;
  /** Raw voice level 0..1 as it arrives from the microphone (~30 Hz, spiky). */
  level: number;
  translate: boolean;
  reduced: boolean;
  /** Where to look, in eye units (see `gazeToward`); null = look around on its own. */
  gaze?: { x: number; y: number } | null;
  /** Living in the notch's ear while idle (§4.2a): calmer looks, an occasional smile, no glow. */
  rest?: boolean;
}

/** Everything the SVG needs for one frame. */
export interface Frame {
  rot: number;
  y: number;
  ex: number;
  ey: number;
  lid: number;
  smile: number;
  glow: number;
  /** 0 = white, 1 = cyan (translate). */
  cool: number;
  /** Smoothed voice envelope 0..1. */
  env: number;
  /** Ring visibility 0..1 and their phase. */
  rings: number;
  flow: number;
  /** Seconds since the success/error effect started, or Infinity. */
  good: number;
  bad: number;
  /** Nothing moves until `wakeAt`: the caller may stop drawing frames until then. */
  settled: boolean;
  /** When the next scheduled move (look, blink, smile) starts, in the same clock as `now`. */
  wakeAt: number;
}

/**
 * Still enough to stop drawing: positions (eye units, ~0.7 px each at the notch size, 1.6 px in
 * the settings logo) within 0.02 px-ish, 0..1 values (lids, smile, glow, colour) within 0.5 %.
 * A critically damped spring spends most of its time creeping through the last invisible
 * fraction; drawing it kept the character awake half the time.
 */
const STILL_POS = 0.02;
const STILL = 0.005;
const POSITIONS = new Set(["ex", "ey", "y", "rot"]);
/** In the notch: a new look every 2–5 s, a smile every 30–60 s held for 1.6 s (§4.2a). */
const REST_LOOK_MS = [2000, 3000];
const REST_SMILE_MS = [30000, 30000];
const REST_SMILE_HOLD_MS = 1600;

/** Eye travel limits, in the same units as LOOK_IDLE. */
const GAZE_X = 8;
const GAZE_UP = 5.5;
const GAZE_DOWN = 3;
/** Closer than this (px) the eyes move proportionally less, so they do not roll at a nearby cursor. */
const GAZE_NEAR = 120;

/** Eye offset that looks from the character's centre towards a point `dx, dy` pixels away. */
export function gazeToward(dx: number, dy: number): { x: number; y: number } {
  const dist = Math.hypot(dx, dy);
  if (dist === 0) return { x: 0, y: 0 };
  const k = Math.min(1, dist / GAZE_NEAR);
  const y = (dy / dist) * GAZE_UP * k;
  return { x: (dx / dist) * GAZE_X * k, y: Math.max(-GAZE_UP, Math.min(GAZE_DOWN, y)) };
}

export const LOOK_IDLE: [number, number][] = [[0, 0], [-8, 0], [8, 0], [-6, -5], [6, -5], [0, -5.5], [-8, 2], [8, 2]];
const LOOK_THINK: [number, number][] = [[-6, -5.5], [6, -5.5], [0, -6], [-8, -2], [8, -2]];

type Springs = Record<"close" | "blink" | "smile" | "y" | "rot" | "ex" | "ey" | "env" | "fx" | "glow" | "cool", Spring>;

export interface Model {
  P: Springs;
  mood: Mood | null;
  moodAt: number;
  phase: number;
  flow: number;
  nextLook: number;
  nextBlink: number;
  blinkUntil: number;
  nextSmile: number;
  smileUntil: number;
  last: number;
  random: () => number;
}

/** `gaze` starts the eyes looking there (handed over from the Drop in the notch). */
export function createModel(now: number, random: () => number = Math.random, gaze?: { x: number; y: number }): Model {
  return {
    P: {
      close: spring(0, 7),
      blink: spring(0, 22),
      smile: spring(0, 7),
      y: spring(0, 6),
      rot: spring(0, 6),
      ex: spring(gaze?.x ?? 0, 5.5),
      ey: spring(gaze?.y ?? 0, 5.5),
      env: spring(0, 4.5),
      fx: spring(0, 4),
      glow: spring(0.75, 4.5),
      cool: spring(0, 4),
    },
    mood: null,
    moodAt: now,
    phase: 0,
    flow: 0,
    nextLook: 0,
    nextBlink: now + 2600,
    blinkUntil: 0,
    nextSmile: 0,
    smileUntil: 0,
    last: now,
    random,
  };
}

function enter(m: Model, mood: Mood, now: number) {
  const P = m.P;
  // The first mood keeps the starting gaze; later changes look straight ahead first.
  if (m.mood !== null) {
    P.ex.t = 0;
    P.ey.t = 0;
  }
  m.mood = mood;
  m.moodAt = now;
  P.smile.t = 0;
  P.y.t = 0;
  P.glow.t = 0.75;
  if (mood === "listening") P.close.t = 1;
  if (mood === "thinking" || mood === "idle") {
    P.close.t = 0;
    m.nextLook = now + 500;
  }
  if (mood === "happy") {
    P.close.t = 0;
    P.smile.t = 1;
    P.y.t = -3;
  }
  if (mood === "sad") {
    P.close.t = 1;
    P.ey.t = 4.5;
    P.y.t = 2.5;
    P.glow.t = 0.35;
  }
}

/** Advances the character to `now` (ms) and returns what to draw. */
export function stepModel(m: Model, input: ModelInput, now: number): Frame {
  const P = m.P;
  const dt = Math.min(0.05, Math.max(0, (now - m.last) / 1000));
  m.last = now;
  if (input.mood !== m.mood) enter(m, input.mood, now);
  const since = (now - m.moodAt) / 1000;
  const still = input.reduced;
  const mood = m.mood;

  P.env.t = mood === "listening" && !still ? input.level : 0;
  P.fx.t = mood === "listening" && !still ? 1 : 0;
  P.cool.t = input.translate ? 1 : 0;
  if (mood === "listening") {
    m.phase += dt * Math.PI * 2 * 0.4;
    const e = P.env.x;
    P.rot.t = still ? 0 : Math.sin(m.phase) * (2 + e * 5);
    P.y.t = still ? 0 : -e * 2.2;
    P.glow.t = 0.75 + e * 0.25;
  } else {
    P.rot.t = 0;
    if (mood === "happy") {
      if (since > 0.9) P.y.t = 0;
      if (since > 2.0) P.smile.t = 0;
    }
    if (mood === "idle" && input.gaze && !still) {
      P.ex.t = input.gaze.x;
      P.ey.t = input.gaze.y;
      m.nextLook = now + 500;
    } else if ((mood === "idle" || mood === "thinking") && !still && now > m.nextLook) {
      const L = mood === "thinking" ? LOOK_THINK : LOOK_IDLE;
      const [x, y] = L[Math.floor(m.random() * L.length)];
      P.ex.t = x;
      P.ey.t = y;
      m.nextLook = input.rest && mood === "idle" ? now + REST_LOOK_MS[0] + m.random() * REST_LOOK_MS[1] : now + 1400 + m.random() * 1800;
    }
    if (mood === "idle" && now > m.nextBlink) {
      P.blink.t = 1;
      m.blinkUntil = now + 90;
      m.nextBlink = now + 2800 + m.random() * 2600;
    }
  }
  if (m.blinkUntil && now > m.blinkUntil) {
    P.blink.t = 0;
    m.blinkUntil = 0;
  }
  const resting = !!input.rest && mood === "idle";
  if (mood === "idle") P.glow.t = resting ? 0 : 0.75;
  if (resting && !still) {
    if (!m.nextSmile) m.nextSmile = now + REST_SMILE_MS[0] + m.random() * REST_SMILE_MS[1];
    if (!m.smileUntil && now > m.nextSmile) {
      P.smile.t = 1;
      m.smileUntil = now + REST_SMILE_HOLD_MS;
      m.nextSmile = now + REST_SMILE_MS[0] + m.random() * REST_SMILE_MS[1];
    }
  } else {
    m.nextSmile = 0;
  }
  if (m.smileUntil && (now > m.smileUntil || !resting)) {
    if (mood === "idle") P.smile.t = 0;
    m.smileUntil = 0;
  }
  for (const s of Object.values(P)) stepSpring(s, dt);
  m.flow += dt * (0.3 + P.env.x * 0.45);

  const clamp = (v: number) => Math.max(0, Math.min(1, v));
  return {
    rot: P.rot.x,
    y: P.y.x,
    ex: P.ex.x,
    ey: P.ey.x,
    lid: Math.max(clamp(P.close.x), clamp(P.blink.x)),
    smile: clamp(P.smile.x),
    glow: Math.max(0, P.glow.x),
    cool: clamp(P.cool.x),
    env: P.env.x,
    rings: clamp(P.fx.x),
    flow: m.flow,
    good: mood === "happy" && !still ? since : Infinity,
    bad: mood === "sad" && !still ? since : Infinity,
    settled: (mood === "idle" || mood === "thinking") && Object.entries(P).every(([k, s]) => {
        const tol = POSITIONS.has(k) ? STILL_POS : STILL;
        return Math.abs(s.x - s.t) < tol && Math.abs(s.v) < tol * 4;
      }),
    wakeAt: Math.min(
      m.nextLook > now && !still ? m.nextLook : Infinity,
      mood === "idle" ? m.nextBlink : Infinity,
      m.blinkUntil || Infinity,
      m.smileUntil || Infinity,
      m.nextSmile || Infinity,
    ),
  };
}
