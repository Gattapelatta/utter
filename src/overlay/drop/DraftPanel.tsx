import { useLayoutEffect, useRef } from "react";
import { motion } from "motion/react";
import type { DraftParts } from "../../lib/types";
import { leaveTarget } from "./islandMotion";

/** The field under the wings of the draft window (spec 2026-11-06 §3.1, variant C1). */
const FIELD = { left: 14, top: 206 - 14 - 128, width: 392, height: 128 };

/** Laid out (invisibly) before the window opens, so the italic face is ready mid-animation. */
const WARM_UP = "Привет, это черновик.";

/**
 * The live draft: italic, top-down, scrolled to the last line; the first lines leave over the
 * frame's top edge (no fade). Mounted hidden for the whole recording — creating it while the
 * island grew cost a frame (video 2026-11-07) — and shown once the window opens; leaves with the
 * timer when the result is pasted.
 */
export function DraftPanel({ draft, shown = true }: { draft: DraftParts | null; shown?: boolean }) {
  const view = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const v = view.current;
    if (v) v.scrollTop = v.scrollHeight;
  }, [draft]);
  return (
    <motion.div
      data-draft-panel
      data-shown={shown ? "" : undefined}
      initial={{ opacity: 0 }}
      animate={shown ? { opacity: 1, transition: { delay: 0.15, duration: 0.25 } } : { opacity: 0, transition: { duration: 0 } }}
      exit={leaveTarget}
      style={{
        position: "absolute",
        ...Object.fromEntries(Object.entries(FIELD).map(([k, v]) => [k, `${v}px`])),
        boxSizing: "border-box",
        padding: "10px 12px",
        background: "#1C1C1E",
        border: "1px solid #3A3A3D",
        borderRadius: "14px",
        transformOrigin: "50% 0%",
        // Its own layer from the start: promoting it when the fade began cost a frame (video).
        willChange: "opacity, transform",
      }}
    >
      <div
        data-draft-view
        ref={view}
        style={{
          height: "100%",
          overflow: "hidden",
          scrollBehavior: "smooth",
          fontFamily: '"Manrope Variable", "Manrope", sans-serif',
          fontSize: "12.5px",
          lineHeight: "18px",
          fontStyle: "italic",
          textAlign: "left",
          overflowWrap: "anywhere",
        }}
      >
        {draft ? (
          <>
            <span data-settled style={{ color: "#E6E8EA" }}>
              {draft.settled}
            </span>
            {draft.settled && draft.interim ? " " : null}
            <span data-interim style={{ color: "#8A8F96" }}>
              {draft.interim}
            </span>
          </>
        ) : (
          <span aria-hidden style={{ color: "#E6E8EA" }}>
            {WARM_UP}
          </span>
        )}
      </div>
    </motion.div>
  );
}
