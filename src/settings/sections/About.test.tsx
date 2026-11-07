// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { UpdateState } from "../../lib/types";
import { useApp } from "../../lib/store";

const listeners: Record<string, (e: { payload: unknown }) => void> = {};
let update: UpdateState = { kind: "idle" };
const invoke = vi.fn(async (cmd: string, _args?: Record<string, unknown>): Promise<unknown> => {
  if (cmd === "app_info") return { version: "0.1.0", os: "macOS 15.6" };
  if (cmd === "copy_diagnostics") return "report";
  if (cmd === "update_state") return update;
  return undefined;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners[name] = cb;
    return () => delete listeners[name];
  }),
}));

const { About } = await import("./About");
const called = (cmd: string) => invoke.mock.calls.some((c) => c[0] === cmd);
const push = (s: UpdateState) => act(() => listeners["ui://update"]?.({ payload: s }));

beforeEach(() => {
  invoke.mockClear();
  update = { kind: "idle" };
  useApp.setState({ lang: "ru" });
});
afterEach(cleanup);

describe("About", () => {
  it("shows version and copies diagnostics", async () => {
    render(<About />);
    expect(await screen.findByText("0.1.0")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("Скопировать")));
    expect(invoke.mock.calls.find((c) => c[0] === "copy_text")?.[1]).toEqual({ text: "report" });
    expect(screen.getByText("Скопировано")).toBeTruthy();
  });

  it("checks for updates on request", async () => {
    render(<About />);
    await act(async () => fireEvent.click(await screen.findByText("Проверить обновления")));
    expect(called("check_updates")).toBe(true);
    push({ kind: "checking" });
    expect(screen.getByText("Проверяю…")).toBeTruthy();
    push({ kind: "up_to_date" });
    expect(screen.getByText("У тебя последняя версия")).toBeTruthy();
  });

  it("offers install only when an update is available", async () => {
    render(<About />);
    await screen.findByText("Проверить обновления");
    expect(screen.queryByText("Установить и перезапустить")).toBeNull();
    push({ kind: "available", version: "0.2.0", notes: null });
    expect(screen.getByText("Доступна версия 0.2.0")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("Установить и перезапустить")));
    expect(called("install_update")).toBe(true);
    push({ kind: "downloading", percent: 42 });
    expect(screen.getByText("Загрузка… 42%")).toBeTruthy();
    expect(screen.queryByText("Установить и перезапустить")).toBeNull();
  });

  it("shows the state found before the section opened", async () => {
    update = { kind: "available", version: "0.3.0", notes: null };
    render(<About />);
    expect(await screen.findByText("Доступна версия 0.3.0")).toBeTruthy();
  });

  it("shows a calm message when the check fails", async () => {
    render(<About />);
    await screen.findByText("Проверить обновления");
    push({ kind: "failed" });
    expect(screen.getByText("Не удалось проверить обновления")).toBeTruthy();
    // It stays possible to try again; nothing pops up.
    expect(screen.getByText("Проверить обновления")).toBeTruthy();
  });
});
