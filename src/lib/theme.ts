import type { Theme } from "./types";

export type Resolved = "dark" | "light";

const DARK_QUERY = "(prefers-color-scheme: dark)";
const FADE_MS = 300;

export function resolveTheme(pref: Theme, systemDark: boolean): Resolved {
  if (pref === "System") return systemDark ? "dark" : "light";
  return pref === "Light" ? "light" : "dark";
}

export function systemIsDark(): boolean {
  return typeof matchMedia === "function" && matchMedia(DARK_QUERY).matches;
}

type ViewTransitionDoc = Document & { startViewTransition?: (cb: () => void) => unknown };

/** Sets `data-theme` on <html>; with `animate`, crossfades (250 ms) instead of flashing. */
export function applyTheme(t: Resolved, animate: boolean): void {
  const root = document.documentElement;
  if (root.dataset.theme === t) return;
  const set = () => {
    root.dataset.theme = t;
  };
  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (!animate || reduced) return set();
  const doc = document as ViewTransitionDoc;
  if (doc.startViewTransition) {
    doc.startViewTransition(set);
    return;
  }
  root.classList.add("theme-fade");
  set();
  setTimeout(() => root.classList.remove("theme-fade"), FADE_MS);
}

/** Calls `cb` whenever the OS switches between light and dark. Returns an unsubscribe. */
export function watchSystemDark(cb: (dark: boolean) => void): () => void {
  if (typeof matchMedia !== "function") return () => {};
  const mq = matchMedia(DARK_QUERY);
  const listener = (e: { matches: boolean }) => cb(e.matches);
  mq.addEventListener("change", listener);
  return () => mq.removeEventListener("change", listener);
}
