import type { AgentCard, AgentRow, AgentView, OverlayState, SessionState } from "../../lib/types";

/** A state dot: blue — works, amber — waits for the user, green — finished, not yet looked at. */
export type Dot = "working" | "waiting" | "done";

/** What the island shows for the agents (spec 2026-11-09 §3.2). */
export type AgentShow =
  | { kind: "none" }
  | { kind: "ears"; dot: Dot; dots: Dot[]; more: boolean }
  | { kind: "list"; rows: AgentRow[] }
  | { kind: "card"; card: AgentCard };

/** The colours of the Figma logo, chosen by the user 2026-11-10 (variant F2): blue works, coral waits, green is done. */
export const DOT_COLOR: Record<Dot, string> = { working: "#1ABCFE", waiting: "#FF7262", done: "#0ACF83" };

/** Dots in the right ear; with more agents, three and a «+». */
const MAX_DOTS = 4;

export function dotOf(state: SessionState): Dot {
  return state === "permission" || state === "waiting" ? "waiting" : state;
}

/**
 * Dictation always wins (§3.2 «Очерёдность»); then a card; then the list while the pointer is over
 * the ears; then the dots in the ears. Rows come sorted by urgency from Utter.
 */
export function agentShow(view: AgentView, dictation: OverlayState["kind"], hover: boolean): AgentShow {
  if (dictation !== "hidden") return { kind: "none" };
  if (view.card) return { kind: "card", card: view.card };
  if (view.rows.length === 0) return { kind: "none" };
  if (hover) return { kind: "list", rows: view.rows };
  const all = view.rows.map((r) => dotOf(r.state));
  const more = all.length > MAX_DOTS;
  return { kind: "ears", dot: all[0], dots: more ? all.slice(0, MAX_DOTS - 1) : all, more };
}

/** The easter egg (§3.6): this many pokes at the Drop within `POKE_WINDOW_MS` make it dizzy. */
const POKES = 5;
const POKE_WINDOW_MS = 2000;
export const DIZZY_MS = 3000;

/** Whether the pokes (click times, ms) up to `now` are enough to make the Drop dizzy. */
export function pokes(times: number[], now: number): boolean {
  return times.filter((t) => t <= now && now - t < POKE_WINDOW_MS).length >= POKES;
}

/** Dizzy for `DIZZY_MS` from `since` (the poke that did it), and never over a dictation. */
export function dizzyNow(since: number | null, now: number, dictation: OverlayState["kind"]): boolean {
  return since !== null && now - since < DIZZY_MS && dictation === "hidden";
}
