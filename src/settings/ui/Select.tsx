import { CaretDown } from "@phosphor-icons/react";
import type { Option } from "./Segmented";

export interface Group<T> {
  label: string;
  options: Option<T>[];
}

/** A native select (macOS draws its own menu) dressed as a pill. Groups show as labelled sections. */
export function Select<T extends string>({
  options,
  groups = [],
  value,
  onChange,
  label,
}: {
  options: Option<T>[];
  groups?: Group<T>[];
  value: T;
  onChange: (v: T) => void;
  label?: string;
}) {
  const opt = (o: Option<T>) => (
    <option key={o.value} value={o.value}>
      {o.label}
    </option>
  );
  return (
    <div className="relative shrink-0">
      <select
        aria-label={label}
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
        className="h-8 appearance-none rounded-full bg-surface-raised pr-8 pl-3.5 text-[13px] text-text outline-none"
      >
        {options.map(opt)}
        {groups.map((g) => (
          <optgroup key={g.label} label={g.label}>
            {g.options.map(opt)}
          </optgroup>
        ))}
      </select>
      <CaretDown aria-hidden size={12} className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-text-secondary" />
    </div>
  );
}
