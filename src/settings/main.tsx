import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./settings.css";
import { ipc } from "../lib/ipc";

// Errors on the user's Mac are otherwise invisible: send them to the app log.
window.addEventListener("error", (e) => ipc.uiLog("error", `${e.message} at ${e.filename}:${e.lineno}`).catch(() => {}));
window.addEventListener("unhandledrejection", (e) => ipc.uiLog("error", `unhandled: ${String(e.reason)}`).catch(() => {}));

if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  (await import("./devMock")).installDevMock();
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
