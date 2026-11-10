// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";

type Level = number | (() => number);
const stage = vi.hoisted(() => ({ renders: 0, level: null as unknown }));
const handlers = vi.hoisted(() => ({ level: null as null | ((l: number) => void) }));

vi.mock("./Stage", () => ({
  Stage: (p: { level: Level }) => {
    stage.renders++;
    stage.level = p.level;
    return null;
  },
}));
vi.mock("../lib/store", () => ({ useApp: { getState: () => ({ init: () => Promise.resolve(() => {}) }) } }));
vi.mock("../lib/ipc", () => {
  const none = () => Promise.resolve(() => {});
  return {
    events: {
      overlayState: none,
      overlayScreen: none,
      overlayRest: none,
      overlayDraft: none,
      agents: none,
      agentHover: none,
      overlayLevel: (cb: (l: number) => void) => {
        handlers.level = cb;
        return Promise.resolve(() => {});
      },
    },
    ipc: { agentsView: () => new Promise(() => {}), overlayReady: () => Promise.resolve(), uiLog: () => Promise.resolve(), overlayPainted: () => Promise.resolve() },
  };
});

import { Overlay } from "./Overlay";

afterEach(cleanup);

describe("Overlay", () => {
  it("voice levels reach the Drop without redrawing the island", async () => {
    // 30 levels a second while recording: each redraw of the island tree kept ~8 KB in WebKit
    // (the island page grew 55 MB over 7000 levels and never gave it back).
    render(<Overlay />);
    await act(async () => {});
    const before = stage.renders;
    for (let i = 0; i < 100; i++) act(() => handlers.level?.(i % 2 ? 0.04 : 0.16));
    expect(stage.renders).toBe(before);
    expect(typeof stage.level).toBe("function");
    // The Drop reads the latest level (the last one sent was 0.04), scaled so normal speech reaches ~1.
    expect((stage.level as () => number)()).toBeCloseTo(Math.sqrt(0.04) * 2.2, 5);
  });
});
