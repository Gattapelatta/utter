import type { Transition, Variants } from "motion/react";

/** Spec §5.4. Full `transform` strings keep the animation on the compositor. */
const APPEAR_EASE: [number, number, number, number] = [0.2, 0.9, 0.3, 1.15];
const EXIT_EASE: [number, number, number, number] = [0.23, 1, 0.32, 1];

export const widthSpring: Transition = { type: "spring", bounce: 0, duration: 0.35 };
export const barSpring: Transition = { type: "spring", bounce: 0.15, duration: 0.18 };
export const shake: Transition = { type: "spring", bounce: 0.4, duration: 0.4 };

export function pillVariants(reduced: boolean): Variants {
  const fade = { duration: 0.15 };
  const v: Variants = reduced
    ? {
        hidden: { opacity: 0 },
        visible: { opacity: 1, transition: fade },
        exit: { opacity: 0, transition: fade },
        cancel: { opacity: 0, transition: fade },
      }
    : {
        hidden: { opacity: 0, transform: "translateY(8px) scale(0.96)", filter: "blur(4px)" },
        visible: {
          opacity: 1,
          transform: "translateY(0px) scale(1)",
          filter: "blur(0px)",
          transition: { duration: 0.22, ease: APPEAR_EASE },
          // A leftover filter/transform keeps the pill on a soft-rasterised layer.
          transitionEnd: { filter: "none", transform: "none" },
        },
        exit: { opacity: 0, transform: "translateY(8px) scale(1)", transition: { duration: 0.16, ease: EXIT_EASE } },
        cancel: { opacity: 0, transition: { duration: 0.12 } },
      };
  // AnimatePresence passes `custom` = "was cancelled" to the leaving pill.
  v.leave = ((cancelled: boolean) => (cancelled ? v.cancel : v.exit)) as Variants[string];
  return v;
}
