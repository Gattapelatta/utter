import { useRef } from "react";
import { AnimatePresence } from "motion/react";
import { Island } from "./drop/Island";
import type { Notch, View } from "./drop/islandMotion";
import type { OverlayState } from "../lib/types";

interface Props {
  state: OverlayState;
  /** The ears are shown while idle (§4.2a). */
  rest: boolean;
  notch: Notch;
  level: number;
  now: number;
  onClick?: () => void;
}

/**
 * Everything the overlay window draws: the island while a recording or its result is shown,
 * the ears while idle, nothing otherwise (the island then folds into the notch and goes away).
 */
export function Stage({ state, rest, notch, level, now, onClick }: Props) {
  const visible = state.kind !== "hidden" && state.kind !== "cancelled";
  // The last visible state stays on screen while the island folds.
  const shown = useRef<OverlayState>(state);
  if (visible) shown.current = state;
  const cancelled = state.kind === "cancelled";
  const view: View = visible ? "open" : "rest";

  return (
    <AnimatePresence custom={cancelled}>
      {(visible || rest) && (
        <div key="island" onClick={onClick}>
          <Island state={shown.current} level={level} now={now} view={view} notch={notch} cancelled={cancelled} />
        </div>
      )}
    </AnimatePresence>
  );
}
