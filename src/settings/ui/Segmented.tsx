import { useId } from "react";
import { motion } from "motion/react";

export interface Option<T> {
  value: T;
  label: string;
}

/** A pill group; the selected pill slides to the new choice. */
export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
}: {
  options: Option<T>[];
  value: T;
  onChange: (v: T) => void;
  label?: string;
}) {
  const id = useId();
  return (
    <div role="radiogroup" aria-label={label} className="flex shrink-0 rounded-full bg-surface-raised p-0.5">
      {options.map((o) => {
        const on = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={on}
            onClick={() => !on && onChange(o.value)}
            className={`relative h-7 rounded-full px-3 text-[13px] whitespace-nowrap select-none ${on ? "text-bg" : "segment-off text-text-secondary"}`}
          >
            {on && (
              <motion.span
                layoutId={id}
                className="absolute inset-0 rounded-full bg-text"
                transition={{ type: "spring", bounce: 0, duration: 0.25 }}
              />
            )}
            <span className="relative">{o.label}</span>
          </button>
        );
      })}
    </div>
  );
}
