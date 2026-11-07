import { useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion, useIsPresent, usePresenceData } from "motion/react";
import { useT } from "../../i18n";
import type { DraftParts, OverlayState } from "../../lib/types";
import { DraftPanel } from "./DraftPanel";
import { Drop, type Level, type Mood } from "./Drop";
import { RestDrop } from "./RestDrop";
import type { Gaze } from "./restMotion";
import { contentTarget, dropTarget, shapeTarget, timerLeaveTarget, timerTarget, type Notch, type View } from "./islandMotion";

/** After this long without an answer the network is the likely culprit. */
const SLOW_AFTER_MS = 4_000;

export function moodFor(s: OverlayState): Mood {
  switch (s.kind) {
    case "recording":
      return "listening";
    case "processing":
      return "thinking";
    case "done":
      return s.outcome === "failed" ? "sad" : "happy";
    case "error":
      return "sad";
    default:
      return "idle";
  }
}

function formatTimer(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

function prefersReducedMotion(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

interface Props {
  state: OverlayState;
  level: Level;
  now: number;
  /** The open island, the ears at rest (§4.2a), or folded into the notch. */
  view?: View;
  notch?: Notch;
  /** The recording was cancelled with Esc: fold faster. */
  cancelled?: boolean;
  /** The live draft shown in the window view. */
  draft?: DraftParts | null;
}

/**
 * The black notch shape with the one Drop and, on its right, the timer or a short message
 * (§4.2). The shape is a clip of the island; the Drop flies between its place on the island
 * and the left ear.
 */
export function Island({ state, level, now, view = "open", notch = null, cancelled = false, draft = null }: Props) {
  const t = useT();
  const reduced = useMemo(prefersReducedMotion, []);
  // While leaving, the props are those of the last render; AnimatePresence passes the fresh "cancelled".
  const present = useIsPresent();
  const leaving = usePresenceData() as boolean | undefined;
  const fast = present ? cancelled : (leaving ?? cancelled);
  // Non-recording states keep the colour of the recording they belong to.
  const translate = useRef(false);
  if (state.kind === "recording") translate.current = state.mode === "Translate";
  // This recording opened the draft window: its timer stays in the wing until it leaves with the field.
  const windowed = useRef(false);
  if (view === "window") windowed.current = true;
  else if (state.kind === "recording") windowed.current = false;
  const lastTimer = useRef("0:00");
  if (state.kind === "recording") lastTimer.current = formatTimer(now - state.started_at_ms);
  const frozenTimer = (
    <span className="island-timer">
      <span data-timer>{lastTimer.current}</span>
    </span>
  );
  const fieldLeaving = windowed.current && state.kind === "done" && state.outcome !== "copied";

  let right: React.ReactNode = null;
  if (fieldLeaving || (windowed.current && state.kind === "processing")) {
    right = frozenTimer;
  } else if (state.kind === "recording") {
    right = (
      // 30 s before the 10-minute limit (§3.3); the island is always black: the dark theme's amber.
      <span className="island-timer" data-warn={state.warn ? "" : undefined} style={state.warn ? { color: "#F5B544" } : undefined}>
        <span data-timer>{formatTimer(now - state.started_at_ms)}</span>
        {state.pending > 0 && (
          <span data-pending className="absolute -top-2 -right-3.5 text-[11px] text-[#8a9097]">
            {state.pending}
          </span>
        )}
      </span>
    );
  } else if (state.kind === "processing" && now - state.since_ms > SLOW_AFTER_MS) {
    right = <span className="island-text">{t("pill.slow")}</span>;
  } else if (state.kind === "done" && state.outcome === "copied") {
    right = <span className="island-timer">⌘V</span>;
  } else if (state.kind === "done" && state.outcome === "failed") {
    right = <span className="island-text">{t("pill.failed")}</span>;
  } else if (state.kind === "done" && state.degraded) {
    // §8.3: the raw transcript went in — say so (a failed translation is still in Russian).
    right = <span className="island-text">{t("pill.degraded")}</span>;
  } else if (state.kind === "error") {
    const key =
      state.message_key === "rate_limited" && state.minutes === null ? "errors.rate_limited_later" : `errors.${state.message_key}`;
    right = <span className="island-text">{t(key, { provider: state.provider ?? "", n: state.minutes ?? 0 })}</span>;
  }

  const folded = { notch, fast, reduced };
  const at = (v: View) => ({
    shape: shapeTarget(v, folded.notch, folded.fast, folded.reduced),
    drop: dropTarget(v, folded.notch, folded.fast, folded.reduced),
  });
  const open = view === "open" || view === "window";
  const content = contentTarget(open ? "open" : "rest");
  const timer = timerTarget(view === "window" ? "window" : "open");
  const slot = fieldLeaving
    ? timerLeaveTarget()
    : { opacity: content.opacity, filter: "blur(0px)", transform: timer.transform, transition: { opacity: content.transition, transform: timer.transition, filter: { duration: 0 } } };
  // In the ear the full Drop hands over to the compositor-driven one once it has come to rest,
  // and takes the eyes back from it when a recording starts (§4.2a, idle budget §8.4).
  const gaze = useRef<Gaze>({ x: 0, y: 0 });
  const [handed, setHanded] = useState(false);
  useEffect(() => {
    if (view !== "rest") setHanded(false);
  }, [view]);
  const inEar = view === "rest" && handed;

  return (
    <div data-island data-view={view} className="island" data-clickable={state.kind === "error" && state.action ? "" : undefined}>
      <motion.div data-shape className="island-shape" initial={at("folded").shape} animate={at(view).shape} exit={at("folded").shape} />
      {/* The field is cut by the island's own shape: while the window shrinks back into the island,
          a fading field must not hang below it. */}
      <motion.div data-draft-clip className="island-shape-clip" initial={at("folded").shape} animate={at(view).shape} exit={at("folded").shape}>
        <AnimatePresence>
          {(view === "window" || state.kind === "recording") && (
            <DraftPanel key="draft" draft={view === "window" ? draft : null} shown={view === "window"} />
          )}
        </AnimatePresence>
      </motion.div>
      <div className="island-row">
        <motion.div data-drop-place initial={at("folded").drop} animate={at(view).drop} exit={at("folded").drop}>
          {inEar ? (
            <RestDrop start={gaze.current} reduced={reduced} gazeRef={gaze} />
          ) : (
            <Drop
              mood={open ? moodFor(state) : "idle"}
              rest={view === "rest"}
              level={level}
              translate={translate.current}
              reduced={reduced}
              startGaze={() => gaze.current}
              onSettle={(g) => {
                gaze.current = g;
                if (view === "rest") setHanded(true);
              }}
            />
          )}
        </motion.div>
        {/* Always present and fixed-width: the character keeps its place between states. */}
        <motion.span data-slot className="island-slot" initial={{ opacity: 0 }} animate={slot} exit={contentTarget("folded")}>
          {right}
        </motion.span>
      </div>
    </div>
  );
}
