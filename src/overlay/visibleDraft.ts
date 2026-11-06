import type { DraftParts, OverlayState } from "../lib/types";

/** A draft together with the recording (its start) that was on screen when it arrived. */
export type TaggedDraft = { parts: DraftParts; start: number | null };

/** The draft to show: never one that belongs to an earlier recording (it may arrive late). */
export function visibleDraft(state: OverlayState, draft: TaggedDraft | null): DraftParts | null {
  if (!draft) return null;
  if (state.kind === "recording" && draft.start !== state.started_at_ms) return null;
  return draft.parts;
}
