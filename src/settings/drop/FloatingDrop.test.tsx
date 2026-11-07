// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
const { FloatingDrop, BASE } = await import("./FloatingDrop");
const { useDropStage } = await import("./dropStage");

const wait = (ms: number) => act(() => new Promise((r) => setTimeout(r, ms)));
function slot(kind: string, rect: { left: number; top: number; width: number }) {
  const el = document.createElement("div");
  el.dataset.dropSlot = kind;
  el.getBoundingClientRect = () => ({ ...rect, right: 0, bottom: 0, height: 0, x: rect.left, y: rect.top, toJSON: () => ({}) }) as DOMRect;
  document.body.appendChild(el);
  return el;
}
const where = () => {
  const m = (document.querySelector("[data-floating-drop]") as HTMLElement).style.transform.match(/translate\(([-\d.]+)px, ([-\d.]+)px\) scale\(([-\d.]+)\)/)!;
  return m.slice(1).map(Number);
};

afterEach(() => {
  cleanup();
  document.querySelectorAll("[data-drop-slot]").forEach((e) => e.remove());
});

describe("floating drop", () => {
  it("appears on its slot without flying, then flies to a better slot", async () => {
    slot("logo", { left: 24, top: 52, width: 60 });
    render(<FloatingDrop />);
    await wait(50);
    const [x0, y0, s0] = where();
    expect([x0, y0]).toEqual([24, 52]);
    expect(s0).toBeCloseTo(60 / BASE, 3);
    slot("about", { left: 300, top: 120, width: 140 });
    await wait(60);
    const [x] = where();
    expect(x).toBeGreaterThan(24);
    expect(x).toBeLessThan(300); // in flight
    expect(useDropStage.getState().settled).toBe(false);
    await wait(1500);
    const [x2, y2, s2] = where();
    expect(Math.abs(x2 - 300)).toBeLessThan(1.5);
    expect(Math.abs(y2 - 120)).toBeLessThan(1.5);
    expect(Math.abs(s2 - 140 / BASE)).toBeLessThan(0.02);
    expect(useDropStage.getState().settled).toBe(true);
  });

  it("follows its own slot at once when the slot moves (scroll, resize)", async () => {
    const rect = { left: 24, top: 52, width: 60 };
    slot("logo", rect);
    render(<FloatingDrop />);
    await wait(50);
    rect.top = 10;
    await wait(40);
    expect(where()[1]).toBe(10);
  });

  it("celebrate makes the drop happy for a while", async () => {
    vi.useFakeTimers();
    useDropStage.getState().celebrate();
    expect(useDropStage.getState().mood).toBe("happy");
    vi.advanceTimersByTime(2_300);
    expect(useDropStage.getState().mood).toBe("idle");
    vi.useRealTimers();
  });
});
