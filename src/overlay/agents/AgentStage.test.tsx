// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";
import { Stage } from "../Stage";
import { useApp } from "../../lib/store";
import type { AgentRow, AgentView } from "../../lib/types";

const invoke = vi.hoisted(() => vi.fn(async (..._a: unknown[]) => undefined));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

beforeEach(() => useApp.setState({ lang: "en" }));

const notch = { width: 179, height: 32 };
const row: AgentRow = {
  key: { agent: "codex", id: "t" }, agent: "codex", project: "shop", state: "done",
  plan: [], actions: [], summary: "Fixed it", seen: false,
};
const hidden = { kind: "hidden" } as const;
const stage = (agents: AgentView, rest: boolean, hover = false) =>
  render(<Stage state={hidden} rest={rest} notch={notch} level={0} now={0} agents={agents} hover={hover} />).container;

describe("Stage with agents", () => {
  it("a card opens the island even when the ears are off (a full-screen app)", () => {
    const c = stage({ rows: [row], card: { kind: "done", key: row.key }, paused: false }, false);
    expect(c.querySelector("[data-island]")?.getAttribute("data-view")).toBe("agent");
    expect(c.querySelector("[data-agent-title]")?.textContent?.replace(/\s+/g, " ")).toBe("shop Codex finished");
    expect(c.querySelector("[data-draft-panel]")).toBeNull();
  });

  it("tells Utter where the buttons and the Drop are, and nothing once closed", () => {
    // The user 2026-11-10: only right over these may the window take clicks (the stuck bar).
    invoke.mockClear();
    const zones = () => invoke.mock.calls.filter((c) => c[0] === "agent_hit_zones").map((c) => (c[1] as { zones: unknown[] }).zones);
    const { rerender } = render(
      <Stage state={hidden} rest={true} notch={notch} level={0} now={0} agents={{ rows: [row], card: { kind: "done", key: row.key }, paused: false }} />,
    );
    expect(zones().slice(-1)[0]).toHaveLength(3); // Open, OK, the Drop
    rerender(<Stage state={hidden} rest={true} notch={notch} level={0} now={0} agents={{ rows: [row], card: null, paused: false }} />);
    expect(zones().slice(-1)[0]).toEqual([]);
  });

  it("dots sit in the ears at rest, nothing without rows", () => {
    const c = stage({ rows: [row], card: null, paused: false }, true);
    expect(c.querySelector("[data-island]")?.getAttribute("data-view")).toBe("rest");
    expect(c.querySelector("[data-agent-dot='done']")).not.toBeNull();
    // The user 2026-11-10: the right ear always shows all three colours, the current one shimmers.
    const ear = [...c.querySelectorAll("[data-agent-ear-dot]")] as HTMLElement[];
    expect(ear.map((d) => d.getAttribute("data-agent-ear-dot"))).toEqual(["working", "waiting", "done"]);
    expect(ear.map((d) => d.hasAttribute("data-pulse"))).toEqual([false, false, true]);
    expect(ear.every((d) => d.style.opacity === "" || d.style.opacity === "1")).toBe(true);
    // A corner (the user 2026-11-10): the second right of the first, the third under the first.
    const at = ear.map((d) => [parseFloat(d.style.left), parseFloat(d.style.top)]);
    expect(at[1][0]).toBeGreaterThan(at[0][0]);
    expect(at[1][1]).toBe(at[0][1]);
    expect(at[2][0]).toBe(at[0][0]);
    expect(at[2][1]).toBeGreaterThan(at[0][1]);
    // Smaller, so it does not cover the Drop's eye.
    expect((c.querySelector("[data-agent-dot]") as HTMLElement).style.width).toBe("7px");
    const none = stage({ rows: [], card: null, paused: false }, true);
    expect(none.querySelector("[data-agent-ears]")).toBeNull();
  });

  it("connected but idle: three colours in the ear, no dot on the Drop, nothing shimmers", () => {
    const c = stage({ rows: [], card: null, paused: false, connected: true }, true);
    expect(c.querySelectorAll("[data-agent-ear-dot]")).toHaveLength(3);
    expect(c.querySelector("[data-agent-dot]")).toBeNull();
    expect(c.querySelector("[data-pulse]")).toBeNull();
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
    expect(c.querySelector("[data-island]")?.getAttribute("data-view")).toBe("agent");
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
      expect(c.querySelector("[data-island]")?.getAttribute("data-view")).toBe("agent");
      act(() => vi.advanceTimersByTime(3000));
      // The line fades out (still in the tree while it leaves); the island is back to the ears.
      expect(c.querySelector("[data-island]")?.getAttribute("data-view")).toBe("rest");
    } finally {
      vi.useRealTimers();
    }
  });

  it("every poke tickles the Drop, and clicking never selects the island's text", () => {
    const c = stage({ rows: [], card: null, paused: false }, true);
    const island = c.querySelector("[data-island]") as HTMLElement;
    expect(island.style.userSelect).toBe("none");
    const drop = c.querySelector("[data-drop-place]") as HTMLElement;
    expect(drop.getAttribute("data-tickle")).toBe("0");
    fireEvent.click(drop);
    expect(drop.getAttribute("data-tickle")).toBe("1");
    fireEvent.click(drop);
    expect(drop.getAttribute("data-tickle")).toBe("2");
  });

  it("pokes during a recording do nothing", () => {
    const rec = { kind: "recording", mode: "Dictation", style: "Business", started_at_ms: 0, warn: false, pending: 0 } as const;
    const c = render(<Stage state={rec} rest={true} notch={notch} level={0} now={0} />).container;
    const drop = c.querySelector("[data-drop-place]") as HTMLElement;
    for (let i = 0; i < 6; i++) fireEvent.click(drop);
    expect(c.querySelector("[data-agent-note]")).toBeNull();
  });
});
