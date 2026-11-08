// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { PermissionsStatus, Settings } from "../../lib/types";
import { useApp } from "../../lib/store";
import { canAdvance, dropAt, dropY, firstIncompleteStep, GROUP_H, OPTICAL_LIFT, STEPS, TOP_Y } from "./steps";
import { APPLE, createFlight, GLIDE, settled, springEasing, stepFlight } from "./flight";

const listeners: Record<string, (e: { payload: unknown }) => void> = {};
let perms: PermissionsStatus = { microphone: "Granted", accessibility: true, hotkeys_running: true };
let mask: string | null = null;
let clipboard: string | null = null;
let verify: () => Promise<void> = async () => {};
const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (cmd === "permissions_status") return { ...perms };
  if (cmd === "api_key_mask") return mask;
  if (cmd === "clipboard_key") return clipboard;
  if (cmd === "set_api_key") mask = (args as { key: string }).key ? "AIza••••" : null;
  if (cmd === "verify_key") return verify();
  if (cmd === "save_settings") useApp.setState({ settings: (args as { settings: Settings }).settings });
  return undefined;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners[name] = cb;
    return () => delete listeners[name];
  }),
}));

const { Onboarding } = await import("./Onboarding");
const { useDropStage } = await import("../drop/dropStage");
const { DemoPhrase } = await import("./DemoPhrase");

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
  onboarding_done: false,
  notch_drop: true,
};
const calls = (cmd: string) => invoke.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);
const wait = (ms: number) => act(() => new Promise((r) => setTimeout(r, ms)));
/** Waits for something that takes real time (a flight, a poll) instead of a fixed number of ms:
 *  a slow machine (CI) only waits longer, it does not fail. */
async function until(ok: () => unknown, what: string, ms = 10_000) {
  const end = Date.now() + ms;
  while (!ok()) {
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await wait(25);
  }
}
const phase = () => document.querySelector("[data-stage]")?.getAttribute("data-phase");
/** The step's text is in: the drop has landed. */
const landed = () => until(() => phase() === "in", "the drop to land");
/** After a click: the old text has left and the next step's text is in. */
async function movedOn() {
  await until(() => phase() !== "in", "the old text to leave");
  await landed();
}
const heading = () => screen.getByRole("heading", { level: 1 }).textContent;
const next = () => screen.getByRole("button", { name: /Далее|Начать|Готово/ }) as HTMLButtonElement;
const G: PermissionsStatus = { microphone: "Granted", accessibility: true, hotkeys_running: true };

beforeEach(() => {
  invoke.mockClear();
  perms = { ...G };
  mask = null;
  clipboard = null;
  verify = async () => {};
  useApp.setState({ settings: { ...base }, lang: "ru", theme: "dark", systemDark: true });
});
afterEach(cleanup);

async function mount() {
  const onDone = vi.fn();
  render(<Onboarding onDone={onDone} />);
  await until(() => screen.queryByRole("heading", { level: 1 }), "the first step");
  return onDone;
}

