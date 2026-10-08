import { useCallback, useEffect, useState } from "react";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { events, ipc } from "../lib/ipc";
import { useApp } from "../lib/store";
import { parseRoute, type Route } from "./routes";
import { General } from "./sections/General";
import { Hotkeys } from "./sections/Hotkeys";
import { Section } from "./sections/Section";
import { Translate } from "./sections/Translate";
import { Sidebar } from "./Sidebar";

function Page({ route }: { route: Route }) {
  switch (route) {
    case "general":
      return <General />;
    case "hotkeys":
      return <Hotkeys />;
    case "translate":
      return <Translate />;
    default:
      return <Section route={route} />;
  }
}

export function App() {
  const [route, setRoute] = useState<Route>(() => parseRoute(location.hash));
  const loaded = useApp((s) => s.settings !== null);

  const go = useCallback((r: Route) => {
    setRoute(r);
    history.replaceState(null, "", `#/${r}`);
  }, []);

  useEffect(() => {
    let stop: (() => void) | undefined;
    let gone = false;
    useApp
      .getState()
      .init()
      .then((f) => {
        if (gone) return f();
        stop = f;
      });
    const un = events.navigate((r) => go(parseRoute(r)));
    return () => {
      gone = true;
      stop?.();
      un.then((f) => f());
    };
  }, [go]);

  // Show the window once the themed page is in the DOM: no white flash. (Not via
  // requestAnimationFrame — a hidden WKWebView never runs it, so the window waited 1.5 s.)
  useEffect(() => {
    if (loaded) ipc.settingsReady().catch(() => {});
  }, [loaded]);

  if (!loaded) return null;

  const page = (
    <AnimatePresence mode="wait" initial={false}>
      <motion.div
        key={route}
        initial={{ opacity: 0 }}
        animate={{ opacity: 1, transition: { duration: 0.15, ease: [0.23, 1, 0.32, 1] } }}
        exit={{ opacity: 0, transition: { duration: 0.08 } }}
      >
        <Page route={route} />
      </motion.div>
    </AnimatePresence>
  );

  return (
    <MotionConfig reducedMotion="user">
      {route === "onboarding" ? (
        <main className="relative h-full overflow-y-auto">
          <div data-tauri-drag-region className="absolute inset-x-0 top-0 h-[52px]" />
          {page}
        </main>
      ) : (
        <div className="flex h-full">
          <Sidebar route={route} onGo={go} />
          <main className="relative h-full flex-1 overflow-y-auto">
            <div data-tauri-drag-region className="absolute inset-x-0 top-0 h-[52px]" />
            {page}
          </main>
        </div>
      )}
    </MotionConfig>
  );
}
