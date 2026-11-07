// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { applyTheme, resolveTheme, watchSystemDark } from "./theme";

afterEach(() => vi.unstubAllGlobals());

describe("theme", () => {
  it("System follows the OS, explicit choices win", () => {
    expect(resolveTheme("System", true)).toBe("dark");
    expect(resolveTheme("System", false)).toBe("light");
    expect(resolveTheme("Light", true)).toBe("light");
    expect(resolveTheme("Dark", false)).toBe("dark");
  });

  it("applyTheme sets data-theme without animation", () => {
    applyTheme("light", false);
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.classList.contains("theme-fade")).toBe(false);
  });

  it("watchSystemDark reports changes", () => {
    let listener: ((e: { matches: boolean }) => void) | null = null;
    vi.stubGlobal("matchMedia", (q: string) => ({
      matches: true,
      media: q,
      addEventListener: (_: string, l: (e: { matches: boolean }) => void) => (listener = l),
      removeEventListener: () => (listener = null),
    }));
    const cb = vi.fn();
    const stop = watchSystemDark(cb);
    listener!({ matches: false });
    expect(cb).toHaveBeenCalledWith(false);
    stop();
    expect(listener).toBeNull();
  });
});
