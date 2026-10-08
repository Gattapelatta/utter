import { create } from "zustand";
import type { Mood } from "../../overlay/drop/Drop";

const HAPPY_MS = 2_200;
let happyTimer: ReturnType<typeof setTimeout> | undefined;

/** The window's one Drop: its mood, and whether it has landed on its current place. */
interface DropStage {
  mood: Mood;
  settled: boolean;
  setSettled(settled: boolean): void;
  /** Smile, a satellite and sparkles (the pill's success effect), then back to idle. */
  celebrate(): void;
}

export const useDropStage = create<DropStage>()((set, get) => ({
  mood: "idle",
  settled: true,
  setSettled(settled) {
    if (get().settled !== settled) set({ settled });
  },
  celebrate() {
    set({ mood: "happy" });
    clearTimeout(happyTimer);
    happyTimer = setTimeout(() => set({ mood: "idle" }), HAPPY_MS);
  },
}));
