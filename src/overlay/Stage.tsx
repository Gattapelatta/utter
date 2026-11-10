import { useEffect, useRef, useState } from "react";
import { AnimatePresence } from "motion/react";
import { Island } from "./drop/Island";
import type { Notch, View } from "./drop/islandMotion";
import type { Level } from "./drop/Drop";
import type { AgentView, DraftParts, OverlayState } from "../lib/types";
import { agentShow, DIZZY_MS, dizzyNow, pokes } from "./agents/agentIsland";
import { useT } from "../i18n";
import { AgentCard, AgentList, AgentNote } from "./agents/AgentCard";
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
  const t = useT();
  // The easter egg (§3.6): 5 pokes in 2 s make the Drop dizzy for 3 s; never over a dictation.
  const pokeTimes = useRef<number[]>([]);
  const [dizzySince, setDizzySince] = useState<number | null>(null);
  useEffect(() => {
    if (dizzySince === null) return;
    const done = setTimeout(() => setDizzySince(null), DIZZY_MS);
    return () => clearTimeout(done);
  }, [dizzySince]);
  const dizzy = dizzyNow(dizzySince, Date.now(), state.kind);
  const onPoke = () => {
    if (state.kind !== "hidden" || dizzy) return;
    const at = Date.now();
    pokeTimes.current = [...pokeTimes.current.filter((p) => at - p < 2000), at];
    if (pokes(pokeTimes.current, at)) {
      pokeTimes.current = [];
      setDizzySince(at);
    }
  };
  const show = agentShow(agents, state.kind, hover);
  const agentOpen = dizzy || show.kind === "card" || show.kind === "list";
  const view: View = visible ? (live ? "window" : "open") : agentOpen ? "window" : "rest";
  const agent = dizzy ? (
    <AgentNote text={t("agents.dizzy")} notch={notch} />
  ) : show.kind === "card" ? (
      <AgentCard card={show.card} rows={agents.rows} notch={notch} />
    ) : show.kind === "list" ? (
      <AgentList rows={show.rows} notch={notch} />
    ) : null;
  const ears = show.kind === "ears" ? <AgentEars notch={notch} dot={show.dot} /> : null;

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
            onPoke={visible ? undefined : onPoke}
            mood={dizzy ? "dizzy" : undefined}
          />
        </div>
      )}
    </AnimatePresence>
  );
}
