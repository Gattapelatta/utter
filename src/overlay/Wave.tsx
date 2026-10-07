import { motion } from "motion/react";
import { barSpring } from "./pillMotion";

/** 24 level bars; heights spring towards the input level (§5.4). */
export function Wave({ levels, reduced }: { levels: number[]; reduced: boolean }) {
  return (
    <div className="flex h-[22px] items-center gap-[2px]" aria-hidden>
      {levels.map((l, i) => {
        const scale = Math.max(0.12, Math.min(1, l));
        return reduced ? (
          <span key={i} data-bar className="h-[22px] w-[3px] rounded-full bg-[var(--accent)]" style={{ transform: `scaleY(${scale})` }} />
        ) : (
          <motion.span
            key={i}
            data-bar
            className="h-[22px] w-[3px] rounded-full bg-[var(--accent)]"
            initial={false}
            animate={{ transform: `scaleY(${scale})` }}
            transition={barSpring}
          />
        );
      })}
    </div>
  );
}
