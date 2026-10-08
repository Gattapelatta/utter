import { useCallback, useEffect, useState } from "react";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { events, ipc } from "../lib/ipc";
import { useApp } from "../lib/store";
import { FloatingDrop } from "./drop/FloatingDrop";
import { useDropStage } from "./drop/dropStage";
import { Onboarding } from "./onboarding/Onboarding";
import { parseRoute, type Route } from "./routes";
import { About } from "./sections/About";
import { General } from "./sections/General";
import { History } from "./sections/History";
import { Hotkeys } from "./sections/Hotkeys";
import { Permissions } from "./sections/Permissions";
import { Provider } from "./sections/Provider";
import { Section } from "./sections/Section";
import { Translate } from "./sections/Translate";
import { Sidebar } from "./Sidebar";

/** After onboarding the Drop needs a moment to notice the logo before "settled" means "landed there". */
const LAND_GRACE_MS = 100;
const EASE_OUT = "cubic-bezier(0.22, 1, 0.36, 1)";
const POP = "cubic-bezier(0.2, 0.9, 0.3, 1.15)";

/** Once the Drop has landed on the logo: the menu slides in from the left, then the page cascades (§4.4). */
function revealApp() {
  document.querySelectorAll<HTMLElement>("[data-intro-item]").forEach((el, i) =>
    el.animate?.([{ opacity: 0, transform: "translateX(-8px)" }, { opacity: 1, transform: "none" }], {
      duration: 380,
      delay: i * 45,
      easing: EASE_OUT,
      fill: "backwards",
    }),
  );
  [...document.querySelectorAll<HTMLElement>("main h1, main h2, main p, main .row")].slice(0, 16).forEach((el, i) =>
    el.animate?.(
      [
        { opacity: 0, transform: "translateY(10px)", filter: "blur(6px)" },
        { opacity: 1, transform: "none", filter: "blur(0px)" },
      ],
      { duration: 420, delay: 180 + i * 40, easing: POP, fill: "backwards" },
    ),
  );
}

function Page({ route, go, finishOnboarding }: { route: Route; go: (r: Route) => void; finishOnboarding: () => void }) {
  switch (route) {
    case "onboarding":
      return <Onboarding onDone={finishOnboarding} />;
    case "permissions":
      return <Permissions onDone={() => go("general")} />;
    case "general":
      return <General />;
    case "hotkeys":
      return <Hotkeys />;
    case "translate":
      return <Translate />;
    case "provider":
      return <Provider />;
    case "history":
      return <History />;
    case "about":
      return <About />;
    default:
      return <Section route={route} />;
  }
}

export function App() {
  const [route, setRoute] = useState<Route>(() => parseRoute(location.hash));
  const loaded = useApp((s) => s.settings !== null);
  // Coming from onboarding: the menu and page stay hidden until the Drop lands on the logo.
  const [intro, setIntro] = useState(false);

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

  const finishOnboarding = useCallback(() => {
    setIntro(true);
    go("general");
  }, [go]);

  useEffect(() => {
    if (!intro) return;
    const since = performance.now();
    let raf = 0;
    const check = (now: number) => {
      if (now - since > LAND_GRACE_MS && useDropStage.getState().settled) {
        setIntro(false);
        revealApp();
      } else raf = requestAnimationFrame(check);
    };
    raf = requestAnimationFrame(check);
    return () => cancelAnimationFrame(raf);
  }, [intro]);

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
        className="h-full"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1, transition: { duration: 0.15, ease: [0.23, 1, 0.32, 1] } }}
        exit={{ opacity: 0, transition: { duration: 0.08 } }}
      >
        <Page route={route} go={go} finishOnboarding={finishOnboarding} />
      </motion.div>
    </AnimatePresence>
  );

  return (
    <MotionConfig reducedMotion="user">
      <FloatingDrop />
      {route === "onboarding" ? (
        <div className="relative h-full">
          <div data-tauri-drag-region className="absolute inset-x-0 top-0 z-10 h-[52px]" />
          <main className="h-full overflow-y-auto">{page}</main>
        </div>
      ) : (
        <div className="flex h-full" data-intro={intro ? "wait" : undefined}>
          <Sidebar route={route} onGo={go} />
          <div className="relative h-full min-w-0 flex-1">
            {/* Outside the scrolling area, so the window can be dragged by its top edge at any scroll position. */}
            <div data-tauri-drag-region className="absolute inset-x-0 top-0 z-10 h-[52px]" />
            <main className="h-full overflow-y-auto">{page}</main>
          </div>
        </div>
      )}
    </MotionConfig>
  );
}
