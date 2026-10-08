import { describe, expect, it } from "vitest";
import { contentVariants, islandVariants } from "./islandMotion";

const fns = (t: string) => [...t.matchAll(/([a-zA-Z]+)\(/g)].map((m) => m[1]);
type V = { transform: string; transitionEnd?: { transform: string } };

describe("island motion", () => {
  for (const notch of [{ width: 179, height: 32 }, null]) {
    it(`grows to full size with interpolable transforms (${notch ? "notch" : "no notch"})`, () => {
      const v = islandVariants(notch);
      const hidden = v.hidden as V;
      const visible = v.visible as V;
      const leave = (v.leave as (c: boolean) => V)(false);
      // Motion turns a mismatched pair (e.g. "scaleX(..)" → "none") into scale 0: the island vanished.
      expect(fns(visible.transform)).toEqual(fns(hidden.transform));
      expect(fns(leave.transform)).toEqual(fns(hidden.transform));
      expect(visible.transform).toBe("translateY(0px) scaleX(1) scaleY(1)");
      expect(visible.transitionEnd?.transform).toBe("none");
    });
  }

  it("starts from the notch's own size", () => {
    expect((islandVariants({ width: 180, height: 32 }).hidden as V).transform).toBe("translateY(0px) scaleX(0.5) scaleY(0.307692)");
  });

  it("folds back into the notch slowly and evenly, not with a spring", () => {
    const leave = (islandVariants({ width: 179, height: 32 }).leave as (c: boolean) => { transition: { type?: string; duration: number; ease: number[] } })(false);
    expect(leave.transition.type).toBe("tween");
    expect(leave.transition.duration).toBe(0.6);
    expect(leave.transition.ease).toEqual([0.4, 0, 0.2, 1]);
  });

  it("the character and timer fade out together with the fold, not before it", () => {
    const leave = contentVariants.leave as { opacity: number; transition: { duration: number } };
    expect(leave.opacity).toBe(0);
    // A 0.12 s fade made the whole pill look like it vanished at once on a dark desktop.
    expect(leave.transition.duration).toBeGreaterThanOrEqual(0.45);
    expect(leave.transition.duration).toBeLessThanOrEqual(0.6);
  });
});
