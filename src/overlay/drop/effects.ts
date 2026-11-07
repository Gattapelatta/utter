/** Success/error effects around the character, as pure functions of time (seconds). */

export const clamp01 = (v: number) => Math.max(0, Math.min(1, v));
export const easeInOut = (p: number) => (p < 0.5 ? 2 * p * p : 1 - Math.pow(-2 * p + 2, 2) / 2);
const bell = (p: number) => (p <= 0 || p >= 1 ? 0 : Math.sin(Math.PI * p));

export const SATELLITE_SECS = 1.7;
export const CLOUDS_SECS = 3.1;
const TAIL = 7;

/** A glowing dot with a short tail circling the body once; `front` = drawn over the body. */
export function satellite(t: number) {
  const p = clamp01(t / SATELLITE_SECS);
  const env = bell(p);
  return Array.from({ length: TAIL }, (_, i) => {
    const a = -Math.PI / 2 + easeInOut(p) * Math.PI * 2.15 - i * 0.12;
    const front = Math.sin(a) > 0;
    return {
      x: 50 + Math.cos(a) * 48,
      y: 54 + Math.sin(a) * 15,
      r: 1.9 - i * 0.2,
      opacity: env * (1 - i / TAIL) * (front ? 1 : 0.4),
      front,
    };
  });
}

const GLINTS: [number, number, number][] = [
  [84, 22, 5],
  [93, 36, 3.4],
  [73, 11, 3.8],
];

/** Three sparkles by the head, one after another. */
export function glints(t: number) {
  return GLINTS.map(([x, y, s], i) => {
    const q = clamp01((t - 0.35 - i * 0.18) / 0.95);
    const e = bell(q);
    return { x, y, scale: s * e, rotate: q * 40, opacity: e };
  });
}

/** [x, y, scale, delay, drift direction] in character units. Left of the character, at
 *  head height: above the head is the camera notch, where nothing can be shown. */
const CLOUDS: [number, number, number, number, number][] = [
  [-118, 40, 1.0, 0, 1],
  [-82, 34, 1.35, 0.22, -1],
  [-50, 44, 0.9, 0.44, 1],
];

/** Three small grey clouds beside the head that drift and dissolve. */
export function clouds(t: number) {
  return CLOUDS.map(([x, y, scale, delay, dir], i) => {
    const p = clamp01((t - delay) / 2.6);
    const e = Math.min(1, p * 5) * Math.min(1, (1 - p) * 4);
    const rise = (1 - easeInOut(Math.min(1, p * 4))) * 4;
    return { x: x + dir * Math.sin(t * 0.9 + i) * 1.6, y: y - rise, scale, opacity: e * 0.95 };
  });
}

/** The two tiny drops under the middle cloud. */
export function drops(t: number) {
  return [0, 1].map((i) => {
    const q = (t * 0.85 + i * 0.5) % 1;
    return { dy: q * 5, opacity: bell(q) * clamp01(t * 2) * clamp01((3.2 - t) * 2) };
  });
}

/**
 * One listening ring at cycle phase `p` (0..1) for voice envelope `env`; `visible` fades
 * the whole effect in and out. Radius in character units.
 */
export function ring(p: number, env: number, visible: number): { r: number; opacity: number } {
  const opacity = visible * Math.pow(1 - p, 1.4) * Math.min(1, p * 6) * (0.35 + env * 0.5);
  return { r: 38 + easeInOut(p) * 44, opacity };
}

/** Half-angle of each side arc: at the largest ring (82 units) the arc stays below the notch. */
const ARC_DEG = 30;

/** Two arcs of a circle around the body (cx 50, cy 54), one on each side, never above it. */
export function arcPath(r: number): string {
  const a = (ARC_DEG * Math.PI) / 180;
  const dx = r * Math.cos(a);
  const dy = r * Math.sin(a);
  const f = (n: number) => n.toFixed(2);
  const R = `${f(r)} ${f(r)}`;
  return (
    `M${f(50 + dx)} ${f(54 - dy)} A${R} 0 0 1 ${f(50 + dx)} ${f(54 + dy)} ` +
    `M${f(50 - dx)} ${f(54 + dy)} A${R} 0 0 1 ${f(50 - dx)} ${f(54 - dy)}`
  );
}