describe("onboarding logic", () => {
  it("firstIncompleteStep resumes at the first missing requirement", () => {
    expect(firstIncompleteStep({ ...G, microphone: "NotDetermined" }, false)).toBe("welcome");
    expect(firstIncompleteStep({ ...G, accessibility: false }, false)).toBe("accessibility");
    expect(firstIncompleteStep(G, false)).toBe("provider");
    expect(firstIncompleteStep(G, true)).toBe("key");
  });

  it("drop waits at the centre on steps 1-3 and 6", () => {
    expect(STEPS.map(dropAt)).toEqual(["center", "center", "center", "top", "top", "center"]);
  });

  it("the centred group sits in the middle of the whole window", () => {
    for (const h of [520, 620, 900]) {
      const top = dropY("center", h);
      const bottom = h - top - GROUP_H;
      expect(Math.abs(bottom - top - 2 * OPTICAL_LIFT)).toBeLessThanOrEqual(1);
    }
    expect(dropY("top", 620)).toBe(TOP_Y);
  });

  it("canAdvance needs each step's requirement", () => {
    expect(canAdvance("microphone", { ...G, microphone: "Denied" }, false)).toBe(false);
    expect(canAdvance("microphone", G, false)).toBe(true);
    expect(canAdvance("accessibility", { ...G, accessibility: false }, false)).toBe(false);
    expect(canAdvance("provider", G, false)).toBe(false);
    expect(canAdvance("provider", G, true)).toBe(true);
    expect(canAdvance("welcome", { ...G, microphone: "Denied" }, false)).toBe(true);
  });

  it("the long glide (logo ↔ About) is slower and barely overshoots", () => {
    const f = createFlight(21);
    f.target = 496;
    let highest = 21;
    let t = 0;
    let halfway = 0;
    while (!settled(f) && t < 3) {
      stepFlight(f, 1 / 60, GLIDE);
      highest = Math.max(highest, f.x);
      if (!halfway && f.x > 258) halfway = t;
      t += 1 / 60;
    }
    expect(highest - 496).toBeLessThan(4); // was 17 px past the target with the onboarding spring
    expect(halfway).toBeGreaterThan(0.2); // was 0.17 s to cover half the way
    expect(t).toBeLessThan(1.6);
  });

  it("a spring becomes a compositor easing that starts at 0, ends at 1 and keeps the overshoot", () => {
    const glide = springEasing(GLIDE);
    const apple = springEasing(APPLE);
    const values = (e: string) => e.slice("linear(".length, -1).split(",").map(Number);
    expect(glide.easing.startsWith("linear(")).toBe(true);
    expect(values(glide.easing)[0]).toBe(0);
    expect(values(glide.easing).slice(-1)[0]).toBe(1);
    expect(Math.max(...values(apple.easing))).toBeGreaterThan(1.01); // Apple spring overshoots
    expect(Math.max(...values(glide.easing))).toBeLessThan(1.01);
    expect(glide.duration).toBeGreaterThan(600);
    expect(glide.duration).toBeLessThan(1600);
  });

  it("the drop's spring overshoots a little and then settles", () => {
    const f = createFlight(200);
    f.target = 60;
    let lowest = 200;
    let t = 0;
    while (!settled(f) && t < 3) {
      stepFlight(f, 1 / 60);
      lowest = Math.min(lowest, f.x);
      t += 1 / 60;
    }
    expect(lowest).toBeLessThan(60); // Apple-style: a slight overshoot
    expect(lowest).toBeGreaterThan(40);
    expect(t).toBeLessThan(1.5);
    expect(Math.abs(f.x - 60)).toBeLessThan(1.5);
  });
});

