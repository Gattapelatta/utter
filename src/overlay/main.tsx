import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { Stage } from "./Stage";
import { useClock } from "./useClock";
import type { Notch } from "./drop/islandMotion";
import { events, ipc } from "../lib/ipc";
import { useApp } from "../lib/store";
import type { OverlayState } from "../lib/types";
import "./overlay.css";

function Overlay() {
  const [state, setState] = useState<OverlayState>({ kind: "hidden" });
  const [rest, setRest] = useState(false);
  const [level, setLevel] = useState(0);
  const [notch, setNotch] = useState<Notch>(null);
  const visible = state.kind !== "hidden" && state.kind !== "cancelled";
  const now = useClock(visible);

  useEffect(() => {
    const stopStore = useApp.getState().init();
    const un0 = events.overlayScreen((s) => setNotch(s.notch));
    const un1 = events.overlayState((s) => setState(s));
    // Speech RMS rarely exceeds ~0.3; scale so normal speech reaches ~1.
    const un2 = events.overlayLevel((l) => setLevel(Math.min(1, Math.sqrt(l) * 2.2)));
    const un3 = events.overlayRest((r) => setRest(r.shown));
    // Events sent before these listeners existed are lost: ask for the ears again.
    Promise.all([un0, un3]).then(() => ipc.overlayReady().catch(() => {}));
    // Diagnostics: whether WebKit treats the page as visible when the pill is shown.
    const vis = () => ipc.uiLog("info", `overlay page ${document.visibilityState}`).catch(() => {});
    document.addEventListener("visibilitychange", vis);
    return () => {
      stopStore.then((f) => f());
      un0.then((f) => f());
      un1.then((f) => f());
      un2.then((f) => f());
      un3.then((f) => f());
      document.removeEventListener("visibilitychange", vis);
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
      <Stage state={state} rest={rest} notch={notch} level={level} now={now} onClick={onClick} />
      {import.meta.env.DEV && <DevStates onState={setState} onLevel={setLevel} onNotch={setNotch} onRest={setRest} />}
    </>
  );
}

/** Development-only buttons to walk through every pill state in a browser. */
function DevStates({
  onState,
  onLevel,
  onNotch,
  onRest,
}: {
  onState: (s: OverlayState) => void;
  onLevel: (l: number) => void;
  onNotch: (n: Notch) => void;
  onRest: (f: (r: boolean) => boolean) => void;
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
      <button onClick={() => onRest((r) => !r)}>rest</button>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
