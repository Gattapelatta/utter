// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { Pill } from "./Pill";
import { useApp } from "../lib/store";
import type { OverlayState } from "../lib/types";

const levels = Array.from({ length: 24 }, (_, i) => i / 24);

beforeEach(() => useApp.setState({ lang: "ru" }));

describe("Pill", () => {
  it("renders nothing for hidden and cancelled", () => {
    for (const s of [{ kind: "hidden" }, { kind: "cancelled" }] as OverlayState[]) {
      const { container, unmount } = render(<Pill state={s} levels={levels} now={0} />);
      expect(container.textContent).toBe("");
      unmount();
    }
  });

  it("shows mode label and timer while recording", () => {
    const s: OverlayState = { kind: "recording", mode: "Translate", style: "Casual", started_at_ms: 0, warn: false, pending: 0 };
    const { container } = render(<Pill state={s} levels={levels} now={67_000} />);
    expect(container.textContent).toContain("→ EN · Разговорный");
    expect(container.textContent).toContain("1:07");
    expect(container.querySelectorAll("[data-bar]").length).toBe(24);
  });

  it("shows translated labels in English", () => {
    useApp.setState({ lang: "en" });
    const s: OverlayState = { kind: "recording", mode: "Translate", style: "Casual", started_at_ms: 0, warn: false, pending: 0 };
    const { container } = render(<Pill state={s} levels={levels} now={0} />);
    expect(container.textContent).toContain("→ EN · Casual");
  });

  it("shows pending jobs and the limit warning while recording", () => {
    const s: OverlayState = { kind: "recording", mode: "Dictation", style: "Business", started_at_ms: 0, warn: true, pending: 1 };
    const { container } = render(<Pill state={s} levels={levels} now={1000} />);
    expect(container.textContent).toContain("Диктовка");
    expect(container.textContent).toContain("ещё 1 в обработке");
    expect(container.querySelector("[data-warn]")).not.toBeNull();
  });

  it("renders processing, done and error states", () => {
    const cases: [OverlayState, string][] = [
      [{ kind: "processing", pending: 1, since_ms: 0 }, "Обработка…"],
      [{ kind: "done", outcome: "pasted", degraded: false }, "Вставлено"],
      [{ kind: "done", outcome: "copied", degraded: false }, "Скопировано — нажми ⌘V"],
      [{ kind: "done", outcome: "pasted", degraded: true }, "Вставлено без обработки"],
      [{ kind: "error", message_key: "no_key", action: "open_provider", minutes: null, provider: null }, "Добавь API-ключ"],
    ];
    for (const [s, text] of cases) {
      const { container, unmount } = render(<Pill state={s} levels={levels} now={0} />);
      expect(container.textContent).toContain(text);
      unmount();
    }
  });

  it("names the provider in rate-limit errors", () => {
    const s: OverlayState = { kind: "error", message_key: "rate_limited", action: null, minutes: 3, provider: "Gemini" };
    const { container } = render(<Pill state={s} levels={levels} now={0} />);
    expect(container.textContent).toContain("Лимит Gemini исчерпан, сбросится через 3 мин");
    const later: OverlayState = { ...s, minutes: null };
    const { container: c2 } = render(<Pill state={later} levels={levels} now={0} />);
    expect(c2.textContent).toContain("попробуй позже");
  });

  it("renders the success check for shown trial results", () => {
    const s: OverlayState = { kind: "done", outcome: "shown", degraded: false };
    const { container } = render(<Pill state={s} levels={levels} now={0} />);
    expect(container.textContent).toContain("Готово");
    expect(container.querySelector("[data-check]")).not.toBeNull();
  });

  it("label is truncated, not wrapped", () => {
    const s: OverlayState = { kind: "error", message_key: "service_down", action: null, minutes: null, provider: "OpenAI" };
    const { container } = render(<Pill state={s} levels={levels} now={0} />);
    const label = container.querySelector("[data-label]") as HTMLElement;
    expect(label.className).toMatch(/truncate/);
    const pill = container.querySelector("[data-pill]") as HTMLElement;
    expect(pill.className).toMatch(/max-w-\[448px\]/);
  });

  it("hints at a slow connection after 4 seconds of processing", () => {
    const s: OverlayState = { kind: "processing", pending: 1, since_ms: 0 };
    const { container: fast } = render(<Pill state={s} levels={levels} now={3_000} />);
    expect(fast.textContent).toContain("Обработка…");
    const { container: slow } = render(<Pill state={s} levels={levels} now={4_500} />);
    expect(slow.textContent).toContain("Жду ответа сервера…");
  });
});
