import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { Pill } from "./Pill";
import { events, ipc } from "../lib/ipc";
import type { OverlayState } from "../lib/types";

const BARS = 24;

function Overlay() {
  const [state, setState] = useState<OverlayState>({ kind: "hidden" });
  const [levels, setLevels] = useState<number[]>(() => Array(BARS).fill(0));
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    const un1 = events.overlayState((s) => {
      setState(s);
      if (s.kind === "recording" && !s.warn) setLevels(Array(BARS).fill(0));
    });
    // Speech RMS rarely exceeds ~0.3; scale so normal speech fills the bars.
    const un2 = events.overlayLevel((l) =>
      setLevels((prev) => [...prev.slice(1), Math.min(1, Math.sqrt(l) * 2.2)]),
    );
    const timer = setInterval(() => setNow(Date.now()), 250);
    return () => {
      un1.then((f) => f());
      un2.then((f) => f());
      clearInterval(timer);
    };
  }, []);

  const onClick = () => {
    if (state.kind === "error" && state.action) ipc.overlayAction(state.action);
  };

  return (
    <div onClick={onClick}>
      <Pill state={state} levels={levels} now={now} />
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
