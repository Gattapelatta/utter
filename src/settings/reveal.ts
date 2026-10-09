/** The approved "expensive" motion (onboarding), tuned a little quicker for sections opened often. */
const EASE_OUT = "cubic-bezier(0.22, 1, 0.36, 1)";
const POP = "cubic-bezier(0.2, 0.9, 0.3, 1.15)";
const STAGGER_MS = 35;
const MAX_DELAY_MS = 420;

/** Blocks to cascade in reading order: direct children, but lists and wrappers are opened up. */
function blocks(root: Element, title: Element | null): Element[] {
  const out: Element[] = [];
  for (const child of root.children) {
    if (child.matches("[data-title]") || child.matches("[data-drop-slot]")) continue;
    if (title && child.contains(title)) {
      out.push(...blocks(child, title));
      continue;
    }
    const items = child.querySelectorAll(":scope > .row, :scope > li, :scope > [role=radio]");
    if (items.length > 1) out.push(...items);
    else out.push(child);
  }
  return out;
}

/**
 * A section's entrance (§5.4): the title rises from under its mask, then the line, rows and
 * blocks appear one after another from a slight blur. WAAPI with `fill: backwards`, so every
 * element is hidden until its turn even before the first paint.
 */
export function revealPage(root: Element) {
  if (typeof Element.prototype.animate !== "function") return;
  if (typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const title = root.querySelector("[data-title] h1");
  title?.animate([{ transform: "translateY(105%)" }, { transform: "translateY(0%)" }], {
    duration: 520,
    easing: EASE_OUT,
    fill: "backwards",
  });
  blocks(root, title).forEach((el, i) =>
    el.animate(
      [
        { opacity: 0, transform: "translateY(10px)", filter: "blur(6px)" },
        { opacity: 1, transform: "translateY(0px)", filter: "blur(0px)" },
      ],
      { duration: 380, delay: Math.min(MAX_DELAY_MS, 60 + i * STAGGER_MS), easing: POP, fill: "backwards" },
    ),
  );
}
