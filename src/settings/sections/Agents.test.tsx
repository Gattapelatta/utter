// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { AgentsStatus, Settings } from "../../lib/types";
import { useApp } from "../../lib/store";

let status: AgentsStatus;
const invoke = vi.fn(async (cmd: string, _args?: unknown): Promise<unknown> => {
  if (cmd === "agents_status") return status;
  return undefined;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const { Agents } = await import("./Agents");

const base: Settings = {
  schema_version: 1,
  record_mode: "Hold",
  insert_method: "Paste",
  speech_language: null,
  cleanup: true,
  paragraphs: false,
  autostart: false,
  theme: "Dark",
  ui_language: "Ru",
  translate_style: "Business",
  provider: "Gemini",
  hotkeys: { dictation: "RightAlt", translate: "RightMeta" },
  history_enabled: true,
  onboarding_done: true,
  notch_drop: true,
  agents: { claude_code: false, codex: false, summary: true },
  agents_codex_prev_notify: null,
};
const saved = () => invoke.mock.calls.filter((c) => c[0] === "save_settings").map((c) => (c[1] as { settings: Settings }).settings);

beforeEach(() => {
  invoke.mockClear();
  status = { detected: { claude_code: true, codex: true }, claude_code: { kind: "off" }, codex: { kind: "off" } };
  useApp.setState({ settings: { ...base }, lang: "ru", theme: "dark", systemDark: true });
});
afterEach(cleanup);

async function mount() {
  render(<Agents />);
  await act(async () => {});
}

describe("Agents section", () => {
  it("toggles_call_update_settings", async () => {
    await mount();
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Агенты");
    expect(screen.getByText("Капля скажет, когда Claude Code закончил или ждёт тебя, и даст разрешить из острова")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: "Claude Code" })));
    expect(saved().slice(-1)[0].agents).toEqual({ claude_code: true, codex: false, summary: true });
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: "Codex" })));
    expect(saved().slice(-1)[0].agents?.codex).toBe(true);
    // The status is read again after a change: the files were just written.
    expect(invoke.mock.calls.filter((c) => c[0] === "agents_status").length).toBeGreaterThan(1);
  });

  it("missing_agent_disables_toggle_with_text", async () => {
    status = { ...status, detected: { claude_code: false, codex: true } };
    await mount();
    const sw = screen.getByRole("switch", { name: "Claude Code" }) as HTMLButtonElement;
    expect(sw.disabled).toBe(true);
    expect(screen.getByText("Не нашёл Claude Code на этом Mac")).toBeTruthy();
  });

  it("broken_link_shows_error", async () => {
    useApp.setState({ settings: { ...base, agents: { claude_code: true, codex: false, summary: true } } });
    status = { ...status, claude_code: { kind: "broken", message: "expected value at line 1" } };
    await mount();
    expect(screen.getByText("Не смог подключиться: файл настроек Claude Code повреждён")).toBeTruthy();
  });

  it("summary_toggle_names_provider", async () => {
    useApp.setState({ settings: { ...base, provider: "Groq" } });
    await mount();
    expect(screen.getByText("Итог одной строкой (через Groq)")).toBeTruthy();
    expect(screen.getByText("Последнее сообщение агента уходит в Groq по твоему ключу, чтобы сделать короткую строку")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: "Итог одной строкой (через Groq)" })));
    expect(saved().slice(-1)[0].agents?.summary).toBe(false);
  });
});
