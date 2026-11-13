import type { AgentCard, AgentRow, AgentView, OverlayState, PlanStep, SessionState } from "../../lib/types";

/** A state dot: blue — works, amber — waits for the user, green — finished, not yet looked at. */
export type Dot = "working" | "waiting" | "done";

/** What the island shows for the agents (spec 2026-11-09 §3.2). */
export type AgentShow =
  | { kind: "none" }
  /** `dot`: the Drop's state dot; null when an agent is connected but nothing happens. */
  | { kind: "ears"; dot: Dot | null; dots: Dot[]; more: boolean }
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
  if (view.rows.length === 0) {
    // Connected, nothing happening: the colours stay in the right ear, the Drop has no dot;
    // hovering opens «Все агенты свободны» (there the Drop can be poked).
    if (!view.connected || view.paused) return { kind: "none" };
    return hover ? { kind: "list", rows: [] } : { kind: "ears", dot: null, dots: [], more: false };
  }
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

/** The ladder (spec 2026-11-13 §3.1): the current step's number, the step before, the current, the next. */
export type Ladder = { n: number; m: number; prev: string | null; current: string | null; next: string | null; allDone: boolean };

/**
 * The current step is the active one, else the first not done; `prev` — the nearest done step
 * before it, `next` — the first unfinished after it. Null without a plan.
 */
export function ladderOf(plan: PlanStep[]): Ladder | null {
  if (plan.length === 0) return null;
  const m = plan.length;
  let i = plan.findIndex((s) => s.status === "active");
  if (i < 0) i = plan.findIndex((s) => s.status === "pending");
  if (i < 0) return { n: m, m, prev: null, current: null, next: null, allDone: true };
  let p = i - 1;
  while (p >= 0 && plan[p].status !== "done") p--;
  const next = plan.slice(i + 1).find((s) => s.status !== "done");
  return { n: i + 1, m, prev: p >= 0 ? plan[p].title : null, current: plan[i].title, next: next?.title ?? null, allDone: false };
}

/** Slack around a button or row where a click still counts (live 2026-11-14: clicks 1–3 pt under «Открыть» fell through to the app below). */
export const HIT_SLACK = 8;

/** Where the window takes a click for one `[data-hit]` element: its box and `HIT_SLACK` around it, never above the window's top. */
export function hitZone(r: { left: number; top: number; width: number; height: number }): { x: number; y: number; w: number; h: number } {
  const x = Math.max(0, r.left - HIT_SLACK);
  const y = Math.max(0, r.top - HIT_SLACK);
  return { x, y, w: r.left + r.width + HIT_SLACK - x, h: r.top + r.height + HIT_SLACK - y };
}
