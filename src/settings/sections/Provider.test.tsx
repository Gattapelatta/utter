// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Settings } from "../../lib/types";
import { useApp } from "../../lib/store";

const listeners: Record<string, (e: { payload: unknown }) => void> = {};
let mask: string | null = null;
let verify: () => Promise<void> = async () => {};
const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (cmd === "api_key_mask") return mask;
  if (cmd === "set_api_key") mask = `${String(args!.key).slice(0, 4)}…`;
  if (cmd === "verify_key") return verify();
  if (cmd === "usage_today") return { date: "2026-10-08", stt_requests: 400, audio_secs: 750, llm_requests: 3, llm_tokens: 1200 };
  if (cmd === "free_limits") return args!.provider === "Groq" ? { stt_requests: 2000, audio_secs: 28800, llm_requests: null, llm_tokens: null } : null;
  return undefined;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners[name] = cb;
    return () => delete listeners[name];
  }),
}));

const { Provider } = await import("./Provider");
const { Usage, usageBars } = await import("./Usage");

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
};
const calls = (cmd: string) => invoke.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);
const settle = () => act(() => new Promise((r) => setTimeout(r, 20)));

beforeEach(() => {
  invoke.mockClear();
  mask = null;
  verify = async () => {};
  useApp.setState({ settings: { ...base }, lang: "ru", theme: "dark", systemDark: true });
});
afterEach(cleanup);

describe("provider and usage", () => {
  it("usageBars keeps only limited counters", () => {
    const day = { date: "d", stt_requests: 10, audio_secs: 60, llm_requests: 1, llm_tokens: 5 };
    expect(usageBars(day, null)).toEqual([]);
    expect(usageBars(day, { stt_requests: 1500, audio_secs: null, llm_requests: null, llm_tokens: null })).toEqual([
      { key: "stt_requests", value: 10, max: 1500 },
    ]);
  });

  it("saving a key refreshes the mask and clears the field", async () => {
    render(<Provider />);
    await settle();
    const field = screen.getByPlaceholderText("Вставь ключ") as HTMLInputElement;
    fireEvent.change(field, { target: { value: "AIzaSECRET" } });
    await act(async () => fireEvent.click(screen.getByText("Сохранить")));
    await settle();
    expect(calls("set_api_key")).toEqual([{ provider: "Gemini", key: "AIzaSECRET" }]);
    expect(field.value).toBe("");
    expect(field.placeholder).toBe("AIza…");
  });

  it("verify shows success and mapped errors", async () => {
    mask = "AIza…";
    render(<Provider />);
    await settle();
    await act(async () => fireEvent.click(screen.getByText("Проверить")));
    expect(screen.getByText("Ключ работает")).toBeTruthy();
    verify = () => Promise.reject("bad_key");
    await act(async () => fireEvent.click(screen.getByText("Проверить")));
    expect(screen.getByText(/Ключ не подходит/)).toBeTruthy();
    expect(screen.queryByText("Ключ работает")).toBeNull();
  });

  it("switching provider updates settings and mask", async () => {
    render(<Provider />);
    await settle();
    await act(async () => fireEvent.click(screen.getByText("Groq")));
    await settle();
    expect(useApp.getState().settings?.provider).toBe("Groq");
    expect(calls("api_key_mask").slice(-1)[0]).toEqual({ provider: "Groq" });
  });

  it("free-key link opens the provider page", async () => {
    render(<Provider />);
    await settle();
    await act(async () => fireEvent.click(screen.getByText("Получить бесплатный ключ")));
    expect(calls("open_link")).toEqual([{ link: "gemini_key" }]);
  });

  it("today's counters show numbers and bars only for known limits", async () => {
    useApp.setState({ settings: { ...base, provider: "Groq" } });
    const { container } = render(<Usage />);
    await settle();
    expect(container.textContent).toContain("400");
    expect(container.textContent).toContain("12,5"); // 750 s of audio in minutes
    expect(container.querySelectorAll("[role=progressbar]")).toHaveLength(2);
  });

  it("refreshes on ui://history", async () => {
    render(<Usage />);
    await settle();
    const before = calls("usage_today").length;
    await act(async () => listeners["ui://history"]({ payload: null }));
    expect(calls("usage_today").length).toBe(before + 1);
  });
});
