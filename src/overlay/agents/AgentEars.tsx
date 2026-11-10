import type { Notch } from "../drop/islandMotion";
import { DOT_COLOR, type Dot } from "./agentIsland";

/** The island's base width (the ears are centred in it). */
const BASE_W = 420;
const EAR_W = 36;
/** Small enough not to cover the Drop's eye (the user 2026-11-10: 9 pt was too big). */
const STATE_DOT = 7;
const DOT = 6;

/** The right ear's three colours, always in this order, as a corner (the user 2026-11-10):
 *  the first, the second to its right, the third under the first. */
const ALL: Dot[] = ["working", "waiting", "done"];
const CORNER: [number, number][] = [
  [0, 0],
  [1, 0],
  [0, 1],
];

/**
 * The agents in the ears at rest (spec 2026-11-09 §3.2.1, changed by the user 2026-11-10): the
 * state dot on the Drop in the left ear, drawn over it; in the right ear all three colours,
 * bright, the current one (the Drop's) shimmering. Beside the notch only: nothing is drawn under
 * the camera. Without a notch there is no right ear: only the state dot.
 */
export function AgentEars({ notch, dot }: { notch: Notch; dot: Dot }) {
  const h = notch?.height ?? 32;
  const left = notch ? BASE_W / 2 - notch.width / 2 - EAR_W / 2 : BASE_W / 2;
  const right = notch ? BASE_W / 2 + notch.width / 2 + EAR_W / 2 : null;
  const step = DOT + 4;
  // The 2 × 2 grid the corner sits in, centred in the right ear.
  const x0 = (right ?? 0) - (DOT + step) / 2;
  const y0 = h / 2 - (DOT + step) / 2;
  return (
    <div data-agent-ears style={{ position: "absolute", inset: 0, pointerEvents: "none" }}>
      <span
        data-agent-dot={dot}
        style={{
          position: "absolute",
          left: `${left + 13 - STATE_DOT + 1}px`,
          top: `${h / 2 + 13 - STATE_DOT + 1}px`,
          width: STATE_DOT,
          height: STATE_DOT,
          borderRadius: STATE_DOT / 2,
          background: DOT_COLOR[dot],
          boxShadow: "0 0 0 1.5px #000",
        }}
      />
      {right !== null &&
        ALL.map((c, i) => (
          <span
            key={c}
            data-agent-ear-dot={c}
            data-pulse={c === dot ? "" : undefined}
            className={c === dot ? "agent-pulse" : undefined}
            style={{
              position: "absolute",
              left: `${x0 + CORNER[i][0] * step}px`,
              top: `${y0 + CORNER[i][1] * step}px`,
              width: DOT,
              height: DOT,
              borderRadius: DOT / 2,
              background: DOT_COLOR[c],
            }}
          />
        ))}
    </div>
  );
}