// Flights and polls take real time; a slow machine needs more than vitest's default 5 s.
describe("onboarding flow", { timeout: 30_000 }, () => {
  it("Next is disabled until the microphone is granted", async () => {
    perms = { ...G, microphone: "NotDetermined" };
    await mount();
    expect(heading()).toBe("Говори — Utter напишет");
    await act(async () => fireEvent.click(next()));
    await until(() => heading() === "Разреши микрофон", "the microphone step");
    expect(next().disabled).toBe(true);
    perms = { ...G };
    await until(() => !next().disabled, "the permission poll to enable Next");
  });

  it("accessibility status updates by polling", async () => {
    perms = { ...G, accessibility: false };
    await mount();
    expect(heading()).toBe("Разреши вставку текста");
    expect(next().disabled).toBe(true);
    perms = { ...G };
    await until(() => !next().disabled, "the permission poll to enable Next");
  });

  it("provider step requires a verified key", async () => {
    await mount();
    expect(heading()).toBe("Бесплатный ключ Google");
    expect(next().disabled).toBe(true);
    verify = () => Promise.reject("bad_key");
    fireEvent.click(screen.getByText("Вставить вручную"));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "AIzaWRONG" } });
    await act(async () => fireEvent.click(screen.getByText("Сохранить")));
    await until(() => screen.queryByText(/Ключ не подходит/), "the rejection");
    expect(next().disabled).toBe(true);
  });

  it("key from the clipboard is saved and verified on focus", async () => {
    await mount();
    clipboard = `AIza${"a".repeat(35)}`;
    await act(async () => window.dispatchEvent(new Event("focus")));
    await until(() => screen.queryByText("Ключ подходит"), "the key check");
    expect(calls("set_api_key")).toEqual([{ provider: "Gemini", key: clipboard }]);
    expect(calls("verify_key")).toHaveLength(1);
    expect(screen.getByText("Ключ подходит")).toBeTruthy();
    expect(next().disabled).toBe(false);
  });

  it("a key that failed for another reason is checked again when the user comes back", async () => {
    // Review 2026-11-07: without the VPN Google answers "location not supported" (service_down);
    // after turning the VPN on the same key in the clipboard was never checked again.
    await mount();
    clipboard = `AIza${"b".repeat(35)}`;
    verify = () => Promise.reject("service_down");
    await act(async () => window.dispatchEvent(new Event("focus")));
    await until(() => screen.queryByText(/недоступен/), "the service_down message");
    await wait(1_700); // the clipboard poll does not hammer a failing service
    expect(calls("verify_key")).toHaveLength(1);
    verify = async () => {};
    await act(async () => window.dispatchEvent(new Event("focus")));
    await until(() => screen.queryByText("Ключ подходит"), "the second check");
    expect(calls("verify_key")).toHaveLength(2);
    expect(screen.getByText("Ключ подходит")).toBeTruthy();
    expect(next().disabled).toBe(false);
  });

  it("a failed check can be retried by hand", async () => {
    await mount();
    clipboard = `AIza${"c".repeat(35)}`;
    verify = () => Promise.reject("rate_limited");
    await act(async () => window.dispatchEvent(new Event("focus")));
    await until(() => screen.queryByText("Проверить снова"), "the retry button");
    verify = async () => {};
    await act(async () => fireEvent.click(screen.getByText("Проверить снова")));
    await until(() => screen.queryByText("Ключ подходит"), "the retried check");
  });

  it("a rejected key is not kept, so a restart does not take it for a working one", async () => {
    // Review 2026-11-07: the key was saved before the check; after a restart a rejected key
    // counted as verified and the trial dictation failed.
    await mount();
    clipboard = `AIza${"d".repeat(35)}`;
    verify = () => Promise.reject("bad_key");
    await act(async () => window.dispatchEvent(new Event("focus")));
    await until(() => screen.queryByText(/Ключ не подходит/), "the rejection");
    expect(mask).toBeNull();
  });

  it("step text appears only after the drop settles", async () => {
    mask = "AIza••••"; // starts at «Клавиши» (top)
    await mount();
    expect(heading()).toBe("Твои клавиши");
    // Every phase the stage goes through, however slow the machine.
    const stage = document.querySelector("[data-stage]")!;
    const seen: string[] = [];
    const watch = new MutationObserver(() => seen.push(stage.getAttribute("data-phase")!));
    watch.observe(stage, { attributes: true, attributeFilter: ["data-phase"] });
    await act(async () => fireEvent.click(next()));
    await movedOn();
    watch.disconnect();
    // The old text left, the new one waited while the drop flew to the centre, then came in.
    expect(seen).toEqual(["out", "wait", "in"]);
    expect(heading()).toBe("Попробуй");
  });

  it("skip is offered only on the last two steps", async () => {
    await mount(); // provider step
    expect(screen.queryByText("Пропустить")).toBeNull();
    cleanup();
    mask = "AIza••••";
    await mount(); // keys step
    expect(screen.getByText("Пропустить")).toBeTruthy();
  });

  it("trial mode is on only while the trial step is shown", async () => {
    mask = "AIza••••";
    const { unmount } = render(<Onboarding onDone={vi.fn()} />);
    await until(() => screen.queryByRole("heading", { level: 1 }), "the first step");
    await act(async () => fireEvent.click(next()));
    await movedOn();
    expect(calls("set_trial").slice(-1)[0]).toEqual({ on: true });
    await act(async () => fireEvent.click(screen.getByText("Назад")));
    await movedOn();
    expect(calls("set_trial").slice(-1)[0]).toEqual({ on: false });
    await act(async () => fireEvent.click(next()));
    await movedOn();
    expect(calls("set_trial").slice(-1)[0]).toEqual({ on: true });
    unmount();
    expect(calls("set_trial").slice(-1)[0]).toEqual({ on: false });
  });

  it("finishing enables autostart and marks onboarding done", async () => {
    mask = "AIza••••";
    const onDone = await mount();
    await act(async () => fireEvent.click(screen.getByText("Пропустить")));
    await until(() => onDone.mock.calls.length > 0, "the text and the footer to leave"); // they leave first

    const saved = calls("save_settings").slice(-1)[0] as { settings: Settings };
    expect(saved.settings.onboarding_done).toBe(true);
    expect(saved.settings.autostart).toBe(true);
    expect(onDone).toHaveBeenCalled();
  });

  it("trial result is shown and makes the drop happy", async () => {
    mask = "AIza••••";
    await mount();
    await act(async () => fireEvent.click(next()));
    await movedOn();
    await act(async () => listeners["ui://trial"]({ payload: { kind: "text", mode: "Dictation", text: "Привет, это проверка" } }));
    expect(screen.getByText("Привет, это проверка")).toBeTruthy();
    expect(useDropStage.getState().mood).toBe("happy");
  });

  it("names the key in lower case inside the sentence", async () => {
    mask = "AIza••••";
    await mount();
    await act(async () => fireEvent.click(next()));
    await movedOn();
    expect(screen.getByText("Зажми правый ⌥ и скажи что-нибудь.")).toBeTruthy();
  });

  it("the theme switch on the welcome step keeps its own width", async () => {
    perms = { ...G, microphone: "NotDetermined" };
    await mount();
    const group = screen.getByRole("radiogroup");
    expect(group.parentElement!.className).toContain("justify-center");
  });
});

describe("demo phrase", () => {
  it("demo phrase shows the final text at once with reduced motion", () => {
    render(<DemoPhrase reduced />);
    expect(screen.getByText("Hi, could you send me the report by Friday?")).toBeTruthy();
  });
});
