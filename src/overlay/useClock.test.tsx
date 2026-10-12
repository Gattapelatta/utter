// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { useClock } from "./useClock";

afterEach(() => vi.useRealTimers());

describe("pill clock", () => {
  it("does not tick while the pill is hidden (idle: no wake-ups)", () => {
    vi.useFakeTimers();
    const spy = vi.spyOn(globalThis, "setInterval");
    const { result, rerender } = renderHook(({ on }) => useClock(on), { initialProps: { on: false } });
    expect(spy).not.toHaveBeenCalled();
    rerender({ on: true });
    const start = result.current;
    act(() => vi.advanceTimersByTime(1_000));
    expect(result.current).toBeGreaterThan(start);
    rerender({ on: false });
    const stopped = result.current;
    act(() => vi.advanceTimersByTime(1_000));
    expect(result.current).toBe(stopped);
  });
});
