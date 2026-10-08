/** The Drop's move between the centre and the top: an Apple-style spring with a slight overshoot. */
export const RESPONSE = 0.55;
export const DAMPING = 0.72;
const W = (2 * Math.PI) / RESPONSE;

export interface Flight {
  x: number;
  v: number;
  target: number;
}

export const createFlight = (x: number): Flight => ({ x, v: 0, target: x });

export function stepFlight(f: Flight, dt: number): void {
  // Real time up to 0.1 s per frame (a slow frame must not slow the flight down), in small sub-steps.
  const h = Math.min(dt, 0.1) / 16;
  for (let i = 0; i < 16; i++) {
    f.v += (W * W * (f.target - f.x) - 2 * DAMPING * W * f.v) * h;
    f.x += f.v * h;
  }
}

/** Close enough to rest that the step's text may appear. */
export const settled = (f: Flight) => Math.abs(f.target - f.x) < 1.2 && Math.abs(f.v) < 25;
