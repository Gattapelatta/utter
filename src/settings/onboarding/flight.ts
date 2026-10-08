export interface Spring {
  /** Seconds to (nearly) reach the target. */
  response: number;
  /** 1 = no overshoot; lower = more bounce. */
  damping: number;
}

/** Onboarding's short hops between the centre and the top: Apple-style, a slight overshoot. */
export const APPLE: Spring = { response: 0.55, damping: 0.72 };
/** Long glides across the window (logo ↔ About): slower, and barely past the target. */
export const GLIDE: Spring = { response: 0.9, damping: 0.9 };

export interface Flight {
  x: number;
  v: number;
  target: number;
}

export const createFlight = (x: number): Flight => ({ x, v: 0, target: x });

export function stepFlight(f: Flight, dt: number, spring: Spring = APPLE): void {
  const w = (2 * Math.PI) / spring.response;
  // Real time up to 0.1 s per frame (a slow frame must not slow the flight down), in small sub-steps.
  const h = Math.min(dt, 0.1) / 16;
  for (let i = 0; i < 16; i++) {
    f.v += (w * w * (f.target - f.x) - 2 * spring.damping * w * f.v) * h;
    f.x += f.v * h;
  }
}

/** Close enough to rest that the step's text may appear. */
export const settled = (f: Flight) => Math.abs(f.target - f.x) < 1.2 && Math.abs(f.v) < 25;
