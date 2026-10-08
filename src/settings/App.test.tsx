// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { Settings } from "../lib/types";
import { parseRoute } from "./routes";

const listeners: Record<string, (e: { payload: unknown }) => void> = {};
const calls: string[] = [];
const settings: Settings = {
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
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    calls.push(cmd);
    if (cmd === "get_settings") return { ...settings };
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners[name] = cb;
    return () => delete listeners[name];
  }),
}));

const { App } = await import("./App");

async function mount(hash: string) {
  history.replaceState(null, "", hash);
  render(<App />);
  await act(async () => {
    await new Promise((r) => setTimeout(r, 50));
  });
}

beforeEach(() => {
  calls.length = 0;
});
afterEach(cleanup);

describe("settings app", () => {
  it("parses routes and falls back to general", () => {
    expect(parseRoute("#/history")).toBe("history");
    expect(parseRoute("#/onboarding")).toBe("onboarding");
    expect(parseRoute("#/evil")).toBe("general");
    expect(parseRoute("")).toBe("general");
  });

  it("onboarding hides the sidebar", async () => {
    await mount("#/onboarding");
    expect(document.querySelector("[data-nav]")).toBeNull();
    expect(document.querySelector("[data-logo]")).toBeNull();
    cleanup();
    await mount("#/general");
    expect(document.querySelector("[data-nav]")).not.toBeNull();
  });

  it("full-window pages get the window's height", async () => {
    // Onboarding is all absolutely positioned: with a zero-height wrapper it was clipped to nothing.
    await mount("#/onboarding");
    const main = document.querySelector("main")!;
    const page = main.firstElementChild!;
    expect(page.className).toContain("h-full");
    cleanup();
    await mount("#/general");
    expect(document.querySelector("main")!.firstElementChild!.className).toContain("h-full");
  });

  it("navigation event switches section", async () => {
    await mount("#/general");
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Основное");
    act(() => listeners["ui://navigate"]({ payload: "history" }));
    await act(() => new Promise((r) => setTimeout(r, 400)));
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("История");
    expect(location.hash).toBe("#/history");
  });

  it("reports ready once settings are rendered, without waiting for a frame", async () => {
    const raf = vi.spyOn(window, "requestAnimationFrame").mockImplementation(() => 0); // hidden windows never run it
    await mount("#/general");
    raf.mockRestore();
    expect(calls).toContain("settings_ready");
    expect(calls.indexOf("get_settings")).toBeLessThan(calls.indexOf("settings_ready"));
  });

  it("logo shows the drop and the name", async () => {
    await mount("#/general");
    const logo = document.querySelector("[data-logo]")!;
    expect(logo.textContent).toBe("Utter");
    // The logo marks the place; the window's one Drop is drawn there.
    expect((logo.querySelector("[data-drop-slot=logo]") as HTMLElement).style.width).toBe("60px");
    expect(document.querySelector("[data-floating-drop] svg")).not.toBeNull();
  });

  it("logo eyes follow the pointer", async () => {
    await mount("#/general");
    const eyes = document.querySelector("[data-floating-drop] [data-eyes]")!;
    await act(async () => {
      document.dispatchEvent(new MouseEvent("pointermove", { clientX: 900, clientY: 0 }));
      await new Promise((r) => setTimeout(r, 900));
    });
    const x = Number(eyes.getAttribute("transform")!.match(/translate\(([-\d.]+)/)![1]);
    expect(x).toBeGreaterThan(5);
  });
});
