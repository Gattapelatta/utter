import { describe, expect, it } from "vitest";
import { visibleDraft } from "./visibleDraft";
import type { OverlayState } from "../lib/types";

const rec = (start: number): OverlayState => ({ kind: "recording", mode: "Dictation", style: "Business", started_at_ms: start, warn: false, pending: 0 });
const parts = { settled: "старый", interim: "черновик" };

describe("visibleDraft", () => {
  it("shows the draft of the recording on screen", () => {
    expect(visibleDraft(rec(100), { parts, start: 100 })).toEqual(parts);
  });

  it("never shows the last recording's draft in a new one, not even for a frame", () => {
    expect(visibleDraft(rec(200), { parts, start: 100 })).toBeNull();
  });

  it("keeps the draft after release, while the result is processed", () => {
    expect(visibleDraft({ kind: "processing", pending: 0, since_ms: 0 }, { parts, start: 100 })).toEqual(parts);
    expect(visibleDraft(rec(100), null)).toBeNull();
  });
});
