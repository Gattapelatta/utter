import type { PermissionsStatus } from "../../lib/types";

export type StepId = "welcome" | "microphone" | "accessibility" | "provider" | "key" | "trial";
export const STEPS: StepId[] = ["welcome", "microphone", "accessibility", "provider", "key", "trial"];

/** Where the Drop stands (§4.4): centred on steps 1–3 and 6, at the top while there is more to show. */
export function dropAt(s: StepId): "center" | "top" {
  return s === "provider" || s === "key" ? "top" : "center";
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
