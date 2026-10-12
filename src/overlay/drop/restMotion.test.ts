import { describe, expect, it } from "vitest";
import { blinkFrames, gazeMove, smileFrames, UNIT_PX } from "./restMotion";

const num = (t: string, fn: string) => Number(t.match(new RegExp(`${fn}\\(([-\\d.e]+)`))![1]);

describe("compositor motion of the Drop in the notch", () => {
  it("moves the eyes with the gaze spring (ω 5.5), in pixels of the 48 px Drop", () => {
    const m = gazeMove({ x: 0, y: 0 }, { x: -8, y: 2 });
    expect(m.keyframes).toEqual([{ transform: "translate(0px, 0px)" }, { transform: `translate(${-8 * UNIT_PX}px, ${2 * UNIT_PX}px)` }]);
    expect(m.easing.startsWith("linear(0, ")).toBe(true);
    // A critically damped ω 5.5 spring settles to 0.1 % in about 1.7 s.
    expect(m.duration).toBeGreaterThan(1400);
    expect(m.duration).toBeLessThan(2000);
    expect(UNIT_PX).toBeCloseTo(48 / 74, 6);
  });

  it("blinks with the blink spring: closes almost fully and opens again", () => {
    const b = blinkFrames();
    const scales = b.keyframes.map((k) => num(k.transform, "scaleY"));
    expect(scales[0]).toBe(1);
    expect(scales[scales.length - 1]).toBe(1);
    // As deep as the full Drop's blink: the ω 22 lid gets ~63 % shut in its 90 ms hold.
    expect(Math.min(...scales)).toBeGreaterThan(0.38);
    expect(Math.min(...scales)).toBeLessThan(0.48);
    expect(b.duration).toBeGreaterThan(250);
    expect(b.duration).toBeLessThan(700);
    for (let i = 1; i < b.keyframes.length; i++) expect(b.keyframes[i].offset).toBeGreaterThan(b.keyframes[i - 1].offset);
  });

  it("smiles with the smile spring (ω 7): eyes cross into ^ ^ and back after 1.6 s", () => {
    const s = smileFrames();
    const open = s.open.map((k) => k.opacity);
    const smile = s.smile.map((k) => k.opacity);
    expect([open[0], smile[0]]).toEqual([1, 0]);
    expect([open[open.length - 1], smile[smile.length - 1]]).toEqual([1, 0]);
    expect(Math.max(...smile)).toBeGreaterThan(0.95);
    for (let i = 0; i < open.length; i++) expect(open[i] + smile[i]).toBeCloseTo(1, 6);
    expect(s.duration).toBeGreaterThan(1600 + 600);
    expect(s.duration).toBeLessThan(1600 + 2400);
  });
});
