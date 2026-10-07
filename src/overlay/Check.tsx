import { motion } from "motion/react";

/** Success check drawn with a 200 ms stroke (§5.4). */
export function Check({ reduced }: { reduced: boolean }) {
  return (
    <svg data-check data-accent width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden className="shrink-0 text-success">
      <motion.path
        d="M3.5 8.5l3 3 6-7"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
        initial={{ pathLength: reduced ? 1 : 0 }}
        animate={{ pathLength: 1 }}
        transition={{ duration: 0.2, ease: "easeOut" }}
      />
    </svg>
  );
}
