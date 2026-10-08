import { useMemo, useRef } from "react";
import { motion } from "motion/react";
import { useT } from "../../i18n";
import type { OverlayState } from "../../lib/types";
import { Drop, type Mood } from "./Drop";

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

/** The black notch island with the character and, on its right, the timer or a short message (§4.2). */
export function Island({ state, level, now }: { state: OverlayState; level: number; now: number }) {
  const t = useT();
  const reduced = useMemo(prefersReducedMotion, []);
  // Non-recording states keep the colour of the recording they belong to.
  const translate = useRef(false);
  if (state.kind === "recording") translate.current = state.mode === "Translate";

  let right: React.ReactNode = null;
  if (state.kind === "recording") {
    right = (
      <span className="island-timer" data-warn={state.warn ? "" : undefined}>
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
  } else if (state.kind === "error") {
    const key =
      state.message_key === "rate_limited" && state.minutes === null ? "errors.rate_limited_later" : `errors.${state.message_key}`;
    right = <span className="island-text">{t(key, { provider: state.provider ?? "", n: state.minutes ?? 0 })}</span>;
  }

  return (
    <div data-island className="island" data-clickable={state.kind === "error" && state.action ? "" : undefined}>
      {/* The content appears once the island has grown and leaves before it folds. */}
      <motion.div
        className="island-row"
        variants={{
          hidden: { opacity: 0 },
          visible: { opacity: 1, transition: { delay: 0.18, duration: 0.22 } },
          leave: { opacity: 0, transition: { duration: 0.12 } },
        }}
      >
        <Drop mood={moodFor(state)} level={level} translate={translate.current} reduced={reduced} />
        {/* Always present and fixed-width: the character keeps its place between states. */}
        <span data-slot className="island-slot">
          {right}
        </span>
      </motion.div>
    </div>
  );
}
