import type { OverlayState } from "../lib/types";
import { errorText, overlayText } from "../lib/messages";
import "./pill.css";

interface Props {
  state: OverlayState;
  /** Recent input levels, oldest first, 0..1. */
  levels: number[];
  now: number;
}

/** After this long without an answer the network is the likely culprit. */
const SLOW_AFTER_MS = 4_000;

function formatTimer(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export function Pill({ state, levels, now }: Props) {
  if (state.kind === "hidden") return null;

  if (state.kind === "recording") {
    const translate = state.mode === "Translate";
    const label = translate ? overlayText.translate[state.style] : overlayText.dictation;
    return (
      <div className="pill" data-mode={translate ? "translate" : "dictation"}>
        <div className="bars" aria-hidden>
          {levels.map((l, i) => (
            <span key={i} data-bar style={{ transform: `scaleY(${Math.max(0.12, Math.min(1, l))})` }} />
          ))}
        </div>
        <span className="label">{label}</span>
        <span className="timer" {...(state.warn ? { "data-warn": "" } : {})}>
          {formatTimer(now - state.started_at_ms)}
        </span>
        {state.pending > 0 && <span className="pending">{overlayText.pending(state.pending)}</span>}
      </div>
    );
  }

  if (state.kind === "processing") {
    return (
      <div className="pill">
        <span className="shimmer" aria-hidden />
        <span className="label">
          {now - state.since_ms > SLOW_AFTER_MS ? overlayText.slow : overlayText.processing}
        </span>
      </div>
    );
  }

  if (state.kind === "done") {
    const text = state.degraded ? overlayText.degraded : overlayText.done[state.outcome];
    return (
      <div className="pill" data-tone={state.outcome === "failed" ? "danger" : "success"}>
        <span className="dot" aria-hidden />
        <span className="label">{text}</span>
      </div>
    );
  }

  return (
    <div className="pill" data-tone="danger" data-clickable={state.action ? "" : undefined}>
      <span className="dot" aria-hidden />
      <span className="label">{errorText(state.message_key, state.minutes)}</span>
    </div>
  );
}
