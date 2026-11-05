import { useLayoutEffect, useRef } from "react";
import { motion } from "motion/react";
import type { DraftParts } from "../../lib/types";
import { leaveTarget } from "./islandMotion";

/** The field under the wings of the draft window (spec 2026-11-06 §3.1, variant C1). */
const FIELD = { left: 14, top: 206 - 14 - 128, width: 392, height: 128 };

/**
 * The live draft: italic, top-down, scrolled to the last line; the first lines leave over the
 * frame's top edge (no fade). Leaves with the timer when the result is pasted.
 */
export function DraftPanel({ draft }: { draft: DraftParts }) {
  const view = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const v = view.current;
    if (v) v.scrollTop = v.scrollHeight;
  }, [draft]);
  return (
    <motion.div
      data-draft-panel
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: { delay: 0.15, duration: 0.25 } }}
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
        <span data-settled style={{ color: "#E6E8EA" }}>
          {draft.settled}
        </span>
        {draft.settled && draft.interim ? " " : null}
        <span data-interim style={{ color: "#8A8F96" }}>
          {draft.interim}
        </span>
      </div>
    </motion.div>
  );
}
