// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "./types";

const listeners: Record<string, (e: { payload: unknown }) => void> = {};
const saved: Settings[] = [];

const base: Settings = {
  schema_version: 1,
  record_mode: "Hold",
  insert_method: "Paste",
  speech_language: "ru",
  cleanup: true,
  paragraphs: false,
  sounds: true,
  autostart: false,
  theme: "Dark",
  ui_language: "Auto",
  translate_style: "Business",
  provider: "Gemini",
  hotkeys: { dictation: "RightAlt", translate: "RightMeta" },
  history_enabled: true,
  onboarding_done: false,
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: { settings: Settings }) => {
    if (cmd === "get_settings") return { ...base };
    if (cmd === "save_settings" && args) saved.push(args.settings);
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners[name] = cb;
    return () => delete listeners[name];
  }),
}));

const { useApp } = await import("./store");

beforeEach(() => {
  saved.length = 0;
});

describe("store", () => {
  it("store applies patches on top of the latest broadcast settings", async () => {
    await useApp.getState().init();
    listeners["ui://settings"]({ payload: { ...base, record_mode: "Toggle" } });
    await useApp.getState().update({ theme: "Light" });
    expect(saved[saved.length - 1]).toMatchObject({ record_mode: "Toggle", theme: "Light" });
  });

  it("store resolves theme and language from settings", async () => {
    await useApp.getState().init();
    useApp.getState().applyRemote({ ...base, theme: "Light", ui_language: "En" });
    expect(useApp.getState().theme).toBe("light");
    expect(useApp.getState().lang).toBe("en");
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});
