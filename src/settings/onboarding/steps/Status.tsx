import { CheckCircle } from "@phosphor-icons/react";

/** "Done" line shown once a step's requirement is met. */
export function Done({ label }: { label: string }) {
  return (
    <span role="status" className="inline-flex items-center gap-2 text-[15px] text-text">
      <CheckCircle data-accent size={20} weight="fill" className="text-success" aria-hidden />
      {label}
    </span>
  );
}
