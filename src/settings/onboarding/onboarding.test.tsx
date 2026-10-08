// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { PermissionsStatus, Settings } from "../../lib/types";
import { useApp } from "../../lib/store";
import { canAdvance, dropAt, dropY, firstIncompleteStep, GROUP_H, OPTICAL_LIFT, STEPS, TOP_Y } from "./steps";
import { createFlight, settled, stepFlight } from "./flight";

const listeners: Record<string, (e: { payload: unknown }) => void> = {};
let perms: PermissionsStatus = { microphone: "Granted", accessibility: true, hotkeys_running: true };
let mask: string | null = null;
let clipboard: string | null = null;
let verify: () => Promise<void> = async () => {};
const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (cmd === "permissions_status") return { ...perms };
  if (cmd === "api_key_mask") return mask;
  if (cmd === "clipboard_key") return clipboard;
  if (cmd === "set_api_key") mask = "AIza••••";
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
};
const calls = (cmd: string) => invoke.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);
const wait = (ms: number) => act(() => new Promise((r) => setTimeout(r, ms)));
/** Waits until the step's text is in (the drop has landed), up to 4 s. */
async function landed() {
  for (let i = 0; i < 80; i++) {
    if (document.querySelector("[data-stage]")?.getAttribute("data-phase") === "in") return;
    await wait(50);
  }
  throw new Error("the drop never landed");
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
  await wait(60);
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

describe("onboarding flow", () => {
  it("Next is disabled until the microphone is granted", async () => {
    perms = { ...G, microphone: "NotDetermined" };
    await mount();
    expect(heading()).toBe("Говори — Utter напишет");
    await act(async () => fireEvent.click(next()));
    await wait(400);
    expect(heading()).toBe("Разреши микрофон");
    expect(next().disabled).toBe(true);
    perms = { ...G };
    await wait(1100);
    expect(next().disabled).toBe(false);
  });

  it("accessibility status updates by polling", async () => {
    perms = { ...G, accessibility: false };
    await mount();
    expect(heading()).toBe("Разреши вставку текста");
    expect(next().disabled).toBe(true);
    perms = { ...G };
    await wait(1100);
    expect(next().disabled).toBe(false);
  });

  it("provider step requires a verified key", async () => {
    await mount();
    expect(heading()).toBe("Бесплатный ключ Google");
    expect(next().disabled).toBe(true);
    verify = () => Promise.reject("bad_key");
    fireEvent.click(screen.getByText("Вставить вручную"));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "AIzaWRONG" } });
    await act(async () => fireEvent.click(screen.getByText("Сохранить")));
    await wait(20);
    expect(screen.getByText(/Ключ не подходит/)).toBeTruthy();
    expect(next().disabled).toBe(true);
  });

  it("key from the clipboard is saved and verified on focus", async () => {
    await mount();
    clipboard = `AIza${"a".repeat(35)}`;
    await act(async () => window.dispatchEvent(new Event("focus")));
    await wait(30);
    expect(calls("set_api_key")).toEqual([{ provider: "Gemini", key: clipboard }]);
    expect(calls("verify_key")).toHaveLength(1);
    expect(screen.getByText("Ключ подходит")).toBeTruthy();
    expect(next().disabled).toBe(false);
  });

  it("step text appears only after the drop settles", async () => {
    mask = "AIza••••"; // starts at «Клавиши» (top)
    await mount();
    expect(heading()).toBe("Твои клавиши");
    await act(async () => fireEvent.click(next()));
    await wait(300); // old text gone, the drop is flying to the centre
    const stage = document.querySelector("[data-stage]")!;
    expect(stage.getAttribute("data-phase")).toBe("wait");
    await wait(1400);
    expect(stage.getAttribute("data-phase")).toBe("in");
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
    await wait(60);
    await act(async () => fireEvent.click(next()));
    await wait(300);
    await landed();
    expect(calls("set_trial").slice(-1)[0]).toEqual({ on: true });
    await act(async () => fireEvent.click(screen.getByText("Назад")));
    await wait(300);
    await landed();
    expect(calls("set_trial").slice(-1)[0]).toEqual({ on: false });
    await act(async () => fireEvent.click(next()));
    await wait(300);
    await landed();
    expect(calls("set_trial").slice(-1)[0]).toEqual({ on: true });
    unmount();
    expect(calls("set_trial").slice(-1)[0]).toEqual({ on: false });
  });

  it("finishing enables autostart and marks onboarding done", async () => {
    mask = "AIza••••";
    const onDone = await mount();
    await act(async () => fireEvent.click(screen.getByText("Пропустить")));
    await wait(700); // the text and the footer leave first

    const saved = calls("save_settings").slice(-1)[0] as { settings: Settings };
    expect(saved.settings.onboarding_done).toBe(true);
    expect(saved.settings.autostart).toBe(true);
    expect(onDone).toHaveBeenCalled();
  });

  it("trial result is shown and makes the drop happy", async () => {
    mask = "AIza••••";
    await mount();
    await act(async () => fireEvent.click(next()));
    await wait(300);
    await landed();
    await act(async () => listeners["ui://trial"]({ payload: { kind: "text", mode: "Dictation", text: "Привет, это проверка" } }));
    expect(screen.getByText("Привет, это проверка")).toBeTruthy();
    expect(useDropStage.getState().mood).toBe("happy");
  });

  it("names the key in lower case inside the sentence", async () => {
    mask = "AIza••••";
    await mount();
    await act(async () => fireEvent.click(next()));
    await wait(300);
    await landed();
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
