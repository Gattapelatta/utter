import { useRef } from "react";
import { AnimatePresence } from "motion/react";
import { Island } from "./drop/Island";
import type { Notch, View } from "./drop/islandMotion";
import type { Level } from "./drop/Drop";
import type { AgentView, DraftParts, OverlayState } from "../lib/types";
import { agentShow } from "./agents/agentIsland";
import { AgentCard, AgentList } from "./agents/AgentCard";
import { AgentEars } from "./agents/AgentEars";

const NO_AGENTS: AgentView = { rows: [], card: null, paused: false };

interface Props {
  state: OverlayState;
  /** The ears are shown while idle (§4.2a). */
  rest: boolean;
  notch: Notch;
  level: Level;
  now: number;
  onClick?: () => void;
  /** The live draft (spec 2026-11-06): the island opens into a window while there is one. */
  draft?: DraftParts | null;
  /** The AI agents the Drop watches (spec 2026-11-09). */
  agents?: AgentView;
  /** The pointer is over the ears (Utter polls it while there are agents). */
  hover?: boolean;
}

/**
 * Everything the overlay window draws: the island while a recording or its result is shown,
 * the ears while idle, nothing otherwise (the island then folds into the notch and goes away).
 */
export function Stage({ state, rest, notch, level, now, onClick, draft = null, agents = NO_AGENTS, hover = false }: Props) {
  const visible = state.kind !== "hidden" && state.kind !== "cancelled";
  // The last visible state stays on screen while the island folds.
  const shown = useRef<OverlayState>(state);
  if (visible) shown.current = state;
  const cancelled = state.kind === "cancelled";
  const live = (state.kind === "recording" || state.kind === "processing") && !!draft && (draft.settled !== "" || draft.interim !== "");
  const show = agentShow(agents, state.kind, hover);
  const agentOpen = show.kind === "card" || show.kind === "list";
  const view: View = visible ? (live ? "window" : "open") : agentOpen ? "window" : "rest";
  const agent =
    show.kind === "card" ? (
      <AgentCard card={show.card} rows={agents.rows} notch={notch} />
    ) : show.kind === "list" ? (
      <AgentList rows={show.rows} notch={notch} />
    ) : null;
  const ears = show.kind === "ears" ? <AgentEars notch={notch} dot={show.dot} dots={show.dots} more={show.more} /> : null;

  return (
    <AnimatePresence custom={cancelled}>
      {(visible || rest || agentOpen) && (
        <div key="island" onClick={onClick}>
          <Island
            state={shown.current}
            level={level}
            now={now}
            view={view}
            notch={notch}
            cancelled={cancelled}
            draft={live ? draft : null}
            agent={visible ? null : agent}
            ears={ears}
          />
        </div>
      )}
    </AnimatePresence>
  );
}
