import { useMemo } from "react";
import { Drop } from "../overlay/drop/Drop";

function prefersReducedMotion(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** The living logo (§4.3): the Drop looks around, and at the pointer while it is over the window. */
export function Logo() {
  const reduced = useMemo(prefersReducedMotion, []);
  return (
    <div data-logo className="flex items-center gap-3 px-6">
      <Drop mood="idle" level={0} translate={false} reduced={reduced} size={60} follow />
      <span className="text-[22px] leading-none font-medium tracking-[-0.015em]">Utter</span>
    </div>
  );
}
