// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { HistoryEntry, Settings } from "../../lib/types";
import { useApp } from "../../lib/store";

const listeners: Record<string, (e: { payload: unknown }) => void> = {};
const now = Date.now();
let entries: HistoryEntry[] = [];
const invoke = vi.fn(async (cmd: string, _args?: Record<string, unknown>): Promise<unknown> => {
  if (cmd === "history_list") return entries;
  return undefined;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners[name] = cb;
    return () => delete listeners[name];
  }),
}));

const { History } = await import("./History");

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
const entry = (id: number, over: Partial<HistoryEntry> = {}): HistoryEntry => ({
  id,
  created_at_ms: now - id * 60_000,
  mode: "Dictation",
  text: `текст ${id}`,
  raw: `сырой ${id}`,
  status: { kind: "done" },
  audio_file: null,
  ...over,
});
const calls = (cmd: string) => invoke.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);
const settle = () => act(() => new Promise((r) => setTimeout(r, 20)));

beforeEach(() => {
  invoke.mockClear();
  entries = [entry(1), entry(2, { mode: "Translate", text: "Hello there" })];
  useApp.setState({ settings: { ...base }, lang: "ru", theme: "dark", systemDark: true });
});
afterEach(cleanup);

describe("history section", () => {
  it("lists entries newest first with mode labels", async () => {
    const { container } = render(<History />);
    await settle();
    const items = [...container.querySelectorAll("[data-entry]")];
    expect(items.map((i) => i.querySelector("[data-text]")!.textContent)).toEqual(["текст 1", "Hello there"]);
    expect(items[0].textContent).toContain("Диктовка");
    expect(items[1].textContent).toContain("→ EN");
  });

  it("long entries are clamped until expanded", async () => {
    entries = [entry(1, { text: "очень длинный текст ".repeat(40), raw: "сырая расшифровка" })];
    const { container } = render(<History />);
    await settle();
    const text = container.querySelector("[data-text]")!;
    expect(text.hasAttribute("data-clamped")).toBe(true);
    expect(screen.queryByText("сырая расшифровка")).toBeNull();
    fireEvent.click(text);
    expect(text.hasAttribute("data-clamped")).toBe(false);
    expect(screen.getByText("сырая расшифровка")).toBeTruthy();
  });

  it("retry only for failed entries with audio", async () => {
    entries = [
      entry(1, { status: { kind: "failed", message: "network" }, audio_file: "/a.wav", text: "" }),
      entry(2, { status: { kind: "failed", message: "network" }, audio_file: null, text: "" }),
      entry(3),
    ];
    render(<History />);
    await settle();
    const retry = screen.getAllByText("Повторить");
    expect(retry).toHaveLength(1);
    await act(async () => fireEvent.click(retry[0]));
    expect(calls("history_retry")).toEqual([{ id: 1 }]);
  });

  it("copy uses copyText and shows feedback", async () => {
    render(<History />);
    await settle();
    await act(async () => fireEvent.click(screen.getAllByLabelText("Копировать")[0]));
    expect(calls("copy_text")).toEqual([{ text: "текст 1" }]);
    expect(screen.getByLabelText("Скопировано")).toBeTruthy();
  });

  it("turning history off asks for confirmation", async () => {
    render(<History />);
    await settle();
    fireEvent.click(screen.getByRole("switch"));
    expect(screen.getByText("История будет удалена")).toBeTruthy();
    expect(calls("save_settings")).toHaveLength(0);
    await act(async () => fireEvent.click(screen.getByText("Удалить")));
    expect((calls("save_settings")[0] as { settings: Settings }).settings.history_enabled).toBe(false);
  });

  it("refreshes on ui://history", async () => {
    render(<History />);
    await settle();
    entries = [entry(0, { text: "новая" }), ...entries];
    await act(async () => listeners["ui://history"]({ payload: null }));
    await settle();
    expect(screen.getByText("новая")).toBeTruthy();
  });

  it("shows an empty state", async () => {
    entries = [];
    render(<History />);
    await settle();
    expect(screen.getByText("Здесь появятся твои диктовки и переводы")).toBeTruthy();
  });
});
