import { useEffect, useMemo, useRef, useState } from "react";
import { Drop } from "../../overlay/drop/Drop";
import { createFlight, settled, stepFlight, type Flight } from "../onboarding/flight";
import { useDropStage } from "./dropStage";

/** Drawn at its largest size (the About page) and scaled down, so it stays sharp. */
export const BASE = 140;
/** Highest priority first: a page's own place for the Drop beats the sidebar logo. */
const SLOTS = ["about", "onboarding", "logo"];

function prefersReducedMotion(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function findSlot(): HTMLElement | null {
  for (const kind of SLOTS) {
    const el = document.querySelector<HTMLElement>(`[data-drop-slot="${kind}"]`);
    if (el) return el;
  }
  return null;
}

/**
 * The settings window's one Drop (§4.3–4.4). Pages only mark a place for it
 * (`data-drop-slot`); when the place changes to another element the Drop flies there on
 * the onboarding spring (position and size), and when its own place moves (scroll,
 * resize) it follows at once.
 */
export function FloatingDrop() {
  const reduced = useMemo(prefersReducedMotion, []);
  const mood = useDropStage((s) => s.mood);
  const el = useRef<HTMLDivElement>(null);
  const [look, setLook] = useState<{ x: number; y: number } | null>(null);

  useEffect(() => {
    const axes: Record<"x" | "y" | "s", Flight> = { x: createFlight(0), y: createFlight(0), s: createFlight(0) };
    let current: HTMLElement | null = null;
    let flying = false;
    let raf = 0;
    let last = performance.now();
    const frame = (now: number) => {
      const dt = (now - last) / 1000;
      last = now;
      const slot = findSlot();
      if (slot) {
        const r = slot.getBoundingClientRect();
        const target = { x: r.left, y: r.top, s: r.width / BASE };
        if (slot !== current && current && !reduced) {
          // A new place: fly there, looking where it is going.
          flying = true;
          setLook({ x: Math.sign(target.x - axes.x.x) * 7, y: target.y < axes.y.x ? -5.5 : 3 });
        }
        if (!flying || reduced || !current) {
          // First appearance, or the same place moved: stay glued to it.
          for (const k of ["x", "y", "s"] as const) axes[k] = createFlight(target[k]);
          flying = false;
        } else {
          for (const k of ["x", "y", "s"] as const) {
            axes[k].target = target[k];
            stepFlight(axes[k], dt);
          }
          // Scale settles on a smaller number; compare it in pixels.
          const sPx = { ...axes.s, x: axes.s.x * BASE, v: axes.s.v * BASE, target: axes.s.target * BASE };
          if (settled(axes.x) && settled(axes.y) && settled(sPx)) {
            flying = false;
            setLook(null);
          }
        }
        current = slot;
        useDropStage.getState().setSettled(!flying);
        if (el.current) {
          el.current.style.opacity = "1";
          el.current.style.transform = `translate(${+axes.x.x.toFixed(2)}px, ${+axes.y.x.toFixed(2)}px) scale(${+axes.s.x.toFixed(4)})`;
        }
      } else if (el.current) {
        el.current.style.opacity = "0";
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, [reduced]);

  return (
    <div
      ref={el}
      data-floating-drop
      data-mood={mood}
      aria-hidden
      className="pointer-events-none fixed top-0 left-0 z-20 origin-top-left"
      style={{ opacity: 0, width: BASE }}
    >
      <Drop mood={mood} level={0} translate={false} reduced={reduced} size={BASE} follow look={look} />
    </div>
  );
}
