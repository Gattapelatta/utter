import { describe, expect, it } from "vitest";
import type { Transition } from "motion/react";
import { barSpring, pillVariants, shake, widthSpring } from "./pillMotion";

type V = Record<string, unknown> & { transition?: Transition & { duration?: number; ease?: number[] } };
const variant = (reduced: boolean, name: string, custom?: unknown): V => {
  const v = pillVariants(reduced)[name];
  return (typeof v === "function" ? (v as (c: unknown) => V)(custom) : v) as V;
};

describe("pill motion", () => {
  it("reduced motion uses opacity only", () => {
    for (const name of ["hidden", "visible", "exit", "cancel"]) {
      const v = variant(true, name);
      expect(Object.keys(v).filter((k) => k !== "transition")).toEqual(["opacity"]);
    }
    expect(variant(true, "visible").transition?.duration).toBe(0.15);
  });

  it("appear matches the spec", () => {
    expect(variant(false, "hidden")).toEqual({ opacity: 0, transform: "translateY(8px) scale(0.96)", filter: "blur(4px)" });
    const v = variant(false, "visible");
    expect(v).toMatchObject({ opacity: 1, transform: "translateY(0px) scale(1)", filter: "blur(0px)" });
    expect(v.transition).toMatchObject({ duration: 0.22, ease: [0.2, 0.9, 0.3, 1.15] });
    expect(v.transitionEnd).toEqual({ filter: "none", transform: "none" });
  });

  it("exit and cancel timings", () => {
    const exit = variant(false, "exit");
    expect(exit).toMatchObject({ opacity: 0, transform: "translateY(8px) scale(1)" });
    expect(exit.transition).toMatchObject({ duration: 0.16, ease: [0.23, 1, 0.32, 1] });
    const cancel = variant(false, "cancel");
    expect(cancel).toMatchObject({ opacity: 0 });
    expect(cancel.transition).toMatchObject({ duration: 0.12 });
  });

  it("leave picks cancel or exit", () => {
    expect(variant(false, "leave", true)).toEqual(variant(false, "cancel"));
    expect(variant(false, "leave", false)).toEqual(variant(false, "exit"));
  });

  it("springs match the spec", () => {
    expect(widthSpring).toEqual({ type: "spring", bounce: 0, duration: 0.35 });
    expect(barSpring).toEqual({ type: "spring", bounce: 0.15, duration: 0.18 });
    expect(shake).toMatchObject({ type: "spring", bounce: 0.4 });
  });
});
