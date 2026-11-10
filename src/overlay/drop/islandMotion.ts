export type Notch = { width: number; height: number } | null;

/**
 * What the black shape shows: the full island, the draft window (spec 2026-11-06), the ears at
 * rest (§4.2a), or nothing (in the notch).
 */
export type View = "open" | "window" | "rest" | "folded" | "agent";

/** A shape hanging from the top edge, centred: width, height, bottom corner radius (pt). */
export type Box = { w: number; h: number; r: number };

/** Every shape is cut out of this base: the draft window, the largest of them. */
const BASE_W = 420;
const BASE_H = 206;
export const ISLAND_BOX: Box = { w: 360, h: 104, r: 40 };
export const WINDOW_BOX: Box = { w: BASE_W, h: BASE_H, r: 30 };
/** Each ear: the Drop (26) with 5 pt of air on both sides (chosen by the user 2026-10-13). */
const EAR_W = 36;
/** Bottom corners of the ears; matched to the notch's own rounding on screen. */
const EAR_RADIUS = 8;
const REST_DROP = 26;
const PLATE: Box = { w: 36, h: 32, r: EAR_RADIUS };
/** Where the Drop sits on the open island: the pair centred at y 61 (variant C), Drop 48 wide, gap 44, slot 48. */
const DROP_AT = { x: BASE_W / 2 - 46, y: 61 };
/** The timer slot right of the Drop on the open island. */
const SLOT = { left: DROP_AT.x + 24 + 44, w: 48 };
/** In the draft window: the Drop (26 pt) in the left wing, the timer (13 pt) in the right one. */
const WING_DROP = { x: 37, y: 43 };
const WING_TIMER = { right: BASE_W - 24, y: 43, scale: 13 / 21 };

/** The agents' island (the user's mockup 2026-11-09): the draft window's width, as tall as its content. */
export function agentBox(h: number): Box {
  return { w: BASE_W, h: Math.min(Math.round(h), BASE_H), r: 30 };
}

/** The agents' big Drop (44 pt): on the left, 24 pt in, its top 14 pt below the notch. */
const AGENT_DROP = 44;
export function agentDropCentre(notch: Notch): { x: number; y: number } {
  return { x: 24 + AGENT_DROP / 2, y: (notch?.height ?? 0) + 14 + AGENT_DROP / 2 };
}

export function restBox(notch: Notch): Box {
  return notch ? { w: notch.width + 2 * EAR_W, h: notch.height, r: EAR_RADIUS } : PLATE;
}

export function foldedBox(notch: Notch): Box {
  return notch ? { w: notch.width, h: notch.height, r: EAR_RADIUS } : { w: PLATE.w, h: 0, r: 0 };
}

/**
 * The shape as a clip of the 420 × 206 base, so its corners stay round at any size (scaling
 * squashed the 40 pt corners into ovals at ear size). One template for every shape: Motion only
 * interpolates strings whose functions and numbers line up.
 */
export function clip(b: Box): string {
  const side = (BASE_W - b.w) / 2;
  return `inset(0px ${side}px ${BASE_H - b.h}px ${side}px round 0px 0px ${b.r}px ${b.r}px)`;
}

/** How far the Drop moves from its island place to the left ear (or the plate), and its scale there. */
export function restDropOffset(notch: Notch): { x: number; y: number; scale: number } {
  const at = notch ? { x: BASE_W / 2 - notch.width / 2 - EAR_W / 2, y: notch.height / 2 } : { x: BASE_W / 2, y: PLATE.h / 2 };
  return { x: at.x - DROP_AT.x, y: at.y - DROP_AT.y, scale: REST_DROP / 48 };
}

