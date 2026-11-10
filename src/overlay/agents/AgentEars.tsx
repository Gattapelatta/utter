import type { Notch } from "../drop/islandMotion";
import { DOT_COLOR, type Dot } from "./agentIsland";

/** The island's base width (the ears are centred in it). */
const BASE_W = 420;
const EAR_W = 36;
const STATE_DOT = 9;
const DOT = 6;

/** The right ear's three colours, always in this order (the user 2026-11-10). */
const ALL: Dot[] = ["working", "waiting", "done"];

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
  const gap = 4;
  const row = ALL.length * DOT + (ALL.length - 1) * gap;
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
              left: `${right - row / 2 + i * (DOT + gap)}px`,
              top: `${h / 2 - DOT / 2}px`,
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
