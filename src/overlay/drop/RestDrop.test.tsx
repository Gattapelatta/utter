// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { RestDrop } from "./RestDrop";

type Call = { el: Element; keyframes: Keyframe[]; options: KeyframeAnimationOptions };
/** Every time a move plays (a new animation or a replayed one), with its keyframes. */
let calls: Call[] = [];
/** Animations created with Element.animate — each one lives until the page drops it. */
let created = 0;

beforeEach(() => {
  vi.useFakeTimers();
  calls = [];
  let seed = 5;
  vi.spyOn(Math, "random").mockImplementation(() => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed / 2147483648;
  });
  vi.stubGlobal("requestAnimationFrame", vi.fn());
  created = 0;
  Element.prototype.animate = function (this: Element, keyframes: Keyframe[], options: KeyframeAnimationOptions) {
    const el = this;
    let frames = keyframes;
    created++;
    calls.push({ el, keyframes: frames, options });
    const effect = {
      setKeyframes(k: Keyframe[]) {
        frames = k;
      },
      updateTiming() {},
    };
    return {
      effect,
      currentTime: 0,
      cancel() {},
      finish() {},
      play() {
        calls.push({ el, keyframes: frames, options });
      },
      onfinish: null,
    } as unknown as Animation;
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
    const first = (c: Call) => (c.keyframes[0] as { transform: string }).transform;
    const last = (c: Call) => (c.keyframes[c.keyframes.length - 1] as { transform: string }).transform;
    expect(first(l[0])).toBe(`translate(${3 * (48 / 74)}px, ${-1 * (48 / 74)}px)`);
    for (let i = 1; i < l.length; i++) expect(first(l[i])).toBe(last(l[i - 1]));
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
    const l = looks()[looks().length - 1].keyframes;
    const last = l[l.length - 1] as { transform: string };
    expect(last.transform).toBe(`translate(${gaze.current.x * (48 / 74)}px, ${gaze.current.y * (48 / 74)}px)`);
  });

  it("hands back where the eyes are right now when a recording interrupts a look", () => {
    const gaze = { current: { x: 0, y: 0 } };
    const view = render(<RestDrop start={{ x: 0, y: 0 }} reduced={false} gazeRef={gaze} />);
    while (looks().length === 0) vi.advanceTimersByTime(1);
    const l = looks()[0].keyframes;
    const to = (l[l.length - 1] as { transform: string }).transform;
    vi.advanceTimersByTime(250); // a quarter second into a ~1.7 s look
    view.unmount();
    const g = gaze.current;
    const target = to.match(/translate\(([-\d.e]+)px, ([-\d.e]+)px\)/)!.slice(1).map((v) => Number(v) / (48 / 74));
    const p = 1 - (1 + 5.5 * 0.25) * Math.exp(-5.5 * 0.25);
    if (target[0] !== 0) expect(g.x / target[0]).toBeCloseTo(p, 2);
    if (target[1] !== 0) expect(g.y / target[1]).toBeCloseTo(p, 2);
    expect(Math.abs(g.x - target[0]) + Math.abs(g.y - target[1])).toBeGreaterThan(0.5);
  });

  it("replays one animation per kind of move instead of piling up new ones", () => {
    render(<RestDrop start={{ x: 0, y: 0 }} reduced={false} />);
    vi.advanceTimersByTime(300_000);
    expect(looks().length).toBeGreaterThan(50);
    // Measured in a bare WKWebView: a new animation every few seconds grew the page by
    // 34 MB in 3 minutes; replaying one kept it flat. Gaze, blink, and smile (two layers).
    expect(created).toBeLessThanOrEqual(4);
  });
});
