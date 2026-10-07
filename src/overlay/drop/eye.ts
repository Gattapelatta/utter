/** Eye size in the 100-unit character space (docs/design/drop-prototype.html). */
export const EYE_RX = 7.6;
export const EYE_RY = 11.5;

const K = 1.333;
const mix = (a: number, b: number, t: number) => a + (b - a) * t;

/**
 * One closed shape that morphs between an open oval (lid 0), a closed lid crescent
 * (lid 1) and a smiling "^" arc (smile 1).
 */
export function eyePath(cx: number, cy: number, lid: number, smile: number): string {
  let top = cy - K * EYE_RY * (1 - lid);
  let bot = cy + K * EYE_RY * (1 - 0.8 * lid);
  top = mix(top, cy - K * EYE_RY * 0.55, smile);
  bot = mix(bot, cy - K * EYE_RY * 0.22, smile);
  const y = mix(cy, cy + 2.5, smile);
  const l = cx - EYE_RX;
  const r = cx + EYE_RX;
  const f = (n: number) => n.toFixed(2);
  return `M${f(l)} ${f(y)} C${f(l)} ${f(top)} ${f(r)} ${f(top)} ${f(r)} ${f(y)} C${f(r)} ${f(bot)} ${f(l)} ${f(bot)} ${f(l)} ${f(y)}Z`;
}
