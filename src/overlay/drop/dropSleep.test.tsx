// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import { Drop } from "./Drop";

const invoke = vi.fn(async (_cmd: string, _args?: unknown) => undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));

/** Frame callbacks the page asked for; the test plays the display's refresh. */
let queue: FrameRequestCallback[] = [];
const raf = vi.fn((cb: FrameRequestCallback) => {
  queue.push(cb);
  return queue.length;
});

/** Plays `ms` of wall time at 60 Hz; returns how many frames were drawn. */
function play(ms: number): number {
  let drawn = 0;
  for (let t = 0; t < ms; t += 1000 / 60) {
    vi.advanceTimersByTime(1000 / 60);
    const due = queue;
    queue = [];
    for (const cb of due) {
      cb(performance.now());
      drawn++;
    }
  }
  return drawn;
}

beforeEach(() => {
  let seed = 11;
  vi.spyOn(Math, "random").mockImplementation(() => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed / 2147483648;
  });
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "performance"] });
  queue = [];
  raf.mockClear();
  vi.stubGlobal("requestAnimationFrame", raf);
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("Drop frame loop", () => {
  it("sleeps between moves in the notch", () => {
    render(<Drop mood="idle" level={0} translate={false} reduced={false} rest size={26} />);
    play(3000); // settle after mounting
    const drawn = play(30_000);
    // Moving all the time would be 1800 frames; between looks it must not draw at all.
    expect(drawn).toBeLessThan(1800 * 0.6);
    expect(drawn).toBeGreaterThan(0);
  });

  it("wakes at once when a recording starts", () => {
    const view = render(<Drop mood="idle" level={0} translate={false} reduced={false} rest size={26} />);
    // Play until a quarter second passes without a single frame: the loop is asleep.
    let quiet = 0;
    for (let i = 0; i < 600 && quiet < 15; i++) quiet = play(1000 / 60) ? 0 : quiet + 1;
    expect(quiet).toBe(15);
    raf.mockClear();
    act(() => view.rerender(<Drop mood="listening" level={0.5} translate={false} reduced={false} rest={false} size={26} />));
    expect(raf).toHaveBeenCalled();
  });

  it("while only the eyes move, touches only the eyes", () => {
    render(<Drop mood="idle" level={0} translate={false} reduced={false} rest size={26} />);
    play(8000); // the glow, colour and effects have long settled
    const spy = vi.spyOn(Element.prototype, "setAttribute");
    play(20_000);
    const touched = new Set(spy.mock.calls.map((c, i) => {
      const el = spy.mock.contexts[i] as Element;
      return `${[...el.attributes].map((a) => a.name).find((n) => n.startsWith("data-")) ?? el.tagName}.${c[0]}`;
    }));
    expect([...touched].sort()).toEqual(["data-eye.d", "data-eyes.transform"]);
  });

  it("reports how many frames it drew per minute in the notch", () => {
    render(<Drop mood="idle" level={0} translate={false} reduced={false} rest size={26} />);
    invoke.mockClear();
    play(70_000); // asleep between moves, so the report comes with the first frame after a minute
    const logs = invoke.mock.calls.filter((c) => c[0] === "ui_log").map((c) => (c[1] as { message: string }).message);
    expect(logs.some((m) => /^timing: drop frames while resting per_min=\d+$/.test(m))).toBe(true);
  });
});
