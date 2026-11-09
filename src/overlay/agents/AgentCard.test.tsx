// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { AgentRow, SessionKey } from "../../lib/types";

const invoke = vi.fn(async () => undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...(a as [])) }));

const { AgentCard, AgentList } = await import("./AgentCard");

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
  steps: [2, 4],
  actions: ["Читает a.rs", "Правит billing.ts"],
  summary: "Исправил округление в billing.ts",
  seen: false,
};
const notch = { width: 179, height: 32 };

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
    expect(screen.getByText(/shop · Claude Code/)).toBeTruthy();
    fireEvent.click(screen.getByText("Always"));
    expect(invoke).toHaveBeenCalledWith("agent_permission", { id: 7, choice: "always" });
    fireEvent.click(screen.getByText("Deny"));
    expect(invoke).toHaveBeenCalledWith("agent_permission", { id: 7, choice: "deny" });
    fireEvent.click(screen.getByText("Allow"));
    expect(invoke).toHaveBeenCalledWith("agent_permission", { id: 7, choice: "allow" });
  });

  it("always_hidden_without_suggestion", () => {
    render(<AgentCard card={{ kind: "permission", id: 7, key, tool: "Edit", detail: "billing.ts", can_always: false }} rows={[row]} notch={notch} />);
    expect(screen.queryByText("Always")).toBeNull();
    expect(screen.getByText("Allow")).toBeTruthy();
  });

  it("done_card_shows_summary_and_open", () => {
    render(<AgentCard card={{ kind: "done", key }} rows={[row]} notch={notch} />);
    expect(screen.getByText("shop · Claude Code finished")).toBeTruthy();
    expect(screen.getByText("Исправил округление в billing.ts")).toBeTruthy();
    fireEvent.click(screen.getByText("Open"));
    expect(invoke).toHaveBeenCalledWith("agent_open", { key });
    fireEvent.click(screen.getByText("OK"));
    expect(invoke).toHaveBeenCalledWith("agent_dismiss");
  });

  it("waiting_card_says_it_waits", () => {
    render(<AgentCard card={{ kind: "waiting", key }} rows={[{ ...row, state: "waiting" }]} notch={notch} />);
    expect(screen.getByText("shop · Claude Code is waiting for you")).toBeTruthy();
  });

  it("after_pause_card_counts", () => {
    render(<AgentCard card={{ kind: "after_pause", done: 2, waiting: 1 }} rows={[]} notch={notch} />);
    expect(screen.getByText("While paused: 2 finished, 1 waiting for you")).toBeTruthy();
    expect(screen.queryByText("OK")).toBeNull();
  });

  it("list_rows_show_step_and_actions_and_open", () => {
    render(<AgentList rows={[{ ...row, state: "working" }]} notch={notch} />);
    expect(screen.getByText("step 2 of 4")).toBeTruthy();
    expect(screen.getByText("Правит billing.ts")).toBeTruthy();
    fireEvent.click(screen.getByText("shop"));
    expect(invoke).toHaveBeenCalledWith("agent_open", { key });
  });
});
