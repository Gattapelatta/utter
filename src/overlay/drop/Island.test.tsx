// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render } from "@testing-library/react";
import { Island, moodFor } from "./Island";
import { useApp } from "../../lib/store";
import type { OverlayState } from "../../lib/types";

beforeEach(() => {
  useApp.setState({ lang: "ru" });
  vi.useRealTimers();
});

const rec = (mode: "Dictation" | "Translate" = "Dictation", pending = 0): OverlayState => ({
  kind: "recording", mode, style: "Business", started_at_ms: 0, warn: false, pending,
});

describe("Island", () => {
  it("moodFor maps overlay states", () => {
    expect(moodFor(rec())).toBe("listening");
    expect(moodFor({ kind: "processing", pending: 1, since_ms: 0 })).toBe("thinking");
    expect(moodFor({ kind: "done", outcome: "pasted", degraded: false })).toBe("happy");
    expect(moodFor({ kind: "done", outcome: "failed", degraded: false })).toBe("sad");
    expect(moodFor({ kind: "error", message_key: "network", action: null, minutes: null, provider: null })).toBe("sad");
  });

  it("island shows the timer only while recording and error text on errors", () => {
    const { container, rerender } = render(<Island state={rec()} level={0} now={67_000} />);
    expect(container.querySelector("[data-timer]")?.textContent).toBe("1:07");
    rerender(<Island state={{ kind: "processing", pending: 1, since_ms: 0 }} level={0} now={0} />);
    expect(container.querySelector("[data-timer]")).toBeNull();
    rerender(<Island state={{ kind: "error", message_key: "network", action: null, minutes: null, provider: null }} level={0} now={0} />);
    expect(container.textContent).toContain("Нет соединения");
    rerender(<Island state={{ kind: "done", outcome: "copied", degraded: false }} level={0} now={0} />);
    expect(container.textContent).toContain("⌘V");
  });

  it("keeps the right-hand slot in every state so the character never moves", () => {
    const states: OverlayState[] = [
      rec(),
      { kind: "processing", pending: 1, since_ms: 0 },
      { kind: "done", outcome: "pasted", degraded: false },
      { kind: "error", message_key: "rate_limited", action: null, minutes: 12, provider: "Gemini" },
    ];
    for (const s of states) {
      const { container, unmount } = render(<Island state={s} level={0} now={0} />);
      const slot = container.querySelector("[data-slot]");
      expect(slot, s.kind).not.toBeNull();
      expect(slot!.className).toContain("island-slot");
      unmount();
    }
  });

  it("shows queued jobs next to the timer", () => {
    const { container } = render(<Island state={rec("Dictation", 2)} level={0} now={0} />);
    expect(container.querySelector("[data-pending]")?.textContent).toBe("2");
  });

  it("translate mode turns rings cyan", { timeout: 6000 }, async () => {
    const { container } = render(<Island state={rec("Translate")} level={0.5} now={0} />);
    await act(() => new Promise((r) => setTimeout(r, 3000)));
    const stroke = container.querySelector("[data-ring]")?.getAttribute("stroke");
    expect(stroke).toBe("rgb(74,216,236)");
  });

  it("reduced motion keeps the character still", async () => {
    vi.stubGlobal("matchMedia", (q: string) => ({ matches: q.includes("reduce"), media: q, addEventListener() {}, removeEventListener() {} }));
    const { container } = render(<Island state={rec()} level={1} now={0} />);
    await act(() => new Promise((r) => setTimeout(r, 400)));
    const tr = container.querySelector("[data-body]")?.getAttribute("transform") ?? "";
    expect(tr).toMatch(/rotate\(0(\.0+)? /);
    vi.unstubAllGlobals();
  });
});
