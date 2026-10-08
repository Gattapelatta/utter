import type { PermissionsStatus } from "../../lib/types";

export type StepId = "welcome" | "microphone" | "accessibility" | "provider" | "key" | "trial";
export const STEPS: StepId[] = ["welcome", "microphone", "accessibility", "provider", "key", "trial"];

/** Where the Drop stands (§4.4): centred on steps 1–3 and 6, at the top while there is more to show. */
export function dropAt(s: StepId): "center" | "top" {
  return s === "provider" || s === "key" ? "top" : "center";
}

export const DROP_W = 88;
export const DROP_H = Math.round((DROP_W * 50) / 48);
export const GAP = 24;
export const TOP_Y = 60;
/** Drop + gap + the welcome text: title 38.5, line 54.5, demo (two lines reserved) 54. */
export const GROUP_H = DROP_H + GAP + 147;
/** The eye reads the exact middle as low; lift the group a little. */
export const OPTICAL_LIFT = 8;

/** The Drop's top in a window of `height`: the centred group sits in the middle of the whole window. */
export function dropY(at: "center" | "top", height: number): number {
  if (at === "top") return TOP_Y;
  return Math.max(TOP_Y, Math.round((height - GROUP_H) / 2) - OPTICAL_LIFT);
}

/** A restarted onboarding resumes at the first requirement still missing. */
export function firstIncompleteStep(p: PermissionsStatus, hasKey: boolean): StepId {
  if (p.microphone !== "Granted") return "welcome";
  if (!p.accessibility) return "accessibility";
  if (!hasKey) return "provider";
  return "key";
}

export function canAdvance(step: StepId, p: PermissionsStatus | null, keyOk: boolean): boolean {
  switch (step) {
    case "microphone":
      return p?.microphone === "Granted";
    case "accessibility":
      return p?.accessibility === true;
    case "provider":
      return keyOk;
    default:
      return true;
  }
}
