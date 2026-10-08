export type Notch = { width: number; height: number } | null;

const ISLAND_W = 360;
const ISLAND_H = 104;
/** The island grows out of the notch and folds back into it: no overshoot (§5.4). */
const grow = { type: "spring" as const, bounce: 0, duration: 0.5 };

const transform = (y: number, sx: number, sy: number) =>
  `translateY(${y}px) scaleX(${+sx.toFixed(6)}) scaleY(${+sy.toFixed(6)})`;

/**
 * From the notch's own size (black on black, so it reads as the notch growing), or from
 * above the screen edge. Every variant uses the same transform functions: Motion cannot
 * interpolate between different lists and collapses the island to scale 0 instead.
 */
export function islandVariants(notch: Notch) {
  const folded = {
    opacity: 1,
    transform: notch ? transform(0, notch.width / ISLAND_W, notch.height / ISLAND_H) : transform(-ISLAND_H, 1, 1),
  };
  return {
    hidden: folded,
    visible: { opacity: 1, transform: transform(0, 1, 1), transition: grow, transitionEnd: { transform: "none" } },
    // Folding back is an even ease-in-out tween, so it ends exactly when the window is hidden.
    leave: (cancelled: boolean) => ({
      ...folded,
      transition: { type: "tween" as const, duration: cancelled ? 0.35 : 0.6, ease: [0.4, 0, 0.2, 1] as [number, number, number, number] },
    }),
  };
}
