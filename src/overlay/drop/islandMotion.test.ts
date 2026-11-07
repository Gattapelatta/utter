import { describe, expect, it } from "vitest";
import { clip, contentTarget, dropTarget, foldedBox, ISLAND_BOX, leaveTarget, restBox, restDropOffset, shapeTarget, timerTarget, WINDOW_BOX } from "./islandMotion";

const notch = { width: 185, height: 32 };
/** The same string with every number replaced: Motion only interpolates matching templates. */
const template = (s: string) => s.replace(/-?\d+(\.\d+)?(e-?\d+)?/g, "#");
type T = { transition: { type?: string; duration?: number; ease?: number[]; bounce?: number } };

describe("island geometry", () => {
  it("ears are the notch widened by 36 pt each side, as tall as the notch", () => {
    expect(restBox(notch)).toEqual({ w: 257, h: 32, r: 8 });
  });

  it("without a notch the rest is a small plate", () => {
    expect(restBox(null)).toEqual({ w: 36, h: 32, r: 8 });
  });

  it("folds into the notch, or into nothing at the top edge", () => {
    expect(foldedBox(notch)).toEqual({ w: 185, h: 32, r: 8 });
    expect(foldedBox(null)).toEqual({ w: 36, h: 0, r: 0 });
    expect(ISLAND_BOX).toEqual({ w: 360, h: 104, r: 40 });
  });

  it("the Drop sits in the left ear, 5 pt clear of the notch", () => {
    const o = restDropOffset(notch);
    // Island place of the Drop: centre (134, 61). Ear centre: 180 − 92.5 − 18 = 69.5, y = 16.
    expect(o.x).toBeCloseTo(-64.5, 5);
    expect(o.y).toBeCloseTo(-45, 5);
    expect(o.scale).toBeCloseTo(26 / 48, 5);
    const centre = 134 + o.x;
    const half = 13;
    expect(centre - half).toBeGreaterThanOrEqual(180 - 92.5 - 36);
    expect(centre + half).toBeLessThanOrEqual(180 - 92.5 - 5 + 1e-9);
  });

  it("without a notch the Drop sits in the middle of the plate", () => {
    expect(restDropOffset(null)).toEqual({ x: 46, y: -45, scale: 26 / 48 });
  });

  it("every shape uses one clip template", () => {
    const all = [notch, null].flatMap((n) => [clip(restBox(n)), clip(foldedBox(n)), clip(ISLAND_BOX)]);
    for (const c of all) expect(template(c)).toBe(template(all[0]));
    expect(clip(ISLAND_BOX)).toBe("inset(0px 30px 102px 30px round 0px 0px 40px 40px)");
    expect(template(clip(WINDOW_BOX))).toBe(template(all[0]));
  });

  it("the draft window is the whole 420 × 206 base", () => {
    expect(WINDOW_BOX).toEqual({ w: 420, h: 206, r: 30 });
    expect(clip(WINDOW_BOX)).toBe("inset(0px 0px 0px 0px round 0px 0px 30px 30px)");
  });

  it("in the window the Drop sits in the left wing: centre (37, 43), 26 pt", () => {
    // Island place of the Drop in the 420 base: centre (164, 61).
    expect(dropTarget("window", notch, false, false).transform).toBe("translate(-127px, -18px) scale(0.541667)");
    expect(dropTarget("window", notch, false, false).opacity).toBe(1);
  });

  it("in the window the timer shrinks to 13 pt with its right edge 24 pt from the window's", () => {
    expect(timerTarget("open").transform).toBe("translate(0px, 0px) scale(1)");
    // Slot left in the base: 164 + 24 + 44 = 232, 48 wide; 13/21 of it ends at 420 − 24 = 396.
    expect(timerTarget("window").transform).toBe("translate(134.2857px, -18px) scale(0.619048)");
    expect(template(timerTarget("window").transform)).toBe(template(timerTarget("open").transform));
  });

  it("the field and the timer leave together: fade, rise 10, shrink to 0.96, blur 2, 0.3 s", () => {
    expect(leaveTarget).toMatchObject({ opacity: 0, y: -10, scale: 0.96, filter: "blur(2px)" });
    expect((leaveTarget as T).transition.duration).toBe(0.3);
  });

  it("every Drop place uses one transform template", () => {
    const all = (["open", "window", "rest", "folded"] as const).flatMap((v) => [notch, null].map((n) => dropTarget(v, n, false, false).transform));
    for (const t of all) expect(template(t)).toBe(template(all[0]));
    expect(dropTarget("open", notch, false, false).transform).toBe("translate(0px, 0px) scale(1)");
  });
});

describe("island motion", () => {
  it("grows with a spring without overshoot", () => {
    const t = (shapeTarget("open", notch, false, false) as T).transition;
    expect(t.type).toBe("spring");
    expect(t.bounce).toBe(0);
  });

  it("folds into the ears slowly and evenly; Esc is quicker", () => {
    const t = (shapeTarget("rest", notch, false, false) as T).transition;
    expect([t.type, t.duration, t.ease]).toEqual(["tween", 0.6, [0.4, 0, 0.2, 1]]);
    expect((shapeTarget("rest", notch, true, false) as T).transition.duration).toBe(0.35);
  });

  it("the Drop flies with the island", () => {
    expect((dropTarget("open", notch, false, false) as T).transition).toEqual((shapeTarget("open", notch, false, false) as T).transition);
    expect((dropTarget("rest", notch, false, false) as T).transition).toEqual((shapeTarget("rest", notch, false, false) as T).transition);
  });

  it("reduced motion changes shape in a short fade-length step", () => {
    expect((shapeTarget("rest", notch, false, true) as T).transition).toEqual({ duration: 0.15 });
  });

  it("the timer shows only on the open island and fades out with the fold, not before it", () => {
    expect(contentTarget("open").opacity).toBe(1);
    expect(contentTarget("rest").opacity).toBe(0);
    const d = (contentTarget("rest") as T).transition.duration!;
    // A 0.12 s fade made the whole pill look like it vanished at once on a dark desktop.
    expect(d).toBeGreaterThanOrEqual(0.45);
    expect(d).toBeLessThanOrEqual(0.6);
  });
});
