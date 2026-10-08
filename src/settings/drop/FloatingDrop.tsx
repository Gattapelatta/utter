import { useEffect, useMemo, useRef, useState } from "react";
import { Drop } from "../../overlay/drop/Drop";
import { APPLE, createFlight, GLIDE, settled, springEasing, stepFlight, type Flight } from "../onboarding/flight";
import { useDropStage } from "./dropStage";

/** Drawn at its largest size (the About page) and scaled down, so it stays sharp. */
export const BASE = 140;
/** Highest priority first: a page's own place for the Drop beats the sidebar logo. */
const SLOTS = ["about", "onboarding", "logo"];

function prefersReducedMotion(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

const transform = (p: { x: number; y: number; s: number }) =>
  `translate(${+p.x.toFixed(2)}px, ${+p.y.toFixed(2)}px) scale(${+p.s.toFixed(4)})`;

/** Where the Drop is on screen right now, even in the middle of a compositor animation. */
function onScreen(el: HTMLElement): { x: number; y: number; s: number } | null {
  const m = getComputedStyle(el).transform.match(/matrix\(([^)]+)\)/);
  if (!m) return null;
  const [a, , , , e, f] = m[1].split(",").map(Number);
  return { x: e, y: f, s: a };
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
 * (`data-drop-slot`); when the place changes to another element the Drop flies there
 * (position and size: onboarding's springy hop, or a calm glide across the window), and
 * when its own place moves (scroll, resize) it follows at once.
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
    // A flight handed to the compositor (WAAPI): position and size in step with the display.
    let anim: Animation | null = null;
    let raf = 0;
    let last = performance.now();
    const place = (p: { x: number; y: number; s: number }) => {
      for (const k of ["x", "y", "s"] as const) axes[k] = createFlight(p[k]);
    };
    const frame = (now: number) => {
      const dt = (now - last) / 1000;
      last = now;
      const node = el.current;
      const slot = findSlot();
      if (slot && node) {
        const r = slot.getBoundingClientRect();
        const target = { x: r.left, y: r.top, s: r.width / BASE };
        const spring = slot.dataset.dropSlot === "onboarding" ? APPLE : GLIDE;
        const moved = slot !== current && current !== null && !reduced;
        if (moved) {
          // A new place: fly there, looking where it is going.
          setLook({ x: Math.sign(target.x - axes.x.x) * 7, y: target.y < axes.y.x ? -5.5 : 3 });
          if (typeof node.animate === "function") {
            const from = (anim && onScreen(node)) || { x: axes.x.x, y: axes.y.x, s: axes.s.x };
            anim?.cancel();
            const { easing, duration } = springEasing(spring);
            node.style.transform = transform(target);
            const a = node.animate([{ transform: transform(from) }, { transform: transform(target) }], { duration, easing });
            anim = a;
            a.onfinish = () => {
              if (anim !== a) return;
              anim = null;
              flying = false;
              setLook(null);
            };
            place(target);
          }
          flying = true;
        }
        if (anim) {
          // The compositor is flying the Drop; leave it alone until it lands.
        } else if (!flying || reduced || !current) {
          // First appearance, or the same place moved (scroll, resize): stay glued to it.
          place(target);
          flying = false;
        } else {
          // No compositor animations (tests): the same spring, stepped here.
          for (const k of ["x", "y", "s"] as const) {
            axes[k].target = target[k];
            stepFlight(axes[k], dt, spring);
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
        node.style.opacity = "1";
        if (!anim) node.style.transform = transform({ x: axes.x.x, y: axes.y.x, s: axes.s.x });
      } else if (node) {
        node.style.opacity = "0";
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => {
      cancelAnimationFrame(raf);
      anim?.cancel();
    };
  }, [reduced]);

  return (
    <div
      ref={el}
      data-floating-drop
      data-mood={mood}
      aria-hidden
      className="pointer-events-none fixed top-0 left-0 z-20 origin-top-left"
      style={{ opacity: 0, width: BASE, willChange: "transform" }}
    >
      <Drop mood={mood} level={0} translate={false} reduced={reduced} size={BASE} follow look={look} />
    </div>
  );
}
