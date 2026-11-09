import type { Notch } from "../drop/islandMotion";
import { DOT_COLOR, type Dot } from "./agentIsland";

/** The island's base width (the ears are centred in it). */
const BASE_W = 420;
const EAR_W = 36;
const STATE_DOT = 9;
const DOT = 6;

/**
 * The agents in the ears at rest (spec 2026-11-09 §3.2.1): a state dot on the Drop in the left
 * ear, one dot per agent in the right ear (up to four; more — three and «+»). Beside the notch
 * only: nothing is drawn under the camera. Without a notch there is no right ear: only the state dot.
 */
export function AgentEars({ notch, dot, dots, more }: { notch: Notch; dot: Dot; dots: Dot[]; more: boolean }) {
  const h = notch?.height ?? 32;
  const left = notch ? BASE_W / 2 - notch.width / 2 - EAR_W / 2 : BASE_W / 2;
  const right = notch ? BASE_W / 2 + notch.width / 2 + EAR_W / 2 : null;
  const cells: (Dot | "+")[] = more ? [...dots, "+"] : dots;
  // Two columns, up to two rows, centred in the right ear.
  const at = (i: number) => {
    const cols = Math.min(cells.length, 2);
    const rows = Math.ceil(cells.length / 2);
    const col = i % 2;
    const row = Math.floor(i / 2);
    return { x: (col - (cols - 1) / 2) * (DOT + 4), y: (row - (rows - 1) / 2) * (DOT + 4) };
  };
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
        cells.map((c, i) => {
          const p = at(i);
          return c === "+" ? (
            <span
              key={i}
              data-agent-more
              style={{ position: "absolute", left: `${right + p.x - 4}px`, top: `${h / 2 + p.y - 6}px`, width: 8, fontSize: 9, lineHeight: "12px", color: "#c9ccd1", textAlign: "center" }}
            >
              +
            </span>
          ) : (
            <span
              key={i}
              data-agent-ear-dot={c}
              style={{ position: "absolute", left: `${right + p.x - DOT / 2}px`, top: `${h / 2 + p.y - DOT / 2}px`, width: DOT, height: DOT, borderRadius: DOT / 2, background: DOT_COLOR[c] }}
            />
          );
        })}
    </div>
  );
}
