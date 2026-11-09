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

  it("five_agents_show_three_dots_and_plus", () => {
    const rows = ["a", "b", "c", "d", "e"].map((id) => row(id, "working"));
    const shown = agentShow(view(rows), "hidden", false);
    expect(shown).toEqual({ kind: "ears", dot: "working", dots: ["working", "working", "working"], more: true });
    const four = agentShow(view(rows.slice(0, 4)), "hidden", false);
    expect(four).toEqual({ kind: "ears", dot: "working", dots: ["working", "working", "working", "working"], more: false });
  });
});
