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

  it("tells when the text went in without cleanup and when it could not be copied", () => {
    // Review 2026-11-07: §8.3 — lost in the pill → island rewrite; a failed translation pasted
    // the raw Russian under a happy Drop.
    const { container, rerender } = render(<Island state={{ kind: "done", outcome: "pasted", degraded: true }} level={0} now={0} />);
    expect(container.textContent).toContain("Вставлено без обработки");
    rerender(<Island state={{ kind: "done", outcome: "failed", degraded: false }} level={0} now={0} />);
    expect(container.textContent).toContain("Не удалось скопировать");
    rerender(<Island state={{ kind: "done", outcome: "pasted", degraded: false }} level={0} now={0} />);
    expect(container.querySelector("[data-slot]")?.textContent).toBe("");
  });

  it("the timer turns amber 30 s before the 10-minute limit", () => {
    // Review 2026-11-07: §3.3 — the island set data-warn but nothing styled it. The island is
    // always black: the dark theme's amber, whatever the app's theme.
    const warn: OverlayState = { kind: "recording", mode: "Dictation", style: "Business", started_at_ms: 0, warn: true, pending: 0 };
    const { container, rerender } = render(<Island state={warn} level={0} now={0} />);
    expect((container.querySelector(".island-timer") as HTMLElement).style.color).toBe("rgb(245, 181, 68)");
    rerender(<Island state={rec()} level={0} now={0} />);
    expect((container.querySelector(".island-timer") as HTMLElement).style.color).toBe("");
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

  it("draws the character at 48 × 50 so the camera notch does not cut it (variant C)", () => {
    const { container } = render(<Island state={rec()} level={0} now={0} />);
    const svg = container.querySelector("svg")!;
    expect([svg.getAttribute("width"), svg.getAttribute("height")]).toEqual(["48", "50"]);
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

  it("the Drop hears a voice level it reads each frame", async () => {
    // The overlay hands the level over as a function so a level never redraws the island.
    const loud = () => 1;
    const rings = async (level: number | (() => number)) => {
      const { container, unmount } = render(<Island state={rec()} level={level} now={0} />);
      await act(() => new Promise((r) => setTimeout(r, 700)));
      // How strongly the rings show: they swell with the voice.
      const r = [...container.querySelectorAll("[data-ring]")].reduce((sum, e) => sum + Number(e.getAttribute("opacity")), 0);
      unmount();
      return r;
    };
    const heard = await rings(loud);
    const silent = await rings(() => 0);
    expect(heard).toBeGreaterThan(silent + 0.2);
    expect(Math.abs(heard - (await rings(1)))).toBeLessThan(0.1);
  });

  it("reduced motion keeps the character still", async () => {
    vi.stubGlobal("matchMedia", (q: string) => ({ matches: q.includes("reduce"), media: q, addEventListener() {}, removeEventListener() {} }));
    const { container } = render(<Island state={rec()} level={1} now={0} />);
    await act(() => new Promise((r) => setTimeout(r, 400)));
    const tr = container.querySelector("[data-body]")?.getAttribute("transform") ?? "";
    expect(tr).toMatch(/rotate\(0(\.0+)? /);
    vi.unstubAllGlobals();
  });

  it("hands the ear over to the compositor Drop once it has settled there, and takes it back for a recording", async () => {
    Element.prototype.animate = (() => ({ cancel() {}, onfinish: null })) as unknown as Element["animate"];
    const notch = { width: 185, height: 32 };
    const { container, rerender } = render(<Island state={rec()} level={0} now={0} view="rest" notch={notch} />);
    expect(container.querySelector("[data-body]")).not.toBeNull();
    await act(() => new Promise((r) => setTimeout(r, 4500)));
    expect(container.querySelector("[data-rest-drop]")).not.toBeNull();
    expect(container.querySelector("[data-body]")).toBeNull();
    rerender(<Island state={rec()} level={0} now={0} view="open" notch={notch} />);
    expect(container.querySelector("[data-body]")).not.toBeNull();
    expect(container.querySelector("[data-rest-drop]")).toBeNull();
  }, 8000);
});
