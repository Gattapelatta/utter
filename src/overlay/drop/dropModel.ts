import { spring, stepSpring, type Spring } from "./springs";

/** What the character is doing (spec §4.2). */
export type Mood = "idle" | "listening" | "thinking" | "happy" | "sad";

export interface ModelInput {
  mood: Mood;
  /** Raw voice level 0..1 as it arrives from the microphone (~30 Hz, spiky). */
  level: number;
  translate: boolean;
  reduced: boolean;
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
}

const LOOK_IDLE: [number, number][] = [[0, 0], [-8, 0], [8, 0], [-6, -5], [6, -5], [0, -5.5], [-8, 2], [8, 2]];
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
  last: number;
  random: () => number;
}

export function createModel(now: number, random: () => number = Math.random): Model {
  return {
    P: {
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
    },
    mood: null,
    moodAt: now,
    phase: 0,
    flow: 0,
    nextLook: 0,
    nextBlink: now + 2600,
    blinkUntil: 0,
    last: now,
    random,
  };
}

function enter(m: Model, mood: Mood, now: number) {
  const P = m.P;
  m.mood = mood;
  m.moodAt = now;
  P.smile.t = 0;
  P.ex.t = 0;
  P.ey.t = 0;
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
    if ((mood === "idle" || mood === "thinking") && !still && now > m.nextLook) {
      const L = mood === "thinking" ? LOOK_THINK : LOOK_IDLE;
      const [x, y] = L[Math.floor(m.random() * L.length)];
      P.ex.t = x;
      P.ey.t = y;
      m.nextLook = now + 1400 + m.random() * 1800;
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
  };
}
