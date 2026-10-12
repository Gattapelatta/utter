// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { RestDrop } from "./RestDrop";

type Call = { el: Element; keyframes: Keyframe[]; options: KeyframeAnimationOptions };
let calls: Call[] = [];

beforeEach(() => {
  vi.useFakeTimers();
  calls = [];
  let seed = 5;
  vi.spyOn(Math, "random").mockImplementation(() => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed / 2147483648;
  });
  vi.stubGlobal("requestAnimationFrame", vi.fn());
  Element.prototype.animate = function (this: Element, keyframes: Keyframe[], options: KeyframeAnimationOptions) {
    calls.push({ el: this, keyframes, options });
    return { cancel() {}, finish() {}, onfinish: null } as unknown as Animation;
  } as Element["animate"];
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const looks = () => calls.filter((c) => c.el.hasAttribute("data-rest-gaze"));
const smiles = () => calls.filter((c) => c.el.hasAttribute("data-rest-smile"));
const blinks = () => calls.filter((c) => c.el.hasAttribute("data-rest-open"));

describe("RestDrop", () => {
  it("never asks the page for frames: the compositor moves it", () => {
    render(<RestDrop start={{ x: 0, y: 0 }} reduced={false} />);
    vi.advanceTimersByTime(120_000);
    expect(requestAnimationFrame).not.toHaveBeenCalled();
    expect(looks().length).toBeGreaterThan(20);
    expect(blinks().filter((c) => (c.keyframes[0] as { transform?: string }).transform).length).toBeGreaterThan(20);
  });

  it("looks around every 2 to 5 s, each look starting where the last one ended", () => {
    const at: number[] = [];
    render(<RestDrop start={{ x: 3, y: -1 }} reduced={false} />);
    for (let t = 0; t < 60_000; t += 100) {
      const n = looks().length;
      vi.advanceTimersByTime(100);
      if (looks().length > n) at.push(t);
    }
    for (let i = 1; i < at.length; i++) {
      expect(at[i] - at[i - 1]).toBeGreaterThanOrEqual(2000 - 100);
      expect(at[i] - at[i - 1]).toBeLessThanOrEqual(5000 + 100);
    }
    const l = looks();
    expect(l[0].keyframes[0]).toEqual({ transform: `translate(${3 * (48 / 74)}px, ${-1 * (48 / 74)}px)` });
    for (let i = 1; i < l.length; i++) expect(l[i].keyframes[0]).toEqual(l[i - 1].keyframes[1]);
  });

  it("starts with the eyes where the full Drop left them", () => {
    const { container } = render(<RestDrop start={{ x: 8, y: 2 }} reduced={false} />);
    const gaze = container.querySelector("[data-rest-gaze]") as HTMLElement;
    expect(gaze.style.transform).toBe(`translate(${8 * (48 / 74)}px, ${2 * (48 / 74)}px)`);
  });

  it("smiles every 30 to 60 s", () => {
    render(<RestDrop start={{ x: 0, y: 0 }} reduced={false} />);
    vi.advanceTimersByTime(300_000);
    expect(smiles().length).toBeGreaterThanOrEqual(5);
    expect(smiles().length).toBeLessThanOrEqual(10);
  });

  it("with reduced motion neither looks around nor smiles", () => {
    render(<RestDrop start={{ x: 0, y: 0 }} reduced />);
    vi.advanceTimersByTime(300_000);
    expect(looks()).toHaveLength(0);
    expect(smiles()).toHaveLength(0);
  });

  it("reports where it is looking", () => {
    const gaze = { current: { x: 0, y: 0 } };
    render(<RestDrop start={{ x: 0, y: 0 }} reduced={false} gazeRef={gaze} />);
    vi.advanceTimersByTime(6000);
    const last = looks()[looks().length - 1].keyframes[1] as { transform: string };
    expect(last.transform).toBe(`translate(${gaze.current.x * (48 / 74)}px, ${gaze.current.y * (48 / 74)}px)`);
  });
});
