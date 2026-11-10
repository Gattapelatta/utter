import { describe, expect, it } from "vitest";
import type { AgentRow, AgentView, SessionState } from "../../lib/types";
import { agentShow } from "./agentIsland";

const row = (id: string, state: SessionState, seen = false): AgentRow => ({
  key: { agent: "claude_code", id },
  agent: "claude_code",
  project: `p-${id}`,
  state,
  steps: null,
  actions: [],
  summary: null,
  seen,
});

const view = (rows: AgentRow[], card: AgentView["card"] = null): AgentView => ({ rows, card, paused: false });

describe("agentShow", () => {
  it("dictation_hides_agents", () => {
    const v = view([row("a", "working")], { kind: "done", key: { agent: "claude_code", id: "a" } });
    expect(agentShow(v, "recording", true)).toEqual({ kind: "none" });
    expect(agentShow(v, "processing", false)).toEqual({ kind: "none" });
    expect(agentShow(v, "cancelled", false)).toEqual({ kind: "none" });
  });

  it("card_beats_list", () => {
    const card = { kind: "done" as const, key: { agent: "claude_code" as const, id: "a" } };
    expect(agentShow(view([row("a", "done")], card), "hidden", true)).toEqual({ kind: "card", card });
  });

  it("hover_opens_list", () => {
    const rows = [row("a", "working"), row("b", "waiting")];
    expect(agentShow(view(rows), "hidden", true)).toEqual({ kind: "list", rows });
    expect(agentShow(view(rows), "hidden", false).kind).toBe("ears");
    // Nobody to list: hovering over the plain ears changes nothing.
    expect(agentShow(view([]), "hidden", true)).toEqual({ kind: "none" });
  });

  it("ears_show_the_most_urgent_state_on_the_drop", () => {
    const shown = agentShow(view([row("p", "permission"), row("w", "working"), row("d", "done")]), "hidden", false);
    expect(shown).toEqual({ kind: "ears", dot: "waiting", dots: ["waiting", "working", "done"], more: false });
  });

  it("connected_agent_keeps_the_colours_in_the_ear", () => {
    // The user 2026-11-10: with Claude Code connected the right ear keeps its three colours even
    // when no agent works; the Drop then has no dot.
    expect(agentShow({ ...view([]), connected: true }, "hidden", false)).toEqual({ kind: "ears", dot: null, dots: [], more: false });
    expect(agentShow({ ...view([]), connected: false }, "hidden", false)).toEqual({ kind: "none" });
    expect(agentShow({ ...view([]), connected: true, paused: true }, "hidden", false)).toEqual({ kind: "none" });
    expect(agentShow({ ...view([]), connected: true }, "recording", false)).toEqual({ kind: "none" });
  });

  it("hovering_bare_ears_opens_an_empty_list_when_connected", () => {
    // So the Drop can be poked there: the top strip itself never takes clicks (the stuck menu bar).
    expect(agentShow({ ...view([]), connected: true }, "hidden", true)).toEqual({ kind: "list", rows: [] });
    expect(agentShow({ ...view([]), connected: false }, "hidden", true)).toEqual({ kind: "none" });
  });

  it("five_agents_show_three_dots_and_plus", () => {
    const rows = ["a", "b", "c", "d", "e"].map((id) => row(id, "working"));
    const shown = agentShow(view(rows), "hidden", false);
    expect(shown).toEqual({ kind: "ears", dot: "working", dots: ["working", "working", "working"], more: true });
    const four = agentShow(view(rows.slice(0, 4)), "hidden", false);
    expect(four).toEqual({ kind: "ears", dot: "working", dots: ["working", "working", "working", "working"], more: false });
  });
});

import { DIZZY_MS, dizzyNow, pokes } from "./agentIsland";

describe("pokes", () => {
  it("five_pokes_in_2s_make_dizzy", () => {
    expect(pokes([0, 400, 800, 1200, 1900], 1900)).toBe(true);
  });

  it("four_pokes_do_not", () => {
    expect(pokes([0, 400, 800, 1900], 1900)).toBe(false);
    // Five, but spread over more than 2 s.
    expect(pokes([0, 600, 1200, 1800, 2100], 2100)).toBe(false);
  });

  it("dizzy_ends_after_3s", () => {
    expect(DIZZY_MS).toBe(3000);
    expect(dizzyNow(1000, 3999, "hidden")).toBe(true);
    expect(dizzyNow(1000, 4000, "hidden")).toBe(false);
    expect(dizzyNow(null, 0, "hidden")).toBe(false);
  });

  it("no_dizzy_while_recording", () => {
    expect(dizzyNow(1000, 1500, "recording")).toBe(false);
  });
});

import { DOT_COLOR } from "./agentIsland";

describe("colours (chosen by the user 2026-11-10, the Figma logo, variant F2)", () => {
  it("blue works, coral waits, green is done", () => {
    expect(DOT_COLOR).toEqual({ working: "#1ABCFE", waiting: "#FF7262", done: "#0ACF83" });
  });
});
