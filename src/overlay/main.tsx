import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { Pill } from "./Pill";
import { pillVariants } from "./pillMotion";
import { events, ipc } from "../lib/ipc";
import { useApp } from "../lib/store";
import type { OverlayState } from "../lib/types";
import "./overlay.css";

const BARS = 24;

function Overlay() {
  const [state, setState] = useState<OverlayState>({ kind: "hidden" });
  const [levels, setLevels] = useState<number[]>(() => Array(BARS).fill(0));
  const [now, setNow] = useState(Date.now());
  const reduced = useReducedMotion() ?? false;
  // The last visible state stays on screen while the pill animates out.
  const shown = useRef<OverlayState>(state);
  const visible = state.kind !== "hidden" && state.kind !== "cancelled";
  if (visible) shown.current = state;

  useEffect(() => {
    const stopStore = useApp.getState().init();
    const un1 = events.overlayState((s) => {
      setState(s);
      if (s.kind === "recording" && !s.warn) setLevels(Array(BARS).fill(0));
    });
    // Speech RMS rarely exceeds ~0.3; scale so normal speech fills the bars.
    const un2 = events.overlayLevel((l) => setLevels((prev) => [...prev.slice(1), Math.min(1, Math.sqrt(l) * 2.2)]));
    const timer = setInterval(() => setNow(Date.now()), 250);
    return () => {
      stopStore.then((f) => f());
      un1.then((f) => f());
      un2.then((f) => f());
      clearInterval(timer);
    };
  }, []);

  const onClick = () => {
    if (state.kind === "error" && state.action) ipc.overlayAction(state.action);
  };

  return (
    <>
      <AnimatePresence custom={state.kind === "cancelled"}>
        {visible && (
          <motion.div
            key="pill"
            onClick={onClick}
            variants={pillVariants(reduced)}
            initial="hidden"
            animate="visible"
            exit="leave"
            style={{ transformOrigin: "bottom center" }}
          >
            <Pill state={shown.current} levels={levels} now={now} />
          </motion.div>
        )}
      </AnimatePresence>
      {import.meta.env.DEV && <DevStates onState={setState} onLevels={setLevels} />}
    </>
  );
}

/** Development-only buttons to walk through every pill state in a browser. */
function DevStates({ onState, onLevels }: { onState: (s: OverlayState) => void; onLevels: (l: number[]) => void }) {
  const [on, setOn] = useState(false);
  useEffect(() => {
    if (!on) return;
    const t = setInterval(() => onLevels(Array.from({ length: BARS }, () => Math.random() * 0.9)), 33);
    return () => clearInterval(t);
  }, [on, onLevels]);
  const now = Date.now();
  const states: [string, OverlayState][] = [
    ["rec", { kind: "recording", mode: "Dictation", style: "Business", started_at_ms: now, warn: false, pending: 0 }],
    ["rec EN+2", { kind: "recording", mode: "Translate", style: "Casual", started_at_ms: now - 571_000, warn: true, pending: 2 }],
    ["proc", { kind: "processing", pending: 1, since_ms: now }],
    ["done", { kind: "done", outcome: "pasted", degraded: false }],
    ["copied", { kind: "done", outcome: "copied", degraded: false }],
    ["err", { kind: "error", message_key: "rate_limited", action: null, minutes: 12, provider: "Gemini" }],
    ["no key", { kind: "error", message_key: "no_key", action: "open_provider", minutes: null, provider: null }],
    ["hide", { kind: "hidden" }],
    ["esc", { kind: "cancelled" }],
  ];
  return (
    <div style={{ position: "fixed", top: 4, left: 4, display: "flex", gap: 4, flexWrap: "wrap" }}>
      {states.map(([name, s]) => (
        <button key={name} onClick={() => onState(s)}>{name}</button>
      ))}
      <button onClick={() => setOn((v) => !v)}>levels</button>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
