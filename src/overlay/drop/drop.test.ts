import { describe, expect, it } from "vitest";
import { spring, stepSpring } from "./springs";
import { eyePath } from "./eye";
import { clouds, glints, ring, satellite } from "./effects";

/** All y coordinates mentioned in an SVG path, in order. */
function ys(d: string): number[] {
  const n = d.match(/-?\d+(\.\d+)?/g)!.map(Number);
  return n.filter((_, i) => i % 2 === 1);
}

describe("springs", () => {
  it("critically damped spring reaches the target without overshoot", () => {
    const s = spring(0, 7);
    s.t = 1;
    let max = 0;
    for (let i = 0; i < 120; i++) {
      stepSpring(s, 1 / 60);
      max = Math.max(max, s.x);
    }
    expect(s.x).toBeGreaterThan(0.999);
    expect(max).toBeLessThanOrEqual(1);
  });
});

describe("eyes", () => {
  it("closed eye is a thin crescent, open eye is tall", () => {
    const h = (d: string) => Math.max(...ys(d)) - Math.min(...ys(d));
    expect(h(eyePath(38, 55, 1, 0))).toBeLessThan(4);
    expect(h(eyePath(38, 55, 0, 0))).toBeGreaterThan(20);
  });

  it("smile lifts the eye above its centre", () => {
    for (const y of ys(eyePath(38, 55, 0, 1))) expect(y).toBeLessThanOrEqual(55 + 2.5);
  });
});

describe("effects", () => {
  it("satellite makes one lap and fades out", () => {
    const head = (t: number) => satellite(t)[0];
    expect(head(0).opacity).toBe(0);
    expect(head(0.85).opacity).toBeGreaterThan(0.5);
    expect(head(1.7).opacity).toBeCloseTo(0, 5);
    const fronts = new Set([0.3, 0.6, 0.9, 1.2].map((t) => head(t).front));
    expect(fronts.size).toBe(2);
  });

  it("glints twinkle one after another", () => {
    const g = glints(0.6);
    expect(g[0].opacity).toBeGreaterThan(g[2].opacity);
    expect(glints(2.5).every((x) => x.opacity === 0)).toBe(true);
  });

  it("clouds appear in sequence and vanish by 3.1 s", () => {
    const early = clouds(0.3);
    expect(early[0].opacity).toBeGreaterThan(early[2].opacity);
    expect(clouds(3.1).every((c) => c.opacity < 0.01)).toBe(true);
  });

  it("clouds float left of the character, clear of the camera notch", () => {
    for (const t of [0.5, 1, 1.5, 2]) {
      for (const c of clouds(t)) {
        // In character units the notch starts at x ≈ −17 (island left edge ≈ −156); above y = 14 is under the camera.
        expect(c.x + 16 * c.scale, `t=${t}`).toBeLessThan(-20);
        expect(c.x - 16 * c.scale, `t=${t}`).toBeGreaterThan(-145);
        expect(c.y - 10 * c.scale, `t=${t}`).toBeGreaterThan(16);
      }
    }
  });

  it("listening rings are big and bright enough to see", () => {
    expect(ring(0.99, 0.5, 1).r).toBeGreaterThanOrEqual(80);
    expect(ring(0.3, 0.5, 1).opacity).toBeGreaterThan(0.3);
    expect(ring(0.3, 0.5, 0).opacity).toBe(0);
  });
});
