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
  return m ? 420 - 2 * Number(m[1]) : NaN;
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

describe("draft window", () => {
  const draft = { settled: "Привет, слушай", interim: "я" };
  const proc: OverlayState = { kind: "processing", pending: 1, since_ms: 0 };
  const done: OverlayState = { kind: "done", outcome: "pasted", degraded: false };
  const at = (state: OverlayState, d: typeof draft | null) => <Stage state={state} rest={true} notch={notch} level={0} now={7_000} draft={d} />;
  const viewOf = (c: HTMLElement) => c.querySelector("[data-island]")?.getAttribute("data-view");

  it("window_opens_only_with_draft", async () => {
    const { container, rerender } = render(at(rec, null));
    await wait(50);
    expect(viewOf(container)).toBe("open");
    expect(container.querySelector("[data-draft-panel]")).toBeNull();
    rerender(at(rec, draft));
    await wait(50);
    expect(viewOf(container)).toBe("window");
    expect(container.querySelector("[data-draft-panel]")).not.toBeNull();
    rerender(at(proc, draft));
    expect(viewOf(container)).toBe("window");
    expect(container.querySelector("[data-timer]")?.textContent).toBe("0:07");
  });

  it("done_after_window_marks_leaving: field and timer leave together, then go", async () => {
    const { container, rerender } = render(at(proc, draft));
    await wait(50);
    rerender(at(done, null));
    expect(viewOf(container)).toBe("open");
    expect(container.querySelector("[data-draft-panel]"), "still leaving").not.toBeNull();
    expect(container.querySelector("[data-timer]"), "timer leaves with it").not.toBeNull();
    await wait(600);
    expect(container.querySelector("[data-draft-panel]")).toBeNull();
    // The timer fades out in the wing (its slot stays, as in every state).
    expect((container.querySelector("[data-slot]") as HTMLElement).style.opacity).toBe("0");
  });

  it("the field is clipped by the island's shape, so it never hangs below a shrinking island", async () => {
    const { container } = render(at(rec, draft));
    await wait(50);
    const clipBox = container.querySelector("[data-draft-clip]") as HTMLElement;
    expect(clipBox.contains(container.querySelector("[data-draft-panel]"))).toBe(true);
    await wait(700);
    expect(clipBox.style.clipPath).toBe((container.querySelector("[data-shape]") as HTMLElement).style.clipPath);
  });

  it("new_recording_starts_without_old_draft", async () => {
    const { container, rerender } = render(at(proc, draft));
    await wait(50);
    rerender(at(done, null));
    await wait(600);
    rerender(at(rec, null));
    await wait(50);
    expect(viewOf(container)).toBe("open");
    expect(container.querySelector("[data-draft-panel]")).toBeNull();
  });
});
