/** A critically damped spring (k = ω², c = 2ω): reaches its target smoothly, never overshoots. */
export interface Spring {
  x: number;
  v: number;
  t: number;
  k: number;
  c: number;
}

export function spring(x: number, omega: number): Spring {
  return { x, v: 0, t: x, k: omega * omega, c: 2 * omega };
}

/** Advances by `dt` seconds in four sub-steps (stable at 30–120 fps). */
export function stepSpring(s: Spring, dt: number): void {
  const h = dt / 4;
  for (let i = 0; i < 4; i++) {
    const a = s.k * (s.t - s.x) - s.c * s.v;
    s.v += a * h;
    s.x += s.v * h;
  }
}
