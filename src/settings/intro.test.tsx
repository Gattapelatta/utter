// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Settings } from "../lib/types";

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
  onboarding_done: false,
  notch_drop: true,
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "get_settings") return { ...settings };
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
// The finish of onboarding itself is covered by its own tests: here only its hand-over to the app.
vi.mock("./onboarding/Onboarding", () => ({
  Onboarding: ({ onDone }: { onDone: () => void }) => <button onClick={onDone}>finish</button>,
}));

const { App } = await import("./App");
const { useDropStage } = await import("./drop/dropStage");

afterEach(cleanup);

const wait = (ms: number) => act(() => new Promise((r) => setTimeout(r, ms)));

it("after onboarding the first section cascades in once", { timeout: 30_000 }, async () => {
  // Review 2026-11-07: the intro's own cascade (120 ms after the menu) and the section's cascade
  // (when the intro ended) both ran — the title rose, dropped back under its mask and rose again.
  const titleMoves: Element[] = [];
  Element.prototype.animate = function (this: Element) {
    if (this.matches("[data-title] h1")) titleMoves.push(this);
    return { cancel() {}, finish() {}, onfinish: null } as unknown as Animation;
  } as Element["animate"];
  history.replaceState(null, "", "#onboarding");
  render(<App />);
  for (let end = Date.now() + 10_000; !screen.queryByText("finish") && Date.now() < end; ) await wait(25);
  fireEvent.click(screen.getByText("finish"));
  await wait(50);
  act(() => useDropStage.setState({ settled: true }));
  // Wait for the first rise however slow the machine, then long enough for a second one to show.
  for (let end = Date.now() + 10_000; titleMoves.length === 0 && Date.now() < end; ) await wait(25);
  await wait(1_000);
  expect(screen.getByRole("heading", { level: 1 })).toBeTruthy();
  expect(titleMoves).toHaveLength(1);
});
