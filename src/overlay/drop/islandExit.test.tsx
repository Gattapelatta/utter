// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render } from "@testing-library/react";
import { Stage } from "../Stage";
import { useApp } from "../../lib/store";
import type { OverlayState } from "../../lib/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

const notch = { width: 185, height: 32 };
const rec: OverlayState = { kind: "recording", mode: "Dictation", style: "Business", started_at_ms: 0, warn: false, pending: 0 };
const hidden: OverlayState = { kind: "hidden" };
/** Width of the black shape from its clip-path inset. */
const width = (el: Element | null) => {
  const m = (el as HTMLElement | null)?.style.clipPath.match(/inset\(0px ([\d.]+)px/);
  return m ? 360 - 2 * Number(m[1]) : NaN;
};
const shape = (c: HTMLElement) => c.querySelector("[data-shape]");
const wait = (ms: number) => act(() => new Promise((r) => setTimeout(r, ms)));
const stage = (state: OverlayState, rest: boolean) => <Stage state={state} rest={rest} notch={notch} level={0} now={0} />;

beforeEach(() => {
  useApp.setState({ lang: "ru" });
});

describe("island and ears", () => {
  it("folds into the ears, not into nothing", async () => {
    const { container, rerender } = render(stage(rec, true));
    await wait(900);
    expect(width(shape(container))).toBeCloseTo(360, 0);
    rerender(stage(hidden, true));
    const samples: number[] = [];
    for (let i = 0; i < 9; i++) {
      await wait(80);
      samples.push(width(shape(container)));
    }
    for (let i = 1; i < samples.length; i++) expect(samples[i], samples.join(", ")).toBeLessThanOrEqual(samples[i - 1] + 0.01);
    for (const s of samples) expect(s, samples.join(", ")).toBeGreaterThanOrEqual(257 - 0.01);
    await wait(300);
    expect(width(shape(container))).toBeCloseTo(257, 0);
  });

  it("folds into the notch and goes away when the ears are off", async () => {
    const { container, rerender } = render(stage(rec, false));
    await wait(900);
    rerender(stage(hidden, false));
    await wait(160);
    const mid = width(shape(container));
    expect(mid).toBeGreaterThan(185);
    expect(mid).toBeLessThan(360);
    await wait(900);
    expect(shape(container)).toBeNull();
  });

  it("grows out of the ears", async () => {
    const { container, rerender } = render(stage(hidden, true));
    await wait(900);
    expect(width(shape(container))).toBeCloseTo(257, 0);
    rerender(stage(rec, true));
    await wait(40);
    const first = width(shape(container));
    expect(first).toBeGreaterThanOrEqual(257 - 0.01);
    await wait(900);
    expect(width(shape(container))).toBeCloseTo(360, 0);
  });

  it("shows the Drop in the ear with nothing on the right", async () => {
    const { container } = render(stage(hidden, true));
    await wait(700);
    expect(container.querySelector("svg")).not.toBeNull();
    expect(container.querySelector("[data-timer]")).toBeNull();
  });
});
