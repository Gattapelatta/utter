// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";
import { Stage } from "../Stage";
import { useApp } from "../../lib/store";
import type { AgentRow, AgentView } from "../../lib/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

beforeEach(() => useApp.setState({ lang: "en" }));

const notch = { width: 179, height: 32 };
const row: AgentRow = {
  key: { agent: "codex", id: "t" }, agent: "codex", project: "shop", state: "done",
  steps: null, actions: [], summary: "Fixed it", seen: false,
};
const hidden = { kind: "hidden" } as const;
const stage = (agents: AgentView, rest: boolean, hover = false) =>
  render(<Stage state={hidden} rest={rest} notch={notch} level={0} now={0} agents={agents} hover={hover} />).container;

describe("Stage with agents", () => {
  it("a card opens the island even when the ears are off (a full-screen app)", () => {
    const c = stage({ rows: [row], card: { kind: "done", key: row.key }, paused: false }, false);
    expect(c.querySelector("[data-island]")?.getAttribute("data-view")).toBe("window");
    expect(c.textContent).toContain("shop · Codex finished");
    expect(c.querySelector("[data-draft-panel]")).toBeNull();
  });

  it("dots sit in the ears at rest, nothing without rows", () => {
    const c = stage({ rows: [row], card: null, paused: false }, true);
    expect(c.querySelector("[data-island]")?.getAttribute("data-view")).toBe("rest");
    expect(c.querySelector("[data-agent-dot='done']")).not.toBeNull();
    expect(c.querySelectorAll("[data-agent-ear-dot]")).toHaveLength(1);
    const none = stage({ rows: [], card: null, paused: false }, true);
    expect(none.querySelector("[data-agent-ears]")).toBeNull();
  });

  it("the state dot is drawn in front of the Drop, not behind it", () => {
    // The user 2026-11-10: half of the blue dot hid behind the Drop in the ear.
    const c = stage({ rows: [{ ...row, state: "working" }], card: null, paused: false }, true);
    const dot = c.querySelector("[data-agent-ears]") as HTMLElement;
    const drop = c.querySelector("[data-drop-place]") as HTMLElement;
    // Later in the tree = painted on top (both are positioned, no z-index).
    expect(drop.compareDocumentPosition(dot) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("hovering the ears opens the list", () => {
    const c = stage({ rows: [row], card: null, paused: false }, true, true);
    expect(c.querySelector("[data-island]")?.getAttribute("data-view")).toBe("window");
    expect(c.querySelector("[data-agent-row]")).not.toBeNull();
  });

  it("a dictation hides the agents", () => {
    const rec = { kind: "recording", mode: "Dictation", style: "Business", started_at_ms: 0, warn: false, pending: 0 } as const;
    const c = render(
      <Stage state={rec} rest={true} notch={notch} level={0} now={0} agents={{ rows: [row], card: { kind: "done", key: row.key }, paused: false }} />,
    ).container;
    expect(c.querySelector("[data-agent-panel]")).toBeNull();
    expect(c.querySelector("[data-agent-ears]")).toBeNull();
  });
});

import { act, fireEvent } from "@testing-library/react";

describe("easter egg through the stage", () => {
  it("five pokes at the Drop open the island with the dizzy line, gone after 3 s", () => {
    vi.useFakeTimers();
    try {
      const c = stage({ rows: [], card: null, paused: false }, true);
      const drop = c.querySelector("[data-drop-place]") as HTMLElement;
      for (let i = 0; i < 4; i++) fireEvent.click(drop);
      expect(c.querySelector("[data-agent-note]")).toBeNull();
      fireEvent.click(drop);
      expect(c.querySelector("[data-agent-note]")?.textContent).toBe("Too many pokes. Give me a sec — back in three seconds.");
      expect(c.querySelector("[data-island]")?.getAttribute("data-view")).toBe("window");
      act(() => vi.advanceTimersByTime(3000));
      // The line fades out (still in the tree while it leaves); the island is back to the ears.
      expect(c.querySelector("[data-island]")?.getAttribute("data-view")).toBe("rest");
    } finally {
      vi.useRealTimers();
    }
  });

  it("pokes during a recording do nothing", () => {
    const rec = { kind: "recording", mode: "Dictation", style: "Business", started_at_ms: 0, warn: false, pending: 0 } as const;
    const c = render(<Stage state={rec} rest={true} notch={notch} level={0} now={0} />).container;
    const drop = c.querySelector("[data-drop-place]") as HTMLElement;
    for (let i = 0; i < 6; i++) fireEvent.click(drop);
    expect(c.querySelector("[data-agent-note]")).toBeNull();
  });
});
