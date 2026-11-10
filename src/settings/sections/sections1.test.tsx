// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { KeyId, Settings } from "../../lib/types";
import { useApp } from "../../lib/store";
import { SPEECH_LANGUAGES } from "../languages";

const listeners: Record<string, (e: { payload: unknown }) => void> = {};
let captureResult: () => Promise<KeyId> = async () => "F5" as unknown as KeyId;
const invoke = vi.fn(async (cmd: string, _args?: unknown): Promise<unknown> => {
  if (cmd === "capture_key") return captureResult();
  return undefined;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners[name] = cb;
    return () => delete listeners[name];
  }),
}));

const { General } = await import("./General");
const { Hotkeys } = await import("./Hotkeys");
const { Translate } = await import("./Translate");

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

const saved = () => invoke.mock.calls.filter((c) => c[0] === "save_settings").map((c) => (c[1] as { settings: Settings }).settings);
const last = () => saved()[saved().length - 1];

beforeEach(() => {
  invoke.mockClear();
  document.documentElement.dataset.theme = "dark";
  useApp.setState({ settings: { ...base }, lang: "ru", theme: "dark", systemDark: true });
});
afterEach(cleanup);

describe("general, hotkeys and translation sections", () => {
  it("changing record mode calls update with Toggle", async () => {
    render(<General />);
    await act(async () => fireEvent.click(screen.getByText("Нажатие")));
    expect(last()?.record_mode).toBe("Toggle");
  });

  it("the Drop in the notch can be switched off", async () => {
    render(<General />);
    const sw = screen.getByRole("switch", { name: "Капля в чёлке" });
    expect(sw.getAttribute("aria-checked")).toBe("true");
    await act(async () => fireEvent.click(sw));
    expect(last()?.notch_drop).toBe(false);
  });

  it("theme switch applies immediately", () => {
    invoke.mockImplementationOnce(() => new Promise(() => {})); // the save never answers
    render(<General />);
    fireEvent.click(screen.getByText("Светлая"));
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("rejects a key taken by the other mode", async () => {
    captureResult = async () => "RightMeta";
    render(<Hotkeys />);
    await act(async () => fireEvent.click(screen.getAllByText("Изменить")[0]));
    expect(screen.getByText(/уже назначена на Перевод/)).toBeTruthy();
    expect(saved()).toHaveLength(0);
  });

  it("assigns a free key", async () => {
    captureResult = async () => ({ F: 5 });
    render(<Hotkeys />);
    await act(async () => fireEvent.click(screen.getAllByText("Изменить")[0]));
    expect(last()?.hotkeys).toEqual({ dictation: { F: 5 }, translate: "RightMeta" });
  });

  it("hotkeys_section_has_three_keys", async () => {
    useApp.setState({ settings: { ...base, hotkeys: { dictation: "RightAlt", translate: "RightMeta", task: "RightCtrl" } } });
    render(<Hotkeys />);
    expect(screen.getAllByText("Изменить")).toHaveLength(3);
    expect(screen.getByText("Задача")).toBeTruthy();
    expect(screen.getByText("Сбивчивая речь — чёткое задание для ИИ-агента")).toBeTruthy();
    // The task key cannot take a key of either other mode.
    captureResult = async () => "RightAlt";
    await act(async () => fireEvent.click(screen.getAllByText("Изменить")[2]));
    expect(screen.getByText(/уже назначена на Диктовка/)).toBeTruthy();
    captureResult = async () => ({ F: 6 });
    await act(async () => fireEvent.click(screen.getAllByText("Изменить")[2]));
    expect(last()?.hotkeys).toEqual({ dictation: "RightAlt", translate: "RightMeta", task: { F: 6 } });
  });

  it("a letter as a hotkey is allowed, with a warning that it stops typing", async () => {
    useApp.setState({ settings: { ...base, hotkeys: { dictation: "RightAlt", translate: "RightMeta", task: { Raw: 0 } } } });
    render(<Hotkeys />);
    expect(screen.getByText("A")).toBeTruthy();
    expect(screen.getByText("Пока эта клавиша назначена, она не печатает")).toBeTruthy();
  });

  it("left Control can be the task key", async () => {
    useApp.setState({ settings: { ...base, hotkeys: { dictation: "RightAlt", translate: "RightMeta", task: "RightCtrl" } } });
    captureResult = async () => "LeftCtrl";
    render(<Hotkeys />);
    await act(async () => fireEvent.click(screen.getAllByText("Изменить")[2]));
    expect(last()?.hotkeys.task).toBe("LeftCtrl");
    expect(screen.getByText("Левый ⌃")).toBeTruthy();
  });

  it("shows the assignable-keys hint on not_assignable", async () => {
    captureResult = () => Promise.reject("not_assignable");
    render(<Hotkeys />);
    await act(async () => fireEvent.click(screen.getAllByText("Изменить")[1]));
    expect(screen.getByText(/Подойдёт любая клавиша, кроме Esc/)).toBeTruthy();
  });

  it("cancels capture on unmount", async () => {
    captureResult = () => new Promise(() => {});
    const { unmount } = render(<Hotkeys />);
    await act(async () => fireEvent.click(screen.getAllByText("Изменить")[0]));
    expect(screen.getByText(/Нажми клавишу/)).toBeTruthy();
    unmount();
    expect(invoke.mock.calls.some((c) => c[0] === "cancel_capture")).toBe(true);
  });

  it("keycap presses on ui://hotkey for its mode only", async () => {
    const { container } = render(<Hotkeys />);
    await act(async () => {});
    act(() => listeners["ui://hotkey"]({ payload: { mode: "Translate", down: true } }));
    const caps = container.querySelectorAll("kbd");
    expect(caps[0].hasAttribute("data-pressed")).toBe(false);
    expect(caps[1].hasAttribute("data-pressed")).toBe(true);
    act(() => listeners["ui://hotkey"]({ payload: { mode: "Translate", down: false } }));
    expect(caps[1].hasAttribute("data-pressed")).toBe(false);
  });

  it("translation example highlights the selected style", async () => {
    const { container } = render(<Translate />);
    const selected = () => container.querySelector("[data-selected]")?.textContent ?? "";
    expect(selected()).toContain("could you please send me the report");
    await act(async () => fireEvent.click(screen.getAllByText("Разговорный")[0]));
    expect(last()?.translate_style).toBe("Casual");
    expect(selected()).toContain("Hey, can you send me");
  });

  it("language list starts with Auto and the six main languages", () => {
    expect(SPEECH_LANGUAGES.slice(0, 7).map((l) => l.code)).toEqual([null, "ru", "en", "uk", "de", "es", "fr"]);
    const rest = SPEECH_LANGUAGES.slice(7).map((l) => l.name);
    expect(rest).toEqual([...rest].sort((a, b) => a.localeCompare(b)));
    expect(SPEECH_LANGUAGES).toHaveLength(7 + 37);
  });
});