/** The island grows out of the ears or the notch: no overshoot (§5.4). */
const grow = { type: "spring" as const, bounce: 0, duration: 0.5 };
/** Folding back is an even ease-in-out tween, so it ends exactly when the window may be hidden. */
const fold = (cancelled: boolean) => ({
  type: "tween" as const,
  duration: cancelled ? 0.35 : 0.6,
  ease: [0.4, 0, 0.2, 1] as [number, number, number, number],
});
const quick = { duration: 0.15 };
/** The agents' island opens the moment the pointer arrives (the user 2026-11-10). */
const snap = { type: "spring" as const, bounce: 0, duration: 0.28 };

function motionFor(view: View, cancelled: boolean, reduced: boolean) {
  if (reduced) return quick;
  if (view === "agent") return snap;
  return view === "open" || view === "window" ? grow : fold(cancelled);
}

function boxFor(view: View, notch: Notch, agentH: number): Box {
  switch (view) {
    case "agent":
      return agentBox(agentH);
    case "open":
      return ISLAND_BOX;
    case "window":
      return WINDOW_BOX;
    case "rest":
      return restBox(notch);
    case "folded":
      return foldedBox(notch);
  }
}

/** `agentH`: the agents' island height (only for the "agent" view). */
export function shapeTarget(view: View, notch: Notch, cancelled: boolean, reduced: boolean, agentH = 150) {
  return { clipPath: clip(boxFor(view, notch, agentH)), transition: motionFor(view, cancelled, reduced) };
}

const place = (x: number, y: number, s: number) => `translate(${+x.toFixed(4)}px, ${+y.toFixed(4)}px) scale(${+s.toFixed(6)})`;

/** The one Drop flies between its island place and the left ear, together with the shape. */
export function dropTarget(view: View, notch: Notch, cancelled: boolean, reduced: boolean, _agentH = 150) {
  const o = restDropOffset(notch);
  const a = agentDropCentre(notch);
  const transform =
    view === "open"
      ? place(0, 0, 1)
      : view === "agent"
        ? place(a.x - DROP_AT.x, a.y - DROP_AT.y, AGENT_DROP / 48)
        : view === "window"
        ? place(WING_DROP.x - DROP_AT.x, WING_DROP.y - DROP_AT.y, REST_DROP / 48)
        : place(o.x, o.y, o.scale);
  return {
    transform,
    opacity: view === "folded" ? 0 : 1,
    transition: motionFor(view, cancelled, reduced),
  };
}

/**
 * The timer and texts: they appear once the island has grown, and on the way out fade over most
 * of the fold. On a dark desktop the black island's motion is barely visible, so this fade is
 * what the eye reads as "closing" — a short fade looked like a sudden vanish.
 */
export function contentTarget(view: View) {
  if (view === "agent") return { opacity: 1, transition: { delay: 0.06, duration: 0.16 } };
  return view === "open" || view === "window"
    ? { opacity: 1, transition: { delay: 0.18, duration: 0.22 } }
    : { opacity: 0, transition: { duration: 0.5, ease: [0.4, 0, 0.6, 1] as [number, number, number, number] } };
}

/** The timer flies from its slot on the island to the window's right wing, shrinking to 13 pt. */
function timerPlace(view: "open" | "window", lift = 0, shrink = 1): string {
  if (view === "open") return place(0, lift, shrink);
  const s = WING_TIMER.scale * shrink;
  return place(WING_TIMER.right - SLOT.w * s - SLOT.left, WING_TIMER.y - DROP_AT.y + lift, s);
}

export function timerTarget(view: "open" | "window") {
  return { transform: timerPlace(view), transition: grow };
}

/** The draft field and the timer leave the window together (spec 2026-11-06 §3.1 p. 6). */
export const leaveTarget = {
  opacity: 0,
  y: -10,
  scale: 0.96,
  filter: "blur(2px)",
  transition: { duration: 0.3, ease: [0.45, 0, 0.25, 1] as [number, number, number, number] },
};

/** The timer's share of `leaveTarget`, staying in the wing while it fades. */
export function timerLeaveTarget() {
  return { opacity: 0, filter: "blur(2px)", transform: timerPlace("window", -10, 0.96), transition: leaveTarget.transition };
}
