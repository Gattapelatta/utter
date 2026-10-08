import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { Island } from "./drop/Island";
import { islandVariants, type Notch } from "./drop/islandMotion";
import { events, ipc } from "../lib/ipc";
import { useApp } from "../lib/store";
import type { OverlayState } from "../lib/types";
import "./overlay.css";

const fadeOnly = {
  hidden: { opacity: 0 },
  visible: { opacity: 1, transition: { duration: 0.15 } },
  leave: { opacity: 0, transition: { duration: 0.15 } },
};

function Overlay() {
  const [state, setState] = useState<OverlayState>({ kind: "hidden" });
  const [level, setLevel] = useState(0);
  const [notch, setNotch] = useState<Notch>(null);
  const [now, setNow] = useState(Date.now());
  const reduced = useReducedMotion() ?? false;
  // The last visible state stays on screen while the pill animates out.
  const shown = useRef<OverlayState>(state);
  const visible = state.kind !== "hidden" && state.kind !== "cancelled";
  if (visible) shown.current = state;

  useEffect(() => {
    const stopStore = useApp.getState().init();
    const un0 = events.overlayScreen((s) => setNotch(s.notch));
    const un1 = events.overlayState((s) => setState(s));
    // Speech RMS rarely exceeds ~0.3; scale so normal speech reaches ~1.
    const un2 = events.overlayLevel((l) => setLevel(Math.min(1, Math.sqrt(l) * 2.2)));
    const timer = setInterval(() => setNow(Date.now()), 250);
    return () => {
      stopStore.then((f) => f());
      un0.then((f) => f());
      un1.then((f) => f());
      un2.then((f) => f());
      clearInterval(timer);
    };
  }, []);

  // Budget §8.4: report the first painted frame of each new recording.
  const wasRecording = useRef(false);
  useEffect(() => {
    const recording = state.kind === "recording";
    if (recording && !wasRecording.current) {
      requestAnimationFrame(() => requestAnimationFrame(() => ipc.overlayPainted().catch(() => {})));
    }
    wasRecording.current = recording;
  }, [state]);

  const onClick = () => {
    if (state.kind === "error" && state.action) ipc.overlayAction(state.action);
  };

  return (
    <>
      <AnimatePresence custom={state.kind === "cancelled"}>
        {visible && (
          <motion.div
            key="island"
            onClick={onClick}
            variants={reduced ? fadeOnly : islandVariants(notch)}
            initial="hidden"
            animate="visible"
            exit="leave"
            style={{ transformOrigin: "top center" }}
          >
            <Island state={shown.current} level={level} now={now} />
          </motion.div>
        )}
      </AnimatePresence>
      {import.meta.env.DEV && <DevStates onState={setState} onLevel={setLevel} onNotch={setNotch} />}
    </>
  );
}

/** Development-only buttons to walk through every pill state in a browser. */
function DevStates({
  onState,
  onLevel,
  onNotch,
}: {
  onState: (s: OverlayState) => void;
  onLevel: (l: number) => void;
  onNotch: (n: Notch) => void;
}) {
  const [on, setOn] = useState(false);
  useEffect(() => {
    if (!on) return;
    const t = setInterval(() => onLevel(Math.random() < 0.1 ? 0.05 : 0.3 + Math.random() * 0.7), 120);
    return () => clearInterval(t);
  }, [on, onLevel]);
  const now = Date.now();
  const states: [string, OverlayState][] = [
    ["rec", { kind: "recording", mode: "Dictation", style: "Business", started_at_ms: now, warn: false, pending: 0 }],
    ["rec EN+2", { kind: "recording", mode: "Translate", style: "Casual", started_at_ms: now - 571_000, warn: true, pending: 2 }],
    ["proc", { kind: "processing", pending: 1, since_ms: now }],
    ["done", { kind: "done", outcome: "pasted", degraded: false }],
    ["copied", { kind: "done", outcome: "copied", degraded: false }],
    ["slow", { kind: "processing", pending: 1, since_ms: now - 5_000 }],
    ["err", { kind: "error", message_key: "rate_limited", action: null, minutes: 12, provider: "Gemini" }],
    ["no key", { kind: "error", message_key: "no_key", action: "open_provider", minutes: null, provider: null }],
    ["hide", { kind: "hidden" }],
    ["esc", { kind: "cancelled" }],
  ];
  return (
    <div style={{ position: "fixed", bottom: 4, left: 4, display: "flex", gap: 4, flexWrap: "wrap" }}>
      {states.map(([name, s]) => (
        <button key={name} onClick={() => onState(s)}>{name}</button>
      ))}
      <button onClick={() => setOn((v) => !v)}>levels</button>
      <button onClick={() => onNotch({ width: 179, height: 32 })}>notch</button>
      <button onClick={() => onNotch(null)}>no notch</button>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
